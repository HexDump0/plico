<script lang="ts">
	import { cropSize, isFullPage } from '$lib/pdf/crop-area';
	import type { CropArea } from '$lib/pdf/types';

	let {
		area,
		width,
		height,
		editable,
		drawable,
		reducedMotion,
		onchange
	}: {
		area: CropArea;
		/// The page in points as displayed, for keyboard steps and the size.
		width: number;
		height: number;
		/// Handles to resize the area and its body to move it.
		editable: boolean;
		/// Dragging across the page draws a new area.
		drawable: boolean;
		reducedMotion: boolean;
		onchange: (area: CropArea) => void;
	} = $props();

	type Handle = 'nw' | 'n' | 'ne' | 'e' | 'se' | 's' | 'sw' | 'w' | 'move' | 'draw';
	// Which of left, top, right and bottom each handle drags.
	const edges: Record<Handle, [boolean, boolean, boolean, boolean]> = {
		nw: [true, true, false, false],
		n: [false, true, false, false],
		ne: [false, true, true, false],
		e: [false, false, true, false],
		se: [false, false, true, true],
		s: [false, false, false, true],
		sw: [true, false, false, true],
		w: [true, false, false, false],
		move: [true, true, true, true],
		draw: [false, false, false, false]
	};
	const cursors: Record<Handle, string> = {
		nw: 'nwse-resize',
		n: 'ns-resize',
		ne: 'nesw-resize',
		e: 'ew-resize',
		se: 'nwse-resize',
		s: 'ns-resize',
		sw: 'nesw-resize',
		w: 'ew-resize',
		move: 'move',
		draw: 'crosshair'
	};
	// Brackets and bars sit just inside the edge, so they stay whole when the
	// area meets the page edge and everything past it is clipped.
	const corners = [
		{ handle: 'nw', place: '-top-3 -left-3', bracket: 'top-3 left-3 border-t-3 border-l-3' },
		{ handle: 'ne', place: '-top-3 -right-3', bracket: 'top-3 right-3 border-t-3 border-r-3' },
		{
			handle: 'se',
			place: '-right-3 -bottom-3',
			bracket: 'right-3 bottom-3 border-r-3 border-b-3'
		},
		{ handle: 'sw', place: '-bottom-3 -left-3', bracket: 'bottom-3 left-3 border-b-3 border-l-3' }
	] as const;
	const sides = [
		{
			handle: 'n',
			index: 1,
			label: 'Top edge',
			place: 'inset-x-7 -top-2 h-6',
			bar: 'top-2 left-1/2 h-[3px] w-6 -translate-x-1/2'
		},
		{
			handle: 'e',
			index: 2,
			label: 'Right edge',
			place: 'inset-y-7 -right-2 w-6',
			bar: 'top-1/2 right-2 h-6 w-[3px] -translate-y-1/2'
		},
		{
			handle: 's',
			index: 3,
			label: 'Bottom edge',
			place: 'inset-x-7 -bottom-2 h-6',
			bar: 'bottom-2 left-1/2 h-[3px] w-6 -translate-x-1/2'
		},
		{
			handle: 'w',
			index: 0,
			label: 'Left edge',
			place: 'inset-y-7 -left-2 w-6',
			bar: 'top-1/2 left-2 h-6 w-[3px] -translate-y-1/2'
		}
	] as const;

	// Small enough to crop a margin, large enough to keep every handle apart.
	const MIN_PIXELS = 48;

	let root = $state<HTMLDivElement>();
	let drag = $state<{
		handle: Handle;
		pointer: number;
		start: [number, number];
		origin: CropArea;
		moved: boolean;
	} | null>(null);
	const [left, top, right, bottom] = $derived(area);
	const size = $derived(cropSize(area, width, height));
	// Nothing to move yet, so dragging anywhere draws.
	const whole = $derived(isFullPage(area));

	function point(event: PointerEvent): [number, number] {
		const box = root!.getBoundingClientRect();
		return [(event.clientX - box.left) / box.width, (event.clientY - box.top) / box.height];
	}

	function minimum(): [number, number] {
		const box = root!.getBoundingClientRect();
		return [Math.min(0.5, MIN_PIXELS / box.width), Math.min(0.5, MIN_PIXELS / box.height)];
	}

	function begin(event: PointerEvent, chosen: Handle) {
		const handle = chosen === 'move' && whole ? 'draw' : chosen;
		if (!root || event.button !== 0 || !(handle === 'draw' ? drawable : editable)) return;
		event.preventDefault();
		event.stopPropagation();
		root.setPointerCapture(event.pointerId);
		drag = { handle, pointer: event.pointerId, start: point(event), origin: area, moved: false };
	}

	// From `anchor` toward `reach`, at least `min` long and on the page.
	function span(anchor: number, reach: number, min: number): [number, number] {
		const end = Math.min(1, Math.max(0, reach));
		if (end >= anchor) {
			const to = Math.min(1, Math.max(end, anchor + min));
			return [Math.min(anchor, to - min), to];
		}
		const from = Math.max(0, Math.min(end, anchor - min));
		return [from, Math.max(anchor, from + min)];
	}

	// `origin` with the edges `handle` holds moved by `dx` and `dy`, kept on
	// the page and never smaller than the minimum.
	function shifted([l, t, r, b]: CropArea, handle: Handle, dx: number, dy: number): CropArea {
		if (handle === 'move') {
			const shiftX = Math.min(1 - r, Math.max(-l, dx));
			const shiftY = Math.min(1 - b, Math.max(-t, dy));
			return [l + shiftX, t + shiftY, r + shiftX, b + shiftY];
		}
		const [minWidth, minHeight] = minimum();
		const [moveLeft, moveTop, moveRight, moveBottom] = edges[handle];
		return [
			moveLeft ? Math.min(r - minWidth, Math.max(0, l + dx)) : l,
			moveTop ? Math.min(b - minHeight, Math.max(0, t + dy)) : t,
			moveRight ? Math.max(l + minWidth, Math.min(1, r + dx)) : r,
			moveBottom ? Math.max(t + minHeight, Math.min(1, b + dy)) : b
		];
	}

	function update(event: PointerEvent) {
		if (!drag || event.pointerId !== drag.pointer) return;
		const [x, y] = point(event);
		const [dx, dy] = [x - drag.start[0], y - drag.start[1]];
		const box = root!.getBoundingClientRect();
		// A click outside the area leaves it alone; only a drag draws.
		if (!drag.moved && Math.hypot(dx * box.width, dy * box.height) < 4) return;
		drag.moved = true;
		if (drag.handle === 'draw') {
			const [minWidth, minHeight] = minimum();
			const [from, to] = span(drag.start[0], x, minWidth);
			const [above, below] = span(drag.start[1], y, minHeight);
			onchange([from, above, to, below]);
		} else onchange(shifted(drag.origin, drag.handle, dx, dy));
	}

	function end(event: PointerEvent) {
		if (!drag || event.pointerId !== drag.pointer) return;
		root?.releasePointerCapture(event.pointerId);
		drag = null;
	}

	// Arrow keys step two points, or twenty with Shift. An edge only follows
	// the arrows along its own axis.
	function nudge(event: KeyboardEvent, handle: Handle) {
		const direction = {
			ArrowLeft: [-1, 0],
			ArrowRight: [1, 0],
			ArrowUp: [0, -1],
			ArrowDown: [0, 1]
		}[event.key];
		if (!direction) return;
		const [moveLeft, moveTop, moveRight, moveBottom] = edges[handle];
		if (direction[0] !== 0 ? !(moveLeft || moveRight) : !(moveTop || moveBottom)) return;
		event.preventDefault();
		const points = event.shiftKey ? 20 : 2;
		onchange(
			shifted(area, handle, (direction[0] * points) / width, (direction[1] * points) / height)
		);
	}

	const percent = (value: number) => `${(value * 100).toFixed(3)}%`;
</script>

<!-- Dragging across the page draws a new area, unless it starts inside one
that is already there, which moves it. Handles and arrow keys resize. -->
<div
	bind:this={root}
	role="presentation"
	class="absolute inset-0 {drawable ? 'touch-none' : 'pointer-events-none'}"
	style:cursor={drag ? cursors[drag.handle] : drawable ? 'crosshair' : undefined}
	onpointerdown={(event) => begin(event, 'draw')}
	onpointermove={update}
	onpointerup={end}
	onpointercancel={end}
>
	<div
		class="absolute"
		style:left={percent(left)}
		style:top={percent(top)}
		style:width={percent(right - left)}
		style:height={percent(bottom - top)}
		style:box-shadow="0 0 0 9999px rgb(11 11 13 / 0.6)"
		style:transition={drag || reducedMotion
			? 'none'
			: 'left 320ms cubic-bezier(0.22, 1, 0.36, 1), top 320ms cubic-bezier(0.22, 1, 0.36, 1), width 320ms cubic-bezier(0.22, 1, 0.36, 1), height 320ms cubic-bezier(0.22, 1, 0.36, 1)'}
	>
		<div aria-hidden="true" class="pointer-events-none absolute inset-0 border border-brand"></div>
		{#if editable}
			<button
				type="button"
				aria-label="Crop area, {size}"
				class="absolute inset-0 {whole
					? 'cursor-crosshair'
					: 'cursor-move'} focus-visible:bg-brand/10 focus-visible:outline-none motion-safe:transition-colors"
				onpointerdown={(event) => begin(event, 'move')}
				onkeydown={(event) => nudge(event, 'move')}
			></button>
			{#each sides as side (side.handle)}
				<div
					role="slider"
					tabindex="0"
					aria-label={side.label}
					aria-orientation={side.handle === 'n' || side.handle === 's' ? 'vertical' : 'horizontal'}
					aria-valuemin={0}
					aria-valuemax={100}
					aria-valuenow={Math.round(area[side.index] * 100)}
					aria-valuetext="{Math.round(area[side.index] * 100)}%"
					class="group absolute {side.place} focus-visible:outline-none"
					style:cursor={cursors[side.handle]}
					onpointerdown={(event) => begin(event, side.handle)}
					onkeydown={(event) => nudge(event, side.handle)}
				>
					<span
						class="absolute {side.bar} rounded-full bg-brand shadow-[0_0_0_1px_rgb(11_11_13/0.35)] group-focus-visible:shadow-[0_0_0_2px_white] motion-safe:transition-shadow"
					></span>
				</div>
			{/each}
			{#each corners as corner (corner.handle)}
				<div
					role="presentation"
					class="absolute size-10 {corner.place}"
					style:cursor={cursors[corner.handle]}
					onpointerdown={(event) => begin(event, corner.handle)}
				>
					<span
						class="absolute size-4 border-brand drop-shadow-[0_0_1px_rgb(11_11_13/0.5)] {corner.bracket}"
					></span>
				</div>
			{/each}
		{/if}
		<span
			aria-hidden="true"
			class="pointer-events-none absolute top-1/2 left-1/2 -translate-1/2 rounded-md bg-canvas/85 px-2 py-1 text-[11px] font-semibold whitespace-nowrap text-white tabular-nums backdrop-blur-sm motion-safe:transition-opacity motion-safe:duration-200 {drag?.moved
				? 'opacity-100'
				: 'opacity-0'}">{size}</span
		>
	</div>
</div>
