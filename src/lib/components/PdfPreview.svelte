<script lang="ts">
	import { IconFileTypePdf } from '@tabler/icons-svelte-runes';
	let {
		file,
		pageNumber = 1,
		width = 360,
		onload = () => {}
	}: {
		file: File;
		pageNumber?: number;
		width?: number;
		onload?: (count: number) => void;
	} = $props();
	let canvas = $state<HTMLCanvasElement>();
	let status = $state('Loading preview...');
	let imageUrl = $state('');
	$effect(() => {
		const source = file;
		const number = pageNumber;
		let cancelled = false;

		if (source.type.startsWith('image/') || /\.(jpe?g|png)$/i.test(source.name)) {
			const url = URL.createObjectURL(source);
			imageUrl = url;
			status = '';
			onload(1);
			return () => {
				URL.revokeObjectURL(url);
			};
		}

		imageUrl = '';
		let task: import('pdfjs-dist').PDFDocumentLoadingTask | undefined;
		status = 'Loading preview...';
		async function render() {
			try {
				const pdfjs = await import('pdfjs-dist');
				const worker = await import('pdfjs-dist/build/pdf.worker.min.mjs?url');
				if (cancelled) return;
				pdfjs.GlobalWorkerOptions.workerSrc = worker.default;
				const data = new Uint8Array(await source.arrayBuffer());
				task = pdfjs.getDocument({
					data,
					cMapUrl: '/pdfjs/cmaps/',
					cMapPacked: true,
					standardFontDataUrl: '/pdfjs/standard_fonts/'
				});
				task.onPassword = () => {
					if (!cancelled) status = 'Password-protected PDF';
					void task?.destroy();
				};
				const pdf = await task.promise;
				if (cancelled) return;
				onload(pdf.numPages);
				const page = await pdf.getPage(Math.min(number, pdf.numPages));
				if (cancelled || !canvas) return;
				const base = page.getViewport({ scale: 1 });
				// Render at `width` CSS pixels scaled for the display's pixel
				// density, so the preview stays sharp on retina screens instead
				// of the browser upscaling a fixed 360 px canvas.
				const viewport = page.getViewport({ scale: width / base.width });
				const outputScale = Math.min(window.devicePixelRatio || 1, 2);
				canvas.width = Math.max(1, Math.floor(viewport.width * outputScale));
				canvas.height = Math.max(1, Math.floor(viewport.height * outputScale));
				await page.render({
					canvas,
					viewport,
					transform: [outputScale, 0, 0, outputScale, 0, 0]
				}).promise;
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
	class="relative flex aspect-[3/4] w-full items-center justify-center overflow-hidden {status
		? 'bg-canvas/60 p-4'
		: 'bg-white'}"
>
	{#if imageUrl}
		<img
			src={imageUrl}
			alt={`Preview of ${file.name}`}
			draggable="false"
			class="max-h-full max-w-full object-contain shadow-lg select-none"
		/>
	{:else}
		<canvas
			bind:this={canvas}
			aria-label={`Page ${pageNumber} of ${file.name}`}
			class="max-h-full max-w-full object-contain shadow-lg {status ? 'hidden' : ''}"
		></canvas>
	{/if}
	{#if status}<div class="flex flex-col items-center gap-3 text-center text-muted">
			<IconFileTypePdf size={40} stroke={1.2} /><span class="text-xs">{status}</span>
		</div>{/if}
</div>
