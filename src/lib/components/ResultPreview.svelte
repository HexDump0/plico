<script lang="ts">
	import { IconFile, IconFileZip, IconX } from '@tabler/icons-svelte-runes';
	import { formatSize } from '$lib/workspace.svelte';
	import PdfPreview from './PdfPreview.svelte';

	let {
		file,
		format,
		name,
		size,
		onclose
	}: {
		file: File | null;
		format: string;
		name: string;
		size: number;
		onclose: () => void;
	} = $props();

	const isPdf = $derived(format === 'pdf');
</script>

<section
	aria-label="Result preview"
	class="mx-auto flex w-full max-w-7xl flex-1 flex-col gap-6 py-6 lg:py-10"
>
	<div class="flex flex-wrap items-center justify-between gap-3">
		<div class="flex min-w-0 items-center gap-3">
			<span class="shrink-0 text-[11px] font-bold tracking-[0.14em] text-muted uppercase"
				>Preview</span
			>
			<span class="h-4 w-px bg-white/15" aria-hidden="true"></span>
			<p class="max-w-sm min-w-0 truncate text-sm font-medium text-muted" title={name}>{name}</p>
			<span class="shrink-0 text-xs whitespace-nowrap text-subtle">{formatSize(size)}</span>
		</div>
		<button
			type="button"
			onclick={onclose}
			class="flex size-9 shrink-0 items-center justify-center rounded-xl border-2 border-white/10 bg-panel text-muted transition-colors hover:border-white/25 hover:text-white"
			aria-label="Back to editing"
			title="Back to editing"><IconX size={18} stroke={2} /></button
		>
	</div>

	{#if isPdf && file}
		<div class="mx-auto flex w-full max-w-md flex-col items-center gap-4">
			<div
				class="w-full overflow-hidden rounded-xl border border-white/10 bg-panel/60 shadow-lg shadow-black/30"
			>
				<PdfPreview {file} width={640} />
			</div>
		</div>
	{:else}
		<div class="grid flex-1 place-items-center py-16">
			<div class="flex flex-col items-center gap-4 text-center">
				{#if format === 'zip'}<IconFileZip size={44} stroke={1.2} class="text-subtle" />
				{:else}<IconFile size={44} stroke={1.2} class="text-subtle" />{/if}
				<p class="max-w-xs text-sm leading-relaxed text-muted">
					{format === 'zip'
						? 'Your files are packaged into one archive.'
						: `Your result is ready as ${format.toUpperCase()}.`}
				</p>
			</div>
		</div>
	{/if}

	<!-- <p class="text-center text-xs text-muted">
		This is just a preview!
	</p> -->
</section>
