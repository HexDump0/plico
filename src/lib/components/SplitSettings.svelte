<script lang="ts">
	import { IconPlus, IconX } from '@tabler/icons-svelte-runes';
	import type { SplitRange } from '$lib/split-ranges';
	import { rangeCollapse, rangeReveal } from '$lib/motion/range';
	let {
		pageCount,
		ranges = $bindable<SplitRange[]>(),
		mode = $bindable<'ranges' | 'fixed'>(),
		interval = $bindable<number>(),
		reducedMotion
	}: {
		pageCount: number;
		ranges: SplitRange[];
		mode: 'ranges' | 'fixed';
		interval: number;
		reducedMotion: boolean;
	} = $props();
	let nextId = 1;
	let combine = $state(false);
	const modes = [
		{ id: 'ranges', label: 'Page ranges' },
		{ id: 'fixed', label: 'Every N pages' }
	] as const;
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
			class="relative grid grid-cols-2 gap-2 rounded-xl bg-canvas p-1"
			role="group"
			aria-label="Split mode"
		>
			<span
				aria-hidden="true"
				class="pointer-events-none absolute inset-y-1 left-1 w-[calc((100%-1rem)/2)] rounded-lg bg-panel-hover motion-safe:transition-transform motion-safe:duration-200 motion-safe:ease-[cubic-bezier(0.22,1,0.36,1)] {mode ===
				'fixed'
					? 'translate-x-[calc(100%+0.5rem)]'
					: ''}"
			></span>
			{#each modes as option (option.id)}<button
					aria-pressed={mode === option.id}
					class="relative z-10 rounded-lg px-2 py-3 text-xs font-semibold motion-safe:transition-colors {mode ===
					option.id
						? 'text-split'
						: 'text-muted hover:text-white'}"
					onclick={() => (mode = option.id)}>{option.label}</button
				>{/each}
		</div>
	</div>
	{#if mode === 'ranges'}
		<div class="space-y-2">
			{#each ranges as range, index (range.id)}<div
					role="group"
					aria-label={`Range ${index + 1}`}
					in:rangeReveal={{ reducedMotion }}
					out:rangeCollapse={{ reducedMotion }}
					class="grid items-center gap-2 {ranges.length > 1
						? 'grid-cols-[2rem_minmax(0,1fr)_minmax(0,1fr)_2rem]'
						: 'grid-cols-[2rem_minmax(0,1fr)_minmax(0,1fr)]'}"
				>
					<span
						class="flex size-8 items-center justify-center rounded-lg bg-split/10 text-xs font-bold text-split"
						aria-hidden="true">{index + 1}</span
					>
					<label
						class="flex h-11 min-w-0 items-center gap-1 rounded-lg border border-white/10 bg-canvas px-2 text-xs text-muted focus-within:border-split/50"
						>From<input
							class="page-number w-full min-w-0 bg-transparent text-right text-sm text-white outline-none"
							type="number"
							min="1"
							max={pageCount || undefined}
							bind:value={range.from}
						/></label
					><label
						class="flex h-11 min-w-0 items-center gap-1 rounded-lg border border-white/10 bg-canvas px-2 text-xs text-muted focus-within:border-split/50"
						>To<input
							class="page-number w-full min-w-0 bg-transparent text-right text-sm text-white outline-none"
							type="number"
							min={range.from}
							max={pageCount || undefined}
							bind:value={range.to}
						/></label
					>
					{#if ranges.length > 1}<button
							type="button"
							class="flex size-8 items-center justify-center rounded-lg bg-canvas/80 text-white transition-colors hover:bg-convert hover:text-canvas"
							aria-label={`Remove range ${index + 1}`}
							onclick={() => (ranges = ranges.filter((item) => item.id !== range.id))}
							><IconX size={18} stroke={2.5} /></button
						>{/if}
				</div>{/each}
		</div>
		<button
			class="flex w-full items-center justify-center gap-2 rounded-xl border border-split/30 py-3 text-sm text-split hover:bg-split/5 motion-safe:transition-colors motion-safe:duration-150"
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
				class="page-number mt-3 w-full rounded-xl border border-white/10 bg-canvas p-3 text-white"
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

<style>
	.page-number {
		appearance: textfield;
		-moz-appearance: textfield;
	}
	.page-number::-webkit-inner-spin-button,
	.page-number::-webkit-outer-spin-button {
		-webkit-appearance: none;
		margin: 0;
	}
</style>
