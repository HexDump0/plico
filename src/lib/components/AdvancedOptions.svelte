<script lang="ts">
	import type { Snippet } from 'svelte';
	import { slide } from 'svelte/transition';
	import { IconChevronDown } from '@tabler/icons-svelte-runes';

	let {
		reducedMotion,
		spacing = 'space-y-2',
		children
	}: { reducedMotion: boolean; spacing?: string; children: Snippet } = $props();
	const id = $props.id();
	let open = $state(false);
</script>

<div>
	<button
		type="button"
		aria-expanded={open}
		aria-controls={id}
		class="flex w-full items-center justify-between text-sm font-medium text-muted hover:text-white"
		onclick={() => (open = !open)}
	>
		Advanced options
		<IconChevronDown
			size={16}
			stroke={1.75}
			aria-hidden="true"
			class="motion-safe:transition-transform {open ? 'rotate-180' : ''}"
		/>
	</button>
	{#if open}
		<div {id} class="pt-4 {spacing}" transition:slide={{ duration: reducedMotion ? 0 : 300 }}>
			{@render children()}
		</div>
	{/if}
</div>
