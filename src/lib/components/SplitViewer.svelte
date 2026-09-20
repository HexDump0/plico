<script lang="ts">
	import { onMount, tick, untrack } from 'svelte';
	import { SvelteMap } from 'svelte/reactivity';
	import { cubicOut } from 'svelte/easing';
	import { slide } from 'svelte/transition';
	import { IconArrowRight, IconX } from '@tabler/icons-svelte-runes';
	import type { PDFDocumentLoadingTask, PDFDocumentProxy } from 'pdfjs-dist';
	import type { SplitRange } from '$lib/split-ranges';
	import { rangeCollapse, rangeReveal } from '$lib/motion/range';
	import SplitThumbnail from './SplitThumbnail.svelte';

	let {
		file,
		ranges,
		mode,
		interval,
		onrangechange,
		onload,
		onremove
	}: {
		file: File;
		ranges: SplitRange[];
		mode: 'ranges' | 'fixed';
		interval: number;
		onrangechange: (id: number, from: number, to: number) => void;
		onload: (count: number) => void;
		onremove: () => void;
	} = $props();

	let pdf = $state<PDFDocumentProxy | null>(null);
	let status = $state('Loading PDF…');
	let picker = $state<{ id: number; edge: 'from' | 'to' } | null>(null);
	let reducedMotion = $state(false);
	let hasPagesBefore = $state(false);
	let hasPagesAfter = $state(false);
	let rangeGrid = $state<HTMLDivElement>();
	let previousRangeCount = 0;
	let previousMode: 'ranges' | 'fixed' | null = null;
	const positionAnimations = new SvelteMap<HTMLElement, Animation>();
	const pageStripMask = $derived(
		`linear-gradient(to right, ${hasPagesBefore ? 'transparent 0%, black 2rem' : 'black 0%, black 2rem'}, black calc(100% - 2rem), ${hasPagesAfter ? 'transparent 100%' : 'black 100%'})`
	);
	const pageCount = $derived(pdf?.numPages ?? 0);
	const fixedCount = $derived(
		pageCount && Number.isInteger(interval) && interval > 0 ? Math.ceil(pageCount / interval) : 0
	);
	const visibleRanges = $derived(
		mode === 'ranges'
			? ranges
			: Array.from({ length: Math.min(fixedCount, 4) }, (_, index) => ({
					id: index,
					from: index * interval + 1,
					to: Math.min(pageCount, (index + 1) * interval)
				}))
	);
	const pickedRange = $derived(ranges.find((range) => range.id === picker?.id));

	onMount(() => {
		const preference = window.matchMedia('(prefers-reduced-motion: reduce)');
		const update = () => {
			reducedMotion = preference.matches;
			if (reducedMotion) {
				for (const animation of positionAnimations.values()) animation.cancel();
				positionAnimations.clear();
			}
		};
		update();
		preference.addEventListener('change', update);
		return () => preference.removeEventListener('change', update);
	});

	$effect.pre(() => {
		const count = visibleRanges.length;
		const currentMode = mode;
		if (
			count > previousRangeCount &&
			previousMode === currentMode &&
			currentMode === 'ranges' &&
			!reducedMotion &&
			rangeGrid
		) {
			const before = untrack(measureRangePositions);
			void tick().then(() => animateRangePositions(before));
		}
		previousRangeCount = count;
		previousMode = currentMode;
	});

	$effect(() => {
		if (mode !== 'ranges' || (picker && !ranges.some((range) => range.id === picker?.id)))
			picker = null;
	});

	$effect(() => {
		const source = file;
		let cancelled = false;
		let loadingTask: PDFDocumentLoadingTask | undefined;
		pdf = null;
		status = 'Loading PDF…';
		async function load() {
			try {
				const pdfjs = await import('pdfjs-dist');
				const worker = await import('pdfjs-dist/build/pdf.worker.min.mjs?url');
				if (cancelled) return;
				pdfjs.GlobalWorkerOptions.workerSrc = worker.default;
				const data = new Uint8Array(await source.arrayBuffer());
				if (cancelled) return;
				loadingTask = pdfjs.getDocument({ data });
				loadingTask.onPassword = () => {
					if (!cancelled) status = 'Password-protected PDF';
					void loadingTask?.destroy();
				};
				const document = await loadingTask.promise;
				if (cancelled) return;
				pdf = document;
				status = '';
				onload(document.numPages);
			} catch {
				if (!cancelled && status !== 'Password-protected PDF') status = 'Preview unavailable';
			}
		}
		void load();
		return () => {
			cancelled = true;
			void loadingTask?.destroy();
		};
	});

	function choosePage(number: number) {
		if (!picker || !pickedRange) return;
		const { id, edge } = picker;
		const from =
			edge === 'from'
				? number
				: Math.min(Number.isInteger(pickedRange.from) ? pickedRange.from : number, number);
		const to =
			edge === 'to'
				? number
				: Math.max(Number.isInteger(pickedRange.to) ? pickedRange.to : number, number);
		onrangechange(id, from, to);
		picker = null;
	}

	function horizontalPageStrip(node: HTMLDivElement) {
		const updateEdges = () => {
			const end = Math.max(0, node.scrollWidth - node.clientWidth);
			hasPagesBefore = node.scrollLeft > 1;
			hasPagesAfter = node.scrollLeft < end - 1;
		};
		const onWheel = (event: WheelEvent) => {
			if (
				event.ctrlKey ||
				event.metaKey ||
				event.shiftKey ||
				Math.abs(event.deltaX) >= Math.abs(event.deltaY)
			)
				return;
			const unit = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? node.clientWidth : 1;
			const delta = event.deltaY * unit;
			const end = Math.max(0, node.scrollWidth - node.clientWidth);
			if ((delta > 0 && node.scrollLeft < end - 1) || (delta < 0 && node.scrollLeft > 1)) {
				event.preventDefault();
				node.scrollLeft = Math.max(0, Math.min(end, node.scrollLeft + delta));
				updateEdges();
			}
		};
		node.addEventListener('wheel', onWheel, { passive: false });
		node.addEventListener('scroll', updateEdges, { passive: true });
		const observer = new ResizeObserver(updateEdges);
		observer.observe(node);
		if (node.firstElementChild) observer.observe(node.firstElementChild);
		updateEdges();
		const frame = requestAnimationFrame(updateEdges);
		return {
			destroy() {
				cancelAnimationFrame(frame);
				observer.disconnect();
				node.removeEventListener('wheel', onWheel);
				node.removeEventListener('scroll', updateEdges);
			}
		};
	}

	function measureRangePositions(exclude?: HTMLElement) {
		return Array.from(rangeGrid?.querySelectorAll<HTMLElement>('[data-range-content]') ?? [])
			.filter((element) => !exclude?.contains(element))
			.map((element) => {
				const rect = element.getBoundingClientRect();
				positionAnimations.get(element)?.cancel();
				positionAnimations.delete(element);
				return { element, rect };
			});
	}

	function animateRangePositions(before: ReturnType<typeof measureRangePositions>) {
		if (reducedMotion) return;
		requestAnimationFrame(() => {
			for (const { element, rect } of before) {
				if (!element.isConnected) continue;
				const after = element.getBoundingClientRect();
				const x = rect.left - after.left;
				const y = rect.top - after.top;
				if (Math.abs(x) < 1 && Math.abs(y) < 1) continue;
				const animation = element.animate(
					[{ transform: `translate(${x}px, ${y}px)` }, { transform: 'translate(0, 0)' }],
					{ duration: 280, easing: 'cubic-bezier(0.22, 1, 0.36, 1)' }
				);
				positionAnimations.set(element, animation);
				animation.addEventListener('finish', () => {
					if (positionAnimations.get(element) === animation) positionAnimations.delete(element);
				});
			}
		});
	}

	function settleAfterRemoval(node: HTMLDivElement) {
		const onOutroEnd = () => {
			if (reducedMotion) return;
			animateRangePositions(measureRangePositions(node));
		};
		node.addEventListener('outroend', onOutroEnd);
		return { destroy: () => node.removeEventListener('outroend', onOutroEnd) };
	}
</script>

<section aria-label="Split preview" class="mx-auto w-full max-w-7xl space-y-8">
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

	{#if pdf && visibleRanges.length}
		<div bind:this={rangeGrid} class="grid grid-cols-1 gap-x-12 gap-y-12 min-[1700px]:grid-cols-2">
			{#each visibleRanges as range, index (range.id)}
				<div
					use:settleAfterRemoval
					in:rangeReveal={{ reducedMotion, preview: mode === 'ranges' }}
					out:rangeCollapse={{ reducedMotion, preview: mode === 'ranges' }}
					class="min-w-0 min-[1700px]:only:col-span-2"
				>
					<div data-range-content class="mx-auto flex w-fit max-w-full items-start gap-2 sm:gap-3">
						<span
							class="mt-1 flex size-8 shrink-0 items-center justify-center rounded-lg bg-split/10 text-xs font-bold text-split"
							aria-label={`${mode === 'ranges' ? 'Range' : 'Part'} ${index + 1}`}>{index + 1}</span
						>
						<div class="flex items-center gap-2 sm:gap-3">
							{#if Number.isInteger(range.from) && range.from >= 1 && range.from <= pageCount}
								<SplitThumbnail
									{pdf}
									{reducedMotion}
									number={range.from}
									variant="range"
									caption="From"
									active={picker?.id === range.id && picker.edge === 'from'}
									selected={false}
									disabled={mode === 'fixed'}
									onselect={() => (picker = { id: range.id, edge: 'from' })}
								/>
							{:else}<span
									class="flex aspect-[2/3] w-24 items-center justify-center text-center text-xs text-muted"
									>Invalid start page</span
								>{/if}
							<IconArrowRight size={18} stroke={1.5} class="shrink-0 text-muted" />
							{#if Number.isInteger(range.to) && range.to >= 1 && range.to <= pageCount}
								<SplitThumbnail
									{pdf}
									{reducedMotion}
									number={range.to}
									variant="range"
									caption="To"
									active={picker?.id === range.id && picker.edge === 'to'}
									selected={false}
									disabled={mode === 'fixed'}
									onselect={() => (picker = { id: range.id, edge: 'to' })}
								/>
							{:else}<span
									class="flex aspect-[2/3] w-24 items-center justify-center text-center text-xs text-muted"
									>Invalid end page</span
								>{/if}
						</div>
					</div>
				</div>
			{/each}
		</div>
		{#if mode === 'fixed' && fixedCount > visibleRanges.length}<p
				class="text-center text-xs text-muted"
			>
				And {fixedCount - visibleRanges.length} more parts.
			</p>{/if}
	{:else if status}
		<p role="status" class="py-16 text-center text-sm text-muted">{status}</p>
	{/if}

	{#if pdf && picker && pickedRange && mode === 'ranges'}
		<div
			transition:slide={{ duration: reducedMotion ? 0 : 220, easing: cubicOut }}
			class="mx-auto w-full max-w-2xl"
		>
			<div class="mb-3 flex items-center justify-between gap-3">
				<div class="flex items-center gap-2">
					<span
						class="flex size-8 items-center justify-center rounded-lg bg-split/10 text-xs font-bold text-split"
						aria-label={`Range ${ranges.findIndex((range) => range.id === picker?.id) + 1}`}
						>{ranges.findIndex((range) => range.id === picker?.id) + 1}</span
					>
					<p class="text-xs font-medium text-split">
						{picker.edge === 'from' ? 'Start' : 'End'} page
					</p>
				</div>
				<button
					type="button"
					aria-label="Close page picker"
					onclick={() => (picker = null)}
					class="rounded-md p-1 text-muted hover:text-white"><IconX size={17} /></button
				>
			</div>
			<div
				use:horizontalPageStrip
				aria-label="Choose a page"
				class="page-strip overflow-x-auto pb-1"
				style={`mask-image: ${pageStripMask}; -webkit-mask-image: ${pageStripMask};`}
			>
				<div class="flex w-max min-w-full justify-center gap-3">
					{#each Array.from({ length: pageCount }, (_, index) => index + 1) as number (number)}
						<SplitThumbnail
							{pdf}
							{reducedMotion}
							{number}
							variant="picker"
							active={number === pickedRange[picker.edge]}
							selected={number >= pickedRange.from && number <= pickedRange.to}
							caption=""
							onselect={() => choosePage(number)}
						/>
					{/each}
				</div>
			</div>
		</div>
	{/if}
</section>

<style>
	.page-strip {
		scrollbar-width: none;
	}
	.page-strip::-webkit-scrollbar {
		display: none;
	}
</style>
