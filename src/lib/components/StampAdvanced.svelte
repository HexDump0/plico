<script lang="ts">
	let {
		margin = $bindable<number>(),
		marginLabel = 'Margin',
		behind = $bindable<boolean | undefined>(undefined),
		pages = $bindable<string | undefined>(undefined),
		pageCount = 0,
		pagesInvalid = false,
		disabled = false
	}: {
		margin: number;
		marginLabel?: string;
		behind?: boolean;
		pages?: string;
		pageCount?: number;
		pagesInvalid?: boolean;
		disabled?: boolean;
	} = $props();
	const id = $props.id();

	const margins = [
		{ value: 18, label: 'Narrow', hint: '0.25 in' },
		{ value: 36, label: 'Normal', hint: '0.5 in' },
		{ value: 72, label: 'Wide', hint: '1 in' }
	] as const;
	const layers = [
		{ behind: false, label: 'Over content' },
		{ behind: true, label: 'Behind content' }
	] as const;
</script>

<div class="space-y-6">
	{#if pages !== undefined}
		<div>
			<div class="mb-3 flex items-center justify-between">
				<label for="{id}-pages" class="text-sm font-semibold">Pages</label>
				{#if pageCount}<span class="text-xs text-muted">of {pageCount}</span>{/if}
			</div>
			<div
				class="flex items-center rounded-xl border bg-canvas px-3 transition-colors duration-200 {pagesInvalid
					? 'border-convert/60'
					: 'border-white/10 focus-within:border-brand/50'}"
			>
				<input
					id="{id}-pages"
					type="text"
					inputmode="numeric"
					autocomplete="off"
					bind:value={pages}
					{disabled}
					placeholder="All pages (e.g. 1-3, 5)"
					aria-invalid={pagesInvalid}
					class="min-w-0 flex-1 bg-transparent py-3 text-sm text-white outline-none placeholder:text-white/35 disabled:opacity-50"
				/>
			</div>
			{#if pagesInvalid}<p role="alert" class="pt-2 text-xs text-convert">
					Enter pages between 1 and {pageCount}.
				</p>{/if}
		</div>
	{/if}

	{#if behind !== undefined}
		<div>
			<h2 class="mb-3 text-sm font-semibold">Layer</h2>
			<div class="grid grid-cols-2 gap-1 rounded-xl bg-canvas p-1" role="group" aria-label="Layer">
				{#each layers as option (option.label)}<button
						type="button"
						aria-pressed={behind === option.behind}
						{disabled}
						class="rounded-lg px-2 py-3 text-xs font-semibold motion-safe:transition-colors {behind ===
						option.behind
							? 'bg-panel-hover text-brand'
							: 'text-muted hover:text-white'}"
						onclick={() => (behind = option.behind)}>{option.label}</button
					>{/each}
			</div>
		</div>
	{/if}

	<div>
		<div class="mb-3 flex items-center justify-between">
			<h2 class="text-sm font-semibold">{marginLabel}</h2>
			<span class="text-xs text-muted"
				>{margins.find((option) => option.value === margin)?.hint ?? `${margin} pt`}</span
			>
		</div>
		<div
			class="grid grid-cols-3 gap-1 rounded-xl bg-canvas p-1"
			role="group"
			aria-label={marginLabel}
		>
			{#each margins as option (option.value)}<button
					type="button"
					aria-pressed={margin === option.value}
					{disabled}
					class="rounded-lg px-2 py-3 text-xs font-semibold motion-safe:transition-colors {margin ===
					option.value
						? 'bg-panel-hover text-brand'
						: 'text-muted hover:text-white'}"
					onclick={() => (margin = option.value)}>{option.label}</button
				>{/each}
		</div>
	</div>
</div>
