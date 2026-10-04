<script lang="ts">
	import { untrack } from 'svelte';
	import { cubicOut } from 'svelte/easing';
	import {
		IconChevronLeft,
		IconChevronRight,
		IconArrowsHorizontal
	} from '@tabler/icons-svelte-runes';
	import type { PDFDocumentProxy, RenderTask } from 'pdfjs-dist';

	let {
		original,
		changed,
		pairs,
		page = $bindable(1),
		highlight,
		reducedMotion
	}: {
		original: PDFDocumentProxy;
		changed: PDFDocumentProxy;
		/// For each changed page, from 0, the original page shown under it.
		pairs: number[];
		/// The changed page shown, from 1.
		page?: number;
		highlight: boolean;
		reducedMotion: boolean;
	} = $props();

	// How far apart two pixels' channels must be to count as a difference;
	// pdf.js draws the same content to the same pixels, so this only has to
	// clear antialiasing noise.
	const THRESHOLD = 40;

	let available = $state(0);
	let room = $state(0);
	// Where the divider sits, as a share of the page's width: the original
	// shows to its left, the changed page to its right.
	let split = $state(0.5);
	let size = $state.raw<{ width: number; height: number } | null>(null);
	let shown = $state.raw<{
		page: number;
		under: number;
		width: number;
		height: number;
		before: HTMLCanvasElement;
		after: HTMLCanvasElement;
		mask: HTMLCanvasElement;
		direction: number;
	} | null>(null);
	let travel = $state(0);
	let dragging = $state(false);
	let frameElement = $state<HTMLDivElement>();

	const pageCount = $derived(changed.numPages);
	const under = $derived(Math.min(original.numPages, pairs[page - 1] ?? page));
	let last = untrack(() => page);
	$effect.pre(() => {
		const now = page;
		if (now !== last) {
			travel = Math.sign(now - last);
			last = now;
		}
	});

	function fit(width: number, height: number) {
		const fitted = Math.max(1, Math.min(available, 760, (Math.max(240, room) * width) / height));
		return { width: fitted, height: (fitted * height) / width };
	}
	const frame = $derived(
		shown ? fit(shown.width, shown.height) : size ? fit(size.width, size.height) : null
	);

	$effect(() => {
		const number = Math.min(page, pageCount);
		let cancelled = false;
		void changed.getPage(number).then((proxy) => {
			if (cancelled) return;
			const { width, height } = proxy.getViewport({ scale: 1 });
			size = { width, height };
		});
		return () => {
			cancelled = true;
		};
	});

	async function draw(
		source: PDFDocumentProxy,
		number: number,
		width: number,
		tasks: RenderTask[]
	) {
		const proxy = await source.getPage(number);
		const viewport = proxy.getViewport({ scale: width / proxy.getViewport({ scale: 1 }).width });
		const bitmap = document.createElement('canvas');
		bitmap.width = Math.max(1, Math.floor(viewport.width));
		bitmap.height = Math.max(1, Math.floor(viewport.height));
		const task = proxy.render({ canvas: bitmap, viewport });
		tasks.push(task);
		await task.promise;
		return bitmap;
	}

	/// Every few pixels that differ between the two, grown by a cell so a
	/// changed letter reads as a mark rather than specks.
	function difference(before: HTMLCanvasElement, after: HTMLCanvasElement, cell: number) {
		const [width, height] = [after.width, after.height];
		const read = (source: HTMLCanvasElement) => {
			const canvas = document.createElement('canvas');
			canvas.width = width;
			canvas.height = height;
			const context = canvas.getContext('2d', { willReadFrequently: true })!;
			context.fillStyle = '#fff';
			context.fillRect(0, 0, width, height);
			context.drawImage(source, 0, 0);
			return context.getImageData(0, 0, width, height).data;
		};
		const [a, b] = [read(before), read(after)];
		const columns = Math.ceil(width / cell);
		const rows = Math.ceil(height / cell);
		const cells = new Uint8Array(columns * rows);
		for (let y = 0; y < height; y++) {
			const row = Math.floor(y / cell) * columns;
			for (let x = 0; x < width; x++) {
				const at = (y * width + x) * 4;
				if (
					Math.abs(a[at] - b[at]) > THRESHOLD ||
					Math.abs(a[at + 1] - b[at + 1]) > THRESHOLD ||
					Math.abs(a[at + 2] - b[at + 2]) > THRESHOLD
				)
					cells[row + Math.floor(x / cell)] = 1;
			}
		}
		const mask = document.createElement('canvas');
		mask.width = width;
		mask.height = height;
		const context = mask.getContext('2d')!;
		context.fillStyle = '#ff94ae';
		for (let row = 0; row < rows; row++)
			for (let column = 0; column < columns; column++) {
				let near = false;
				for (let dy = -1; dy <= 1 && !near; dy++)
					for (let dx = -1; dx <= 1 && !near; dx++) {
						const [r, c] = [row + dy, column + dx];
						near = r >= 0 && r < rows && c >= 0 && c < columns && cells[r * columns + c] === 1;
					}
				if (near) context.fillRect(column * cell, row * cell, cell, cell);
			}
		return mask;
	}

	$effect(() => {
		const number = Math.min(page, pageCount);
		const below = under;
		const width = size ? Math.round(fit(size.width, size.height).width) : 0;
		if (!size || width <= 1) return;
		const { width: pageWidth, height: pageHeight } = size;
		let cancelled = false;
		const tasks: RenderTask[] = [];
		const resizing = untrack(() => shown?.page === number);
		const delay = window.setTimeout(
			async () => {
				try {
					const ratio = Math.min(window.devicePixelRatio || 1, 2);
					const pixels = Math.floor(width * ratio);
					const [before, after] = await Promise.all([
						draw(original, below, pixels, tasks),
						draw(changed, number, pixels, tasks)
					]);
					if (cancelled) return;
					const mask = difference(before, after, Math.max(2, Math.round(3 * ratio)));
					const previous = untrack(() => shown);
					shown = {
						page: number,
						under: below,
						width: pageWidth,
						height: pageHeight,
						before,
						after,
						mask,
						direction: previous ? Math.sign(number - previous.page) : 0
					};
				} catch {
					// A cancelled render is replaced by the next one.
				}
			},
			resizing ? 120 : 0
		);
		return () => {
			cancelled = true;
			clearTimeout(delay);
			for (const task of tasks) task.cancel();
		};
	});

	function paint(canvas: HTMLCanvasElement, bitmap: HTMLCanvasElement) {
		const copy = (source: HTMLCanvasElement) => {
			canvas.width = source.width;
			canvas.height = source.height;
			canvas.getContext('2d')?.drawImage(source, 0, 0);
		};
		copy(bitmap);
		return { update: copy };
	}

	// StampPreview's page turn: the new page drifts a few pixels in from the
	// side the reader is heading while the old one fades.
	function turn(node: Element, { direction, entering }: { direction: number; entering: boolean }) {
		if (reducedMotion) return { duration: 0 };
		if (!entering)
			return {
				duration: 160,
				easing: cubicOut,
				css: (t: number, u: number) =>
					`transform: translateX(${-direction * u * 8}px); opacity: ${t}`
			};
		return {
			duration: 240,
			easing: cubicOut,
			css: (t: number, u: number) => `transform: translateX(${direction * u * 12}px); opacity: ${t}`
		};
	}

	function roll(node: Element, { entering }: { entering: boolean }) {
		if (reducedMotion || travel === 0) return { duration: 0 };
		const distance = (entering ? travel : -travel) * 6;
		return {
			duration: entering ? 200 : 140,
			easing: cubicOut,
			css: (t: number, u: number) => `transform: translateY(${u * distance}px); opacity: ${t}`
		};
	}

	function go(offset: number) {
		const next = Math.max(1, Math.min(pageCount, page + offset));
		if (next !== page) page = next;
	}

	function slide(event: PointerEvent) {
		const box = frameElement?.getBoundingClientRect();
		if (!box) return;
		split = Math.max(0, Math.min(1, (event.clientX - box.left) / box.width));
	}

	function key(event: KeyboardEvent) {
		const step = event.shiftKey ? 0.1 : 0.02;
		const next =
			event.key === 'ArrowLeft'
				? split - step
				: event.key === 'ArrowRight'
					? split + step
					: event.key === 'Home'
						? 0
						: event.key === 'End'
							? 1
							: null;
		if (next === null) return;
		event.preventDefault();
		split = Math.max(0, Math.min(1, next));
	}

	const badge =
		'pointer-events-none absolute top-3 z-20 flex items-center gap-1.5 rounded-md bg-canvas/80 px-2 py-1 text-[11px] font-semibold text-white backdrop-blur-sm motion-safe:transition-opacity motion-safe:duration-200';
</script>

<div class="flex size-full flex-col items-center gap-5">
	<div
		bind:clientWidth={available}
		bind:clientHeight={room}
		class="grid min-h-0 w-full flex-1 place-items-center"
	>
		{#if frame}
			<div
				bind:this={frameElement}
				role="presentation"
				class="relative touch-none select-none {dragging ? 'cursor-grabbing' : 'cursor-ew-resize'}"
				style:width="{frame.width}px"
				style:height="{frame.height}px"
				style:transition={reducedMotion || !shown
					? 'none'
					: 'width 320ms cubic-bezier(0.22, 1, 0.36, 1), height 320ms cubic-bezier(0.22, 1, 0.36, 1)'}
				onpointerdown={(event) => {
					if (event.button !== 0) return;
					frameElement?.setPointerCapture(event.pointerId);
					dragging = true;
					slide(event);
				}}
				onpointermove={(event) => dragging && slide(event)}
				onpointerup={() => (dragging = false)}
				onpointercancel={() => (dragging = false)}
			>
				{#if shown}
					{#key shown.page}
						{@const layer = shown}
						<div
							class="absolute inset-0 isolate overflow-hidden rounded-sm bg-white shadow-2xl shadow-black/40"
							in:turn={{ direction: layer.direction, entering: true }}
							out:turn={{ direction: layer.direction, entering: false }}
						>
							<canvas
								use:paint={layer.before}
								aria-label={`Page ${layer.under} of the original`}
								class="absolute inset-x-0 top-0 w-full"
							></canvas>
							<canvas
								use:paint={layer.after}
								aria-label={`Page ${layer.page} of the changed PDF`}
								class="absolute inset-0 size-full bg-white"
								style:clip-path="inset(0 0 0 {split * 100}%)"
							></canvas>
							<canvas
								use:paint={layer.mask}
								aria-hidden="true"
								class="pointer-events-none absolute inset-0 size-full mix-blend-multiply motion-safe:transition-opacity motion-safe:duration-200 {highlight
									? 'opacity-60'
									: 'opacity-0'}"
							></canvas>
						</div>
					{/key}
					<span class="{badge} left-3 {split < 0.12 ? 'opacity-0' : ''}"
						><span class="size-1.5 rounded-full bg-convert"
						></span>Original{#if shown.under !== shown.page}<span class="font-normal text-muted"
								>· page {shown.under}</span
							>{/if}</span
					>
					<span class="{badge} right-3 {split > 0.88 ? 'opacity-0' : ''}"
						><span class="size-1.5 rounded-full bg-merge"></span>Changed</span
					>
					<div
						class="pointer-events-none absolute inset-y-0 z-10 w-0.5 -translate-x-1/2 bg-white shadow-[0_0_0_1px_rgb(0_0_0/0.15)]"
						style:left="{split * 100}%"
					>
						<span
							role="slider"
							tabindex="0"
							aria-label="Divider between the original and the changed page"
							aria-valuemin={0}
							aria-valuemax={100}
							aria-valuenow={Math.round(split * 100)}
							aria-orientation="horizontal"
							onkeydown={key}
							class="pointer-events-auto absolute top-1/2 left-1/2 flex size-9 -translate-1/2 items-center justify-center rounded-full bg-white text-canvas shadow-lg shadow-black/30 motion-safe:transition-transform {dragging
								? 'scale-110'
								: ''}"><IconArrowsHorizontal size={18} stroke={2} /></span
						>
					</div>
				{/if}
			</div>
		{/if}
	</div>

	{#if pageCount > 1}
		<div class="flex items-center justify-center gap-3" role="group" aria-label="Page">
			<button
				type="button"
				onclick={() => go(-1)}
				disabled={page <= 1}
				aria-label="Previous page"
				class="flex size-10 shrink-0 items-center justify-center rounded-xl border-2 border-white/10 bg-panel text-muted transition-colors enabled:hover:border-merge/40 enabled:hover:text-merge disabled:opacity-40"
				><IconChevronLeft size={18} /></button
			>
			<p
				class="flex min-w-24 items-center justify-center text-sm text-muted tabular-nums"
				aria-live="polite"
			>
				<span class="inline-grid justify-items-end font-semibold text-white">
					{#key page}<span
							class="col-start-1 row-start-1"
							in:roll={{ entering: true }}
							out:roll={{ entering: false }}>{page}</span
						>{/key}
				</span>
				<span class="ml-1">/ {pageCount}</span>
			</p>
			<button
				type="button"
				onclick={() => go(1)}
				disabled={page >= pageCount}
				aria-label="Next page"
				class="flex size-10 shrink-0 items-center justify-center rounded-xl border-2 border-white/10 bg-panel text-muted transition-colors enabled:hover:border-merge/40 enabled:hover:text-merge disabled:opacity-40"
				><IconChevronRight size={18} /></button
			>
		</div>
	{/if}
</div>
