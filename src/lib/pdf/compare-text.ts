// Compares two PDFs' text as the engine reads it (`redaction_text`), word by
// word. Lines are matched first and only the words of unmatched lines are
// compared, which keeps the diff small when a paragraph reflows. Positions are
// fractions of the page as shown, like `CropArea`.

import type { CropArea, PageAnnotation, PageGlyphs } from './types';
import { apart, glyphBox, glyphText, isBlank, lineBoxes, sameLine, union } from './redact-text';

export type CompareSide = 'original' | 'changed';

type Word = { page: number; line: number; key: string; boxes: CropArea[] };

/// Where a change sits on one page of one side.
export type ChangePlace = { page: number; boxes: CropArea[] };

/// One run of words that differs. A side with no words has a caret instead:
/// where the other side's words would be, as a zero-width box.
export type Change = {
	id: number;
	removed: string;
	added: string;
	original: ChangePlace[];
	changed: ChangePlace[];
	caret: { side: CompareSide; page: number; box: CropArea } | null;
};

/// The same word on both sides, as page and height on the page, for keeping
/// the two views level while they scroll.
export type Anchor = { original: [number, number]; changed: [number, number] };

export type Comparison = {
	changes: Change[];
	anchors: Anchor[];
	/// For each changed page, from 0, the original page most of its text came
	/// from, from 1.
	pairs: number[];
};

// Edits past these give up on matching that stretch and call it all changed;
// the work grows with their square.
const LINE_LIMIT = 2000;
const WORD_LIMIT = 2000;

function words(pages: PageGlyphs[]): Word[] {
	const found: Word[] = [];
	let line = -1;
	pages.forEach((page, index) => {
		let glyphs: number[] = [];
		let key = '';
		let previous: CropArea | null = null;
		const close = () => {
			if (glyphs.length && key)
				found.push({ page: index + 1, line, key, boxes: lineBoxes(page, glyphs) });
			glyphs = [];
			key = '';
		};
		for (let glyph = 0; glyph < page.ends.length; glyph++) {
			const text = glyphText(page, glyph);
			const box = glyphBox(page, glyph);
			if (!previous || !sameLine(previous, box)) {
				close();
				line++;
			} else if (apart(previous, box)) close();
			previous = box;
			if (isBlank(text)) {
				close();
				continue;
			}
			glyphs.push(glyph);
			key += text.normalize('NFKC');
		}
		close();
	});
	return dehyphenate(found);
}

/// A word broken across lines reads as one, so a reflow that moves the break
/// is not a change.
function dehyphenate(list: Word[]) {
	const joined: Word[] = [];
	for (let index = 0; index < list.length; index++) {
		const word = list[index];
		const next = list[index + 1];
		if (
			next &&
			next.line !== word.line &&
			/[\p{L}][-­]$/u.test(word.key) &&
			/^\p{Ll}/u.test(next.key)
		) {
			joined.push({
				...word,
				key: word.key.slice(0, -1) + next.key,
				boxes: [...word.boxes, ...next.boxes]
			});
			index++;
		} else joined.push(word);
	}
	return joined;
}

/// Myers' diff: the pairs of equal positions in one shortest edit, or null
/// past `limit` edits.
function myers(a: Int32Array, b: Int32Array, limit: number): [number, number][] | null {
	const [n, m] = [a.length, b.length];
	if (n === 0 || m === 0) return [];
	const max = Math.min(n + m, limit);
	const offset = max + 1;
	const v = new Int32Array(2 * offset + 1);
	const trace: Int32Array[] = [];
	for (let d = 0; d <= max; d++) {
		trace.push(v.slice(offset - d - 1, offset + d + 2));
		for (let k = -d; k <= d; k += 2) {
			let x =
				k === -d || (k !== d && v[offset + k - 1] < v[offset + k + 1])
					? v[offset + k + 1]
					: v[offset + k - 1] + 1;
			let y = x - k;
			while (x < n && y < m && a[x] === b[y]) {
				x++;
				y++;
			}
			v[offset + k] = x;
			if (x >= n && y >= m) return backtrack(trace, d, n, m);
		}
	}
	return null;
}

function backtrack(trace: Int32Array[], last: number, n: number, m: number) {
	const pairs: [number, number][] = [];
	let [x, y] = [n, m];
	for (let d = last; d > 0; d--) {
		const before = trace[d];
		const at = (k: number) => before[k + d + 1];
		const k = x - y;
		const previous = k === -d || (k !== d && at(k - 1) < at(k + 1)) ? k + 1 : k - 1;
		const px = at(previous);
		const py = px - previous;
		while (x > px && y > py) pairs.push([--x, --y]);
		[x, y] = [px, py];
	}
	while (x > 0 && y > 0) pairs.push([--x, --y]);
	return pairs.reverse();
}

function intern(values: string[][]) {
	const ids = new Map<string, number>();
	return values.map(
		(list) =>
			new Int32Array(
				list.map((value) => {
					let id = ids.get(value);
					if (id === undefined) ids.set(value, (id = ids.size));
					return id;
				})
			)
	);
}

/// Equal words, as pairs of positions in each side's word list, in order.
function matchWords(original: Word[], changed: Word[]): [number, number][] {
	const lines = (list: Word[]) => {
		const keys: string[] = [];
		const starts: number[] = [];
		list.forEach((word, index) => {
			if (index === 0 || word.line !== list[index - 1].line) {
				starts.push(index);
				keys.push(word.key);
			} else keys[keys.length - 1] += ' ' + word.key;
		});
		starts.push(list.length);
		return { keys, starts };
	};
	const [a, b] = [lines(original), lines(changed)];
	const [lineA, lineB] = intern([a.keys, b.keys]);
	const lineMatches = myers(lineA, lineB, LINE_LIMIT) ?? [];
	const [wordA, wordB] = intern([
		original.map((word) => word.key),
		changed.map((word) => word.key)
	]);
	const pairs: [number, number][] = [];
	// The words between two matched lines, compared word by word.
	const between = (fromA: number, toA: number, fromB: number, toB: number) => {
		if (fromA >= toA || fromB >= toB) return;
		const inner = myers(wordA.subarray(fromA, toA), wordB.subarray(fromB, toB), WORD_LIMIT) ?? [];
		for (const [x, y] of inner) pairs.push([fromA + x, fromB + y]);
	};
	let [lastA, lastB] = [0, 0];
	for (const [x, y] of lineMatches) {
		between(a.starts[lastA], a.starts[x], b.starts[lastB], b.starts[y]);
		for (let offset = 0; offset < a.starts[x + 1] - a.starts[x]; offset++)
			pairs.push([a.starts[x] + offset, b.starts[y] + offset]);
		[lastA, lastB] = [x + 1, y + 1];
	}
	between(a.starts[lastA], original.length, b.starts[lastB], changed.length);
	return pairs;
}

/// Neighbouring words on one line share a box, so a phrase reads as one mark.
function places(list: Word[], from: number, to: number): ChangePlace[] {
	const found: ChangePlace[] = [];
	for (let index = from; index < to; index++) {
		const word = list[index];
		let place = found.at(-1);
		if (place?.page !== word.page) found.push((place = { page: word.page, boxes: [] }));
		for (const box of word.boxes) {
			const last = place.boxes.at(-1);
			if (last && sameLine(last, box) && box[0] >= last[0])
				place.boxes[place.boxes.length - 1] = union([last, box]);
			else place.boxes.push(box);
		}
	}
	return found;
}

/// A zero-width box at the end of the word before, or the start of the one
/// after when there is none.
function caretAt(list: Word[], before: number) {
	const word = list[before] ?? list[before + 1];
	if (!word) return null;
	const box = list[before] ? word.boxes[word.boxes.length - 1] : word.boxes[0];
	const x = list[before] ? box[2] : box[0];
	return { page: word.page, box: [x, box[1], x, box[3]] as CropArea };
}

const sentence = (list: Word[], from: number, to: number) =>
	list
		.slice(from, to)
		.map((word) => word.key)
		.join(' ');

const middle = (box: CropArea) => (box[1] + box[3]) / 2;

export function compareText(original: PageGlyphs[], changed: PageGlyphs[]): Comparison {
	const [a, b] = [words(original), words(changed)];
	const pairs = matchWords(a, b);
	const changes: Change[] = [];
	const anchors: Anchor[] = [];
	let [lastA, lastB] = [-1, -1];
	const differ = (toA: number, toB: number) => {
		const [fromA, fromB] = [lastA + 1, lastB + 1];
		if (fromA === toA && fromB === toB) return;
		const caret = fromA === toA ? caretAt(a, lastA) : fromB === toB ? caretAt(b, lastB) : null;
		changes.push({
			id: changes.length,
			removed: sentence(a, fromA, toA),
			added: sentence(b, fromB, toB),
			original: places(a, fromA, toA),
			changed: places(b, fromB, toB),
			caret: caret && { side: fromA === toA ? 'original' : 'changed', ...caret }
		});
	};
	pairs.forEach(([x, y], index) => {
		differ(x, y);
		// The first word of each equal stretch, and every so often within one.
		if (x !== lastA + 1 || y !== lastB + 1 || index % 20 === 0)
			anchors.push({
				original: [a[x].page, middle(a[x].boxes[0])],
				changed: [b[y].page, middle(b[y].boxes[0])]
			});
		[lastA, lastB] = [x, y];
	});
	differ(a.length, b.length);

	const votes = new Map<number, Map<number, number>>();
	for (const [x, y] of pairs) {
		const page = votes.get(b[y].page) ?? new Map<number, number>();
		page.set(a[x].page, (page.get(a[x].page) ?? 0) + 1);
		votes.set(b[y].page, page);
	}
	const pagePairs = changed.map((_, index) => {
		const page = votes.get(index + 1);
		if (!page) return Math.min(index + 1, Math.max(1, original.length));
		return [...page.entries()].reduce((best, entry) => (entry[1] > best[1] ? entry : best))[0];
	});
	return { changes, anchors, pairs: pagePairs };
}

/// Whether either PDF has any text to compare.
export const hasText = (pages: PageGlyphs[]) => pages.some((page) => page.ends.length > 0);

// The site's merge and convert colours, which mark added and removed text.
const ADDED = 0x86eaba;
const REMOVED = 0xff94ae;
// A note's icon is 20 points; this keeps it on a small page too.
const EDGE = 0.96;

/// The changed PDF's side of the comparison as annotations: added text
/// highlighted, with what it replaced as the comment, and a note where text
/// was taken out.
export function markup(changes: Change[]): PageAnnotation[] {
	const clip = (text: string) => (text.length > 2000 ? `${text.slice(0, 2000)}…` : text);
	return changes.flatMap((change): PageAnnotation[] => {
		if (change.changed.length)
			return change.changed.map((place, index) => ({
				kind: 'highlight',
				boxes: place.boxes,
				page: place.page,
				color: ADDED,
				opacity: 1,
				comment: index === 0 && change.removed ? `Was: ${clip(change.removed)}` : ''
			}));
		if (change.caret?.side !== 'changed') return [];
		const [x, top] = change.caret.box;
		return [
			{
				kind: 'note',
				at: [Math.min(EDGE, Math.max(0, x)), Math.min(EDGE, Math.max(0, top))],
				page: change.caret.page,
				color: REMOVED,
				opacity: 1,
				comment: `Removed: ${clip(change.removed)}`
			}
		];
	});
}
