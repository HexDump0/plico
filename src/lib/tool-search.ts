import type { CatalogTool } from './tool-catalog';

const keywords: Record<string, string> = {
	merge: 'combine join concatenate together',
	compress: 'shrink reduce size smaller optimize',
	split: 'separate divide individual pages',
	edit: 'modify change text',
	sign: 'signature autograph esign',
	organize: 'reorder rearrange sort move pages',
	extract: 'save export selected pages',
	remove: 'delete discard pages',
	rotate: 'turn orientation sideways upside down',
	annotate: 'highlight comment draw markup notes',
	crop: 'trim margins resize',
	'page-numbers': 'pagination numbering',
	watermark: 'stamp overlay',
	forms: 'fillable fields fill form',
	'jpg-to-pdf': 'image photo picture',
	'png-to-pdf': 'image photo picture',
	'word-to-pdf': 'document',
	'powerpoint-to-pdf': 'slides presentation',
	'excel-to-pdf': 'spreadsheet sheet',
	'html-to-pdf': 'website webpage',
	'scan-to-pdf': 'scanner camera',
	repair: 'fix damaged broken corrupt recover',
	ocr: 'recognize scanned text searchable recognition',
	flatten: 'layers annotations fields',
	summarize: 'summary overview shorten',
	translate: 'language translation',
	'pdf-to-jpg': 'image photo picture export',
	'pdf-to-png': 'image photo picture export',
	'pdf-to-word': 'document editable',
	'pdf-to-powerpoint': 'slides presentation',
	'pdf-to-excel': 'spreadsheet sheet table',
	'pdf-to-pdfa': 'archive archival',
	'pdf-to-markdown': 'md text',
	protect: 'encrypt password lock secure',
	unlock: 'decrypt remove password unprotect',
	redact: 'hide censor black out sensitive',
	compare: 'difference differences diff changes'
};

const formats: Record<string, string> = {
	jpeg: 'jpg',
	doc: 'word',
	docx: 'word',
	ppt: 'powerpoint',
	pptx: 'powerpoint',
	xls: 'excel',
	xlsx: 'excel'
};
const formatNames = new Set([
	'pdf',
	'jpg',
	'png',
	'word',
	'powerpoint',
	'excel',
	'html',
	'pdfa',
	'markdown'
]);
const filler = new Set(['a', 'an', 'the', 'my', 'please', 'file', 'files']);

function normalize(value: string) {
	return value
		.normalize('NFKD')
		.toLowerCase()
		.replace(/[\u0300-\u036f]/g, '')
		.replace(/pdf\s*\/\s*a\b/g, 'pdfa')
		.replace(/[^a-z0-9]+/g, ' ')
		.trim()
		.split(/\s+/)
		.filter(Boolean)
		.map((word) => formats[word] ?? word);
}

function distance(left: string, right: string) {
	const rows = Array.from({ length: left.length + 1 }, (_, i) =>
		Array.from({ length: right.length + 1 }, (_, j) => (i === 0 ? j : j === 0 ? i : 0))
	);
	for (let i = 1; i <= left.length; i += 1) {
		for (let j = 1; j <= right.length; j += 1) {
			rows[i][j] = Math.min(
				rows[i - 1][j] + 1,
				rows[i][j - 1] + 1,
				rows[i - 1][j - 1] + Number(left[i - 1] !== right[j - 1])
			);
			if (i > 1 && j > 1 && left[i - 1] === right[j - 2] && left[i - 2] === right[j - 1]) {
				rows[i][j] = Math.min(rows[i][j], rows[i - 2][j - 2] + 1);
			}
		}
	}
	return rows[left.length][right.length];
}

function wordScore(query: string, word: string) {
	if (query === word) return 0;
	if (word.startsWith(query)) return 0.12;
	if (query.length >= 3 && word.includes(query)) return 0.25;
	const tolerance = query.length >= 7 ? 2 : query.length >= 4 ? 1 : 0;
	if (!tolerance || Math.abs(query.length - word.length) > tolerance) return Infinity;
	const edits = distance(query, word);
	return edits <= tolerance ? 0.6 + edits * 0.15 : Infinity;
}

export function searchTools<T extends CatalogTool>(
	tools: readonly T[],
	query: string,
	category = ''
): T[] {
	const words = normalize(query).filter((word) => !filler.has(word));
	if (!words.length) return [...tools];
	const phrase = words.join(' ');
	const direction = phrase.match(/\b(\w+) to (\w+)\b/);
	return tools
		.map((tool, index) => {
			const title = normalize(tool.label);
			const titleText = title.join(' ');
			if (
				direction &&
				formatNames.has(direction[1]) &&
				formatNames.has(direction[2]) &&
				!titleText.includes(direction[0])
			) {
				return { tool, index, score: Infinity };
			}
			const fields = [
				{ words: title, weight: 0 },
				{ words: normalize(keywords[tool.id] ?? ''), weight: 0.3 },
				{ words: normalize(`${category} pdf`), weight: 0.5 }
			];
			let score = 0;
			for (const word of words) {
				score += Math.min(
					...fields.flatMap((field) =>
						field.words.map((candidate) => wordScore(word, candidate) + field.weight)
					)
				);
			}
			if (titleText === phrase) score -= 0.4;
			else if (titleText.startsWith(phrase)) score -= 0.2;
			return { tool, index, score };
		})
		.filter(({ score }) => Number.isFinite(score))
		.sort((a, b) => a.score - b.score || a.index - b.index)
		.map(({ tool }) => tool);
}
