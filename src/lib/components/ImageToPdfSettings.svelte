<script lang="ts">
	import { cubicOut } from 'svelte/easing';
	import { slide } from 'svelte/transition';

	let {
		pageSize = $bindable<'a4' | 'letter' | 'fit'>(),
		orientation = $bindable<'auto' | 'portrait' | 'landscape'>(),
		margin = $bindable<number>(),
		reducedMotion
	}: {
		pageSize: 'a4' | 'letter' | 'fit';
		orientation: 'auto' | 'portrait' | 'landscape';
		margin: number;
		reducedMotion: boolean;
	} = $props();

	const pageSizes = [
		{ id: 'a4', label: 'A4' },
		{ id: 'letter', label: 'US Letter' },
		{ id: 'fit', label: 'Fit image' }
	] as const;

	const orientations = [
		{ id: 'auto', label: 'Auto' },
		{ id: 'portrait', label: 'Portrait' },
		{ id: 'landscape', label: 'Landscape' }
	] as const;

	const margins = [
		{ value: 0, label: 'None' },
		{ value: 18, label: 'Small' },
		{ value: 36, label: 'Big' }
	] as const;
</script>

<div class="space-y-6">
	<div>
		<div class="mb-3 flex items-center justify-between">
			<h2 class="text-sm font-semibold">Page size</h2>
			<span class="text-xs text-muted">
				{pageSize === 'a4' ? '210 × 297 mm' : pageSize === 'letter' ? '8.5 × 11 in' : 'Per image'}
			</span>
		</div>
		<div
			class="grid grid-cols-3 gap-1 rounded-xl bg-canvas p-1"
			role="group"
			aria-label="Page size"
		>
			{#each pageSizes as option (option.id)}
				<button
					type="button"
					aria-pressed={pageSize === option.id}
					class="rounded-lg px-2 py-3 text-xs font-semibold motion-safe:transition-colors {pageSize ===
					option.id
						? 'bg-panel-hover text-convert'
						: 'text-muted hover:text-white'}"
					onclick={() => (pageSize = option.id)}>{option.label}</button
				>
			{/each}
		</div>
	</div>

	{#if pageSize !== 'fit'}
		<div transition:slide={{ duration: reducedMotion ? 0 : 220, easing: cubicOut }}>
			<h2 class="mb-3 text-sm font-semibold">Orientation</h2>
			<div
				class="grid grid-cols-3 gap-1 rounded-xl bg-canvas p-1"
				role="group"
				aria-label="Orientation"
			>
				{#each orientations as option (option.id)}
					<button
						type="button"
						aria-pressed={orientation === option.id}
						class="rounded-lg px-2 py-3 text-xs font-semibold motion-safe:transition-colors {orientation ===
						option.id
							? 'bg-panel-hover text-convert'
							: 'text-muted hover:text-white'}"
						onclick={() => (orientation = option.id)}>{option.label}</button
					>
				{/each}
			</div>
		</div>
	{/if}

	<div>
		<div class="mb-3 flex items-center justify-between">
			<h2 class="text-sm font-semibold">Margin</h2>
			<span class="text-xs text-muted">
				{margin === 0 ? 'No margin' : margin === 18 ? '0.25 in' : '0.5 in'}
			</span>
		</div>
		<div class="grid grid-cols-3 gap-1 rounded-xl bg-canvas p-1" role="group" aria-label="Margin">
			{#each margins as option (option.value)}
				<button
					type="button"
					aria-pressed={margin === option.value}
					class="rounded-lg px-2 py-3 text-xs font-semibold motion-safe:transition-colors {margin ===
					option.value
						? 'bg-panel-hover text-convert'
						: 'text-muted hover:text-white'}"
					onclick={() => (margin = option.value)}>{option.label}</button
				>
			{/each}
		</div>
	</div>
</div>
