<script lang="ts">
	import { onDestroy, untrack } from 'svelte';
	import { resolve } from '$app/paths';
	import {
		IconArrowLeft,
		IconArrowRight,
		IconPlus,
		IconX,
		IconArrowsSort,
		IconDownload,
		IconCheck,
		IconLoader2
	} from '@tabler/icons-svelte-runes';
	import type { CatalogTool } from '$lib/tool-catalog';
	import { getWorkspace, formatSize } from '$lib/workspace.svelte';
	import { processPdfs } from '$lib/pdf/processor';
	import PdfDropzone from './PdfDropzone.svelte';
	import PdfPreview from './PdfPreview.svelte';
	import SplitSettings from './SplitSettings.svelte';
	let { tool }: { tool: CatalogTool } = $props();
	const workspace = getWorkspace();
	const isMerge = $derived(tool.id === 'merge');
	const isSplit = $derived(tool.id === 'split');
	const accent = $derived(isMerge ? 'text-merge' : isSplit ? 'text-split' : 'text-brand');
	let input = $state<HTMLInputElement>();
	let dragged = $state(-1);
	let activeFile = $state<File | null>(null);
	let pageNumber = $state(1);
	let pageCount = $state(0);
	let filename = $state('plico-merged');
	let processing = $state(false);
	let error = $state('');
	let result = $state('');
	let resultSize = $state(0);
	let controller: AbortController | undefined;
	const currentFile = $derived(
		activeFile && workspace.files.includes(activeFile) ? activeFile : workspace.files[0]
	);
	const totalSize = $derived(workspace.files.reduce((sum, file) => sum + file.size, 0));
	$effect(() => {
		void workspace.files;
		untrack(clearResult);
	});
	$effect(() => {
		void currentFile;
		pageNumber = 1;
		pageCount = 0;
	});
	function clearResult() {
		controller?.abort();
		controller = undefined;
		processing = false;
		error = '';
		if (result) URL.revokeObjectURL(result);
		result = '';
	}
	onDestroy(clearResult);
	async function merge() {
		if (processing || workspace.files.length < 2) return;
		clearResult();
		const job = new AbortController();
		controller = job;
		processing = true;
		try {
			const bytes = await processPdfs('merge', [...workspace.files], job.signal);
			if (job.signal.aborted) return;
			resultSize = bytes.byteLength;
			result = URL.createObjectURL(new Blob([bytes.slice().buffer], { type: 'application/pdf' }));
		} catch (cause) {
			if (!job.signal.aborted)
				error = cause instanceof Error ? cause.message : 'Could not merge these PDFs.';
		} finally {
			if (controller === job) {
				processing = false;
				controller = undefined;
			}
		}
	}
	function add(files: FileList) {
		workspace.add(files);
		if (input) input.value = '';
	}
</script>

<main id="main-content" class="flex flex-1 flex-col border-t border-white/10">
	<div
		class="flex flex-wrap items-center justify-between gap-4 border-b border-white/10 px-6 py-5 sm:px-10 lg:px-16"
	>
		<div class="flex items-center gap-4">
			<a
				href={resolve('/')}
				aria-label="Back to home"
				class="rounded-xl border border-white/10 p-2.5 text-muted hover:bg-panel hover:text-white"
				><IconArrowLeft size={20} /></a
			>
			<div class="flex items-center gap-3">
				<span class="rounded-xl bg-panel p-3 {accent}"><tool.icon size={24} stroke={1.7} /></span>
				<div>
					<h1 class="text-xl font-semibold tracking-tight sm:text-2xl">{tool.label}</h1>
				</div>
			</div>
		</div>
	</div>
	<div class="grid flex-1 lg:grid-cols-[minmax(0,1fr)_22rem] xl:grid-cols-[minmax(0,1fr)_24rem]">
		<section
			aria-label="Documents"
			class="min-w-0 bg-panel/20 p-6 sm:p-10 lg:p-12"
			ondragover={(event) => {
				if (dragged < 0) event.preventDefault();
			}}
			ondrop={(event) => {
				if (dragged < 0 && event.dataTransfer?.files.length) {
					event.preventDefault();
					if (!processing) workspace.add(event.dataTransfer.files);
				}
			}}
		>
			{#if workspace.files.length === 0}
				<div class="mx-auto flex h-full max-w-xl flex-col justify-center py-8 lg:py-16">
					<div class="h-80"><PdfDropzone selectedTool={tool} /></div>
				</div>
			{:else}
				<input
					bind:this={input}
					type="file"
					accept="application/pdf,.pdf"
					multiple
					class="hidden"
					aria-label="Add PDF files"
					onchange={() => input?.files && add(input.files)}
				/>
				<div class="mb-7 flex flex-wrap items-center justify-between gap-3">
					<div>
						<h2 class="text-sm font-semibold">
							Documents <span class="ml-2 rounded-md bg-white/5 px-2 py-1 text-xs text-muted"
								>{workspace.files.length}</span
							>
						</h2>
						{#if isMerge}<p class="mt-2 text-xs text-muted">
								Drag to reorder, or use the arrows.
							</p>{/if}
					</div>
					<button
						disabled={processing}
						class="flex items-center gap-2 rounded-xl border border-white/10 bg-panel px-4 py-3 text-xs font-semibold hover:border-brand/50 disabled:opacity-40"
						onclick={() => input?.click()}><IconPlus size={17} />Add PDFs</button
					>
				</div>
				{#if isSplit && currentFile}
					<label class="mb-6 block text-xs text-muted"
						>Document<select
							class="mt-2 block w-full max-w-md truncate rounded-xl border border-white/10 bg-panel p-3 text-sm text-white"
							value={workspace.files.indexOf(currentFile)}
							onchange={(event) =>
								(activeFile = workspace.files[Number(event.currentTarget.value)])}
							>{#each workspace.files as file, index (file)}<option value={index}
									>{file.name}</option
								>{/each}</select
						></label
					>
					<div class="mx-auto max-w-52 sm:max-w-72">
						{#key currentFile}{#key pageNumber}<PdfPreview
									file={currentFile}
									{pageNumber}
									onload={(count) => (pageCount = count)}
								/>{/key}{/key}
						<div class="mt-4 flex items-center justify-between">
							<button
								aria-label="Previous page"
								disabled={pageNumber <= 1}
								class="rounded-lg border border-white/10 p-2 disabled:opacity-30"
								onclick={() => pageNumber--}><IconArrowLeft size={18} /></button
							><span class="text-xs text-muted"
								>Page {pageNumber} {pageCount ? `of ${pageCount}` : ''}</span
							><button
								aria-label="Next page"
								disabled={pageNumber >= pageCount}
								class="rounded-lg border border-white/10 p-2 disabled:opacity-30"
								onclick={() => pageNumber++}><IconArrowRight size={18} /></button
							>
						</div>
						<button
							class="mx-auto mt-5 block rounded-lg p-2 text-xs text-muted hover:text-convert"
							onclick={() => workspace.remove(currentFile)}>Remove document</button
						>
					</div>
				{:else}
					<ol class="grid grid-cols-2 gap-4 sm:grid-cols-[repeat(auto-fill,13rem)]">
						{#each workspace.files as file, index (file)}
							<li
								draggable={isMerge && !processing}
								class="min-w-0 rounded-2xl border bg-panel p-3 {dragged === index
									? 'border-brand opacity-50'
									: 'border-white/10'}"
								ondragstart={(event) => {
									dragged = index;
									event.dataTransfer?.setData('text/plain', String(index));
								}}
								ondragend={() => (dragged = -1)}
								ondragover={(event) => {
									if (dragged >= 0) event.preventDefault();
								}}
								ondrop={(event) => {
									event.preventDefault();
									if (!processing && dragged >= 0) workspace.move(dragged, index);
									dragged = -1;
								}}
							>
								<div class="mb-2 flex items-center justify-between">
									<span
										class="flex size-6 items-center justify-center rounded-md bg-white/5 text-xs {accent}"
										>{index + 1}</span
									><button
										disabled={processing}
										class="rounded-lg p-1.5 text-muted hover:text-convert"
										aria-label={`Remove ${file.name}`}
										onclick={() => workspace.remove(file)}><IconX size={17} /></button
									>
								</div>
								<PdfPreview {file} />
								<p class="mt-3 truncate text-xs font-medium" title={file.name}>{file.name}</p>
								<div class="mt-2 flex flex-wrap items-center justify-between gap-1">
									<span class="text-xs text-muted">{formatSize(file.size)}</span>{#if isMerge}<div
											class="flex"
										>
											<button
												disabled={index === 0 || processing}
												aria-label={`Move ${file.name} earlier`}
												class="rounded-lg p-2 text-muted hover:bg-white/5 hover:text-white disabled:opacity-20"
												onclick={() => workspace.move(index, index - 1)}
												><IconArrowLeft size={16} /></button
											><button
												disabled={index === workspace.files.length - 1 || processing}
												aria-label={`Move ${file.name} later`}
												class="rounded-lg p-2 text-muted hover:bg-white/5 hover:text-white disabled:opacity-20"
												onclick={() => workspace.move(index, index + 1)}
												><IconArrowRight size={16} /></button
											>
										</div>{/if}
								</div>
							</li>
						{/each}
					</ol>
				{/if}
				{#if workspace.error}<p role="alert" class="mt-4 text-sm text-convert">
						{workspace.error}
					</p>{/if}
			{/if}
		</section>
		<aside
			aria-label={`${tool.label} settings`}
			class="flex flex-col border-t border-white/10 bg-panel/40 lg:border-t-0 lg:border-l"
		>
			<div class="flex-1 space-y-7 p-6 sm:p-8">
				<div>
					<h2 class="text-lg font-semibold">
						{isMerge ? 'Merge settings' : isSplit ? 'Split settings' : tool.label}
					</h2>
				</div>
				{#if isMerge}
					<button
						disabled={processing || workspace.files.length < 2}
						class="flex w-full items-center justify-between rounded-xl border border-white/10 p-3 text-sm text-muted hover:bg-panel disabled:opacity-40"
						onclick={() =>
							(workspace.files = [...workspace.files].sort((a, b) =>
								a.name.localeCompare(b.name, undefined, { numeric: true })
							))}>Sort by filename<IconArrowsSort size={18} /></button
					>
					<label class="block text-sm font-medium"
						>Output filename
						<div
							class="mt-3 flex items-center rounded-xl border border-white/10 bg-canvas px-3 focus-within:border-brand"
						>
							<input
								bind:value={filename}
								class="min-w-0 flex-1 bg-transparent py-3 text-sm outline-none"
								aria-label="Output filename"
								placeholder="plico-merged"
							/><span class="text-xs text-muted">.pdf</span>
						</div></label
					>
					<div class="space-y-3 border-t border-white/10 pt-5 text-xs">
						<div class="flex justify-between text-muted">
							<span>Documents</span><span class="text-white">{workspace.files.length}</span>
						</div>
						<div class="flex justify-between text-muted">
							<span>Total input size</span><span class="text-white">{formatSize(totalSize)}</span>
						</div>
						<div class="flex justify-between text-muted">
							<span>Result</span><span class="text-merge">One PDF</span>
						</div>
					</div>
				{:else if isSplit}
					{#key currentFile}<SplitSettings {pageCount} />{/key}
				{:else}
					<p class="text-sm leading-relaxed text-muted">This tool is not available yet.</p>
				{/if}
			</div>
			<div class="space-y-4 border-t border-white/10 p-6 sm:p-8" aria-live="polite">
				{#if result}<div class="flex items-center gap-2 text-sm text-merge">
						<IconCheck size={19} />Your PDF is ready · {formatSize(resultSize)}
					</div>
					<a
						href={result}
						rel="external"
						download={`${filename.trim().replace(/\.pdf$/i, '') || 'plico-merged'}.pdf`}
						class="flex min-h-14 items-center justify-center gap-3 rounded-xl bg-brand px-4 py-4 text-sm font-bold text-canvas hover:bg-violet-300"
						><IconDownload size={20} />Download PDF</a
					>
				{:else}<p class="text-xs leading-relaxed text-muted">
						{isMerge
							? workspace.files.length < 2
								? 'Add at least two PDFs.'
								: ''
							: isSplit
								? 'Split processing is not available yet.'
								: ''}
					</p>
					<button
						disabled={!isMerge || workspace.files.length < 2 || processing}
						onclick={merge}
						class="flex min-h-14 w-full items-center justify-center gap-3 rounded-xl bg-brand px-4 py-4 text-sm font-bold text-canvas hover:bg-violet-300 disabled:cursor-not-allowed disabled:opacity-40"
						>{#if processing}<IconLoader2
								class="animate-spin"
								size={20}
							/>Merging…{:else}{tool.label}<IconArrowRight size={20} />{/if}</button
					>{/if}
				{#if processing}<button
						class="w-full rounded-lg py-2 text-sm text-muted hover:text-white"
						onclick={clearResult}>Cancel</button
					>{/if}
				{#if error}<p role="alert" class="text-sm text-convert">{error}</p>{/if}
			</div>
		</aside>
	</div>
</main>
