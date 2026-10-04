<script lang="ts">
	import { cubicOut } from 'svelte/easing';
	import { fade, scale } from 'svelte/transition';
	import { IconX } from '@tabler/icons-svelte-runes';
	import {
		bounds,
		extent,
		groupOf,
		hexColor,
		LEADING,
		NOTE_SIZE,
		nearestGlyph,
		refit,
		TEXT_PADDING,
		textBetween,
		translate,
		type AnnotateMark,
		type Geometry
	} from '$lib/pdf/annotate';
	import type { AnnotateEditor } from '$lib/pdf/annotate-editor.svelte';
	import {
		caretAt,
		colorAround,
		runAt,
		sampleColors,
		textRuns,
		type TextRun
	} from '$lib/pdf/edit-text';
	import { wordAt } from '$lib/pdf/redact-text';
	import { capHeight, type FontFamily } from '$lib/pdf/standard-fonts';
	import type { PreviewPage } from '$lib/pdf/stamp-layout';
	import type { CropArea, PageGlyphs, PagePoint } from '$lib/pdf/types';
	import AnnotationShape from './AnnotationShape.svelte';

	let {
		page,
		editor,
		glyphs,
		editable,
		reducedMotion
	}: {
		page: PreviewPage;
		editor: AnnotateEditor;
		/// The page's text as the engine reads it, for marking up words.
		glyphs?: PageGlyphs;
		editable: boolean;
		reducedMotion: boolean;
	} = $props();

	type Corner = 'nw' | 'ne' | 'se' | 'sw';
	type Drag = { pointer: number; start: PagePoint; moved: boolean } & (
		| { kind: 'create'; over: number | null; glyph: number; run?: TextRun }
		| { kind: 'move'; origin: AnnotateMark; wasSelected: boolean; fresh?: boolean }
		| { kind: 'corner'; origin: AnnotateMark; corner: Corner }
		| { kind: 'end'; origin: AnnotateMark; end: 'from' | 'to' }
	);

	const corners = [
		{ corner: 'nw', place: '-top-3 -left-3', bracket: 'top-2.5 left-2.5 border-t-3 border-l-3' },
		{ corner: 'ne', place: '-top-3 -right-3', bracket: 'top-2.5 right-2.5 border-t-3 border-r-3' },
		{
			corner: 'se',
			place: '-right-3 -bottom-3',
			bracket: 'right-2.5 bottom-2.5 border-r-3 border-b-3'
		},
		{
			corner: 'sw',
			place: '-bottom-3 -left-3',
			bracket: 'bottom-2.5 left-2.5 border-b-3 border-l-3'
		}
	] as const;
	// Ascent and descent of the fonts the browser falls back to, Arial, Times
	// New Roman and Courier New, for placing typed text on the engine's lines.
	const families: Record<FontFamily, { css: string; ascent: number; descent: number }> = {
		helvetica: {
			css: "Helvetica, Arial, 'Liberation Sans', sans-serif",
			ascent: 0.905,
			descent: 0.212
		},
		times: {
			css: "'Times New Roman', Times, 'Liberation Serif', serif",
			ascent: 0.891,
			descent: 0.216
		},
		courier: {
			css: "'Courier New', Courier, 'Liberation Mono', monospace",
			ascent: 0.833,
			descent: 0.3
		}
	};
	const ends = ['from', 'to'] as const;
	// A drag shorter than this is a click; a shape smaller than this is a slip.
	const CLICK_PIXELS = 4;
	const MIN_PIXELS = 6;
	// How far from text a markup drag may start and still follow the text.
	const TEXT_REACH = 0.015;
	// How far from a line of text a click still edits it, in points.
	const RUN_REACH = 2;

	let root = $state<HTMLDivElement>();
	let pixels = $state(0);
	// Not reactive: nothing on screen reads it, and the mark it holds stays plain.
	let drag: Drag | null = null;
	// What the gesture under way would make.
	let draft = $state.raw<Geometry | null>(null);
	// Where a click would put the picture, shown faintly under a mouse.
	let ghost = $state.raw<PagePoint | null>(null);
	// The line of text a click would edit, or the image Select would pick,
	// outlined under a mouse.
	let hovered = $state.raw<TextRun | null>(null);
	let hoveredImage = $state.raw<CropArea | null>(null);

	const tool = $derived(editor.tool);
	const editing = $derived(editor.mode === 'edit');
	// The page's lines of text Edit can replace, and the images it draws.
	const runs = $derived(editing && glyphs ? textRuns(glyphs, page.width, page.height) : []);
	const images = $derived.by(() => {
		if (!editing || !glyphs?.images) return [];
		const found: CropArea[] = [];
		for (let index = 0; index + 3 < glyphs.images.length; index += 4)
			found.push([...glyphs.images.subarray(index, index + 4)] as CropArea);
		return found;
	});
	const marks = $derived(editor.marks.filter((mark) => mark.page === page.number));
	const selected = $derived(marks.find((mark) => mark.id === editor.selected));
	const editingMark = $derived(marks.find((mark) => mark.id === editor.editing));
	// Pixels per point.
	const zoom = $derived(pixels / page.width);
	const draftMark = $derived(
		draft
			? editor.build(page, draft, draft.kind === 'area' ? matchedAt(draft.area) : undefined)
			: null
	);
	const ghostMark = $derived(
		ghost && tool === 'image' ? editor.build(page, { kind: 'point', at: ghost }) : null
	);

	$effect(() => {
		editor.sizes[page.number] = page;
	});

	function point(event: PointerEvent): PagePoint {
		const box = root!.getBoundingClientRect();
		return [
			Math.min(1, Math.max(0, (event.clientX - box.left) / box.width)),
			Math.min(1, Math.max(0, (event.clientY - box.top) / box.height))
		];
	}

	function size(): [number, number] {
		const box = root!.getBoundingClientRect();
		return [box.width, box.height];
	}

	/// Whether a mark answers the pointer with the current tool. Drawing
	/// passes over everything, marking up text picks only other markup, and
	/// the shape tools pick everything but markup. In Edit, each tool picks
	/// only its own kind of mark.
	function interactive(mark: AnnotateMark) {
		if (!editable) return false;
		if (mark.id === editor.selected || tool === 'select') return true;
		const group = groupOf(mark.kind);
		if (tool === 'ink') return false;
		if (editing) return group === tool;
		return tool === 'markup' ? group === 'markup' : group !== 'markup';
	}

	/// The line of text at a point, within reach of it.
	function runNear([x, y]: PagePoint) {
		return runAt(runs, x, y, RUN_REACH / page.width);
	}

	/// The smallest image the page draws under a point: the one in front,
	/// most likely.
	function imageAt([x, y]: PagePoint) {
		let best: CropArea | null = null;
		for (const area of images) {
			if (x < area[0] || x > area[2] || y < area[1] || y > area[3]) continue;
			const size = (area[2] - area[0]) * (area[3] - area[1]);
			if (!best || size < (best[2] - best[0]) * (best[3] - best[1])) best = area;
		}
		return best;
	}

	/// What Select would pick at a point: text before the image under it.
	function pickableAt(at: PagePoint) {
		const run = runNear(at);
		if (run) return { run };
		const image = imageAt(at);
		return image ? { image } : null;
	}

	const canvas = () => root?.parentElement?.querySelector('canvas');

	/// The page as drawn under an area, as a picture.
	function snapshot(area: CropArea) {
		const source = canvas();
		if (!source?.width) return '';
		const [x0, y0] = [Math.floor(area[0] * source.width), Math.floor(area[1] * source.height)];
		const [x1, y1] = [Math.ceil(area[2] * source.width), Math.ceil(area[3] * source.height)];
		const copy = document.createElement('canvas');
		copy.width = Math.max(1, x1 - x0);
		copy.height = Math.max(1, y1 - y0);
		copy
			.getContext('2d')
			?.drawImage(source, x0, y0, copy.width, copy.height, 0, 0, copy.width, copy.height);
		return copy.toDataURL('image/png');
	}

	/// Picks what the page has at a point with Select and starts moving it.
	function pickOriginal(event: PointerEvent, at: PagePoint) {
		const found = pickableAt(at);
		if (!found) return false;
		let picked: { id: number; fresh: boolean };
		if (found.run) {
			const sampled = sampleColors(canvas(), found.run.box);
			picked = editor.pickRun(page, found.run, sampled.background, found.run.color ?? sampled.text);
		} else {
			const area = found.image;
			picked = editor.pick(page, area, snapshot(area), colorAround(canvas(), area));
		}
		const mark = editor.marks.find((existing) => existing.id === picked.id);
		if (mark) grab(event, mark, 'move', picked.fresh);
		return true;
	}

	/// The colour of the page around an area being erased.
	function matchedAt(area: CropArea) {
		return tool === 'erase' ? colorAround(canvas(), area) : undefined;
	}

	/// Starts replacing a line of text, matching its colour, and the colour
	/// behind it, from the page as drawn.
	function editRun(run: TextRun, x: number) {
		const sampled = sampleColors(canvas(), run.box);
		const caret = glyphs ? caretAt(glyphs, run, x) : null;
		editor.replace(page, run, sampled.background, run.color ?? sampled.text, caret);
	}

	function begin(event: PointerEvent, over: number | null = null) {
		if (!editable || !root || event.button !== 0) return;
		event.stopPropagation();
		// A click away from the text being typed only ends typing, unless it
		// lands on another line of text to edit.
		if (editor.editing !== null) {
			editor.finishEditing();
			editor.select(null);
			const at = point(event);
			const onward =
				editing && ((tool === 'text' && runNear(at)) || (tool === 'select' && pickableAt(at)));
			if (!onward) return;
		}
		if (tool === 'select') {
			hovered = hoveredImage = null;
			if (!(editing && pickOriginal(event, point(event)))) editor.select(null);
			return;
		}
		event.preventDefault();
		root.setPointerCapture(event.pointerId);
		ghost = null;
		const start = point(event);
		const glyph =
			tool === 'markup' && glyphs ? nearestGlyph(glyphs, start[0], start[1], TEXT_REACH) : -1;
		const run = editing && tool === 'text' ? runNear(start) : undefined;
		hovered = null;
		drag = { kind: 'create', pointer: event.pointerId, start, moved: false, over, glyph, run };
		if (tool === 'ink') draft = { kind: 'stroke', points: [start] };
	}

	/// `fresh` when the mark was made by this very press, which then neither
	/// counts as a second click nor takes its own undo step to move.
	function grab(
		event: PointerEvent,
		mark: AnnotateMark,
		handle: 'move' | Corner | 'from' | 'to',
		fresh = false
	) {
		if (!editable || !root || event.button !== 0) return;
		event.preventDefault();
		event.stopPropagation();
		const wasSelected = !fresh && editor.selected === mark.id;
		if (editor.editing !== null && editor.editing !== mark.id) editor.finishEditing();
		editor.select(mark.id);
		if (handle === 'move' && groupOf(mark.kind) === 'markup') return;
		root.setPointerCapture(event.pointerId);
		const common = { pointer: event.pointerId, start: point(event), moved: false, origin: mark };
		drag =
			handle === 'move'
				? { ...common, kind: 'move', wasSelected, fresh }
				: handle === 'from' || handle === 'to'
					? { ...common, kind: 'end', end: handle }
					: { ...common, kind: 'corner', corner: handle };
	}

	function box(from: PagePoint, to: PagePoint): CropArea {
		return [
			Math.min(from[0], to[0]),
			Math.min(from[1], to[1]),
			Math.max(from[0], to[0]),
			Math.max(from[1], to[1])
		];
	}

	function shaped(start: PagePoint, at: PagePoint, glyph: number): Geometry | null {
		if (tool === 'markup') {
			if (glyph >= 0 && glyphs) {
				const end = nearestGlyph(glyphs, at[0], at[1], Infinity);
				return { kind: 'boxes', boxes: textBetween(glyphs, glyph, end) };
			}
			return { kind: 'boxes', boxes: [box(start, at)] };
		}
		if (tool === 'shape') {
			const kind = editor.style.shape.kind;
			return kind === 'line' || kind === 'arrow'
				? { kind: 'segment', from: start, to: at }
				: { kind: 'area', area: box(start, at) };
		}
		if (tool === 'text' || tool === 'erase') return { kind: 'area', area: box(start, at) };
		return null;
	}

	/// Whether a drawn shape is big enough to be meant.
	function meant(geometry: Geometry, followsText: boolean) {
		const [width, height] = size();
		const minimum = (area: CropArea) =>
			(area[2] - area[0]) * width >= MIN_PIXELS && (area[3] - area[1]) * height >= MIN_PIXELS;
		switch (geometry.kind) {
			case 'area':
				return minimum(geometry.area);
			case 'segment':
				return (
					Math.hypot(
						(geometry.to[0] - geometry.from[0]) * width,
						(geometry.to[1] - geometry.from[1]) * height
					) >= MIN_PIXELS
				);
			case 'boxes':
				return geometry.boxes.length > 0 && (followsText || geometry.boxes.every(minimum));
			default:
				return true;
		}
	}

	function resized(origin: AnnotateMark, corner: Corner, [x, y]: PagePoint): AnnotateMark {
		const aspect = editor.aspect(origin);
		const from = extent(origin, page, aspect);
		const [width, height] = size();
		const [east, south] = [corner.includes('e'), corner.includes('s')];
		if (origin.kind === 'image' || origin.kind === 'picture') {
			// The opposite corner stays put and the shape never changes, as in Sign.
			const ratio = (aspect * page.width) / page.height;
			const span = from[2] - from[0];
			const [anchorX, anchorY] = [
				east ? from[0] : from[2],
				south ? from[1] : from[1] + span * ratio
			];
			const room = Math.min(east ? 1 - anchorX : anchorX, (south ? 1 - anchorY : anchorY) / ratio);
			const across = Math.max(Math.abs(x - anchorX), Math.abs(y - anchorY) / ratio);
			const next = Math.min(room, Math.max(32 / width, across));
			const left = east ? anchorX : anchorX - next;
			const top = south ? anchorY : anchorY - next * ratio;
			return refit(origin, from, [left, top, left + next, top + next * ratio]);
		}
		const [minimumX, minimumY] = [MIN_PIXELS / width, MIN_PIXELS / height];
		const [left, top, right, bottom] = from;
		// A box sized by hand stops growing to fit its text.
		if (origin.kind === 'text' && origin.fit) origin = { ...origin, fit: false };
		return refit(origin, from, [
			east ? left : Math.min(right - minimumX, x),
			south ? top : Math.min(bottom - minimumY, y),
			east ? Math.max(left + minimumX, x) : right,
			south ? Math.max(top + minimumY, y) : bottom
		]);
	}

	function update(event: PointerEvent) {
		if (!drag) {
			const mouse = editable && event.pointerType === 'mouse';
			ghost = mouse && tool === 'image' && editor.image !== null ? point(event) : null;
			// Only over the page itself: a mark under the pointer answers first.
			const free = mouse && editing && editor.editing === null && event.target === root;
			const at = free ? point(event) : null;
			const over = at && (tool === 'text' || tool === 'select') ? runNear(at) : undefined;
			if (over !== (hovered ?? undefined)) hovered = over ?? null;
			const image = at && tool === 'select' && !over ? imageAt(at) : null;
			if (image !== hoveredImage) hoveredImage = image;
			return;
		}
		if (event.pointerId !== drag.pointer) return;
		const at = point(event);
		if (drag.kind === 'create' && tool === 'ink' && draft?.kind === 'stroke') {
			// Coalesced events keep fast strokes round; points closer than half
			// a point add nothing the engine would draw.
			const points = [...draft.points];
			for (const sample of event.getCoalescedEvents?.() ?? [event]) {
				const next = point(sample);
				const last = points[points.length - 1];
				if (Math.hypot((next[0] - last[0]) * page.width, (next[1] - last[1]) * page.height) >= 0.5)
					points.push(next);
			}
			draft = { kind: 'stroke', points };
			drag.moved = points.length > 1;
			return;
		}
		const [dx, dy] = [at[0] - drag.start[0], at[1] - drag.start[1]];
		const [width, height] = size();
		if (!drag.moved) {
			if (Math.hypot(dx * width, dy * height) < CLICK_PIXELS) return;
			drag.moved = true;
			if (drag.kind !== 'create' && !(drag.kind === 'move' && drag.fresh)) editor.snapshot();
		}
		switch (drag.kind) {
			case 'create':
				draft = shaped(drag.start, at, drag.glyph);
				break;
			case 'move':
				editor.change(
					drag.origin.id,
					translate(drag.origin, dx, dy, page, editor.aspect(drag.origin))
				);
				break;
			case 'corner':
				editor.change(drag.origin.id, resized(drag.origin, drag.corner, at));
				break;
			case 'end': {
				const origin = drag.origin;
				if (origin.kind !== 'line' && origin.kind !== 'arrow') break;
				const other = drag.end === 'from' ? origin.to : origin.from;
				if (Math.hypot((at[0] - other[0]) * width, (at[1] - other[1]) * height) >= MIN_PIXELS)
					editor.change(origin.id, { ...origin, [drag.end]: at });
			}
		}
	}

	function end(event: PointerEvent) {
		if (!drag || event.pointerId !== drag.pointer) return;
		root?.releasePointerCapture(event.pointerId);
		const finished = drag;
		const geometry = draft;
		drag = null;
		draft = null;
		// An erased area that moved matches the page where it now is.
		if (finished.kind !== 'create' && finished.moved && finished.origin.kind === 'erase') {
			const mark = editor.marks.find((existing) => existing.id === finished.origin.id);
			if (mark?.kind === 'erase') {
				const matched = colorAround(canvas(), mark.area);
				editor.change(mark.id, { ...mark, matched, color: mark.match ? matched : mark.color });
			}
		}
		if (finished.kind === 'move' && !finished.moved) {
			// A note opens on a click; a selected text box opens on another, or on
			// the first with Edit's text tool.
			const mark = finished.origin;
			if (
				mark.kind === 'note' ||
				(mark.kind === 'text' && (finished.wasSelected || (editing && tool === 'text')))
			)
				editor.edit(mark.id);
			return;
		}
		if (finished.kind !== 'create') return;
		if (finished.moved) {
			if (geometry && meant(geometry, finished.glyph >= 0))
				editor.create(
					page,
					geometry,
					geometry.kind === 'area' ? matchedAt(geometry.area) : undefined
				);
			return;
		}
		const [x, y] = finished.start;
		switch (tool) {
			case 'markup': {
				if (finished.over !== null) return editor.select(finished.over);
				const word = glyphs ? wordAt(glyphs, x, y) : [];
				if (word.length) editor.create(page, { kind: 'boxes', boxes: word });
				else editor.select(null);
				break;
			}
			case 'ink':
				if (geometry) editor.create(page, geometry);
				break;
			case 'note':
				// Centred on the click.
				editor.create(page, {
					kind: 'point',
					at: [
						Math.max(0, x - NOTE_SIZE / 2 / page.width),
						Math.max(0, y - NOTE_SIZE / 2 / page.height)
					]
				});
				break;
			case 'text':
				if (finished.run) editRun(finished.run, x);
				else editor.create(page, { kind: 'point', at: [x, y] });
				break;
			case 'image':
				editor.create(page, { kind: 'point', at: [x, y] });
				break;
			default:
				editor.select(null);
		}
	}

	function focus(node: HTMLElement) {
		node.focus({ preventScroll: true });
		if (node instanceof HTMLTextAreaElement) {
			const at = Math.min(editor.caret ?? node.value.length, node.value.length);
			node.setSelectionRange(at, at);
		}
	}

	function label(mark: AnnotateMark) {
		const names: Record<AnnotateMark['kind'], string> = {
			highlight: 'Highlight',
			underline: 'Underline',
			strikeout: 'Strikethrough',
			squiggly: 'Squiggly underline',
			ink: 'Drawing',
			rectangle: 'Rectangle',
			ellipse: 'Ellipse',
			line: 'Line',
			arrow: 'Arrow',
			text: 'Text box',
			note: 'Note',
			image: 'Image',
			erase: 'Erased area',
			picture: 'Image in the PDF'
		};
		const name =
			mark.kind === 'picture' && mark.deleted
				? 'Deleted image'
				: mark.kind === 'text' && mark.replaces
					? mark.text.trim()
						? `Edited text, ${mark.text.trim().slice(0, 60)}`
						: 'Deleted text'
					: mark.kind === 'text' && mark.text.trim()
						? `Text box, ${mark.text.trim().slice(0, 60)}`
						: names[mark.kind];
		const hints =
			groupOf(mark.kind) === 'markup'
				? 'Delete removes it.'
				: mark.kind === 'text' && mark.replaces
					? 'Enter edits it, arrow keys move it, Delete deletes the text.'
					: mark.kind === 'picture'
						? 'Arrow keys move it, plus and minus resize it, Delete deletes it.'
						: mark.kind === 'text' || mark.kind === 'note'
							? 'Enter edits it, arrow keys move it, Delete removes it.'
							: 'Arrow keys move it, plus and minus resize it, Delete removes it.';
		return `${name}. ${hints}`;
	}

	const percent = (value: number) => `${(value * 100).toFixed(3)}%`;
	const place = ([left, top, right, bottom]: CropArea) =>
		`left: ${percent(left)}; top: ${percent(top)}; width: ${percent(right - left)}; height: ${percent(bottom - top)}`;
	// Small marks still get a target a finger can find.
	function target(area: CropArea): CropArea {
		const [width, height] = [pixels, (pixels * page.height) / page.width];
		if (!width) return area;
		const [padX, padY] = [
			Math.max(0, (16 - (area[2] - area[0]) * width) / 2 / width),
			Math.max(0, (16 - (area[3] - area[1]) * height) / 2 / height)
		];
		return [area[0] - padX, area[1] - padY, area[2] + padX, area[3] + padY];
	}
	const cursor = $derived(
		!editable
			? ''
			: tool === 'select'
				? hovered || hoveredImage
					? 'cursor-move'
					: 'cursor-default'
				: tool === 'markup' || (tool === 'text' && hovered)
					? 'cursor-text'
					: tool === 'note' || tool === 'image'
						? editor.image === null && tool === 'image'
							? 'cursor-default'
							: 'cursor-copy'
						: 'cursor-crosshair'
	);
</script>

<!-- Gestures on the page make annotations with the current tool; marks are
picked, moved and resized where the tool lets them be. -->
<div
	bind:this={root}
	bind:clientWidth={pixels}
	role="presentation"
	class="absolute inset-0 {!editable
		? 'pointer-events-none'
		: tool === 'select'
			? cursor
			: `touch-none ${cursor}`}"
	onpointerdown={(event) => begin(event)}
	onpointermove={update}
	onpointerup={end}
	onpointercancel={end}
	onpointerleave={() => {
		ghost = null;
		hovered = hoveredImage = null;
	}}
>
	<!-- Where replaced text was, painted in what was behind it, wherever its
	replacement has moved to. -->
	{#each marks as mark (mark.id)}
		{#if mark.kind === 'picture'}
			<div
				class="pointer-events-none absolute {mark.deleted
					? 'outline-1 -outline-offset-1 outline-brand/35 outline-dashed'
					: ''}"
				style="{place(mark.from)}; background-color: {hexColor(mark.cover)}"
			></div>
		{:else if mark.replaces}
			<div
				class="pointer-events-none absolute {mark.kind === 'text' && !mark.text.trim()
					? 'outline-1 -outline-offset-1 outline-brand/35 outline-dashed'
					: ''}"
				style="{place(mark.replaces.ink)}; background-color: {hexColor(mark.replaces.cover)}"
			></div>
		{/if}
	{/each}

	{#if (hovered || hoveredImage) && (tool === 'text' || tool === 'select') && editor.editing === null}
		{@const area = hovered?.box ?? hoveredImage}
		{#if area}
			<div
				class="pointer-events-none absolute rounded-[3px] outline-1 outline-offset-2 outline-brand/70 outline-dashed"
				style={place(area)}
				transition:fade={{ duration: reducedMotion ? 0 : 120 }}
			></div>
		{/if}
	{/if}

	{#each marks as mark (mark.id)}
		<div
			class="pointer-events-none absolute inset-0"
			transition:fade={{ duration: reducedMotion ? 0 : 160, easing: cubicOut }}
		>
			<AnnotationShape
				{page}
				{mark}
				image={mark.kind === 'image' ? editor.images[mark.image] : undefined}
				hideText={mark.id === editor.editing}
			/>
			{#if mark.kind === 'erase'}
				<!-- White on white would leave nothing to find it by. -->
				<div
					class="absolute outline-1 -outline-offset-1 outline-brand/35 outline-dashed"
					style={place(mark.area)}
				></div>
			{/if}
		</div>
	{/each}

	{#each marks as mark (mark.id)}
		{@const live = interactive(mark)}
		{@const areas =
			mark.kind === 'highlight' ||
			mark.kind === 'underline' ||
			mark.kind === 'strikeout' ||
			mark.kind === 'squiggly'
				? mark.boxes
				: [bounds(mark, page, editor.aspect(mark))]}
		{#each areas as area, index (index)}
			{#if index === 0}
				<button
					type="button"
					aria-label={label(mark)}
					aria-pressed={editor.selected === mark.id}
					tabindex={editable ? 0 : -1}
					class="absolute rounded-[2px] {live
						? groupOf(mark.kind) === 'markup'
							? 'cursor-pointer'
							: 'cursor-move touch-none'
						: 'pointer-events-none'} outline-brand/70 focus-visible:outline-2 focus-visible:outline-brand {editor.selected !==
						mark.id && live
						? 'hover:outline-1 hover:outline-dashed'
						: ''}"
					style={place(target(area))}
					onpointerdown={(event) =>
						tool === 'markup' && groupOf(mark.kind) === 'markup' && editor.selected !== mark.id
							? begin(event, mark.id)
							: grab(event, mark, 'move')}
					onfocus={() => editable && editor.selected !== mark.id && editor.select(mark.id)}
					onkeydown={(event) => {
						if (event.key === 'Enter' && (mark.kind === 'text' || mark.kind === 'note')) {
							event.preventDefault();
							editor.edit(mark.id);
						}
					}}
				></button>
			{:else}
				<div
					role="presentation"
					class="absolute {live ? 'cursor-pointer' : 'pointer-events-none'}"
					style={place(target(area))}
					onpointerdown={(event) =>
						tool === 'markup' && editor.selected !== mark.id
							? begin(event, mark.id)
							: grab(event, mark, 'move')}
				></div>
			{/if}
		{/each}
	{/each}

	{#if selected && editable && selected.id !== editor.editing}
		{@const area = bounds(selected, page, editor.aspect(selected))}
		{@const group = groupOf(selected.kind)}
		{@const resizable =
			selected.kind === 'ink' ||
			selected.kind === 'rectangle' ||
			selected.kind === 'ellipse' ||
			selected.kind === 'text' ||
			selected.kind === 'image' ||
			selected.kind === 'erase' ||
			(selected.kind === 'picture' && !selected.deleted)}
		{@const nearTop = area[1] * ((pixels * page.height) / page.width) < 18}
		<div
			class="pointer-events-none absolute rounded-[2px] outline-1 outline-brand outline-dashed"
			style={place(area)}
			transition:fade={{ duration: reducedMotion ? 0 : 120 }}
		>
			{#if resizable}
				{#each corners as item (item.corner)}
					<div
						role="presentation"
						class="pointer-events-auto absolute size-7 touch-none {item.place}"
						style:cursor={item.corner === 'nw' || item.corner === 'se'
							? 'nwse-resize'
							: 'nesw-resize'}
						onpointerdown={(event) => grab(event, selected, item.corner)}
					>
						<span
							class="absolute size-2.5 border-brand drop-shadow-[0_0_1px_rgb(11_11_13/0.5)] {item.bracket}"
						></span>
					</div>
				{/each}
			{/if}
			<button
				type="button"
				aria-label={selected.replaces
					? 'Restore the original text'
					: selected.kind === 'picture'
						? 'Restore the original image'
						: `Remove this ${group === 'markup' ? 'markup' : editing ? 'edit' : 'annotation'}`}
				title={selected.replaces || selected.kind === 'picture' ? 'Restore original' : 'Remove'}
				class="pointer-events-auto absolute right-0 flex size-6 translate-x-1/2 items-center justify-center rounded-full bg-canvas text-white shadow-lg ring-1 ring-white/15 hover:bg-brand hover:text-canvas motion-safe:transition-colors {nearTop
					? 'bottom-0 translate-y-1/2'
					: 'top-0 -translate-y-1/2'}"
				onpointerdown={(event) => event.stopPropagation()}
				onclick={() => editor.remove(selected.id)}
				in:scale={{ start: 0.6, duration: reducedMotion ? 0 : 180, easing: cubicOut }}
				><IconX size={13} stroke={2.75} /></button
			>
		</div>
		{#if selected.kind === 'line' || selected.kind === 'arrow'}
			{#each ends as end (end)}
				{@const [x, y] = selected[end]}
				<div
					role="presentation"
					class="absolute size-7 -translate-1/2 cursor-grab touch-none"
					style:left={percent(x)}
					style:top={percent(y)}
					onpointerdown={(event) => grab(event, selected, end)}
				>
					<span
						class="absolute top-1/2 left-1/2 size-3 -translate-1/2 rounded-full border-2 border-brand bg-canvas shadow"
					></span>
				</div>
			{/each}
		{/if}
	{/if}

	{#if editingMark?.kind === 'text'}
		{@const family = families[editingMark.family]}
		{@const [left, top, right, bottom] = editingMark.area}
		{@const shift =
			(capHeight(editingMark.family, editingMark.bold) -
				family.ascent -
				(LEADING - family.ascent - family.descent) / 2) *
			editingMark.size *
			zoom}
		<!-- Shifted so the first baseline sits where the engine puts it: the
		padding plus the cap height below the top. -->
		<textarea
			use:focus
			aria-label="Text box"
			spellcheck="false"
			value={editingMark.text}
			oninput={(event) => editor.type(editingMark.id, event.currentTarget.value)}
			onblur={() => editor.finishEditing()}
			onpointerdown={(event) => event.stopPropagation()}
			onkeydown={(event) => {
				event.stopPropagation();
				if (event.key === 'Escape') {
					event.preventDefault();
					event.currentTarget.blur();
				}
			}}
			class="absolute resize-none overflow-hidden bg-transparent outline-1 outline-brand outline-dashed focus-visible:outline-1 focus-visible:outline-brand"
			style:left={percent(left)}
			style:top="calc({percent(top)} + {shift}px)"
			style:width={percent(right - left)}
			style:height="calc({percent(bottom - top)} - {shift}px)"
			style:padding="{TEXT_PADDING * zoom}px"
			style:font-family={family.css}
			style:font-weight={editingMark.bold ? 700 : 400}
			style:font-size="{editingMark.size * zoom}px"
			style:line-height={LEADING}
			style:color={hexColor(editingMark.color)}
			style:caret-color={hexColor(editingMark.color)}></textarea>
	{:else if editingMark?.kind === 'note'}
		{@const [left, top, right] = extent(editingMark, page)}
		{@const height = (pixels * page.height) / page.width}
		{@const across =
			right * pixels + 8 + 232 <= pixels ? right * pixels + 8 : left * pixels - 8 - 232}
		<div
			class="absolute w-58 rounded-xl bg-panel p-1.5 shadow-2xl ring-1 shadow-black/40 ring-white/10"
			style:left="{Math.max(4, across)}px"
			style:top="{Math.max(4, Math.min(top * height, height - 124))}px"
			onpointerdown={(event) => event.stopPropagation()}
			role="presentation"
			transition:scale={{ start: 0.94, duration: reducedMotion ? 0 : 180, easing: cubicOut }}
		>
			<textarea
				use:focus
				aria-label="Note"
				placeholder="Note"
				rows="4"
				value={editingMark.comment}
				oninput={(event) => editor.type(editingMark.id, event.currentTarget.value)}
				onblur={() => editor.finishEditing()}
				onkeydown={(event) => {
					event.stopPropagation();
					if (event.key === 'Escape') {
						event.preventDefault();
						event.currentTarget.blur();
					}
				}}
				class="block w-full resize-none rounded-lg bg-canvas px-3 py-2.5 text-sm text-white outline-none placeholder:text-white/30"
			></textarea>
		</div>
	{/if}

	{#if draftMark && draft}
		<div class="pointer-events-none absolute inset-0 opacity-80">
			<AnnotationShape {page} mark={draftMark} />
		</div>
		{#if draft.kind === 'area'}
			<div
				class="pointer-events-none absolute outline-1 outline-brand outline-dashed"
				style={place(draft.area)}
			></div>
		{/if}
	{/if}
	{#if ghostMark}
		<div
			class="pointer-events-none absolute inset-0 opacity-35"
			transition:fade={{ duration: reducedMotion ? 0 : 120 }}
		>
			<AnnotationShape
				{page}
				mark={ghostMark}
				image={editor.image === null ? undefined : editor.images[editor.image]}
			/>
		</div>
	{/if}
</div>
