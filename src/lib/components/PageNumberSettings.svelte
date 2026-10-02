<script lang="ts">
	import { IconMinus, IconPlus } from '@tabler/icons-svelte-runes';
	import type { StampPosition } from '$lib/pdf/stamp-layout';
	import type { FontFamily } from '$lib/pdf/standard-fonts';
	import PageScope from './PageScope.svelte';
	import PositionPicker from './PositionPicker.svelte';
	import SelectMenu from './SelectMenu.svelte';

	let {
		position = $bindable<StampPosition>(),
		template = $bindable<string>(),
		firstNumber = $bindable<number>(),
		pages = $bindable<string>(),
		lastNumber,
		pageCount,
		pagesInvalid,
		family,
		reducedMotion,
		disabled = false
	}: {
		position: StampPosition;
		template: string;
		firstNumber: number;
		pages: string;
		lastNumber: number;
		pageCount: number;
		pagesInvalid: boolean;
		family: FontFamily;
		reducedMotion: boolean;
		disabled?: boolean;
	} = $props();
	const id = $props.id();

	const templates = ['{n}', 'Page {n}', '{n} / {total}', 'Page {n} / {total}'];
	// The examples are set in the font the numbers are stamped with.
	const fonts = {
		helvetica: "Helvetica, Arial, 'Liberation Sans', sans-serif",
		times: "'Times New Roman', Times, 'Liberation Serif', serif",
		courier: "'Courier New', Courier, 'Liberation Mono', monospace"
	};
	const firstInvalid = $derived(
		!(Number.isInteger(firstNumber) && firstNumber >= 0 && firstNumber <= 99999)
	);
	const first = $derived(firstInvalid ? 1 : firstNumber);
	const formats = $derived(
		templates.map((value) => ({
			value,
			label: value
				.replace('{n}', String(first))
				.replace('{total}', String(Math.max(first, lastNumber)))
		}))
	);
</script>

<div class="space-y-6">
	<div>
		<h2 class="mb-3 text-sm font-semibold">Position</h2>
		<PositionPicker bind:value={position} positions={[0, 1, 2, 6, 7, 8]} {disabled} />
	</div>

	<div>
		<h2 class="mb-3 text-sm font-semibold">Format</h2>
		<SelectMenu
			bind:value={template}
			options={formats}
			label="Format"
			labelFont={fonts[family]}
			{reducedMotion}
			{disabled}
		/>
	</div>

	<div class="space-y-3">
		<h2 class="text-sm font-semibold">Pages</h2>
		<PageScope
			bind:pages
			{pageCount}
			invalid={pagesInvalid}
			label="Pages to number"
			{reducedMotion}
			{disabled}
		/>
		<div
			class="flex items-center justify-between gap-4 rounded-xl bg-canvas/50 py-2 pr-2 pl-3.5 text-sm"
		>
			<label for="{id}-first">Start numbering at</label>
			<div
				class="flex items-center rounded-lg border bg-canvas transition-colors duration-200 {firstInvalid
					? 'border-convert/60'
					: 'border-white/10 focus-within:border-brand/50'}"
			>
				<button
					type="button"
					aria-label="Decrease"
					disabled={disabled || first <= 0}
					onclick={() => (firstNumber = Math.max(0, first - 1))}
					class="flex size-8 items-center justify-center text-muted transition-colors enabled:hover:text-white disabled:opacity-30"
					><IconMinus size={15} stroke={2.25} /></button
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
					class="w-12 [appearance:textfield] bg-transparent text-center text-sm text-white tabular-nums outline-none disabled:opacity-50 [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
				/>
				<button
					type="button"
					aria-label="Increase"
					disabled={disabled || first >= 99999}
					onclick={() => (firstNumber = Math.min(99999, first + 1))}
					class="flex size-8 items-center justify-center text-muted transition-colors enabled:hover:text-white disabled:opacity-30"
					><IconPlus size={15} stroke={2.25} /></button
				>
			</div>
		</div>
	</div>
</div>
