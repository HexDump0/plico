// The Annotate tool's marks, selection, undo history and drawing style. The
// overlay on each page and the sidebar both work through one of these.

import {
	colorNumber,
	extent,
	groupOf,
	hexColor,
	imageHeight,
	textHeight,
	type AnnotateMark,
	type AnnotateTool,
	type Geometry,
	type MarkupKind,
	type ShapeKind,
	type StyleGroup
} from './annotate';
import { undrawable, type FontFamily } from './standard-fonts';
import type { PreviewPage, StampImage } from './stamp-layout';
import type { AnnotationMark, CropArea, PageAnnotation } from './types';

export type AnnotateStyle = {
	markup: { kind: MarkupKind; color: string };
	ink: { color: string; width: number; opacity: number };
	shape: { kind: ShapeKind; color: string; width: number; opacity: number; fill: boolean };
	text: { color: string; family: FontFamily; bold: boolean; size: number; background: boolean };
	note: { color: string };
	image: { opacity: number };
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
}>;

const defaults = (): AnnotateStyle => ({
	markup: { kind: 'highlight', color: '#facc15' },
	ink: { color: '#dc2626', width: 2, opacity: 100 },
	shape: { kind: 'rectangle', color: '#dc2626', width: 2, opacity: 100, fill: false },
	text: { color: '#000000', family: 'helvetica', bold: false, size: 14, background: false },
	note: { color: '#facc15' },
	image: { opacity: 100 }
});

const WHITE = 0xffffff;
const unique = <T>(values: T[]) => values.filter((value, index) => values.indexOf(value) === index);
const TEXT_WIDTH = 180;

/// Whether a mark has what the engine needs to draw it: text boxes need
/// text, notes need their comment.
const filled = (mark: AnnotationMark & { comment: string }) =>
	mark.kind === 'text'
		? mark.text.trim() !== ''
		: mark.kind === 'note'
			? mark.comment.trim() !== ''
			: true;

export class AnnotateEditor {
	tool = $state<AnnotateTool>('markup');
	style = $state<AnnotateStyle>(defaults());
	marks = $state.raw<AnnotateMark[]>([]);
	/// Earlier states of the marks, newest last, for Undo.
	history = $state.raw<AnnotateMark[][]>([]);
	selected = $state<number | null>(null);
	/// The text box or note whose text is being typed.
	editing = $state<number | null>(null);
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
			case 'note':
			case 'image':
				return base;
			default:
				return { ...base, kind: mark.kind };
		}
	});
	ready = $derived(this.marks.filter(filled));
	signature = $derived(JSON.stringify([this.ready, this.flatten]));
	hasNotes = $derived(this.ready.some((mark) => mark.kind === 'note'));
	/// The first character in any text box the built-in fonts cannot draw.
	undrawable = $derived(
		this.marks
			.map((mark) => (mark.kind === 'text' ? undrawable(mark.text) : undefined))
			.find(Boolean)
	);

	aspect(mark: AnnotationMark) {
		return mark.kind === 'image' ? (this.images[mark.image]?.aspect ?? 1) : 1;
	}

	setTool(tool: AnnotateTool) {
		this.finishEditing();
		this.tool = tool;
		this.selected = null;
	}

	select(id: number | null) {
		if (this.editing !== null && this.editing !== id) this.finishEditing();
		this.selected = id;
	}

	/// Before a change, so it can be undone as one step.
	snapshot() {
		this.history = [...this.history.slice(-99), this.marks];
	}

	/// The annotation a gesture makes with the current style, or nothing.
	build(
		page: PreviewPage,
		geometry: Geometry
	): (AnnotationMark & { color: number; opacity: number }) | null {
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
			default:
				return null;
		}
	}

	create(page: PreviewPage, geometry: Geometry) {
		const built = this.build(page, geometry);
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

	/// Typed text for a text box, which grows to show every line, or a
	/// note's comment.
	type(id: number, text: string) {
		const mark = this.marks.find((existing) => existing.id === id);
		if (!mark) return;
		if (mark.kind === 'note') return this.change(id, { ...mark, comment: text });
		if (mark.kind !== 'text') return;
		const page = this.sizes[mark.page];
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

	/// Ends typing; a text box or note left empty goes.
	finishEditing() {
		const id = this.editing;
		if (id === null) return;
		this.editing = null;
		const mark = this.marks.find((existing) => existing.id === id);
		if (mark && !filled(mark)) {
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
		const used = unique(this.ready.flatMap((mark) => (mark.kind === 'image' ? [mark.image] : [])));
		const annotations = this.ready.map((mark) =>
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
