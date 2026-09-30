<script lang="ts">
	import { onMount } from 'svelte';
	import gsap from 'gsap';
	import type { PDFDocumentProxy, PDFPageProxy, RenderTask } from 'pdfjs-dist';
	import { spring } from '$lib/motion/link';

	// A blank page has no `pdf`, only the `size` it will be written at.
	let {
		pdf,
		number,
		rotation,
		size
	}: {
		pdf?: PDFDocumentProxy;
		number: number;
		rotation: number;
		size?: { width: number; height: number };
	} = $props();
	let holder: HTMLDivElement;
	let canvas: HTMLCanvasElement;
	let visible = $state(false);
	let rendered = $state(false);
	// Rotation of the image currently on the canvas, and how far the canvas has
	// been turned past it while the next render is prepared.
	let shownRotation: number | null = null;
	let requestedRotation = 0;
	let pendingTurn = 0;
	let shownSize = { width: 0, height: 0 };

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
		const blank = size;
		const pageNumber = number;
		const turn = rotation;
		let cancelled = false;
		let task: RenderTask | undefined;
		const step = ((((turn - requestedRotation) % 360) + 540) % 360) - 180;
		requestedRotation = turn;
		const spin = shownRotation === null || !step ? Promise.resolve() : turnCanvas(step);
		async function render() {
			try {
				let page: PDFPageProxy | undefined;
				let width: number;
				let height: number;
				if (source) {
					page = await source.getPage(pageNumber);
					if (cancelled) return;
					({ width, height } = page.getViewport({ scale: 1, rotation: page.rotate + turn }));
				} else if (blank) {
					[width, height] = turn % 180 ? [blank.height, blank.width] : [blank.width, blank.height];
				} else return;
				const fit = Math.min(320 / width, 426 / height);
				const viewport = { width: width * fit, height: height * fit };
				const scale = Math.min(window.devicePixelRatio || 1, 2);
				const nextCanvas = document.createElement('canvas');
				nextCanvas.width = Math.max(1, Math.floor(viewport.width * scale));
				nextCanvas.height = Math.max(1, Math.floor(viewport.height * scale));
				if (page) {
					task = page.render({
						canvas: nextCanvas,
						viewport: page.getViewport({ scale: fit, rotation: page.rotate + turn }),
						transform: [scale, 0, 0, scale, 0, 0]
					});
					await task.promise;
				} else {
					const context = nextCanvas.getContext('2d');
					if (context) {
						context.fillStyle = '#fff';
						context.fillRect(0, 0, nextCanvas.width, nextCanvas.height);
					}
				}
				await spin;
				if (!cancelled) {
					// No CSS size: left auto, max-width and max-height scale the canvas
					// by its own aspect ratio instead of squashing each axis separately.
					canvas.width = nextCanvas.width;
					canvas.height = nextCanvas.height;
					shownSize = { width: viewport.width, height: viewport.height };
					canvas.getContext('2d')?.drawImage(nextCanvas, 0, 0);
					gsap.set(canvas, { clearProps: 'transform' });
					shownRotation = turn;
					pendingTurn = 0;
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

	// Turns and resizes the current image in one motion toward the requested
	// rotation, holding it there until the new render lands.
	function turnCanvas(step: number) {
		pendingTurn += step === -180 && pendingTurn > 0 ? 180 : step;
		if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return Promise.resolve();
		const sideways = Math.abs(pendingTurn) % 180 === 90;
		// Aim for the size the next render will be shown at, so the turn and the
		// resize finish together and the swap is invisible.
		let nextWidth = shownSize.width;
		let nextHeight = shownSize.height;
		if (sideways) [nextWidth, nextHeight] = [nextHeight, nextWidth];
		const pixels =
			Math.min(320 / nextWidth, 426 / nextHeight) * Math.min(window.devicePixelRatio || 1, 2);
		const shown =
			pixels *
			Math.min(
				1,
				holder.clientWidth / (nextWidth * pixels),
				holder.clientHeight / (nextHeight * pixels)
			);
		const shownWidth = nextWidth * shown;
		const shownHeight = nextHeight * shown;
		return new Promise<void>((resolve) =>
			gsap.to(canvas, {
				rotation: pendingTurn,
				scaleX: (sideways ? shownHeight : shownWidth) / canvas.offsetWidth,
				scaleY: (sideways ? shownWidth : shownHeight) / canvas.offsetHeight,
				duration: 0.5,
				ease: spring,
				overwrite: true,
				onComplete: resolve
			})
		);
	}
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
