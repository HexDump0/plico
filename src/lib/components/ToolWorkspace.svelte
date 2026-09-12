<script lang="ts">
	import { onDestroy, onMount, untrack } from 'svelte';
	import { flip } from 'svelte/animate';
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
	let dropTarget = $state(-1);
	let reducedMotion = $state(false);
	onMount(() => {
		const preference = window.matchMedia('(prefers-reduced-motion: reduce)');
		const update = () => {
			reducedMotion = preference.matches;
		};
		update();
		preference.addEventListener('change', update);
		return () => preference.removeEventListener('change', update);
	});
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

<main id="main-content" class="flex flex-1 flex-col">
	<div
		class="grid flex-1 grid-cols-[minmax(0,1fr)] lg:grid-cols-[minmax(0,1fr)_22rem] xl:grid-cols-[minmax(0,1fr)_24rem]"
	>
		<section
			aria-label="Documents"
			class="relative isolate flex min-w-0 flex-col overflow-hidden px-6 pt-6 pb-12 sm:px-10 lg:px-16 lg:pt-10 lg:pb-20"
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
			<img
				src="/hero-contours.svg"
				alt=""
				aria-hidden="true"
				class="pointer-events-none absolute -right-48 -bottom-48 -z-10 w-240 max-w-none opacity-[0.07] select-none"
			/>
			<div class="flex flex-wrap items-center justify-end gap-4">
				{#if isMerge && workspace.files.length > 1}
					<button
						disabled={processing}
						class="flex min-h-11 items-center gap-2 rounded-lg px-2 text-xs text-muted transition-colors hover:text-merge disabled:opacity-40"
						onclick={() =>
							(workspace.files = [...workspace.files].sort((a, b) =>
								a.name.localeCompare(b.name, undefined, { numeric: true })
							))}><IconArrowsSort size={17} />Sort by filename</button
					>
				{:else if isSplit && workspace.files.length}
					<button
						class="flex min-h-11 items-center gap-2 rounded-lg px-2 text-xs text-muted hover:text-split"
						onclick={() => input?.click()}><IconPlus size={17} />Add PDFs</button
					>
				{/if}
			</div>
			{#if workspace.files.length === 0}
				<div class="mx-auto flex w-full max-w-xl flex-1 flex-col justify-center py-12 lg:py-20">
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
				{#if isSplit && currentFile}
					<label class="mt-10 mb-6 block text-xs text-muted"
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
					<ol
						aria-label="PDF order"
						class="my-auto flex flex-wrap items-center justify-center gap-5 py-16 sm:gap-7 lg:py-24"
					>
						{#each workspace.files as file, index (file)}
							<li
								animate:flip={{ duration: reducedMotion ? 0 : 220 }}
								draggable={isMerge && !processing}
								class="group relative w-[calc((100%-1.25rem)/2)] min-w-0 rounded-2xl border bg-panel p-3 shadow-xl shadow-black/20 transition-[border-color,box-shadow] sm:w-52 {dragged ===
								index
									? 'border-merge opacity-40'
									: dropTarget === index
										? 'border-merge shadow-merge/10'
										: 'border-white/10 hover:border-white/25'}"
								ondragstart={(event) => {
									dragged = index;
									event.dataTransfer?.setData('text/plain', String(index));
								}}
								ondragend={() => {
									dragged = -1;
									dropTarget = -1;
								}}
								ondragover={(event) => {
									if (dragged >= 0) {
										event.preventDefault();
										dropTarget = index;
									}
								}}
								ondrop={(event) => {
									event.preventDefault();
									if (!processing && dragged >= 0) workspace.move(dragged, index);
									dragged = -1;
									dropTarget = -1;
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
								<div
									class="motion-safe:transition-transform motion-safe:duration-200 motion-safe:group-hover:-translate-y-1"
								>
									<PdfPreview {file} />
								</div>
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
						<li
							class="flex w-[calc((100%-1.25rem)/2)] items-center justify-center self-stretch sm:w-52"
						>
							<button
								disabled={processing}
								onclick={() => input?.click()}
								class="group flex min-h-52 w-full flex-col items-center justify-center gap-4 rounded-2xl border border-dashed border-merge/25 bg-canvas/40 text-merge/80 transition-colors hover:border-merge/60 hover:bg-merge/5 hover:text-merge disabled:opacity-40 sm:min-h-64"
							>
								<span
									class="flex size-12 items-center justify-center rounded-full bg-merge/10 motion-safe:transition-transform motion-safe:group-hover:scale-110"
									><IconPlus size={24} stroke={1.5} /></span
								><span class="text-sm font-medium">Add PDFs</span>
							</button>
						</li>
					</ol>
				{/if}
				{#if workspace.error}<p role="alert" class="mt-4 text-sm text-convert">
						{workspace.error}
					</p>{/if}
			{/if}
		</section>
		<aside
			aria-label={`${tool.label} settings`}
			class="m-6 flex flex-col rounded-2xl bg-panel lg:sticky lg:top-6 lg:ml-0 lg:h-[calc(100svh-9rem)] lg:self-start lg:overflow-y-auto"
		>
			<div class="flex-1 space-y-7 p-6 sm:p-8">
				<h1 class="flex items-center gap-3 text-xl font-semibold tracking-tight">
					<tool.icon size={24} stroke={1.7} class={`shrink-0 ${accent}`} />{tool.label}
				</h1>
				{#if isMerge}
					<label class="block text-sm font-medium"
						>Output filename
						<div
							class="mt-3 flex items-center rounded-xl border border-white/10 bg-canvas px-3 focus-within:border-white/25"
						>
							<input
								bind:value={filename}
								class="min-w-0 flex-1 bg-transparent py-3 text-sm outline-none"
								aria-label="Output filename"
								placeholder="plico-merged"
							/><span class="text-xs text-muted">.pdf</span>
						</div></label
					>
				{:else if isSplit}
					{#key currentFile}<SplitSettings {pageCount} />{/key}
				{/if}
			</div>
			<div class="shrink-0 space-y-4 p-6 sm:p-8" aria-live="polite">
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
				{:else}
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
