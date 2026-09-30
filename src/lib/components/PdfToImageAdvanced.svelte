<script lang="ts">
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

<div class="space-y-1.5">
	<label for="page-range-input" class="block text-xs font-medium text-muted">
		Page range
		{#if pageCount > 0}
			<span class="text-[11px] text-muted/70">({pageCount} pages total)</span>
		{/if}
	</label>
	<div
		class="flex items-center rounded-xl border border-white/10 bg-canvas px-3 focus-within:border-split/50"
	>
		<input
			id="page-range-input"
			type="text"
			bind:value={pageRange}
			placeholder="All pages (e.g. 1-3, 5)"
			aria-label="Page range"
			class="min-w-0 flex-1 bg-transparent py-2.5 text-xs text-white outline-none placeholder:text-white/25"
		/>
	</div>
</div>
