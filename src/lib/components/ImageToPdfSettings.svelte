<script lang="ts">
	let {
		pageSize = $bindable<'a4' | 'letter'>(),
		margin = $bindable<number>()
	}: {
		pageSize: 'a4' | 'letter';
		margin: number;
	} = $props();

	const pageSizes = [
		{ id: 'a4', label: 'A4' },
		{ id: 'letter', label: 'US Letter' }
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
				{pageSize === 'a4' ? '210 × 297 mm' : '8.5 × 11 in'}
			</span>
		</div>
		<div
			class="grid grid-cols-2 gap-1 rounded-xl bg-canvas p-1"
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
