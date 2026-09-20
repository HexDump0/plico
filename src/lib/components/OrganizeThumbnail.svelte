<script lang="ts">
	import { onMount } from 'svelte';
	import type { PDFDocumentProxy, RenderTask } from 'pdfjs-dist';

	let { pdf, number, rotation }: { pdf: PDFDocumentProxy; number: number; rotation: number } =
		$props();
	let holder: HTMLDivElement;
	let canvas: HTMLCanvasElement;
	let visible = $state(false);
	let rendered = $state(false);

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
		observer.observe(holder);
		return () => observer.disconnect();
	});

	$effect(() => {
		if (!visible) return;
		const source = pdf;
		const pageNumber = number;
		const turn = rotation;
		let cancelled = false;
		let task: RenderTask | undefined;
		async function render() {
			try {
				const page = await source.getPage(pageNumber);
				if (cancelled) return;
				const base = page.getViewport({ scale: 1, rotation: page.rotate + turn });
				const viewport = page.getViewport({
					scale: Math.min(320 / base.width, 426 / base.height),
					rotation: page.rotate + turn
				});
				const scale = Math.min(window.devicePixelRatio || 1, 2);
				const nextCanvas = document.createElement('canvas');
				nextCanvas.width = Math.max(1, Math.floor(viewport.width * scale));
				nextCanvas.height = Math.max(1, Math.floor(viewport.height * scale));
				task = page.render({
					canvas: nextCanvas,
					viewport,
					transform: [scale, 0, 0, scale, 0, 0]
				});
				await task.promise;
				if (!cancelled) {
					canvas.width = nextCanvas.width;
					canvas.height = nextCanvas.height;
					canvas.style.width = `${viewport.width}px`;
					canvas.style.height = `${viewport.height}px`;
					canvas.getContext('2d')?.drawImage(nextCanvas, 0, 0);
					rendered = true;
				}
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

<div
	bind:this={holder}
	class="flex aspect-3/4 w-full items-center justify-center overflow-hidden bg-white"
>
	<canvas
		bind:this={canvas}
		class="max-h-full max-w-full shadow-md shadow-black/20 {rendered ? '' : 'invisible'}"
	></canvas>
</div>
