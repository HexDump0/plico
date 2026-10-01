<script lang="ts">
	import PageRangeField from './PageRangeField.svelte';

	let {
		format,
		quality = $bindable<number>(),
		pageRange = $bindable<string>(),
		pageCount = 0
	}: {
		format: 'jpg' | 'png';
		quality: number;
		pageRange: string;
		pageCount?: number;
	} = $props();
</script>

{#if format === 'jpg'}
	<div class="space-y-2">
		<div class="flex items-center justify-between text-xs font-medium text-muted">
			<span>JPG quality</span>
			<span class="font-mono text-white">{quality}%</span>
		</div>
		<input
			type="range"
			min="30"
			max="100"
			step="5"
			bind:value={quality}
			aria-label="JPG image quality"
			class="w-full cursor-pointer accent-split"
		/>
	</div>
{/if}

<PageRangeField bind:value={pageRange} {pageCount} />
