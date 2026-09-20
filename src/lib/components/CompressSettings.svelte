<script lang="ts">
	let {
		level = $bindable<'light' | 'balanced' | 'strong'>(),
		removeMetadata = $bindable<boolean>(),
		removeThumbnails = $bindable<boolean>()
	}: {
		level: 'light' | 'balanced' | 'strong';
		removeMetadata: boolean;
		removeThumbnails: boolean;
	} = $props();
	const levels = [
		{ id: 'light', label: 'Light', hint: 'Lossless. Only repacks streams; images untouched.' },
		{
			id: 'balanced',
			label: 'Balanced',
			hint: 'Images are recompressed at high quality; selectable text stays lossless.'
		},
		{
			id: 'strong',
			label: 'Strong',
			hint: 'Smaller photos, up to 1600 pixels on the longest side. Fine detail may soften.'
		}
	] as const;
	const index = $derived(levels.findIndex((option) => option.id === level));
	const hint = $derived(levels[index]?.hint ?? '');
</script>

<div class="space-y-6">
	<div>
		<h2 class="mb-3 text-sm font-semibold">Compression</h2>
		<div
			class="relative grid grid-cols-3 gap-2 rounded-xl bg-canvas p-1"
			role="group"
			aria-label="Compression level"
		>
			<span
				aria-hidden="true"
				class="pointer-events-none absolute inset-y-1 left-1 w-[calc((100%-1rem)/3)] rounded-lg bg-panel-hover motion-safe:transition-transform motion-safe:duration-200 motion-safe:ease-[cubic-bezier(0.22,1,0.36,1)] motion-safe:style:translate-x-[calc({index}*(100%+0.5rem))]"
			></span>
			{#each levels as option (option.id)}<button
					aria-pressed={level === option.id}
					class="relative z-10 rounded-lg px-2 py-3 text-xs font-semibold motion-safe:transition-colors {level ===
					option.id
						? 'text-compress'
						: 'text-muted hover:text-white'}"
					onclick={() => (level = option.id)}>{option.label}</button
				>{/each}
		</div>
		<p class="mt-3 text-xs leading-relaxed text-muted">{hint}</p>
	</div>
	<div class="space-y-3">
		<label class="flex items-start gap-3 text-sm leading-relaxed text-muted"
			><input
				type="checkbox"
				class="mt-1 size-4 accent-compress"
				bind:checked={removeMetadata}
			/>Remove metadata and document properties</label
		>
		<label class="flex items-start gap-3 text-sm leading-relaxed text-muted"
			><input
				type="checkbox"
				class="mt-1 size-4 accent-compress"
				bind:checked={removeThumbnails}
			/>Remove embedded page thumbnails</label
		>
	</div>
</div>
