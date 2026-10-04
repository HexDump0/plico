// The lines of text Edit PDF can replace, read from the engine's glyphs: runs
// of upright, visible glyphs that share a baseline and sit close together, so
// a table's cells or a page's columns stay apart. Each knows what it reads,
// how it is drawn, and the boxes the engine would take out for it.

import { glyphBox, glyphText, isBlank } from './redact-text';
import { capHeight, type FontFamily } from './standard-fonts';
import type { CropArea, PageGlyphs } from './types';

export type TextRun = {
	/// Glyph indices, left to right.
	glyphs: number[];
	/// The union of their boxes.
	box: CropArea;
	/// What the engine is asked to take out: a band from the baseline up to
	/// about the height of a lower case letter. Taller boxes would take
	/// neighbouring lines with them where lines sit closer than their fonts
	/// are tall, since a glyph goes once a quarter of it is covered.
	band: CropArea;
	/// Where the run's ink can reach, accents and descenders included, which
	/// is painted over in the preview and when the engine cannot take it out.
	ink: CropArea;
	/// As a fraction of the page's height from its top.
	baseline: number;
	/// In points.
	size: number;
	text: string;
	family: FontFamily;
	bold: boolean;
	italic: boolean;
	/// 0xRRGGBB, or undefined when the engine could not tell.
	color?: number;
};

const UNKNOWN_COLOR = 0xffffffff;
const families: FontFamily[] = ['helvetica', 'times', 'courier'];
// Glyphs further apart than this many ems are separate runs, as columns and
// table cells are. Further than WORD_GAP is a space: justified lines shrink
// spaces to a fifth of an em, and kerning stays well under a tenth.
const RUN_GAP = 1.2;
const WORD_GAP = 0.15;
// The sizes the engine draws text at, in points.
const SMALLEST = 1;
const LARGEST = 1000;
// Space between a text box's edges and its text, as the engine leaves it.
const PADDING = 2;

const size = (page: PageGlyphs, index: number) => page.metrics[index * 2];
const baseline = (page: PageGlyphs, index: number) => page.metrics[index * 2 + 1];
const look = (page: PageGlyphs, index: number) => page.looks[index];
const upright = (page: PageGlyphs, index: number) => (look(page, index) & 32) !== 0;
const invisible = (page: PageGlyphs, index: number) => (look(page, index) & 16) !== 0;

/// The value most of `values` share.
function most<T>(values: T[]): T {
	const counts = new Map<T, number>();
	let [best, count] = [values[0], 0];
	for (const value of values) {
		const next = (counts.get(value) ?? 0) + 1;
		counts.set(value, next);
		if (next > count) [best, count] = [value, next];
	}
	return best;
}

function median(values: number[]) {
	const sorted = [...values].sort((a, b) => a - b);
	return sorted[Math.floor(sorted.length / 2)];
}

const clamp = ([left, top, right, bottom]: CropArea): CropArea => [
	Math.max(0, left),
	Math.max(0, top),
	Math.min(1, right),
	Math.min(1, bottom)
];

/// Positions in `glyphs`, left to right, that draw the glyph before them
/// again nearly in place, as fake bold does. They read once.
function overprints(page: PageGlyphs, glyphs: number[]) {
	const repeats = new Set<number>();
	let kept = -1;
	glyphs.forEach((index, position) => {
		const text = glyphText(page, index);
		if (kept >= 0 && !isBlank(text) && text === glyphText(page, glyphs[kept])) {
			const [left, , right] = glyphBox(page, glyphs[kept]);
			if (glyphBox(page, index)[0] < (left + right) / 2) {
				repeats.add(position);
				return;
			}
		}
		kept = position;
	});
	return repeats;
}

function build(page: PageGlyphs, glyphs: number[], width: number, height: number): TextRun {
	const boxes = glyphs.map((index) => glyphBox(page, index));
	const shown = glyphs.filter((index) => !isBlank(glyphText(page, index)));
	const sizes = shown.map((index) => size(page, index));
	const em = median(sizes);
	const repeats = overprints(page, glyphs);
	let text = '';
	glyphs.forEach((index, position) => {
		if (repeats.has(position)) return;
		const characters = glyphText(page, index).normalize('NFKC');
		if (position > 0) {
			const gap = (boxes[position][0] - boxes[position - 1][2]) * width;
			if (gap > em * WORD_GAP && !text.endsWith(' ') && !characters.startsWith(' ')) text += ' ';
		}
		text += isBlank(characters) ? ' ' : characters;
	});
	const looks = shown.map((index) => look(page, index) & 15);
	const style = most(looks);
	const colors = shown
		.map((index) => page.colors[index])
		.filter((color) => color !== UNKNOWN_COLOR);
	const line = median(shown.map((index) => baseline(page, index)));
	const [left, right] = [
		Math.min(...boxes.map((box) => box[0])),
		Math.max(...boxes.map((box) => box[2]))
	];
	const [across, down] = [em / width, em / height];
	return {
		glyphs,
		// Text partly off the page is edited where it shows.
		box: [
			Math.max(0, Math.min(...boxes.map((box) => box[0]))),
			Math.max(0, Math.min(...boxes.map((box) => box[1]))),
			Math.min(1, Math.max(...boxes.map((box) => box[2]))),
			Math.min(1, Math.max(...boxes.map((box) => box[3])))
		],
		// A little past either end, for glyphs drawn with no width, which sit
		// on the run's edge; other runs are further off than this.
		band: clamp([left - 0.1 * across, line - 0.55 * down, right + 0.1 * across, line]),
		ink: clamp([
			left - 0.05 * across,
			line - 0.95 * down,
			right + 0.05 * across,
			line + 0.28 * down
		]),
		baseline: line,
		size: em,
		text: text.replace(/\s+/g, ' ').trim(),
		family: families[style & 3] ?? 'helvetica',
		// Most glyphs drawn twice is bold drawn by hand.
		bold: (style & 4) !== 0 || repeats.size * 2 >= shown.length - repeats.size,
		italic: (style & 8) !== 0,
		color: colors.length ? most(colors) : undefined
	};
}

// Worked out once per page, not on every pointer move.
const cache = new WeakMap<PageGlyphs, TextRun[]>();

/// Every run on a page that is shown, `width` and `height` in points.
export function textRuns(page: PageGlyphs, width: number, height: number): TextRun[] {
	const cached = cache.get(page);
	if (cached) return cached;
	const lines: number[][] = [];
	for (let index = 0; index < page.ends.length; index++) {
		const points = size(page, index);
		if (!upright(page, index) || invisible(page, index) || points < SMALLEST || points > LARGEST)
			continue;
		const at = baseline(page, index);
		const tolerance = (size(page, index) * 0.2) / height;
		const line = lines.find(
			(glyphs) =>
				Math.abs(baseline(page, glyphs[0]) - at) <= tolerance &&
				Math.abs(size(page, glyphs[0]) - size(page, index)) <= size(page, glyphs[0]) * 0.5
		);
		if (line) line.push(index);
		else lines.push([index]);
	}
	const runs: TextRun[] = [];
	for (const line of lines) {
		line.sort((a, b) => glyphBox(page, a)[0] - glyphBox(page, b)[0]);
		let run: number[] = [];
		const close = () => {
			// Spaces at either end belong to neither run.
			while (run.length && isBlank(glyphText(page, run[0]))) run.shift();
			while (run.length && isBlank(glyphText(page, run[run.length - 1]))) run.pop();
			const built = run.length ? build(page, run, width, height) : undefined;
			// A line whose baseline is off the page, as printer's marks are,
			// shows too little to edit, and its replacement would not show.
			if (built && built.baseline > 0 && built.baseline <= 1 && built.band[2] > built.band[0])
				runs.push(built);
			run = [];
		};
		for (const index of line) {
			const previous = run[run.length - 1];
			if (previous !== undefined) {
				const gap = (glyphBox(page, index)[0] - glyphBox(page, previous)[2]) * width;
				// Overprinted glyphs, as fake bold draws them, are one run too.
				if (gap > Math.max(size(page, index), size(page, previous)) * RUN_GAP) close();
			}
			run.push(index);
		}
		close();
	}
	cache.set(page, runs);
	return runs;
}

/// Where a text box replacing `run` goes so its first baseline sits on the
/// run's, as the engine lays text out, and the size it draws at.
export function placeRun(run: TextRun, page: { width: number; height: number }) {
	const size = Math.min(LARGEST, Math.max(SMALLEST, Math.round(run.size * 2) / 2));
	const left = Math.min(Math.max(0, run.box[0] - PADDING / page.width), 1 - 1 / page.width);
	const top = Math.min(
		Math.max(0, run.baseline - (capHeight(run.family, run.bold) * size + PADDING) / page.height),
		1 - 1 / page.height
	);
	return { size, left, top };
}

/// The run under a point, within `reach` of it, or undefined.
export function runAt(runs: TextRun[], x: number, y: number, reach = 0) {
	let [best, distance] = [undefined as TextRun | undefined, reach];
	for (const run of runs) {
		const [left, top, right, bottom] = run.box;
		const away = Math.hypot(Math.max(left - x, 0, x - right), Math.max(top - y, 0, y - bottom));
		if (away < distance || (away === 0 && !best)) [best, distance] = [run, away];
		if (away === 0) break;
	}
	return best;
}

/// Where in the run's text a click at `x` falls, for placing the caret.
export function caretAt(page: PageGlyphs, run: TextRun, x: number) {
	let characters = 0;
	let offset = run.text.length;
	let found = false;
	const repeats = overprints(page, run.glyphs);
	for (const [position, index] of run.glyphs.entries()) {
		if (repeats.has(position)) continue;
		const [left, , right] = glyphBox(page, index);
		const text = glyphText(page, index).normalize('NFKC');
		if (!found && x < (left + right) / 2) {
			offset = characters;
			found = true;
		}
		characters += isBlank(text) ? 0 : text.length;
	}
	// Spaces were added between words; count the ones before the caret.
	if (!found) return run.text.length;
	let seen = 0;
	for (let position = 0; position < run.text.length; position++) {
		if (seen === offset) return position;
		if (run.text[position] !== ' ') seen++;
	}
	return run.text.length;
}

// Five bits a channel is enough to tell paper from ink and still put
// antialiased edges with one or the other.
function commonColor(data: Uint8ClampedArray): [number, number, number] | undefined {
	const buckets = new Map<number, [number, number, number, number]>();
	for (let at = 0; at < data.length; at += 4) {
		const key = ((data[at] >> 3) << 10) | ((data[at + 1] >> 3) << 5) | (data[at + 2] >> 3);
		const bucket = buckets.get(key) ?? [0, 0, 0, 0];
		bucket[0] += data[at];
		bucket[1] += data[at + 1];
		bucket[2] += data[at + 2];
		bucket[3]++;
		buckets.set(key, bucket);
	}
	let common: [number, number, number, number] | undefined;
	for (const bucket of buckets.values()) if (!common || bucket[3] > common[3]) common = bucket;
	if (!common) return undefined;
	const count = common[3];
	return [
		Math.round(common[0] / count),
		Math.round(common[1] / count),
		Math.round(common[2] / count)
	];
}

const pack = ([red, green, blue]: number[]) => (red << 16) | (green << 8) | blue;

function context(canvas: HTMLCanvasElement | null | undefined) {
	if (!canvas?.width || !canvas.height) return undefined;
	return canvas.getContext('2d', { willReadFrequently: true }) ?? undefined;
}

function pixels(
	context: CanvasRenderingContext2D,
	x0: number,
	y0: number,
	x1: number,
	y1: number
): Uint8ClampedArray | undefined {
	if (x1 <= x0 || y1 <= y0) return undefined;
	try {
		return context.getImageData(x0, y0, x1 - x0, y1 - y0).data;
	} catch {
		return undefined;
	}
}

/// The colour most of the page shows just outside `area`, which is what an
/// erased area should be painted to disappear into the page; inside it when
/// the area leaves no page around it. White when the picture cannot be read.
export function colorAround(canvas: HTMLCanvasElement | null | undefined, area: CropArea) {
	const drawing = context(canvas);
	if (!canvas || !drawing) return 0xffffff;
	const [width, height] = [canvas.width, canvas.height];
	const x0 = Math.max(0, Math.floor(area[0] * width));
	const y0 = Math.max(0, Math.floor(area[1] * height));
	const x1 = Math.min(width, Math.ceil(area[2] * width));
	const y1 = Math.min(height, Math.ceil(area[3] * height));
	// A few pixels clear of the edge, past the antialiasing of whatever was
	// drawn there.
	const gap = 2;
	const band = Math.max(3, Math.round(Math.min(width, height) * 0.006));
	const strips = [
		[x0 - gap - band, y0 - gap - band, x1 + gap + band, y0 - gap],
		[x0 - gap - band, y1 + gap, x1 + gap + band, y1 + gap + band],
		[x0 - gap - band, y0 - gap, x0 - gap, y1 + gap],
		[x1 + gap, y0 - gap, x1 + gap + band, y1 + gap]
	].map(([left, top, right, bottom]) =>
		pixels(
			drawing,
			Math.max(0, left),
			Math.max(0, top),
			Math.min(width, right),
			Math.min(height, bottom)
		)
	);
	const found = strips.filter((strip) => strip !== undefined);
	const total = found.reduce((sum, strip) => sum + strip.length, 0);
	let data: Uint8ClampedArray | undefined;
	if (total) {
		data = new Uint8ClampedArray(total);
		let offset = 0;
		for (const strip of found) {
			data.set(strip, offset);
			offset += strip.length;
		}
	} else data = pixels(drawing, x0, y0, x1, y1);
	const common = data && commonColor(data);
	return common ? pack(common) : 0xffffff;
}

/// The colour most of a picture's pixels share around and under `area`,
/// which is what shows once the text over it is gone, and the colour that
/// stands out most from it, which is the text's. `canvas` shows the whole
/// page. White and black when the picture cannot be read.
export function sampleColors(canvas: HTMLCanvasElement | null | undefined, area: CropArea) {
	const fallback = { background: 0xffffff, text: 0x000000 };
	const drawing = context(canvas);
	if (!canvas || !drawing) return fallback;
	const [left, top, right, bottom] = area;
	const grow = (bottom - top) * 0.35;
	const x0 = Math.max(0, Math.floor((left - grow * (canvas.height / canvas.width)) * canvas.width));
	const y0 = Math.max(0, Math.floor((top - grow) * canvas.height));
	const x1 = Math.min(
		canvas.width,
		Math.ceil((right + grow * (canvas.height / canvas.width)) * canvas.width)
	);
	const y1 = Math.min(canvas.height, Math.ceil((bottom + grow) * canvas.height));
	const data = pixels(drawing, x0, y0, x1, y1);
	const paper = data && commonColor(data);
	if (!data || !paper) return fallback;
	let far = 0;
	const distances = new Float32Array(data.length / 4);
	for (let at = 0, pixel = 0; at < data.length; at += 4, pixel++) {
		const distance = Math.hypot(
			data[at] - paper[0],
			data[at + 1] - paper[1],
			data[at + 2] - paper[2]
		);
		distances[pixel] = distance;
		far = Math.max(far, distance);
	}
	const ink = [0, 0, 0];
	let count = 0;
	for (let pixel = 0; pixel < distances.length; pixel++) {
		if (far < 40 || distances[pixel] < far * 0.8) continue;
		ink[0] += data[pixel * 4];
		ink[1] += data[pixel * 4 + 1];
		ink[2] += data[pixel * 4 + 2];
		count++;
	}
	return {
		background: pack(paper),
		text: count ? pack(ink.map((sum) => Math.round(sum / count))) : fallback.text
	};
}
