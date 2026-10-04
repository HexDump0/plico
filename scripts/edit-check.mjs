// Edit PDF over the PDF corpus: is replaced text gone, its replacement drawn,
// and the rest of the page as it was?
//
// On every page the longest line of text the app would let a person edit is
// replaced with "Plico edit", placed and styled the way the app places it.
// Lines are found by `src/lib/pdf/edit-text.ts` itself. Then:
//
// - Poppler, which shares no code with the engine, must find no word left
//   on the old line, a word whose middle lies in the band along its baseline
//   that the engine was asked to clear, unless the engine said it painted
//   that page over instead. It must find the new text on the page.
// - pdf.js renders source and result, and the page outside the line and its
//   replacement, beyond a margin, must render as before.
//
// Run from the repo root, with the corpus on disk and poppler's pdftotext on
// PATH:
//
// ```sh
// npm run test:edit [-- --limit 100]
// ```

import { registerHooks } from 'node:module';
import { getDocument } from 'pdfjs-dist/legacy/build/pdf.mjs';
import { createCanvas } from '@napi-rs/canvas';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import init, { edit_pdf, redaction_text } from '../src/lib/pdf/wasm/plico_engine.js';

// The app's TypeScript imports its neighbours without an extension.
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
const { placeRun, textRuns } = await import('../src/lib/pdf/edit-text.ts');
const { capHeight, textWidth } = await import('../src/lib/pdf/standard-fonts.ts');

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const corpusDir = process.env.PLICO_CORPUS || path.join(root, 'testing/pdfjs/test/pdfs');

const scale = 0.5;
const maxPagePixels = 20_000_000;
const maxChannelDiff = 32;
const maxChangedFraction = 0.005;
// Glyphs a quarter under the box go whole, and the old and new text differ
// in width, so the page may change this far past them, in points.
const margin = 12;

const replacement = 'Plico edit';
const PADDING = 2;
const NO_FILL = 0xffffffff;
const TEXT = 9;
const families = { helvetica: 0, times: 1, courier: 2 };
const reasons = ['text', 'image', 'content'];

const wasmPath = path.join(root, 'src/lib/pdf/wasm/plico_engine_bg.wasm');
await init({ module_or_path: new Uint8Array(fs.readFileSync(wasmPath)) });

const pdfjsOptions = {
	useSystemFonts: false,
	verbosity: 0,
	cMapUrl: path.join(root, 'node_modules/pdfjs-dist/cmaps') + '/',
	cMapPacked: true,
	standardFontDataUrl: path.join(root, 'node_modules/pdfjs-dist/standard_fonts') + '/'
};

async function renderPages(source, numbers, scales) {
	const task = getDocument({ data: new Uint8Array(source), ...pdfjsOptions });
	const pdf = await task.promise;
	const pages = new Map();
	try {
		for (const number of numbers) {
			const page = await pdf.getPage(number);
			const base = page.getViewport({ scale: 1 });
			const pageScale =
				scales?.get(number) ??
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
			pages.set(number, image);
			page.cleanup();
		}
	} finally {
		await task.destroy();
	}
	return pages;
}

/// Each page's size in points as shown, from pdf.js.
async function pageSizes(source) {
	const task = getDocument({ data: new Uint8Array(source), ...pdfjsOptions });
	const pdf = await task.promise;
	try {
		const sizes = [];
		for (let number = 1; number <= pdf.numPages; number++) {
			const { width, height } = (await pdf.getPage(number)).getViewport({ scale: 1 });
			sizes.push({ width, height });
		}
		return sizes;
	} finally {
		await task.destroy();
	}
}

function words(bytes) {
	const file = path.join(os.tmpdir(), `plico-edit-${process.pid}.pdf`);
	fs.writeFileSync(file, bytes);
	try {
		const html = execFileSync('pdftotext', ['-bbox', '-cropbox', file, '-'], {
			encoding: 'utf8',
			stdio: ['ignore', 'pipe', 'ignore'],
			maxBuffer: 256 * 1024 * 1024,
			timeout: 60_000
		});
		return html
			.split('<page ')
			.slice(1)
			.map((chunk) => {
				const [, width, height] = chunk.match(/width="([\d.]+)" height="([\d.]+)"/) ?? [];
				return [
					...chunk.matchAll(
						/<word xMin="([\d.-]+)" yMin="([\d.-]+)" xMax="([\d.-]+)" yMax="([\d.-]+)">([^<]*)<\/word>/g
					)
				].map(([, x0, y0, x1, y1, text]) => ({
					box: [x0 / width, y0 / height, x1 / width, y1 / height],
					text
				}));
			});
	} finally {
		fs.rmSync(file, { force: true });
	}
}

/// Whether a word's middle lies in `area`.
function on({ box: [left, top, right, bottom] }, [l, t, r, b]) {
	const [x, y] = [(left + right) / 2, (top + bottom) / 2];
	return x >= l && x <= r && y >= t && y <= b;
}

/// The replacement's text box, as `AnnotateEditor.replace` places it.
function placed(run, page) {
	const { size, left, top } = placeRun(run, page);
	const width = textWidth(replacement, run.family, run.bold, size) + 2 * PADDING + 0.5;
	const height = 2 * PADDING + capHeight(run.family, run.bold) * size + 0.25 * size;
	return {
		size,
		area: [
			left,
			top,
			Math.min(1, left + width / page.width),
			Math.min(1, top + height / page.height)
		]
	};
}

const limitArg = process.argv.indexOf('--limit');
const limit = limitArg >= 0 ? Number(process.argv[limitArg + 1]) : Infinity;
const files = fs
	.readdirSync(corpusDir)
	.filter((name) => name.toLowerCase().endsWith('.pdf'))
	.sort()
	.slice(0, limit === Infinity ? undefined : limit);

const counts = {
	files: 0,
	edited: 0,
	pages: 0,
	covered: { text: 0, image: 0, content: 0 },
	withoutText: 0,
	engineRefused: 0,
	unreadable: 0,
	wordsReplaced: 0,
	// Pages poppler finds no words on even before the edit.
	unreadByPoppler: 0
};
const looks = { helvetica: 0, times: 0, courier: 0, bold: 0, italic: 0, unknownColor: 0 };
const leaks = [];
const missing = [];
const damage = [];

for (const name of files) {
	const bytes = fs.readFileSync(path.join(corpusDir, name));
	let glyphs;
	let sizes;
	try {
		glyphs = redaction_text(bytes, '').map(([boxes, text, ends, metrics, colors, looks]) => ({
			boxes,
			text,
			ends,
			metrics,
			colors,
			looks
		}));
		sizes = await pageSizes(bytes);
	} catch {
		counts.unreadable++;
		continue;
	}
	counts.files++;
	const edits = [];
	glyphs.forEach((page, index) => {
		const size = sizes[index];
		if (!size) return;
		const runs = textRuns(page, size.width, size.height).filter((run) => run.text.length >= 3);
		if (!runs.length) return;
		const run = runs.reduce((best, next) =>
			next.glyphs.length > best.glyphs.length ? next : best
		);
		edits.push({ number: index + 1, run, ...placed(run, size) });
	});
	if (!edits.length) {
		counts.withoutText++;
		continue;
	}
	for (const { run } of edits) {
		looks[run.family]++;
		if (run.bold) looks.bold++;
		if (run.italic) looks.italic++;
		if (run.color === undefined) looks.unknownColor++;
	}

	let output;
	let covered;
	try {
		const [result, pages, why] = edit_pdf(
			bytes,
			'',
			new Uint32Array(),
			new Float32Array(),
			new Float32Array(),
			new Uint8Array(),
			Uint32Array.from(edits, (edit) => edit.number),
			Float32Array.from(edits.flatMap((edit) => edit.run.band)),
			Float32Array.from(edits.flatMap((edit) => edit.run.ink)),
			Uint32Array.from(edits, () => 0xffffff),
			new Uint32Array(),
			new Float32Array(),
			new Uint32Array(),
			Uint8Array.from(edits, () => TEXT),
			Uint32Array.from(edits, (edit) => edit.number),
			Uint32Array.from(edits, (edit) => edit.run.color ?? 0),
			Float32Array.from(edits, () => 1),
			Float32Array.from(edits, (edit) => edit.size),
			Uint32Array.from(edits, () => NO_FILL),
			Uint8Array.from(edits, (edit) => families[edit.run.family] * 2 + Number(edit.run.bold)),
			Uint32Array.from(edits, () => 4),
			Float32Array.from(edits.flatMap((edit) => edit.area)),
			edits.map(() => replacement),
			new Uint8Array(),
			new Uint32Array()
		);
		output = result;
		covered = new Map(Array.from(pages, (page, index) => [page, reasons[why[index]] ?? 'content']));
	} catch (error) {
		counts.engineRefused++;
		damage.push(`${name}: the engine refused: ${error?.message ?? error}`);
		continue;
	}
	for (const reason of covered.values()) counts.covered[reason]++;

	let before;
	let after;
	try {
		before = words(bytes);
	} catch {
		counts.unreadable++;
		continue;
	}
	try {
		after = words(output);
	} catch (error) {
		leaks.push(`${name}: poppler could not read the result: ${error?.message ?? error}`);
		continue;
	}
	for (const { number, run } of edits) {
		const label = `${name} page ${number}`;
		const found = after[number - 1] ?? [];
		if (!before[number - 1]?.length) {
			counts.unreadByPoppler++;
			continue;
		}
		counts.wordsReplaced += (before[number - 1] ?? []).filter((word) => on(word, run.band)).length;
		if (!found.some((word) => word.text === 'Plico'))
			missing.push(`${label}: the replacement is not on the page`);
		if (covered.has(number)) continue;
		const left = found.find(
			(word) => on(word, run.band) && word.text !== 'Plico' && word.text !== 'edit'
		);
		if (left) leaks.push(`${label}: "${left.text}" from "${run.text}" is still under its box`);
	}

	const numbers = edits.map((edit) => edit.number);
	let sources;
	let results;
	try {
		sources = await renderPages(bytes, numbers);
		results = await renderPages(
			output,
			numbers,
			new Map([...sources].map(([number, page]) => [number, page.scale]))
		);
	} catch (error) {
		damage.push(`${name}: would not render: ${error?.message ?? error}`);
		continue;
	}
	for (const { number, run, area } of edits) {
		const source = sources.get(number);
		const page = results.get(number);
		// lopdf writes boxes back with fewer digits, which can round a
		// page's pixel size the other way: one pixel either way is the same.
		if (
			!source ||
			!page ||
			Math.abs(source.width - page.width) > 1 ||
			Math.abs(source.height - page.height) > 1
		) {
			damage.push(`${name} page ${number}: renders at another size`);
			continue;
		}
		const [width, height] = [
			Math.min(source.width, page.width),
			Math.min(source.height, page.height)
		];
		const reach = margin * source.scale;
		const skip = [run.ink, area].map(([l, t, r, b]) => [
			l * width - reach,
			t * height - reach,
			r * width + reach,
			b * height + reach
		]);
		let changed = 0;
		let outside = 0;
		for (let y = 0; y < height; y++) {
			for (let x = 0; x < width; x++) {
				if (skip.some(([l, t, r, b]) => x >= l && x < r && y >= t && y < b)) continue;
				const i = (y * page.width + x) * 4;
				const j = (y * source.width + x) * 4;
				outside++;
				const difference = Math.max(
					Math.abs(page.data[i] - source.data[j]),
					Math.abs(page.data[i + 1] - source.data[j + 1]),
					Math.abs(page.data[i + 2] - source.data[j + 2])
				);
				if (difference > maxChannelDiff) changed++;
			}
		}
		if (outside && changed / outside > maxChangedFraction)
			damage.push(
				`${name} page ${number}: ${((changed / outside) * 100).toFixed(3)}% of pixels away from the edit changed`
			);
		counts.pages++;
	}
	counts.edited++;
}

console.log(
	`${counts.files} files read, ${counts.edited} edited, ${counts.pages} pages compared; ` +
		`${counts.withoutText} with no editable text, ${counts.engineRefused} refused, ` +
		`${counts.unreadable} unreadable`
);
console.log(
	`${counts.wordsReplaced} words replaced, ${counts.unreadByPoppler} pages poppler cannot read; ` +
		`painted over instead: ` +
		`${counts.covered.text} pages for unmeasurable text, ${counts.covered.image} for images, ` +
		`${counts.covered.content} for unreadable content`
);
console.log(
	`Lines read as Helvetica ${looks.helvetica}, Times ${looks.times}, Courier ${looks.courier}; ` +
		`${looks.bold} bold, ${looks.italic} italic, ${looks.unknownColor} of unknown colour`
);
for (const [title, list] of [
	['Left behind', leaks],
	['Replacement missing', missing],
	['Changed elsewhere', damage]
]) {
	if (!list.length) continue;
	console.log(`\n${title} (${list.length}):`);
	for (const line of list) console.log(`  ${line}`);
}
process.exitCode = leaks.length || missing.length || damage.length ? 1 : 0;
