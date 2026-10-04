<script lang="ts">
	import { cubicOut } from 'svelte/easing';
	import { slide } from 'svelte/transition';
	import { IconArrowBackUp, IconEraser } from '@tabler/icons-svelte-runes';

	let {
		changed,
		filled,
		undrawable,
		onreset,
		onclear,
		reducedMotion,
		disabled = false
	}: {
		changed: number;
		/// Whether any field holds a value Clear all would take out.
		filled: boolean;
		undrawable?: { label: string; character: string };
		onreset: () => void;
		onclear: () => void;
		reducedMotion: boolean;
		disabled?: boolean;
	} = $props();

	const action =
		'flex items-center justify-center gap-2 rounded-lg px-2 py-3 text-xs font-semibold text-muted enabled:hover:bg-panel-hover enabled:hover:text-white disabled:opacity-40 motion-safe:transition-colors';
</script>

<div>
	<h2 class="mb-3 text-sm font-semibold">Fields</h2>
	<div class="grid grid-cols-2 gap-1 rounded-xl bg-canvas p-1" role="group" aria-label="Fields">
		<button type="button" onclick={onreset} disabled={disabled || changed === 0} class={action}
			><IconArrowBackUp size={16} />Reset</button
		>
		<button type="button" onclick={onclear} disabled={disabled || !filled} class={action}
			><IconEraser size={16} />Clear all</button
		>
	</div>
	{#if undrawable}<p
			role="alert"
			transition:slide={{ duration: reducedMotion ? 0 : 180, easing: cubicOut }}
			class="pt-3 text-xs text-convert"
		>
			“{undrawable.character}” in {undrawable.label} can't be drawn with Plico's fonts
		</p>{/if}
</div>
