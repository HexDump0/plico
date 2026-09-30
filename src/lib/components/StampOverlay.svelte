<script lang="ts">
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
</script>

<svg
	viewBox="0 0 {width} {height}"
	aria-hidden="true"
	class="pointer-events-none absolute inset-0 size-full overflow-hidden"
	style:opacity
>
	{#each layout.centers as [x, y], index (index)}
		<g
			style:transform="translate({x}px, {height - y}px) rotate({-placement.rotation}deg)"
			style:transition={reducedMotion ? 'none' : 'transform 320ms cubic-bezier(0.22, 1, 0.36, 1)'}
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
</svg>
