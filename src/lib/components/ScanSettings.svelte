<script lang="ts">
	import { cubicOut } from 'svelte/easing';
	import { slide } from 'svelte/transition';
	import type { OcrModels } from '$lib/pdf/ocr.svelte';
	import type { ScanLook } from '$lib/pdf/types';
	import OcrLanguages from './OcrLanguages.svelte';
	import ToggleSwitch from './ToggleSwitch.svelte';

	let {
		look,
		paper = $bindable<'a4' | 'letter' | 'fit'>(),
		searchable = $bindable<boolean>(),
		languages = $bindable<string[]>(),
		models,
		onlook,
		reducedMotion,
		disabled = false
	}: {
		look: ScanLook;
		paper: 'a4' | 'letter' | 'fit';
		searchable: boolean;
		languages: string[];
		models: OcrModels;
		onlook: (look: ScanLook) => void;
		reducedMotion: boolean;
		disabled?: boolean;
	} = $props();

	const looks = [
		{ id: 'original', label: 'Original' },
		{ id: 'document', label: 'Document' },
		{ id: 'grayscale', label: 'Grayscale' },
		{ id: 'bw', label: 'B&W' }
	] as const;
	const papers = [
		{ id: 'a4', label: 'A4' },
		{ id: 'letter', label: 'US Letter' },
		{ id: 'fit', label: 'Fit scan' }
	] as const;
	const lookIndex = $derived(looks.findIndex((option) => option.id === look));
</script>

<div class="space-y-6">
	<div>
		<h2 class="mb-3 text-sm font-semibold">Look</h2>
		<div
			class="relative grid grid-cols-4 gap-1 rounded-xl bg-canvas p-1"
			role="group"
			aria-label="Look"
		>
			<span
				aria-hidden="true"
				class="pointer-events-none absolute inset-y-1 left-1 w-[calc((100%-1.25rem)/4)] rounded-lg bg-panel-hover motion-safe:transition-transform motion-safe:duration-200 motion-safe:ease-[cubic-bezier(0.22,1,0.36,1)]"
				style:transform="translateX(calc({lookIndex} * (100% + 0.25rem)))"
			></span>
			{#each looks as option (option.id)}<button
					type="button"
					aria-pressed={look === option.id}
					{disabled}
					class="relative z-10 rounded-lg px-1 py-3 text-xs font-semibold motion-safe:transition-colors {look ===
					option.id
						? 'text-convert'
						: 'text-muted enabled:hover:text-white'}"
					onclick={() => onlook(option.id)}>{option.label}</button
				>{/each}
		</div>
	</div>

	<div>
		<div class="mb-3 flex items-center justify-between">
			<h2 class="text-sm font-semibold">Page size</h2>
			<span class="text-xs text-muted">
				{paper === 'a4' ? '210 × 297 mm' : paper === 'letter' ? '8.5 × 11 in' : 'Per scan'}
			</span>
		</div>
		<div
			class="grid grid-cols-3 gap-1 rounded-xl bg-canvas p-1"
			role="group"
			aria-label="Page size"
		>
			{#each papers as option (option.id)}
				<button
					type="button"
					aria-pressed={paper === option.id}
					{disabled}
					class="rounded-lg px-2 py-3 text-xs font-semibold motion-safe:transition-colors {paper ===
					option.id
						? 'bg-panel-hover text-convert'
						: 'text-muted enabled:hover:text-white'}"
					onclick={() => (paper = option.id)}>{option.label}</button
				>
			{/each}
		</div>
	</div>

	<div class="space-y-4">
		<ToggleSwitch bind:checked={searchable} label="Make text searchable" tone="convert" />
		{#if searchable}<div transition:slide={{ duration: reducedMotion ? 0 : 220, easing: cubicOut }}>
				<OcrLanguages bind:languages {models} tone="convert" {reducedMotion} {disabled} />
			</div>{/if}
	</div>
</div>
