// Rasterising check for cropping over the PDF corpus.
//
// Every page is cropped to an off-centre area, and the cropped page, rendered
// with pdf.js, must be the matching part of the source page pixel for pixel.
// The area differs on each side, so a crop mapped through the wrong /Rotate
// keeps the wrong part of the page and fails; /CropBox and /UserUnit have to
// be honoured for the same reason.
//
// The edges are snapped to whole pixels at the render scale, so the cropped
// render is the source render moved by a whole number of pixels rather than
// resampled.
//
// Run from the repo root, with the corpus on disk:
//
// ```sh
// npm run test:raster:crop [-- --limit 100]
// ```

import { getDocument } from 'pdfjs-dist/legacy/build/pdf.mjs';
import { createCanvas } from '@napi-rs/canvas';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import init, { crop_pdf } from '../src/lib/pdf/wasm/plico_engine.js';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const corpusDir = process.env.PLICO_CORPUS || path.join(root, 'testing/pdfjs/test/pdfs');

const scale = 0.5;
const maxPagePixels = 20_000_000;
const maxChannelDiff = 32;
const maxChangedFraction = 0.001;
// Left, top, right, bottom.
const area = [0.1, 0.05, 0.85, 0.8];

const wasmPath = path.join(root, 'src/lib/pdf/wasm/plico_engine_bg.wasm');
await init({ module_or_path: new Uint8Array(fs.readFileSync(wasmPath)) });

const pdfjsOptions = {
	useSystemFonts: false,
	verbosity: 0,
	cMapUrl: path.join(root, 'node_modules/pdfjs-dist/cmaps') + '/',
	cMapPacked: true,
	standardFontDataUrl: path.join(root, 'node_modules/pdfjs-dist/standard_fonts') + '/'
};

// Renders at `scales[index]` when given, so the cropped pages use the scale
// their source pages were snapped to.
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

// Pixels of `cropped` that differ from `source` read from `offsetX, offsetY`,
// skipping the outermost pixel, which the page edge antialiases.
function changedFraction(source, cropped, offsetX, offsetY) {
	let changed = 0;
	let total = 0;
	for (let y = 1; y < cropped.height - 1; y++) {
		for (let x = 1; x < cropped.width - 1; x++) {
			const i = (y * cropped.width + x) * 4;
			const j = ((y + offsetY) * source.width + x + offsetX) * 4;
			const difference = Math.max(
				Math.abs(cropped.data[i] - source.data[j]),
				Math.abs(cropped.data[i + 1] - source.data[j + 1]),
				Math.abs(cropped.data[i + 2] - source.data[j + 2])
			);
			if (difference > maxChannelDiff) changed++;
			total++;
		}
	}
	return total ? changed / total : 0;
}

const limitArg = process.argv.indexOf('--limit');
const limit = limitArg >= 0 ? Number(process.argv[limitArg + 1]) : Infinity;

const files = fs
	.readdirSync(corpusDir)
	.filter((name) => name.toLowerCase().endsWith('.pdf'))
	.sort()
	.slice(0, limit === Infinity ? undefined : limit);

const counts = { checked: 0, engineRefused: 0, unrenderableSource: 0, tooSmall: 0 };
const failures = [];

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
	if (pixelAreas.some(([left, top, right, bottom]) => right - left < 4 || bottom - top < 4)) {
		counts.tooSmall++;
		continue;
	}
	const fractions = pixelAreas.flatMap(([left, top, right, bottom], index) => [
		left / sourcePages[index].exactWidth,
		top / sourcePages[index].exactHeight,
		right / sourcePages[index].exactWidth,
		bottom / sourcePages[index].exactHeight
	]);

	let cropped;
	try {
		cropped = crop_pdf(
			bytes,
			'',
			Uint32Array.from(sourcePages, (_, index) => index + 1),
			Float32Array.from(fractions)
		);
	} catch {
		counts.engineRefused++;
		continue;
	}

	let croppedPages;
	try {
		croppedPages = await renderPages(
			cropped,
			sourcePages.map((page) => page.scale)
		);
	} catch (error) {
		failures.push(`${name}: cropped output would not render: ${error?.message ?? error}`);
		continue;
	}
	if (croppedPages.length !== sourcePages.length) {
		failures.push(`${name}: rendered page count changed`);
		continue;
	}

	for (let index = 0; index < sourcePages.length; index++) {
		const label = `${name} page ${index + 1}`;
		const source = sourcePages[index];
		const page = croppedPages[index];
		const [left, top, right, bottom] = pixelAreas[index];
		if (
			Math.abs(page.exactWidth - (right - left)) > 0.5 ||
			Math.abs(page.exactHeight - (bottom - top)) > 0.5
		) {
			failures.push(
				`${label}: cropped to ${page.exactWidth.toFixed(1)}x${page.exactHeight.toFixed(1)}, expected ${right - left}x${bottom - top}`
			);
			continue;
		}
		const changed = changedFraction(source, page, left, top);
		if (changed > maxChangedFraction) {
			failures.push(`${label}: ${(changed * 100).toFixed(3)}% of pixels differ from the source`);
		}
	}
	counts.checked++;
}

console.log(`\ncrop raster corpus: ${files.length} files, scale ${scale}`);
console.log(`  cropped, rendered, and compared: ${counts.checked}`);
console.log(`  engine refused: ${counts.engineRefused}`);
console.log(`  source would not render: ${counts.unrenderableSource}`);
console.log(`  a page too small to crop: ${counts.tooSmall}`);
console.log(`  failures: ${failures.length}`);
for (const failure of failures.slice(0, 25)) console.log(`  ${failure}`);
if (failures.length > 25) console.log(`  ... and ${failures.length - 25} more`);

if (failures.length > 0) {
	console.error('\ncrop raster compare failed');
	process.exit(1);
}
