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
		disabled = false,
		onselect
	}: {
		pdf: PDFDocumentProxy;
		number: number;
		variant: 'range' | 'picker';
		caption: string;
		active: boolean;
		selected: boolean;
		disabled?: boolean;
		onselect: () => void;
	} = $props();
	let button: HTMLButtonElement;
	let canvas: HTMLCanvasElement;
	let visible = $state(false);
	let rendered = $state(false);
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
		let cancelled = false;
		let task: RenderTask | undefined;
		async function render() {
			try {
				const page = await source.getPage(pageNumber);
				if (cancelled) return;
				const base = page.getViewport({ scale: 1 });
				const viewport = page.getViewport({
					scale: Math.min(width / base.width, height / base.height)
				});
				const outputScale = Math.min(window.devicePixelRatio || 1, 2);
				canvas.width = Math.floor(viewport.width * outputScale);
				canvas.height = Math.floor(viewport.height * outputScale);
				canvas.style.width = `${viewport.width}px`;
				canvas.style.height = 'auto';
				task = page.render({ canvas, viewport, transform: [outputScale, 0, 0, outputScale, 0, 0] });
				await task.promise;
				if (!cancelled) rendered = true;
			} catch {
				if (!cancelled) rendered = false;
			}
		}
		void render();
		return () => {
			cancelled = true;
			task?.cancel();
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
	class="group flex shrink-0 flex-col items-center gap-2 rounded-lg p-1 text-center transition-[color,transform] duration-200 {large
		? 'w-[clamp(6rem,31vw,13rem)] md:w-60 xl:w-64'
		: 'w-24'} {active ? 'text-split' : selected ? 'text-split/80' : 'text-muted'} {disabled
		? 'cursor-default'
		: 'hover:text-split motion-safe:hover:-translate-y-0.5'}"
>
	<span
		class="flex w-full items-center justify-center overflow-hidden rounded-sm bg-white shadow-md shadow-black/25 ring-offset-2 ring-offset-panel transition-[box-shadow] duration-200 {large
			? 'aspect-[3/4]'
			: 'aspect-[2/3]'} {active
			? 'ring-2 ring-split'
			: 'group-hover:ring-1 group-hover:ring-split/50'}"
	>
		<canvas
			bind:this={canvas}
			class="max-h-full max-w-full object-contain {rendered ? '' : 'hidden'}"
		></canvas>
	</span>
	<span class="text-[11px] font-medium">{caption ? `${caption} ${number}` : number}</span>
</button>
