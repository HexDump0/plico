<script lang="ts">
	import { onDestroy } from 'svelte';
	import { IconCheck, IconCopy, IconX } from '@tabler/icons-svelte-runes';

	let { text, label }: { text: string; label: string } = $props();
	let copied = $state<'idle' | 'done' | 'failed'>('idle');
	let timer: ReturnType<typeof setTimeout> | undefined;

	async function copy() {
		clearTimeout(timer);
		try {
			await navigator.clipboard.writeText(text);
			copied = 'done';
		} catch {
			copied = 'failed';
		}
		timer = setTimeout(() => (copied = 'idle'), 1600);
	}

	onDestroy(() => clearTimeout(timer));
</script>

<button
	type="button"
	onclick={copy}
	aria-label={label}
	title={copied === 'done' ? 'Copied' : copied === 'failed' ? "Couldn't copy" : label}
	class="grid size-14 shrink-0 place-items-center rounded-xl border-2 border-white/10 transition-colors hover:border-white/20 hover:text-split {copied ===
	'failed'
		? 'text-convert'
		: 'text-white'}"
>
	{#each [{ state: 'idle', Icon: IconCopy }, { state: 'done', Icon: IconCheck }, { state: 'failed', Icon: IconX }] as { state, Icon } (state)}
		<Icon
			size={20}
			aria-hidden="true"
			class="col-start-1 row-start-1 motion-safe:transition-[opacity,transform] motion-safe:duration-200 {state ===
			'done'
				? 'text-split'
				: ''} {copied === state ? 'scale-100 opacity-100' : 'scale-50 opacity-0'}"
		/>
	{/each}
	<span class="sr-only" aria-live="polite"
		>{copied === 'done' ? 'Copied' : copied === 'failed' ? "Couldn't copy" : ''}</span
	>
</button>
