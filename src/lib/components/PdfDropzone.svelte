<script lang="ts">
	import { IconUpload, IconFileTypePdf, IconX, IconPlus } from '@tabler/icons-svelte-runes';
	import { tools, type ToolId } from '$lib/tools';

	let { selectedTool = null }: { selectedTool?: ToolId | null } = $props();
	let input: HTMLInputElement;
	let files = $state<File[]>([]);
	let error = $state('');
	let dragging = $state(false);
	let dragDepth = 0;
	const tool = $derived(tools.find((item) => item.id === selectedTool));

	function addFiles(incoming: FileList | File[]) {
		const candidates = Array.from(incoming);
		const pdfs = candidates.filter(
			(file) => file.type === 'application/pdf' || /\.pdf$/i.test(file.name)
		);
		error =
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
		files = next;
		input.value = '';
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
	class="drop-zone relative flex h-full min-h-64 w-full flex-col rounded-2xl bg-panel p-5 transition-colors duration-200 sm:p-6 {dragging
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
			class="group relative flex min-h-56 w-full flex-1 flex-col items-center justify-center gap-5 rounded-2xl text-muted transition-colors duration-200 hover:bg-brand/5 hover:text-brand sm:min-h-60 lg:min-h-0"
			onclick={() => input.click()}
			aria-label="Choose PDFs or drop them here"
			aria-describedby="file-feedback"
		>
			<svg
				class="pointer-events-none absolute inset-0 h-full w-full overflow-visible"
				aria-hidden="true"
				><rect
					x="1"
					y="1"
					width="calc(100% - 2px)"
					height="calc(100% - 2px)"
					rx="16"
					fill="none"
					stroke="currentColor"
					stroke-width="2"
					stroke-dasharray="12 12"
				/></svg
			>
			<IconUpload
				class="upload-icon size-10 transition-transform duration-200 sm:size-11"
				stroke={1.8}
				aria-hidden="true"
			/>
			<span class="text-xl font-medium lg:text-[clamp(1.0625rem,1.05vw,1.25rem)]"
				>{dragging
					? 'Let go. They stay here.'
					: tool
						? `Drop PDFs to ${tool.label.toLowerCase()}`
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
							onclick={() => (files = files.filter((item) => item !== file))}
							><IconX size={18} aria-hidden="true" /></button
						>
					</li>
				{/each}
			</ul>
			<p class="mt-4 text-xs leading-relaxed text-muted"></p>
		</div>
	{/if}
	<p id="file-feedback" role="status" class={error ? 'mt-3 text-xs text-convert' : 'sr-only'}>
		{error ||
			(files.length ? `${files.length} PDF files selected. Nothing has been uploaded.` : '')}
	</p>
</section>
