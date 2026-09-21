<script lang="ts">
	import { fade } from 'svelte/transition';
	import { IconTrash } from '@tabler/icons-svelte-runes';

	let {
		hovered,
		reducedMotion,
		zone = $bindable()
	}: {
		hovered: boolean;
		reducedMotion: boolean;
		zone: HTMLDivElement | undefined;
	} = $props();
</script>

<div
	bind:this={zone}
	transition:fade={{ duration: reducedMotion ? 0 : 160 }}
	aria-hidden="true"
	class="pointer-events-none fixed top-32 bottom-12 left-0 z-30 flex w-24 flex-col items-center justify-center gap-3 rounded-r-2xl border-2 border-l-0 border-dashed text-center text-xs font-semibold transition-colors sm:w-28 {hovered
		? 'border-red-400 bg-red-500/25 text-red-100'
		: 'border-red-500/50 bg-red-500/[0.08] text-red-300'}"
>
	<IconTrash size={30} stroke={1.8} />
</div>
