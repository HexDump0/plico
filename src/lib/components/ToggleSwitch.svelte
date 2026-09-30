<script lang="ts">
	let {
		checked = $bindable<boolean>(),
		label,
		tone
	}: {
		checked: boolean;
		label: string;
		tone: 'compress' | 'split' | 'merge' | 'brand';
	} = $props();
	// Whole class names so Tailwind's scanner sees them.
	const tones = {
		compress: 'peer-checked:bg-compress peer-focus-visible:outline-compress',
		split: 'peer-checked:bg-split peer-focus-visible:outline-split',
		merge: 'peer-checked:bg-merge peer-focus-visible:outline-merge',
		brand: 'peer-checked:bg-brand peer-focus-visible:outline-brand'
	};
</script>

<label
	class="flex cursor-pointer items-center justify-between gap-4 rounded-xl bg-canvas/50 px-3.5 py-3 text-sm"
>
	<span>{label}</span>
	<input type="checkbox" role="switch" class="peer sr-only" bind:checked />
	<span
		aria-hidden="true"
		class="relative h-6 w-10 shrink-0 rounded-full bg-white/15 peer-focus-visible:outline-2 peer-focus-visible:outline-offset-2 motion-safe:transition-colors {tones[
			tone
		]}"
	>
		<span
			class="absolute top-1 left-1 size-4 rounded-full bg-white motion-safe:transition-transform {checked
				? 'translate-x-4'
				: ''}"
		></span>
	</span>
</label>
