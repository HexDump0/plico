// Translate PDF's state: the languages, what each page says and its
// translation, and the run that translates pages and writes them. Pages are
// read once, translated paragraph by paragraph (each distinct paragraph once,
// so a running head is translated once for the whole document), and written
// through Edit: old text out, the translation drawn where it was.

import { SvelteMap } from 'svelte/reactivity';
import { fontsLoaded } from './font-faces.svelte';
import { parsePageRange } from './page-range';
import { processEdit, processOrganizePdf } from './processor';
import { sourceKey } from './sources';
import type { FontFamily } from './standard-fonts';
import {
	detectLanguage,
	modelPairs,
	preferredTarget,
	translateLanguage,
	TranslateModels
} from './translate.svelte';
import { Translator } from './translator';
import {
	blockRemovals,
	pageBlocks,
	setPage,
	translationMark,
	unchanged,
	type SetBlock,
	type TranslateBlock
} from './translate-layout';
import type { CoveredPage, PageAnnotation, PageGlyphs, PdfOutput, TextRemoval } from './types';

export type TranslateStage = 'models' | 'starting' | 'translating' | 'writing';

// Paragraphs sent to the translator at once: enough to fill its batches,
// few enough that a page shows its progress.
const CHUNK = 16;

const cancelled = () => new DOMException('Cancelled', 'AbortError');

export class TranslateDocument {
	readonly models = new TranslateModels();
	private translator = new Translator();

	from = $state('en');
	to = $state(preferredTarget('en'));
	pages = $state('');
	keepOriginal = $state(false);
	/// The preview shows the pages as they were.
	showOriginal = $state(false);
	/// What the document was found to be in.
	detected = $state<string>();

	/// The engine's glyphs and each page's visible size in points, once read.
	glyphs = $state.raw<PageGlyphs[] | null>(null);
	sizes = $state.raw<{ width: number; height: number }[]>([]);

	/// Translations by models and paragraph text, kept across runs.
	private translations = new SvelteMap<string, string>();
	/// Translations corrected by hand, by models and block.
	private edits = new SvelteMap<string, string>();
	/// The paper behind each block, sampled from the preview.
	private covers: Record<string, number> = {};

	stage = $state<TranslateStage>('models');
	done = $state(0);
	total = $state(0);
	/// Pages whose paragraphs are with the translator.
	active = $state.raw<number[]>([]);

	readonly pairs = $derived(modelPairs(this.from, this.to));
	private readonly key = $derived(this.pairs.join('>'));
	readonly rtl = $derived(!!translateLanguage(this.to)?.rtl);
	readonly blocks = $derived.by(() => {
		const glyphs = this.glyphs;
		if (!glyphs || this.sizes.length !== glyphs.length) return [];
		return glyphs.map((page, index) =>
			pageBlocks(page, index + 1, this.sizes[index].width, this.sizes[index].height)
		);
	});
	readonly pageCount = $derived(this.sizes.length);
	readonly scope = $derived(
		this.pages.trim()
			? (parsePageRange(this.pages, this.pageCount) ?? [])
			: Array.from({ length: this.pageCount }, (_, index) => index + 1)
	);
	readonly pagesInvalid = $derived(
		this.pages.trim() !== '' && this.pageCount > 0 && this.scope.length === 0
	);
	/// Pages in scope with something to translate, and without.
	readonly targets = $derived(this.scope.filter((page) => this.blocks[page - 1]?.length));
	readonly empty = $derived(
		this.glyphs ? this.scope.filter((page) => !this.blocks[page - 1]?.length) : []
	);
	/// Whether any block has a translation to show.
	readonly translated = $derived(
		this.blocks.some((page) => page.some((block) => this.text(block)))
	);
	readonly hasText = $derived(this.blocks.some((page) => page.length > 0));
	/// What a result depends on beyond the file.
	readonly signature = $derived(
		JSON.stringify([this.from, this.to, this.pages, this.keepOriginal, [...this.edits]])
	);

	private detecting = 0;

	/// The document's text and page sizes, read; detects its language.
	async read(glyphs: PageGlyphs[], sizes: { width: number; height: number }[]) {
		this.glyphs = glyphs;
		this.sizes = sizes;
		const run = ++this.detecting;
		const sample = this.blocks
			.flat()
			.map((block) => block.text)
			.join('\n');
		const from = this.from;
		const detected =
			sample.length >= 20 ? await detectLanguage(sample).catch(() => undefined) : undefined;
		if (run !== this.detecting) return;
		this.detected = detected;
		// Unless someone picked a language while it was read.
		if (detected && this.from === from) {
			this.from = detected;
			if (this.to === detected) this.to = preferredTarget(detected);
		}
	}

	/// Forgets the document, keeping what the translator has learnt.
	reset() {
		this.detecting++;
		this.glyphs = null;
		this.sizes = [];
		this.detected = undefined;
		this.pages = '';
		this.edits.clear();
		this.showOriginal = false;
		this.covers = {};
		this.active = [];
	}

	private editKey = (block: TranslateBlock) => `${this.key}\u0000${block.id}`;

	/// A block's translation, or undefined while it has none or when it says
	/// what the original says.
	text(block: TranslateBlock): string | undefined {
		const text =
			this.edits.get(this.editKey(block)) ??
			this.translations.get(`${this.key}\u0000${block.text}`);
		return text === undefined || unchanged(block.text, text) ? undefined : text;
	}

	edit(block: TranslateBlock, text: string) {
		const machine = this.translations.get(`${this.key}\u0000${block.text}`);
		if (text === machine) this.edits.delete(this.editKey(block));
		else this.edits.set(this.editKey(block), text);
	}

	cover(block: TranslateBlock, color: number) {
		this.covers[block.id] = color;
	}

	/// Each block of `page` as it is set, from 1.
	set(page: number): { texts: (string | undefined)[]; sets: (SetBlock | undefined)[] } {
		const blocks = this.blocks[page - 1] ?? [];
		const size = this.sizes[page - 1];
		const texts = blocks.map((block) => this.text(block));
		if (!size) return { texts, sets: blocks.map(() => undefined) };
		return { texts, sets: setPage(blocks, texts, size, this.rtl) };
	}

	/// Downloads what the languages need, without waiting for it.
	prefetch() {
		for (const pair of this.pairs) void this.models.ensure(pair).catch(() => {});
	}

	/// Translates the target pages, nearest `first` first, and writes them.
	async run(file: File, password: string, first: number, signal: AbortSignal) {
		const pairs = [...this.pairs];
		const key = this.key;
		const targets = [...this.targets];
		const stop = () => this.translator.destroy();
		signal.addEventListener('abort', stop, { once: true });
		try {
			this.stage = 'models';
			await Promise.all(pairs.map((pair) => this.models.ensure(pair)));
			if (signal.aborted) throw cancelled();
			const order = [...targets].sort((a, b) => Math.abs(a - first) - Math.abs(b - first));
			this.total = order.length;
			this.done = 0;
			const waiting = (page: number) =>
				(this.blocks[page - 1] ?? []).some(
					(block) => !this.translations.has(`${key}\u0000${block.text}`)
				);
			if (order.some(waiting)) {
				this.stage = 'starting';
				await this.translator.prepare(this.models, pairs);
				if (signal.aborted) throw cancelled();
			}
			this.stage = 'translating';
			for (const page of order) {
				const missing = (this.blocks[page - 1] ?? [])
					.map((block) => block.text)
					.filter(
						(text, index, texts) =>
							texts.indexOf(text) === index && !this.translations.has(`${key}\u0000${text}`)
					);
				this.active = missing.length ? [page] : [];
				for (let start = 0; start < missing.length; start += CHUNK) {
					const chunk = missing.slice(start, start + CHUNK);
					const translated = await this.translator.translate(pairs, chunk);
					if (signal.aborted) throw cancelled();
					chunk.forEach((text, index) =>
						this.translations.set(`${key}\u0000${text}`, translated[index].trim())
					);
				}
				this.done++;
			}
			this.active = [];
			this.stage = 'writing';
			return await this.write(file, password, targets, signal);
		} finally {
			signal.removeEventListener('abort', stop);
			this.active = [];
		}
	}

	private async write(file: File, password: string, targets: number[], signal: AbortSignal) {
		// Measured in the fonts the engine draws, once they are here.
		const byFamily: Partial<Record<FontFamily, string[]>> = {};
		for (const page of targets)
			for (const block of this.blocks[page - 1]) {
				const text = this.text(block);
				if (text) (byFamily[block.family] ??= []).push(text);
			}
		await Promise.all(
			Object.entries(byFamily).map(([family, texts]) =>
				fontsLoaded(texts.join(' '), family as FontFamily)
			)
		);
		if (signal.aborted) throw cancelled();
		const replace: TextRemoval[] = [];
		const additions: PageAnnotation[] = [];
		for (const page of targets) {
			const blocks = this.blocks[page - 1];
			const { texts, sets } = this.set(page);
			blocks.forEach((block, index) => {
				const [text, set] = [texts[index], sets[index]];
				if (!text || !set) return;
				replace.push(...blockRemovals(block, this.covers[block.id] ?? 0xffffff));
				additions.push(translationMark(block, text, set));
			});
		}
		if (!additions.length)
			throw new Error(
				targets.length === 1
					? 'This page already reads the same in the target language.'
					: 'These pages already read the same in the target language.'
			);
		const translated = await processEdit(
			file,
			password,
			{ images: [], replace, erase: [], additions },
			[],
			signal
		);
		const covered: CoveredPage[] = translated.covered;
		if (!this.keepOriginal) return { output: translated as PdfOutput, covered };
		// Each translated page after its original.
		const copy = new File([translated.bytes.slice().buffer], `translated-${file.name}`);
		const original = sourceKey(file);
		const translation = sourceKey(copy);
		const pages = Array.from({ length: this.pageCount }, (_, index) => index + 1).flatMap(
			(number) => [
				{ source: original, number, rotation: 0 as const },
				...(targets.includes(number) ? [{ source: translation, number, rotation: 0 as const }] : [])
			]
		);
		const output = await processOrganizePdf([file, copy], [password, ''], pages, signal);
		return { output: output as PdfOutput, covered };
	}

	destroy() {
		this.translator.destroy();
	}
}
