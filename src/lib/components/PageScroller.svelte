<script lang="ts">
	import { onMount, tick, type Snippet } from 'svelte';
	import { IconArrowAutofitWidth, IconMinus, IconPlus, IconX } from '@tabler/icons-svelte-runes';
	import type { PDFDocumentLoadingTask, PDFDocumentProxy } from 'pdfjs-dist';
	import type { PreviewPage } from '$lib/pdf/stamp-layout';
	import { getWorkspace } from '$lib/workspace.svelte';
	import PdfUnlock from './PdfUnlock.svelte';
	import ScrollerPage from './ScrollerPage.svelte';

	let {
		file,
		current = $bindable(1),
		overlay,
		forms,
		onload,
		onremove
	}: {
		file: File;
		/// The page most in view, from 1.
		current?: number;
		overlay: Snippet<[PreviewPage]>;
		/// Renders form fields apart from each page, handing over the canvases
		/// pdf.js draws some of them on; see `ScrollerPage`.
		forms?: (page: number, canvases: Map<string, HTMLCanvasElement | HTMLCanvasElement[]>) => void;
		/// Once the PDF opens, with its page count and the open document.
		onload: (count: number, pdf: PDFDocumentProxy) => void;
		onremove: () => void;
	} = $props();

	// Zoom steps, as a share of the page's printed size on a 96 DPI screen.
	const LEVELS = [0.25, 0.33, 0.5, 0.67, 0.75, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2, 2.5, 3, 4];
	// CSS pixels per point at 100%.
	const ACTUAL = 96 / 72;
	// Fitting a wide screen's width makes text larger than anyone reads.
	const FIT_LIMIT = 1100;

	const workspace = getWorkspace();
	const password = $derived(workspace.passwordFor(file));
	const lock = $derived(workspace.lockState(file));
	let pdf = $state<PDFDocumentProxy | null>(null);
	let status = $state('Loading PDF...');
	// Each page's size in points as the reader sees it; pages not yet
	// measured borrow the first page's.
	let sizes = $state.raw<{ width: number; height: number }[]>([]);
	let near = $state.raw<boolean[]>([]);
	let available = $state(0);
	// Pixels per point, or null to fit the width.
	let zoom = $state<number | null>(null);
	let scroller = $state<HTMLDivElement>();
	const pages: HTMLDivElement[] = [];

	const widest = $derived(Math.max(1, ...sizes.map((size) => size.width)));
	const scale = $derived(zoom ?? Math.max(0.1, Math.min(available, FIT_LIMIT) / widest));
	const percent = $derived(Math.round((scale / ACTUAL) * 100));
	const pageCount = $derived(pdf?.numPages ?? 0);

	$effect(() => {
		const source = file;
		const secret = password;
		let cancelled = false;
		let loadingTask: PDFDocumentLoadingTask | undefined;
		pdf = null;
		sizes = [];
		near = [];
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
				const first = (await document.getPage(1)).getViewport({ scale: 1 });
				if (cancelled) return;
				workspace.opened(source);
				const measured = Array.from({ length: document.numPages }, () => ({
					width: first.width,
					height: first.height
				}));
				sizes = measured;
				near = measured.map((_, index) => index < 2);
				pdf = document;
				status = '';
				onload(document.numPages, document);
				// The rest are measured in the background, a batch at a time.
				for (let number = 2; number <= document.numPages; number++) {
					const { width, height } = (await document.getPage(number)).getViewport({ scale: 1 });
					if (cancelled) return;
					measured[number - 1] = { width, height };
					if (number % 25 === 0 || number === document.numPages) sizes = [...measured];
				}
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

	// Pages within two screens keep their pixels.
	let observer: IntersectionObserver | undefined;
	onMount(() => {
		observer = new IntersectionObserver(
			(entries) => {
				const next = [...near];
				for (const entry of entries) {
					const index = Number((entry.target as HTMLElement).dataset.index);
					next[index] = entry.isIntersecting;
				}
				near = next;
			},
			{ rootMargin: '200% 0px' }
		);
		for (const page of pages) if (page) observer.observe(page);
		return () => observer?.disconnect();
	});

	function watch(node: HTMLDivElement, index: number) {
		pages[index] = node;
		observer?.observe(node);
		return {
			destroy() {
				observer?.unobserve(node);
				delete pages[index];
			}
		};
	}

	// The page crossing the middle of the screen, or failing that the one
	// nearest it.
	let frame = 0;
	function track() {
		cancelAnimationFrame(frame);
		frame = requestAnimationFrame(() => {
			const middle = window.innerHeight / 2;
			let [best, distance] = [current, Infinity];
			pages.forEach((page, index) => {
				if (!page || !near[index]) return;
				const box = page.getBoundingClientRect();
				const away =
					middle < box.top ? box.top - middle : middle > box.bottom ? middle - box.bottom : 0;
				if (away < distance) [best, distance] = [index + 1, away];
			});
			if (best !== current) current = best;
		});
	}
	$effect(() => {
		void near;
		void scale;
		track();
	});

	/// Which page is under a point, and where on it, so a zoom can keep that
	/// spot where it is.
	function anchor(x: number, y: number) {
		let found: { index: number; fx: number; fy: number } | undefined;
		let distance = Infinity;
		pages.forEach((page, index) => {
			if (!page) return;
			const box = page.getBoundingClientRect();
			const away = y < box.top ? box.top - y : y > box.bottom ? y - box.bottom : 0;
			if (away < distance) {
				distance = away;
				found = {
					index,
					fx: (x - box.left) / box.width,
					fy: (y - box.top) / box.height
				};
			}
		});
		return found && { ...found, x, y };
	}

	async function zoomTo(next: number | null, at?: [number, number]) {
		const view = scroller?.getBoundingClientRect();
		const [x, y] = at ?? [view ? view.left + view.width / 2 : 0, window.innerHeight / 2];
		const held = anchor(x, y);
		const [low, high] = [LEVELS[0] * ACTUAL, LEVELS[LEVELS.length - 1] * ACTUAL];
		zoom = next === null ? null : Math.min(high, Math.max(low, next));
		await tick();
		const page = held && pages[held.index];
		if (!held || !page || !scroller) return;
		const box = page.getBoundingClientRect();
		window.scrollBy(0, box.top + held.fy * box.height - held.y);
		scroller.scrollLeft += box.left + held.fx * box.width - held.x;
	}

	/// Scrolls so that `at`, a share of the page's height from its top, sits a
	/// little above the middle of the screen.
	export function reveal(number: number, at = 0) {
		const page = pages[number - 1];
		if (!page) return;
		const box = page.getBoundingClientRect();
		const still = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
		window.scrollBy({
			top: box.top + at * box.height - window.innerHeight * 0.4,
			behavior: still ? 'auto' : 'smooth'
		});
	}

	function step(direction: number) {
		const share = scale / ACTUAL;
		const level =
			direction > 0
				? (LEVELS.find((value) => value > share + 0.005) ?? LEVELS[LEVELS.length - 1])
				: ([...LEVELS].reverse().find((value) => value < share - 0.005) ?? LEVELS[0]);
		void zoomTo(level * ACTUAL);
	}

	// Ctrl or Cmd with the wheel, and trackpad pinches, zoom about the pointer.
	function pinch(node: HTMLDivElement) {
		const wheel = (event: WheelEvent) => {
			if (!event.ctrlKey && !event.metaKey) return;
			event.preventDefault();
			void zoomTo(scale * Math.exp(-event.deltaY * 0.0025), [event.clientX, event.clientY]);
		};
		node.addEventListener('wheel', wheel, { passive: false });
		return { destroy: () => node.removeEventListener('wheel', wheel) };
	}

	const control =
		'flex size-9 items-center justify-center rounded-lg text-muted transition-colors enabled:hover:bg-panel-hover enabled:hover:text-white disabled:opacity-40';
</script>

<svelte:window onscroll={track} onresize={track} />

<section aria-label="Preview" class="w-full space-y-6">
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
		{#if pdf}
			<div bind:this={scroller} use:pinch class="w-full overflow-x-auto overscroll-x-contain">
				<!-- As wide as the widest page once zoomed past the screen, so it
				can be scrolled sideways; centred otherwise. -->
				<div class="mx-auto flex w-max min-w-full flex-col items-center gap-6 pb-2">
					{#each sizes as size, index (index)}
						{@const page = { number: index + 1, width: size.width, height: size.height }}
						<div
							use:watch={index}
							data-index={index}
							class="relative isolate shrink-0 overflow-hidden rounded-sm bg-white shadow-2xl shadow-black/40"
							style:width="{size.width * scale}px"
							style:height="{size.height * scale}px"
						>
							<ScrollerPage
								{pdf}
								number={page.number}
								{scale}
								near={near[index] ?? false}
								label={`Page ${page.number} of ${file.name}`}
								forms={forms && ((canvases) => forms(page.number, canvases))}
							/>
							{@render overlay(page)}
						</div>
					{/each}
				</div>
			</div>
		{:else if lock}
			<div class="py-16">
				<PdfUnlock {lock} focus onunlock={(value) => workspace.unlock(file, value)} />
			</div>
		{:else if status}
			<p role="status" class="py-16 text-center text-sm text-muted">{status}</p>
		{/if}
	</div>

	{#if pdf}
		<!-- Floats at the bottom of the screen while the pages scroll; above the
		pinned action bar on phones. -->
		<div
			class="sticky bottom-6 z-10 mx-auto flex w-fit items-center gap-1 rounded-xl bg-panel/90 p-1 shadow-xl ring-1 shadow-black/40 ring-white/10 backdrop-blur-md max-lg:bottom-28"
			role="group"
			aria-label="Zoom"
		>
			<button
				type="button"
				class={control}
				onclick={() => step(-1)}
				disabled={scale <= LEVELS[0] * ACTUAL + 1e-3}
				aria-label="Zoom out"
				title="Zoom out"><IconMinus size={18} /></button
			>
			<span class="w-12 text-center text-xs font-semibold text-white tabular-nums">{percent}%</span>
			<button
				type="button"
				class={control}
				onclick={() => step(1)}
				disabled={scale >= LEVELS[LEVELS.length - 1] * ACTUAL - 1e-3}
				aria-label="Zoom in"
				title="Zoom in"><IconPlus size={18} /></button
			>
			<button
				type="button"
				class="{control} {zoom === null ? 'text-brand' : ''}"
				onclick={() => void zoomTo(null)}
				aria-pressed={zoom === null}
				aria-label="Fit width"
				title="Fit width"><IconArrowAutofitWidth size={18} /></button
			>
			{#if pageCount > 1}
				<span aria-hidden="true" class="mx-1 h-5 w-px bg-white/10"></span>
				<span class="px-2 text-xs text-muted tabular-nums" aria-live="polite"
					><span class="font-semibold text-white">{current}</span> / {pageCount}</span
				>
			{/if}
		</div>
	{/if}
</section>
