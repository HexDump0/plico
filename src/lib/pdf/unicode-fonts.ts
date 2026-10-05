// The Noto fonts in `static/fonts/` that draw what the standard PDF fonts
// cannot, and which of them a piece of text needs. The engine decides for a
// whole piece of text at once: if any character is outside WinAnsiEncoding,
// all of it is set in these fonts, the family's own first and then each
// script's, a character taking the first font that has it. The coverage here
// only chooses what to fetch; the engine has the final say.

import type { FontFamily } from './standard-fonts';

export type FontFace = {
	/// The engine's group: a family name serves only that family.
	group: string;
	regular: string;
	bold?: string;
	covers: RegExp;
};

const western =
	/[\p{Script=Latin}\p{Script=Greek}\p{Script=Cyrillic}\p{Script=Inherited} -\u00bf\u2000-\u206f\u20a0-\u20cf\u2100-\u218f\u2190-\u21ff\u2200-\u22ff\u25a0-\u25ff]/u;

export const familyFaces: Record<FontFamily, FontFace> = {
	helvetica: {
		group: 'helvetica',
		regular: 'NotoSans-Regular.ttf',
		bold: 'NotoSans-Bold.ttf',
		covers: western
	},
	times: {
		group: 'times',
		regular: 'NotoSerif-Regular.ttf',
		bold: 'NotoSerif-Bold.ttf',
		covers: western
	},
	courier: {
		group: 'courier',
		regular: 'NotoSansMono-Regular.ttf',
		bold: 'NotoSansMono-Bold.ttf',
		covers: western
	}
};

const scriptFaces: FontFace[] = [
	{
		group: 'arabic',
		regular: 'NotoSansArabic-Regular.ttf',
		bold: 'NotoSansArabic-Bold.ttf',
		covers: /\p{Script=Arabic}/u
	},
	{
		group: 'hebrew',
		regular: 'NotoSansHebrew-Regular.ttf',
		bold: 'NotoSansHebrew-Bold.ttf',
		covers: /\p{Script=Hebrew}/u
	},
	{
		group: 'devanagari',
		regular: 'NotoSansDevanagari-Regular.ttf',
		bold: 'NotoSansDevanagari-Bold.ttf',
		covers: /\p{Script=Devanagari}/u
	},
	{
		group: 'bengali',
		regular: 'NotoSansBengali-Regular.ttf',
		bold: 'NotoSansBengali-Bold.ttf',
		covers: /\p{Script=Bengali}/u
	},
	{
		group: 'tamil',
		regular: 'NotoSansTamil-Regular.ttf',
		bold: 'NotoSansTamil-Bold.ttf',
		covers: /\p{Script=Tamil}/u
	},
	{
		group: 'gujarati',
		regular: 'NotoSansGujarati-Regular.ttf',
		bold: 'NotoSansGujarati-Bold.ttf',
		covers: /\p{Script=Gujarati}/u
	},
	{
		group: 'kannada',
		regular: 'NotoSansKannada-Regular.ttf',
		bold: 'NotoSansKannada-Bold.ttf',
		covers: /\p{Script=Kannada}/u
	},
	{
		group: 'malayalam',
		regular: 'NotoSansMalayalam-Regular.ttf',
		bold: 'NotoSansMalayalam-Bold.ttf',
		covers: /\p{Script=Malayalam}/u
	},
	{
		group: 'telugu',
		regular: 'NotoSansTelugu-Regular.ttf',
		bold: 'NotoSansTelugu-Bold.ttf',
		covers: /\p{Script=Telugu}/u
	},
	{
		group: 'thai',
		regular: 'NotoSansThai-Regular.ttf',
		bold: 'NotoSansThai-Bold.ttf',
		covers: /\p{Script=Thai}/u
	}
];

const cjk = /[\p{Script=Han}\u3000-\u303f\uff00-\uffef]/u;
const cjkFaces = {
	japanese: {
		group: 'japanese',
		regular: 'NotoSansJP-Regular.otf',
		covers: /[\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Han}\u3000-\u303f\uff00-\uffef]/u
	},
	korean: {
		group: 'korean',
		regular: 'NotoSansKR-Regular.otf',
		covers: /[\p{Script=Hangul}\p{Script=Han}\u3000-\u303f\uff00-\uffef]/u
	},
	simplified: { group: 'simplified', regular: 'NotoSansSC-Regular.otf', covers: cjk },
	traditional: { group: 'traditional', regular: 'NotoSansTC-Regular.otf', covers: cjk }
} satisfies Record<string, FontFace>;

/// Han characters look different in each region, so they take the font of
/// the kana or hangul around them, or else the reader's language.
function cjkFacesFor(text: string): FontFace[] {
	const faces: FontFace[] = [];
	if (/[\p{Script=Hiragana}\p{Script=Katakana}]/u.test(text)) faces.push(cjkFaces.japanese);
	if (/\p{Script=Hangul}/u.test(text)) faces.push(cjkFaces.korean);
	if (faces.length || !cjk.test(text)) return faces;
	const language = typeof navigator === 'undefined' ? '' : navigator.language.toLowerCase();
	if (language.startsWith('ja')) return [cjkFaces.japanese];
	if (language.startsWith('ko')) return [cjkFaces.korean];
	if (/^zh-(tw|hk|mo|hant)/.test(language)) return [cjkFaces.traditional];
	return [cjkFaces.simplified];
}

const winAnsiHigh = new Set('€‚ƒ„…†‡ˆ‰Š‹ŒŽ‘’“”•–—˜™š›œžŸ');

function inWinAnsi(character: string) {
	const code = character.codePointAt(0) ?? 0;
	return (
		(code >= 0x20 && code <= 0x7e) || (code >= 0xa0 && code <= 0xff) || winAnsiHigh.has(character)
	);
}

/// Whether the engine sets `text` in these fonts rather than the standard ones.
export function needsEmbedding(text: string) {
	return [...text.replace(/[\r\n]/g, '')].some((character) => !inWinAnsi(character));
}

/// The fonts that draw `text` in `family`, in the order the engine tries them.
export function facesFor(text: string, family: FontFamily): FontFace[] {
	const faces = [familyFaces[family]];
	for (const face of scriptFaces) if (face.covers.test(text)) faces.push(face);
	return [...faces, ...cjkFacesFor(text)];
}

/// The first character none of the fonts can draw.
export function uncovered(text: string, family: FontFamily = 'helvetica') {
	if (!needsEmbedding(text)) return undefined;
	const faces = facesFor(text, family);
	return [...text.replace(/[\r\n]/g, '')].find(
		(character) => !inWinAnsi(character) && !faces.some((face) => face.covers.test(character))
	);
}

export type TextItem = { text: string; family: FontFamily; bold: boolean };

/// A font file to fetch and the role the engine knows it by.
export type FontRequest = { file: string; role: string };

/// The files a job's texts need; none when the standard fonts draw them all.
export function fontRequests(items: TextItem[]): FontRequest[] {
	const requests = new Map<string, string>();
	for (const item of items) {
		if (!needsEmbedding(item.text)) continue;
		for (const face of facesFor(item.text, item.family)) {
			const bold = item.bold && face.bold !== undefined;
			const file = bold ? face.bold! : face.regular;
			requests.set(file, bold ? `${face.group}-bold` : face.group);
		}
	}
	// A family's own fonts go first: script fonts serve every family, and a
	// character takes the first font that has it.
	const families = new Set(Object.keys(familyFaces));
	return [...requests]
		.map(([file, role]) => ({ file, role }))
		.sort(
			(a, b) =>
				Number(!families.has(a.role.replace(/-bold$/, ''))) -
				Number(!families.has(b.role.replace(/-bold$/, '')))
		);
}
