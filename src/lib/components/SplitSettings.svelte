<script lang="ts">
	import { IconPlus, IconX } from '@tabler/icons-svelte-runes';
	let { pageCount }: { pageCount: number } = $props();
	let mode = $state('ranges');
	let ranges = $state([{ id: 0, from: 1, to: 1 }]);
	let nextId = 1;
	let interval = $state(1);
	let combine = $state(false);
	const invalid = $derived(
		ranges.some(
			(range) =>
				!Number.isInteger(range.from) ||
				!Number.isInteger(range.to) ||
				range.from < 1 ||
				range.to < range.from ||
				(pageCount > 0 && range.to > pageCount)
		)
	);
</script>

<div class="space-y-6">
	<div>
		<h2 class="mb-3 text-sm font-semibold">Split by</h2>
		<div
			class="grid grid-cols-2 gap-2 rounded-xl bg-canvas p-1"
			role="group"
			aria-label="Split mode"
		>
			{#each [{ id: 'ranges', label: 'Page ranges' }, { id: 'fixed', label: 'Every N pages' }] as option (option.id)}<button
					aria-pressed={mode === option.id}
					class="rounded-lg px-2 py-3 text-xs font-semibold {mode === option.id
						? 'bg-panel-hover text-split'
						: 'text-muted hover:text-white'}"
					onclick={() => (mode = option.id)}>{option.label}</button
				>{/each}
		</div>
	</div>
	{#if mode === 'ranges'}
		<div class="space-y-3">
			{#each ranges as range, index (range.id)}<div class="rounded-xl border border-white/10 p-3">
					<div class="mb-3 flex items-center justify-between">
						<span class="text-xs text-muted">Range {index + 1}</span><button
							disabled={ranges.length === 1}
							class="rounded p-1 text-muted hover:text-white disabled:opacity-25"
							aria-label={`Remove range ${index + 1}`}
							onclick={() => (ranges = ranges.filter((item) => item.id !== range.id))}
							><IconX size={16} /></button
						>
					</div>
					<div class="grid grid-cols-2 gap-3">
						<label class="text-xs text-muted"
							>From<input
								class="mt-2 w-full rounded-lg border border-white/10 bg-canvas p-3 text-sm text-white"
								type="number"
								min="1"
								max={pageCount || undefined}
								bind:value={range.from}
							/></label
						><label class="text-xs text-muted"
							>To<input
								class="mt-2 w-full rounded-lg border border-white/10 bg-canvas p-3 text-sm text-white"
								type="number"
								min={range.from}
								max={pageCount || undefined}
								bind:value={range.to}
							/></label
						>
					</div>
				</div>{/each}
		</div>
		<button
			class="flex w-full items-center justify-center gap-2 rounded-xl border border-split/30 py-3 text-sm text-split hover:bg-split/5"
			onclick={() => (ranges = [...ranges, { id: nextId++, from: 1, to: pageCount || 1 }])}
			><IconPlus size={18} />Add range</button
		>
		{#if invalid}<p role="alert" class="text-xs text-convert">
				Enter valid page ranges between 1 and {pageCount || 'the last page'}.
			</p>{/if}
		<label class="flex items-start gap-3 text-sm leading-relaxed text-muted"
			><input type="checkbox" class="mt-1 size-4 accent-brand" bind:checked={combine} />Combine
			ranges into one PDF</label
		>
	{:else}
		<label class="block text-sm text-muted"
			>Pages per PDF<input
				class="mt-3 w-full rounded-xl border border-white/10 bg-canvas p-3 text-white"
				type="number"
				min="1"
				max={pageCount || undefined}
				bind:value={interval}
			/></label
		>
		<p class="text-xs leading-relaxed text-muted">
			{pageCount && Number.isInteger(interval) && interval > 0
				? `Creates ${Math.ceil(pageCount / interval)} PDFs. The last file may have fewer pages.`
				: 'Choose how many pages each new PDF should contain.'}
		</p>
	{/if}
</div>
