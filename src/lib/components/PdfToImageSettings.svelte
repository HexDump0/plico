<script lang="ts">
	let {
		format = $bindable<'jpg' | 'png'>(),
		dpi = $bindable<number>()
	}: {
		format: 'jpg' | 'png';
		dpi: number;
	} = $props();

	const formats = [
		{ id: 'jpg', label: 'JPG' },
		{ id: 'png', label: 'PNG' }
	] as const;

	const resolutions = [
		{ dpi: 72, label: '72 DPI' },
		{ dpi: 150, label: '150 DPI' },
		{ dpi: 300, label: '300 DPI' }
	] as const;
</script>

<div class="space-y-6">
	<div>
		<h2 class="mb-3 text-sm font-semibold">Image format</h2>
		<div
			class="grid grid-cols-2 gap-1 rounded-xl bg-canvas p-1"
			role="group"
			aria-label="Image format"
		>
			{#each formats as option (option.id)}
				<button
					type="button"
					aria-pressed={format === option.id}
					class="rounded-lg px-2 py-3 text-xs font-semibold motion-safe:transition-colors {format ===
					option.id
						? 'bg-panel-hover text-split'
						: 'text-muted hover:text-white'}"
					onclick={() => (format = option.id)}>{option.label}</button
				>
			{/each}
		</div>
	</div>

	<div>
		<div class="mb-3 flex items-center justify-between">
			<h2 class="text-sm font-semibold">Resolution</h2>
			<span class="text-xs text-muted"
				>{dpi === 300 ? 'Print' : dpi === 150 ? 'Standard' : 'Screen'}</span
			>
		</div>
		<div
			class="grid grid-cols-3 gap-1 rounded-xl bg-canvas p-1"
			role="group"
			aria-label="Resolution"
		>
			{#each resolutions as option (option.dpi)}
				<button
					type="button"
					aria-pressed={dpi === option.dpi}
					class="rounded-lg px-2 py-3 text-xs font-semibold motion-safe:transition-colors {dpi ===
					option.dpi
						? 'bg-panel-hover text-split'
						: 'text-muted hover:text-white'}"
					onclick={() => (dpi = option.dpi)}>{option.label}</button
				>
			{/each}
		</div>
	</div>
</div>
