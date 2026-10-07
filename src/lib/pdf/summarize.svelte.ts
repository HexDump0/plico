// Summarize PDF's state and run. Each part of the document gives a few key
// points, and the overview is written from those points, or from the text
// itself when it fits in one part. Each point is traced back to the paragraph
// that shares most of its words, so the preview can show where it came from.

import { untrack } from 'svelte';
import { parsePageRange } from './page-range';
import { Summarizer, type ChatMessage } from './summarizer';
import { MODEL_NAME, modelBuild, modelBytes, storedBuilds, type ModelBuild } from './summary-model';
import {
	central,
	documentParagraphs,
	listedPoints,
	parts,
	relevant,
	sources,
	trace,
	type Paragraph,
	type SummaryPoint
} from './summary-text';
import type { CropArea } from './types';
import type { PageGlyphs } from './types';

export type { SummaryPoint } from './summary-text';

export type SummaryLength = 'short' | 'medium' | 'long';
export type SummarizeStage = 'model' | 'starting' | 'reading' | 'writing';

/// A question and the model's answer, with the paragraphs it drew on.
export type ChatTurn = {
	question: string;
	answer: string;
	sources: Paragraph[];
	status: 'starting' | 'reading' | 'writing' | 'done' | 'stopped' | 'failed';
	error?: string;
};

export type ModelState =
	| { status: 'checking' }
	| { status: 'missing' | 'ready'; total: number }
	| { status: 'loading'; received: number; total: number }
	| { status: 'failed'; total: number };

// Points from each part when the document fits in one part, and when it
// does not; and how many are kept at most, the most central first.
const LENGTHS: Record<
	SummaryLength,
	{ single: number; each: number; most: number; sentences: number }
> = {
	short: { single: 3, each: 1, most: 5, sentences: 2 },
	medium: { single: 5, each: 2, most: 10, sentences: 4 },
	long: { single: 8, each: 3, most: 20, sentences: 6 }
};

const cancelled = () => new DOMException('Cancelled', 'AbortError');

const POINTS = (count: number) =>
	`You summarize documents. List the ${count === 1 ? 'single most important point' : `${count} most important points`} of the text you are given, as a Markdown list with one short sentence per point. Use only facts stated in the text. Write in the language of the text. Reply with the list only.`;

const ASK = `You answer questions about a document, using only the excerpts and summary you are given. If they do not hold the answer, say that the document does not seem to say. Answer briefly and plainly. Write in the language of the question.`;

// How much of the document a question is answered from: about 2,500 tokens
// on a GPU, half that on a CPU, where reading a prompt is slow.
const EXCERPT_BUDGET = { webgpu: 10000, wasm: 5000 };
// Earlier turns sent with a question, so a follow-up can lean on them.
const REMEMBERED = 3;

const OVERVIEW = (sentences: number) =>
	`You summarize documents. Write a summary of the document in ${sentences} sentences, as one paragraph of plain text. Use only facts stated in what you are given. Write in the language of the document. Reply with the paragraph only.`;

export class SummarizeDocument {
	private summarizer = new Summarizer();
	private build: ModelBuild | undefined;

	length = $state<SummaryLength>('medium');
	pages = $state('');

	glyphs = $state.raw<PageGlyphs[] | null>(null);
	sizes = $state.raw<{ width: number; height: number }[]>([]);

	model = $state<ModelState>({ status: 'checking' });
	stage = $state<SummarizeStage>('model');
	done = $state(0);
	total = $state(0);
	overview = $state('');
	points = $state.raw<SummaryPoint[]>([]);
	/// The point picked in the list.
	current = $state(-1);
	/// The paragraph the preview outlines: a point's or an answer's source.
	focus = $state<{ page: number; box?: CropArea } | null>(null);
	/// What runs the model here, once known.
	device = $state<ModelBuild['device']>();
	/// Tokens a second the model is writing at, 0 while it reads.
	speed = $state(0);

	mode = $state<'summary' | 'ask'>('summary');
	chat = $state<ChatTurn[]>([]);
	readonly answering = $derived(
		this.chat.some((turn) => ['starting', 'reading', 'writing'].includes(turn.status))
	);

	readonly pageCount = $derived(this.sizes.length);
	readonly scope = $derived(
		this.pages.trim()
			? (parsePageRange(this.pages, this.pageCount) ?? [])
			: Array.from({ length: this.pageCount }, (_, index) => index + 1)
	);
	readonly pagesInvalid = $derived(
		this.pages.trim() !== '' && this.pageCount > 0 && this.scope.length === 0
	);
	/// The paragraphs in scope, in order, without running heads and footers.
	readonly paragraphs = $derived(
		this.glyphs && this.sizes.length === this.glyphs.length
			? documentParagraphs(this.glyphs, this.sizes, this.scope)
			: []
	);
	readonly hasText = $derived(this.paragraphs.length > 0);
	readonly summarized = $derived(this.overview !== '' || this.points.length > 0);
	readonly signature = $derived(JSON.stringify([this.length, this.pages]));
	readonly modelName = MODEL_NAME;

	read(glyphs: PageGlyphs[], sizes: { width: number; height: number }[]) {
		this.glyphs = glyphs;
		this.sizes = sizes;
	}

	reset() {
		this.stop();
		this.glyphs = null;
		this.sizes = [];
		this.pages = '';
		this.clear();
		this.chat = [];
	}

	clear() {
		this.overview = '';
		this.points = [];
		this.current = -1;
		this.focus = null;
	}

	/// Shows a point's paragraph, or hides it when it is already shown.
	pick(index: number) {
		this.current = this.current === index ? -1 : index;
		const point = this.points[this.current];
		this.focus = point ? { page: point.page, box: point.box } : null;
	}

	show(paragraph: Paragraph) {
		this.current = -1;
		this.focus = { page: paragraph.page, box: paragraph.box };
	}

	/// Which build this browser runs, and whether it is stored.
	async check() {
		this.build ??= await modelBuild();
		this.device = this.build.device;
		const total = modelBytes(this.build.dtype);
		if (this.model.status === 'loading') return;
		const stored = (await storedBuilds()).some((build) => build.dtype === this.build!.dtype);
		this.model = { status: stored ? 'ready' : 'missing', total };
	}

	/// The model loaded, downloading it first when it is not stored.
	private async ready() {
		await this.check();
		const build = this.build!;
		const total = modelBytes(build.dtype);
		this.stage = this.model.status === 'ready' ? 'starting' : 'model';
		if (this.model.status !== 'ready') this.model = { status: 'loading', received: 0, total };
		await this.summarizer
			.load(build, (received, reported) => {
				// Once the weights are in, what is left is compiling shaders.
				if (received >= reported && reported > total * 0.9) this.stage = 'starting';
				this.model = { status: 'loading', received: Math.min(received, total), total };
			})
			.catch((error) => {
				this.model = { status: 'failed', total };
				throw error;
			});
		this.model = { status: 'ready', total };
		return build;
	}

	private generate(
		system: string,
		prompt: string,
		maxTokens: number,
		text: (text: string) => void
	) {
		this.speed = 0;
		return this.summarizer.generate(
			[
				{ role: 'system', content: system },
				{ role: 'user', content: prompt }
			],
			maxTokens,
			(written, speed) => {
				this.speed = speed;
				text(written);
			}
		);
	}

	/// The summary as Markdown, for saving and copying.
	markdown(title: string) {
		const points = this.points.map((point) => `- ${point.text} (page ${point.page})`).join('\n');
		return `# Summary of ${title}\n\n${this.overview.trim()}\n\n## Key points\n\n${points}\n`;
	}

	/// The conversation as Markdown, for saving.
	chatMarkdown(title: string) {
		const turns = this.chat
			.filter((turn) => turn.answer)
			.map((turn) => {
				const pages = turn.sources
					.map((source) => source.page)
					.filter((page, index, all) => all.indexOf(page) === index);
				const cited = pages.length
					? `\n\n_${pages.length === 1 ? 'Page' : 'Pages'} ${pages.join(', ')}_`
					: '';
				return `**${turn.question}**\n\n${turn.answer.trim()}${cited}`;
			});
		return `# Questions about ${title}\n\n${turns.join('\n\n---\n\n')}\n`;
	}

	async run(signal: AbortSignal) {
		const stop = () => this.summarizer.interrupt();
		signal.addEventListener('abort', stop, { once: true });
		try {
			this.clear();
			await this.ready();
			if (signal.aborted) throw cancelled();

			const { single, each, most, sentences } = LENGTHS[this.length];
			const found = parts(this.paragraphs);
			const count = found.length === 1 ? single : each;
			this.stage = 'reading';
			this.total = found.length;
			const points: SummaryPoint[] = [];
			for (const [index, part] of found.entries()) {
				this.done = index;
				const prompt = `Text from ${part.first === part.last ? `page ${part.first}` : `pages ${part.first} to ${part.last}`}:\n\n${part.text}`;
				const shown = (text: string) =>
					(this.points = [...points, ...listedPoints(text).map((point) => trace(point, part))]);
				const text = await this.generate(POINTS(count), prompt, count * 60 + 40, shown);
				if (signal.aborted) throw cancelled();
				points.push(
					...listedPoints(text)
						.slice(0, count)
						.map((point) => trace(point, part))
				);
				this.points = [...points];
			}
			this.done = found.length;
			this.points = central(points, most, this.paragraphs);

			this.stage = 'writing';
			const source =
				found.length === 1
					? found[0].text
					: `Key points of the document, in order:\n${points.map((point) => `- ${point.text}`).join('\n')}`;
			const overview = await this.generate(
				OVERVIEW(sentences),
				source,
				sentences * 50 + 60,
				(text) => (this.overview = text.trim())
			);
			if (signal.aborted) throw cancelled();
			this.overview = overview.trim();
			if (!this.overview && !this.points.length)
				throw new Error('The model wrote nothing for this PDF.');
		} finally {
			signal.removeEventListener('abort', stop);
		}
	}

	/// Answers `question` from the paragraphs that best match it (with the
	/// last question, so a follow-up finds what it follows), the summary when
	/// there is one, and the last few turns.
	async ask(question: string, title: string) {
		if (this.answering || !question.trim()) return;
		const earlier = this.chat.filter((turn) => turn.answer).slice(-REMEMBERED);
		this.chat.push({ question: question.trim(), answer: '', sources: [], status: 'starting' });
		const turn = this.chat[this.chat.length - 1];
		try {
			const build = await this.ready();
			if (turn.status === 'stopped') return;
			const budget = EXCERPT_BUDGET[build.device];
			const excerpts = relevant(
				this.paragraphs,
				`${earlier.at(-1)?.question ?? ''} ${question}`,
				budget
			);
			const summary = this.overview ? `Summary of the document:\n${this.overview}\n\n` : '';
			const quoted = excerpts
				.map((paragraph) => `[page ${paragraph.page}] ${paragraph.text.slice(0, budget)}`)
				.join('\n\n');
			const messages: ChatMessage[] = [
				{ role: 'system', content: ASK },
				...earlier.flatMap((past): ChatMessage[] => [
					{ role: 'user', content: past.question },
					{ role: 'assistant', content: past.answer }
				]),
				{
					role: 'user',
					content: `Document: ${title}\n\n${summary}Excerpts:\n${quoted}\n\nQuestion: ${question.trim()}`
				}
			];
			turn.status = 'reading';
			this.speed = 0;
			const answer = await this.summarizer.generate(messages, 400, (text, speed) => {
				if (turn.status === 'reading') turn.status = 'writing';
				turn.answer = text;
				this.speed = speed;
			});
			turn.answer = answer.trim();
			turn.sources = sources(turn.answer, excerpts);
			// `stop` may have changed it while the answer was written.
			if ((turn.status as ChatTurn['status']) !== 'stopped') turn.status = 'done';
			if (!turn.answer) {
				turn.status = 'failed';
				turn.error = 'The model wrote nothing.';
			}
		} catch (error) {
			turn.status = 'failed';
			turn.error = error instanceof Error ? error.message : 'The model could not answer.';
		}
	}

	/// Ends the answer being written, keeping what it has.
	/// Untracked, since the workspace resets this from an effect that would
	/// otherwise rerun on the conversation it is clearing.
	stop() {
		const turn = untrack(() =>
			this.chat.findLast((turn) => ['starting', 'reading', 'writing'].includes(turn.status))
		);
		if (!turn) return;
		turn.status = 'stopped';
		this.summarizer.interrupt();
	}

	clearChat() {
		this.stop();
		this.chat = [];
		this.focus = null;
	}

	destroy() {
		this.summarizer.destroy();
	}
}
