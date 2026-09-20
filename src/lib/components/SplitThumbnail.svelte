<script lang="ts">
	import { onMount } from 'svelte';
	import type { PDFDocumentProxy, RenderTask } from 'pdfjs-dist';

	let {
		pdf,
		number,
		variant,
		caption,
		active,
		selected,
		reducedMotion,
		disabled = false,
		onselect
	}: {
		pdf: PDFDocumentProxy;
		number: number;
		variant: 'range' | 'picker';
		caption: string;
		active: boolean;
		selected: boolean;
		reducedMotion: boolean;
		disabled?: boolean;
		onselect: () => void;
	} = $props();
	let button: HTMLButtonElement;
	let canvas: HTMLCanvasElement;
	let snapshot = $state<HTMLCanvasElement>();
	let visible = $state(false);
	let rendered = $state(false);
	let displayedNumber = $state<number | null>(null);
	const large = $derived(variant === 'range');

	onMount(() => {
		const observer = new IntersectionObserver(
			([entry]) => {
				if (entry.isIntersecting) {
					visible = true;
					observer.disconnect();
				}
			},
			{ rootMargin: '120px' }
		);
		observer.observe(button);
		return () => observer.disconnect();
	});

	$effect(() => {
		if (!visible) return;
		const source = pdf;
		const pageNumber = number;
		const width = large ? 248 : 80;
		const height = large ? 330 : 112;
		const allowMotion = !reducedMotion;
		const snapshotCanvas = snapshot;
		let cancelled = false;
		let task: RenderTask | undefined;
		let frame = 0;
		let timeout = 0;
		if (!allowMotion && snapshotCanvas) snapshotCanvas.style.display = 'none';
		async function render() {
			try {
				const page = await source.getPage(pageNumber);
				if (cancelled) return;
				const base = page.getViewport({ scale: 1 });
				const viewport = page.getViewport({
					scale: Math.min(width / base.width, height / base.height)
				});
				const outputScale = Math.min(window.devicePixelRatio || 1, 2);
				const nextCanvas = document.createElement('canvas');
				nextCanvas.width = Math.floor(viewport.width * outputScale);
				nextCanvas.height = Math.floor(viewport.height * outputScale);
				task = page.render({
					canvas: nextCanvas,
					viewport,
					transform: [outputScale, 0, 0, outputScale, 0, 0]
				});
				await task.promise;
				if (cancelled) return;
				const fadingCanvas =
					large && allowMotion && canvas.dataset.ready === 'true' && snapshotCanvas;
				if (fadingCanvas) {
					fadingCanvas.width = canvas.width;
					fadingCanvas.height = canvas.height;
					fadingCanvas.style.width = canvas.style.width;
					fadingCanvas.style.height = canvas.style.height;
					fadingCanvas.getContext('2d')?.drawImage(canvas, 0, 0);
					fadingCanvas.style.transition = 'none';
					fadingCanvas.style.opacity = '1';
					fadingCanvas.style.display = 'block';
				}
				canvas.width = nextCanvas.width;
				canvas.height = nextCanvas.height;
				canvas.style.width = `${viewport.width}px`;
				canvas.style.height = 'auto';
				canvas.getContext('2d')?.drawImage(nextCanvas, 0, 0);
				canvas.dataset.ready = 'true';
				rendered = true;
				displayedNumber = pageNumber;
				if (fadingCanvas) {
					void fadingCanvas.offsetWidth;
					fadingCanvas.style.transition = 'opacity 180ms ease-out';
					frame = requestAnimationFrame(() => {
						if (!cancelled) fadingCanvas.style.opacity = '0';
					});
					timeout = window.setTimeout(() => {
						if (!cancelled) fadingCanvas.style.display = 'none';
					}, 220);
				}
			} catch {
				if (!cancelled && canvas.dataset.ready !== 'true') rendered = false;
			}
		}
		void render();
		return () => {
			cancelled = true;
			task?.cancel();
			cancelAnimationFrame(frame);
			clearTimeout(timeout);
		};
	});
</script>

<button
	bind:this={button}
	type="button"
	{disabled}
	aria-label={`${caption ? `${caption} page` : 'Choose page'} ${number}`}
	aria-pressed={active}
	title={disabled
		? undefined
		: caption
			? `Choose ${caption.toLowerCase()} page`
			: `Choose page ${number}`}
	onclick={onselect}
	class="group flex shrink-0 flex-col items-center gap-2 rounded-lg p-1 text-center transition-colors duration-200 {large
		? 'w-[clamp(6rem,31vw,13rem)] md:w-60 xl:w-64'
		: 'w-24'} {active ? 'text-split' : selected ? 'text-split/80' : 'text-muted'} {disabled
		? 'cursor-default'
		: 'hover:text-split'}"
>
	<span
		class="relative flex w-full items-center justify-center overflow-hidden rounded-sm bg-white shadow-md shadow-black/25 ring-offset-2 ring-offset-panel transition-[box-shadow] duration-200 {large
			? 'aspect-[3/4]'
			: 'aspect-[2/3]'} {active
			? 'ring-2 ring-split'
			: 'group-hover:ring-1 group-hover:ring-split/50'}"
	>
		<canvas
			bind:this={canvas}
			class="max-h-full max-w-full object-contain {rendered ? '' : 'hidden'}"
		></canvas>
		{#if large}<canvas
				bind:this={snapshot}
				aria-hidden="true"
				class="pointer-events-none absolute top-1/2 left-1/2 max-h-full max-w-full -translate-x-1/2 -translate-y-1/2 object-contain"
				style="display: none; opacity: 0"
			></canvas>{/if}
	</span>
	<span class="text-[11px] font-medium"
		>{caption ? `${caption} ${displayedNumber ?? number}` : number}</span
	>
</button>
