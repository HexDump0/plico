<script lang="ts">
	import { fontStack } from '$lib/pdf/font-faces.svelte';
	import { needsEmbedding } from '$lib/pdf/unicode-fonts';
	import {
		arrowHead,
		extent,
		hexColor,
		inkPath,
		markupStroke,
		NOTE_SIZE,
		noteLines,
		notePolygon,
		textLines
	} from '$lib/pdf/annotate';
	import type { Drawn } from '$lib/pdf/annotate';
	import type { PreviewPage, StampImage } from '$lib/pdf/stamp-layout';
	import type { CropArea } from '$lib/pdf/types';

	let {
		page,
		mark,
		image,
		hideText = false
	}: {
		page: PreviewPage;
		mark: Drawn & { color: number; opacity: number };
		/// The picture an image annotation draws.
		image?: StampImage;
		/// Leaves a text box's text out while it is typed in place.
		hideText?: boolean;
	} = $props();
	const id = $props.id();

	const families = {
		helvetica: "Helvetica, Arial, 'Liberation Sans', sans-serif",
		times: "'Times New Roman', Times, 'Liberation Serif', serif",
		courier: "'Courier New', Courier, 'Liberation Mono', monospace"
	};
	const color = $derived(hexColor(mark.color));
	const [width, height] = $derived([page.width, page.height]);
	const point = ([x, y]: [number, number]): [number, number] => [x * width, y * height];
	const box = ([left, top, right, bottom]: CropArea): CropArea => [
		left * width,
		top * height,
		right * width,
		bottom * height
	];
</script>

<!-- Drawn in points on the page as displayed, like the engine draws the
appearance, so widths and sizes match. Opacity applies to the whole
annotation, and highlights multiply, as in the output. -->
<svg
	aria-hidden="true"
	class="pointer-events-none absolute inset-0 size-full overflow-visible"
	viewBox="0 0 {width} {height}"
	preserveAspectRatio="none"
	style:opacity={mark.opacity}
	style:mix-blend-mode={mark.kind === 'highlight' ? 'multiply' : undefined}
>
	{#if mark.kind === 'highlight'}
		{#each mark.boxes.map(box) as [left, top, right, bottom], index (index)}
			<rect x={left} y={top} width={right - left} height={bottom - top} fill={color} />
		{/each}
	{:else if mark.kind === 'underline' || mark.kind === 'strikeout' || mark.kind === 'squiggly'}
		{@const kind = mark.kind}
		{#each mark.boxes.map((found) => markupStroke(kind, box(found))) as line, index (index)}
			<path
				d={line.path}
				fill="none"
				stroke={color}
				stroke-width={line.thickness}
				stroke-linejoin="round"
			/>
		{/each}
	{:else if mark.kind === 'ink'}
		{#each mark.strokes as stroke, index (index)}
			<path
				d={inkPath(stroke.map(point))}
				fill="none"
				stroke={color}
				stroke-width={mark.width}
				stroke-linecap="round"
				stroke-linejoin="round"
			/>
		{/each}
	{:else if mark.kind === 'rectangle' || mark.kind === 'ellipse'}
		{@const [left, top, right, bottom] = box(mark.area)}
		{@const inset = Math.min(mark.width / 2, (right - left) / 2, (bottom - top) / 2)}
		{@const fill = mark.fill === null ? 'none' : hexColor(mark.fill)}
		{#if mark.kind === 'rectangle'}
			<rect
				x={left + inset}
				y={top + inset}
				width={right - left - 2 * inset}
				height={bottom - top - 2 * inset}
				{fill}
				stroke={color}
				stroke-width={mark.width}
			/>
		{:else}
			<ellipse
				cx={(left + right) / 2}
				cy={(top + bottom) / 2}
				rx={(right - left) / 2 - inset}
				ry={(bottom - top) / 2 - inset}
				{fill}
				stroke={color}
				stroke-width={mark.width}
			/>
		{/if}
	{:else if mark.kind === 'line' || mark.kind === 'arrow'}
		{@const [from, to] = [point(mark.from), point(mark.to)]}
		<path
			d="M{from.join(' ')}L{to.join(' ')}{mark.kind === 'arrow'
				? arrowHead(from, to, mark.width)
						.map((end) => `M${to.join(' ')}L${end.join(' ')}`)
						.join('')
				: ''}"
			fill="none"
			stroke={color}
			stroke-width={mark.width}
			stroke-linecap="round"
			stroke-linejoin="round"
		/>
	{:else if mark.kind === 'text'}
		{@const [left, top, right, bottom] = box(mark.area)}
		<clipPath id="{id}-clip">
			<rect x={left} y={top} width={right - left} height={bottom - top} />
		</clipPath>
		{#if mark.fill !== null}
			<rect
				x={left}
				y={top}
				width={right - left}
				height={bottom - top}
				fill={hexColor(mark.fill)}
			/>
		{/if}
		{#if !hideText}
			<g clip-path="url(#{id}-clip)">
				{#each textLines(mark, page) as line, index (index)}
					<text
						x={line.x}
						y={line.y}
						fill={color}
						font-family={needsEmbedding(mark.text)
							? fontStack(mark.text, mark.family)
							: families[mark.family]}
						font-weight={mark.bold ? 700 : 400}
						font-size={mark.size}
						textLength={line.width > 0 ? line.width : undefined}
						lengthAdjust="spacingAndGlyphs"
						style:white-space="pre">{line.text}</text
					>
				{/each}
			</g>
		{/if}
	{:else if mark.kind === 'note'}
		{@const [left, top] = box(extent(mark, page))}
		<g transform="translate({left} {top}) scale({NOTE_SIZE / 20})">
			<polygon
				points={notePolygon}
				fill={color}
				stroke="#333333"
				stroke-width="1"
				stroke-linejoin="round"
			/>
			{#each noteLines as [x1, y1, x2, y2], index (index)}
				<line {x1} {y1} {x2} {y2} stroke="#333333" stroke-width="1" />
			{/each}
		</g>
	{:else if mark.kind === 'picture'}
		{#if !mark.deleted}
			{@const [left, top, right, bottom] = box(mark.area)}
			<image
				href={mark.snapshot}
				x={left}
				y={top}
				width={right - left}
				height={bottom - top}
				preserveAspectRatio="none"
			/>
		{/if}
	{:else if mark.kind === 'erase'}
		{@const [left, top, right, bottom] = box(mark.area)}
		<rect x={left} y={top} width={right - left} height={bottom - top} fill={color} />
	{:else if mark.kind === 'image' && image}
		{@const [left, top, right, bottom] = box(extent(mark, page, image.aspect))}
		<image
			href={image.url}
			x={left}
			y={top}
			width={right - left}
			height={bottom - top}
			preserveAspectRatio="none"
		/>
	{/if}
</svg>
