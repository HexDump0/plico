<script lang="ts">
	import { onDestroy } from 'svelte';
	import { IconCheck, IconCopy, IconX } from '@tabler/icons-svelte-runes';

	// `display` turns the square icon button into a pill that shows that text beside a small icon.
	let { text, label, display }: { text: string; label: string; display?: string } = $props();
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
	class={display
		? 'group flex min-w-0 items-center gap-2.5 rounded-lg bg-panel-hover py-2 pr-2.5 pl-3 text-sm transition-colors hover:bg-white/10'
		: `grid size-14 shrink-0 place-items-center rounded-xl border-2 border-white/10 transition-colors hover:border-white/20 hover:text-split ${copied === 'failed' ? 'text-convert' : 'text-white'}`}
>
	{#if display}<code class="min-w-0 truncate text-tool-label">{display}</code>{/if}
	<span
		class={display
			? `grid shrink-0 place-items-center transition-colors ${copied === 'failed' ? 'text-convert' : 'text-muted group-hover:text-white'}`
			: 'contents'}
	>
		{#each [{ state: 'idle', Icon: IconCopy }, { state: 'done', Icon: IconCheck }, { state: 'failed', Icon: IconX }] as { state, Icon } (state)}
			<Icon
				size={display ? 16 : 20}
				aria-hidden="true"
				class="col-start-1 row-start-1 motion-safe:transition-[opacity,transform] motion-safe:duration-200 {state ===
				'done'
					? 'text-split'
					: ''} {copied === state ? 'scale-100 opacity-100' : 'scale-50 opacity-0'}"
			/>
		{/each}
	</span>
	<span class="sr-only" aria-live="polite"
		>{copied === 'done' ? 'Copied' : copied === 'failed' ? "Couldn't copy" : ''}</span
	>
</button>
