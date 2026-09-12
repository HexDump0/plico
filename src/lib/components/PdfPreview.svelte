<script lang="ts">
	import { IconFileTypePdf } from '@tabler/icons-svelte-runes';
	let {
		file,
		pageNumber = 1,
		onload = () => {}
	}: { file: File; pageNumber?: number; onload?: (count: number) => void } = $props();
	let canvas: HTMLCanvasElement;
	let status = $state('Loading preview…');
	$effect(() => {
		const source = file;
		const number = pageNumber;
		let cancelled = false;
		let task: import('pdfjs-dist').PDFDocumentLoadingTask | undefined;
		status = 'Loading preview…';
		async function render() {
			try {
				const pdfjs = await import('pdfjs-dist');
				const worker = await import('pdfjs-dist/build/pdf.worker.min.mjs?url');
				if (cancelled) return;
				pdfjs.GlobalWorkerOptions.workerSrc = worker.default;
				const data = new Uint8Array(await source.arrayBuffer());
				if (cancelled) return;
				task = pdfjs.getDocument({ data });
				task.onPassword = () => {
					if (!cancelled) status = 'Password-protected PDF';
					void task?.destroy();
				};
				const pdf = await task.promise;
				if (cancelled) return;
				onload(pdf.numPages);
				const page = await pdf.getPage(Math.min(number, pdf.numPages));
				if (cancelled) return;
				const viewport = page.getViewport({ scale: 360 / page.getViewport({ scale: 1 }).width });
				canvas.width = viewport.width;
				canvas.height = viewport.height;
				await page.render({ canvas, viewport }).promise;
				if (!cancelled) status = '';
			} catch {
				if (!cancelled && status !== 'Password-protected PDF') status = 'Preview unavailable';
			}
		}
		void render();
		return () => {
			cancelled = true;
			void task?.destroy();
		};
	});
</script>

<div
	class="relative flex aspect-[3/4] w-full items-center justify-center overflow-hidden rounded-lg bg-canvas/60 p-4"
>
	<canvas
		bind:this={canvas}
		aria-label={`Page ${pageNumber} of ${file.name}`}
		class="max-h-full max-w-full object-contain shadow-lg {status ? 'hidden' : ''}"
	></canvas>
	{#if status}<div class="flex flex-col items-center gap-3 text-center text-muted">
			<IconFileTypePdf size={40} stroke={1.2} /><span class="text-xs">{status}</span>
		</div>{/if}
</div>
