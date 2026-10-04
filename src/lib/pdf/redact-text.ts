// The engine's glyphs, for finding text, picking a word by clicking it, and
// showing which glyphs a box removes. Coverage follows `geometry.rs`: a glyph
// goes once boxes cover a quarter of it, and one with no area goes when its
// centre is covered. Both sides measure in fractions of the page as shown,
// which keeps the shares the engine measures in page space.

import type { CropArea, PageGlyphs } from './types';

const COVERED = 0.25;

export function glyphBox(page: PageGlyphs, index: number): CropArea {
	const at = index * 4;
	return [page.boxes[at], page.boxes[at + 1], page.boxes[at + 2], page.boxes[at + 3]];
}

export function glyphText(page: PageGlyphs, index: number) {
	return page.text.slice(index ? page.ends[index - 1] : 0, page.ends[index]);
}

/// Whether `areas` cover at least `share` of the box.
export function covered([left, top, right, bottom]: CropArea, areas: CropArea[], share = COVERED) {
	const size = (right - left) * (bottom - top);
	if (size <= 1e-12) {
		const [x, y] = [(left + right) / 2, (top + bottom) / 2];
		return areas.some(([l, t, r, b]) => x >= l && x <= r && y >= t && y <= b);
	}
	let overlap = 0;
	for (const [l, t, r, b] of areas) {
		const width = Math.min(right, r) - Math.max(left, l);
		const height = Math.min(bottom, b) - Math.max(top, t);
		if (width > 0 && height > 0) overlap += width * height;
	}
	return overlap >= share * size;
}

/// The boxes of glyphs `areas` remove that reach outside them, which the
/// preview marks, since they disappear beyond the painted box.
export function spilledGlyphs(page: PageGlyphs, areas: CropArea[]): CropArea[] {
	if (!areas.length) return [];
	const spilled: CropArea[] = [];
	for (let index = 0; index < page.ends.length; index++) {
		const box = glyphBox(page, index);
		if (!covered(box, areas)) continue;
		const [left, top, right, bottom] = box;
		const inside = areas.some(([l, t, r, b]) => left >= l && right <= r && top >= t && bottom <= b);
		if (!inside) spilled.push(box);
	}
	return spilled;
}

export function sameLine(a: CropArea, b: CropArea) {
	const height = Math.max(a[3] - a[1], b[3] - b[1]);
	return Math.abs((a[1] + a[3]) / 2 - (b[1] + b[3]) / 2) < height / 2;
}

/// Whether a reader would see a word break between two glyphs on one line:
/// a gap wider than a third of the wider glyph.
export function apart(a: CropArea, b: CropArea) {
	return b[0] - a[2] > Math.max(a[2] - a[0], b[2] - b[0]) / 3;
}

export const isBlank = (text: string) => !text.trim();

export function union(boxes: CropArea[]): CropArea {
	return [
		Math.min(...boxes.map((box) => box[0])),
		Math.min(...boxes.map((box) => box[1])),
		Math.max(...boxes.map((box) => box[2])),
		Math.max(...boxes.map((box) => box[3]))
	];
}

/// One box per line the glyphs run across, leaving out blanks at the ends.
export function lineBoxes(page: PageGlyphs, glyphs: number[]): CropArea[] {
	const lines: CropArea[][] = [];
	for (const index of glyphs) {
		if (isBlank(glyphText(page, index))) continue;
		const box = glyphBox(page, index);
		const line = lines.find((boxes) => sameLine(boxes[0], box));
		if (line) line.push(box);
		else lines.push([box]);
	}
	return lines.map(union);
}

/// The word under a point, as boxes, or nothing when no glyph is there.
export function wordAt(page: PageGlyphs, x: number, y: number): CropArea[] {
	const count = page.ends.length;
	let hit = -1;
	for (let index = 0; index < count; index++) {
		const [left, top, right, bottom] = glyphBox(page, index);
		if (x >= left && x <= right && y >= top && y <= bottom && !isBlank(glyphText(page, index))) {
			hit = index;
			break;
		}
	}
	if (hit < 0) return [];
	const joined = (from: number, to: number) => {
		const [a, b] = [glyphBox(page, from), glyphBox(page, to)];
		return (
			!isBlank(glyphText(page, to)) &&
			sameLine(a, b) &&
			!apart(from < to ? a : b, from < to ? b : a)
		);
	};
	let [start, end] = [hit, hit];
	while (start > 0 && joined(start, start - 1)) start--;
	while (end < count - 1 && joined(end, end + 1)) end++;
	return lineBoxes(
		page,
		Array.from({ length: end - start + 1 }, (_, offset) => start + offset)
	);
}

// Built once per page, not on every keystroke.
const searchables = new WeakMap<PageGlyphs, { text: string; owners: number[] }>();

/// A page's text as a reader would copy it, lower case and with ligatures
/// spelled out, and the glyph each character came from.
function searchable(page: PageGlyphs) {
	const cached = searchables.get(page);
	if (cached) return cached;
	let text = '';
	const owners: number[] = [];
	const add = (characters: string, owner: number) => {
		for (const character of characters) {
			const blank = isBlank(character);
			if (blank && (text === '' || text.endsWith(' '))) continue;
			text += blank ? ' ' : character;
			for (let unit = 0; unit < (blank ? 1 : character.length); unit++) owners.push(owner);
		}
	};
	for (let index = 0; index < page.ends.length; index++) {
		if (index > 0) {
			const [previous, box] = [glyphBox(page, index - 1), glyphBox(page, index)];
			if (!sameLine(previous, box) || apart(previous, box)) add(' ', -1);
		}
		add(glyphText(page, index).normalize('NFKC').toLowerCase(), index);
	}
	const built = { text, owners };
	searchables.set(page, built);
	return built;
}

export type TextMatch = { page: number; boxes: CropArea[] };

/// Every place `query` appears, ignoring case and runs of white space,
/// across lines too, with one box per line it covers.
export function findText(pages: PageGlyphs[], query: string): TextMatch[] {
	const needle = query.normalize('NFKC').toLowerCase().trim().replace(/\s+/g, ' ');
	if (!needle) return [];
	const matches: TextMatch[] = [];
	pages.forEach((page, index) => {
		const { text, owners } = searchable(page);
		let from = 0;
		for (;;) {
			const at = text.indexOf(needle, from);
			if (at < 0) break;
			const glyphs = [
				...new Set(owners.slice(at, at + needle.length).filter((owner) => owner >= 0))
			];
			const boxes = lineBoxes(page, glyphs);
			if (boxes.length) matches.push({ page: index + 1, boxes });
			from = at + needle.length;
		}
	});
	return matches;
}
