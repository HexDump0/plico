<script lang="ts">
	import { cubicOut } from 'svelte/easing';
	import { fade } from 'svelte/transition';
	import type { PreviewPage } from '$lib/pdf/stamp-layout';

	let {
		page,
		place,
		imageUrl,
		aspect,
		placed,
		editable,
		reducedMotion,
		onchange
	}: {
		page: PreviewPage;
		/// Left edge, top edge and width as fractions of the page, from its
		/// top left; the height follows from the image.
		place: [number, number, number];
		imageUrl: string;
		/// The signature's height over its width.
		aspect: number;
		/// Whether this page carries the signature. Clicking a page that does
		/// not puts the signature there.
		placed: boolean;
		editable: boolean;
		reducedMotion: boolean;
		onchange: (place: [number, number, number]) => void;
	} = $props();

	type Corner = 'nw' | 'ne' | 'se' | 'sw';
	const corners = [
		{ corner: 'nw', place: '-top-3 -left-3', bracket: 'top-3 left-3 border-t-3 border-l-3' },
		{ corner: 'ne', place: '-top-3 -right-3', bracket: 'top-3 right-3 border-t-3 border-r-3' },
		{
			corner: 'se',
			place: '-right-3 -bottom-3',
			bracket: 'right-3 bottom-3 border-r-3 border-b-3'
		},
		{ corner: 'sw', place: '-bottom-3 -left-3', bracket: 'bottom-3 left-3 border-b-3 border-l-3' }
	] as const;
	const MIN_PIXELS = 32;

	let root = $state<HTMLDivElement>();
	let drag = $state<{
		kind: 'move' | Corner | 'place';
		pointer: number;
		start: [number, number];
		origin: [number, number, number];
		moved: boolean;
	} | null>(null);
	// Where a click would put the signature, shown faintly under a mouse.
	let ghost = $state<[number, number] | null>(null);

	// Height as a fraction of the page height for a width fraction.
	const ratio = $derived((aspect * page.width) / page.height);
	const [left, top, width] = $derived(place);
	const height = $derived(width * ratio);

	function point(event: PointerEvent): [number, number] {
		const box = root!.getBoundingClientRect();
		return [(event.clientX - box.left) / box.width, (event.clientY - box.top) / box.height];
	}

	function minimumWidth() {
		return Math.min(0.5, MIN_PIXELS / root!.getBoundingClientRect().width);
	}

	// Kept wholly on the page, and no wider than fits on it.
	function fitted([l, t, w]: [number, number, number]): [number, number, number] {
		const widest = Math.min(1, 1 / ratio);
		const size = Math.min(widest, Math.max(minimumWidth(), w));
		return [Math.min(1 - size, Math.max(0, l)), Math.min(1 - size * ratio, Math.max(0, t)), size];
	}

	// Centred on a point.
	function at([x, y]: [number, number]): [number, number, number] {
		return fitted([x - width / 2, y - height / 2, width]);
	}

	function begin(event: PointerEvent, kind: 'move' | Corner | 'place') {
		if (!editable || !root || !imageUrl || event.button !== 0) return;
		event.preventDefault();
		event.stopPropagation();
		root.setPointerCapture(event.pointerId);
		ghost = null;
		drag = { kind, pointer: event.pointerId, start: point(event), origin: place, moved: false };
	}

	function update(event: PointerEvent) {
		if (!drag) {
			ghost =
				editable && imageUrl && !placed && event.pointerType === 'mouse' ? point(event) : null;
			return;
		}
		if (event.pointerId !== drag.pointer) return;
		const [x, y] = point(event);
		const [dx, dy] = [x - drag.start[0], y - drag.start[1]];
		const box = root!.getBoundingClientRect();
		if (!drag.moved && Math.hypot(dx * box.width, dy * box.height) < 4) return;
		drag.moved = true;
		const [l, t, w] = drag.origin;
		if (drag.kind === 'place') {
			onchange(at([x, y]));
		} else if (drag.kind === 'move') {
			onchange(fitted([l + dx, t + dy, w]));
		} else {
			// The opposite corner stays put; the width follows whichever way
			// the pointer pulls further, so the shape never changes.
			const [east, south] = [drag.kind.includes('e'), drag.kind.includes('s')];
			const [anchorX, anchorY] = [east ? l : l + w, south ? t : t + w * ratio];
			const across = Math.abs(x - anchorX);
			const down = Math.abs(y - anchorY) / ratio;
			const room = Math.min(east ? 1 - anchorX : anchorX, (south ? 1 - anchorY : anchorY) / ratio);
			const size = Math.min(room, Math.max(minimumWidth(), Math.max(across, down)));
			onchange([east ? anchorX : anchorX - size, south ? anchorY : anchorY - size * ratio, size]);
		}
	}

	function end(event: PointerEvent) {
		if (!drag || event.pointerId !== drag.pointer) return;
		root?.releasePointerCapture(event.pointerId);
		// A click puts the signature where it landed.
		if (drag.kind === 'place' && !drag.moved) onchange(at(point(event)));
		drag = null;
	}

	// Arrows move two points, or twenty with Shift; plus and minus resize.
	function nudge(event: KeyboardEvent) {
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
			onchange(fitted([left + moves[event.key][0], top + moves[event.key][1], width]));
		} else if (event.key === '+' || event.key === '=' || event.key === '-') {
			event.preventDefault();
			const size = width * (event.key === '-' ? 0.9 : 1.1);
			// Grows and shrinks about its centre.
			onchange(fitted([left + (width - size) / 2, top + ((width - size) * ratio) / 2, size]));
		}
	}

	const percent = (value: number) => `${(value * 100).toFixed(3)}%`;
	const glide = 'cubic-bezier(0.22, 1, 0.36, 1)';
</script>

<div
	bind:this={root}
	role="presentation"
	class="absolute inset-0 {editable && imageUrl ? 'cursor-copy touch-none' : 'pointer-events-none'}"
	onpointerdown={(event) => begin(event, 'place')}
	onpointermove={update}
	onpointerup={end}
	onpointercancel={end}
	onpointerleave={() => (ghost = null)}
>
	{#if imageUrl && placed}
		<div
			class="group absolute"
			style:left={percent(left)}
			style:top={percent(top)}
			style:width={percent(width)}
			style:height={percent(height)}
			style:transition={drag?.moved || reducedMotion
				? 'none'
				: `left 320ms ${glide}, top 320ms ${glide}, width 320ms ${glide}, height 320ms ${glide}`}
			transition:fade={{ duration: reducedMotion ? 0 : 200, easing: cubicOut }}
		>
			<img
				src={imageUrl}
				alt=""
				draggable="false"
				class="pointer-events-none size-full select-none"
			/>
			{#if editable}
				<button
					type="button"
					aria-label="Signature. Arrow keys move it, plus and minus resize it."
					class="absolute inset-0 cursor-move rounded-[2px] outline-1 outline-convert/70 outline-dashed focus-visible:bg-convert/10 focus-visible:outline-2 focus-visible:outline-solid motion-safe:transition-colors"
					onpointerdown={(event) => begin(event, 'move')}
					onkeydown={nudge}
				></button>
				{#each corners as item (item.corner)}
					<div
						role="presentation"
						class="absolute size-8 {item.place}"
						style:cursor={item.corner === 'nw' || item.corner === 'se'
							? 'nwse-resize'
							: 'nesw-resize'}
						onpointerdown={(event) => begin(event, item.corner)}
					>
						<span
							class="absolute size-3 border-convert drop-shadow-[0_0_1px_rgb(11_11_13/0.5)] {item.bracket}"
						></span>
					</div>
				{/each}
			{/if}
		</div>
	{/if}
	{#if ghost && imageUrl}
		{@const [x, y] = ghost}
		<img
			src={imageUrl}
			alt=""
			aria-hidden="true"
			draggable="false"
			class="pointer-events-none absolute opacity-35 select-none"
			style:left={percent(Math.min(1 - width, Math.max(0, x - width / 2)))}
			style:top={percent(Math.min(1 - height, Math.max(0, y - height / 2)))}
			style:width={percent(width)}
			style:height={percent(height)}
			transition:fade={{ duration: reducedMotion ? 0 : 120 }}
		/>
	{/if}
</div>
