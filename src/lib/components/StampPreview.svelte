<script lang="ts">
	import { untrack, type Snippet } from 'svelte';
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
		onremove: () => void;
	} = $props();

	const workspace = getWorkspace();
	const password = $derived(workspace.passwordFor(file));
	const lock = $derived(workspace.lockState(file));
	let pdf = $state<PDFDocumentProxy | null>(null);
	let status = $state('Loading PDF...');
	let current = $state(1);
	let page = $state<PreviewPage | null>(null);
	let available = $state(0);
	let viewportHeight = $state(900);
	let canvas = $state<HTMLCanvasElement>();
	let snapshot = $state<HTMLCanvasElement>();
	let rendered = $state(false);
	const pageCount = $derived(pdf?.numPages ?? 0);
	const maxHeight = $derived(Math.max(320, Math.min(900, viewportHeight - 300)));
	const displayWidth = $derived(
		page ? Math.max(1, Math.min(available, 560, (maxHeight * page.width) / page.height)) : 0
	);
	const displayHeight = $derived(page ? (displayWidth * page.height) / page.width : 0);
	const isStamped = $derived(stamped(current));

	$effect(() => {
		const source = file;
		const secret = password;
		let cancelled = false;
		let loadingTask: PDFDocumentLoadingTask | undefined;
		pdf = null;
		page = null;
		rendered = false;
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
			page = { number, width, height };
		});
		return () => {
			cancelled = true;
		};
	});

	$effect(() => {
		const source = pdf;
		const shown = page;
		const target = canvas;
		const fade = snapshot;
		// Resizing re-renders only once the size settles.
		const width = Math.round(displayWidth);
		const allowMotion = !reducedMotion;
		if (!source || !shown || !target || width <= 1) return;
		let cancelled = false;
		let task: RenderTask | undefined;
		let frame = 0;
		let fadeTimeout = 0;
		const delay = window.setTimeout(
			async () => {
				try {
					const proxy = await source.getPage(shown.number);
					if (cancelled) return;
					const viewport = proxy.getViewport({ scale: width / shown.width });
					const outputScale = Math.min(window.devicePixelRatio || 1, 2);
					const next = document.createElement('canvas');
					next.width = Math.max(1, Math.floor(viewport.width * outputScale));
					next.height = Math.max(1, Math.floor(viewport.height * outputScale));
					task = proxy.render({
						canvas: next,
						viewport,
						transform: [outputScale, 0, 0, outputScale, 0, 0]
					});
					await task.promise;
					if (cancelled) return;
					const crossfade = allowMotion && fade && target.dataset.page !== String(shown.number);
					if (crossfade && rendered) {
						fade.width = target.width;
						fade.height = target.height;
						fade.getContext('2d')?.drawImage(target, 0, 0);
						fade.style.transition = 'none';
						fade.style.opacity = '1';
						fade.style.display = 'block';
					}
					target.width = next.width;
					target.height = next.height;
					target.getContext('2d')?.drawImage(next, 0, 0);
					target.dataset.page = String(shown.number);
					rendered = true;
					if (crossfade && fade.style.display === 'block') {
						void fade.offsetWidth;
						fade.style.transition = 'opacity 200ms ease-out';
						frame = requestAnimationFrame(() => {
							if (!cancelled) fade.style.opacity = '0';
						});
						fadeTimeout = window.setTimeout(() => {
							if (!cancelled) fade.style.display = 'none';
						}, 240);
					}
				} catch {
					// A cancelled render is replaced by the next one.
				}
			},
			untrack(() => rendered) ? 120 : 0
		);
		return () => {
			cancelled = true;
			clearTimeout(delay);
			clearTimeout(fadeTimeout);
			cancelAnimationFrame(frame);
			task?.cancel();
		};
	});

	function go(offset: number) {
		current = Math.max(1, Math.min(pageCount, current + offset));
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
		{#if pdf && page}
			<div
				class="relative isolate mx-auto overflow-hidden rounded-sm bg-white shadow-2xl shadow-black/40"
				style:width="{displayWidth}px"
				style:height="{displayHeight}px"
			>
				{#if behind && isStamped}{@render overlay(page)}{/if}
				<canvas
					bind:this={canvas}
					aria-label={`Page ${current} of ${file.name}`}
					class="absolute inset-0 size-full {behind ? 'mix-blend-multiply' : ''} {rendered
						? ''
						: 'opacity-0'}"
				></canvas>
				<canvas
					bind:this={snapshot}
					aria-hidden="true"
					class="pointer-events-none absolute inset-0 size-full {behind
						? 'mix-blend-multiply'
						: ''}"
					style="display: none; opacity: 0"
				></canvas>
				{#if !behind && isStamped}{@render overlay(page)}{/if}
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
			<p class="min-w-24 text-center text-sm text-muted tabular-nums" aria-live="polite">
				<span class="font-semibold text-white">{current}</span> / {pageCount}{#if !isStamped}<span
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
