<script lang="ts">
	import { slide } from 'svelte/transition';
	import { IconChevronDown } from '@tabler/icons-svelte-runes';
	import ToggleSwitch from './ToggleSwitch.svelte';

	let advancedOpen = $state(false);
	let {
		level = $bindable<'light' | 'balanced' | 'strong'>(),
		removeMetadata = $bindable<boolean>(),
		removeThumbnails = $bindable<boolean>(),
		reducedMotion
	}: {
		level: 'light' | 'balanced' | 'strong';
		removeMetadata: boolean;
		removeThumbnails: boolean;
		reducedMotion: boolean;
	} = $props();
	const levels = [
		{ id: 'light', label: 'Light' },
		{
			id: 'balanced',
			label: 'Balanced'
		},
		{
			id: 'strong',
			label: 'High'
		}
	] as const;
</script>

<div class="space-y-6">
	<div>
		<h2 class="mb-3 text-sm font-semibold">Compression</h2>
		<div
			class="grid grid-cols-3 gap-1 rounded-xl bg-canvas p-1"
			role="group"
			aria-label="Compression level"
		>
			{#each levels as option (option.id)}<button
					type="button"
					aria-pressed={level === option.id}
					class="rounded-lg px-2 py-3 text-xs font-semibold motion-safe:transition-colors {level ===
					option.id
						? 'bg-panel-hover text-compress'
						: 'text-muted hover:text-white'}"
					onclick={() => (level = option.id)}>{option.label}</button
				>{/each}
		</div>
	</div>
	<div>
		<button
			type="button"
			aria-expanded={advancedOpen}
			aria-controls="compress-advanced-options"
			class="flex w-full items-center justify-between text-sm font-medium text-muted hover:text-white"
			onclick={() => (advancedOpen = !advancedOpen)}
		>
			Advanced options
			<IconChevronDown
				size={16}
				stroke={1.75}
				aria-hidden="true"
				class="motion-safe:transition-transform {advancedOpen ? 'rotate-180' : ''}"
			/>
		</button>
		{#if advancedOpen}
			<div
				id="compress-advanced-options"
				class="space-y-2 pt-4"
				transition:slide={{ duration: reducedMotion ? 0 : 300 }}
			>
				<ToggleSwitch bind:checked={removeMetadata} label="Remove metadata" tone="compress" />
				<ToggleSwitch
					bind:checked={removeThumbnails}
					label="Remove page thumbnails"
					tone="compress"
				/>
			</div>
		{/if}
	</div>
</div>
