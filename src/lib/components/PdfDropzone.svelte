<script lang="ts">
	import { onDestroy, untrack } from 'svelte';
	import {
		IconUpload,
		IconFileTypePdf,
		IconX,
		IconPlus,
		IconDownload,
		IconLoader2
	} from '@tabler/icons-svelte-runes';
	import type { CatalogTool } from '$lib/tool-catalog';
	import { processPdfs } from '$lib/pdf/processor';

	let {
		selectedTool = null,
		onneedstool,
		onmoretools
	}: {
		selectedTool?: CatalogTool | null;
		onneedstool: () => void;
		onmoretools: () => void;
	} = $props();
	let input: HTMLInputElement;
	let files = $state<File[]>([]);
	let selectionError = $state('');
	let dragging = $state(false);
	let dragDepth = 0;
	let processing = $state(false);
	let processingError = $state('');
	let resultUrl = $state('');
	let resultSize = $state(0);
	let controller: AbortController | undefined;
	let previousToolId: string | undefined;

	$effect(() => {
		const toolId = selectedTool?.id;
		if (toolId === previousToolId) return;
		previousToolId = toolId;
		untrack(clearProcessingResult);
	});

	onDestroy(clearProcessingResult);

	function clearProcessingResult() {
		controller?.abort();
		controller = undefined;
		processing = false;
		processingError = '';
		resultSize = 0;
		if (resultUrl) URL.revokeObjectURL(resultUrl);
		resultUrl = '';
	}

	function addFiles(incoming: FileList | File[]) {
		const candidates = Array.from(incoming);
		const pdfs = candidates.filter(
			(file) => file.type === 'application/pdf' || /\.pdf$/i.test(file.name)
		);
		selectionError =
			pdfs.length === candidates.length
				? ''
				: 'Please choose PDF files. Other file types were skipped.';
		const next = [...files];
		for (const file of pdfs) {
			if (
				!next.some(
					(existing) =>
						existing.name === file.name &&
						existing.size === file.size &&
						existing.lastModified === file.lastModified
				)
			)
				next.push(file);
		}
		const firstFiles = files.length === 0 && next.length > 0;
		if (next.length !== files.length) clearProcessingResult();
		files = next;
		input.value = '';
		if (firstFiles && !selectedTool) onneedstool();
	}

	function removeFile(file: File) {
		clearProcessingResult();
		files = files.filter((item) => item !== file);
	}

	async function mergeFiles() {
		if (processing || files.length < 2 || selectedTool?.id !== 'merge') return;
		clearProcessingResult();
		processing = true;
		controller = new AbortController();
		try {
			const bytes = await processPdfs('merge', files, controller.signal);
			resultSize = bytes.byteLength;
			resultUrl = URL.createObjectURL(
				new Blob([bytes.slice().buffer], { type: 'application/pdf' })
			);
		} catch (error) {
			if (!(error instanceof DOMException && error.name === 'AbortError')) {
				processingError = error instanceof Error ? error.message : 'The PDFs could not be merged.';
			}
		} finally {
			processing = false;
			controller = undefined;
		}
	}

	function drop(event: DragEvent) {
		event.preventDefault();
		dragging = false;
		dragDepth = 0;
		if (event.dataTransfer) addFiles(event.dataTransfer.files);
	}

	function formatSize(bytes: number) {
		return bytes < 1024 * 1024
			? `${Math.max(1, Math.round(bytes / 1024))} KB`
			: `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
	}
</script>

<input
	bind:this={input}
	type="file"
	accept="application/pdf,.pdf"
	multiple
	class="hidden"
	aria-label="Choose PDF files"
	onchange={() => input.files && addFiles(input.files)}
/>

<section
	aria-label="PDF file selection"
	class="drop-zone flex h-full min-h-64 w-full flex-col rounded-2xl bg-panel p-5 transition-colors duration-200 sm:p-6 {dragging
		? 'ring-2 ring-brand'
		: ''}"
	ondragenter={(event) => {
		event.preventDefault();
		dragDepth += 1;
		dragging = true;
	}}
	ondragover={(event) => {
		event.preventDefault();
		if (event.dataTransfer) event.dataTransfer.dropEffect = 'copy';
	}}
	ondragleave={() => {
		dragDepth -= 1;
		if (dragDepth <= 0) dragging = false;
	}}
	ondrop={drop}
>
	{#if files.length === 0}
		<button
			type="button"
			class="group flex min-h-56 w-full flex-1 flex-col items-center justify-center gap-5 rounded-2xl border-2 border-dashed border-current text-muted transition-colors duration-200 hover:bg-brand/5 hover:text-brand sm:min-h-60 lg:min-h-0"
			onclick={() => input.click()}
			aria-label="Choose PDFs or drop them here"
			aria-describedby="file-feedback"
		>
			<IconUpload
				class="upload-icon size-10 transition-transform duration-200 sm:size-11"
				stroke={1.8}
				aria-hidden="true"
			/>
			<span class="text-xl font-medium lg:text-lg 2xl:text-xl"
				>{dragging
					? 'Let go. They stay here.'
					: selectedTool
						? `Drop PDFs for ${selectedTool.label}`
						: 'Drop in your PDFs'}</span
			>
		</button>
	{:else}
		<div class="flex min-h-0 flex-1 flex-col">
			<div class="mb-4 flex items-center justify-between gap-3">
				<h2 class="font-semibold">
					{files.length}
					{files.length === 1 ? 'PDF selected' : 'PDFs selected'}
				</h2>
				<button
					class="flex items-center gap-1 rounded-lg p-2 text-sm text-brand hover:bg-brand/10"
					onclick={() => input.click()}><IconPlus size={18} aria-hidden="true" /> Add files</button
				>
			</div>
			<ul class="min-h-0 flex-1 space-y-2 overflow-y-auto pr-1">
				{#each files as file (file)}
					<li class="flex items-center gap-3 rounded-xl bg-canvas/50 p-3">
						<IconFileTypePdf class="shrink-0 text-brand" size={26} aria-hidden="true" />
						<div class="min-w-0 flex-1">
							<p class="truncate text-sm" title={file.name}>{file.name}</p>
							<p class="mt-1 text-xs text-muted">{formatSize(file.size)}</p>
						</div>
						<button
							class="rounded-md p-2 text-muted hover:bg-white/5 hover:text-white"
							aria-label={`Remove ${file.name}`}
							disabled={processing}
							onclick={() => removeFile(file)}><IconX size={18} aria-hidden="true" /></button
						>
					</li>
				{/each}
			</ul>
			<div class="mt-3 text-xs leading-relaxed text-muted" role="status">
				{#if selectedTool}
					{selectedTool.label} selected
				{:else}
					Choose a tool, or browse
					<button
						class="rounded-sm text-brand underline decoration-brand/40 underline-offset-4 hover:decoration-brand"
						onclick={onmoretools}>More tools</button
					>.
				{/if}
			</div>
			{#if selectedTool?.id === 'merge'}
				<div class="mt-4 border-t border-white/10 pt-4">
					{#if resultUrl}
						<div class="flex flex-wrap items-center justify-between gap-3">
							<p class="text-sm font-medium text-merge">
								Merged locally · {formatSize(resultSize)}
							</p>
							<a
								href={resultUrl}
								download="plico-merged.pdf"
								class="flex min-h-11 items-center gap-2 rounded-xl bg-brand px-4 py-2.5 text-sm font-bold text-panel transition-colors hover:bg-violet-300"
								><IconDownload size={19} aria-hidden="true" />Download PDF</a
							>
						</div>
					{:else}
						<div class="flex flex-wrap items-center justify-between gap-3">
							<p class="text-xs leading-relaxed text-muted">
								{files.length < 2 ? 'Add one more PDF to merge.' : 'Ready to merge on this device.'}
							</p>
							{#if processing}
								<div class="flex items-center gap-3">
									<button
										class="min-h-11 px-2 text-sm text-muted hover:text-white"
										onclick={() => controller?.abort()}>Cancel</button
									>
									<span
										class="flex min-h-11 items-center gap-2 rounded-xl bg-brand/80 px-4 py-2.5 text-sm font-bold text-panel"
										><IconLoader2 class="animate-spin" size={19} aria-hidden="true" />Merging…</span
									>
								</div>
							{:else}
								<button
									disabled={files.length < 2}
									class="min-h-11 rounded-xl bg-brand px-4 py-2.5 text-sm font-bold text-panel transition-colors hover:bg-violet-300 disabled:cursor-not-allowed disabled:opacity-40"
									onclick={mergeFiles}>Merge PDFs</button
								>
							{/if}
						</div>
					{/if}
					{#if processingError}<p role="alert" class="mt-3 text-xs leading-relaxed text-convert">
							{processingError}
						</p>{/if}
				</div>
			{:else if selectedTool}
				<p class="mt-4 border-t border-white/10 pt-4 text-xs leading-relaxed text-muted">
					Merge PDF is connected to the local engine first. {selectedTool.label} is not available yet.
				</p>
			{/if}
		</div>
	{/if}
	<p
		id="file-feedback"
		role="status"
		class={selectionError ? 'mt-3 text-xs text-convert' : 'sr-only'}
	>
		{selectionError ||
			(files.length ? `${files.length} PDF files selected. Nothing has been uploaded.` : '')}
	</p>
</section>
