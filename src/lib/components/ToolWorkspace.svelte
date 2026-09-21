<script lang="ts">
	import { onDestroy, onMount, untrack } from 'svelte';
	import { cubicIn, cubicOut } from 'svelte/easing';
	import { fade } from 'svelte/transition';
	import { IconArrowRight, IconDownload, IconLoader2 } from '@tabler/icons-svelte-runes';
	import { isToolSupported, toolCategoryColor, type CatalogTool } from '$lib/tool-catalog';
	import type { SplitRange } from '$lib/split-ranges';
	import type { OrganizePage, SplitOptions } from '$lib/pdf/types';
	import { pageKey } from '$lib/pdf/sources';
	import { getWorkspace, formatSize } from '$lib/workspace.svelte';
	import {
		processCompressPdf,
		processImagesToPdf,
		processOrganizePdf,
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
	import ResultPreview from './ResultPreview.svelte';
	import CompressSettings from './CompressSettings.svelte';
	import SplitSettings from './SplitSettings.svelte';
	import SplitViewer from './SplitViewer.svelte';
	import OrganizeViewer from './OrganizeViewer.svelte';
	import PdfToImageSettings from './PdfToImageSettings.svelte';
	import ImageToPdfSettings from './ImageToPdfSettings.svelte';
	let { tool }: { tool: CatalogTool } = $props();
	const workspace = getWorkspace();
	const supported = $derived(isToolSupported(tool.id));
	const isMerge = $derived(tool.id === 'merge');
	const isSplit = $derived(tool.id === 'split');
	const isOrganize = $derived(tool.id === 'organize');
	const isExtract = $derived(tool.id === 'extract');
	const isRemove = $derived(tool.id === 'remove');
	const isRotate = $derived(tool.id === 'rotate');
	const isPageTool = $derived(isOrganize || isExtract || isRemove || isRotate);
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
	let organizePages = $state<OrganizePage[]>([]);
	let selectedPages = $state<string[]>([]);
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
	const resultBlob = $derived(job.resultBlob);
	// Only a single PDF can feed the next tool. Zips and image outputs are dead
	// ends for Continue, so they keep just the download button.
	const canContinue = $derived(resultFormat === 'pdf');
	const splitSignature = $derived(
		JSON.stringify({
			mode: splitMode,
			interval: splitInterval,
			combine: splitCombine,
			ranges: splitRanges.map(({ from, to }) => [from, to])
		})
	);
	const organizeSignature = $derived(JSON.stringify([organizePages, selectedPages]));
	const pageToolValid = $derived(
		!!currentFile &&
			organizePages.length > 0 &&
			(isExtract
				? selectedPages.length > 0
				: isRemove
					? selectedPages.length > 0 && selectedPages.length < organizePages.length
					: isRotate
						? organizePages.some((page) => page.rotation !== 0)
						: true)
	);
	const compressSignature = $derived(
		JSON.stringify([compressLevel, compressRemoveMetadata, compressRemoveThumbnails])
	);
	const pdfToImageSignature = $derived(
		JSON.stringify([pdfToImageFormat, pdfToImageDpi, pdfToImageQuality, pdfToImagePageRange])
	);
	const imagePdfSignature = $derived(JSON.stringify([imagePdfPageSize, imagePdfMargin]));
	const compressOptions = $derived({
		// clarification here
		// 0 = preserve original, no changes; a higher compression value means better quality otherwise
		// dimension of 0 also means preserve original
		// TODO: should we do image compression with light mode?
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
					: isPageTool
						? !pageToolValid || processing
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
					: isPageTool
						? !pageToolValid
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
				: isOrganize
					? `${currentFile?.name.replace(/\.pdf$/i, '') || 'document'}-organized.pdf`
					: isExtract
						? `${currentFile?.name.replace(/\.pdf$/i, '') || 'document'}-extracted.pdf`
						: isRemove
							? `${currentFile?.name.replace(/\.pdf$/i, '') || 'document'}-pages-removed.pdf`
							: isRotate
								? `${currentFile?.name.replace(/\.pdf$/i, '') || 'document'}-rotated.pdf`
								: isCompress
									? `${currentFile?.name.replace(/\.pdf$/i, '') || 'document'}-compressed.${resultFormat}`
									: isPdfToImage
										? `${currentFile?.name.replace(/\.pdf$/i, '') || 'document'}-images.${resultFormat}`
										: isImageToPdf
											? `${filename.trim().replace(/\.pdf$/i, '') || (currentFile ? currentFile.name.replace(/\.[^.]+$/, '') : 'images')}.pdf`
											: `${filename.trim().replace(/\.pdf$/i, '') || 'plico-merged'}.pdf`
	);
	const resultFile = $derived(
		resultBlob ? new File([resultBlob], `preview.${resultFormat}`, { type: resultBlob.type }) : null
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
		void splitSignature;
		void organizeSignature;
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
	// Pages are owned here but reconciled by the viewer as PDFs come and go, so
	// drop selections that no longer name a live page.
	$effect(() => {
		const keys = new Set(organizePages.map(pageKey));
		if (selectedPages.some((key) => !keys.has(key)))
			selectedPages = selectedPages.filter((key) => keys.has(key));
	});
	$effect(() => {
		void tool.id;
		organizePages = [];
		selectedPages = [];
	});
	onDestroy(() => job.clear());
	async function merge() {
		if (processing || dragged || workspace.files.length < 2) return;
		const files = [...workspace.files];
		await job.run(
			async (signal) => ({ bytes: await processPdfs('merge', files, signal), format: 'pdf' }),
			'Could not merge these PDFs.'
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
		await job.run((signal) => processSplitPdf(file, options, signal), 'Could not split this PDF.');
	}
	async function organize() {
		if (processing || !pageToolValid) return;
		const files = [...workspace.files];
		const selected = new Set(selectedPages);
		const pages = organizePages
			.filter((page) =>
				isExtract ? selected.has(pageKey(page)) : isRemove ? !selected.has(pageKey(page)) : true
			)
			.map((page) => ({ ...page }));
		await job.run(
			(signal) => processOrganizePdf(files, pages, signal),
			`Could not ${isExtract ? 'extract pages from' : isRemove ? 'remove pages from' : isRotate ? 'rotate pages in' : 'organize'} ${files.length > 1 ? 'these PDFs' : 'this PDF'}.`
		);
	}
	async function compress() {
		if (processing || dragged || workspace.files.length === 0) return;
		const files = [...workspace.files];
		const inputSize = files.reduce((total, file) => total + file.size, 0);
		await job.run(
			(signal) => processCompressPdf(files, compressOptions, signal),
			'Could not compress these PDFs.',
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
			'Could not convert this PDF to images.'
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
			'Could not convert images to PDF.'
		);
	}
	async function convertOffice() {
		if (!officeTool || !currentFile || processing) return;
		const operation = officeTool;
		const file = currentFile;
		await job.run(
			(signal) => processOfficeFile(operation, file, signal),
			`Could not convert this ${inputType.toUpperCase()} file.`
		);
	}
	// Load the result back in as the input for the next step. The effect that
	// clears the job when files change then resets this workspace to the
	// editing state, with the produced PDF already loaded.
	function continueWithResult() {
		const blob = job.resultBlob;
		if (!blob || resultFormat !== 'pdf') return;
		const file = new File([blob], downloadName, { type: blob.type });
		workspace.use('pdf');
		workspace.files = [file];
		workspace.error = '';
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
	{#if !supported}
		<div class="grid flex-1 place-items-center px-6 py-16">
			<div
				class="flex w-full max-w-md flex-col items-center gap-4 rounded-2xl bg-panel px-8 py-14 text-center"
			>
				<tool.icon size={36} stroke={1.5} class={accent} />
				<h1 class="text-xl font-semibold tracking-tight">{tool.label}</h1>
				<p class="text-sm leading-relaxed text-muted">
					This tool is listed in the toolbox but is not available yet. It has not been built — try
					Merge, Split, Compress, Organize, or a conversion in the meantime.
				</p>
			</div>
		</div>
	{:else}
		<div
			class="grid flex-1 grid-cols-[minmax(0,1fr)] lg:grid-cols-[minmax(0,1fr)_22rem] xl:grid-cols-[minmax(0,1fr)_24rem]"
		>
			<section
				aria-label="Documents"
				class="relative isolate flex min-w-0 flex-col overflow-hidden px-6 pt-6 sm:px-10 lg:px-16 lg:pt-10 {isSplit ||
				isPdfToImage ||
				isPageTool
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
							if (isSplit || isPdfToImage || officeTool || (isPageTool && !isOrganize))
								replace(event.dataTransfer.files);
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
							{#if result}
								<ResultPreview
									file={resultFile}
									format={resultFormat}
									name={downloadName}
									size={resultSize}
									onclose={() => job.clear()}
								/>
							{:else}
								{#if isOrganize || (!isSplit && !isPageTool && !isPdfToImage && !officeTool)}<input
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
								{:else if isPageTool && currentFile}
									<div class="flex min-h-0 w-full flex-1 flex-col py-6 lg:py-0">
										{#key isOrganize ? 'organize' : currentFile}<OrganizeViewer
												files={workspace.files}
												pages={organizePages}
												mode={isExtract
													? 'extract'
													: isRemove
														? 'remove'
														: isRotate
															? 'rotate'
															: 'organize'}
												selected={selectedPages}
												{processing}
												{reducedMotion}
												onpageschange={(pages) => (organizePages = pages)}
												onselectionchange={(keys) => (selectedPages = keys)}
												onadd={() => input?.click()}
												onremove={(file) => workspace.remove(file)}
											/>{/key}
									</div>
								{:else}
									<DocumentCards
										mode={cardMode}
										{accent}
										officeFormat={inputType === 'docx' ||
										inputType === 'pptx' ||
										inputType === 'xlsx'
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
							{/if}
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
					{#if result}
						{#if canContinue}
							<p class="text-center text-xs text-muted">
								Loads this result as the input, so you can pick the next tool.
							</p>
							<button
								onclick={continueWithResult}
								class="flex min-h-14 w-full items-center justify-center gap-3 rounded-xl border-2 border-white/10 px-4 py-4 text-sm font-bold transition-colors hover:border-white/20 hover:bg-white/5"
								aria-label="Continue with this result as the next input"
								><IconArrowRight size={20} />Continue</button
							>
						{/if}
						<button
							onclick={() => downloadLink?.click()}
							class="group relative isolate flex min-h-14 w-full items-center justify-center overflow-hidden rounded-xl px-4 py-4 text-sm font-bold text-canvas transition-[background-color,filter,transform] duration-200 enabled:hover:brightness-110 motion-safe:enabled:active:scale-[0.985] {buttonColor}"
							aria-label={`Download ${resultFormat.toUpperCase()}`}
						>
							<span
								class="pointer-events-none absolute inset-0 origin-left scale-x-100 bg-white/15"
								aria-hidden="true"
							></span>
							<span class="relative z-10 flex items-center justify-center gap-3 whitespace-nowrap"
								><IconDownload size={20} />Download {resultFormat.toUpperCase()}</span
							>
						</button>
					{:else}
						<p class="text-center text-xs text-muted">
							Preview first — nothing downloads until you say so.
						</p>
						<button
							disabled={actionDisabled}
							onclick={() =>
								isSplit
									? void split()
									: isPageTool
										? void organize()
										: officeTool
											? void convertOffice()
											: isCompress
												? void compress()
												: isPdfToImage
													? void convertPdfToImage()
													: isImageToPdf
														? void convertImagesToPdf()
														: void merge()}
							aria-label={processing
								? isSplit
									? 'Splitting PDF'
									: isPageTool
										? `${tool.label} in progress`
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
								class="pointer-events-none absolute inset-0 origin-left scale-x-0 bg-white/15 motion-safe:transition-transform motion-safe:duration-500 motion-safe:ease-[cubic-bezier(0.22,1,0.36,1)]"
								aria-hidden="true"
							></span>
							<span class="relative z-10 grid place-items-center">
								<span
									class="col-start-1 row-start-1 flex items-center justify-center gap-3 whitespace-nowrap"
									>{#if processing}<IconLoader2 class="animate-spin" size={20} />{officeTool
											? 'Converting...'
											: isSplit
												? 'Splitting...'
												: isPageTool
													? 'Processing...'
													: isCompress
														? 'Compressing...'
														: isPdfToImage
															? 'Converting...'
															: isImageToPdf
																? 'Converting...'
																: 'Merging...'}{:else}
										{tool.label}<IconArrowRight size={20} />{/if}</span
								>
							</span></button
						>
					{/if}
					{#if error}<p role="alert" class="text-sm text-convert">{error}</p>{/if}
				</div>
			</aside>
		</div>
	{/if}
</main>
