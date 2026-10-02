<script lang="ts">
	import { untrack, type Snippet } from 'svelte';
	import { cubicOut } from 'svelte/easing';
	import { IconChevronLeft, IconChevronRight, IconX } from '@tabler/icons-svelte-runes';
	import type { PDFDocumentLoadingTask, PDFDocumentProxy, RenderTask } from 'pdfjs-dist';
	import type { PreviewPage } from '$lib/pdf/stamp-layout';
	import { getWorkspace } from '$lib/workspace.svelte';
	import PdfUnlock from './PdfUnlock.svelte';

	let {
		file,
		behind = false,
		stamped,
		reducedMotion,
		overlay,
		onload,
		onrender,
		onremove
	}: {
		file: File;
		/// Draws the overlay under the page, the way a watermark behind the
		/// content shows through a page without a background.
		behind?: boolean;
		stamped: (page: number) => boolean;
		reducedMotion: boolean;
		overlay: Snippet<[PreviewPage]>;
		onload: (count: number) => void;
		/// Each time a page finishes drawing, before it is shown, with the
		/// canvas it was drawn on.
		onrender?: (canvas: HTMLCanvasElement, page: PreviewPage) => void;
		onremove: () => void;
	} = $props();

	const workspace = getWorkspace();
	const password = $derived(workspace.passwordFor(file));
	const lock = $derived(workspace.lockState(file));
	let pdf = $state<PDFDocumentProxy | null>(null);
	let status = $state('Loading PDF...');
	// The page asked for, then its size once known, then the page on screen.
	// A page is only shown once drawn, so it arrives whole with its overlay.
	let current = $state(1);
	let target = $state<PreviewPage | null>(null);
	let shown = $state.raw<{
		page: PreviewPage;
		bitmap: HTMLCanvasElement;
		direction: number;
	} | null>(null);
	// Which way the pager last moved, so the number rolls the same way.
	let travel = $state(0);
	let available = $state(0);
	let viewportHeight = $state(900);
	const pageCount = $derived(pdf?.numPages ?? 0);
	const maxHeight = $derived(Math.max(320, Math.min(900, viewportHeight - 300)));
	const isStamped = $derived(stamped(current));

	function fit(page: PreviewPage) {
		const width = Math.max(1, Math.min(available, 560, (maxHeight * page.width) / page.height));
		return { width, height: (width * page.height) / page.width };
	}
	const frame = $derived(shown ? fit(shown.page) : target ? fit(target) : null);

	$effect(() => {
		const source = file;
		const secret = password;
		let cancelled = false;
		let loadingTask: PDFDocumentLoadingTask | undefined;
		pdf = null;
		target = null;
		shown = null;
		current = 1;
		status = 'Loading PDF...';
		async function load() {
			try {
				const pdfjs = await import('pdfjs-dist');
				const worker = await import('pdfjs-dist/build/pdf.worker.min.mjs?url');
				if (cancelled) return;
				pdfjs.GlobalWorkerOptions.workerSrc = worker.default;
				const data = new Uint8Array(await source.arrayBuffer());
				if (cancelled) return;
				loadingTask = pdfjs.getDocument({
					data,
					password: secret || undefined,
					cMapUrl: '/pdfjs/cmaps/',
					cMapPacked: true,
					standardFontDataUrl: '/pdfjs/standard_fonts/'
				});
				loadingTask.onPassword = () => {
					if (!cancelled) {
						status = '';
						workspace.setLock(source, secret ? 'incorrect' : 'locked');
					}
					void loadingTask?.destroy();
				};
				const document = await loadingTask.promise;
				if (cancelled) return;
				workspace.opened(source);
				pdf = document;
				status = '';
				onload(document.numPages);
			} catch {
				if (!cancelled && !workspace.lockState(source)) status = 'Preview unavailable';
			}
		}
		void load();
		return () => {
			cancelled = true;
			void loadingTask?.destroy();
		};
	});

	// The page's size in points, as the reader sees it: crop box, rotation.
	$effect(() => {
		const source = pdf;
		const number = current;
		if (!source) return;
		let cancelled = false;
		void source.getPage(number).then((proxy) => {
			if (cancelled) return;
			const { width, height } = proxy.getViewport({ scale: 1 });
			target = { number, width, height };
		});
		return () => {
			cancelled = true;
		};
	});

	$effect(() => {
		const source = pdf;
		const page = target;
		const width = page ? Math.round(fit(page).width) : 0;
		if (!source || !page || width <= 1) return;
		let cancelled = false;
		let task: RenderTask | undefined;
		// A resize redraws the same page once the size settles; a new page
		// draws straight away.
		const resizing = untrack(() => shown?.page.number === page.number);
		const delay = window.setTimeout(
			async () => {
				try {
					const proxy = await source.getPage(page.number);
					if (cancelled) return;
					const viewport = proxy.getViewport({ scale: width / page.width });
					const outputScale = Math.min(window.devicePixelRatio || 1, 2);
					const bitmap = document.createElement('canvas');
					bitmap.width = Math.max(1, Math.floor(viewport.width * outputScale));
					bitmap.height = Math.max(1, Math.floor(viewport.height * outputScale));
					task = proxy.render({
						canvas: bitmap,
						viewport,
						transform: [outputScale, 0, 0, outputScale, 0, 0]
					});
					await task.promise;
					if (cancelled) return;
					onrender?.(bitmap, page);
					const previous = untrack(() => shown);
					shown = {
						page,
						bitmap,
						direction: previous ? Math.sign(page.number - previous.page.number) : 0
					};
				} catch {
					// A cancelled render is replaced by the next one.
				}
			},
			resizing ? 120 : 0
		);
		return () => {
			cancelled = true;
			clearTimeout(delay);
			task?.cancel();
		};
	});

	function paint(canvas: HTMLCanvasElement, bitmap: HTMLCanvasElement) {
		const draw = (source: HTMLCanvasElement) => {
			canvas.width = source.width;
			canvas.height = source.height;
			canvas.getContext('2d')?.drawImage(source, 0, 0);
		};
		draw(bitmap);
		return { update: draw };
	}

	// A quiet crossfade with a hint of direction: the new page drifts in a
	// few pixels from the side the reader is heading, the old one fades.
	function turn(node: Element, { direction, entering }: { direction: number; entering: boolean }) {
		if (reducedMotion) return { duration: 0 };
		if (!entering)
			return {
				duration: 160,
				easing: cubicOut,
				css: (t: number, u: number) =>
					`transform: translateX(${-direction * u * 8}px); opacity: ${t}`
			};
		return {
			duration: 240,
			easing: cubicOut,
			css: (t: number, u: number) => `transform: translateX(${direction * u * 12}px); opacity: ${t}`
		};
	}

	function roll(node: Element, { entering }: { entering: boolean }) {
		if (reducedMotion || travel === 0) return { duration: 0 };
		const distance = (entering ? travel : -travel) * 6;
		return {
			duration: entering ? 200 : 140,
			easing: cubicOut,
			css: (t: number, u: number) => `transform: translateY(${u * distance}px); opacity: ${t}`
		};
	}

	function go(offset: number) {
		const next = Math.max(1, Math.min(pageCount, current + offset));
		if (next === current) return;
		travel = Math.sign(next - current);
		current = next;
	}
</script>

<svelte:window bind:innerHeight={viewportHeight} />

<section aria-label="Preview" class="mx-auto w-full max-w-3xl space-y-6">
	<div class="mx-auto flex w-fit max-w-full items-center gap-2 px-1">
		<p class="max-w-xl min-w-0 truncate text-sm font-semibold" title={file.name}>{file.name}</p>
		<span class="shrink-0 text-xs whitespace-nowrap text-muted"
			>{pageCount ? `${pageCount} ${pageCount === 1 ? 'page' : 'pages'}` : status}</span
		>
		<button
			type="button"
			onclick={onremove}
			class="flex size-8 shrink-0 items-center justify-center rounded-lg bg-white/10 text-white backdrop-blur-sm transition-colors hover:bg-convert hover:text-canvas"
			aria-label="Remove PDF"
			title="Remove PDF"><IconX size={18} stroke={2.5} /></button
		>
	</div>

	<div bind:clientWidth={available} class="w-full">
		{#if pdf && frame}
			<!-- Sized to the page on screen, easing between page sizes; each page
			is its own layer so the old one can leave while the new arrives. -->
			<div
				class="relative mx-auto"
				style:width="{frame.width}px"
				style:height="{frame.height}px"
				style:transition={reducedMotion || !shown
					? 'none'
					: 'width 320ms cubic-bezier(0.22, 1, 0.36, 1), height 320ms cubic-bezier(0.22, 1, 0.36, 1)'}
			>
				{#if shown}
					{#key shown.page.number}
						{@const layer = shown}
						{@const size = fit(layer.page)}
						{@const drawn = stamped(layer.page.number)}
						<div
							class="absolute top-1/2 left-1/2 isolate -translate-1/2 overflow-hidden rounded-sm bg-white shadow-2xl shadow-black/40"
							style:width="{size.width}px"
							style:height="{size.height}px"
							in:turn={{ direction: layer.direction, entering: true }}
							out:turn={{ direction: layer.direction, entering: false }}
						>
							{#if behind && drawn}{@render overlay(layer.page)}{/if}
							<canvas
								use:paint={layer.bitmap}
								aria-label={`Page ${layer.page.number} of ${file.name}`}
								class="absolute inset-0 size-full {behind ? 'mix-blend-multiply' : ''}"
							></canvas>
							{#if !behind && drawn}{@render overlay(layer.page)}{/if}
						</div>
					{/key}
				{/if}
			</div>
		{:else if lock}
			<div class="py-16">
				<PdfUnlock {lock} focus onunlock={(value) => workspace.unlock(file, value)} />
			</div>
		{:else if status}
			<p role="status" class="py-16 text-center text-sm text-muted">{status}</p>
		{/if}
	</div>

	{#if pdf && pageCount > 1}
		<div class="flex items-center justify-center gap-3" role="group" aria-label="Preview page">
			<button
				type="button"
				onclick={() => go(-1)}
				disabled={current <= 1}
				aria-label="Previous page"
				class="flex size-10 shrink-0 items-center justify-center rounded-xl border-2 border-white/10 bg-panel text-muted transition-colors enabled:hover:border-brand/40 enabled:hover:text-brand disabled:opacity-40"
				><IconChevronLeft size={18} /></button
			>
			<p
				class="flex min-w-24 items-center justify-center text-sm text-muted tabular-nums"
				aria-live="polite"
			>
				<span class="inline-grid justify-items-end font-semibold text-white">
					{#key current}<span
							class="col-start-1 row-start-1"
							in:roll={{ entering: true }}
							out:roll={{ entering: false }}>{current}</span
						>{/key}
				</span>
				<span class="ml-1">/ {pageCount}</span>{#if !isStamped}<span
						class="ml-2 rounded-md bg-white/10 px-1.5 py-0.5 text-[11px]">Skipped</span
					>{/if}
			</p>
			<button
				type="button"
				onclick={() => go(1)}
				disabled={current >= pageCount}
				aria-label="Next page"
				class="flex size-10 shrink-0 items-center justify-center rounded-xl border-2 border-white/10 bg-panel text-muted transition-colors enabled:hover:border-brand/40 enabled:hover:text-brand disabled:opacity-40"
				><IconChevronRight size={18} /></button
			>
		</div>
	{/if}
</section>
