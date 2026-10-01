<script lang="ts">
	import { untrack } from 'svelte';
	import { cubicInOut, cubicOut } from 'svelte/easing';
	import {
		stampLayout,
		type StampLayout,
		type StampMark,
		type StampPlacement
	} from '$lib/pdf/stamp-layout';

	let {
		width,
		height,
		mark,
		placement,
		color,
		opacity,
		imageUrl = '',
		reducedMotion,
		onlayout
	}: {
		width: number;
		height: number;
		mark: StampMark;
		placement: StampPlacement;
		color: string;
		opacity: number;
		imageUrl?: string;
		reducedMotion: boolean;
		onlayout?: (layout: StampLayout) => void;
	} = $props();

	// Metric-compatible with the standard PDF fonts the engine names, and
	// every line is stretched to the engine's exact width, so a missing font
	// shifts glyph shapes but never placement.
	const families = {
		helvetica: "Helvetica, Arial, 'Liberation Sans', 'Nimbus Sans', sans-serif",
		times: "'Times New Roman', Times, 'Liberation Serif', 'Nimbus Roman', serif",
		courier: "'Courier New', Courier, 'Liberation Mono', 'Nimbus Mono PS', monospace"
	};

	const layout = $derived(stampLayout(mark, width, height, placement));
	$effect(() => onlayout?.(layout));

	// Turning repeat on sends copies out of the single mark to their places,
	// and turning it off gathers them back in. Tiles that come and go while
	// sizes change just appear.
	const anchor = $derived(
		stampLayout(mark, width, height, { ...placement, tile: false }).centers[0] ?? [0, 0]
	);
	let switching = false;
	let previousTile = untrack(() => placement.tile);
	$effect.pre(() => {
		if (placement.tile === previousTile) return;
		previousTile = placement.tile;
		switching = true;
		requestAnimationFrame(() => (switching = false));
	});

	// The stagger lives inside one curve, so every copy sits on the original
	// until its own start rather than showing at its final place first.
	function travel(node: SVGGElement, [x, y]: [number, number], leaving: boolean) {
		if (!switching || reducedMotion || node.dataset.tiled !== 'true') return { duration: 0 };
		const [fromX, fromY] = [anchor[0], height - anchor[1]];
		const [toX, toY] = [x, height - y];
		const start = leaving
			? 0
			: (0.2 * Math.hypot(toX - fromX, toY - fromY)) / Math.hypot(width, height);
		const ease = leaving ? cubicInOut : cubicOut;
		return {
			duration: leaving ? 380 : 640,
			css: (t: number) => {
				const progress = ease(Math.max(0, (t - start) / (1 - start)));
				const opacity = leaving ? `; opacity: ${Math.min(1, progress * 4)}` : '';
				return `transform: translate(${fromX + (toX - fromX) * progress}px, ${fromY + (toY - fromY) * progress}px) rotate(${-placement.rotation}deg)${opacity}`;
			}
		};
	}
	const spread = (node: SVGGElement, center: [number, number]) => travel(node, center, false);
	const gather = (node: SVGGElement, center: [number, number]) => travel(node, center, true);
</script>

<svg
	viewBox="0 0 {width} {height}"
	aria-hidden="true"
	class="pointer-events-none absolute inset-0 size-full overflow-hidden"
	style:opacity
>
	{#key placement.tile}
		{#each layout.centers as [x, y], index (index)}
			<g
				data-tiled={placement.tile}
				style:transform="translate({x}px, {height - y}px) rotate({-placement.rotation}deg)"
				style:transition={reducedMotion ? 'none' : 'transform 320ms cubic-bezier(0.22, 1, 0.36, 1)'}
				in:spread|global={[x, y]}
				out:gather|global={[x, y]}
			>
				{#if mark.kind === 'text'}
					{#each layout.lines as line, lineIndex (lineIndex)}
						<text
							x={line.x}
							y={-line.y}
							font-family={families[mark.family]}
							font-weight={mark.bold ? 700 : 400}
							font-size={mark.size}
							fill={color}
							textLength={line.width > 0 ? line.width : undefined}
							lengthAdjust="spacingAndGlyphs"
							style:white-space="pre">{line.text}</text
						>
					{/each}
				{:else if imageUrl}
					<image
						href={imageUrl}
						x={-layout.width / 2}
						y={-layout.height / 2}
						width={layout.width}
						height={layout.height}
						preserveAspectRatio="none"
					/>
				{/if}
			</g>
		{/each}
	{/key}
</svg>
