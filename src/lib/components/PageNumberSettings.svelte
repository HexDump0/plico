<script lang="ts">
	import { tick, untrack } from 'svelte';
	import { cubicOut } from 'svelte/easing';
	import { slide } from 'svelte/transition';
	import { IconMinus, IconPlus } from '@tabler/icons-svelte-runes';
	import type { StampPosition } from '$lib/pdf/stamp-layout';
	import type { FontFamily } from '$lib/pdf/standard-fonts';
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
	let custom = $state(untrack(() => pages !== ''));
	let rangeInput = $state<HTMLInputElement>();

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

	async function setCustom(next: boolean) {
		custom = next;
		if (!next) pages = '';
		else {
			await tick();
			rangeInput?.focus();
		}
	}
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
		<div
			class="relative grid grid-cols-2 gap-2 rounded-xl bg-canvas p-1"
			role="group"
			aria-label="Pages"
		>
			<span
				aria-hidden="true"
				class="pointer-events-none absolute inset-y-1 left-1 w-[calc((100%-1rem)/2)] rounded-lg bg-panel-hover motion-safe:transition-transform motion-safe:duration-200 motion-safe:ease-[cubic-bezier(0.22,1,0.36,1)] {custom
					? 'translate-x-[calc(100%+0.5rem)]'
					: ''}"
			></span>
			{#each [false, true] as option (option)}<button
					type="button"
					aria-pressed={custom === option}
					{disabled}
					class="relative z-10 rounded-lg px-2 py-3 text-xs font-semibold motion-safe:transition-colors {custom ===
					option
						? 'text-brand'
						: 'text-muted hover:text-white'}"
					onclick={() => void setCustom(option)}>{option ? 'Custom' : 'All pages'}</button
				>{/each}
		</div>
		{#if custom}
			<div transition:slide={{ duration: reducedMotion ? 0 : 220, easing: cubicOut }}>
				<div
					class="flex items-center gap-2 rounded-xl border bg-canvas px-3 transition-colors duration-200 {pagesInvalid
						? 'border-convert/60'
						: 'border-white/10 focus-within:border-brand/50'}"
				>
					<input
						bind:this={rangeInput}
						type="text"
						inputmode="numeric"
						autocomplete="off"
						bind:value={pages}
						{disabled}
						placeholder={pageCount > 1 ? `e.g. 2-${pageCount}` : 'e.g. 1'}
						aria-label="Pages to number"
						aria-invalid={pagesInvalid}
						class="min-w-0 flex-1 bg-transparent py-3 text-sm text-white outline-none placeholder:text-white/30 disabled:opacity-50"
					/>{#if pageCount}<span class="shrink-0 text-xs text-muted">of {pageCount}</span>{/if}
				</div>
				{#if pagesInvalid}<p role="alert" class="pt-2 text-xs text-convert">
						Enter pages between 1 and {pageCount}.
					</p>{/if}
			</div>
		{/if}
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
