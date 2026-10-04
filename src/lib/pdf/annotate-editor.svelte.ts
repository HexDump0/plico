// The marks, selection, undo history and drawing style of Annotate PDF, and
// of Edit PDF, which draws everything into the page, erases areas and
// replaces text the page already has. The overlay on each page and the
// sidebar both work through one of these.

import {
	capHeightOf,
	colorNumber,
	extent,
	groupOf,
	hexColor,
	imageHeight,
	TEXT_PADDING,
	textFitWidth,
	textHeight,
	type AnnotateMark,
	type AnnotateTool,
	type Drawn,
	type Geometry,
	type MarkupKind,
	type ShapeKind,
	type StyleGroup
} from './annotate';
import { placeRun, type TextRun } from './edit-text';
import { undrawable, type FontFamily } from './standard-fonts';
import type { PreviewPage, StampImage } from './stamp-layout';
import type {
	CropArea,
	EditOptions,
	Erasure,
	ImageEdit,
	PageAnnotation,
	TextRemoval
} from './types';

export type AnnotateStyle = {
	markup: { kind: MarkupKind; color: string };
	ink: { color: string; width: number; opacity: number };
	shape: { kind: ShapeKind; color: string; width: number; opacity: number; fill: boolean };
	text: { color: string; family: FontFamily; bold: boolean; size: number; background: boolean };
	note: { color: string };
	image: { opacity: number };
	erase: { color: string; match: boolean };
	picture: Record<string, never>;
};

/// Every setting the sidebar can show, for whichever group it shows.
export type StyleValues = Partial<{
	kind: MarkupKind | ShapeKind;
	color: string;
	width: number;
	opacity: number;
	fill: boolean;
	family: FontFamily;
	bold: boolean;
	size: number;
	background: boolean;
	match: boolean;
}>;

const defaults = (mode: EditorMode): AnnotateStyle => ({
	markup: { kind: 'highlight', color: '#facc15' },
	ink: { color: '#dc2626', width: 2, opacity: 100 },
	shape: { kind: 'rectangle', color: '#dc2626', width: 2, opacity: 100, fill: false },
	text: {
		color: '#000000',
		family: 'helvetica',
		bold: false,
		size: mode === 'edit' ? 12 : 14,
		background: false
	},
	note: { color: '#facc15' },
	image: { opacity: 100 },
	erase: { color: '#ffffff', match: true },
	picture: {}
});

export type EditorMode = 'annotate' | 'edit';

const WHITE = 0xffffff;
const unique = <T>(values: T[]) => values.filter((value, index) => values.indexOf(value) === index);
const TEXT_WIDTH = 180;

/// Whether a mark has what the engine needs to draw it: text boxes need
/// text, notes need their comment. Replaced text left empty is text taken out.
const filled = (mark: AnnotateMark) =>
	mark.kind === 'text'
		? mark.replaces !== undefined || mark.text.trim() !== ''
		: mark.kind === 'note'
			? mark.comment.trim() !== ''
			: true;

const same = (a: CropArea, b: CropArea) =>
	a.every((value, index) => Math.abs(value - b[index]) < 1e-6);

/// Replaced text that still reads, sits and looks as it was matched, or a
/// picked image still where the page draws it.
const pristine = (mark: AnnotateMark) => {
	if (mark.kind === 'picture') return !mark.deleted && same(mark.area, mark.from);
	if (mark.kind !== 'text' || !mark.replaces) return false;
	const { original } = mark.replaces;
	return (
		mark.text === original.text &&
		same(mark.area, original.area) &&
		mark.family === original.family &&
		mark.bold === original.bold &&
		mark.size === original.size &&
		mark.color === original.color &&
		mark.opacity === 1
	);
};

/// A text box that grows sideways to fit what is typed, and down one line
/// at a time; it wraps only once it reaches the page's right edge.
function fitted(
	mark: Extract<AnnotateMark, { kind: 'text' }>,
	page: PreviewPage
): Extract<AnnotateMark, { kind: 'text' }> {
	const [left, top] = mark.area;
	const width = Math.max(
		textFitWidth(mark.text, mark.family, mark.bold, mark.size),
		mark.size + 2 * TEXT_PADDING
	);
	const right = Math.min(1, left + width / page.width);
	const height = textHeight(
		mark.text,
		mark.family,
		mark.bold,
		mark.size,
		(right - left) * page.width
	);
	return { ...mark, area: [left, top, right, Math.min(1, top + height / page.height)] };
}

export class AnnotateEditor {
	readonly mode: EditorMode;
	tool = $state<AnnotateTool>('markup');
	style = $state<AnnotateStyle>(defaults('annotate'));
	marks = $state.raw<AnnotateMark[]>([]);
	/// Earlier states of the marks, newest last, for Undo.
	history = $state.raw<AnnotateMark[][]>([]);
	selected = $state<number | null>(null);
	/// The text box or note whose text is being typed.
	editing = $state<number | null>(null);
	/// Where the caret goes once typing starts, when not at the end.
	caret: number | null = null;
	/// Pictures image annotations draw, by their `image` index, and the one
	/// the Image tool places next.
	images = $state.raw<StampImage[]>([]);
	image = $state<number | null>(null);
	flatten = $state(false);
	/// Each page's size once shown, for placing a picture chosen in the
	/// sidebar on the page on screen.
	sizes: Record<number, PreviewPage> = {};
	#next = 0;
	#lastStyleEdit = { id: -1, key: '', time: 0 };

	constructor(mode: EditorMode = 'annotate') {
		this.mode = mode;
		this.tool = mode === 'edit' ? 'text' : 'markup';
		this.style = defaults(mode);
	}

	selectedMark = $derived(this.marks.find((mark) => mark.id === this.selected));
	/// The settings the sidebar shows: the selected mark's, or the tool's.
	group = $derived<StyleGroup | null>(
		this.selectedMark ? groupOf(this.selectedMark.kind) : this.tool === 'select' ? null : this.tool
	);
	values = $derived.by((): StyleValues => {
		const group = this.group;
		if (!group) return {};
		const base: StyleValues = { ...this.style[group] };
		const mark = this.selectedMark;
		if (!mark) return base;
		base.color = hexColor(mark.color);
		base.opacity = Math.round(mark.opacity * 100);
		switch (mark.kind) {
			case 'ink':
				return { ...base, width: mark.width };
			case 'rectangle':
			case 'ellipse':
				return { ...base, kind: mark.kind, width: mark.width, fill: mark.fill !== null };
			case 'line':
			case 'arrow':
				return { ...base, kind: mark.kind, width: mark.width, fill: false };
			case 'text':
				return {
					...base,
					family: mark.family,
					bold: mark.bold,
					size: mark.size,
					background: mark.fill !== null
				};
			case 'erase':
				return { ...base, match: mark.match };
			case 'picture':
			case 'note':
			case 'image':
				return base;
			default:
				return { ...base, kind: mark.kind };
		}
	});
	ready = $derived(this.marks.filter((mark) => filled(mark) && !pristine(mark)));
	signature = $derived(JSON.stringify([this.ready, this.flatten]));
	hasNotes = $derived(this.ready.some((mark) => mark.kind === 'note'));
	/// The first character in any text box the built-in fonts cannot draw.
	undrawable = $derived(
		this.marks
			.map((mark) => (mark.kind === 'text' ? undrawable(mark.text) : undefined))
			.find(Boolean)
	);

	/// An image's height over its width, in points.
	aspect(mark: Drawn) {
		if (mark.kind === 'image') return this.images[mark.image]?.aspect ?? 1;
		const page = 'page' in mark ? this.sizes[mark.page as number] : undefined;
		if (mark.kind !== 'picture' || !page) return 1;
		const [left, top, right, bottom] = mark.area;
		return ((bottom - top) * page.height) / Math.max(1e-9, (right - left) * page.width);
	}

	setTool(tool: AnnotateTool) {
		this.finishEditing();
		this.tool = tool;
		this.select(null);
	}

	/// Selecting something else lets go of what the page had and was only
	/// picked, not changed.
	select(id: number | null) {
		if (this.editing !== null && this.editing !== id) this.finishEditing();
		const previous = this.selected;
		this.selected = id;
		if (previous !== null && previous !== id) this.#dropIfPristine(previous);
	}

	/// Takes away a mark that changes nothing, with the step that made it.
	#dropIfPristine(id: number) {
		const mark = this.marks.find((existing) => existing.id === id);
		if (!mark || !pristine(mark)) return;
		if (!this.history.at(-1)?.some((earlier) => earlier.id === id))
			this.history = this.history.slice(0, -1);
		this.marks = this.marks.filter((existing) => existing.id !== id);
		if (this.selected === id) this.selected = null;
	}

	/// Before a change, so it can be undone as one step.
	snapshot() {
		this.history = [...this.history.slice(-99), this.marks];
	}

	/// The annotation a gesture makes with the current style, or nothing.
	/// `matched` is the colour of the page around an erased area.
	build(
		page: PreviewPage,
		geometry: Geometry,
		matched = WHITE
	):
		| (Drawn & { color: number; opacity: number; fit?: boolean; match?: boolean; matched?: number })
		| null {
		const style = this.style;
		switch (this.tool) {
			case 'markup':
				if (geometry.kind !== 'boxes' || !geometry.boxes.length) return null;
				return {
					kind: style.markup.kind,
					boxes: geometry.boxes,
					color: colorNumber(style.markup.color),
					opacity: 1
				};
			case 'ink':
				if (geometry.kind !== 'stroke' || !geometry.points.length) return null;
				return {
					kind: 'ink',
					strokes: [geometry.points],
					width: style.ink.width,
					color: colorNumber(style.ink.color),
					opacity: style.ink.opacity / 100
				};
			case 'shape': {
				const color = colorNumber(style.shape.color);
				const common = { width: style.shape.width, color, opacity: style.shape.opacity / 100 };
				const kind = style.shape.kind;
				if (kind === 'line' || kind === 'arrow')
					return geometry.kind === 'segment'
						? { kind, from: geometry.from, to: geometry.to, ...common }
						: null;
				return geometry.kind === 'area'
					? { kind, area: geometry.area, fill: style.shape.fill ? color : null, ...common }
					: null;
			}
			case 'text': {
				const { family, bold, size } = style.text;
				let area: CropArea;
				if (geometry.kind === 'area') {
					// Never shorter than one line.
					const [left, top, right, bottom] = geometry.area;
					const line =
						textHeight('', family, bold, size, (right - left) * page.width) / page.height;
					area = [left, top, right, Math.min(1, Math.max(bottom, top + line))];
				} else if (geometry.kind === 'point' && this.mode === 'edit') {
					// Typed where clicked: the first line's middle on the click.
					const height = textHeight('', family, bold, size, TEXT_WIDTH) / page.height;
					const width = (size + 2 * TEXT_PADDING) / page.width;
					const left = Math.min(Math.max(0, geometry.at[0] - TEXT_PADDING / page.width), 1 - width);
					const middle = (TEXT_PADDING + (capHeightOf(family, bold) * size) / 2) / page.height;
					const top = Math.min(Math.max(0, geometry.at[1] - middle), 1 - height);
					return {
						kind: 'text',
						area: [left, top, left + width, top + height],
						text: '',
						family,
						bold,
						size,
						fill: style.text.background ? WHITE : null,
						color: colorNumber(style.text.color),
						opacity: 1,
						fit: true
					};
				} else if (geometry.kind === 'point') {
					const width = Math.min(1, TEXT_WIDTH / page.width);
					const height = Math.min(1, textHeight('', family, bold, size, TEXT_WIDTH) / page.height);
					const left = Math.min(geometry.at[0], 1 - width);
					const top = Math.min(geometry.at[1], 1 - height);
					area = [left, top, left + width, top + height];
				} else return null;
				return {
					kind: 'text',
					area,
					text: '',
					family,
					bold,
					size,
					fill: style.text.background ? WHITE : null,
					color: colorNumber(style.text.color),
					opacity: 1
				};
			}
			case 'note':
				if (geometry.kind !== 'point') return null;
				return { kind: 'note', at: geometry.at, color: colorNumber(style.note.color), opacity: 1 };
			case 'image': {
				const image = this.image === null ? undefined : this.images[this.image];
				if (geometry.kind !== 'point' || !image || this.image === null) return null;
				const width = Math.min(0.3, 0.4 / ((image.aspect * page.width) / page.height));
				const height = imageHeight(width, image.aspect, page);
				return {
					kind: 'image',
					place: [
						Math.min(1 - width, Math.max(0, geometry.at[0] - width / 2)),
						Math.min(1 - height, Math.max(0, geometry.at[1] - height / 2)),
						width
					],
					image: this.image,
					color: 0,
					opacity: style.image.opacity / 100
				};
			}
			case 'erase': {
				if (geometry.kind !== 'area') return null;
				const { match, color } = style.erase;
				return {
					kind: 'erase',
					area: geometry.area,
					color: match ? matched : colorNumber(color),
					opacity: 1,
					match,
					matched
				};
			}
			default:
				return null;
		}
	}

	create(page: PreviewPage, geometry: Geometry, matched?: number) {
		const built = this.build(page, geometry, matched);
		if (!built) return;
		this.finishEditing();
		this.snapshot();
		const mark = { ...built, page: page.number, comment: '', id: ++this.#next } as AnnotateMark;
		this.marks = [...this.marks, mark];
		this.selected = mark.id;
		if (mark.kind === 'text' || mark.kind === 'note') this.editing = mark.id;
	}

	change(id: number, mark: AnnotateMark) {
		this.marks = this.marks.map((existing) => (existing.id === id ? mark : existing));
	}

	remove(id: number) {
		this.snapshot();
		this.marks = this.marks.filter((mark) => mark.id !== id);
		if (this.selected === id) this.selected = null;
		if (this.editing === id) this.editing = null;
	}

	edit(id: number) {
		// A blur can take an empty note away just before the click that opens it.
		if (!this.marks.some((mark) => mark.id === id)) return;
		this.snapshot();
		this.selected = id;
		this.editing = id;
	}

	/// Starts replacing a run of the page's text: a text box over it in the
	/// standard font nearest its own, its first baseline on the run's, with
	/// `cover` painted where the run was. Clicking text already replaced
	/// opens its replacement instead.
	replace(
		page: PreviewPage,
		run: TextRun,
		cover: number,
		color: number,
		caret: number | null = null
	) {
		const { id, fresh } = this.pickRun(page, run, cover, color);
		this.caret = caret;
		if (!fresh) return this.edit(id);
		this.editing = id;
	}

	/// Picks a run of the page's text, as `replace` does without opening it
	/// for typing, or the mark already standing in for it.
	pickRun(page: PreviewPage, run: TextRun, cover: number, color: number) {
		const existing = this.marks.find(
			(mark) =>
				mark.page === page.number &&
				mark.replaces !== undefined &&
				same(mark.replaces.band, run.band)
		);
		if (existing) {
			this.select(existing.id);
			return { id: existing.id, fresh: false };
		}
		this.finishEditing();
		const { size, left, top } = placeRun(run, page);
		const base = {
			kind: 'text' as const,
			area: [left, top, left, top] as CropArea,
			text: run.text,
			family: run.family,
			bold: run.bold,
			size,
			fill: null,
			color,
			opacity: 1,
			page: page.number,
			comment: '',
			fit: true
		};
		const mark = fitted({ ...base, id: 0 }, page);
		this.snapshot();
		const id = ++this.#next;
		this.marks = [
			...this.marks,
			{
				...mark,
				id,
				replaces: {
					band: run.band,
					ink: run.ink,
					cover,
					original: {
						text: mark.text,
						area: mark.area,
						family: mark.family,
						bold: mark.bold,
						size: mark.size,
						color
					}
				}
			}
		];
		this.select(id);
		return { id, fresh: true };
	}

	/// Picks an image the page draws at `from`, or the mark already
	/// standing in for it. `snapshot` and `cover` are how it looks and the
	/// colour around it.
	pick(page: PreviewPage, from: CropArea, snapshot: string, cover: number) {
		const existing = this.marks.find(
			(mark) => mark.page === page.number && mark.kind === 'picture' && same(mark.from, from)
		);
		if (existing) {
			this.select(existing.id);
			return { id: existing.id, fresh: false };
		}
		this.finishEditing();
		this.snapshot();
		const id = ++this.#next;
		this.marks = [
			...this.marks,
			{
				kind: 'picture',
				area: [...from] as CropArea,
				from: [...from] as CropArea,
				snapshot,
				cover,
				deleted: false,
				page: page.number,
				color: 0,
				opacity: 1,
				comment: '',
				id
			}
		];
		this.select(id);
		return { id, fresh: true };
	}

	/// Takes a picked image away, or the text of a line the page had.
	discard(id: number) {
		const mark = this.marks.find((existing) => existing.id === id);
		if (mark?.kind === 'picture' && !mark.deleted) {
			this.snapshot();
			this.change(id, { ...mark, deleted: true });
		} else if (mark?.kind === 'text' && mark.replaces && mark.text) {
			this.snapshot();
			this.type(id, '');
		}
	}

	/// Typed text for a text box, which grows to show every line, or a
	/// note's comment.
	type(id: number, text: string) {
		const mark = this.marks.find((existing) => existing.id === id);
		if (!mark) return;
		if (mark.kind === 'note') return this.change(id, { ...mark, comment: text });
		if (mark.kind !== 'text') return;
		const page = this.sizes[mark.page];
		if (mark.fit && page) return this.change(id, fitted({ ...mark, text }, page));
		let area = mark.area;
		if (page) {
			const [left, top, right, bottom] = area;
			const needed =
				textHeight(text, mark.family, mark.bold, mark.size, (right - left) * page.width) /
				page.height;
			if (needed > bottom - top) area = [left, top, right, Math.min(1, top + needed)];
		}
		this.change(id, { ...mark, text, area });
	}

	/// Ends typing; a text box or note left empty goes, and so does replaced
	/// text left as it was, with the step that made it.
	finishEditing() {
		const id = this.editing;
		if (id === null) return;
		this.editing = null;
		this.caret = null;
		const mark = this.marks.find((existing) => existing.id === id);
		if (mark && pristine(mark)) this.#dropIfPristine(id);
		else if (mark && !filled(mark)) {
			this.marks = this.marks.filter((existing) => existing.id !== id);
			if (this.selected === id) this.selected = null;
		}
	}

	undo() {
		const previous = this.history.at(-1);
		if (!previous) return;
		this.editing = null;
		this.history = this.history.slice(0, -1);
		this.marks = previous;
		if (!previous.some((mark) => mark.id === this.selected)) this.selected = null;
	}

	clear() {
		this.finishEditing();
		this.snapshot();
		this.marks = [];
		this.selected = null;
	}

	/// Changes the style for new marks and, when one is selected, that mark.
	setStyle(patch: StyleValues) {
		const group = this.group;
		if (!group) return;
		Object.assign(this.style[group], patch);
		const mark = this.selectedMark;
		if (!mark) return;
		// A slider reports every step; they undo as one.
		const key = Object.keys(patch).join();
		const now = performance.now();
		const last = this.#lastStyleEdit;
		if (last.id !== mark.id || last.key !== key || now - last.time > 1000) this.snapshot();
		this.#lastStyleEdit = { id: mark.id, key, time: now };
		this.change(mark.id, this.restyled(mark, patch));
	}

	restyled(mark: AnnotateMark, patch: StyleValues): AnnotateMark {
		let next = { ...mark } as AnnotateMark;
		if (patch.color !== undefined) next.color = colorNumber(patch.color);
		if (next.kind === 'erase') {
			if (patch.color !== undefined) next.match = false;
			if (patch.match) next.color = next.matched;
			if (patch.match !== undefined) next.match = patch.match;
		}
		if (patch.opacity !== undefined) next.opacity = patch.opacity / 100;
		if ('width' in next && patch.width !== undefined) next.width = patch.width;
		const page = this.sizes[mark.page];
		if (patch.kind !== undefined && patch.kind !== next.kind && page) {
			// Rectangle and ellipse trade places, so do line and arrow; across
			// the two, a box becomes its diagonal and a line its box.
			const kind = patch.kind;
			const box = extent(next, page);
			const { id, page: number, comment, color, opacity } = next;
			const width = 'width' in next ? next.width : this.style.shape.width;
			const common = { id, page: number, comment, color, opacity, width };
			if (kind === 'line' || kind === 'arrow')
				next =
					next.kind === 'line' || next.kind === 'arrow'
						? { ...next, kind }
						: { ...common, kind, from: [box[0], box[1]], to: [box[2], box[3]] };
			else if (kind === 'rectangle' || kind === 'ellipse')
				next =
					next.kind === 'rectangle' || next.kind === 'ellipse'
						? { ...next, kind }
						: {
								...common,
								kind,
								fill: null,
								area: [
									box[0],
									box[1],
									Math.max(box[2], box[0] + 0.02),
									Math.max(box[3], box[1] + 0.02)
								]
							};
			else if ('boxes' in next) next = { ...next, kind };
		}
		if (patch.fill !== undefined && (next.kind === 'rectangle' || next.kind === 'ellipse'))
			next.fill = patch.fill ? next.color : null;
		else if ((next.kind === 'rectangle' || next.kind === 'ellipse') && next.fill !== null)
			next.fill = next.color;
		if (next.kind === 'text') {
			if (patch.family !== undefined) next.family = patch.family;
			if (patch.bold !== undefined) next.bold = patch.bold;
			if (patch.size !== undefined) next.size = patch.size;
			if (patch.background !== undefined) next.fill = patch.background ? WHITE : null;
			if (next.fit && page) next = fitted(next, page);
		}
		return next;
	}

	/// Adds a chosen picture and places it on `page`, which the Image tool
	/// then repeats wherever the page is clicked.
	addImage(image: StampImage, page: number) {
		this.images = [...this.images, image];
		this.image = this.images.length - 1;
		const size = this.sizes[page];
		if (size) this.create(size, { kind: 'point', at: [0.5, 0.5] });
	}

	/// The annotations to send, with only the pictures they use.
	payload() {
		const ready = this.ready.filter(
			(mark): mark is Exclude<AnnotateMark, { kind: 'erase' | 'picture' }> =>
				mark.kind !== 'erase' && mark.kind !== 'picture'
		);
		const used = unique(ready.flatMap((mark) => (mark.kind === 'image' ? [mark.image] : [])));
		const annotations = ready.map((mark) =>
			mark.kind === 'image' ? { ...mark, image: used.indexOf(mark.image) } : mark
		);
		return {
			// Plain copies without the editor's ids: a reactive proxy cannot be
			// posted to the worker.
			annotations: JSON.parse(
				JSON.stringify(annotations, (key, value) => (key === 'id' ? undefined : value))
			) as PageAnnotation[],
			images: used.map((index) => this.images[index].file)
		};
	}

	/// What Edit sends: text to take out, areas to erase, and everything to
	/// draw into the pages, with only the pictures they use.
	edits(): { options: EditOptions; images: File[] } {
		const ready = this.ready;
		const replace: TextRemoval[] = ready.flatMap((mark) =>
			mark.replaces
				? [
						{
							page: mark.page,
							area: [...mark.replaces.band] as CropArea,
							shown: [...mark.replaces.ink] as CropArea,
							cover: mark.replaces.cover
						}
					]
				: []
		);
		const erase: Erasure[] = ready.flatMap((mark) =>
			mark.kind === 'erase'
				? [{ page: mark.page, area: [...mark.area] as CropArea, fill: mark.color }]
				: []
		);
		const images: ImageEdit[] = ready.flatMap((mark) =>
			mark.kind === 'picture'
				? [
						{
							page: mark.page,
							from: [...mark.from] as CropArea,
							to: mark.deleted ? null : ([...mark.area] as CropArea)
						}
					]
				: []
		);
		const drawn = ready.filter(
			(mark): mark is Exclude<AnnotateMark, { kind: 'erase' | 'picture' }> =>
				mark.kind !== 'erase' &&
				mark.kind !== 'picture' &&
				!(mark.kind === 'text' && mark.text.trim() === '')
		);
		const used = unique(drawn.flatMap((mark) => (mark.kind === 'image' ? [mark.image] : [])));
		const additions = drawn.map((mark) =>
			mark.kind === 'image' ? { ...mark, image: used.indexOf(mark.image) } : mark
		);
		return {
			options: {
				images,
				replace,
				erase,
				// Plain copies without the editor's own fields: a reactive proxy
				// cannot be posted to the worker.
				additions: JSON.parse(
					JSON.stringify(additions, (key, value) =>
						key === 'id' || key === 'fit' || key === 'replaces' ? undefined : value
					)
				) as PageAnnotation[]
			},
			images: used.map((index) => this.images[index].file)
		};
	}

	reset() {
		this.marks = [];
		this.history = [];
		this.selected = null;
		this.editing = null;
		this.sizes = {};
	}

	destroy() {
		for (const image of this.images) URL.revokeObjectURL(image.url);
		this.images = [];
		this.image = null;
	}
}
