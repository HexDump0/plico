<script lang="ts">
	import gsap from 'gsap';
	import { untrack } from 'svelte';
	import { cubicIn, cubicOut } from 'svelte/easing';
	import { fade, scale } from 'svelte/transition';
	import {
		IconArrowLeft,
		IconBorderCorners,
		IconChevronLeft,
		IconChevronRight,
		IconLoader2,
		IconMaximize,
		IconRotate,
		IconRotateClockwise
	} from '@tabler/icons-svelte-runes';
	import { pendingTurn, type ScanPhoto } from '$lib/pdf/scan.svelte';

	let {
		file,
		photo,
		index,
		count,
		reducedMotion,
		disabled = false,
		onchange,
		onturn,
		onfind,
		onwhole,
		onstep,
		onclose
	}: {
		file: File;
		photo: ScanPhoto;
		index: number;
		count: number;
		reducedMotion: boolean;
		disabled?: boolean;
		onchange: (corners: number[]) => void;
		onturn: (quarter: number) => void;
		/// Back to the page as it was found.
		onfind: () => void;
		onwhole: () => void;
		onstep: (offset: number) => void;
		onclose: () => void;
	} = $props();

	const names = ['Top left', 'Top right', 'Bottom right', 'Bottom left'];
	let stage = $state<HTMLDivElement>();
	let stageWidth = $state(0);
	let stageHeight = $state(0);
	// The corners on screen, which glide to new ones unless dragged.
	let shown = $state<number[]>(untrack(() => [...photo.corners]));
	let drag = $state<{ handle: number; edge: boolean; x: number; y: number } | null>(null);
	let glide: gsap.core.Tween | undefined;
	const ratio = $derived(photo.width && photo.height ? photo.width / photo.height : 3 / 4);
	const found = $derived(!!photo.found && same(photo.corners, photo.found));
	const whole = $derived(same(photo.corners, [0, 0, 1, 0, 1, 1, 0, 1]));
	const turn = $derived(pendingTurn(photo, 3 / 4));

	function same(a: number[], b: number[]) {
		return a.every((value, at) => Math.abs(value - b[at]) < 1e-4);
	}

	$effect(() => {
		const target = [...photo.corners];
		untrack(() => {
			glide?.kill();
			if (drag || reducedMotion || same(shown, target)) {
				shown = target;
				return;
			}
			const from = [...shown];
			const progress = { t: 0 };
			glide = gsap.to(progress, {
				t: 1,
				duration: 0.32,
				ease: 'power3.out',
				onUpdate: () => {
					shown = from.map((value, at) => value + (target[at] - value) * progress.t);
				}
			});
		});
	});
	$effect(() => () => glide?.kill());

	const points = $derived(
		[0, 1, 2, 3].map((corner) => ({ x: shown[corner * 2], y: shown[corner * 2 + 1] }))
	);
	const outline = $derived(points.map((point) => `${point.x},${point.y}`).join(' '));

	function position(event: PointerEvent) {
		const bounds = stage!.getBoundingClientRect();
		return {
			x: Math.min(1, Math.max(0, (event.clientX - bounds.left) / bounds.width)),
			y: Math.min(1, Math.max(0, (event.clientY - bounds.top) / bounds.height))
		};
	}

	function begin(event: PointerEvent, handle: number, edge: boolean) {
		if (disabled || (event.pointerType === 'mouse' && event.button !== 0)) return;
		event.preventDefault();
		(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
		glide?.kill();
		shown = [...photo.corners];
		const at = position(event);
		drag = { handle, edge, ...at };
	}

	function move(event: PointerEvent) {
		if (!drag || !stage) return;
		const at = position(event);
		const next = [...shown];
		if (drag.edge) {
			// An edge moves both its corners by the same amount, kept inside the photo.
			const corners = [drag.handle, (drag.handle + 1) % 4];
			let dx = at.x - drag.x;
			let dy = at.y - drag.y;
			for (const corner of corners) {
				dx = Math.min(1 - next[corner * 2], Math.max(-next[corner * 2], dx));
				dy = Math.min(1 - next[corner * 2 + 1], Math.max(-next[corner * 2 + 1], dy));
			}
			for (const corner of corners) {
				next[corner * 2] += dx;
				next[corner * 2 + 1] += dy;
			}
			drag = { ...drag, x: drag.x + dx, y: drag.y + dy };
		} else {
			next[drag.handle * 2] = at.x;
			next[drag.handle * 2 + 1] = at.y;
			drag = { ...drag, ...at };
		}
		shown = next;
	}

	function end() {
		if (!drag) return;
		drag = null;
		if (!same(shown, photo.corners)) onchange([...shown]);
	}

	function nudge(event: KeyboardEvent, corner: number) {
		const step = event.shiftKey ? 0.025 : 0.0025;
		const offsets: Record<string, [number, number]> = {
			ArrowLeft: [-step, 0],
			ArrowRight: [step, 0],
			ArrowUp: [0, -step],
			ArrowDown: [0, step]
		};
		const offset = offsets[event.key];
		if (!offset || disabled) return;
		event.preventDefault();
		const next = [...photo.corners];
		next[corner * 2] = Math.min(1, Math.max(0, next[corner * 2] + offset[0]));
		next[corner * 2 + 1] = Math.min(1, Math.max(0, next[corner * 2 + 1] + offset[1]));
		onchange(next);
	}

	// The magnifier sits up and away from the finger, toward the photo's middle.
	const loupe = $derived.by(() => {
		if (!drag || drag.edge || !photo.url) return null;
		const zoom = 2.5;
		const size = 112;
		const x = drag.x * stageWidth;
		const y = drag.y * stageHeight;
		const left = drag.x > 0.5 ? x - size - 28 : x + 28;
		const top = Math.max(-size / 2, y - size - 28);
		return {
			left,
			top,
			size,
			background: `${-x * zoom + size / 2}px ${-y * zoom + size / 2}px / ${stageWidth * zoom}px ${stageHeight * zoom}px`
		};
	});

	const button =
		'flex size-10 shrink-0 items-center justify-center rounded-xl border-2 border-white/10 bg-panel text-muted transition-colors enabled:hover:border-convert/40 enabled:hover:text-convert disabled:opacity-40';
	const pill =
		'flex items-center gap-1.5 rounded-xl border-2 border-white/10 bg-panel px-3 py-2 text-xs font-semibold text-muted transition-colors enabled:hover:border-convert/40 enabled:hover:text-convert disabled:opacity-40';
</script>

<svelte:window
	onkeydown={(event) => {
		if (event.key === 'Escape' && !drag && !event.defaultPrevented) onclose();
	}}
/>

<section aria-label="Page corners" class="mx-auto w-full max-w-5xl space-y-6">
	<div class="flex flex-wrap items-center gap-3">
		<button type="button" class={pill} onclick={onclose}
			><IconArrowLeft size={16} />All pages</button
		>
		<p class="min-w-0 flex-1 truncate text-sm font-semibold" title={file.name}>{file.name}</p>
		<div class="flex items-center gap-2" role="group" aria-label="Page">
			<button
				type="button"
				class={button}
				aria-label="Turn left"
				title="Turn left"
				{disabled}
				onclick={() => onturn(-1)}><IconRotate size={18} /></button
			>
			<button
				type="button"
				class={button}
				aria-label="Turn right"
				title="Turn right"
				{disabled}
				onclick={() => onturn(1)}><IconRotateClockwise size={18} /></button
			>
			{#if photo.found}
				<button type="button" class={pill} disabled={disabled || found} onclick={onfind}
					><IconBorderCorners size={16} />Detected</button
				>
			{/if}
			<button type="button" class={pill} disabled={disabled || whole} onclick={onwhole}
				><IconMaximize size={16} />Whole photo</button
			>
		</div>
	</div>

	<div class="grid items-center gap-6 md:grid-cols-[minmax(0,3fr)_minmax(0,2fr)]">
		<div
			bind:this={stage}
			bind:clientWidth={stageWidth}
			bind:clientHeight={stageHeight}
			class="relative mx-auto touch-none select-none"
			style:aspect-ratio={ratio}
			style:width="min(100%, calc((100svh - 18rem) * {ratio}))"
		>
			{#if photo.url}
				<img
					src={photo.url}
					alt=""
					draggable="false"
					in:fade={{ duration: reducedMotion ? 0 : 200 }}
					class="absolute inset-0 size-full rounded-sm shadow-2xl shadow-black/40"
				/>
				<svg
					viewBox="0 0 1 1"
					preserveAspectRatio="none"
					class="pointer-events-none absolute inset-0 size-full overflow-visible"
					aria-hidden="true"
				>
					<path
						d="M0 0H1V1H0Z M{outline.replaceAll(' ', 'L')}Z"
						fill="rgb(11 11 13 / 0.6)"
						fill-rule="evenodd"
					/>
					<polygon
						points={outline}
						fill="none"
						class="stroke-convert"
						stroke-width="2"
						vector-effect="non-scaling-stroke"
						stroke-linejoin="round"
					/>
				</svg>
				{#each points as point, side (side)}
					{@const next = points[(side + 1) % 4]}
					<!-- An edge's middle moves both its corners. -->
					<div
						role="presentation"
						class="absolute size-9 -translate-1/2 cursor-move"
						style:left="{((point.x + next.x) / 2) * 100}%"
						style:top="{((point.y + next.y) / 2) * 100}%"
						onpointerdown={(event) => begin(event, side, true)}
						onpointermove={move}
						onpointerup={end}
						onpointercancel={end}
					>
						<span
							class="absolute top-1/2 left-1/2 h-1.5 w-5 -translate-1/2 rounded-full bg-convert shadow-[0_0_0_1px_rgb(11_11_13/0.35)]"
							style:rotate="{Math.atan2(
								(next.y - point.y) * stageHeight,
								(next.x - point.x) * stageWidth
							)}rad"
						></span>
					</div>
				{/each}
				{#each points as point, corner (corner)}
					<button
						type="button"
						aria-label="{names[corner]} corner"
						{disabled}
						class="group absolute flex size-11 -translate-1/2 cursor-grab items-center justify-center focus-visible:outline-none active:cursor-grabbing"
						style:left="{point.x * 100}%"
						style:top="{point.y * 100}%"
						onpointerdown={(event) => begin(event, corner, false)}
						onpointermove={move}
						onpointerup={end}
						onpointercancel={end}
						onkeydown={(event) => nudge(event, corner)}
					>
						<span
							class="size-4 rounded-full border-2 border-convert bg-canvas/70 shadow-[0_0_0_1px_rgb(11_11_13/0.35)] group-focus-visible:shadow-[0_0_0_3px_white] motion-safe:transition-transform motion-safe:duration-150 {drag?.handle ===
								corner && !drag.edge
								? 'scale-125 bg-convert'
								: 'group-hover:scale-110'}"
						></span>
					</button>
				{/each}
				{#if loupe}
					<div
						aria-hidden="true"
						transition:scale={{ duration: reducedMotion ? 0 : 140, start: 0.85, easing: cubicOut }}
						class="pointer-events-none absolute z-20 overflow-hidden rounded-full border-2 border-convert bg-canvas shadow-2xl shadow-black/60"
						style:left="{loupe.left}px"
						style:top="{loupe.top}px"
						style:width="{loupe.size}px"
						style:height="{loupe.size}px"
						style:background="url({photo.url}) {loupe.background} no-repeat"
					>
						<span class="absolute top-1/2 left-1/2 h-px w-5 -translate-1/2 bg-convert"></span>
						<span class="absolute top-1/2 left-1/2 h-5 w-px -translate-1/2 bg-convert"></span>
					</div>
				{/if}
			{:else}
				<div class="absolute inset-0 grid place-items-center rounded-sm bg-panel text-muted">
					<IconLoader2 class="animate-spin" size={24} />
				</div>
			{/if}
		</div>

		<div
			class="relative mx-auto grid aspect-[3/4] w-full max-w-72 place-items-center md:max-w-none"
		>
			{#if photo.preview?.url}
				{#key photo.preview.url}
					<img
						src={photo.preview.url}
						alt="Page {index + 1} as it will be saved"
						in:fade={{ duration: reducedMotion ? 0 : 220, easing: cubicOut }}
						out:fade={{ duration: reducedMotion ? 0 : 220, easing: cubicIn }}
						class="col-start-1 row-start-1 max-h-full max-w-full rounded-sm bg-white shadow-2xl shadow-black/40 motion-safe:transition-transform motion-safe:duration-300 motion-safe:ease-[cubic-bezier(0.22,1,0.36,1)]"
						style:transform="rotate({turn.degrees}deg) scale({turn.scale})"
					/>
				{/key}
			{:else}
				<div class="col-start-1 row-start-1 grid size-full place-items-center rounded-sm bg-panel">
					<IconLoader2 class="animate-spin text-muted" size={24} />
				</div>
			{/if}
		</div>
	</div>

	<div class="flex items-center justify-center gap-3" role="group" aria-label="Pages">
		<button
			type="button"
			class={button}
			aria-label="Previous page"
			disabled={index === 0}
			onclick={() => onstep(-1)}><IconChevronLeft size={18} /></button
		>
		<span class="min-w-24 text-center text-sm text-muted tabular-nums"
			><span class="font-semibold text-white">{index + 1}</span> / {count}</span
		>
		<button
			type="button"
			class={button}
			aria-label="Next page"
			disabled={index === count - 1}
			onclick={() => onstep(1)}><IconChevronRight size={18} /></button
		>
	</div>
</section>
