<script lang="ts">
	import { cubicOut } from 'svelte/easing';
	import { fade, scale } from 'svelte/transition';
	import type { SummaryPoint } from '$lib/pdf/summarize.svelte';
	import type { CropArea } from '$lib/pdf/types';

	let {
		points,
		page,
		current,
		focus,
		reducedMotion,
		onpick
	}: {
		/// Marked beside their paragraphs; none while asking.
		points: SummaryPoint[];
		page: number;
		current: number;
		/// The paragraph outlined, a point's or an answer's source.
		focus: { page: number; box?: CropArea } | null;
		reducedMotion: boolean;
		onpick: (index: number) => void;
	} = $props();

	const percent = (value: number) => `${(value * 100).toFixed(3)}%`;
	const place = ([left, top, right, bottom]: CropArea) =>
		`left:${percent(left)};top:${percent(top)};width:${percent(right - left)};height:${percent(bottom - top)}`;

	// Points traced to a paragraph on this page, each marked beside it.
	const here = $derived(
		points.flatMap((point, index) =>
			point.page === page && point.box ? [{ index, box: point.box }] : []
		)
	);
</script>

<div class="pointer-events-none absolute inset-0">
	{#if focus?.page === page && focus.box}
		{#key focus.box}
			<div
				transition:fade={{ duration: reducedMotion ? 0 : 160 }}
				class="absolute rounded-[3px] bg-compress/20 mix-blend-multiply outline-2 outline-offset-2 outline-compress"
				style={place(focus.box)}
			></div>
		{/key}
	{/if}
	{#each here as { index, box } (index)}
		<button
			type="button"
			in:scale={{ duration: reducedMotion ? 0 : 200, start: 0.6, easing: cubicOut }}
			onclick={() => onpick(index)}
			aria-label="Key point {index + 1}"
			aria-pressed={index === current}
			class="pointer-events-auto absolute flex size-5 -translate-x-[calc(100%+6px)] items-center justify-center rounded-md text-[11px] font-bold tabular-nums shadow-sm motion-safe:transition-colors {index ===
			current
				? 'bg-compress text-canvas'
				: 'bg-canvas/85 text-compress backdrop-blur-sm hover:bg-compress hover:text-canvas'}"
			style:left={percent(Math.max(box[0], 0.07))}
			style:top={percent(box[1])}>{index + 1}</button
		>
	{/each}
</div>
