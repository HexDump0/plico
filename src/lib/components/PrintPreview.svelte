<script lang="ts">
	import { IconX } from '@tabler/icons-svelte-runes';
	import PageScroller from './PageScroller.svelte';

	let {
		file,
		preview,
		error,
		onload,
		onremove
	}: {
		/// The HTML or Markdown file.
		file: File;
		/// The PDF made from it, once there is one.
		preview: File | null;
		error: string;
		onload: (count: number) => void;
		onremove: () => void;
	} = $props();

	const status = $derived(error ? '' : 'Laying out pages...');
</script>

{#if preview}
	<PageScroller
		file={preview}
		name={file.name}
		removeLabel="Remove file"
		keep
		onload={(count) => onload(count)}
		{onremove}
	/>
{:else}
	<section aria-label="Preview" class="w-full space-y-6">
		<div class="mx-auto flex w-fit max-w-full items-center gap-2 px-1">
			<p class="max-w-xl min-w-0 truncate text-sm font-semibold" title={file.name}>{file.name}</p>
			<span class="shrink-0 text-xs whitespace-nowrap text-muted">{status}</span>
			<button
				type="button"
				onclick={onremove}
				class="flex size-8 shrink-0 items-center justify-center rounded-lg bg-white/10 text-white backdrop-blur-sm transition-colors hover:bg-convert hover:text-canvas"
				aria-label="Remove file"
				title="Remove file"><IconX size={18} stroke={2.5} /></button
			>
		</div>
		{#if error}
			<p role="alert" class="mx-auto max-w-md py-16 text-center text-sm text-convert">{error}</p>
		{:else}
			<p role="status" class="py-16 text-center text-sm text-muted">{status}</p>
		{/if}
	</section>
{/if}
