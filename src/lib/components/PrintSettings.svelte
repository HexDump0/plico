<script lang="ts">
	import type { PrintSize } from '$lib/pdf/html-print';
	import type { Typeface } from '$lib/pdf/markdown-print';

	let {
		size = $bindable<PrintSize>(),
		landscape = $bindable<boolean>(),
		margin = $bindable<number>(),
		typeface = $bindable<Typeface>(),
		markdown
	}: {
		size: PrintSize;
		landscape: boolean;
		margin: number;
		typeface: Typeface;
		/// Markdown has no look of its own, so it chooses a typeface.
		markdown: boolean;
	} = $props();

	const sizes = [
		{ id: 'a4', label: 'A4' },
		{ id: 'letter', label: 'US Letter' },
		{ id: 'one', label: 'One page' }
	] as const;

	const orientations = [
		{ id: false, label: 'Portrait' },
		{ id: true, label: 'Landscape' }
	] as const;

	const margins = [
		{ value: 0, label: 'None' },
		{ value: 36, label: 'Small' },
		{ value: 72, label: 'Big' }
	] as const;

	const typefaces = [
		{ id: 'sans', label: 'Sans', font: 'font-sans' },
		{ id: 'serif', label: 'Serif', font: 'font-serif' }
	] as const;

	const pill = (active: boolean) =>
		`rounded-lg px-2 py-3 text-xs font-semibold motion-safe:transition-colors ${
			active ? 'bg-panel-hover text-convert' : 'text-muted hover:text-white'
		}`;
</script>

<div class="space-y-6">
	{#if markdown}
		<div>
			<h2 class="mb-3 text-sm font-semibold">Typeface</h2>
			<div
				class="grid grid-cols-2 gap-1 rounded-xl bg-canvas p-1"
				role="group"
				aria-label="Typeface"
			>
				{#each typefaces as option (option.id)}
					<button
						type="button"
						aria-pressed={typeface === option.id}
						class="{pill(typeface === option.id)} {option.font}"
						onclick={() => (typeface = option.id)}>{option.label}</button
					>
				{/each}
			</div>
		</div>
	{/if}

	<div>
		<div class="mb-3 flex items-center justify-between">
			<h2 class="text-sm font-semibold">Page size</h2>
			<span class="text-xs text-muted">
				{size === 'a4' ? '210 × 297 mm' : size === 'letter' ? '8.5 × 11 in' : 'No page breaks'}
			</span>
		</div>
		<div
			class="grid grid-cols-3 gap-1 rounded-xl bg-canvas p-1"
			role="group"
			aria-label="Page size"
		>
			{#each sizes as option (option.id)}
				<button
					type="button"
					aria-pressed={size === option.id}
					class={pill(size === option.id)}
					onclick={() => (size = option.id)}>{option.label}</button
				>
			{/each}
		</div>
	</div>

	<div>
		<h2 class="mb-3 text-sm font-semibold">Orientation</h2>
		<div
			class="grid grid-cols-2 gap-1 rounded-xl bg-canvas p-1"
			role="group"
			aria-label="Orientation"
		>
			{#each orientations as option (option.label)}
				<button
					type="button"
					aria-pressed={landscape === option.id}
					class={pill(landscape === option.id)}
					onclick={() => (landscape = option.id)}>{option.label}</button
				>
			{/each}
		</div>
	</div>

	<div>
		<div class="mb-3 flex items-center justify-between">
			<h2 class="text-sm font-semibold">Margin</h2>
			<span class="text-xs text-muted">
				{margin === 0 ? 'No margin' : margin === 36 ? '0.5 in' : '1 in'}
			</span>
		</div>
		<div class="grid grid-cols-3 gap-1 rounded-xl bg-canvas p-1" role="group" aria-label="Margin">
			{#each margins as option (option.value)}
				<button
					type="button"
					aria-pressed={margin === option.value}
					class={pill(margin === option.value)}
					onclick={() => (margin = option.value)}>{option.label}</button
				>
			{/each}
		</div>
	</div>
</div>
