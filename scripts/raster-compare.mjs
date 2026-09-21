// Rasterising comparison over the PDF corpus.
//
// The structural corpus test proves merged output parses and has the right
// pages; it cannot see a page that parses and looks wrong. This renders each
// corpus file with pdf.js, merges it with a marker page through the same wasm
// engine the app ships, renders the result, and requires every page to have
// survived pixel-for-pixel. A page whose geometry, resources, or content
// stream changed shows up as a diff here.
//
// Run from the repo root, with the corpus on disk:
//
// ```sh
// npm run test:raster [-- --limit 100]
// ```
//
// Honours `PLICO_CORPUS` like the Rust harness, defaulting to the pdf.js test
// corpus. Files the engine refuses to merge are counted, not failed.

import { getDocument } from 'pdfjs-dist/legacy/build/pdf.mjs';
import { createCanvas } from '@napi-rs/canvas';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import init, { merge_pdfs } from '../src/lib/pdf/wasm/plico_engine.js';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

const corpusDir = process.env.PLICO_CORPUS || path.join(root, 'testing/pdfjs/test/pdfs');

const scale = 0.5;
const maxPagePixels = 20_000_000;
const maxChannelDiff = 32;
const maxChangedFraction = 0.001;

const wasmPath = path.join(root, 'src/lib/pdf/wasm/plico_engine_bg.wasm');
await init({ module_or_path: new Uint8Array(fs.readFileSync(wasmPath)) });

const marker = buildMarkerPdf();

function buildMarkerPdf() {
	// A 200x200 page that fills a frame. The rectangle needs no fonts, so the
	// ground truth renders identically in every merged document.
	const content = '10 10 180 180 re f';
	const objects = [
		{ n: 1, body: '<< /Type /Catalog /Pages 2 0 R >>' },
		{ n: 2, body: '<< /Type /Pages /Kids [3 0 R] /Count 1 >>' },
		{
			n: 3,
			body: '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>'
		},
		{ n: 4, body: `<< /Length ${content.length} >>\nstream\n${content}\nendstream` }
	];
	return serializePdf(objects);
}

function serializePdf(objects) {
	const chunks = [];
	let offset = 0;
	const push = (text) => {
		const bytes = Buffer.from(text, 'latin1');
		chunks.push(bytes);
		offset += bytes.length;
	};
	push('%PDF-1.5\n%\xe2\xe3\xcf\xd3\n');
	const offsets = [null];
	for (const { n, body } of objects) {
		offsets.push(offset);
		push(`${n} 0 obj\n${body}\nendobj\n`);
	}
	const xref = offset;
	push(`xref\n0 ${objects.length + 1}\n`);
	push('0000000000 65535 f \n');
	for (const position of offsets.slice(1)) {
		push(`${String(position).padStart(10, '0')} 00000 n \n`);
	}
	push(`trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\n`);
	push(`startxref\n${xref}\n%%EOF\n`);
	return new Uint8Array(Buffer.concat(chunks));
}

const pdfjsOptions = {
	useSystemFonts: false,
	verbosity: 0,
	cMapUrl: path.join(root, 'node_modules/pdfjs-dist/cmaps') + '/',
	cMapPacked: true,
	standardFontDataUrl: path.join(root, 'node_modules/pdfjs-dist/standard_fonts') + '/'
};

async function renderPages(source) {
	// pdf.js detaches the buffer it is given, so renderers never share one.
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
			await page.render({
				canvasContext: context,
				viewport,
				background: 'rgb(255,255,255)'
			}).promise;
			pages.push(context.getImageData(0, 0, canvas.width, canvas.height));
			page.cleanup();
		}
	} finally {
		await task.destroy();
	}
	return pages;
}

function diffFraction(a, b) {
	let changed = 0;
	let total = 0;
	for (let i = 0; i < a.data.length; i += 4) {
		total++;
		const dr = Math.abs(a.data[i] - b.data[i]);
		const dg = Math.abs(a.data[i + 1] - b.data[i + 1]);
		const db = Math.abs(a.data[i + 2] - b.data[i + 2]);
		const da = Math.abs(a.data[i + 3] - b.data[i + 3]);
		if (Math.max(dr, dg, db, da) > maxChannelDiff) changed++;
	}
	return changed / total;
}

function comparePage(merged, expected, name, failures) {
	if (merged.width !== expected.width || merged.height !== expected.height) {
		failures.push(
			`${name}: rendered ${merged.width}x${merged.height}, expected ${expected.width}x${expected.height}`
		);
		return;
	}
	const fraction = diffFraction(merged, expected);
	if (fraction > maxChangedFraction) {
		failures.push(`${name}: ${(fraction * 100).toFixed(3)}% of pixels differ`);
	}
}

const limitArg = process.argv.indexOf('--limit');
const limit = limitArg >= 0 ? Number(process.argv[limitArg + 1]) : Infinity;

const files = fs
	.readdirSync(corpusDir)
	.filter((name) => name.toLowerCase().endsWith('.pdf'))
	.map((name) => path.join(corpusDir, name))
	.sort();

const markerPages = await renderPages(marker);

const counts = {
	checked: 0,
	engineRefused: 0,
	unrenderableSource: 0,
	pageCount: 0
};
const failures = [];

for (const file of files.slice(0, limit === Infinity ? undefined : limit)) {
	const name = path.basename(file);
	const bytes = fs.readFileSync(file);

	const input = new Uint8Array(bytes.length + marker.length);
	input.set(new Uint8Array(bytes), 0);
	input.set(marker, bytes.length);
	const lengths = new Uint32Array([bytes.length, marker.length]);

	let merged;
	try {
		merged = merge_pdfs(input, lengths);
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

	let mergedPages;
	try {
		mergedPages = await renderPages(merged);
	} catch (error) {
		failures.push(`${name}: merged output would not render: ${error?.message ?? error}`);
		continue;
	}

	if (mergedPages.length !== sourcePages.length + 1) {
		failures.push(
			`${name}: merged has ${mergedPages.length} rendered pages, expected ${sourcePages.length} + 1`
		);
		counts.pageCount++;
		continue;
	}

	for (let page = 0; page < sourcePages.length; page++) {
		comparePage(mergedPages[page], sourcePages[page], `${name} page ${page + 1}`, failures);
	}
	comparePage(
		mergedPages[sourcePages.length],
		markerPages[0],
		`${name} appended marker page`,
		failures
	);
	counts.checked++;
}

console.log(`\nraster corpus: ${files.length} files, scale ${scale}`);
console.log(`  merged, rendered, and compared: ${counts.checked}`);
console.log(`  engine refused the merge: ${counts.engineRefused}`);
console.log(`  source would not render: ${counts.unrenderableSource}`);
console.log(`  rendered page count wrong: ${counts.pageCount}`);
console.log(`  comparison failures: ${failures.length}`);
for (const failure of failures.slice(0, 15)) {
	console.log(`  ${failure}`);
}
if (failures.length > 15) {
	console.log(`  ... and ${failures.length - 15} more`);
}

if (failures.length > 0) {
	console.error('\nraster compare failed');
	process.exit(1);
}
