<script lang="ts">
	let {
		level = $bindable<'light' | 'balanced' | 'strong'>()
	}: {
		level: 'light' | 'balanced' | 'strong';
	} = $props();
	const levels = [
		{ id: 'light', label: 'Light' },
		{
			id: 'balanced',
			label: 'Balanced'
		},
		{
			id: 'strong',
			label: 'High'
		}
	] as const;
</script>

<div class="space-y-6">
	<div>
		<h2 class="mb-3 text-sm font-semibold">Compression</h2>
		<div
			class="grid grid-cols-3 gap-1 rounded-xl bg-canvas p-1"
			role="group"
			aria-label="Compression level"
		>
			{#each levels as option (option.id)}<button
					type="button"
					aria-pressed={level === option.id}
					class="rounded-lg px-2 py-3 text-xs font-semibold motion-safe:transition-colors {level ===
					option.id
						? 'bg-panel-hover text-compress'
						: 'text-muted hover:text-white'}"
					onclick={() => (level = option.id)}>{option.label}</button
				>{/each}
		</div>
	</div>
</div>
