// Rasterising check for flattening over the PDF corpus.
//
// pdf.js draws annotation appearances itself, so a page rendered as is and
// the same page after flattening, where those appearances are part of the
// page, must match pixel for pixel. A misplaced, mis-scaled or wrongly turned
// appearance shows up as a difference; so does one drawn that a viewer hides,
// or one dropped that it shows.
//
// Run from the repo root, with the corpus on disk:
//
// ```sh
// npm run test:raster:flatten [-- --limit 100]
// ```

import { getDocument } from 'pdfjs-dist/legacy/build/pdf.mjs';
import { createCanvas } from '@napi-rs/canvas';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import init, { flatten_pdf } from '../src/lib/pdf/wasm/plico_engine.js';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const corpusDir = process.env.PLICO_CORPUS || path.join(root, 'testing/pdfjs/test/pdfs');

const scale = 0.5;
const maxPagePixels = 20_000_000;
const maxChannelDiff = 32;
const maxChangedFraction = 0.001;

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
	// Annotations other than links and popups, which flattening leaves alone.
	pages.annotations = 0;
	try {
		for (let number = 1; number <= pdf.numPages; number++) {
			const page = await pdf.getPage(number);
			pages.annotations += (await page.getAnnotations()).filter(
				(annotation) => annotation.subtype !== 'Link' && annotation.subtype !== 'Popup'
			).length;
			const base = page.getViewport({ scale: 1 });
			const pageScale = Math.min(scale, Math.sqrt(maxPagePixels / (base.width * base.height)));
			const viewport = page.getViewport({ scale: pageScale });
			const canvas = createCanvas(Math.ceil(viewport.width), Math.ceil(viewport.height));
			const context = canvas.getContext('2d');
			context.fillStyle = '#ffffff';
			context.fillRect(0, 0, canvas.width, canvas.height);
			// The default annotation mode draws every appearance onto the canvas.
			await page.render({ canvasContext: context, viewport, background: 'rgb(255,255,255)' })
				.promise;
			pages.push(context.getImageData(0, 0, canvas.width, canvas.height));
			page.cleanup();
		}
	} finally {
		await task.destroy();
	}
	return pages;
}

function changedFraction(a, b) {
	let changed = 0;
	for (let i = 0; i < a.data.length; i += 4) {
		const difference = Math.max(
			Math.abs(a.data[i] - b.data[i]),
			Math.abs(a.data[i + 1] - b.data[i + 1]),
			Math.abs(a.data[i + 2] - b.data[i + 2])
		);
		if (difference > maxChannelDiff) changed++;
	}
	return changed / (a.width * a.height);
}

const limitArg = process.argv.indexOf('--limit');
const limit = limitArg >= 0 ? Number(process.argv[limitArg + 1]) : Infinity;

const files = fs
	.readdirSync(corpusDir)
	.filter((name) => name.toLowerCase().endsWith('.pdf'))
	.sort()
	.slice(0, limit === Infinity ? undefined : limit);

const counts = { checked: 0, flattened: 0, kept: 0, engineRefused: 0, unrenderableSource: 0 };
const failures = [];

for (const name of files) {
	const bytes = fs.readFileSync(path.join(corpusDir, name));

	let output;
	let kept;
	try {
		[output, kept] = flatten_pdf(bytes, '', false);
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

	let flatPages;
	try {
		flatPages = await renderPages(output);
	} catch (error) {
		failures.push(`${name}: flattened output would not render: ${error?.message ?? error}`);
		continue;
	}
	if (flatPages.length !== sourcePages.length) {
		failures.push(`${name}: rendered page count changed`);
		continue;
	}
	for (let index = 0; index < sourcePages.length; index++) {
		const source = sourcePages[index];
		const flat = flatPages[index];
		if (source.width !== flat.width || source.height !== flat.height) {
			failures.push(`${name} page ${index + 1}: flattening changed the page size`);
			continue;
		}
		const changed = changedFraction(source, flat);
		if (changed > maxChangedFraction) {
			failures.push(
				`${name} page ${index + 1}: ${(changed * 100).toFixed(3)}% of pixels differ from the source`
			);
		}
	}
	if (sourcePages.annotations > 0) counts.flattened++;
	counts.kept += kept;
	counts.checked++;
}

console.log(`\nflatten raster corpus: ${files.length} files, scale ${scale}`);
console.log(`  flattened, rendered, and compared: ${counts.checked}`);
console.log(`  of which had annotations to flatten: ${counts.flattened}`);
console.log(`  annotations left as they were: ${counts.kept}`);
console.log(`  engine refused: ${counts.engineRefused}`);
console.log(`  source would not render: ${counts.unrenderableSource}`);
console.log(`  failures: ${failures.length}`);
for (const failure of failures.slice(0, 40)) console.log(`  ${failure}`);
if (failures.length > 40) console.log(`  ... and ${failures.length - 40} more`);

if (failures.length > 0) {
	console.error('\nflatten raster compare failed');
	process.exit(1);
}
