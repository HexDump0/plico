// Redaction check over the PDF corpus: is what lies under the box gone, and
// is the rest of the page as it was?
//
// Every page gets a box over its middle. The redacted file is then read by
// poppler, which shares no code with the engine: any word it still finds
// mostly under the box is a leak. pdf.js renders source and result, and the
// box must be solid fill while the page outside it, beyond a margin for
// glyphs that straddle its edge, renders as before.
//
// Pages the engine cannot redact in place are drawn from a picture, as the
// app does: rendered with pdf.js, the box painted in, and handed back.
//
// The box is symmetric about the page's centre on both axes, so it covers the
// same region however the page is turned, which matters because poppler
// reports words in the page's unrotated space.
//
// Run from the repo root, with the corpus on disk and poppler's pdftotext on
// PATH:
//
// ```sh
// npm run test:raster:redact [-- --limit 100]
// ```

import { getDocument } from 'pdfjs-dist/legacy/build/pdf.mjs';
import { createCanvas } from '@napi-rs/canvas';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import init, { redact_pdf } from '../src/lib/pdf/wasm/plico_engine.js';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const corpusDir = process.env.PLICO_CORPUS || path.join(root, 'testing/pdfjs/test/pdfs');

const scale = 0.5;
const maxPagePixels = 20_000_000;
const maxChannelDiff = 32;
// Outside the box and its margin, rendering may differ this much: removed
// annotations under the box can reach beyond it.
const maxChangedFraction = 0.005;
// Glyphs a quarter under the box go whole, so the page may change this far
// past its edge, in points.
const margin = 24;
// A word poppler finds with this much of it under the box is a leak.
const leakShare = 0.5;
const area = [0.3, 0.3, 0.7, 0.7];

const wasmPath = path.join(root, 'src/lib/pdf/wasm/plico_engine_bg.wasm');
await init({ module_or_path: new Uint8Array(fs.readFileSync(wasmPath)) });

const pdfjsOptions = {
	useSystemFonts: false,
	verbosity: 0,
	cMapUrl: path.join(root, 'node_modules/pdfjs-dist/cmaps') + '/',
	cMapPacked: true,
	standardFontDataUrl: path.join(root, 'node_modules/pdfjs-dist/standard_fonts') + '/'
};

async function renderPages(source, scales) {
	const data = new Uint8Array(source);
	const task = getDocument({ data, ...pdfjsOptions });
	const pdf = await task.promise;
	const pages = [];
	try {
		for (let number = 1; number <= pdf.numPages; number++) {
			const page = await pdf.getPage(number);
			const base = page.getViewport({ scale: 1 });
			const pageScale =
				scales?.[number - 1] ??
				Math.min(scale, Math.sqrt(maxPagePixels / (base.width * base.height)));
			const viewport = page.getViewport({ scale: pageScale });
			const canvas = createCanvas(Math.ceil(viewport.width), Math.ceil(viewport.height));
			const context = canvas.getContext('2d');
			context.fillStyle = '#ffffff';
			context.fillRect(0, 0, canvas.width, canvas.height);
			await page.render({ canvasContext: context, viewport, background: 'rgb(255,255,255)' })
				.promise;
			const image = context.getImageData(0, 0, canvas.width, canvas.height);
			image.scale = pageScale;
			image.exactWidth = viewport.width;
			image.exactHeight = viewport.height;
			pages.push(image);
			page.cleanup();
		}
	} finally {
		await task.destroy();
	}
	return pages;
}

/// Pictures of the given pages at 150 DPI with the box painted, as the
/// worker makes them.
async function pictures(source, numbers) {
	const task = getDocument({ data: new Uint8Array(source), ...pdfjsOptions });
	const pdf = await task.promise;
	try {
		const made = [];
		for (const number of numbers) {
			const page = await pdf.getPage(number);
			const viewport = page.getViewport({ scale: 150 / 72 });
			const canvas = createCanvas(Math.ceil(viewport.width), Math.ceil(viewport.height));
			const context = canvas.getContext('2d');
			await page.render({ canvasContext: context, viewport, background: 'rgb(255,255,255)' })
				.promise;
			const [left, top, right, bottom] = area;
			const x = Math.floor(left * canvas.width);
			const y = Math.floor(top * canvas.height);
			context.fillStyle = '#000000';
			context.fillRect(
				x,
				y,
				Math.ceil(right * canvas.width) - x,
				Math.ceil(bottom * canvas.height) - y
			);
			made.push(new Uint8Array(canvas.toBuffer('image/png')));
			page.cleanup();
		}
		return made;
	} finally {
		await task.destroy();
	}
}

function words(bytes) {
	const file = path.join(os.tmpdir(), `plico-redact-${process.pid}.pdf`);
	fs.writeFileSync(file, bytes);
	try {
		const html = execFileSync('pdftotext', ['-bbox', '-cropbox', file, '-'], {
			encoding: 'utf8',
			stdio: ['ignore', 'pipe', 'ignore'],
			maxBuffer: 256 * 1024 * 1024,
			// bomb_giant.pdf keeps poppler busy for good.
			timeout: 60_000
		});
		const pages = [];
		for (const chunk of html.split('<page ').slice(1)) {
			const [, width, height] = chunk.match(/width="([\d.]+)" height="([\d.]+)"/) ?? [];
			const found = [
				...chunk.matchAll(
					/<word xMin="([\d.-]+)" yMin="([\d.-]+)" xMax="([\d.-]+)" yMax="([\d.-]+)">([^<]*)<\/word>/g
				)
			].map(([, x0, y0, x1, y1, text]) => ({
				box: [x0 / width, y0 / height, x1 / width, y1 / height],
				text
			}));
			pages.push(found);
		}
		return pages;
	} finally {
		fs.rmSync(file, { force: true });
	}
}

function share([left, top, right, bottom], [l, t, r, b]) {
	const size = (right - left) * (bottom - top);
	const width = Math.min(right, r) - Math.max(left, l);
	const height = Math.min(bottom, b) - Math.max(top, t);
	if (size <= 0) return left >= l && right <= r && top >= t && bottom <= b ? 1 : 0;
	return width > 0 && height > 0 ? (width * height) / size : 0;
}

const limitArg = process.argv.indexOf('--limit');
const limit = limitArg >= 0 ? Number(process.argv[limitArg + 1]) : Infinity;

const files = fs
	.readdirSync(corpusDir)
	.filter((name) => name.toLowerCase().endsWith('.pdf'))
	.sort()
	.slice(0, limit === Infinity ? undefined : limit);

const counts = {
	checked: 0,
	pages: 0,
	pictured: 0,
	engineRefused: 0,
	unrenderableSource: 0,
	unreadableByPoppler: 0,
	wordsRemoved: 0
};
const leaks = [];
const damage = [];

for (const name of files) {
	const bytes = fs.readFileSync(path.join(corpusDir, name));

	let sourcePages;
	try {
		sourcePages = await renderPages(bytes);
	} catch {
		counts.unrenderableSource++;
		continue;
	}
	if (sourcePages.length === 0) {
		counts.unrenderableSource++;
		continue;
	}

	// Edges on whole pixels of each page's render.
	const pixelAreas = sourcePages.map((page) => [
		Math.round(area[0] * page.exactWidth),
		Math.round(area[1] * page.exactHeight),
		Math.round(area[2] * page.exactWidth),
		Math.round(area[3] * page.exactHeight)
	]);
	const fractions = pixelAreas.flatMap(([left, top, right, bottom], index) => [
		left / sourcePages[index].exactWidth,
		top / sourcePages[index].exactHeight,
		right / sourcePages[index].exactWidth,
		bottom / sourcePages[index].exactHeight
	]);
	const pages = Uint32Array.from(sourcePages, (_, index) => index + 1);

	let redacted;
	let pictured = [];
	try {
		let [output, listed] = redact_pdf(
			bytes,
			'',
			pages,
			Float32Array.from(fractions),
			0,
			false,
			new Uint32Array(),
			[]
		);
		if (!output) {
			const needed = Array.from(listed);
			pictured = needed;
			counts.pictured += needed.length;
			[output] = redact_pdf(
				bytes,
				'',
				pages,
				Float32Array.from(fractions),
				0,
				false,
				Uint32Array.from(needed),
				await pictures(bytes, needed)
			);
		}
		redacted = output;
	} catch {
		counts.engineRefused++;
		continue;
	}

	// Poppler: no word mostly under the box.
	let before;
	try {
		before = words(bytes);
	} catch {
		counts.unreadableByPoppler++;
		continue;
	}
	let after;
	try {
		after = words(redacted);
	} catch (error) {
		leaks.push(`${name}: poppler could not read the result: ${error?.message ?? error}`);
		continue;
	}
	for (let index = 0; index < after.length; index++) {
		const removed = (before[index] ?? []).filter((word) => share(word.box, area) >= leakShare);
		counts.wordsRemoved += removed.length;
		for (const word of after[index]) {
			if (share(word.box, area) >= leakShare) {
				leaks.push(`${name} page ${index + 1}: "${word.text}" is still under the box`);
				break;
			}
		}
	}

	let redactedPages;
	try {
		redactedPages = await renderPages(
			redacted,
			sourcePages.map((page) => page.scale)
		);
	} catch (error) {
		leaks.push(`${name}: redacted output would not render: ${error?.message ?? error}`);
		continue;
	}
	if (redactedPages.length !== sourcePages.length) {
		leaks.push(`${name}: rendered page count changed`);
		continue;
	}

	for (let index = 0; index < sourcePages.length; index++) {
		const label = `${name} page ${index + 1}`;
		const source = sourcePages[index];
		const page = redactedPages[index];
		const [left, top, right, bottom] = pixelAreas[index];
		const reach = Math.ceil(margin * source.scale);
		let unfilled = 0;
		let changed = 0;
		let outside = 0;
		for (let y = 0; y < page.height; y++) {
			for (let x = 0; x < page.width; x++) {
				const i = (y * page.width + x) * 4;
				if (x > left && x < right - 1 && y > top && y < bottom - 1) {
					if (
						page.data[i] > maxChannelDiff ||
						page.data[i + 1] > maxChannelDiff ||
						page.data[i + 2] > maxChannelDiff
					)
						unfilled++;
					continue;
				}
				if (x >= left - reach && x < right + reach && y >= top - reach && y < bottom + reach)
					continue;
				outside++;
				const difference = Math.max(
					Math.abs(page.data[i] - source.data[i]),
					Math.abs(page.data[i + 1] - source.data[i + 1]),
					Math.abs(page.data[i + 2] - source.data[i + 2])
				);
				if (difference > maxChannelDiff) changed++;
			}
		}
		if (unfilled > 0) leaks.push(`${label}: ${unfilled} pixels inside the box are not filled`);
		// A page drawn from a picture differs everywhere by resampling.
		if (outside && changed / outside > maxChangedFraction && !pictured.includes(index + 1)) {
			damage.push(
				`${label}: ${((changed / outside) * 100).toFixed(3)}% of pixels outside the box changed`
			);
		}
		counts.pages++;
	}
	counts.checked++;
}

console.log(`\nredact raster corpus: ${files.length} files, scale ${scale}`);
console.log(`  redacted and checked: ${counts.checked} files, ${counts.pages} pages`);
console.log(`  pages drawn from a picture: ${counts.pictured}`);
console.log(`  words under the box in the sources, gone from the results: ${counts.wordsRemoved}`);
console.log(`  engine refused: ${counts.engineRefused}`);
console.log(`  source would not render: ${counts.unrenderableSource}`);
console.log(`  poppler could not read the source: ${counts.unreadableByPoppler}`);
console.log(`  leaks: ${leaks.length}`);
const shown = Number(process.env.SHOW ?? 25);
for (const leak of leaks.slice(0, shown)) console.log(`  ${leak}`);
if (leaks.length > shown) console.log(`  ... and ${leaks.length - shown} more`);
console.log(`  pages changed outside the box: ${damage.length}`);
for (const item of damage.slice(0, shown)) console.log(`  ${item}`);
if (damage.length > shown) console.log(`  ... and ${damage.length - shown} more`);

if (leaks.length > 0 || damage.length > 0) {
	console.error('\nredact raster check failed');
	process.exit(1);
}
