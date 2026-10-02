<script lang="ts">
	import { tick, untrack } from 'svelte';
	import { cubicOut } from 'svelte/easing';
	import { slide } from 'svelte/transition';

	let {
		pages = $bindable<string>(),
		pageCount,
		invalid,
		label,
		reducedMotion,
		disabled = false
	}: {
		pages: string;
		pageCount: number;
		invalid: boolean;
		/// Names the range field for screen readers, e.g. "Pages to number".
		label: string;
		reducedMotion: boolean;
		disabled?: boolean;
	} = $props();
	let custom = $state(untrack(() => pages !== ''));
	let rangeInput = $state<HTMLInputElement>();

	async function setCustom(next: boolean) {
		custom = next;
		if (!next) pages = '';
		else {
			await tick();
			rangeInput?.focus();
		}
	}
</script>

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
			class="flex items-center gap-2 rounded-xl border bg-canvas px-3 transition-colors duration-200 {invalid
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
				aria-label={label}
				aria-invalid={invalid}
				class="min-w-0 flex-1 bg-transparent py-3 text-sm text-white outline-none placeholder:text-white/30 disabled:opacity-50"
			/>{#if pageCount}<span class="shrink-0 text-xs text-muted">of {pageCount}</span>{/if}
		</div>
		{#if invalid}<p role="alert" class="pt-2 text-xs text-convert">
				Enter pages between 1 and {pageCount}.
			</p>{/if}
	</div>
{/if}
