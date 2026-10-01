<script lang="ts">
	let { value = $bindable<string>(), pageCount = 0 }: { value: string; pageCount?: number } =
		$props();
	const id = $props.id();
</script>

<div class="space-y-1.5">
	<label for={id} class="block text-xs font-medium text-muted">
		Page range
		{#if pageCount > 0}
			<span class="text-[11px] text-muted/70">({pageCount} pages total)</span>
		{/if}
	</label>
	<div
		class="flex items-center rounded-xl border border-white/10 bg-canvas px-3 focus-within:border-split/50"
	>
		<input
			{id}
			type="text"
			bind:value
			placeholder="All pages (e.g. 1-3, 5)"
			class="min-w-0 flex-1 bg-transparent py-2.5 text-xs text-white outline-none placeholder:text-white/25"
		/>
	</div>
</div>
