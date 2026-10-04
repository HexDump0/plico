// OCR PDF over the PDF corpus: does a scan come back searchable, saying
// what the page said, where it said it, and looking as it did?
//
// Pages with text become scans: each is rendered at 150 DPI and put back as
// a picture-only PDF with the engine's images to PDF, so the text it showed
// is known. The scan is read the way the app reads it (pdf.js at up to 300
// DPI, grey PGM, Tesseract with the bundled English model, words measured by
// `src/lib/pdf/ocr.svelte.ts`) and the engine writes the words in. Then:
//
// - Poppler, which shares no code with the engine, must find most of the
//   original page's words in the result (recall), each about where the
//   original had it.
// - pdf.js must render the result exactly as it rendered the scan: the text
//   is invisible.
//
// Run from the repo root, with the corpus on disk and poppler's pdftotext on
// PATH. Recognition is slow, so it reads a sample:
//
// ```sh
// npm run test:ocr [-- --limit 40]
// ```

import { registerHooks } from 'node:module';
import { getDocument } from 'pdfjs-dist/legacy/build/pdf.mjs';
import { createCanvas } from '@napi-rs/canvas';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createWorker, OEM } from 'tesseract.js';
import init, { add_text_layer, images_to_pdf } from '../src/lib/pdf/wasm/plico_engine.js';

registerHooks({
	resolve(specifier, context, next) {
		try {
			return next(specifier, context);
		} catch (error) {
			if (specifier.startsWith('.') && !path.extname(specifier))
				return next(`${specifier}.ts`, context);
			throw error;
		}
	}
});
const { pageWords, ocrText } = await import('../src/lib/pdf/ocr.svelte.ts');

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const corpusDir = process.env.PLICO_CORPUS || path.join(root, 'testing/pdfjs/test/pdfs');
const limitArg = process.argv.indexOf('--limit');
const limit = limitArg > 0 ? Number(process.argv[limitArg + 1]) : 24;

const scanDpi = 150;
const ocrDpi = 300;
const ocrPixels = 12_000_000;
// Tesseract's English model on clean 150 DPI scans of mostly Latin text.
const minRecall = 0.85;
// How far, as a share of the page, a word may sit from where it was.
const placeTolerance = 0.03;

const wasmPath = path.join(root, 'src/lib/pdf/wasm/plico_engine_bg.wasm');
await init({ module_or_path: new Uint8Array(fs.readFileSync(wasmPath)) });

const pdfjsOptions = {
	useSystemFonts: false,
	verbosity: 0,
	cMapUrl: path.join(root, 'node_modules/pdfjs-dist/cmaps') + '/',
	cMapPacked: true,
	standardFontDataUrl: path.join(root, 'node_modules/pdfjs-dist/standard_fonts') + '/'
};

async function render(bytes, scale) {
	const pdf = await getDocument({ data: new Uint8Array(bytes), ...pdfjsOptions }).promise;
	try {
		const page = await pdf.getPage(1);
		const size = page.getViewport({ scale: 1 });
		const factor = typeof scale === 'function' ? scale(size) : scale;
		const viewport = page.getViewport({ scale: factor });
		const canvas = createCanvas(Math.ceil(viewport.width), Math.ceil(viewport.height));
		const context = canvas.getContext('2d');
		context.fillStyle = 'white';
		context.fillRect(0, 0, canvas.width, canvas.height);
		await page.render({ canvasContext: context, viewport }).promise;
		return { canvas, context, factor };
	} finally {
		await pdf.destroy();
	}
}

/// Words poppler reads, with their centres as fractions of the page.
function popplerWords(bytes) {
	const file = path.join(os.tmpdir(), `plico-ocr-${process.pid}.pdf`);
	fs.writeFileSync(file, bytes);
	try {
		const html = execFileSync('pdftotext', ['-bbox', '-cropbox', '-f', '1', '-l', '1', file, '-'], {
			encoding: 'utf8',
			maxBuffer: 64 << 20
		});
		const page = html.match(/<page width="([\d.]+)" height="([\d.]+)">/);
		if (!page) return [];
		const [width, height] = [Number(page[1]), Number(page[2])];
		return [
			...html.matchAll(/xMin="([\d.]+)" yMin="([\d.]+)" xMax="([\d.]+)" yMax="([\d.]+)">([^<]*)</g)
		]
			.map(([, x0, y0, x1, y1, text]) => ({
				text: normalise(text),
				x: (Number(x0) + Number(x1)) / 2 / width,
				y: (Number(y0) + Number(y1)) / 2 / height
			}))
			.filter((word) => word.text.length >= 3);
	} finally {
		fs.rmSync(file, { force: true });
	}
}

function normalise(text) {
	return text
		.replace(/&amp;/g, '&')
		.normalize('NFKC')
		.toLowerCase()
		.replace(/[^\p{L}\p{N}]/gu, '');
}

function pgm({ context, canvas }) {
	const { data } = context.getImageData(0, 0, canvas.width, canvas.height);
	const header = new TextEncoder().encode(`P5\n${canvas.width} ${canvas.height}\n255\n`);
	const image = new Uint8Array(header.length + canvas.width * canvas.height);
	image.set(header);
	for (let pixel = 0, at = header.length; pixel < data.length; pixel += 4, at++)
		image[at] = (data[pixel] * 77 + data[pixel + 1] * 150 + data[pixel + 2] * 29) >> 8;
	return image;
}

function firstPage(bytes) {
	const file = path.join(os.tmpdir(), `plico-ocr-src-${process.pid}.pdf`);
	fs.writeFileSync(file, bytes);
	try {
		return execFileSync('pdftotext', ['-f', '1', '-l', '1', file, '-'], {
			encoding: 'utf8',
			stdio: ['ignore', 'pipe', 'ignore']
		});
	} catch {
		return '';
	} finally {
		fs.rmSync(file, { force: true });
	}
}

const worker = await createWorker('eng', OEM.LSTM_ONLY, {
	langPath: path.join(root, 'static/tessdata'),
	cacheMethod: 'none'
});

const files = fs
	.readdirSync(corpusDir)
	.filter((name) => name.endsWith('.pdf'))
	.sort();
let checked = 0;
let totalWords = 0;
let foundWords = 0;
const failures = [];
for (const name of files) {
	if (checked >= limit) break;
	const source = fs.readFileSync(path.join(corpusDir, name));
	if (source.length > 5_000_000) continue;
	// Pages of ordinary prose, which the English model is for.
	const text = firstPage(source);
	const latin = text.match(/[a-z]{3,}/gi)?.length ?? 0;
	if (latin < 60 || latin > 1500) continue;
	let original;
	let scan;
	try {
		original = popplerWords(source);
		const shot = await render(source, scanDpi / 72);
		const png = shot.canvas.toBuffer('image/png');
		scan = images_to_pdf(png, Uint32Array.of(png.length), 0, 0, 0, 0);
	} catch {
		continue;
	}
	if (original.length < 30) continue;
	checked++;
	const read = await render(scan, (size) =>
		Math.min(ocrDpi / 72, Math.sqrt(ocrPixels / (size.width * size.height)))
	);
	await worker.setParameters({ user_defined_dpi: String(Math.round(read.factor * 72)) });
	const { data } = await worker.recognize(
		Buffer.from(pgm(read)),
		{},
		{ blocks: true, text: false }
	);
	const words = pageWords(1, data.blocks ?? [], read.canvas.width, read.canvas.height).map(ocrText);
	if (!words.length) {
		failures.push(`${name}: nothing recognized`);
		continue;
	}
	const output = add_text_layer(
		new Uint8Array(scan),
		'',
		Uint32Array.from(words, (word) => word.page),
		words.map((word) => word.text),
		Float32Array.from(words.flatMap((w) => [w.left, w.width, w.baseline, w.size, w.angle])),
		Uint8Array.from(words, (word) => Number(word.space))
	);
	const found = popplerWords(output);
	let hits = 0;
	for (const word of original) {
		const match = found.find(
			(other) =>
				other.text === word.text &&
				Math.abs(other.x - word.x) < placeTolerance &&
				Math.abs(other.y - word.y) < placeTolerance
		);
		if (match) {
			hits++;
			found.splice(found.indexOf(match), 1);
		}
	}
	totalWords += original.length;
	foundWords += hits;
	const recall = hits / original.length;
	const pixels = ({ context, canvas }) => context.getImageData(0, 0, canvas.width, canvas.height);
	const before = pixels(await render(scan, 0.5));
	const after = pixels(await render(output, 0.5));
	let changed = 0;
	for (let index = 0; index < before.data.length; index++)
		if (before.data[index] !== after.data[index]) changed++;
	const line = `${name}: ${hits}/${original.length} words (${(recall * 100).toFixed(1)}%)`;
	if (changed) failures.push(`${name}: the text layer changed ${changed} rendered values`);
	if (recall < minRecall) failures.push(line);
	console.log(line);
}
await worker.terminate();

console.log(
	`\n${checked} scans read, ${foundWords}/${totalWords} words found in place (${((foundWords / Math.max(1, totalWords)) * 100).toFixed(1)}%)`
);
if (failures.length) {
	console.log(`\n${failures.length} failures:\n${failures.join('\n')}`);
	process.exitCode = 1;
}
