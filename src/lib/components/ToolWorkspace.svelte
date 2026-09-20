<script lang="ts">
	import { onDestroy, onMount, untrack } from 'svelte';
	import { cubicIn, cubicOut } from 'svelte/easing';
	import { fade } from 'svelte/transition';
	import { IconArrowRight, IconDownload, IconLoader2 } from '@tabler/icons-svelte-runes';
	import { toolCategoryColor, type CatalogTool } from '$lib/tool-catalog';
	import type { SplitRange } from '$lib/split-ranges';
	import type { SplitOptions } from '$lib/pdf/types';
	import { getWorkspace, formatSize } from '$lib/workspace.svelte';
	import {
		processCompressPdf,
		processImagesToPdf,
		processPdfToImages,
		processPdfs,
		processSplitPdf
	} from '$lib/pdf/processor';
	import type { ImagePdfOptions, PdfImageOptions } from '$lib/pdf/types';
	import { DownloadJob } from '$lib/pdf/download-job.svelte';
	import { parsePageRange } from '$lib/pdf/page-range';
	import {
		officeOperation,
		officeTools,
		inputAccept,
		acceptsFile
	} from '$lib/pdf/office-conversion';
	import { processOfficeFile } from '$lib/pdf/office-processor';
	import PdfDropzone from './PdfDropzone.svelte';
	import DocumentCards from './DocumentCards.svelte';
	import CompressSettings from './CompressSettings.svelte';
	import SplitSettings from './SplitSettings.svelte';
	import SplitViewer from './SplitViewer.svelte';
	import PdfToImageSettings from './PdfToImageSettings.svelte';
	import ImageToPdfSettings from './ImageToPdfSettings.svelte';
	let { tool }: { tool: CatalogTool } = $props();
	const workspace = getWorkspace();
	const isMerge = $derived(tool.id === 'merge');
	const isSplit = $derived(tool.id === 'split');
	const isCompress = $derived(tool.id === 'compress');
	const isPdfToImage = $derived(
		tool.id === 'pdf-to-jpg' ||
			tool.id === 'pdf-to-png' ||
			tool.id === 'pdf-to-image' ||
			tool.id === 'pdf-to-images'
	);
	const isImageToPdf = $derived(
		tool.id === 'jpg-to-pdf' ||
			tool.id === 'png-to-pdf' ||
			tool.id === 'image-to-pdf' ||
			tool.id === 'images-to-pdf'
	);
	const officeTool = $derived(officeOperation(tool.id));
	const inputType = $derived(
		officeTool ? officeTools[officeTool].input : isImageToPdf ? 'image' : 'pdf'
	);
	workspace.use(untrack(() => inputType));
	const accent = $derived(toolCategoryColor(tool.id));
	const buttonColor = $derived(
		(
			{
				'text-brand': 'bg-brand',
				'text-convert': 'bg-convert',
				'text-compress': 'bg-compress',
				'text-merge': 'bg-merge',
				'text-split': 'bg-split'
			} as Record<string, string>
		)[accent] ?? 'bg-brand'
	);
	const cardMode = $derived(
		isMerge ? 'merge' : isCompress ? 'compress' : isImageToPdf ? 'image' : 'single'
	);
	let input = $state<HTMLInputElement>();
	let dragged = $state<File | null>(null);
	let keyboardPicked = $state<File | null>(null);
	let reducedMotion = $state(false);
	const currentFile = $derived(workspace.files[0]);
	onMount(() => {
		const preference = window.matchMedia('(prefers-reduced-motion: reduce)');
		const update = () => {
			reducedMotion = preference.matches;
		};
		update();
		preference.addEventListener('change', update);
		return () => preference.removeEventListener('change', update);
	});
	let pageCount = $state(0);
	let splitRanges = $state<SplitRange[]>([{ id: 0, from: 1, to: 1 }]);
	let splitMode = $state<'ranges' | 'fixed'>('ranges');
	let splitInterval = $state(1);
	let splitCombine = $state(false);
	let compressLevel = $state<'light' | 'balanced' | 'strong'>('balanced');
	let compressRemoveMetadata = $state(false);
	let compressRemoveThumbnails = $state(false);
	let filename = $state(untrack(() => (isImageToPdf ? 'plico-images' : 'plico-merged')));
	let downloadLink: HTMLAnchorElement;
	const job = new DownloadJob();
	const processing = $derived(job.processing);
	const error = $derived(job.error);
	const result = $derived(job.result);
	let pdfToImageFormat = $state<'jpg' | 'png'>(
		untrack(() => (tool.id === 'pdf-to-png' ? 'png' : 'jpg'))
	);
	let pdfToImageDpi = $state(150);
	let pdfToImageQuality = $state(80);
	let pdfToImagePageRange = $state('');
	let imagePdfPageSize = $state<'a4' | 'letter'>('a4');
	let imagePdfMargin = $state(18);
	const resultFormat = $derived(job.format);
	const resultSize = $derived(job.size);
	const resultInputSize = $derived(job.inputSize);
	const splitSignature = $derived(
		JSON.stringify({
			mode: splitMode,
			interval: splitInterval,
			combine: splitCombine,
			ranges: splitRanges.map(({ from, to }) => [from, to])
		})
	);
	const compressSignature = $derived(
		JSON.stringify([compressLevel, compressRemoveMetadata, compressRemoveThumbnails])
	);
	const pdfToImageSignature = $derived(
		JSON.stringify([pdfToImageFormat, pdfToImageDpi, pdfToImageQuality, pdfToImagePageRange])
	);
	const imagePdfSignature = $derived(JSON.stringify([imagePdfPageSize, imagePdfMargin]));
	const compressOptions = $derived({
		imageQuality: compressLevel === 'light' ? 0 : compressLevel === 'balanced' ? 80 : 50,
		maxImageDimension: compressLevel === 'strong' ? 1600 : compressLevel === 'balanced' ? 2400 : 0,
		removeMetadata: compressRemoveMetadata,
		removeThumbnails: compressRemoveThumbnails
	});
	const splitValid = $derived(
		!!currentFile &&
			pageCount > 0 &&
			(splitMode === 'ranges'
				? splitRanges.length > 0 &&
					splitRanges.every(
						({ from, to }) =>
							Number.isInteger(from) &&
							Number.isInteger(to) &&
							from >= 1 &&
							to >= from &&
							to <= pageCount
					)
				: Number.isInteger(splitInterval) && splitInterval > 0)
	);
	const pdfToImageValid = $derived(!!currentFile);
	const imagePdfValid = $derived(workspace.files.length > 0);
	const actionDisabled = $derived(
		officeTool
			? !currentFile || processing
			: isMerge
				? workspace.files.length < 2 || processing || !!dragged || !!keyboardPicked
				: isSplit
					? !splitValid || processing
					: isCompress
						? workspace.files.length === 0 || processing || !!dragged || !!keyboardPicked
						: isPdfToImage
							? !pdfToImageValid || processing
							: isImageToPdf
								? !imagePdfValid || processing || !!dragged || !!keyboardPicked
								: true
	);
	const actionUnavailable = $derived(
		officeTool
			? !currentFile
			: isMerge
				? workspace.files.length < 2 || !!dragged || !!keyboardPicked
				: isSplit
					? !splitValid
					: isCompress
						? workspace.files.length === 0 || !!dragged || !!keyboardPicked
						: isPdfToImage
							? !pdfToImageValid
							: isImageToPdf
								? !imagePdfValid || !!dragged || !!keyboardPicked
								: true
	);
	const downloadName = $derived(
		officeTool
			? `${currentFile?.name.replace(/\.[^.]+$/, '') || 'document'}.${officeTools[officeTool].output}`
			: isSplit
				? `${currentFile?.name.replace(/\.pdf$/i, '') || 'document'}-split.${resultFormat}`
				: isCompress
					? `${currentFile?.name.replace(/\.pdf$/i, '') || 'document'}-compressed.${resultFormat}`
					: isPdfToImage
						? `${currentFile?.name.replace(/\.pdf$/i, '') || 'document'}-images.${resultFormat}`
						: isImageToPdf
							? `${filename.trim().replace(/\.pdf$/i, '') || (currentFile ? currentFile.name.replace(/\.[^.]+$/, '') : 'images')}.pdf`
							: `${filename.trim().replace(/\.pdf$/i, '') || 'plico-merged'}.pdf`
	);
	let prevInputType = $state(untrack(() => inputType));
	$effect(() => {
		const currentType = inputType;
		if (prevInputType !== currentType) {
			prevInputType = currentType;
			keyboardPicked = null;
			filename = currentType === 'image' ? 'plico-images' : 'plico-merged';
			workspace.use(currentType);
		}
	});
	$effect(() => {
		void workspace.files;
		void filename;
		void splitSignature;
		void compressSignature;
		void pdfToImageSignature;
		void imagePdfSignature;
		untrack(() => job.clear());
	});
	$effect(() => {
		void currentFile;
		pageCount = 0;
		splitRanges = [{ id: 0, from: 1, to: 1 }];
		splitMode = 'ranges';
		splitInterval = 1;
		splitCombine = false;
		pdfToImagePageRange = '';
	});
	onDestroy(() => job.clear());
	async function merge() {
		if (processing || dragged || workspace.files.length < 2) return;
		const files = [...workspace.files];
		await job.run(
			async (signal) => ({ bytes: await processPdfs('merge', files, signal), format: 'pdf' }),
			'Could not merge these PDFs.',
			() => downloadLink?.click()
		);
	}
	async function split() {
		if (processing || !splitValid || !currentFile) return;
		const file = currentFile;
		const options: SplitOptions =
			splitMode === 'ranges'
				? {
						mode: 'ranges',
						ranges: splitRanges.map(({ from, to }) => ({ from, to })),
						combine: splitCombine
					}
				: { mode: 'fixed', interval: splitInterval };
		await job.run(
			(signal) => processSplitPdf(file, options, signal),
			'Could not split this PDF.',
			() => downloadLink?.click()
		);
	}
	async function compress() {
		if (processing || dragged || workspace.files.length === 0) return;
		const files = [...workspace.files];
		const inputSize = files.reduce((total, file) => total + file.size, 0);
		await job.run(
			(signal) => processCompressPdf(files, compressOptions, signal),
			'Could not compress these PDFs.',
			() => downloadLink?.click(),
			inputSize
		);
	}
	async function convertPdfToImage() {
		if (processing || !pdfToImageValid || !currentFile) return;
		const file = currentFile;
		const options: PdfImageOptions = {
			format: pdfToImageFormat,
			dpi: pdfToImageDpi,
			quality: pdfToImageQuality,
			pages: parsePageRange(pdfToImagePageRange, pageCount)
		};
		await job.run(
			(signal) => processPdfToImages(file, options, signal),
			'Could not convert this PDF to images.',
			() => downloadLink?.click()
		);
	}
	async function convertImagesToPdf() {
		if (processing || !imagePdfValid) return;
		const files = [...workspace.files];
		const options: ImagePdfOptions = {
			pageWidth: imagePdfPageSize === 'letter' ? 612.0 : 595.28,
			pageHeight: imagePdfPageSize === 'letter' ? 792.0 : 841.89,
			margin: imagePdfMargin
		};
		await job.run(
			(signal) => processImagesToPdf(files, options, signal),
			'Could not convert images to PDF.',
			() => downloadLink?.click()
		);
	}
	async function convertOffice() {
		if (!officeTool || !currentFile || processing) return;
		const operation = officeTool;
		const file = currentFile;
		await job.run(
			(signal) => processOfficeFile(operation, file, signal),
			`Could not convert this ${inputType.toUpperCase()} file.`,
			() => downloadLink?.click()
		);
	}
	function add(files: FileList) {
		workspace.add(files, inputType);
		if (input) input.value = '';
	}
	function replace(files: FileList) {
		const file = Array.from(files).find((item) => acceptsFile(item, inputType));
		if (file) {
			workspace.files = [file];
			workspace.error = '';
		} else if (files.length) {
			workspace.error = `Please choose a ${inputType.toUpperCase()} file.`;
		}
		if (input) input.value = '';
	}
</script>

<main id="main-content" class="flex flex-1 flex-col">
	<div
		class="grid flex-1 grid-cols-[minmax(0,1fr)] lg:grid-cols-[minmax(0,1fr)_22rem] xl:grid-cols-[minmax(0,1fr)_24rem]"
	>
		<section
			aria-label="Documents"
			class="relative isolate flex min-w-0 flex-col overflow-hidden px-6 pt-6 sm:px-10 lg:px-16 lg:pt-10 {isSplit ||
			isPdfToImage
				? 'pb-2 lg:pb-16'
				: 'pb-12 lg:pb-20'}"
			ondragover={(event) => event.preventDefault()}
			ondrop={(event) => {
				if (
					!event.defaultPrevented &&
					!dragged &&
					!keyboardPicked &&
					event.dataTransfer?.files.length
				) {
					event.preventDefault();
					if (!processing) {
						if (isSplit || isPdfToImage || officeTool) replace(event.dataTransfer.files);
						else workspace.add(event.dataTransfer.files, inputType);
					}
				}
			}}
		>
			<img
				src="/hero-contours.svg"
				alt=""
				aria-hidden="true"
				class="pointer-events-none absolute -right-48 -bottom-48 -z-10 w-240 max-w-none opacity-[0.07] select-none"
			/>
			<div class="grid min-h-0 flex-1">
				{#if workspace.files.length === 0}
					<div
						in:fade={{ duration: reducedMotion ? 0 : 260, easing: cubicOut }}
						out:fade={{ duration: reducedMotion ? 0 : 150, easing: cubicIn }}
						class="col-start-1 row-start-1 mx-auto flex w-full max-w-xl flex-col justify-center py-12 lg:py-20"
					>
						<div class="h-80"><PdfDropzone selectedTool={tool} emptyOnly /></div>
					</div>
				{:else}
					<div
						in:fade={{ duration: reducedMotion ? 0 : 260, easing: cubicOut }}
						out:fade={{ duration: reducedMotion ? 0 : 150, easing: cubicIn }}
						class="col-start-1 row-start-1 flex min-w-0 flex-col"
					>
						{#if !isSplit && !isPdfToImage && !officeTool}<input
								bind:this={input}
								type="file"
								accept={inputAccept[inputType]}
								multiple
								class="hidden"
								aria-label={isImageToPdf ? 'Add images' : 'Add PDF files'}
								onchange={() => input?.files && add(input.files)}
							/>{/if}
						{#if isSplit && currentFile}
							<div class="my-auto w-full py-6 lg:py-10">
								{#key currentFile}<SplitViewer
										file={currentFile}
										ranges={splitRanges}
										mode={splitMode}
										interval={splitInterval}
										onrangechange={(id, from, to) =>
											(splitRanges = splitRanges.map((range) =>
												range.id === id ? { ...range, from, to } : range
											))}
										onload={(count) => {
											pageCount = count;
											if (
												splitRanges.length === 1 &&
												splitRanges[0].from === 1 &&
												splitRanges[0].to === 1
											)
												splitRanges[0].to = count;
										}}
										onremove={() => workspace.remove(currentFile)}
									/>{/key}
							</div>
						{:else}
							<DocumentCards
								mode={cardMode}
								{accent}
								officeFormat={inputType === 'docx' || inputType === 'pptx' || inputType === 'xlsx'
									? inputType
									: undefined}
								{processing}
								{reducedMotion}
								bind:dragged
								bind:keyboardPicked
								onadd={() => input?.click()}
								onload={(count) => {
									if (!pageCount) pageCount = count;
								}}
							/>
						{/if}
						{#if workspace.error}<p role="alert" class="mt-4 text-sm text-convert">
								{workspace.error}
							</p>{/if}
					</div>
				{/if}
			</div>
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
					{#key currentFile}<SplitSettings
							{pageCount}
							{reducedMotion}
							bind:ranges={splitRanges}
							bind:mode={splitMode}
							bind:interval={splitInterval}
							bind:combine={splitCombine}
						/>{/key}
				{:else if isCompress}
					<CompressSettings
						bind:level={compressLevel}
						bind:removeMetadata={compressRemoveMetadata}
						bind:removeThumbnails={compressRemoveThumbnails}
					/>
				{:else if isPdfToImage}
					<PdfToImageSettings
						bind:format={pdfToImageFormat}
						bind:dpi={pdfToImageDpi}
						bind:quality={pdfToImageQuality}
						bind:pageRange={pdfToImagePageRange}
						{pageCount}
					/>
				{:else if isImageToPdf}
					<ImageToPdfSettings bind:pageSize={imagePdfPageSize} bind:margin={imagePdfMargin} />
					<label class="block text-sm font-medium"
						>Output filename
						<div
							class="mt-3 flex items-center rounded-xl border border-white/10 bg-canvas px-3 focus-within:border-white/25"
						>
							<input
								bind:value={filename}
								class="min-w-0 flex-1 bg-transparent py-3 text-sm outline-none"
								aria-label="Output filename"
								placeholder="plico-images"
							/><span class="text-xs text-muted">.pdf</span>
						</div></label
					>
				{/if}
			</div>
			<div class="shrink-0 space-y-4 p-6 sm:p-8" aria-live="polite">
				<a
					bind:this={downloadLink}
					href={result}
					rel="external"
					download={downloadName}
					class="hidden"
					tabindex="-1"
					aria-hidden="true">Download {resultFormat.toUpperCase()}</a
				>
				{#if isCompress && result && resultInputSize > 0}
					<p
						class="flex flex-wrap items-center justify-center gap-x-1.5 gap-y-0.5 text-center text-xs text-muted"
					>
						<span class="inline-flex items-center gap-1.5">
							<span>{formatSize(resultInputSize)}</span>
							<span class="sr-only">to</span>
							<IconArrowRight size={14} stroke={1.75} aria-hidden="true" />
							<span>{formatSize(resultSize)}</span>
						</span>
						{#if resultSize < resultInputSize}
							<span>({Math.round((1 - resultSize / resultInputSize) * 100)}% smaller)</span>
						{/if}
					</p>
				{/if}
				<button
					disabled={actionDisabled}
					onclick={() =>
						result
							? downloadLink?.click()
							: isSplit
								? void split()
								: officeTool
									? void convertOffice()
									: isCompress
										? void compress()
										: isPdfToImage
											? void convertPdfToImage()
											: isImageToPdf
												? void convertImagesToPdf()
												: void merge()}
					aria-label={result
						? `Download ${resultFormat.toUpperCase()} again`
						: processing
							? isSplit
								? 'Splitting PDF'
								: officeTool
									? `Converting to ${officeTools[officeTool].output.toUpperCase()}...`
									: isCompress
										? 'Compressing PDF'
										: isPdfToImage
											? `Converting to ${pdfToImageFormat.toUpperCase()}...`
											: isImageToPdf
												? 'Converting images to PDF...'
												: 'Merging PDF'
							: tool.label}
					class="group relative isolate flex min-h-14 w-full items-center justify-center overflow-hidden rounded-xl px-4 py-4 text-sm font-bold text-canvas transition-[background-color,filter,transform] duration-200 enabled:hover:brightness-110 disabled:cursor-not-allowed motion-safe:enabled:active:scale-[0.985] {buttonColor} {actionUnavailable
						? 'opacity-40'
						: ''}"
				>
					<span
						class="pointer-events-none absolute inset-0 origin-left bg-white/15 motion-safe:transition-transform motion-safe:duration-500 motion-safe:ease-[cubic-bezier(0.22,1,0.36,1)] {result
							? 'scale-x-100'
							: 'scale-x-0'}"
						aria-hidden="true"
					></span>
					<span class="relative z-10 grid place-items-center">
						<span
							aria-hidden={!!result}
							class="col-start-1 row-start-1 flex items-center justify-center gap-3 whitespace-nowrap motion-safe:transition-[opacity,transform] motion-safe:duration-200 {result
								? '-translate-y-2 opacity-0'
								: 'translate-y-0 opacity-100'}"
							>{#if processing}<IconLoader2 class="animate-spin" size={20} />{officeTool
									? 'Converting...'
									: isSplit
										? 'Splitting...'
										: isCompress
											? 'Compressing...'
											: isPdfToImage
												? 'Converting...'
												: isImageToPdf
													? 'Converting...'
													: 'Merging...'}{:else}
								{tool.label}<IconArrowRight size={20} />{/if}</span
						>
						<span
							aria-hidden={!result}
							class="col-start-1 row-start-1 flex items-center justify-center gap-3 whitespace-nowrap motion-safe:transition-[opacity,transform] motion-safe:duration-300 {result
								? 'translate-y-0 opacity-100 motion-safe:delay-100'
								: 'translate-y-2 opacity-0'}"
							><IconDownload size={20} />Download {resultFormat.toUpperCase()}</span
						>
					</span></button
				>
				{#if error}<p role="alert" class="text-sm text-convert">{error}</p>{/if}
			</div>
		</aside>
	</div>
</main>
