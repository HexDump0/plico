<script lang="ts">
	let {
		formsOnly = $bindable<boolean>(),
		disabled = false
	}: {
		formsOnly: boolean;
		disabled?: boolean;
	} = $props();

	const scopes = [
		{ formsOnly: false, label: 'Everything' },
		{ formsOnly: true, label: 'Form fields' }
	] as const;
</script>

<div>
	<h2 class="mb-3 text-sm font-semibold">Flatten</h2>
	<div
		class="relative grid grid-cols-2 gap-2 rounded-xl bg-canvas p-1"
		role="group"
		aria-label="Flatten"
	>
		<span
			aria-hidden="true"
			class="pointer-events-none absolute inset-y-1 left-1 w-[calc((100%-1rem)/2)] rounded-lg bg-panel-hover motion-safe:transition-transform motion-safe:duration-200 motion-safe:ease-[cubic-bezier(0.22,1,0.36,1)] {formsOnly
				? 'translate-x-[calc(100%+0.5rem)]'
				: ''}"
		></span>
		{#each scopes as option (option.label)}<button
				type="button"
				aria-pressed={formsOnly === option.formsOnly}
				{disabled}
				class="relative z-10 rounded-lg px-2 py-3 text-xs font-semibold motion-safe:transition-colors {formsOnly ===
				option.formsOnly
					? 'text-compress'
					: 'text-muted hover:text-white'}"
				onclick={() => (formsOnly = option.formsOnly)}>{option.label}</button
			>{/each}
	</div>
</div>
