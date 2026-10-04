<script lang="ts" module>
	// Space above the first page and between pages, in CSS pixels. The viewer
	// lays positions out with the same numbers.
	export const PAD = 24;
	export const GAP = 24;
</script>

<script lang="ts">
	import { onMount, type Snippet } from 'svelte';
	import type { PDFDocumentProxy } from 'pdfjs-dist';
	import type { PageSize } from '$lib/pdf/opened-pdf.svelte';
	import type { PreviewPage } from '$lib/pdf/stamp-layout';
	import ScrollerPage from './ScrollerPage.svelte';

	let {
		pdf,
		sizes,
		scale,
		name,
		element = $bindable(),
		overlay,
		onscroll,
		oninterrupt
	}: {
		pdf: PDFDocumentProxy;
		sizes: PageSize[];
		/// CSS pixels per point.
		scale: number;
		name: string;
		/// The scrolling box, for the viewer to read and move.
		element?: HTMLDivElement;
		overlay: Snippet<[PreviewPage]>;
		onscroll: () => void;
		/// The reader took hold of the scroll: wheel, touch or a press.
		oninterrupt: () => void;
	} = $props();

	let near = $state.raw<boolean[]>([]);
	const pages: HTMLDivElement[] = [];

	$effect(() => {
		if (near.length !== sizes.length) near = sizes.map((_, index) => near[index] ?? index < 2);
	});

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
			{ root: element, rootMargin: '200% 0px' }
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
</script>

<!-- Pages fade out under both edges as they scroll, like Split's page
strip, rather than sitting in a box. -->
<div
	bind:this={element}
	role="region"
	aria-label={`Pages of ${name}`}
	{onscroll}
	onwheel={oninterrupt}
	ontouchstart={oninterrupt}
	onpointerdown={oninterrupt}
	class="size-full [scrollbar-width:thin] overflow-auto overscroll-contain [mask-image:linear-gradient(to_bottom,transparent,black_1.5rem,black_calc(100%-1.5rem),transparent)]"
>
	<div
		class="mx-auto flex w-max min-w-full flex-col items-center"
		style:gap="{GAP}px"
		style:padding="{PAD}px 0"
	>
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
					label={`Page ${page.number} of ${name}`}
				/>
				{@render overlay(page)}
			</div>
		{/each}
	</div>
</div>
