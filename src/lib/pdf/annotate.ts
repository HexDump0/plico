// Geometry and appearance for the Annotate preview, mirroring `Sketch::draw`
// and `wrap` in `rust/plico-engine/src/annotate.rs`, so what the overlay draws
// is what the engine writes. Geometry is in fractions of the page as
// displayed, from its top left; drawing is in points on that page, y down.

import { glyphBox, glyphText, isBlank, lineBoxes } from './redact-text';
import { capHeight, textWidth, type FontFamily } from './standard-fonts';
import { needsEmbedding } from './unicode-fonts';
import type { PreviewPage } from './stamp-layout';
import type { AnnotationMark, CropArea, PageAnnotation, PageGlyphs, PagePoint } from './types';

export const TEXT_PADDING = 2;
export const NOTE_SIZE = 20;
export const LEADING = 1.2;

export type AnnotateTool =
	'select' | 'markup' | 'ink' | 'shape' | 'text' | 'note' | 'image' | 'erase';
/// What the sidebar can show settings for: a tool's marks, or an image the
/// page already draws, picked with Select.
export type StyleGroup = Exclude<AnnotateTool, 'select'> | 'picture';
export type MarkupKind = 'highlight' | 'underline' | 'strikeout' | 'squiggly';
export type ShapeKind = 'rectangle' | 'ellipse' | 'line' | 'arrow';

/// An image the page already draws, picked to move, resize or take away:
/// `from` is where the PDF draws it, `area` where it goes, `snapshot` how it
/// looks, copied from the page as drawn, and `cover` the colour around it,
/// which the preview paints where it was.
export type Picture = {
	kind: 'picture';
	area: CropArea;
	from: CropArea;
	snapshot: string;
	cover: number;
	deleted: boolean;
};

/// What the overlay draws: an annotation, an area Edit erases, or an image
/// the page draws, moved.
export type Drawn = AnnotationMark | { kind: 'erase'; area: CropArea } | Picture;

/// Text an edit replaces: the band the engine takes its glyphs out of, where
/// its ink reached, the colour behind it, and the text box as it first
/// matched it, to tell whether anything changed.
export type Replaced = {
	band: CropArea;
	ink: CropArea;
	cover: number;
	original: {
		text: string;
		area: CropArea;
		family: FontFamily;
		bold: boolean;
		size: number;
		color: number;
	};
};

/// An area Edit empties, painted in `color`: `matched`, the colour of the
/// page around it, while `match` holds.
export type Erased = {
	kind: 'erase';
	area: CropArea;
	page: number;
	color: number;
	opacity: number;
	comment: string;
	match: boolean;
	matched: number;
};

/// A mark as the tool edits it. A text box with `fit` grows sideways as it
/// is typed in rather than wrapping; one with `replaces` stands in for text
/// the page already has.
export type AnnotateMark = (
	| PageAnnotation
	| Erased
	| (Picture & { page: number; color: number; opacity: number; comment: string })
) & {
	id: number;
	fit?: boolean;
	replaces?: Replaced;
};

/// What a gesture on the page made, before it becomes an annotation.
export type Geometry =
	| { kind: 'boxes'; boxes: CropArea[] }
	| { kind: 'stroke'; points: PagePoint[] }
	| { kind: 'area'; area: CropArea }
	| { kind: 'segment'; from: PagePoint; to: PagePoint }
	| { kind: 'point'; at: PagePoint };

export function groupOf(kind: Drawn['kind']): StyleGroup {
	switch (kind) {
		case 'highlight':
		case 'underline':
		case 'strikeout':
		case 'squiggly':
			return 'markup';
		case 'rectangle':
		case 'ellipse':
		case 'line':
		case 'arrow':
			return 'shape';
		default:
			return kind;
	}
}

export const hexColor = (color: number) => `#${color.toString(16).padStart(6, '0')}`;
export const colorNumber = (hex: string) => Number.parseInt(hex.slice(1), 16);

const clamp = (value: number, low: number, high: number) => Math.min(high, Math.max(low, value));

function union(boxes: CropArea[]): CropArea {
	return [
		Math.min(...boxes.map((box) => box[0])),
		Math.min(...boxes.map((box) => box[1])),
		Math.max(...boxes.map((box) => box[2])),
		Math.max(...boxes.map((box) => box[3]))
	];
}

function spread(points: PagePoint[]): CropArea {
	return union(points.map(([x, y]) => [x, y, x, y]));
}

/// An image's height as a fraction of the page's, for its width fraction.
export function imageHeight(width: number, aspect: number, page: PreviewPage) {
	return (width * aspect * page.width) / page.height;
}

/// What the geometry spans, as the engine places it: notes and images are
/// pulled back onto the page there, so they are here too.
export function extent(mark: Drawn, page: PreviewPage, aspect = 1): CropArea {
	switch (mark.kind) {
		case 'ink':
			return spread(mark.strokes.flat());
		case 'line':
		case 'arrow':
			return spread([mark.from, mark.to]);
		case 'rectangle':
		case 'ellipse':
		case 'text':
		case 'erase':
		case 'picture':
			return mark.area;
		case 'note': {
			const [width, height] = [NOTE_SIZE / page.width, NOTE_SIZE / page.height];
			const left = clamp(mark.at[0], 0, Math.max(0, 1 - width));
			const top = clamp(mark.at[1], 0, Math.max(0, 1 - height));
			return [left, top, left + width, top + height];
		}
		case 'image': {
			const [left, top, width] = mark.place;
			const height = imageHeight(width, aspect, page);
			const x = clamp(left, 0, Math.max(0, 1 - width));
			const y = clamp(top, 0, Math.max(0, 1 - height));
			return [x, y, x + width, y + height];
		}
		default:
			return union(mark.boxes);
	}
}

/// The extent with the stroke around it, which is what can be seen and
/// picked.
export function bounds(mark: Drawn, page: PreviewPage, aspect = 1): CropArea {
	const [left, top, right, bottom] = extent(mark, page, aspect);
	const reach =
		mark.kind === 'arrow'
			? mark.width * 3 + 4
			: mark.kind === 'ink' || mark.kind === 'line'
				? mark.width / 2
				: 0;
	return [
		Math.max(0, left - reach / page.width),
		Math.max(0, top - reach / page.height),
		Math.min(1, right + reach / page.width),
		Math.min(1, bottom + reach / page.height)
	];
}

/// Every coordinate taken from one box to another, the way a resize or a
/// move carries the geometry along. An axis with no size only moves.
export function refit<T extends Drawn>(mark: T, from: CropArea, to: CropArea): T {
	const axis = (low: number, high: number, newLow: number, newHigh: number) => {
		const size = high - low;
		const scale = size > 1e-9 ? (newHigh - newLow) / size : 1;
		return (value: number) => clamp(newLow + (value - low) * scale, 0, 1);
	};
	const x = axis(from[0], from[2], to[0], to[2]);
	const y = axis(from[1], from[3], to[1], to[3]);
	const point = ([px, py]: PagePoint): PagePoint => [x(px), y(py)];
	const box = ([l, t, r, b]: CropArea): CropArea => [x(l), y(t), x(r), y(b)];
	switch (mark.kind) {
		case 'ink':
			return { ...mark, strokes: mark.strokes.map((stroke) => stroke.map(point)) };
		case 'line':
		case 'arrow':
			return { ...mark, from: point(mark.from), to: point(mark.to) };
		case 'rectangle':
		case 'ellipse':
		case 'text':
		case 'erase':
		case 'picture':
			return { ...mark, area: box(mark.area) };
		case 'note':
			return { ...mark, at: point(mark.at) };
		case 'image':
			return { ...mark, place: [to[0], to[1], to[2] - to[0]] };
		default:
			return { ...mark, boxes: mark.boxes.map(box) };
	}
}

/// Moved by a fraction of the page, kept wholly on it.
export function translate<T extends Drawn>(
	mark: T,
	dx: number,
	dy: number,
	page: PreviewPage,
	aspect = 1
): T {
	const from = extent(mark, page, aspect);
	const x = clamp(dx, -from[0], 1 - from[2]);
	const y = clamp(dy, -from[1], 1 - from[3]);
	return refit(mark, from, [from[0] + x, from[1] + y, from[2] + x, from[3] + y]);
}

/// Grown or shrunk about its centre, kept on the page.
export function scaled<T extends Drawn>(mark: T, factor: number, page: PreviewPage, aspect = 1): T {
	const from = extent(mark, page, aspect);
	const [cx, cy] = [(from[0] + from[2]) / 2, (from[1] + from[3]) / 2];
	let [halfWidth, halfHeight] = [
		((from[2] - from[0]) / 2) * factor,
		((from[3] - from[1]) / 2) * factor
	];
	const fits = Math.min(
		halfWidth > 0 ? Math.min(cx, 1 - cx) / halfWidth : Infinity,
		halfHeight > 0 ? Math.min(cy, 1 - cy) / halfHeight : Infinity,
		1
	);
	if (factor > 1 && fits < 1) return mark;
	[halfWidth, halfHeight] = [halfWidth * fits, halfHeight * fits];
	return refit(mark, from, [cx - halfWidth, cy - halfHeight, cx + halfWidth, cy + halfHeight]);
}

// Scripts written without spaces between words, as `breaks_anywhere` in
// `annotate.rs` lists them.
const breaksAnywhere =
	/[\u0e00-\u0e7f\u3040-\u30ff\u3400-\u4dbf\u4e00-\u9fff\uf900-\ufaff\u3000-\u303f\uff00-\uffef]/u;

/// Lines as `annotate::wrap` breaks them: greedily at spaces, and inside a
/// word only when the word alone is wider than the box, or when it is
/// Chinese, Japanese or Thai, which fills the line it starts on.
export function wrapText(
	text: string,
	family: FontFamily,
	bold: boolean,
	size: number,
	width: number
): string[] {
	const embedded = needsEmbedding(text);
	const measure = (line: string) => textWidth(line, family, bold, size, embedded);
	const lines: string[] = [];
	for (const paragraph of text.split('\n')) {
		let line = '';
		for (const word of paragraph.replace(/\r$/, '').split(' ')) {
			const candidate = line ? `${line} ${word}` : word;
			if (measure(candidate) <= width) {
				line = candidate;
				continue;
			}
			if (line) {
				if (breaksAnywhere.test(word) && measure(`${line} `) < width) line += ' ';
				else {
					lines.push(line);
					line = '';
				}
			}
			for (const character of word) {
				const longer = line + character;
				if (measure(longer) > width && line) {
					lines.push(line);
					line = character;
				} else line = longer;
			}
		}
		lines.push(line);
	}
	return lines;
}

/// Each line's baseline in points from the page's top left, as the engine
/// sets them; lines that start below the box are left out.
export function textLines(mark: Extract<AnnotationMark, { kind: 'text' }>, page: PreviewPage) {
	const [left, top, right, bottom] = [
		mark.area[0] * page.width,
		mark.area[1] * page.height,
		mark.area[2] * page.width,
		mark.area[3] * page.height
	];
	const lines = wrapText(
		mark.text,
		mark.family,
		mark.bold,
		mark.size,
		right - left - 2 * TEXT_PADDING
	);
	const embedded = needsEmbedding(mark.text);
	const first = top + TEXT_PADDING + capHeight(mark.family, mark.bold, embedded) * mark.size;
	const placed: { text: string; x: number; y: number; width: number }[] = [];
	for (const [index, text] of lines.entries()) {
		const y = first + index * LEADING * mark.size;
		if (y > bottom + mark.size) break;
		const width = textWidth(text, mark.family, mark.bold, mark.size, embedded);
		const room = right - left - 2 * TEXT_PADDING - width;
		const shift = mark.align === 'center' ? room / 2 : mark.align === 'right' ? room : 0;
		placed.push({ text, x: left + TEXT_PADDING + shift, y, width });
	}
	return placed;
}

/// The height in points a text box needs to show every line.
export function textHeight(
	text: string,
	family: FontFamily,
	bold: boolean,
	size: number,
	width: number
) {
	const count = wrapText(text, family, bold, size, width - 2 * TEXT_PADDING).length;
	const cap = capHeight(family, bold, needsEmbedding(text));
	return 2 * TEXT_PADDING + cap * size + (count - 1) * LEADING * size + 0.25 * size;
}

/// Where capital letters reach above the baseline, in ems.
export const capHeightOf = (family: FontFamily, bold: boolean) => capHeight(family, bold);

/// The width in points a text box needs to keep each of its lines whole.
export function textFitWidth(text: string, family: FontFamily, bold: boolean, size: number) {
	const embedded = needsEmbedding(text);
	const widest = Math.max(
		0,
		...text
			.split('\n')
			.map((line) => textWidth(line.replace(/\r$/, ''), family, bold, size, embedded))
	);
	// A little over, so rounding never wraps the last word.
	return widest + 2 * TEXT_PADDING + 0.5;
}

const round = (value: number) => Math.round(value * 1000) / 1000;

/// A stroke drawn through the midpoints between its points, as `smooth`
/// draws it, in points.
export function inkPath(points: [number, number][]) {
	const at = ([x, y]: [number, number]) => `${round(x)} ${round(y)}`;
	let path = `M${at(points[0])}`;
	if (points.length <= 2) return `${path}L${at(points[points.length - 1])}`;
	for (let index = 1; index < points.length - 1; index++) {
		const [control, next] = [points[index], points[index + 1]];
		path += `Q${at(control)} ${at([(control[0] + next[0]) / 2, (control[1] + next[1]) / 2])}`;
	}
	return `${path}L${at(points[points.length - 1])}`;
}

/// The two strokes of an open arrow's head at `to`, thirty degrees either
/// side of the line, in points.
export function arrowHead(from: [number, number], to: [number, number], width: number) {
	const length = Math.max(Math.hypot(to[0] - from[0], to[1] - from[1]), Number.EPSILON);
	const [ux, uy] = [(to[0] - from[0]) / length, (to[1] - from[1]) / length];
	const size = width * 3 + 4;
	const [sin, cos] = [Math.sin(Math.PI / 6), Math.cos(Math.PI / 6)];
	return [1, -1].map((side): [number, number] => [
		to[0] - (ux * cos - side * uy * sin) * size,
		to[1] - (side * ux * sin + uy * cos) * size
	]);
}

/// The line a markup style draws along one box, in points: under it, through
/// it, or a zigzag under it.
export function markupStroke(kind: MarkupKind, [left, top, right, bottom]: CropArea) {
	const height = bottom - top;
	const thickness = Math.max(height / 16, 0.5);
	if (kind === 'underline' || kind === 'strikeout') {
		const y = kind === 'underline' ? bottom - thickness : bottom - height * 0.45;
		return { path: `M${round(left)} ${round(y)}H${round(right)}`, thickness };
	}
	const step = Math.max(height / 6, 1);
	const [low, high] = [bottom - thickness, bottom - thickness - step];
	let path = `M${round(left)} ${round(low)}`;
	let [x, up] = [left, true];
	while (x < right) {
		x = Math.min(x + step, right);
		path += `L${round(x)} ${round(up ? high : low)}`;
		up = !up;
	}
	return { path, thickness };
}

/// The note icon's outline and its two text lines, in its own 20 point box,
/// y down.
export const notePolygon = [
	[2, 18],
	[18, 18],
	[18, 6],
	[9, 6],
	[5, 2],
	[6, 6],
	[2, 6]
]
	.map(([x, y]) => `${x},${NOTE_SIZE - y}`)
	.join(' ');
export const noteLines = [
	[5, 14, 15, 14],
	[5, 10, 12, 10]
].map(([x1, y1, x2, y2]) => [x1, NOTE_SIZE - y1, x2, NOTE_SIZE - y2]);

/// The glyph nearest a point, among those within `reach` of it, or -1.
export function nearestGlyph(page: PageGlyphs, x: number, y: number, reach: number) {
	let [best, distance] = [-1, reach];
	for (let index = 0; index < page.ends.length; index++) {
		if (isBlank(glyphText(page, index))) continue;
		const [left, top, right, bottom] = glyphBox(page, index);
		const away = Math.hypot(Math.max(left - x, 0, x - right), Math.max(top - y, 0, y - bottom));
		if (away < distance || (away === 0 && best < 0)) [best, distance] = [index, away];
		if (away === 0) break;
	}
	return best;
}

/// The text from one glyph to another in the order the page draws it, one
/// box per line, the way a reader's text selection runs.
export function textBetween(page: PageGlyphs, from: number, to: number): CropArea[] {
	const [start, end] = from <= to ? [from, to] : [to, from];
	return lineBoxes(
		page,
		Array.from({ length: end - start + 1 }, (_, offset) => start + offset)
	);
}
