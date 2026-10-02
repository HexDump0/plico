<script lang="ts">
	import { IconArrowBackUp, IconX } from '@tabler/icons-svelte-runes';
	import { drawStrokes, type Stroke } from '$lib/pdf/signature';

	let {
		strokes = $bindable<Stroke[]>(),
		color,
		disabled = false,
		onfinish
	}: {
		strokes: Stroke[];
		color: string;
		disabled?: boolean;
		/// After each stroke, undo or clear: the moments the signature changes.
		onfinish: () => void;
	} = $props();

	let canvas = $state<HTMLCanvasElement>();
	let width = $state(0);
	let height = $state(0);
	let drawing = $state<Stroke | null>(null);
	let pointer = 0;

	// Redrawn whole on any change; a signature is a few hundred points.
	$effect(() => {
		const context = canvas?.getContext('2d');
		if (!canvas || !context || width === 0) return;
		const ratio = window.devicePixelRatio || 1;
		canvas.width = Math.round(width * ratio);
		canvas.height = Math.round(height * ratio);
		drawStrokes(context, drawing ? [...strokes, drawing] : strokes, color, ratio);
	});

	function at(event: PointerEvent) {
		const box = canvas!.getBoundingClientRect();
		return { x: event.clientX - box.left, y: event.clientY - box.top, time: event.timeStamp };
	}

	function start(event: PointerEvent) {
		if (disabled || !canvas || event.button !== 0) return;
		event.preventDefault();
		canvas.setPointerCapture(event.pointerId);
		pointer = event.pointerId;
		drawing = [at(event)];
	}

	function move(event: PointerEvent) {
		if (!drawing || event.pointerId !== pointer) return;
		// Coalesced events keep fast strokes round instead of angular.
		const samples = event.getCoalescedEvents?.() ?? [event];
		drawing = [...drawing, ...samples.map(at)];
	}

	function finish(event: PointerEvent) {
		if (!drawing || event.pointerId !== pointer) return;
		strokes = [...strokes, drawing];
		drawing = null;
		onfinish();
	}
</script>

<div
	class="relative h-36 overflow-hidden rounded-xl bg-white {disabled ? 'opacity-50' : ''}"
	bind:clientWidth={width}
	bind:clientHeight={height}
>
	<span
		aria-hidden="true"
		class="pointer-events-none absolute inset-x-5 bottom-9 border-b border-dashed border-black/15"
	></span>
	<canvas
		bind:this={canvas}
		aria-label="Signature pad"
		class="absolute inset-0 size-full cursor-crosshair touch-none"
		onpointerdown={start}
		onpointermove={move}
		onpointerup={finish}
		onpointercancel={finish}
	></canvas>
	{#if strokes.length}
		<div class="absolute top-2 right-2 flex gap-1">
			<button
				type="button"
				{disabled}
				onclick={() => {
					strokes = strokes.slice(0, -1);
					onfinish();
				}}
				aria-label="Undo stroke"
				title="Undo stroke"
				class="flex size-8 items-center justify-center rounded-lg bg-canvas/5 text-canvas/60 transition-colors hover:bg-canvas/10 hover:text-canvas"
				><IconArrowBackUp size={17} /></button
			>
			<button
				type="button"
				{disabled}
				onclick={() => {
					strokes = [];
					onfinish();
				}}
				aria-label="Clear signature"
				title="Clear signature"
				class="flex size-8 items-center justify-center rounded-lg bg-canvas/5 text-canvas/60 transition-colors hover:bg-convert hover:text-canvas"
				><IconX size={17} stroke={2.25} /></button
			>
		</div>
	{/if}
</div>
