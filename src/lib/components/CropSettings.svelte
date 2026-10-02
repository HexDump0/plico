<script lang="ts">
	import { cubicOut } from 'svelte/easing';
	import { fly } from 'svelte/transition';
	import { IconArrowBackUp, IconFocusCentered } from '@tabler/icons-svelte-runes';
	import PageScope from './PageScope.svelte';

	let {
		mode = $bindable<'manual' | 'auto'>(),
		padding = $bindable<number>(),
		pages = $bindable<string>(),
		size,
		changed,
		canFit,
		onfit,
		onreset,
		pageCount,
		pagesInvalid,
		reducedMotion,
		disabled = false
	}: {
		mode: 'manual' | 'auto';
		padding: number;
		pages: string;
		/// The kept area of the page on screen, already formatted.
		size: string;
		changed: boolean;
		canFit: boolean;
		onfit: () => void;
		onreset: () => void;
		pageCount: number;
		pagesInvalid: boolean;
		reducedMotion: boolean;
		disabled?: boolean;
	} = $props();

	const modes = [
		{ id: 'manual', label: 'Selection' },
		{ id: 'auto', label: 'Content' }
	] as const;
	const paddings = [
		{ value: 0, label: 'None' },
		{ value: 9, label: 'Small' },
		{ value: 18, label: 'Normal' }
	] as const;
	const action =
		'flex items-center justify-center gap-2 rounded-lg px-2 py-3 text-xs font-semibold text-muted enabled:hover:bg-panel-hover enabled:hover:text-white disabled:opacity-40 motion-safe:transition-colors';
</script>

<div class="space-y-6">
	<div>
		<h2 class="mb-3 text-sm font-semibold">Crop to</h2>
		<div
			class="relative grid grid-cols-2 gap-2 rounded-xl bg-canvas p-1"
			role="group"
			aria-label="Crop to"
		>
			<span
				aria-hidden="true"
				class="pointer-events-none absolute inset-y-1 left-1 w-[calc((100%-1rem)/2)] rounded-lg bg-panel-hover motion-safe:transition-transform motion-safe:duration-200 motion-safe:ease-[cubic-bezier(0.22,1,0.36,1)] {mode ===
				'auto'
					? 'translate-x-[calc(100%+0.5rem)]'
					: ''}"
			></span>
			{#each modes as option (option.id)}<button
					type="button"
					aria-pressed={mode === option.id}
					{disabled}
					class="relative z-10 rounded-lg px-2 py-3 text-xs font-semibold motion-safe:transition-colors {mode ===
					option.id
						? 'text-brand'
						: 'text-muted hover:text-white'}"
					onclick={() => (mode = option.id)}>{option.label}</button
				>{/each}
		</div>
	</div>

	<!-- Watermark's swap, turned sideways to follow the pill: Selection
	comes in from the left, Content from the right. -->
	{#if mode === 'manual'}
		<div in:fly={{ x: -16, duration: reducedMotion ? 0 : 260, easing: cubicOut }}>
			<div class="mb-3 flex items-center justify-between">
				<h2 class="text-sm font-semibold">Selection</h2>
				<span class="text-xs text-muted tabular-nums">{size}</span>
			</div>
			<div
				class="grid grid-cols-2 gap-1 rounded-xl bg-canvas p-1"
				role="group"
				aria-label="Selection"
			>
				<button type="button" onclick={onfit} disabled={disabled || !canFit} class={action}
					><IconFocusCentered size={16} />Fit content</button
				>
				<button type="button" onclick={onreset} disabled={disabled || !changed} class={action}
					><IconArrowBackUp size={16} />Reset</button
				>
			</div>
		</div>
	{:else}
		<div in:fly={{ x: 16, duration: reducedMotion ? 0 : 260, easing: cubicOut }}>
			<div class="mb-3 flex items-center justify-between">
				<h2 class="text-sm font-semibold">Padding</h2>
				<span class="text-xs text-muted tabular-nums">{size}</span>
			</div>
			<div
				class="grid grid-cols-3 gap-1 rounded-xl bg-canvas p-1"
				role="group"
				aria-label="Padding"
			>
				{#each paddings as option (option.value)}<button
						type="button"
						aria-pressed={padding === option.value}
						{disabled}
						class="rounded-lg px-2 py-3 text-xs font-semibold motion-safe:transition-colors {padding ===
						option.value
							? 'bg-panel-hover text-brand'
							: 'text-muted hover:text-white'}"
						onclick={() => (padding = option.value)}>{option.label}</button
					>{/each}
			</div>
		</div>
	{/if}

	<div class="space-y-3">
		<h2 class="text-sm font-semibold">Pages</h2>
		<PageScope
			bind:pages
			{pageCount}
			invalid={pagesInvalid}
			label="Pages to crop"
			{reducedMotion}
			{disabled}
		/>
	</div>
</div>
