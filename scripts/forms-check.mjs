// Form filling over the PDF corpus, checked three ways.
//
// Every form gets a new value in each field it lets a person fill: a short
// distinctive text, another option, a check box or radio button turned the
// other way. Then:
//
// - pdf.js reads the filled file back, and every widget of every field must
//   report the value given.
// - The file is filled again and flattened, and poppler's pdftotext must find
//   each text in the page content, so a value that is set but never drawn
//   fails. Content order, since a comb field spreads its characters apart.
// - pdf.js renders the filled file, which draws its stored appearances, and
//   the flattened one, which has them in the page; the two must match as in
//   the flatten check. A form that asks readers to redraw its fields is drawn
//   by pdf.js in its own way, so those are reported apart.
//
// Run from the repo root, with the corpus on disk and poppler on PATH:
//
// ```sh
// npm run test:forms [-- --limit 100]
// ```

import { getDocument } from 'pdfjs-dist/legacy/build/pdf.mjs';
import { createCanvas } from '@napi-rs/canvas';
import { execFileSync } from 'node:child_process';
import zlib from 'node:zlib';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import init, { fill_form } from '../src/lib/pdf/wasm/plico_engine.js';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const corpusDir = process.env.PLICO_CORPUS || path.join(root, 'testing/pdfjs/test/pdfs');

const scale = 0.5;
const maxPagePixels = 20_000_000;
const maxChannelDiff = 32;
const maxChangedFraction = 0.001;
const HIDDEN = 2;
const NO_VIEW = 32;

const wasmPath = path.join(root, 'src/lib/pdf/wasm/plico_engine_bg.wasm');
await init({ module_or_path: new Uint8Array(fs.readFileSync(wasmPath)) });

const pdfjsOptions = {
	useSystemFonts: false,
	verbosity: 0,
	cMapUrl: path.join(root, 'node_modules/pdfjs-dist/cmaps') + '/',
	cMapPacked: true,
	standardFontDataUrl: path.join(root, 'node_modules/pdfjs-dist/standard_fonts') + '/'
};

/// Every widget on every page, as pdf.js reads it.
async function readWidgets(source) {
	const task = getDocument({ data: new Uint8Array(source), ...pdfjsOptions });
	const pdf = await task.promise;
	const widgets = [];
	try {
		for (let number = 1; number <= pdf.numPages; number++) {
			const page = await pdf.getPage(number);
			for (const annotation of await page.getAnnotations())
				if (annotation.subtype === 'Widget') widgets.push({ ...annotation, page: number });
			page.cleanup();
		}
	} finally {
		await task.destroy();
	}
	return widgets;
}

async function renderPages(source) {
	const task = getDocument({ data: new Uint8Array(source), ...pdfjsOptions });
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

const objectNumber = (id) => (/^\d+R$/.test(id) ? Number.parseInt(id, 10) : null);

/// A new value for each field a person could fill, by field name.
function plan(widgets) {
	const fields = new Map();
	for (const widget of widgets) {
		if (!fields.has(widget.fieldName)) fields.set(widget.fieldName, []);
		fields.get(widget.fieldName).push(widget);
	}
	const values = [];
	let index = 0;
	for (const [name, group] of fields) {
		const first = group[0];
		if (group.some((widget) => widget.readOnly || widget.password || widget.pushButton)) continue;
		const ids = group.map((widget) => objectNumber(widget.id));
		if (ids.some((id) => id === null)) continue;
		const visible = group.some(
			(widget) => !((widget.annotationFlags ?? 0) & (HIDDEN | NO_VIEW)) && !widget.hidden
		);
		if (first.fieldType === 'Tx') {
			const token = `Plico${(index++).toString(36)}`;
			// Fields can share a name and still differ in length.
			const lengths = group.map((widget) => widget.maxLen).filter((length) => length > 0);
			const text = lengths.length ? token.slice(0, Math.min(...lengths)) : token;
			values.push({ name, ids, kind: 0, values: [text], expect: text, visible, text: true });
		} else if (first.fieldType === 'Ch' && first.options?.length) {
			const current = first.fieldValue?.[0];
			const option =
				first.options.findLast((option) => option.exportValue !== current) ?? first.options[0];
			values.push({
				name,
				ids,
				kind: 1,
				values: [option.exportValue],
				expect: [option.exportValue]
			});
		} else if (first.fieldType === 'Btn' && (first.checkBox || first.radioButton)) {
			// Every widget of the name goes: the chosen one on, the rest off.
			const value = (widget) => (first.checkBox ? widget.exportValue : widget.buttonValue);
			const on = first.checkBox && first.fieldValue !== 'Off' && first.fieldValue === value(first);
			const index = first.checkBox
				? 0
				: Math.max(
						group.findIndex((widget) => value(widget) && value(widget) !== first.fieldValue),
						0
					);
			const chosen = on ? null : group[index];
			if (chosen && !value(chosen)) continue;
			values.push({
				name,
				ids,
				kinds: group.map((widget) => (widget === chosen ? 2 : 3)),
				kind: 2,
				values: [],
				checkBox: first.checkBox,
				expect: chosen ? value(chosen) : 'Off'
			});
		}
	}
	return values;
}

function fill(bytes, planned, flatten) {
	const entries = planned.flatMap((field) =>
		field.ids.map((id, index) => ({ ...field, id, kind: field.kinds?.[index] ?? field.kind }))
	);
	return fill_form(
		bytes,
		'',
		Uint32Array.from(entries.map((entry) => entry.id)),
		Uint8Array.from(entries.map((entry) => entry.kind)),
		Uint32Array.from(entries.map((entry) => entry.values.length)),
		entries.flatMap((entry) => entry.values),
		flatten
	);
}

/// Whether the file says `pattern` anywhere, inside compressed object streams
/// too, where the output keeps its form dictionary.
function mentions(bytes, pattern) {
	const raw = Buffer.from(bytes).toString('latin1');
	if (pattern.test(raw)) return true;
	for (const match of raw.matchAll(/stream\r?\n/g)) {
		const start = match.index + match[0].length;
		const end = raw.indexOf('endstream', start);
		if (end < 0) continue;
		try {
			const inflated = zlib.inflateSync(Buffer.from(bytes).subarray(start, end));
			if (pattern.test(inflated.toString('latin1'))) return true;
		} catch {
			// Not deflated, or not an object stream.
		}
	}
	return false;
}

const same = (a, b) => JSON.stringify(a ?? null) === JSON.stringify(b ?? null);

const limitArg = process.argv.indexOf('--limit');
const limit = limitArg >= 0 ? Number(process.argv[limitArg + 1]) : Infinity;
const files = fs
	.readdirSync(corpusDir)
	.filter((name) => name.toLowerCase().endsWith('.pdf'))
	.sort()
	.slice(0, limit === Infinity ? undefined : limit);

const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'plico-forms-'));
const counts = {
	forms: 0,
	fields: 0,
	texts: 0,
	kept: 0,
	refused: 0,
	unreadable: 0,
	compared: 0,
	redrawn: 0
};
const refusals = [];
const failures = [];
const redrawnFailures = [];

for (const name of files) {
	const bytes = fs.readFileSync(path.join(corpusDir, name));
	let widgets;
	try {
		widgets = await readWidgets(bytes);
	} catch {
		counts.unreadable++;
		continue;
	}
	const planned = plan(widgets);
	if (planned.length === 0) continue;

	let filled;
	let flat;
	let kept;
	try {
		[filled] = fill(bytes, planned, false);
		[flat, kept] = fill(bytes, planned, true);
	} catch (error) {
		counts.refused++;
		refusals.push(`${name}: ${error?.message ?? error}`);
		continue;
	}
	counts.forms++;
	counts.fields += planned.length;
	counts.kept += kept;

	// pdf.js reads back every value given.
	const reread = await readWidgets(filled);
	for (const field of planned) {
		for (const widget of reread.filter((widget) => widget.fieldName === field.name)) {
			const value = widget.fieldValue;
			// Check boxes sharing a name each have their own on state; only
			// the ones sharing the chosen one's turn on.
			const expect = field.checkBox && widget.exportValue !== field.expect ? 'Off' : field.expect;
			if (!same(value, expect))
				failures.push(
					`${name}: “${field.name}” reads back ${JSON.stringify(value)}, not ${JSON.stringify(expect)}`
				);
		}
	}

	// Poppler finds each text in the flattened pages.
	const flatPath = path.join(scratch, 'flat.pdf');
	fs.writeFileSync(flatPath, flat);
	let text = '';
	try {
		text = execFileSync('pdftotext', ['-q', '-raw', flatPath, '-'], {
			maxBuffer: 1 << 28
		}).toString();
	} catch (error) {
		failures.push(`${name}: poppler could not read the flattened file: ${error.message}`);
	}
	for (const field of planned.filter((field) => field.text && field.visible)) {
		counts.texts++;
		if (field.expect.length >= 3 && !text.replace(/\s+/g, '').includes(field.expect))
			failures.push(`${name}: “${field.name}” was not drawn into the page`);
	}

	// Filled and flattened render alike.
	let before;
	let after;
	try {
		before = await renderPages(filled);
		after = await renderPages(flat);
	} catch (error) {
		failures.push(`${name}: output would not render: ${error?.message ?? error}`);
		continue;
	}
	if (before.length !== after.length) {
		failures.push(`${name}: rendered page count changed`);
		continue;
	}
	// A form asking readers to redraw its fields keeps that flag when filled,
	// so pdf.js draws them its own way there.
	const redraws = mentions(filled, /NeedAppearances\s*true/);
	const target = redraws ? redrawnFailures : failures;
	if (redraws) counts.redrawn++;
	else counts.compared++;
	for (let index = 0; index < before.length; index++) {
		if (
			before[index].width !== after[index].width ||
			before[index].height !== after[index].height
		) {
			target.push(`${name} page ${index + 1}: flattening changed the page size`);
			continue;
		}
		const changed = changedFraction(before[index], after[index]);
		if (changed > maxChangedFraction)
			target.push(
				`${name} page ${index + 1}: ${(changed * 100).toFixed(3)}% of pixels differ once flattened`
			);
	}
}
fs.rmSync(scratch, { recursive: true, force: true });

console.log(`\nform filling corpus: ${files.length} files`);
console.log(`  forms filled: ${counts.forms}, ${counts.fields} fields`);
console.log(`  visible texts looked for in the flattened pages: ${counts.texts}`);
console.log(`  fields flattening left as they were: ${counts.kept}`);
console.log(`  refused by the engine: ${counts.refused}`);
for (const refusal of refusals) console.log(`    ${refusal}`);
console.log(`  unreadable by pdf.js: ${counts.unreadable}`);
console.log(`  rendered and compared: ${counts.compared}`);
console.log(`  redrawn by pdf.js, compared apart: ${counts.redrawn}`);
for (const failure of redrawnFailures.slice(0, 40)) console.log(`    ${failure}`);
console.log(`  failures: ${failures.length}`);
for (const failure of failures.slice(0, 60)) console.log(`    ${failure}`);
if (failures.length > 60) console.log(`    ... and ${failures.length - 60} more`);

if (failures.length > 0) {
	console.error('\nform filling check failed');
	process.exit(1);
}
