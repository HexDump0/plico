<script lang="ts">
	import { untrack } from 'svelte';
	import gsap from 'gsap';
	import {
		IconArrowAutofitWidth,
		IconArrowsLeftRight,
		IconMinus,
		IconPlus,
		IconUpload,
		IconX
	} from '@tabler/icons-svelte-runes';
	import type { PDFDocumentProxy } from 'pdfjs-dist';
	import type { Change, CompareSide, Comparison } from '$lib/pdf/compare-text';
	import type { CropArea } from '$lib/pdf/types';
	import type { PreviewPage } from '$lib/pdf/stamp-layout';
	import { OpenedPdf } from '$lib/pdf/opened-pdf.svelte';
	import { getWorkspace } from '$lib/workspace.svelte';
	import ComparePane, { GAP, PAD } from './ComparePane.svelte';
	import CompareVisual from './CompareVisual.svelte';
	import PdfUnlock from './PdfUnlock.svelte';

	let {
		original,
		changed,
		comparison,
		current = $bindable(-1),
		mode,
		highlight,
		reducedMotion,
		onload,
		onremove,
		onswap,
		onplace
	}: {
		original?: File;
		changed?: File;
		comparison: Comparison | null;
		/// The change being looked at, from 0, or -1.
		current?: number;
		mode: 'text' | 'visual';
		/// Whether Visual tints the pixels that differ.
		highlight: boolean;
		reducedMotion: boolean;
		onload: (side: CompareSide, file: File, pdf: PDFDocumentProxy) => void;
		onremove: (side: CompareSide) => void;
		onswap: () => void;
		/// Files dropped or chosen for one side.
		onplace: (side: CompareSide, files: FileList) => void;
	} = $props();

	// PageScroller's zoom steps, as a share of print size on a 96 DPI screen.
	const LEVELS = [0.25, 0.33, 0.5, 0.67, 0.75, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2, 2.5, 3, 4];
	const ACTUAL = 96 / 72;
	const FIT_LIMIT = 1100;
	const sides = ['original', 'changed'] as const;
	const labels = { original: 'Original', changed: 'Changed' };

	const workspace = getWorkspace();
	const files = $derived({ original, changed });
	const docs = {
		original: new OpenedPdf(
			() => original,
			(pdf) => original && onload('original', original, pdf)
		),
		changed: new OpenedPdf(
			() => changed,
			(pdf) => changed && onload('changed', changed, pdf)
		)
	};
	let elements = $state<Record<CompareSide, HTMLDivElement | undefined>>({
		original: undefined,
		changed: undefined
	});
	let width = $state(0);
	// Pixels per point, or null to fit both to their column.
	let zoom = $state<number | null>(null);
	let hovered = $state<number | null>(null);
	let dragging = $state<CompareSide | null>(null);
	let turns = $state(0);
	// Phones show one side at a time.
	let phoneSide = $state<CompareSide>('changed');
	let visualPage = $state(1);
	let inputs: Record<CompareSide, HTMLInputElement | undefined> = {
		original: undefined,
		changed: undefined
	};

	const ready = $derived(!!docs.original.pdf && !!docs.changed.pdf);
	const widest = $derived(
		Math.max(1, ...sides.flatMap((side) => docs[side].sizes.map((size) => size.width)))
	);
	let screenWidth = $state(1024);
	// Each column's width less a little room either side; two columns from md.
	const columnWidth = $derived((screenWidth >= 768 ? (width - 12) / 2 : width) - 32);
	const scale = $derived(zoom ?? Math.max(0.1, Math.min(columnWidth, FIT_LIMIT) / widest));
	const percent = $derived(Math.round((scale / ACTUAL) * 100));

	// Where each page starts in its column, laid out as `ComparePane` does.
	const tops = $derived(
		Object.fromEntries(
			sides.map((side) => {
				let y = PAD;
				return [
					side,
					docs[side].sizes.map((size) => {
						const top = y;
						y += size.height * scale + GAP;
						return top;
					})
				];
			})
		) as Record<CompareSide, number[]>
	);

	function position(side: CompareSide, page: number, share: number) {
		const size = docs[side].sizes[page - 1];
		return size ? tops[side][page - 1] + share * size.height * scale : 0;
	}

	function extent(side: CompareSide) {
		const sizes = docs[side].sizes;
		return sizes.length ? position(side, sizes.length, 1) + PAD : 0;
	}

	// Heights in the two columns that show the same words, both rising, from
	// the top of each to the end of each.
	const links = $derived.by(() => {
		if (!ready) return [] as [number, number][];
		const anchors = comparison?.anchors.length
			? comparison.anchors.map(
					(anchor) =>
						[position('original', ...anchor.original), position('changed', ...anchor.changed)] as [
							number,
							number
						]
				)
			: Array.from(
					{ length: Math.min(docs.original.sizes.length, docs.changed.sizes.length) },
					(_, index) =>
						[position('original', index + 1, 0), position('changed', index + 1, 0)] as [
							number,
							number
						]
				);
		const kept: [number, number][] = [[0, 0]];
		for (const link of [...anchors, [extent('original'), extent('changed')] as [number, number]]) {
			const last = kept[kept.length - 1];
			if (link[0] > last[0] && link[1] > last[1]) kept.push(link);
		}
		return kept;
	});

	/// The height in the other column that shows what `y` shows in this one.
	function follow(side: CompareSide, y: number) {
		const [from, to] = side === 'original' ? [0, 1] : [1, 0];
		if (links.length < 2) return y;
		let [low, high] = [0, links.length - 1];
		while (high - low > 1) {
			const middle = (low + high) >> 1;
			if (links[middle][from] <= y) low = middle;
			else high = middle;
		}
		const [a, b] = [links[low], links[high]];
		const share = Math.max(0, Math.min(1, (y - a[from]) / (b[from] - a[from] || 1)));
		return a[to] + share * (b[to] - a[to]);
	}

	// Whichever column the code moved last, so its scroll event is not taken
	// for the reader's and sent back.
	let echo: CompareSide | null = null;
	let gliding = false;

	function scrolled(side: CompareSide) {
		if (gliding) return;
		if (echo === side) {
			echo = null;
			return;
		}
		const other = side === 'original' ? 'changed' : 'original';
		const [from, to] = [elements[side], elements[other]];
		if (!from || !to) return;
		const top = follow(side, from.scrollTop + from.clientHeight / 2) - to.clientHeight / 2;
		const spare = from.scrollWidth - from.clientWidth;
		const left = spare > 0 ? (from.scrollLeft / spare) * (to.scrollWidth - to.clientWidth) : 0;
		const [beforeTop, beforeLeft] = [to.scrollTop, to.scrollLeft];
		to.scrollTop = top;
		to.scrollLeft = left;
		if (to.scrollTop !== beforeTop || to.scrollLeft !== beforeLeft) echo = other;
	}

	function interrupt() {
		if (!gliding) return;
		gsap.killTweensOf(Object.values(elements));
		gliding = false;
		echo = null;
	}

	/// Where a change sits in one column: its first mark, or its caret.
	function spot(change: Change, side: CompareSide): { page: number; box: CropArea } | null {
		const place = change[side][0];
		if (place) return { page: place.page, box: place.boxes[0] };
		if (change.caret?.side === side) return { page: change.caret.page, box: change.caret.box };
		return null;
	}

	// Both columns glide so the change sits a little above the middle.
	function glide(change: Change) {
		const targets = sides.map((side) => {
			const element = elements[side];
			const found = spot(change, side);
			if (!element || !found) return null;
			const size = docs[side].sizes[found.page - 1];
			if (!size) return null;
			const y = position(side, found.page, (found.box[1] + found.box[3]) / 2);
			const content = Math.max(element.clientWidth, size.width * scale);
			const x =
				(content - size.width * scale) / 2 +
				((found.box[0] + found.box[2]) / 2) * size.width * scale;
			return { element, y, x };
		});
		// A side with nothing to show follows the other.
		const [a, b] = targets;
		if (!a && !b) return;
		const resolved = sides.map((side, index) => {
			const target = targets[index];
			if (target) return target;
			const other = targets[1 - index]!;
			const element = elements[side];
			return element
				? { element, y: follow(sides[1 - index], other.y), x: element.clientWidth / 2 }
				: null;
		});
		gsap.killTweensOf(Object.values(elements));
		gliding = true;
		let pending = 0;
		for (const target of resolved) {
			if (!target) continue;
			const { element } = target;
			pending++;
			gsap.to(element, {
				scrollTop: Math.max(
					0,
					Math.min(
						element.scrollHeight - element.clientHeight,
						target.y - element.clientHeight * 0.4
					)
				),
				scrollLeft: Math.max(
					0,
					Math.min(element.scrollWidth - element.clientWidth, target.x - element.clientWidth / 2)
				),
				duration: reducedMotion ? 0 : 0.55,
				ease: 'power3.out',
				onComplete: () => {
					if (--pending === 0) requestAnimationFrame(() => (gliding = false));
				}
			});
		}
		if (pending === 0) gliding = false;
	}

	$effect(() => {
		const index = current;
		const change = comparison?.changes[index];
		if (!change || !ready) return;
		untrack(() => {
			if (mode === 'visual') {
				const place = change.changed[0] ?? (change.caret?.side === 'changed' ? change.caret : null);
				if (place) visualPage = place.page;
			} else glide(change);
		});
	});

	// Visual starts on the changed page in view.
	$effect(() => {
		if (mode !== 'visual') return;
		untrack(() => {
			const element = elements.changed;
			if (!element) return;
			const middle = element.scrollTop + element.clientHeight / 2;
			const index = tops.changed.findLastIndex((top) => top <= middle);
			visualPage = Math.max(1, index + 1);
		});
	});

	async function zoomTo(next: number | null) {
		const element = elements.changed;
		const held =
			element && (element.scrollTop + element.clientHeight / 2) / (element.scrollHeight || 1);
		const [low, high] = [LEVELS[0] * ACTUAL, LEVELS[LEVELS.length - 1] * ACTUAL];
		zoom = next === null ? null : Math.min(high, Math.max(low, next));
		await new Promise(requestAnimationFrame);
		if (element && held !== undefined) {
			element.scrollTop = held * element.scrollHeight - element.clientHeight / 2;
			element.scrollLeft = (element.scrollWidth - element.clientWidth) / 2;
		}
	}

	function step(direction: number) {
		const share = scale / ACTUAL;
		const level =
			direction > 0
				? (LEVELS.find((value) => value > share + 0.005) ?? LEVELS[LEVELS.length - 1])
				: ([...LEVELS].reverse().find((value) => value < share - 0.005) ?? LEVELS[0]);
		void zoomTo(level * ACTUAL);
	}

	function pinch(node: HTMLDivElement) {
		const wheel = (event: WheelEvent) => {
			if (!event.ctrlKey && !event.metaKey) return;
			event.preventDefault();
			void zoomTo(scale * Math.exp(-event.deltaY * 0.0025));
		};
		node.addEventListener('wheel', wheel, { passive: false });
		return { destroy: () => node.removeEventListener('wheel', wheel) };
	}

	const marks = $derived.by(() => {
		const found = {
			original: new Map<number, { id: number; box: CropArea }[]>(),
			changed: new Map<number, { id: number; box: CropArea }[]>()
		};
		const carets = {
			original: new Map<number, { id: number; box: CropArea }[]>(),
			changed: new Map<number, { id: number; box: CropArea }[]>()
		};
		const add = (
			map: Map<number, { id: number; box: CropArea }[]>,
			page: number,
			mark: { id: number; box: CropArea }
		) => {
			const list = map.get(page);
			if (list) list.push(mark);
			else map.set(page, [mark]);
		};
		for (const change of comparison?.changes ?? []) {
			for (const side of sides)
				for (const place of change[side])
					for (const box of place.boxes) add(found[side], place.page, { id: change.id, box });
			if (change.caret)
				add(carets[change.caret.side], change.caret.page, { id: change.id, box: change.caret.box });
		}
		return { found, carets };
	});

	const box = ([left, top, right, bottom]: CropArea) =>
		`left: calc(${left * 100}% - 1px); top: calc(${top * 100}% - 1px); width: calc(${(right - left) * 100}% + 2px); height: calc(${(bottom - top) * 100}% + 2px)`;

	const control =
		'flex size-9 items-center justify-center rounded-lg text-muted transition-colors enabled:hover:bg-panel-hover enabled:hover:text-white disabled:opacity-40';
</script>

{#snippet highlights(side: CompareSide, page: PreviewPage)}
	{@const tone = side === 'original' ? 'convert' : 'merge'}
	<div class="pointer-events-none absolute inset-0">
		{#each marks.found[side].get(page.number) ?? [] as mark, index (index)}
			{@const active = mark.id === current || mark.id === hovered}
			<button
				type="button"
				tabindex="-1"
				aria-label={`Change ${mark.id + 1}`}
				class="pointer-events-auto absolute cursor-pointer rounded-[3px] mix-blend-multiply motion-safe:transition-[background-color,box-shadow] motion-safe:duration-200 {tone ===
				'convert'
					? active
						? 'bg-convert/80'
						: 'bg-convert/40'
					: active
						? 'bg-merge/80'
						: 'bg-merge/45'} {mark.id === current
					? tone === 'convert'
						? 'ring-2 ring-convert'
						: 'ring-2 ring-merge'
					: ''}"
				style={box(mark.box)}
				onclick={() => (current = mark.id)}
				onpointerenter={() => (hovered = mark.id)}
				onpointerleave={() => (hovered = null)}
			></button>
		{/each}
		<!-- Where the other side's words went, or came from. -->
		{#each marks.carets[side].get(page.number) ?? [] as mark, index (index)}
			{@const active = mark.id === current || mark.id === hovered}
			<button
				type="button"
				tabindex="-1"
				aria-label={`Change ${mark.id + 1}`}
				class="pointer-events-auto absolute flex w-3 -translate-x-1/2 cursor-pointer justify-center motion-safe:transition-opacity {active
					? 'opacity-100'
					: 'opacity-60'}"
				style:left="{mark.box[0] * 100}%"
				style:top="{mark.box[1] * 100}%"
				style:height="{(mark.box[3] - mark.box[1]) * 100}%"
				onclick={() => (current = mark.id)}
				onpointerenter={() => (hovered = mark.id)}
				onpointerleave={() => (hovered = null)}
				><span
					class="relative h-full w-0.5 rounded-full {side === 'original'
						? 'bg-merge'
						: 'bg-convert'}"
					><span
						class="absolute -top-1 left-1/2 size-1.5 -translate-x-1/2 rounded-full {side ===
						'original'
							? 'bg-merge'
							: 'bg-convert'}"
					></span></span
				></button
			>
		{/each}
	</div>
{/snippet}

{#snippet originalHighlights(page: PreviewPage)}{@render highlights('original', page)}{/snippet}
{#snippet changedHighlights(page: PreviewPage)}{@render highlights('changed', page)}{/snippet}

{#snippet fileLine(side: CompareSide)}
	{@const file = files[side]}
	<div
		class="flex min-w-0 items-center justify-center gap-2 px-1 {phoneSide === side
			? ''
			: 'max-md:hidden'}"
	>
		<span
			class="shrink-0 rounded-md px-1.5 py-0.5 text-[11px] font-semibold {side === 'original'
				? 'bg-convert/15 text-convert'
				: 'bg-merge/15 text-merge'}">{labels[side]}</span
		>
		{#if file}
			<p class="min-w-0 truncate text-sm font-semibold" title={file.name}>{file.name}</p>
			<span class="shrink-0 text-xs whitespace-nowrap text-muted"
				>{docs[side].pdf
					? `${docs[side].pdf.numPages} ${docs[side].pdf.numPages === 1 ? 'page' : 'pages'}`
					: docs[side].status}</span
			>
			<button
				type="button"
				onclick={() => onremove(side)}
				class="flex size-8 shrink-0 items-center justify-center rounded-lg bg-white/10 text-white backdrop-blur-sm transition-colors hover:bg-convert hover:text-canvas"
				aria-label={`Remove the ${labels[side].toLowerCase()} PDF`}
				title="Remove PDF"><IconX size={18} stroke={2.5} /></button
			>
		{/if}
	</div>
{/snippet}

{#snippet column(side: CompareSide)}
	{@const file = files[side]}
	{@const doc = docs[side]}
	{@const lock = file && workspace.lockState(file)}
	<div
		role="region"
		aria-label={labels[side]}
		class="relative min-h-0 min-w-0 {phoneSide === side ? '' : 'max-md:hidden'}"
		ondragover={(event) => {
			event.preventDefault();
			if (event.dataTransfer?.types.includes('Files')) dragging = side;
		}}
		ondragleave={(event) => {
			if (!event.currentTarget.contains(event.relatedTarget as Node)) dragging = null;
		}}
		ondrop={(event) => {
			dragging = null;
			if (!event.dataTransfer?.files.length) return;
			event.preventDefault();
			onplace(side, event.dataTransfer.files);
		}}
	>
		<input
			bind:this={inputs[side]}
			type="file"
			accept="application/pdf,.pdf"
			class="hidden"
			aria-label={`Choose the ${labels[side].toLowerCase()} PDF`}
			onchange={(event) => {
				const input = event.currentTarget;
				if (input.files?.length) onplace(side, input.files);
				input.value = '';
			}}
		/>
		{#if file && doc.pdf}
			<ComparePane
				pdf={doc.pdf}
				sizes={doc.sizes}
				{scale}
				name={file.name}
				bind:element={elements[side]}
				overlay={side === 'original' ? originalHighlights : changedHighlights}
				onscroll={() => scrolled(side)}
				oninterrupt={interrupt}
			/>
		{:else if file && lock}
			<div class="grid size-full place-items-center p-4">
				<PdfUnlock {lock} name={file.name} onunlock={(value) => workspace.unlock(file, value)} />
			</div>
		{:else if file}
			<p role="status" class="grid size-full place-items-center text-sm text-muted">
				{doc.status}
			</p>
		{:else}
			<div class="size-full p-6">
				<button
					type="button"
					onclick={() => inputs[side]?.click()}
					class="flex size-full flex-col items-center justify-center gap-5 rounded-xl border-2 border-dashed p-4 transition-colors {dragging ===
					side
						? 'border-merge bg-merge/5 text-merge'
						: 'border-muted/50 text-muted hover:border-merge hover:bg-merge/5 hover:text-merge'}"
				>
					<IconUpload size={40} stroke={1.5} /><span class="text-center text-xl font-medium"
						>{dragging === side
							? 'You can let go btw'
							: `Drop in the ${labels[side].toLowerCase()} PDF`}</span
					>
				</button>
			</div>
		{/if}
		{#if file && doc.pdf && dragging === side}
			<div
				class="pointer-events-none absolute inset-6 grid place-items-center rounded-xl border-2 border-dashed border-merge bg-canvas/80 text-xl font-medium text-merge backdrop-blur-sm"
			>
				Replace the {labels[side].toLowerCase()}
			</div>
		{/if}
	</div>
{/snippet}

<svelte:window bind:innerWidth={screenWidth} />

<section
	aria-label="Comparison"
	class="flex h-[75svh] w-full flex-col gap-3 lg:h-[calc(100svh-9rem)]"
>
	<!-- Phones show one side at a time; the pill picks which. -->
	<div
		class="relative mx-auto grid w-full max-w-xs grid-cols-2 gap-2 rounded-xl bg-panel p-1 md:hidden"
		role="group"
		aria-label="Side"
	>
		<span
			aria-hidden="true"
			class="pointer-events-none absolute inset-y-1 left-1 w-[calc((100%-1rem)/2)] rounded-lg bg-panel-hover motion-safe:transition-transform motion-safe:duration-200 motion-safe:ease-[cubic-bezier(0.22,1,0.36,1)] {phoneSide ===
			'changed'
				? 'translate-x-[calc(100%+0.5rem)]'
				: ''}"
		></span>
		{#each sides as side (side)}<button
				type="button"
				aria-pressed={phoneSide === side}
				class="relative z-10 rounded-lg px-2 py-2.5 text-xs font-semibold motion-safe:transition-colors {phoneSide ===
				side
					? side === 'original'
						? 'text-convert'
						: 'text-merge'
					: 'text-muted hover:text-white'}"
				onclick={() => (phoneSide = side)}>{labels[side]}</button
			>{/each}
	</div>

	<div class="grid grid-cols-1 items-center gap-2 md:grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)]">
		{@render fileLine('original')}
		<button
			type="button"
			onclick={() => {
				turns++;
				onswap();
			}}
			disabled={!original || !changed}
			class="mx-auto flex size-8 items-center justify-center rounded-lg text-muted transition-colors enabled:hover:bg-panel-hover enabled:hover:text-white disabled:opacity-40 max-md:hidden"
			aria-label="Swap original and changed"
			title="Swap"
			><IconArrowsLeftRight
				size={18}
				class="motion-safe:transition-transform motion-safe:duration-500 motion-safe:ease-[cubic-bezier(0.22,1,0.36,1)]"
				style="transform: rotate({turns * 180}deg)"
			/></button
		>
		{@render fileLine('changed')}
	</div>

	<div class="relative min-h-0 flex-1">
		{#if mode === 'visual' && ready && docs.original.pdf && docs.changed.pdf}
			<div class="absolute inset-0">
				<CompareVisual
					original={docs.original.pdf}
					changed={docs.changed.pdf}
					pairs={comparison?.pairs ?? []}
					bind:page={visualPage}
					{highlight}
					{reducedMotion}
				/>
			</div>
		{/if}
		<div
			bind:clientWidth={width}
			use:pinch
			class="grid size-full grid-cols-1 gap-3 md:grid-cols-2 {mode === 'visual' && ready
				? 'invisible'
				: ''}"
		>
			{@render column('original')}
			{@render column('changed')}
		</div>

		{#if ready && mode === 'text'}
			<div
				class="absolute bottom-3 left-1/2 z-10 flex -translate-x-1/2 items-center gap-1 rounded-xl bg-panel/90 p-1 shadow-xl ring-1 shadow-black/40 ring-white/10 backdrop-blur-md"
				role="group"
				aria-label="Zoom"
			>
				<button
					type="button"
					class={control}
					onclick={() => step(-1)}
					disabled={scale <= LEVELS[0] * ACTUAL + 1e-3}
					aria-label="Zoom out"
					title="Zoom out"><IconMinus size={18} /></button
				>
				<span class="w-12 text-center text-xs font-semibold text-white tabular-nums"
					>{percent}%</span
				>
				<button
					type="button"
					class={control}
					onclick={() => step(1)}
					disabled={scale >= LEVELS[LEVELS.length - 1] * ACTUAL - 1e-3}
					aria-label="Zoom in"
					title="Zoom in"><IconPlus size={18} /></button
				>
				<button
					type="button"
					class="{control} {zoom === null ? 'text-merge' : ''}"
					onclick={() => void zoomTo(null)}
					aria-pressed={zoom === null}
					aria-label="Fit width"
					title="Fit width"><IconArrowAutofitWidth size={18} /></button
				>
			</div>
		{/if}
	</div>
</section>
