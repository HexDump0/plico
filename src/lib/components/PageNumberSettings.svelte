<script lang="ts">
	import { positionNames, type StampPosition } from '$lib/pdf/stamp-layout';
	import PositionPicker from './PositionPicker.svelte';

	let {
		position = $bindable<StampPosition>(),
		template = $bindable<string>(),
		firstNumber = $bindable<number>(),
		pages = $bindable<string>(),
		lastNumber,
		pageCount,
		pagesInvalid,
		disabled = false
	}: {
		position: StampPosition;
		template: string;
		firstNumber: number;
		pages: string;
		lastNumber: number;
		pageCount: number;
		pagesInvalid: boolean;
		disabled?: boolean;
	} = $props();
	const id = $props.id();

	const templates = ['{n}', 'Page {n}', '{n} of {total}', 'Page {n} of {total}'];
	const firstInvalid = $derived(
		!(Number.isInteger(firstNumber) && firstNumber >= 0 && firstNumber <= 99999)
	);
	const first = $derived(firstInvalid ? 1 : firstNumber);
</script>

<div class="space-y-6">
	<div>
		<div class="mb-3 flex items-center justify-between">
			<h2 class="text-sm font-semibold">Position</h2>
			<span class="text-xs text-muted">{positionNames[position]}</span>
		</div>
		<PositionPicker bind:value={position} positions={[0, 1, 2, 6, 7, 8]} {disabled} />
	</div>

	<div>
		<h2 class="mb-3 text-sm font-semibold">Format</h2>
		<div class="grid grid-cols-2 gap-1 rounded-xl bg-canvas p-1" role="group" aria-label="Format">
			{#each templates as option (option)}<button
					type="button"
					aria-pressed={template === option}
					{disabled}
					class="truncate rounded-lg px-2 py-3 text-xs font-semibold tabular-nums motion-safe:transition-colors {template ===
					option
						? 'bg-panel-hover text-brand'
						: 'text-muted hover:text-white'}"
					onclick={() => (template = option)}
					>{option
						.replace('{n}', String(first))
						.replace('{total}', String(Math.max(first, lastNumber)))}</button
				>{/each}
		</div>
	</div>

	<div class="grid grid-cols-[minmax(0,1fr)_6.5rem] gap-3">
		<div>
			<div class="mb-3 flex items-center justify-between gap-2">
				<label for="{id}-pages" class="text-sm font-semibold">Pages</label>
				{#if pageCount}<span class="truncate text-xs text-muted">of {pageCount}</span>{/if}
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
					placeholder="All"
					aria-invalid={pagesInvalid}
					class="min-w-0 flex-1 bg-transparent py-3 text-sm text-white outline-none placeholder:text-white/35 disabled:opacity-50"
				/>
			</div>
		</div>
		<div>
			<label for="{id}-first" class="mb-3 block text-sm font-semibold">Start at</label>
			<div
				class="flex items-center rounded-xl border bg-canvas px-3 transition-colors duration-200 {firstInvalid
					? 'border-convert/60'
					: 'border-white/10 focus-within:border-brand/50'}"
			>
				<input
					id="{id}-first"
					type="number"
					inputmode="numeric"
					min="0"
					max="99999"
					step="1"
					bind:value={firstNumber}
					{disabled}
					aria-invalid={firstInvalid}
					class="min-w-0 flex-1 bg-transparent py-3 text-sm text-white tabular-nums outline-none disabled:opacity-50"
				/>
			</div>
		</div>
	</div>
	{#if pagesInvalid}<p role="alert" class="-mt-3 text-xs text-convert">
			Enter pages between 1 and {pageCount}.
		</p>{/if}
</div>
