<script lang="ts">
	let {
		formsOnly = $bindable<boolean>(),
		found,
		disabled = false
	}: {
		formsOnly: boolean;
		/// How many items the chosen scope flattens, while it is counted, or
		/// unknown when the preview could not read the annotations.
		found: number | 'checking' | 'unknown';
		disabled?: boolean;
	} = $props();

	const scopes = [
		{ formsOnly: false, label: 'Everything' },
		{ formsOnly: true, label: 'Form fields' }
	] as const;
</script>

<div>
	<div class="mb-3 flex items-center justify-between">
		<h2 class="text-sm font-semibold">Flatten</h2>
		<span class="text-xs text-muted tabular-nums" role="status"
			>{found === 'checking'
				? 'Checking...'
				: found === 'unknown'
					? ''
					: found === 0
						? 'Nothing found'
						: `${found} ${found === 1 ? 'item' : 'items'}`}</span
		>
	</div>
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
