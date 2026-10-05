// HTML laid out by the browser and measured, for the engine to draw as PDF
// pages (`print.rs`).
//
// The document goes into a sandboxed frame that runs no scripts and fetches
// nothing. Every element is set in the bundled Noto fonts, the same files the
// engine embeds, so what the browser measures is what gets drawn. The root
// becomes a multi-column box one page tall: the browser breaks the document
// into columns the way it breaks pages, keeping lines whole and honouring
// break rules, and each column becomes a page. Text, boxes, borders, images
// and links are then read off the layout in paint order.

import { missingCharacters } from './processor';
import type { FontFamily } from './standard-fonts';
import type { PrintLayout } from './types';
import { facesFor, type FontFace as Face, type TextItem } from './unicode-fonts';

export type PrintSize = 'a4' | 'letter' | 'one';
/// `margin` is in points.
export type PrintOptions = { size: PrintSize; landscape: boolean; margin: number };

export type PrintResult = {
	layout: PrintLayout;
	images: ArrayBuffer[];
	fonts: TextItem[];
	/// Images the document links to rather than contains, which were left out.
	skipped: number;
};

export const PRINT_STRIDE = 20;
const KIND = { fill: 0, line: 1, outline: 2, text: 3, image: 4, link: 5 } as const;
const FAMILY = { helvetica: 0, times: 1, courier: 2 } as const;
const DASH: Record<string, number> = { dashed: 1, dotted: 2 };
// Points per CSS pixel.
const POINT = 0.75;
const PAPER = { a4: [595.28, 841.89], letter: [612, 792] } as const;
const LONGEST_PAGE = 14_400;
// Wider than this and a page is shrunk to fit, as browsers print; never
// below half size.
const SMALLEST_SCALE = 0.5;
// Pictures of what no bundled font draws, in pixels per CSS pixel.
const PICTURE_SCALE = 4;

const SKIPPED = new Set(['HEAD', 'SCRIPT', 'STYLE', 'TEMPLATE', 'TITLE', 'META', 'LINK', 'BASE']);
const VOID = new Set(['IMG', 'INPUT', 'BR', 'WBR', 'AREA', 'COL', 'EMBED', 'SOURCE', 'TRACK']);
const REPLACED = new Set(['IMG', 'SVG', 'VIDEO', 'CANVAS', 'AUDIO', 'IFRAME', 'OBJECT', 'EMBED']);

const segmenter = new Intl.Segmenter(undefined, { granularity: 'word' });
const graphemes = new Intl.Segmenter(undefined, { granularity: 'grapheme' });

/// Text of an HTML file in the encoding it declares, UTF-8 when it does not.
export async function readHtml(file: File) {
	const bytes = new Uint8Array(await file.arrayBuffer());
	if (bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf)
		return new TextDecoder('utf-8').decode(bytes);
	if ((bytes[0] === 0xff && bytes[1] === 0xfe) || (bytes[0] === 0xfe && bytes[1] === 0xff))
		return new TextDecoder(bytes[0] === 0xff ? 'utf-16le' : 'utf-16be').decode(bytes);
	const head = new TextDecoder('latin1').decode(bytes.subarray(0, 2048));
	const declared = /<meta[^>]+charset\s*=\s*["']?\s*([\w-]+)/i.exec(head)?.[1];
	try {
		return new TextDecoder(declared ?? 'utf-8').decode(bytes);
	} catch {
		return new TextDecoder('utf-8').decode(bytes);
	}
}

/// Lays out `html` and reads off everything a PDF of it draws. Throws an
/// AbortError once `signal` aborts.
export async function layoutHtml(
	html: string,
	options: PrintOptions,
	signal?: AbortSignal
): Promise<PrintResult> {
	const { source, skipped } = prepare(html);
	const [short, long] = options.size === 'letter' ? PAPER.letter : PAPER.a4;
	const pageWidth = options.landscape ? long : short;
	const pageHeight = options.size === 'one' ? LONGEST_PAGE : options.landscape ? short : long;
	const margin = Math.min(options.margin, pageWidth / 4);
	const frame = await openFrame(source);
	try {
		const doc = frame.contentDocument!;
		const view = frame.contentWindow!;
		emulatePrint(doc);
		materialize(doc, view);
		const characters = uniqueCharacters(doc);
		const families = setFonts(doc, view, characters);
		const fonts = fontItems(characters, families.used);
		// Not cancelled with the rest: cancelling restarts the shared PDF
		// worker, and this answers quickly.
		const [missing] = await Promise.all([
			missingCharacters(characters, fonts).catch(() => ''),
			loadFaces(doc, view, characters, families.used)
		]);
		check(signal);
		await Promise.all([...doc.images].map((image) => image.decode().catch(() => undefined)));
		check(signal);
		let scale = 1;
		let measured = await measure();
		// Wider than the page: lay it out again on a wider page, scaled down.
		if (measured.widest > measured.width * 1.01) {
			scale = Math.max(SMALLEST_SCALE, measured.width / measured.widest);
			measured = await measure();
		}
		return { ...measured.result, fonts, skipped };

		async function measure() {
			const unit = POINT * scale;
			const width = (pageWidth - 2 * margin) / unit;
			const height = (pageHeight - 2 * margin) / unit;
			const gap = Math.max((2 * margin) / unit, 48);
			paginate(frame, doc, width, height, gap);
			const reader = new Reader(doc, view, {
				width,
				height,
				gap,
				unit,
				margin,
				pageWidth,
				pageHeight,
				one: options.size === 'one',
				missing: new Set(missing),
				families: families.of,
				signal
			});
			const layout = await reader.read();
			return { result: { layout, images: reader.images }, widest: reader.widest, width };
		}
	} finally {
		frame.remove();
	}
}

function check(signal?: AbortSignal) {
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
}

/// The document as the frame gets it: nothing that runs or fetches, a policy
/// that blocks whatever is left, and the stylesheets that set up the page.
function prepare(html: string) {
	const parsed = new DOMParser().parseFromString(html, 'text/html');
	for (const element of parsed.querySelectorAll(
		'script, iframe, frame, frameset, object, embed, applet, base, link, meta[http-equiv], video, audio, track, portal'
	))
		element.remove();
	let skipped = 0;
	const embedded = (url: string | null) => !url || /^\s*data:/i.test(url);
	for (const image of parsed.querySelectorAll('img')) {
		if (embedded(image.getAttribute('src')) && image.getAttribute('src')) {
			image.removeAttribute('srcset');
			image.removeAttribute('loading');
			continue;
		}
		if (image.getAttribute('src') || image.getAttribute('srcset')) skipped++;
		image.remove();
	}
	for (const source of parsed.querySelectorAll('source'))
		if (!embedded(source.getAttribute('srcset'))) source.remove();
	const head = parsed.head;
	const policy = parsed.createElement('meta');
	policy.httpEquiv = 'Content-Security-Policy';
	policy.content =
		"default-src 'none'; style-src 'unsafe-inline'; img-src data:; font-src 'none'; form-action 'none'";
	const base = parsed.createElement('style');
	base.textContent = BASE_STYLE;
	head.prepend(policy, base);
	const forced = parsed.createElement('style');
	forced.id = 'plico-page';
	head.append(forced);
	return { source: `<!doctype html>${parsed.documentElement.outerHTML}`, skipped };
}

// Before the document's own styles, so it can change any of it.
const BASE_STYLE = `
img, svg, video, canvas { max-width: 100%; }
img { height: auto; }
img, svg, figure, tr { break-inside: avoid; }
h1, h2, h3, h4, h5, h6 { break-after: avoid; }
pre { white-space: pre-wrap; overflow-wrap: anywhere; }
`;

async function openFrame(source: string) {
	const frame = document.createElement('iframe');
	// Same origin so it can be measured, and no scripts, so the document
	// cannot use that.
	frame.setAttribute('sandbox', 'allow-same-origin');
	frame.setAttribute('aria-hidden', 'true');
	frame.tabIndex = -1;
	frame.inert = true;
	Object.assign(frame.style, {
		position: 'fixed',
		left: '-200000px',
		top: '0',
		border: '0',
		visibility: 'hidden',
		pointerEvents: 'none'
	});
	const loaded = new Promise((resolve) => frame.addEventListener('load', resolve, { once: true }));
	frame.srcdoc = source;
	document.body.append(frame);
	await loaded;
	return frame;
}

/// One page of content per column.
function paginate(
	frame: HTMLIFrameElement,
	doc: Document,
	width: number,
	height: number,
	gap: number
) {
	frame.style.width = `${width}px`;
	frame.style.height = `${height}px`;
	const style = doc.getElementById('plico-page');
	if (!style) return;
	style.textContent = `
html {
	box-sizing: content-box !important;
	width: ${width}px !important;
	height: ${height}px !important;
	min-width: 0 !important;
	max-width: none !important;
	min-height: 0 !important;
	max-height: none !important;
	margin: 0 !important;
	padding: 0 !important;
	border: 0 !important;
	overflow: visible !important;
	direction: ltr !important;
	columns: ${width}px auto !important;
	column-gap: ${gap}px !important;
	column-fill: auto !important;
	column-rule: none !important;
}
img, svg, video, canvas { max-height: ${height}px; }
[data-plico-before]::before, [data-plico-after]::after { content: none !important; }
`;
}

/// Rules written for print take over from those written for screens.
function emulatePrint(doc: Document) {
	const adjust = (media: MediaList) => {
		if (!media.length) return;
		const queries = [...media].map((query) => {
			const match = /^\s*(not\s+|only\s+)?(print|screen)\b(.*)$/i.exec(query);
			if (!match) return query;
			const negated = /^not/i.test(match[1] ?? '');
			const rest = match[3];
			const matches = (match[2].toLowerCase() === 'print') !== negated;
			return matches ? `all${rest}` : 'not all';
		});
		media.mediaText = queries.join(', ');
	};
	const walk = (rules: CSSRuleList) => {
		for (const rule of rules) {
			if ('media' in rule && rule.media && 'cssRules' in rule) adjust(rule.media as MediaList);
			if ('cssRules' in rule && rule.cssRules) walk(rule.cssRules as CSSRuleList);
		}
	};
	for (const sheet of doc.styleSheets) {
		try {
			adjust(sheet.media);
			walk(sheet.cssRules);
		} catch {
			// A sheet that cannot be read stays as it is.
		}
	}
	// A right-to-left document keeps its direction while the pages still run
	// left to right.
	const view = doc.defaultView!;
	if (view.getComputedStyle(doc.documentElement).direction === 'rtl' && doc.body)
		doc.body.style.setProperty('direction', view.getComputedStyle(doc.body).direction);
}

/// List markers and generated text live outside the DOM, where nothing can
/// measure them; each becomes an element drawing the same text.
function materialize(doc: Document, view: Window) {
	if (!doc.body) return;
	const ordinals = listOrdinals(doc);
	for (const element of [doc.body, ...doc.body.querySelectorAll('*')]) {
		if (VOID.has(element.tagName) || element.closest('svg')) continue;
		const style = view.getComputedStyle(element);
		if (style.display === 'none') continue;
		for (const [where, attribute] of [
			['::before', 'data-plico-before'],
			['::after', 'data-plico-after']
		] as const) {
			const pseudo = view.getComputedStyle(element, where);
			if (pseudo.display === 'none') continue;
			const text = generatedText(pseudo.content, element);
			if (text === null) continue;
			const span = doc.createElement('plico-text');
			const declarations: string[] = [];
			for (let index = 0; index < pseudo.length; index++) {
				const name = pseudo[index];
				if (name === 'content') continue;
				declarations.push(`${name}:${pseudo.getPropertyValue(name)}`);
			}
			span.style.cssText = declarations.join(';');
			span.textContent = text;
			element.setAttribute(attribute, '');
			if (where === '::before') element.prepend(span);
			else element.append(span);
		}
		if (style.display === 'list-item' && style.listStyleImage === 'none') {
			const marker = markerText(style.listStyleType, ordinals.get(element) ?? 1);
			if (!marker) continue;
			const look = view.getComputedStyle(element, '::marker');
			const span = doc.createElement('plico-text');
			const font = `color:${look.color};font-weight:${look.fontWeight};font-style:${look.fontStyle};font-size:${look.fontSize};white-space:pre;text-decoration:none;`;
			span.textContent = marker;
			if (style.listStylePosition === 'inside') {
				span.style.cssText = `display:inline;${font}`;
				element.prepend(span);
			} else {
				// Out of the flow at the start of the first line, ending where
				// the line starts. Inline, so it sits on that line rather than
				// above it.
				span.style.cssText = `position:absolute;display:inline-block;box-sizing:border-box;width:20em;margin-inline-start:-20em;text-align:end;text-indent:0;${font}`;
				const first = firstLine(doc, element);
				if (first) first.before(span);
				else element.prepend(span);
			}
			(element as HTMLElement).style.setProperty('list-style-type', 'none', 'important');
		}
	}
}

/// Where a list item's first line starts: its first visible text or image.
function firstLine(doc: Document, item: Element) {
	const walker = doc.createTreeWalker(item, NodeFilter.SHOW_TEXT | NodeFilter.SHOW_ELEMENT);
	for (let node = walker.nextNode(); node; node = walker.nextNode()) {
		if (node.nodeType === Node.TEXT_NODE) {
			if (/\S/.test((node as Text).data)) return node as ChildNode;
		} else if ((node as Element).tagName === 'IMG') return node as ChildNode;
		else if (/^(UL|OL)$/.test((node as Element).tagName)) return null;
	}
	return null;
}

function listOrdinals(doc: Document) {
	const ordinals = new Map<Element, number>();
	for (const list of doc.querySelectorAll('ol, ul, menu')) {
		const items = [...list.children].filter((child) => child.tagName === 'LI');
		const reversed = list.tagName === 'OL' && list.hasAttribute('reversed');
		const startAttribute = Number.parseInt(list.getAttribute('start') ?? '', 10);
		let value = Number.isFinite(startAttribute) ? startAttribute : reversed ? items.length : 1;
		for (const item of items) {
			const own = Number.parseInt(item.getAttribute('value') ?? '', 10);
			if (Number.isFinite(own)) value = own;
			ordinals.set(item, value);
			value += reversed ? -1 : 1;
		}
	}
	return ordinals;
}

function markerText(type: string, ordinal: number): string | null {
	if (/^["']/.test(type)) return type.slice(1, -1);
	const alphabet = (letters: string) => {
		if (ordinal < 1) return String(ordinal);
		let text = '';
		for (let rest = ordinal; rest > 0; rest = Math.floor((rest - 1) / letters.length))
			text = letters[(rest - 1) % letters.length] + text;
		return text;
	};
	const roman = (value: number) => {
		if (value < 1 || value > 3999) return String(value);
		const steps: [number, string][] = [
			[1000, 'm'],
			[900, 'cm'],
			[500, 'd'],
			[400, 'cd'],
			[100, 'c'],
			[90, 'xc'],
			[50, 'l'],
			[40, 'xl'],
			[10, 'x'],
			[9, 'ix'],
			[5, 'v'],
			[4, 'iv'],
			[1, 'i']
		];
		let text = '';
		for (const [amount, letters] of steps) for (; value >= amount; value -= amount) text += letters;
		return text;
	};
	const latin = 'abcdefghijklmnopqrstuvwxyz';
	switch (type) {
		case 'none':
			return null;
		case 'disc':
			return '• ';
		case 'circle':
			return '◦ ';
		case 'square':
			return '▪ ';
		case 'disclosure-open':
			return '▾ ';
		case 'disclosure-closed':
			return '▸ ';
		case 'decimal-leading-zero':
			return `${ordinal < 10 && ordinal >= 0 ? '0' : ''}${ordinal}. `;
		case 'lower-alpha':
		case 'lower-latin':
			return `${alphabet(latin)}. `;
		case 'upper-alpha':
		case 'upper-latin':
			return `${alphabet(latin).toUpperCase()}. `;
		case 'lower-roman':
			return `${roman(ordinal)}. `;
		case 'upper-roman':
			return `${roman(ordinal).toUpperCase()}. `;
		case 'lower-greek':
			return `${alphabet('αβγδεζηθικλμνξοπρστυφχψω')}. `;
		default:
			return `${ordinal}. `;
	}
}

/// What generated content says, or null for nothing or anything this cannot
/// write out, such as counters.
function generatedText(content: string, element: Element): string | null {
	if (!content || content === 'none' || content === 'normal') return null;
	let text = '';
	const pattern =
		/\s*("(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|attr\(\s*([\w-]+)\s*\)|open-quote|close-quote|no-open-quote|no-close-quote)\s*/gy;
	let consumed = 0;
	for (let match = pattern.exec(content); match; match = pattern.exec(content)) {
		consumed = pattern.lastIndex;
		const [, token, attribute] = match;
		if (attribute) text += element.getAttribute(attribute) ?? '';
		else if (token === 'open-quote') text += '“';
		else if (token === 'close-quote') text += '”';
		else if (token.startsWith('no-')) continue;
		else
			text += token
				.slice(1, -1)
				.replace(/\\([0-9a-f]{1,6})\s?/gi, (_, hex: string) =>
					String.fromCodePoint(Number.parseInt(hex, 16))
				)
				.replace(/\\(.)/g, '$1');
	}
	return consumed === content.length ? text : null;
}

/// The nearest bundled family for a CSS font stack, by its first font.
function classify(stack: string): FontFamily {
	const first = (stack.split(',')[0] ?? '')
		.trim()
		.replace(/^["']|["']$/g, '')
		.toLowerCase();
	if (
		first === 'monospace' ||
		/mono|courier|consol|menlo|monaco|code|terminal|typewriter|lucida console|andale|fixed/.test(
			first
		)
	)
		return 'courier';
	if (
		first === 'serif' ||
		(!first.includes('sans') &&
			/serif|times|georgia|garamond|cambria|palatino|baskerville|antiqua|bookman|didot|bodoni|charter|merriweather|lora|minion|caslon|constantia|century|roman|crimson|playfair|mincho|song/.test(
				first
			))
	)
		return 'times';
	return 'helvetica';
}

function familyName(face: Face) {
	return `Plico ${face.group}`;
}

/// Each element's family, noted before every element is switched to the
/// bundled fonts.
function setFonts(doc: Document, view: Window, characters: string) {
	const of = new Map<Element, FontFamily>();
	const used = new Set<`${FontFamily}:${boolean}`>();
	const elements = [doc.documentElement, ...doc.documentElement.querySelectorAll('*')].filter(
		(element) => !SKIPPED.has(element.tagName)
	);
	const styles = elements.map((element) => view.getComputedStyle(element));
	elements.forEach((element, index) => {
		const family = classify(styles[index].fontFamily);
		of.set(element, family);
		used.add(`${family}:${Number.parseInt(styles[index].fontWeight, 10) >= 600}`);
	});
	const stacks = Object.fromEntries(
		(['helvetica', 'times', 'courier'] as const).map((family) => [
			family,
			facesFor(characters, family).map(familyName).map(quote).join(', ')
		])
	) as Record<FontFamily, string>;
	elements.forEach((element, index) => {
		const target = element as HTMLElement;
		target.style?.setProperty('font-family', stacks[of.get(element)!], 'important');
		// A weight between the two the fonts have takes the nearer, as the
		// engine does.
		const weight = Number.parseInt(styles[index].fontWeight, 10);
		if (weight !== 400 && weight !== 700)
			target.style?.setProperty('font-weight', weight >= 600 ? '700' : '400', 'important');
		target.style?.setProperty('font-variant', 'normal', 'important');
		target.style?.setProperty('font-feature-settings', 'normal', 'important');
	});
	return {
		of,
		used: [...used].map((entry) => {
			const [family, bold] = entry.split(':');
			return { family: family as FontFamily, bold: bold === 'true' };
		})
	};
}

function quote(name: string) {
	return `'${name}'`;
}

function uniqueCharacters(doc: Document) {
	const seen = new Set<string>();
	const walker = doc.createTreeWalker(doc.body ?? doc.documentElement, NodeFilter.SHOW_TEXT);
	for (let node = walker.nextNode(); node; node = walker.nextNode()) {
		const parent = node.parentElement;
		if (parent && (SKIPPED.has(parent.tagName) || parent.closest('svg'))) continue;
		for (const character of (node as Text).data) seen.add(character);
	}
	return [...seen].join('');
}

/// What the engine needs to fetch the same fonts in the same order.
function fontItems(characters: string, used: { family: FontFamily; bold: boolean }[]): TextItem[] {
	const uses = used.length ? used : [{ family: 'helvetica' as const, bold: false }];
	return uses.map(({ family, bold }) => ({ text: characters, family, bold }));
}

const fontFiles = new Map<string, Promise<ArrayBuffer>>();

function fontFile(file: string) {
	let data = fontFiles.get(file);
	if (!data) {
		data = fetch(`/fonts/${file}`).then((response) => {
			if (!response.ok) throw new Error('The fonts for this document could not be loaded.');
			return response.arrayBuffer();
		});
		data.catch(() => fontFiles.delete(file));
		fontFiles.set(file, data);
	}
	return data;
}

async function loadFaces(
	doc: Document,
	view: Window,
	characters: string,
	used: { family: FontFamily; bold: boolean }[]
) {
	const families = new Set(used.map((use) => use.family));
	if (!families.size) families.add('helvetica');
	const faces = new Map<string, Face>();
	for (const family of families)
		for (const face of facesFor(characters, family)) faces.set(face.group, face);
	const Face = (view as unknown as typeof globalThis).FontFace;
	await Promise.all(
		[...faces.values()].flatMap((face) =>
			(
				[
					['400', face.regular],
					['700', face.bold ?? face.regular]
				] as const
			).map(async ([weight, file]) => {
				const font = new Face(familyName(face), await fontFile(file), { weight });
				doc.fonts.add(font);
				await font.load();
			})
		)
	);
	await doc.fonts.ready;
}

type Color = { value: number; alpha: number };

const colorCache = new Map<string, Color>();
let colorProbe: CanvasRenderingContext2D | null | undefined;

function color(text: string): Color {
	const known = colorCache.get(text);
	if (known) return known;
	let parsed: Color = { value: 0, alpha: 0 };
	const rgb = /^rgba?\(\s*([\d.]+)[\s,]+([\d.]+)[\s,]+([\d.]+)(?:[\s,/]+([\d.]+)(%?))?\s*\)$/.exec(
		text
	);
	if (rgb) {
		const channel = (value: string) => Math.max(0, Math.min(255, Math.round(Number(value))));
		const alpha = rgb[4] === undefined ? 1 : Number(rgb[4]) / (rgb[5] ? 100 : 1);
		parsed = {
			value: (channel(rgb[1]) << 16) | (channel(rgb[2]) << 8) | channel(rgb[3]),
			alpha
		};
	} else if (text !== 'transparent') {
		// Other colour spaces, read back as the pixel they paint.
		colorProbe ??= document.createElement('canvas').getContext('2d', { willReadFrequently: true });
		if (colorProbe) {
			colorProbe.clearRect(0, 0, 1, 1);
			colorProbe.fillStyle = text;
			colorProbe.fillRect(0, 0, 1, 1);
			const [red, green, blue, alpha] = colorProbe.getImageData(0, 0, 1, 1).data;
			parsed = { value: (red << 16) | (green << 8) | blue, alpha: alpha / 255 };
		}
	}
	colorCache.set(text, parsed);
	return parsed;
}

type Box = { left: number; top: number; right: number; bottom: number };
type Clip = { page: number; box: Box; radii: number[] } | null;
type Decoration = {
	lines: string;
	color: Color;
	style: string;
	thickness: number | null;
};
type Context = { opacity: number; clip: Clip; decorations: Decoration[] };

type Geometry = {
	width: number;
	height: number;
	gap: number;
	/// Points per CSS pixel of the layout.
	unit: number;
	margin: number;
	pageWidth: number;
	pageHeight: number;
	one: boolean;
	missing: Set<string>;
	families: Map<Element, FontFamily>;
	signal?: AbortSignal;
};

type Piece = { start: number; end: number; box: Box; picture: boolean };

class Reader {
	widest = 0;
	images: ArrayBuffer[] = [];
	private kinds: number[] = [];
	private pages: number[] = [];
	private colors: number[] = [];
	private texts: string[] = [];
	private numbers: number[] = [];
	private headings: PrintLayout['headings'] = [];
	private imageKeys = new Map<string, number>();
	private pending: Promise<void>[] = [];
	private bottoms: number[] = [];
	private ratios = new Map<string, number>();
	private range: Range;
	private work = 0;
	private stride: number;

	constructor(
		private doc: Document,
		private view: Window,
		private geometry: Geometry
	) {
		this.range = doc.createRange();
		this.stride = geometry.width + geometry.gap;
	}

	async read(): Promise<PrintLayout> {
		const { doc, view } = this;
		const root = view.getComputedStyle(doc.documentElement);
		const body = doc.body ? view.getComputedStyle(doc.body) : null;
		const rootColor = color(root.backgroundColor);
		const bodyColor = body ? color(body.backgroundColor) : { value: 0, alpha: 0 };
		// The root's background, or else the body's, paints the whole page.
		const canvas = rootColor.alpha > 0 ? rootColor : bodyColor.alpha > 0 ? bodyColor : null;
		const skipBodyBackground = rootColor.alpha === 0;
		if (doc.body)
			await this.visit(
				doc.body,
				{ opacity: Number(root.opacity) || 1, clip: null, decorations: [] },
				skipBodyBackground
			);
		await Promise.all(this.pending);

		const count = this.pages.reduce((most, page) => Math.max(most, page + 1), 1);
		const { pageWidth, pageHeight, margin, one } = this.geometry;
		const sizes: number[] = [];
		for (let page = 0; page < count; page++) {
			const last = page === count - 1;
			// One long page ends where its content does.
			const height =
				one && last
					? Math.min(pageHeight, Math.max(2 * margin + 1, (this.bottoms[page] ?? 0) + margin))
					: pageHeight;
			sizes.push(pageWidth, height);
		}
		// The page backgrounds paint first.
		const order = [...this.kinds.keys()];
		const backgrounds = canvas
			? Array.from({ length: count }, (_, page) => ({
					page,
					numbers: [
						0,
						0,
						pageWidth,
						sizes[page * 2 + 1],
						0,
						0,
						0,
						0,
						canvas.alpha,
						...Array(8).fill(Number.NaN),
						0,
						0,
						0
					]
				}))
			: [];
		const total = backgrounds.length + order.length;
		const layout: PrintLayout = {
			pages: Float32Array.from(sizes),
			kinds: new Uint8Array(total),
			itemPages: new Uint32Array(total),
			colors: new Uint32Array(total),
			texts: [],
			numbers: new Float32Array(total * PRINT_STRIDE),
			headings: this.headings,
			title: doc.title.trim() || this.headings[0]?.title || '',
			language: doc.documentElement.lang || ''
		};
		backgrounds.forEach((background, index) => {
			layout.kinds[index] = KIND.fill;
			layout.itemPages[index] = background.page;
			layout.colors[index] = canvas!.value;
			layout.texts.push('');
			layout.numbers.set(background.numbers, index * PRINT_STRIDE);
		});
		const offset = backgrounds.length;
		layout.kinds.set(this.kinds, offset);
		layout.itemPages.set(this.pages, offset);
		layout.colors.set(this.colors, offset);
		layout.texts.push(...this.texts);
		layout.numbers.set(this.numbers, offset * PRINT_STRIDE);
		return layout;
	}

	private async pause() {
		if (++this.work % 400 !== 0) return;
		await new Promise((resolve) => setTimeout(resolve));
		check(this.geometry.signal);
	}

	/// The page a box starts on.
	private pageOf(left: number) {
		return Math.max(0, Math.floor((left + this.geometry.gap / 2) / this.stride));
	}

	/// A box in points on its page.
	private place(box: Box, page: number) {
		const { unit, margin } = this.geometry;
		const shift = page * this.stride;
		return {
			left: (box.left - shift) * unit + margin,
			top: box.top * unit + margin,
			width: (box.right - box.left) * unit,
			height: (box.bottom - box.top) * unit
		};
	}

	private add(
		kind: number,
		page: number,
		box: Box,
		values: {
			geometry?: number[];
			radii?: number[];
			opacity: number;
			clip: Clip;
			color?: number;
			text?: string;
			extra?: [number, number, number];
		}
	) {
		const placed = this.place(box, page);
		this.widest = Math.max(
			this.widest,
			Math.min(
				box.right,
				values.clip && values.clip.page === page ? values.clip.box.right : Infinity
			) -
				page * this.stride
		);
		const { unit } = this.geometry;
		this.bottoms[page] = Math.max(this.bottoms[page] ?? 0, placed.top + placed.height);
		this.kinds.push(kind);
		this.pages.push(page);
		this.colors.push(values.color ?? 0);
		this.texts.push(values.text ?? '');
		const clip =
			values.clip && values.clip.page === page
				? [
						...Object.values(this.place(values.clip.box, page)),
						...values.clip.radii.map((radius) => radius * unit)
					]
				: Array(8).fill(Number.NaN);
		this.numbers.push(
			...(values.geometry ?? [placed.left, placed.top, placed.width, placed.height]),
			...(values.radii ?? [0, 0, 0, 0]).map((radius) => radius * unit),
			values.opacity,
			...clip,
			...(values.extra ?? [0, 0, 0])
		);
	}

	private async visit(element: Element, outer: Context, skipBackground = false) {
		if (SKIPPED.has(element.tagName)) return;
		const style = this.view.getComputedStyle(element);
		if (style.display === 'none') return;
		const opacity = outer.opacity * (Number(style.opacity) || 0);
		if (opacity <= 0) return;
		const visible = style.visibility === 'visible';
		const rects = [...element.getClientRects()].filter((rect) => rect.width > 0 || rect.height > 0);
		const tag = element.tagName.toUpperCase();
		if (visible && rects.length) {
			if (!skipBackground) this.background(style, rects, opacity, outer.clip);
			this.borders(element, style, rects, opacity, outer.clip);
		}
		if (REPLACED.has(tag)) {
			if (visible && rects.length)
				await this.replaced(element, style, rects[0], opacity, outer.clip);
			return;
		}
		if (visible && rects.length) {
			if (tag === 'A') this.link(element as HTMLAnchorElement, rects);
			if (/^H[1-6]$/.test(tag)) this.heading(element, rects[0]);
		}
		const context: Context = {
			opacity,
			clip: this.clipFor(style, rects, outer.clip),
			decorations: this.decorationsFor(style, outer.decorations)
		};
		for (const child of element.childNodes) {
			if (child.nodeType === Node.TEXT_NODE)
				await this.text(child as Text, element, style, context);
			else if (child.nodeType === Node.ELEMENT_NODE) await this.visit(child as Element, context);
		}
	}

	private clipFor(style: CSSStyleDeclaration, rects: DOMRect[], outer: Clip): Clip {
		if ((style.overflowX === 'visible' && style.overflowY === 'visible') || rects.length !== 1)
			return outer;
		const rect = rects[0];
		const page = this.pageOf(rect.left);
		const border = (side: string) =>
			Number.parseFloat(style.getPropertyValue(`border-${side}-width`)) || 0;
		let box: Box = {
			left: style.overflowX === 'visible' ? -1e6 : rect.left + border('left'),
			top: style.overflowY === 'visible' ? -1e6 : rect.top + border('top'),
			right: style.overflowX === 'visible' ? 1e6 : rect.right - border('right'),
			bottom: style.overflowY === 'visible' ? 1e6 : rect.bottom - border('bottom')
		};
		if (outer && outer.page === page)
			box = {
				left: Math.max(box.left, outer.box.left),
				top: Math.max(box.top, outer.box.top),
				right: Math.min(box.right, outer.box.right),
				bottom: Math.min(box.bottom, outer.box.bottom)
			};
		const column = page * this.stride;
		box = {
			left: Math.max(box.left, column - this.geometry.gap / 2),
			top: Math.max(box.top, -this.geometry.margin / this.geometry.unit),
			right: Math.min(box.right, column + this.geometry.width + this.geometry.gap / 2),
			bottom: Math.min(box.bottom, this.geometry.height + this.geometry.margin / this.geometry.unit)
		};
		return { page, box, radii: radii(style, rect) };
	}

	private decorationsFor(style: CSSStyleDeclaration, outer: Decoration[]) {
		const lines = style.textDecorationLine;
		if (!lines || lines === 'none') return outer;
		const thickness = Number.parseFloat(style.textDecorationThickness);
		return [
			...outer,
			{
				lines,
				color: color(style.textDecorationColor),
				style: style.textDecorationStyle,
				thickness: Number.isFinite(thickness) ? thickness : null
			}
		];
	}

	private background(style: CSSStyleDeclaration, rects: DOMRect[], opacity: number, clip: Clip) {
		const fill = color(style.backgroundColor);
		if (fill.alpha <= 0) return;
		for (const rect of rects) {
			const box = { left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom };
			this.add(KIND.fill, this.pageOf(rect.left), box, {
				radii: rects.length === 1 ? radii(style, rect) : undefined,
				opacity: opacity * fill.alpha,
				clip,
				color: fill.value
			});
		}
	}

	private borders(
		element: Element,
		style: CSSStyleDeclaration,
		rects: DOMRect[],
		opacity: number,
		clip: Clip
	) {
		const sides = (['top', 'right', 'bottom', 'left'] as const).map((side) => {
			const width = Number.parseFloat(style.getPropertyValue(`border-${side}-width`)) || 0;
			const kind = style.getPropertyValue(`border-${side}-style`);
			const paint = color(style.getPropertyValue(`border-${side}-color`));
			return {
				width,
				kind,
				paint,
				shown: width > 0 && kind !== 'none' && kind !== 'hidden' && paint.alpha > 0
			};
		});
		if (!sides.some((side) => side.shown)) return;
		// Collapsed table borders are shared, centred on the line between
		// cells; any other border lies inside its box.
		const collapsed =
			style.borderCollapse === 'collapse' &&
			/^(TABLE|TR|TD|TH|THEAD|TBODY|TFOOT)$/.test(element.tagName);
		const inline = style.display === 'inline' && rects.length > 1;
		const [top, right, bottom, left] = sides;
		const uniform = sides.every(
			(side) =>
				side.shown &&
				side.width === top.width &&
				side.kind === top.kind &&
				side.paint.value === top.paint.value &&
				side.paint.alpha === top.paint.alpha
		);
		rects.forEach((rect, index) => {
			const first = index === 0;
			const last = index === rects.length - 1;
			const page = this.pageOf(rect.left);
			if (uniform && rects.length === 1) {
				const inset = collapsed ? 0 : top.width / 2;
				const box = {
					left: rect.left + inset,
					top: rect.top + inset,
					right: rect.right - inset,
					bottom: rect.bottom - inset
				};
				this.add(KIND.outline, page, box, {
					radii: radii(style, rect).map((radius) => Math.max(0, radius - inset)),
					opacity: opacity * top.paint.alpha,
					clip,
					color: top.paint.value,
					extra: [top.width * this.geometry.unit, DASH[top.kind] ?? 0, 0]
				});
				return;
			}
			const draw = (side: (typeof sides)[number], from: [number, number], to: [number, number]) => {
				if (!side.shown) return;
				const { unit, margin } = this.geometry;
				const shift = page * this.stride;
				const point = ([x, y]: [number, number]) => [
					(x - shift) * unit + margin,
					y * unit + margin
				];
				this.add(
					KIND.line,
					page,
					{ left: from[0], top: from[1], right: to[0], bottom: to[1] },
					{
						geometry: [...point(from), ...point(to)],
						opacity: opacity * side.paint.alpha,
						clip,
						color: side.paint.value,
						extra: [side.width * unit, DASH[side.kind] ?? 0, 0]
					}
				);
			};
			const half = (side: (typeof sides)[number]) => (collapsed ? 0 : side.width / 2);
			if (inline || first || rects.length === 1)
				draw(top, [rect.left, rect.top + half(top)], [rect.right, rect.top + half(top)]);
			if (inline || last || rects.length === 1)
				draw(
					bottom,
					[rect.left, rect.bottom - half(bottom)],
					[rect.right, rect.bottom - half(bottom)]
				);
			if (!inline || first)
				draw(left, [rect.left + half(left), rect.top], [rect.left + half(left), rect.bottom]);
			if (!inline || last)
				draw(right, [rect.right - half(right), rect.top], [rect.right - half(right), rect.bottom]);
		});
	}

	private link(anchor: HTMLAnchorElement, rects: DOMRect[]) {
		const href = anchor.getAttribute('href')?.trim();
		if (!href) return;
		let target: { uri: string } | { page: number; top: number } | null = null;
		if (href.startsWith('#')) {
			const id = decodeURIComponent(href.slice(1));
			const destination =
				(id && this.doc.getElementById(id)) ||
				this.doc.querySelector(`a[name="${CSS.escape(id)}"]`);
			const box = destination?.getClientRects()[0];
			if (box) {
				const page = this.pageOf(box.left);
				target = { page, top: this.place(box, page).top };
			} else if (!id || id === 'top') target = { page: 0, top: 0 };
		} else if (/^(https?|mailto|tel|ftp):/i.test(href)) {
			try {
				target = { uri: new URL(href).href };
			} catch {
				target = null;
			}
		}
		if (!target) return;
		for (const rect of rects) {
			if (rect.width <= 0 || rect.height <= 0) continue;
			const page = this.pageOf(rect.left);
			const box = { left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom };
			this.add(KIND.link, page, box, {
				opacity: 1,
				clip: null,
				text: 'uri' in target ? target.uri : '',
				extra: 'uri' in target ? [0, 0, Number.NaN] : [target.top, 0, target.page]
			});
		}
	}

	private heading(element: Element, rect: DOMRect) {
		const title = (element.textContent ?? '').replace(/\s+/g, ' ').trim();
		if (!title) return;
		const page = this.pageOf(rect.left);
		this.headings.push({
			level: Number(element.tagName[1]),
			title,
			page,
			top: this.place(rect, page).top
		});
	}

	private async replaced(
		element: Element,
		style: CSSStyleDeclaration,
		rect: DOMRect,
		opacity: number,
		clip: Clip
	) {
		const border = (side: string) =>
			(Number.parseFloat(style.getPropertyValue(`border-${side}-width`)) || 0) +
			(Number.parseFloat(style.getPropertyValue(`padding-${side}`)) || 0);
		const content: Box = {
			left: rect.left + border('left'),
			top: rect.top + border('top'),
			right: rect.right - border('right'),
			bottom: rect.bottom - border('bottom')
		};
		if (content.right <= content.left || content.bottom <= content.top) return;
		const page = this.pageOf(rect.left);
		const tag = element.tagName.toUpperCase();
		let source: () => Promise<ArrayBuffer | null>;
		let key: string;
		let natural: [number, number] | null = null;
		if (tag === 'IMG') {
			const image = element as HTMLImageElement;
			if (!image.complete || !image.naturalWidth) return;
			key = image.currentSrc || image.src;
			natural = [image.naturalWidth, image.naturalHeight];
			source = () => imageBytes(image, content);
		} else if (tag === 'SVG') {
			key = `svg:${this.images.length}`;
			source = () => svgBytes(element as SVGSVGElement, content, style.color);
		} else return;
		// Within its box the picture fits as object-fit says; anything outside
		// the box is cut off.
		let area = content;
		const fit = style.objectFit;
		if (natural && fit && fit !== 'fill') {
			const [width, height] = natural;
			const boxWidth = content.right - content.left;
			const boxHeight = content.bottom - content.top;
			const scale =
				fit === 'cover'
					? Math.max(boxWidth / width, boxHeight / height)
					: fit === 'none'
						? 1
						: fit === 'scale-down'
							? Math.min(1, boxWidth / width, boxHeight / height)
							: Math.min(boxWidth / width, boxHeight / height);
			const drawn = [width * scale, height * scale];
			const left = content.left + (boxWidth - drawn[0]) / 2;
			const top = content.top + (boxHeight - drawn[1]) / 2;
			area = { left, top, right: left + drawn[0], bottom: top + drawn[1] };
		}
		const rounded = radii(style, rect);
		const ownClip: Clip =
			area !== content || rounded.some((radius) => radius > 0)
				? { page, box: content, radii: rounded }
				: clip;
		let index = this.imageKeys.get(key);
		if (index === undefined) {
			const bytes = await source();
			if (!bytes) return;
			index = this.images.length;
			this.images.push(bytes);
			this.imageKeys.set(key, index);
		}
		this.add(KIND.image, page, area, {
			opacity,
			clip: ownClip,
			extra: [0, 0, index]
		});
	}

	private ratio(family: FontFamily, bold: boolean, stack: string) {
		const key = `${family}:${bold}`;
		const known = this.ratios.get(key);
		if (known !== undefined) return known;
		const { doc } = this;
		const probe = doc.createElement('plico-probe');
		probe.style.cssText = `position:fixed;left:0;top:0;display:block;white-space:nowrap;line-height:normal;font-size:100px;font-weight:${bold ? 700 : 400};font-family:${stack}`;
		const text = doc.createTextNode('Hxg');
		const mark = doc.createElement('span');
		mark.style.cssText = 'display:inline-block;width:0;height:0;vertical-align:baseline';
		probe.append(text, mark);
		doc.documentElement.append(probe);
		this.range.selectNodeContents(text);
		const box = this.range.getBoundingClientRect();
		const baseline = mark.getBoundingClientRect().bottom;
		probe.remove();
		const ratio = box.height > 0 ? (baseline - box.top) / box.height : 0.8;
		this.ratios.set(key, ratio);
		return ratio;
	}

	private measure(node: Text, start: number, end: number): Box[] {
		this.range.setStart(node, start);
		this.range.setEnd(node, end);
		return [...this.range.getClientRects()]
			.filter((rect) => rect.width > 0 && rect.height > 0)
			.map((rect) => ({ left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom }));
	}

	private async text(node: Text, element: Element, style: CSSStyleDeclaration, context: Context) {
		const value = node.data;
		if (!/\S/.test(value) || style.visibility !== 'visible') return;
		const paint = color(style.color);
		const opacity = context.opacity * paint.alpha;
		if (opacity <= 0) return;
		const size = Number.parseFloat(style.fontSize);
		if (!(size > 0.5)) return;
		const family = this.geometry.families.get(element) ?? 'helvetica';
		const bold = Number.parseInt(style.fontWeight, 10) >= 600;
		const italic = style.fontStyle !== 'normal';
		const preserve = /^(pre|pre-wrap|break-spaces)$/.test(style.whiteSpace);
		const spacing = Number.parseFloat(style.letterSpacing) || 0;
		const apart =
			style.textAlign === 'justify' || (Number.parseFloat(style.wordSpacing) || 0) !== 0;

		const pieces: Piece[] = [];
		const { missing } = this.geometry;
		for (const segment of segmenter.segment(value)) {
			if (!/\S/.test(segment.segment)) continue;
			await this.pause();
			const start = segment.index;
			const end = start + segment.segment.length;
			const boxes = this.measure(node, start, end);
			if (!boxes.length) continue;
			const oneLine = boxes.every((box) => Math.abs(box.top - boxes[0].top) < 1);
			const hasMissing = missing.size > 0 && [...segment.segment].some((c) => missing.has(c));
			if (oneLine && !hasMissing) {
				pieces.push({ start, end, box: union(boxes), picture: false });
				continue;
			}
			// Across a line break, or holding a character no font has: one
			// piece per character, put back together by line below.
			for (const grapheme of graphemes.segment(segment.segment)) {
				const from = start + grapheme.index;
				const to = from + grapheme.segment.length;
				const parts = this.measure(node, from, to);
				if (!parts.length || !/\S/.test(grapheme.segment)) continue;
				const picture = [...grapheme.segment].some((c) => missing.has(c));
				pieces.push({ start: from, end: to, box: union(parts), picture });
			}
		}
		if (!pieces.length) return;

		const stack = style.fontFamily;
		const ratio = this.ratio(family, bold, stack);
		const runs: { start: number; end: number; text: string; box: Box }[] = [];
		let afterPicture = false;
		for (const piece of pieces) {
			if (piece.picture) {
				this.picture(
					value.slice(piece.start, piece.end),
					piece.box,
					style,
					ratio,
					opacity,
					context
				);
				afterPicture = true;
				continue;
			}
			const last = afterPicture ? undefined : runs.at(-1);
			afterPicture = false;
			const between = last ? value.slice(last.end, piece.start) : '';
			// Pieces join while they follow on along one line, unless spaces
			// were stretched between them. Right-to-left words run the other
			// way, so each stays its own.
			const joinable =
				last &&
				Math.abs(last.box.top - piece.box.top) < 0.5 * (piece.box.bottom - piece.box.top) &&
				this.pageOf(last.box.left) === this.pageOf(piece.box.left) &&
				piece.box.left >= last.box.right - 1 &&
				(between === '' || (!apart && (!preserve || /^[ \u00a0]+$/.test(between))));
			if (last && joinable) {
				last.text +=
					(preserve ? between : between ? ' ' : '') + value.slice(piece.start, piece.end);
				last.end = piece.end;
				last.box = union([last.box, piece.box]);
			} else
				runs.push({
					start: piece.start,
					end: piece.end,
					text: value.slice(piece.start, piece.end),
					box: piece.box
				});
		}

		const { unit } = this.geometry;
		const language = element.closest('[lang]')?.getAttribute('lang') || undefined;
		for (const run of runs) {
			const text = transform(run.text, style.textTransform, language);
			const page = this.pageOf(run.box.left);
			const placed = this.place(run.box, page);
			const baseline = run.box.top + (run.box.bottom - run.box.top) * ratio;
			this.add(KIND.text, page, run.box, {
				geometry: [
					placed.left,
					placed.top + (baseline - run.box.top) * unit,
					size * unit,
					placed.width
				],
				opacity,
				clip: context.clip,
				color: paint.value,
				text,
				extra: [spacing * unit, FAMILY[family], (bold ? 1 : 0) | (italic ? 2 : 0)]
			});
			this.decorate(run.box, baseline, size, page, context);
		}
	}

	/// Underlines, overlines and lines through, in the font's own places:
	/// Noto puts the underline 0.1 em below the baseline and the strike
	/// 0.32 em above, both 0.05 em thick.
	private decorate(box: Box, baseline: number, size: number, page: number, context: Context) {
		const { unit, margin } = this.geometry;
		const shift = page * this.stride;
		for (const decoration of context.decorations) {
			if (decoration.color.alpha <= 0) continue;
			const thickness = Math.max(1, decoration.thickness ?? size * 0.05);
			for (const line of decoration.lines.split(/\s+/)) {
				const y =
					line === 'underline'
						? baseline + size * 0.1 + thickness / 2
						: line === 'overline'
							? box.top + thickness / 2
							: line === 'line-through'
								? baseline - size * 0.32
								: null;
				if (y === null) continue;
				this.add(
					KIND.line,
					page,
					{ left: box.left, top: y, right: box.right, bottom: y },
					{
						geometry: [
							(box.left - shift) * unit + margin,
							y * unit + margin,
							(box.right - shift) * unit + margin,
							y * unit + margin
						],
						opacity: context.opacity * decoration.color.alpha,
						clip: context.clip,
						color: decoration.color.value,
						extra: [thickness * unit, DASH[decoration.style] ?? 0, 0]
					}
				);
			}
		}
	}

	/// What no bundled font draws, an emoji most often, as a picture of how
	/// the browser drew it.
	private picture(
		text: string,
		box: Box,
		style: CSSStyleDeclaration,
		ratio: number,
		opacity: number,
		context: Context
	) {
		const key = `glyph:${text}:${style.color}:${style.fontWeight}:${style.fontStyle}:${Math.round(box.bottom - box.top)}:${Math.round(box.right - box.left)}`;
		const page = this.pageOf(box.left);
		const place = (index: number) =>
			this.add(KIND.image, page, box, { opacity, clip: context.clip, extra: [0, 0, index] });
		const known = this.imageKeys.get(key);
		if (known !== undefined) {
			place(known);
			return;
		}
		const index = this.images.length;
		this.images.push(new ArrayBuffer(0));
		this.imageKeys.set(key, index);
		place(index);
		this.pending.push(
			glyphPicture(text, box, style, ratio).then((bytes) => {
				this.images[index] = bytes;
			})
		);
	}
}

function union(boxes: Box[]): Box {
	return {
		left: Math.min(...boxes.map((box) => box.left)),
		top: Math.min(...boxes.map((box) => box.top)),
		right: Math.max(...boxes.map((box) => box.right)),
		bottom: Math.max(...boxes.map((box) => box.bottom))
	};
}

function radii(style: CSSStyleDeclaration, rect: DOMRect | Box) {
	const width = 'width' in rect ? rect.width : rect.right - rect.left;
	return ['top-left', 'top-right', 'bottom-right', 'bottom-left'].map((corner) => {
		const value = style.getPropertyValue(`border-${corner}-radius`).split(/\s+/)[0] ?? '0';
		const number = Number.parseFloat(value) || 0;
		return value.endsWith('%') ? (number / 100) * width : number;
	});
}

function transform(text: string, how: string, language?: string) {
	if (how === 'uppercase') return text.toLocaleUpperCase(language);
	if (how === 'lowercase') return text.toLocaleLowerCase(language);
	if (how === 'capitalize')
		return text.replace(
			/(^|[\s\p{P}])(\p{L})/gu,
			(_, before: string, letter: string) => before + letter.toLocaleUpperCase(language)
		);
	return text;
}

function isJpegOrPng(bytes: Uint8Array) {
	return (
		(bytes[0] === 0xff && bytes[1] === 0xd8) ||
		(bytes[0] === 0x89 && bytes[1] === 0x50 && bytes[2] === 0x4e && bytes[3] === 0x47)
	);
}

function canvasPng(canvas: HTMLCanvasElement) {
	return new Promise<ArrayBuffer | null>((resolve) =>
		canvas.toBlob((blob) => (blob ? blob.arrayBuffer().then(resolve) : resolve(null)), 'image/png')
	);
}

/// An image's own bytes when they are a JPG or PNG, otherwise the picture
/// drawn again as a PNG.
async function imageBytes(image: HTMLImageElement, box: Box): Promise<ArrayBuffer | null> {
	const source = image.currentSrc || image.src;
	if (/^data:/i.test(source)) {
		try {
			const bytes = await (await fetch(source)).arrayBuffer();
			if (isJpegOrPng(new Uint8Array(bytes))) return bytes;
		} catch {
			// Drawn again below.
		}
	}
	const vector = /^data:image\/svg/i.test(source);
	const largest = 4096;
	let width = vector ? (box.right - box.left) * 3 : image.naturalWidth;
	let height = vector ? (box.bottom - box.top) * 3 : image.naturalHeight;
	const shrink = Math.min(1, largest / Math.max(width, height));
	width = Math.max(1, Math.round(width * shrink));
	height = Math.max(1, Math.round(height * shrink));
	const canvas = document.createElement('canvas');
	canvas.width = width;
	canvas.height = height;
	try {
		canvas.getContext('2d')?.drawImage(image, 0, 0, width, height);
		return await canvasPng(canvas);
	} catch {
		return null;
	}
}

/// An inline SVG drawn as a picture at three times its size.
async function svgBytes(
	svg: SVGSVGElement,
	box: Box,
	currentColor: string
): Promise<ArrayBuffer | null> {
	const width = box.right - box.left;
	const height = box.bottom - box.top;
	const copy = svg.cloneNode(true) as SVGSVGElement;
	copy.setAttribute('xmlns', 'http://www.w3.org/2000/svg');
	copy.setAttribute('width', String(width));
	copy.setAttribute('height', String(height));
	copy.style.color = currentColor;
	const markup = new XMLSerializer().serializeToString(copy);
	const image = new Image();
	image.src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(markup)}`;
	try {
		await image.decode();
	} catch {
		return null;
	}
	const canvas = document.createElement('canvas');
	canvas.width = Math.max(1, Math.min(4096, Math.round(width * 3)));
	canvas.height = Math.max(1, Math.min(4096, Math.round(height * 3)));
	canvas.getContext('2d')?.drawImage(image, 0, 0, canvas.width, canvas.height);
	return canvasPng(canvas);
}

async function glyphPicture(
	text: string,
	box: Box,
	style: CSSStyleDeclaration,
	ratio: number
): Promise<ArrayBuffer> {
	const width = box.right - box.left;
	const height = box.bottom - box.top;
	const canvas = document.createElement('canvas');
	canvas.width = Math.max(1, Math.ceil(width * PICTURE_SCALE));
	canvas.height = Math.max(1, Math.ceil(height * PICTURE_SCALE));
	const context = canvas.getContext('2d');
	if (context) {
		const size = Number.parseFloat(style.fontSize) * PICTURE_SCALE;
		context.font = `${style.fontStyle} ${style.fontWeight} ${size}px sans-serif`;
		context.fillStyle = style.color;
		context.textBaseline = 'alphabetic';
		const drawn = context.measureText(text).width;
		const squeeze = drawn > canvas.width ? canvas.width / drawn : 1;
		context.setTransform(squeeze, 0, 0, 1, 0, 0);
		context.fillText(
			text,
			Math.max(0, (canvas.width - drawn * squeeze) / 2),
			height * ratio * PICTURE_SCALE
		);
	}
	return (await canvasPng(canvas)) ?? blank();
}

/// A transparent pixel, for a picture the browser would not make.
function blank() {
	return Uint8Array.from(
		atob(
			'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII='
		),
		(character) => character.charCodeAt(0)
	).buffer;
}
