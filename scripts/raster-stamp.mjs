// Rasterising check for page numbers and watermarks over the PDF corpus.
//
// Two properties, each rendered with pdf.js:
//
// An invisible watermark (opacity 0) must leave every page pixel-identical to
// the source. The watermark goes through the same wrapping, resource copying
// and graphics state closing as a visible one, so any damage to the page
// itself shows up here with nothing drawn on top to hide it.
//
// Page numbers at the bottom right must change pixels only in the bottom
// right corner as the page is displayed. That is where /CropBox, /Rotate and
// /UserUnit have to be honoured; a stamp placed in raw page coordinates lands
// on the wrong edge of a turned page, or outside a cropped one. They are
// compared against the invisible watermark rather than the source, since both
// went through the same load and save, so any difference is the number alone.
//
// Run from the repo root, with the corpus on disk:
//
// ```sh
// npm run test:raster:stamp [-- --limit 100]
// ```

import { getDocument } from 'pdfjs-dist/legacy/build/pdf.mjs';
import { createCanvas } from '@napi-rs/canvas';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import init, { add_page_numbers, add_watermark } from '../src/lib/pdf/wasm/plico_engine.js';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const corpusDir = process.env.PLICO_CORPUS || path.join(root, 'testing/pdfjs/test/pdfs');

const scale = 0.5;
const maxPagePixels = 20_000_000;
const maxChannelDiff = 32;
const maxChangedFraction = 0.001;

const margin = 24;
const size = 12;
// "Page 999 of 999" in Helvetica at 12 points is under 90 points wide.
const widestNumber = 90;
// Antialiasing reaches a pixel or two past the glyphs.
const slack = 2;

const wasmPath = path.join(root, 'src/lib/pdf/wasm/plico_engine_bg.wasm');
await init({ module_or_path: new Uint8Array(fs.readFileSync(wasmPath)) });

const pdfjsOptions = {
	useSystemFonts: false,
	verbosity: 0,
	cMapUrl: path.join(root, 'node_modules/pdfjs-dist/cmaps') + '/',
	cMapPacked: true,
	standardFontDataUrl: path.join(root, 'node_modules/pdfjs-dist/standard_fonts') + '/'
};

async function renderPages(source) {
	const data = new Uint8Array(source);
	const task = getDocument({ data, ...pdfjsOptions });
	const pdf = await task.promise;
	const pages = [];
	try {
		for (let number = 1; number <= pdf.numPages; number++) {
			const page = await pdf.getPage(number);
			const base = page.getViewport({ scale: 1 });
			const pageScale = Math.min(scale, Math.sqrt(maxPagePixels / (base.width * base.height)));
			const viewport = page.getViewport({ scale: pageScale });
			const canvas = createCanvas(Math.ceil(viewport.width), Math.ceil(viewport.height));
			const context = canvas.getContext('2d');
			context.fillStyle = '#ffffff';
			context.fillRect(0, 0, canvas.width, canvas.height);
			await page.render({ canvasContext: context, viewport, background: 'rgb(255,255,255)' })
				.promise;
			const image = context.getImageData(0, 0, canvas.width, canvas.height);
			image.scale = pageScale;
			pages.push(image);
			page.cleanup();
		}
	} finally {
		await task.destroy();
	}
	return pages;
}

function changedPixels(a, b) {
	const changed = [];
	for (let i = 0; i < a.data.length; i += 4) {
		const difference = Math.max(
			Math.abs(a.data[i] - b.data[i]),
			Math.abs(a.data[i + 1] - b.data[i + 1]),
			Math.abs(a.data[i + 2] - b.data[i + 2]),
			Math.abs(a.data[i + 3] - b.data[i + 3])
		);
		if (difference > maxChannelDiff) changed.push(i / 4);
	}
	return changed;
}

function sameSize(a, b) {
	return a.width === b.width && a.height === b.height;
}

const limitArg = process.argv.indexOf('--limit');
const limit = limitArg >= 0 ? Number(process.argv[limitArg + 1]) : Infinity;

const files = fs
	.readdirSync(corpusDir)
	.filter((name) => name.toLowerCase().endsWith('.pdf'))
	.sort()
	.slice(0, limit === Infinity ? undefined : limit);

const counts = { checked: 0, engineRefused: 0, unrenderableSource: 0, hiddenNumbers: 0 };
const failures = [];
const hidden = [];

for (const name of files) {
	const bytes = fs.readFileSync(path.join(corpusDir, name));
	const none = new Uint32Array();
	const empty = new Uint8Array();

	let invisible;
	let numbered;
	try {
		// prettier-ignore
		invisible = add_watermark(bytes, '', none, 'CONFIDENTIAL', 0, true, 48, 0x000000, empty, 0, 4, margin, 45, 0, false, false);
		// prettier-ignore
		numbered = add_page_numbers(bytes, '', none, 1, 'Page {n} of {total}', 8, margin, 0, false, size, 0x000000, 1);
	} catch {
		counts.engineRefused++;
		continue;
	}

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

	let invisiblePages;
	let numberedPages;
	try {
		invisiblePages = await renderPages(invisible);
		numberedPages = await renderPages(numbered);
	} catch (error) {
		failures.push(`${name}: stamped output would not render: ${error?.message ?? error}`);
		continue;
	}
	if (invisiblePages.length !== sourcePages.length || numberedPages.length !== sourcePages.length) {
		failures.push(`${name}: rendered page count changed`);
		continue;
	}

	for (let index = 0; index < sourcePages.length; index++) {
		const label = `${name} page ${index + 1}`;
		const source = sourcePages[index];

		const after = invisiblePages[index];
		if (!sameSize(after, source)) {
			failures.push(`${label}: invisible watermark changed the page size`);
			continue;
		}
		const damaged = changedPixels(after, source).length / (source.width * source.height);
		if (damaged > maxChangedFraction) {
			failures.push(
				`${label}: invisible watermark changed ${(damaged * 100).toFixed(3)}% of pixels`
			);
			continue;
		}

		const stamped = numberedPages[index];
		if (!sameSize(stamped, source)) {
			failures.push(`${label}: page numbers changed the page size`);
			continue;
		}
		const changed = changedPixels(stamped, after);
		if (changed.length === 0) {
			// Black text over a black corner, or a page too small to show it.
			hidden.push(label);
			counts.hiddenNumbers++;
			continue;
		}
		const s = source.scale;
		const left = source.width - (margin + widestNumber) * s - slack;
		const right = source.width - margin * s + slack;
		const top = source.height - (margin + size) * s - slack;
		const bottom = source.height - (margin - size * 0.25) * s + slack;
		const outside = changed.filter((pixel) => {
			const x = pixel % source.width;
			const y = Math.floor(pixel / source.width);
			return x < left || x > right || y < top || y > bottom;
		});
		if (outside.length > 0) {
			const pixel = outside[0];
			failures.push(
				`${label}: ${outside.length} pixels changed outside the bottom right corner, first at ${pixel % source.width},${Math.floor(pixel / source.width)} of ${source.width}x${source.height}`
			);
		}
	}
	counts.checked++;
}

console.log(`\nstamp raster corpus: ${files.length} files, scale ${scale}`);
console.log(`  stamped, rendered, and compared: ${counts.checked}`);
console.log(`  engine refused: ${counts.engineRefused}`);
console.log(`  source would not render: ${counts.unrenderableSource}`);
console.log(`  pages where the number changed no pixels: ${counts.hiddenNumbers}`);
for (const label of hidden.slice(0, 5)) console.log(`    ${label}`);
console.log(`  failures: ${failures.length}`);
for (const failure of failures.slice(0, 25)) console.log(`  ${failure}`);
if (failures.length > 25) console.log(`  ... and ${failures.length - 25} more`);

if (failures.length > 0) {
	console.error('\nstamp raster compare failed');
	process.exit(1);
}
