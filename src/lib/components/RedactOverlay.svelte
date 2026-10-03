<script lang="ts">
	import { cubicOut } from 'svelte/easing';
	import { fade, scale } from 'svelte/transition';
	import { IconX } from '@tabler/icons-svelte-runes';
	import { cropSize } from '$lib/pdf/crop-area';
	import { spilledGlyphs, wordAt } from '$lib/pdf/redact-text';
	import type { PreviewPage } from '$lib/pdf/stamp-layout';
	import type { CropArea, PageGlyphs, RedactMark } from '$lib/pdf/types';

	let {
		page,
		marks,
		selected,
		glyphs,
		matches,
		current,
		whole,
		fill,
		editable,
		reducedMotion,
		onadd,
		onedit,
		onchange,
		onselect,
		onremove
	}: {
		page: PreviewPage;
		/// This page's boxes.
		marks: RedactMark[];
		selected: number | null;
		glyphs?: PageGlyphs;
		/// Search hits on this page not yet covered, and the one being looked at.
		matches: CropArea[];
		current: CropArea[];
		/// Annotations that go whole if a box touches them.
		whole: CropArea[];
		/// The box colour, as CSS.
		fill: string;
		editable: boolean;
		reducedMotion: boolean;
		onadd: (areas: CropArea[]) => void;
		/// Before the first change of a drag, a key press or a removal, so the
		/// change can be undone as one step.
		onedit: () => void;
		onchange: (id: number, area: CropArea) => void;
		onselect: (id: number | null) => void;
		onremove: (id: number) => void;
	} = $props();

	type Handle = 'nw' | 'ne' | 'se' | 'sw' | 'move' | 'draw';
	// Which of left, top, right and bottom each handle drags.
	const edges: Record<Handle, [boolean, boolean, boolean, boolean]> = {
		nw: [true, true, false, false],
		ne: [false, true, true, false],
		se: [false, false, true, true],
		sw: [true, false, false, true],
		move: [true, true, true, true],
		draw: [false, false, false, false]
	};
	const corners = [
		{ handle: 'nw', place: '-top-3 -left-3', bracket: 'top-2.5 left-2.5 border-t-3 border-l-3' },
		{ handle: 'ne', place: '-top-3 -right-3', bracket: 'top-2.5 right-2.5 border-t-3 border-r-3' },
		{
			handle: 'se',
			place: '-right-3 -bottom-3',
			bracket: 'right-2.5 bottom-2.5 border-r-3 border-b-3'
		},
		{
			handle: 'sw',
			place: '-bottom-3 -left-3',
			bracket: 'bottom-2.5 left-2.5 border-b-3 border-l-3'
		}
	] as const;
	// A drag shorter than this is a click; a box smaller than this is a slip.
	const CLICK_PIXELS = 4;
	const MIN_PIXELS = 3;

	let root = $state<HTMLDivElement>();
	let drag = $state<{
		handle: Handle;
		id: number | null;
		pointer: number;
		start: [number, number];
		origin: CropArea;
		moved: boolean;
	} | null>(null);
	// The box being drawn, before it is added.
	let drawing = $state.raw<CropArea | null>(null);

	const spilled = $derived(
		glyphs
			? spilledGlyphs(
					glyphs,
					marks.map((mark) => mark.area)
				)
			: []
	);

	const touched = $derived(
		whole.filter(([left, top, right, bottom]) =>
			marks.some(({ area: [l, t, r, b] }) => left < r && l < right && top < b && t < bottom)
		)
	);

	function point(event: PointerEvent): [number, number] {
		const box = root!.getBoundingClientRect();
		return [
			Math.min(1, Math.max(0, (event.clientX - box.left) / box.width)),
			Math.min(1, Math.max(0, (event.clientY - box.top) / box.height))
		];
	}

	function minimum(): [number, number] {
		const box = root!.getBoundingClientRect();
		return [MIN_PIXELS / box.width, MIN_PIXELS / box.height];
	}

	function begin(event: PointerEvent, handle: Handle, mark?: RedactMark) {
		if (!editable || !root || event.button !== 0) return;
		event.preventDefault();
		event.stopPropagation();
		root.setPointerCapture(event.pointerId);
		if (mark) onselect(mark.id);
		const start = point(event);
		drag = {
			handle,
			id: mark?.id ?? null,
			pointer: event.pointerId,
			start,
			origin: mark?.area ?? [start[0], start[1], start[0], start[1]],
			moved: false
		};
	}

	// `origin` with the edges `handle` holds moved, kept on the page and
	// never turned inside out.
	function shifted([l, t, r, b]: CropArea, handle: Handle, dx: number, dy: number): CropArea {
		if (handle === 'move') {
			const x = Math.min(1 - r, Math.max(-l, dx));
			const y = Math.min(1 - b, Math.max(-t, dy));
			return [l + x, t + y, r + x, b + y];
		}
		const [width, height] = minimum();
		const [left, top, right, bottom] = edges[handle];
		return [
			left ? Math.min(r - width, Math.max(0, l + dx)) : l,
			top ? Math.min(b - height, Math.max(0, t + dy)) : t,
			right ? Math.max(l + width, Math.min(1, r + dx)) : r,
			bottom ? Math.max(t + height, Math.min(1, b + dy)) : b
		];
	}

	function update(event: PointerEvent) {
		if (!drag || event.pointerId !== drag.pointer) return;
		const [x, y] = point(event);
		const [dx, dy] = [x - drag.start[0], y - drag.start[1]];
		const box = root!.getBoundingClientRect();
		if (!drag.moved && Math.hypot(dx * box.width, dy * box.height) < CLICK_PIXELS) return;
		if (!drag.moved && drag.id !== null) onedit();
		drag.moved = true;
		if (drag.handle === 'draw') {
			const [sx, sy] = drag.start;
			drawing = [Math.min(sx, x), Math.min(sy, y), Math.max(sx, x), Math.max(sy, y)];
		} else if (drag.id !== null) {
			onchange(drag.id, shifted(drag.origin, drag.handle, dx, dy));
		}
	}

	function end(event: PointerEvent) {
		if (!drag || event.pointerId !== drag.pointer) return;
		root?.releasePointerCapture(event.pointerId);
		const finished = drag;
		drag = null;
		if (finished.handle !== 'draw') return;
		const drawn = drawing;
		drawing = null;
		if (finished.moved && drawn) {
			const [width, height] = minimum();
			if (drawn[2] - drawn[0] >= width && drawn[3] - drawn[1] >= height) onadd([drawn]);
			return;
		}
		// A click on text marks its word; anywhere else it lets go of the box.
		const word = glyphs ? wordAt(glyphs, ...finished.start) : [];
		if (word.length) onadd(word);
		else onselect(null);
	}

	// Arrows move two points, or twenty with Shift; plus and minus resize;
	// Delete removes.
	function key(event: KeyboardEvent, mark: RedactMark) {
		const [l, t, r, b] = mark.area;
		const step = (event.shiftKey ? 20 : 2) / page.width;
		const stepY = (event.shiftKey ? 20 : 2) / page.height;
		const moves: Record<string, [number, number]> = {
			ArrowLeft: [-step, 0],
			ArrowRight: [step, 0],
			ArrowUp: [0, -stepY],
			ArrowDown: [0, stepY]
		};
		if (moves[event.key]) {
			event.preventDefault();
			onedit();
			onchange(mark.id, shifted(mark.area, 'move', ...moves[event.key]));
		} else if (event.key === '+' || event.key === '=' || event.key === '-') {
			event.preventDefault();
			const [x, y] = event.key === '-' ? [-step, -stepY] : [step, stepY];
			const [width, height] = minimum();
			if (r - l + 2 * x < width || b - t + 2 * y < height) return;
			onedit();
			onchange(mark.id, [
				Math.max(0, l - x),
				Math.max(0, t - y),
				Math.min(1, r + x),
				Math.min(1, b + y)
			]);
		} else if (event.key === 'Delete' || event.key === 'Backspace') {
			event.preventDefault();
			onremove(mark.id);
		} else if (event.key === 'Escape') {
			onselect(null);
			(event.currentTarget as HTMLElement).blur();
		}
	}

	const percent = (value: number) => `${(value * 100).toFixed(3)}%`;
	const glide = 'cubic-bezier(0.22, 1, 0.36, 1)';
	const place = ([left, top, right, bottom]: CropArea) =>
		`left: ${percent(left)}; top: ${percent(top)}; width: ${percent(right - left)}; height: ${percent(bottom - top)}`;
</script>

<!-- Dragging across the page draws a box; clicking a word marks it. A box is
moved by dragging it and resized from its corners. -->
<div
	bind:this={root}
	role="presentation"
	class="absolute inset-0 {editable ? 'cursor-crosshair touch-none' : 'pointer-events-none'}"
	onpointerdown={(event) => begin(event, 'draw')}
	onpointermove={update}
	onpointerup={end}
	onpointercancel={end}
>
	{#each matches as box, index (index)}
		<div
			aria-hidden="true"
			class="pointer-events-none absolute rounded-[2px] bg-merge/25 outline-1 outline-merge/70"
			style={place(box)}
			transition:fade={{ duration: reducedMotion ? 0 : 160 }}
		></div>
	{/each}
	{#each current as box, index (index)}
		<div
			aria-hidden="true"
			class="pointer-events-none absolute rounded-[2px] bg-merge/40 outline-2 outline-offset-1 outline-merge"
			style={place(box)}
			in:scale={{ start: 1.4, opacity: 0, duration: reducedMotion ? 0 : 260, easing: cubicOut }}
		></div>
	{/each}
	<!-- Glyphs a box removes reach past it, and vanish there too. -->
	{#each spilled as box, index (index)}
		<div
			aria-hidden="true"
			class="pointer-events-none absolute bg-merge/30"
			style={place(box)}
		></div>
	{/each}
	<!-- Annotations drawn with no stored look go whole when a box touches them. -->
	{#each touched as box, index (index)}
		<div
			aria-hidden="true"
			class="pointer-events-none absolute rounded-[2px] bg-merge/15 outline-1 outline-merge/80 outline-dashed"
			style={place(box)}
			transition:fade={{ duration: reducedMotion ? 0 : 160 }}
		></div>
	{/each}
	{#each marks as mark (mark.id)}
		{@const active = editable && selected === mark.id}
		{@const nearTop = mark.area[1] * page.height < 18}
		<div
			class="absolute"
			style={place(mark.area)}
			style:transition={drag?.moved || reducedMotion
				? 'none'
				: `left 200ms ${glide}, top 200ms ${glide}, width 200ms ${glide}, height 200ms ${glide}`}
			transition:fade={{ duration: reducedMotion ? 0 : 160, easing: cubicOut }}
		>
			<div
				aria-hidden="true"
				class="pointer-events-none absolute inset-0 opacity-80"
				style:background-color={fill}
			></div>
			{#if editable}
				<button
					type="button"
					aria-label="Redaction area, {cropSize(
						mark.area,
						page.width,
						page.height
					)}. Arrow keys move it, plus and minus resize it, Delete removes it."
					aria-pressed={active}
					class="absolute inset-0 cursor-move outline-1 outline-merge/80 focus-visible:outline-2 motion-safe:transition-[outline-color] {active
						? 'outline-solid'
						: 'outline-dashed'}"
					onpointerdown={(event) => begin(event, 'move', mark)}
					onfocus={() => onselect(mark.id)}
					onkeydown={(event) => key(event, mark)}
				></button>
				{#if active}
					{#each corners as corner (corner.handle)}
						<div
							role="presentation"
							class="absolute size-7 {corner.place}"
							style:cursor={corner.handle === 'nw' || corner.handle === 'se'
								? 'nwse-resize'
								: 'nesw-resize'}
							onpointerdown={(event) => begin(event, corner.handle, mark)}
							transition:fade={{ duration: reducedMotion ? 0 : 120 }}
						>
							<span
								class="absolute size-2.5 border-merge drop-shadow-[0_0_1px_rgb(11_11_13/0.5)] {corner.bracket}"
							></span>
						</div>
					{/each}
					<button
						type="button"
						aria-label="Remove this area"
						title="Remove"
						class="absolute right-0 flex size-6 translate-x-1/2 items-center justify-center rounded-full bg-canvas text-white shadow-lg ring-1 ring-white/15 hover:bg-merge hover:text-canvas motion-safe:transition-colors {nearTop
							? 'bottom-0 translate-y-1/2'
							: 'top-0 -translate-y-1/2'}"
						onpointerdown={(event) => event.stopPropagation()}
						onclick={() => onremove(mark.id)}
						in:scale={{ start: 0.6, duration: reducedMotion ? 0 : 180, easing: cubicOut }}
						><IconX size={13} stroke={2.75} /></button
					>
				{/if}
			{/if}
		</div>
	{/each}
	{#if drawing}
		<div class="pointer-events-none absolute outline-1 outline-merge" style={place(drawing)}>
			<div class="absolute inset-0 opacity-60" style:background-color={fill}></div>
			<span
				class="absolute top-full left-1/2 mt-1.5 -translate-x-1/2 rounded-md bg-canvas/85 px-2 py-1 text-[11px] font-semibold whitespace-nowrap text-white tabular-nums backdrop-blur-sm"
				>{cropSize(drawing, page.width, page.height)}</span
			>
		</div>
	{/if}
</div>
