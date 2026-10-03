<script lang="ts">
	import { untrack } from 'svelte';
	import type { PDFDocumentProxy, RenderTask } from 'pdfjs-dist';

	let {
		pdf,
		number,
		scale,
		near,
		label,
		forms
	}: {
		pdf: PDFDocumentProxy;
		number: number;
		/// CSS pixels per point.
		scale: number;
		/// Whether the page is on screen or close to it; only those keep pixels.
		near: boolean;
		label: string;
		/// Renders form fields apart from the page, as pdf.js leaves them for
		/// HTML, and hands over the canvases it draws some of them on, by
		/// annotation id: check boxes and radio buttons get one per state.
		forms?: (canvases: Map<string, HTMLCanvasElement | HTMLCanvasElement[]>) => void;
	} = $props();

	// The largest canvas drawn, in pixels, so a page zoomed far in stays
	// within what browsers will allocate.
	const MAX_PIXELS = 16_000_000;

	let canvas = $state<HTMLCanvasElement>();
	let drawn = $state(false);

	// Drawn off screen and copied in once done, so a zoom keeps showing the
	// old pixels, stretched, until the sharp ones are ready.
	$effect(() => {
		const source = pdf;
		const zoom = scale;
		const target = canvas;
		if (!near || !target) return;
		let cancelled = false;
		let task: RenderTask | undefined;
		const delay = window.setTimeout(
			async () => {
				try {
					const proxy = await source.getPage(number);
					if (cancelled) return;
					const viewport = proxy.getViewport({ scale: zoom });
					const ratio = Math.min(
						window.devicePixelRatio || 1,
						2,
						Math.sqrt(MAX_PIXELS / (viewport.width * viewport.height))
					);
					const bitmap = document.createElement('canvas');
					bitmap.width = Math.max(1, Math.floor(viewport.width * ratio));
					bitmap.height = Math.max(1, Math.floor(viewport.height * ratio));
					// pdf.js puts an array of canvases, one per state, under a
					// check box or radio button, whatever its types say.
					const canvases = forms ? new Map<string, HTMLCanvasElement>() : undefined;
					const { AnnotationMode } = await import('pdfjs-dist');
					if (cancelled) return;
					task = proxy.render({
						canvas: bitmap,
						viewport,
						transform: [ratio, 0, 0, ratio, 0, 0],
						...(canvases && {
							annotationMode: AnnotationMode.ENABLE_FORMS,
							annotationCanvasMap: canvases
						})
					});
					await task.promise;
					if (cancelled) return;
					target.width = bitmap.width;
					target.height = bitmap.height;
					target.getContext('2d')?.drawImage(bitmap, 0, 0);
					drawn = true;
					if (canvases) forms?.(canvases as Map<string, HTMLCanvasElement | HTMLCanvasElement[]>);
				} catch {
					// A cancelled render is replaced by the next one.
				}
			},
			untrack(() => drawn) ? 150 : 0
		);
		return () => {
			cancelled = true;
			clearTimeout(delay);
			task?.cancel();
		};
	});

	// Far from the screen, a page lets go of its pixels.
	$effect(() => {
		if (near || !canvas) return;
		canvas.width = 0;
		canvas.height = 0;
		drawn = false;
	});
</script>

<canvas
	bind:this={canvas}
	aria-label={label}
	class="absolute inset-0 size-full motion-safe:transition-opacity motion-safe:duration-200 {drawn
		? 'opacity-100'
		: 'opacity-0'}"
></canvas>
