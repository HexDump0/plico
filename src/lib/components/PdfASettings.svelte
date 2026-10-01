<script lang="ts">
	let {
		part = $bindable<2 | 3>(2),
		accent,
		processing
	}: { part?: 2 | 3; accent: string; processing: boolean } = $props();

	const parts = [
		{ id: 2, label: 'PDF/A-2b' },
		{ id: 3, label: 'PDF/A-3b' }
	] as const;
</script>

<div>
	<h2 class="mb-3 text-sm font-semibold">Standard</h2>
	<div
		class="relative grid grid-cols-2 gap-2 rounded-xl bg-canvas p-1"
		role="group"
		aria-label="Standard"
	>
		<span
			aria-hidden="true"
			class="pointer-events-none absolute inset-y-1 left-1 w-[calc((100%-1rem)/2)] rounded-lg bg-panel-hover motion-safe:transition-transform motion-safe:duration-200 motion-safe:ease-[cubic-bezier(0.22,1,0.36,1)] {part ===
			3
				? 'translate-x-[calc(100%+0.5rem)]'
				: ''}"
		></span>
		{#each parts as option (option.id)}<button
				type="button"
				aria-pressed={part === option.id}
				disabled={processing}
				class="relative z-10 rounded-lg px-2 py-3 text-xs font-semibold motion-safe:transition-colors {part ===
				option.id
					? accent
					: 'text-muted hover:text-white'}"
				onclick={() => (part = option.id)}>{option.label}</button
			>{/each}
	</div>
</div>
