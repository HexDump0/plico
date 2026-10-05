// What Translate PDF reads and writes on a page. Edit's runs of text are
// gathered into paragraphs, since a sentence is translated whole and a line
// is rarely a sentence; each paragraph's translation is then set where it
// was, at its size when it fits the room the paragraph has and smaller when
// it does not, and the old text is taken out as Edit takes it out.

import { LEADING, TEXT_PADDING, wrapText } from './annotate';
import { textRuns, type TextRun } from './edit-text';
import { capHeight, type FontFamily } from './standard-fonts';
import { needsEmbedding } from './unicode-fonts';
import type { CropArea, PageAnnotation, PageGlyphs, TextAlign, TextRemoval } from './types';

/// A paragraph, or a heading, a label or a table cell: lines read as one.
export type TranslateBlock = {
	/// The page and the block's place on it, which stays put while the
	/// page's text does.
	id: string;
	page: number;
	runs: TextRun[];
	text: string;
	family: FontFamily;
	bold: boolean;
	/// In points.
	size: number;
	/// 0xRRGGBB.
	color: number;
	align: TextAlign;
	/// The first line's baseline, as a fraction of the page's height.
	baseline: number;
	/// The union of its lines, as fractions of the page.
	box: CropArea;
	/// How far the translation may reach, as fractions: as wide as the
	/// paragraph, or for a line on its own as far as the next thing beside
	/// it; down to the next thing below it.
	room: CropArea;
};

/// A block's translation as it is set: the text box the engine draws and
/// whether it had to go past the block's room.
export type SetBlock = {
	area: CropArea;
	size: number;
	align: TextAlign;
	overflows: boolean;
};

type Line = TextRun & { x0: number; x1: number; y: number; top: number; bottom: number };

// A line is in a paragraph when its baseline is this far below the line
// before, in ems of the paragraph's size...
const NEAREST = 0.6;
const FURTHEST = 1.85;
// ...its spacing within this share of the paragraph's spacing so far...
const STEADY = 0.25;
// ...and its size within this share of the paragraph's.
const SAME_SIZE = 0.12;
// Translations shrink no further than this share of the original size before
// they are let past their room.
const SMALLEST_SHARE = 0.6;
// Text further than this many points from the page's text is margin.
const MARGIN = 18;

const letters = /\p{L}.*\p{L}/su;
// Addresses, links and code, which say the same in any language.
const verbatim = /\S*(?:[@{}<>\\/_=]|\.\w{2,}\b)\S*/gu;
const bullet = /^(?:[•▪◦‣●○■□➢►–—-]\s|\(?(?:\d{1,2}|[a-z]|[ivx]{1,4})[.)]\s)/u;
const ends = /[.!?:;。！？]["'”’)\]]?$/u;
const closeUp = /[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Thai}]/u;

function most<T>(values: [T, number][]): T {
	const weights = new Map<T, number>();
	let [best, weight] = [values[0][0], -1];
	for (const [value, by] of values) {
		const next = (weights.get(value) ?? 0) + by;
		weights.set(value, next);
		if (next > weight) [best, weight] = [value, next];
	}
	return best;
}

function median(values: number[]) {
	const sorted = [...values].sort((a, b) => a - b);
	return sorted[Math.floor(sorted.length / 2)];
}

const spread = (values: number[]) =>
	values.length ? Math.max(...values) - Math.min(...values) : 0;

/// Lines joined into the paragraph they print: a word broken over a line
/// end is joined again, and lines of Chinese, Japanese or Thai run on
/// without a space.
function joined(lines: string[]) {
	let text = '';
	for (const line of lines) {
		if (!text) text = line;
		else if (/\p{Ll}[-\u00ad]$/u.test(text) && /^\p{Ll}/u.test(line))
			text = text.slice(0, -1) + line;
		else if (closeUp.test(text.at(-1) ?? '') && closeUp.test(line[0])) text += line;
		else text += ` ${line}`;
	}
	return text;
}

/// Whether `line` carries on paragraph `block`.
function continues(block: Line[], line: Line) {
	const last = block[block.length - 1];
	const size = median(block.map((entry) => entry.size));
	const step = line.y - last.y;
	if (step < NEAREST * size || step > FURTHEST * size) return false;
	if (Math.abs(line.size - size) > SAME_SIZE * size) return false;
	if (block.length > 1) {
		const pitch = block[1].y - block[0].y;
		if (Math.abs(step - pitch) > STEADY * pitch) return false;
	}
	if (bullet.test(line.text)) return false;
	const [x0, x1] = [
		Math.min(...block.map((entry) => entry.x0)),
		Math.max(...block.map((entry) => entry.x1))
	];
	const overlap = Math.min(line.x1, x1) - Math.max(line.x0, x0);
	const aligned = Math.abs(line.x0 - x0) <= 1.5 * size;
	if (!aligned && overlap < 0.5 * Math.min(line.x1 - line.x0, x1 - x0)) return false;
	// A bold line is a heading unless a sentence runs on through it, as it
	// does after a run-in label set in bold.
	const runsOn = /[-\u00ad,]$/u.test(last.text) || !ends.test(last.text);
	const long = (entry: Line) => entry.x1 - entry.x0 >= 0.7 * (x1 - x0);
	if (line.bold !== last.bold && !(runsOn && long(last) && (long(line) || block.length > 1)))
		return false;
	// After a full stop, a line set in from the paragraph's edge starts a
	// paragraph of its own, and so does any line after one that stops short.
	if (ends.test(last.text) && Math.abs(last.x0 - x0) <= 0.5 * size) {
		if (line.x0 - x0 > 0.6 * size) return false;
		if (block.length > 1 && last.x1 < x1 - 3 * size) return false;
	}
	return true;
}

/// Left, centred or set against the right, as its lines show.
function alignment(lines: Line[], width: number): TextAlign {
	const size = median(lines.map((line) => line.size));
	const [lefts, rights] = [lines.map((line) => line.x0), lines.map((line) => line.x1)];
	if (lines.length > 1) {
		if (spread(lefts) <= 0.5 * size) return 'left';
		if (spread(lines.map((line) => (line.x0 + line.x1) / 2)) <= 0.5 * size) return 'center';
		// Further in than a first line's indent.
		if (spread(rights) <= 0.5 * size && spread(lefts) > 2 * size) return 'right';
		return 'left';
	}
	const [line] = lines;
	const middle = (line.x0 + line.x1) / 2;
	if (Math.abs(middle - width / 2) <= 0.02 * width && line.x0 > 0.15 * width) return 'center';
	if (line.x0 > width / 2 && line.x1 > 0.85 * width) return 'right';
	return 'left';
}

/// The paragraphs on a page worth translating: lines with at least two
/// letters, so page numbers, figures and formulas stay as they are.
export function pageBlocks(
	glyphs: PageGlyphs,
	page: number,
	width: number,
	height: number
): TranslateBlock[] {
	const all: Line[] = textRuns(glyphs, width, height).map((run) => ({
		...run,
		x0: run.box[0] * width,
		x1: run.box[2] * width,
		y: run.baseline * height,
		top: run.box[1] * height,
		bottom: run.box[3] * height
	}));
	const lines = all
		.filter((line) => letters.test(line.text.replace(verbatim, '')))
		.sort((a, b) => a.y - b.y || a.x0 - b.x0);
	const groups: Line[][] = [];
	for (const line of lines) {
		// The nearest paragraph above it that it carries on.
		let best: Line[] | undefined;
		for (const group of groups)
			if (continues(group, line) && (!best || group[group.length - 1].y > best[best.length - 1].y))
				best = group;
		if (best) best.push(line);
		else groups.push([line]);
	}
	// What the page draws, to keep translations clear of it.
	const drawn: { box: CropArea; line?: Line }[] = [
		...all.map((line) => ({ box: [line.x0, line.top, line.x1, line.bottom] as CropArea, line })),
		...Array.from({ length: glyphs.images.length / 4 }, (_, index) => ({
			box: [
				glyphs.images[index * 4] * width,
				glyphs.images[index * 4 + 1] * height,
				glyphs.images[index * 4 + 2] * width,
				glyphs.images[index * 4 + 3] * height
			] as CropArea
		}))
	];
	const content: CropArea = all.length
		? [
				Math.min(...all.map((line) => line.x0)),
				Math.min(...all.map((line) => line.top)),
				Math.max(...all.map((line) => line.x1)),
				Math.max(...all.map((line) => line.bottom))
			]
		: [0, 0, width, height];
	const right = Math.max(content[2], width - MARGIN);
	const left = Math.min(content[0], MARGIN);
	const bottom = Math.max(content[3], height - MARGIN);

	return groups.map((group, index) => {
		const size = median(group.map((line) => line.size));
		const box: CropArea = [
			Math.min(...group.map((line) => line.x0)),
			Math.min(...group.map((line) => line.top)),
			Math.max(...group.map((line) => line.x1)),
			Math.max(...group.map((line) => line.bottom))
		];
		const align = alignment(group, width);
		const others = drawn.flatMap((other) =>
			other.line && group.includes(other.line) ? [] : [other.box]
		);
		const [x0, top, x1] = box;
		const last = group[group.length - 1];
		// Down to whatever is below it across its width.
		let floor = bottom;
		for (const [ox0, oy0, ox1] of others)
			if (ox1 > x0 && ox0 < x1 && oy0 > last.y) floor = Math.min(floor, oy0 - 0.15 * size);
		floor = Math.max(floor, box[3]);
		// A line alone may also reach sideways, as far as whatever is beside it.
		let [reachLeft, reachRight] = [x0, x1];
		if (group.length === 1) {
			let [wall0, wall1] = [left, right];
			for (const [ox0, oy0, ox1, oy1] of others)
				if (oy1 > top && oy0 < box[3]) {
					if (ox0 >= x1) wall1 = Math.min(wall1, ox0 - 0.5 * size);
					if (ox1 <= x0) wall0 = Math.max(wall0, ox1 + 0.5 * size);
				}
			if (align === 'left') reachRight = Math.max(x1, wall1);
			else if (align === 'right') reachLeft = Math.min(x0, wall0);
			else {
				const half = Math.max(
					(x1 - x0) / 2,
					Math.min((x0 + x1) / 2 - wall0, wall1 - (x0 + x1) / 2)
				);
				[reachLeft, reachRight] = [(x0 + x1) / 2 - half, (x0 + x1) / 2 + half];
			}
		}
		const family = most(group.map((line): [FontFamily, number] => [line.family, line.text.length]));
		const colors = group.flatMap((line): [number, number][] =>
			line.color === undefined ? [] : [[line.color, line.text.length]]
		);
		return {
			id: `${page}:${index}`,
			page,
			runs: group,
			text: joined(group.map((line) => line.text)),
			family,
			bold: most(group.map((line): [boolean, number] => [line.bold, line.text.length])),
			size,
			color: colors.length ? most(colors) : 0,
			align,
			baseline: group[0].baseline,
			box: [box[0] / width, box[1] / height, box[2] / width, box[3] / height],
			room: [reachLeft / width, top / height, reachRight / width, floor / height]
		};
	});
}

/// Where and how big `text` is set for `block`: at the block's size where
/// it fits its room, smaller where it does not, and past the room only at
/// the smallest size. `rtl` sets left-aligned text against the right.
export function setBlock(
	block: TranslateBlock,
	text: string,
	page: { width: number; height: number },
	rtl = false,
	largest = block.size
): SetBlock {
	const [x0, , x1, floor] = [
		block.room[0] * page.width,
		block.room[1] * page.height,
		block.room[2] * page.width,
		block.room[3] * page.height
	];
	const width = x1 - x0;
	const baseline = block.baseline * page.height;
	const embedded = needsEmbedding(text);
	const lines = (size: number) => wrapText(text, block.family, block.bold, size, width).length;
	const depth = (size: number, count: number) =>
		baseline + (count - 1) * LEADING * size + 0.25 * size;
	let size = Math.floor(Math.min(block.size, largest) * 4) / 4;
	let count = lines(size);
	while (depth(size, count) > floor && size > block.size * SMALLEST_SHARE) {
		size = Math.max(block.size * SMALLEST_SHARE, size - 0.25);
		count = lines(size);
	}
	const overflows = depth(size, count) > floor;
	const top = baseline - capHeight(block.family, block.bold, embedded) * size - TEXT_PADDING;
	// A line's room below for the engine's own measure, which can wrap a
	// little sooner than this one; the box only clips, so it shows nothing.
	const down = depth(size, count) + LEADING * size;
	return {
		area: [
			(x0 - TEXT_PADDING) / page.width,
			Math.max(0, top) / page.height,
			(x1 + TEXT_PADDING) / page.width,
			Math.min(page.height, down) / page.height
		],
		size,
		align: rtl && block.align === 'left' ? 'right' : block.align,
		overflows
	};
}

/// Each block of a page set as `setBlock` sets it, except that paragraphs
/// set alike in the original stay alike: none is larger than most of its
/// kind could be. `texts` holds each block's translation, or undefined for
/// those left alone.
export function setPage(
	blocks: TranslateBlock[],
	texts: (string | undefined)[],
	page: { width: number; height: number },
	rtl = false
): (SetBlock | undefined)[] {
	const sets = blocks.map((block, index) => {
		const text = texts[index];
		return text === undefined ? undefined : setBlock(block, text, page, rtl);
	});
	const kind = (block: TranslateBlock) =>
		`${block.family} ${block.bold} ${Math.round(block.size * 2)}`;
	const shares = new Map<string, number[]>();
	blocks.forEach((block, index) => {
		const set = sets[index];
		if (!set) return;
		const list = shares.get(kind(block)) ?? [];
		list.push(set.size / block.size);
		shares.set(kind(block), list);
	});
	return blocks.map((block, index) => {
		const set = sets[index];
		const list = shares.get(kind(block)) ?? [];
		if (!set || list.length < 2) return set;
		const share = median(list);
		return set.size > block.size * share + 0.01
			? setBlock(block, texts[index]!, page, rtl, block.size * share)
			: set;
	});
}

/// The text box the engine draws a translation in.
export function translationMark(
	block: TranslateBlock,
	text: string,
	set: SetBlock
): PageAnnotation {
	return {
		kind: 'text',
		page: block.page,
		area: set.area,
		text,
		family: block.family,
		bold: block.bold,
		size: set.size,
		fill: null,
		align: set.align,
		color: block.color,
		opacity: 1,
		comment: ''
	};
}

/// What the engine takes out for the block's old text, painting over in
/// `cover` where it cannot.
export function blockRemovals(block: TranslateBlock, cover: number): TextRemoval[] {
	return block.runs.map((run) => ({ page: block.page, area: run.band, shown: run.ink, cover }));
}

/// Whether a translation says the same as the original, as a name or a
/// line already in the target language does; such blocks are left alone.
export function unchanged(original: string, translated: string) {
	const plain = (text: string) => text.normalize('NFKC').replace(/\s+/g, ' ').trim().toLowerCase();
	return !translated.trim() || plain(original) === plain(translated);
}
