<script lang="ts">
	import type { AnnotationMark } from '$lib/pdf/annotations';

	let { marks, formsOnly }: { marks: AnnotationMark[]; formsOnly: boolean } = $props();

	const percent = (value: number) => `${(value * 100).toFixed(3)}%`;
</script>

<!-- Outlines what will be drawn into the page. Switching to form fields only
fades the rest out. -->
<div aria-hidden="true" class="pointer-events-none absolute inset-0">
	{#each marks as mark, index (index)}
		{@const [left, top, right, bottom] = mark.area}
		<div
			class="absolute rounded-[2px] bg-compress/10 outline-[1.5px] outline-compress/80 motion-safe:transition-opacity motion-safe:duration-200 {formsOnly &&
			!mark.field
				? 'opacity-0'
				: 'opacity-100'}"
			style:left={percent(left)}
			style:top={percent(top)}
			style:width={percent(right - left)}
			style:height={percent(bottom - top)}
		></div>
	{/each}
</div>
