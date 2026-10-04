<script lang="ts">
	import { cubicOut } from 'svelte/easing';
	import { fade } from 'svelte/transition';
	import type { OcrPageState } from '$lib/pdf/ocr.svelte';
	import type { CropArea } from '$lib/pdf/types';

	let {
		state,
		matches,
		current,
		reducedMotion
	}: {
		/// What reading this page has got to, or `skipped` when it already has
		/// text and is left alone.
		state: OcrPageState | 'skipped' | undefined;
		matches: CropArea[];
		current: CropArea[];
		reducedMotion: boolean;
	} = $props();

	const percent = (value: number) => `${(value * 100).toFixed(3)}%`;
	const place = ([left, top, right, bottom]: CropArea) =>
		`left:${percent(left)};top:${percent(top)};width:${percent(right - left)};height:${percent(bottom - top)}`;
</script>

<div aria-hidden="true" class="pointer-events-none absolute inset-0 overflow-hidden">
	{#if state === 'skipped'}
		<span
			transition:fade={{ duration: reducedMotion ? 0 : 160 }}
			class="absolute top-3 right-3 rounded-md bg-canvas/80 px-2 py-1 text-[11px] font-semibold text-white backdrop-blur-sm"
			>Has text</span
		>
	{:else if state?.status === 'reading'}
		<!-- The line sits where Tesseract has got to on this page; it waits at
		the top, breathing, while the page is drawn and laid out. -->
		<div
			out:fade={{ duration: reducedMotion ? 0 : 240 }}
			class="absolute inset-0 mix-blend-multiply"
		>
			<div
				class="absolute inset-x-0 top-0 bg-linear-to-b from-compress/0 via-compress/5 to-compress/25 motion-safe:transition-[height] motion-safe:duration-500 motion-safe:ease-linear"
				style:height={percent(state.progress)}
			></div>
		</div>
		<div
			out:fade={{ duration: reducedMotion ? 0 : 240 }}
			class="absolute inset-x-0 h-0.5 -translate-y-1/2 bg-compress shadow-[0_0_14px_2px] shadow-compress/60 motion-safe:transition-[top] motion-safe:duration-500 motion-safe:ease-linear {state.progress ===
			0
				? 'motion-safe:animate-pulse'
				: ''}"
			style:top={percent(state.progress)}
		></div>
	{:else if state?.status === 'done'}
		<!-- Words arrive from the top down, following the line that read them. -->
		{#each state.words as word, index (index)}
			<div
				in:fade={{
					duration: reducedMotion ? 0 : 280,
					delay: reducedMotion ? 0 : word.box[1] * 360,
					easing: cubicOut
				}}
				class="absolute rounded-[2px] bg-compress/20 mix-blend-multiply"
				style={place(word.box)}
			></div>
		{/each}
	{/if}
	{#each matches as box, index (index)}
		<div
			class="absolute rounded-[2px] bg-compress/30 outline-1 outline-compress/80"
			style={place(box)}
			transition:fade={{ duration: reducedMotion ? 0 : 160 }}
		></div>
	{/each}
	{#each current as box, index (index)}
		<div
			class="absolute rounded-[2px] bg-compress/45 outline-2 outline-offset-1 outline-compress"
			style={place(box)}
			transition:fade={{ duration: reducedMotion ? 0 : 160 }}
		></div>
	{/each}
</div>
