// Summarize PDF's text work, kept apart from its state: the document's
// paragraphs (Translate's, read from the engine's glyphs) without running
// heads, cut into parts small enough for the model; the points the model
// lists; and where each point came from.

import { pageBlocks } from './translate-layout';
import type { CropArea, PageGlyphs } from './types';

export type SummaryPoint = {
	text: string;
	page: number;
	/// The paragraph it came from, as fractions of the page, when one shares
	/// enough of its words.
	box?: CropArea;
};

export type Paragraph = { page: number; box: CropArea; text: string };
export type Part = { paragraphs: Paragraph[]; text: string; first: number; last: number };

// About 1,200 tokens: room for the instructions and the answer in what a
// small model reads well, and quick to read on a CPU.
const PART_CHARACTERS = 5000;

// A short paragraph seen on this many pages is a running head or footer.
const REPEATED = 3;

// A point is traced to a paragraph holding at least this share of its words.
const TRACED = 0.3;

const STOPWORDS = new Set(
	'the and for are but not you all any can had her was one our out has have this that with from they will would there their what which when where who whom been were into than then them these those such some more most other only very also its your about after before over under between through during while because each both same just should could does did doing being here how why may might must shall upon onto off'.split(
		' '
	)
);

/// The words a point and a paragraph are compared by. Scripts written without
/// spaces are compared by pairs of characters instead.
function words(text: string) {
	const found = new Set<string>();
	for (const word of text.toLowerCase().match(/[\p{L}\p{N}]+/gu) ?? []) {
		if (
			/[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Hangul}\p{Script=Thai}]/u.test(
				word
			)
		) {
			for (let index = 0; index < word.length - 1; index++) found.add(word.slice(index, index + 2));
		} else if (word.length > 2 && !STOPWORDS.has(word)) found.add(word.replace(/s$/, ''));
	}
	return found;
}

/// The paragraphs of the pages in `scope`, in order, without running heads
/// and footers.
export function documentParagraphs(
	glyphs: PageGlyphs[],
	sizes: { width: number; height: number }[],
	scope: number[]
) {
	const found: Paragraph[] = scope.flatMap((page) => {
		const { width, height } = sizes[page - 1];
		return pageBlocks(glyphs[page - 1], page, width, height).map((block) => ({
			page,
			box: block.box,
			text: block.text.replace(/\s+/g, ' ').trim()
		}));
	});
	const seen = new Map<string, Set<number>>();
	const shape = (text: string) => text.toLowerCase().replace(/\d+/g, '#');
	for (const paragraph of found) {
		const key = shape(paragraph.text);
		seen.set(key, (seen.get(key) ?? new Set()).add(paragraph.page));
	}
	return found.filter(
		(paragraph) =>
			paragraph.text.length > 1 &&
			!(paragraph.text.length < 120 && (seen.get(shape(paragraph.text))?.size ?? 0) >= REPEATED)
	);
}

export function parts(paragraphs: Paragraph[]): Part[] {
	const found: Part[] = [];
	let current: Paragraph[] = [];
	let length = 0;
	const close = () => {
		if (!current.length) return;
		found.push({
			paragraphs: current,
			text: current.map((paragraph) => paragraph.text).join('\n\n'),
			first: current[0].page,
			last: current[current.length - 1].page
		});
		current = [];
		length = 0;
	};
	for (const paragraph of paragraphs) {
		// A paragraph longer than a part is cut at spaces, each piece still
		// its paragraph.
		for (let start = 0; start < paragraph.text.length;) {
			let end = Math.min(paragraph.text.length, start + PART_CHARACTERS);
			if (end < paragraph.text.length) end = paragraph.text.lastIndexOf(' ', end) + 1 || end;
			const piece = { ...paragraph, text: paragraph.text.slice(start, end).trim() };
			if (length + piece.text.length > PART_CHARACTERS) close();
			current.push(piece);
			length += piece.text.length + 2;
			start = end;
		}
	}
	close();
	return found;
}

/// The points in a Markdown list the model wrote, the last perhaps unfinished.
export function listedPoints(text: string) {
	const lines = text
		.split('\n')
		.map((line) => line.trim())
		.filter(Boolean);
	const listed = lines.filter((line) => /^([-*•]|\d+[.)])\s+/.test(line));
	return (listed.length ? listed : lines.filter((line) => !line.endsWith(':')))
		.map((line) =>
			line
				.replace(/^([-*•]|\d+[.)])\s+/, '')
				.replace(/\*\*/g, '')
				.trim()
		)
		.filter(Boolean);
}

/// The point, with the paragraph of `part` that holds most of its words.
export function trace(text: string, part: Part): SummaryPoint {
	const point = words(text);
	let best: Paragraph | undefined;
	let score = 0;
	for (const paragraph of part.paragraphs) {
		const theirs = words(paragraph.text);
		let shared = 0;
		for (const word of point) if (theirs.has(word)) shared++;
		const share = point.size ? shared / point.size : 0;
		if (share > score) [best, score] = [paragraph, share];
	}
	return best && score >= TRACED
		? { text, page: best.page, box: best.box }
		: { text, page: part.first };
}

/// The `most` points that share the most words with the whole document, in
/// document order.
export function central(points: SummaryPoint[], most: number, paragraphs: Paragraph[]) {
	if (points.length <= most) return points;
	const counts = new Map<string, number>();
	for (const paragraph of paragraphs)
		for (const word of words(paragraph.text)) counts.set(word, (counts.get(word) ?? 0) + 1);
	const score = (point: SummaryPoint) => {
		const own = [...words(point.text)];
		return (
			own.reduce((sum, word) => sum + Math.log(1 + (counts.get(word) ?? 0)), 0) / (own.length || 1)
		);
	};
	const kept = new Set(
		points
			.map((point, index) => ({ index, score: score(point) }))
			.sort((a, b) => b.score - a.score)
			.slice(0, most)
			.map(({ index }) => index)
	);
	return points.filter((_, index) => kept.has(index));
}

/// The paragraphs that best answer `query`, ranked as BM25 ranks them with
/// each word counted once, as many as fit in `budget` characters, in document
/// order. A document that fits is given whole; a question that matches
/// nothing gets the document's start.
export function relevant(paragraphs: Paragraph[], query: string, budget: number) {
	const total = paragraphs.reduce((sum, paragraph) => sum + paragraph.text.length, 0);
	if (total <= budget) return paragraphs;
	const asked = words(query);
	const sets = paragraphs.map((paragraph) => words(paragraph.text));
	const holding = new Map<string, number>();
	for (const set of sets)
		for (const word of asked) if (set.has(word)) holding.set(word, (holding.get(word) ?? 0) + 1);
	const average = total / paragraphs.length;
	const ranked = paragraphs
		.map((paragraph, index) => {
			let score = 0;
			for (const word of asked) {
				if (!sets[index].has(word)) continue;
				const held = holding.get(word) ?? 0;
				score += Math.log(1 + (paragraphs.length - held + 0.5) / (held + 0.5));
			}
			return {
				index,
				score: (score * 2.2) / (1 + 1.2 * (0.25 + (0.75 * paragraph.text.length) / average))
			};
		})
		.filter(({ score }) => score > 0)
		.sort((a, b) => b.score - a.score);
	const order = ranked.length
		? ranked.map(({ index }) => index)
		: paragraphs.map((_, index) => index);
	const kept: number[] = [];
	let length = 0;
	for (const index of order) {
		const size = paragraphs[index].text.length;
		if (length + size > budget) {
			if (kept.length) continue;
		}
		kept.push(index);
		length += size;
		if (length >= budget) break;
	}
	return kept.sort((a, b) => a - b).map((index) => paragraphs[index]);
}

/// The excerpts an answer draws on most, at most three, in document order.
export function sources(answer: string, excerpts: Paragraph[]) {
	const said = words(answer);
	if (!said.size) return [];
	return excerpts
		.map((paragraph, index) => {
			const theirs = words(paragraph.text);
			let shared = 0;
			for (const word of said) if (theirs.has(word)) shared++;
			return { paragraph, index, share: shared / said.size };
		})
		.filter(({ share }) => share >= 0.2)
		.sort((a, b) => b.share - a.share)
		.slice(0, 3)
		.sort((a, b) => a.index - b.index)
		.map(({ paragraph }) => paragraph);
}
