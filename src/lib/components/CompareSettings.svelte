<script lang="ts">
	import { cubicOut } from 'svelte/easing';
	import { fly } from 'svelte/transition';
	import { IconChevronDown, IconChevronUp, IconEqual } from '@tabler/icons-svelte-runes';
	import type { Change } from '$lib/pdf/compare-text';
	import ToggleSwitch from './ToggleSwitch.svelte';

	let {
		mode = $bindable<'text' | 'visual'>(),
		highlight = $bindable<boolean>(),
		current = $bindable<number>(),
		text,
		changes,
		reducedMotion,
		disabled = false
	}: {
		mode: 'text' | 'visual';
		highlight: boolean;
		/// The change being looked at, from 0, or -1.
		current: number;
		/// Whether both PDFs' text has been read, and whether there is any.
		text: 'waiting' | 'reading' | 'ready' | 'none' | 'unknown';
		changes: Change[];
		reducedMotion: boolean;
		disabled?: boolean;
	} = $props();

	const modes = [
		{ id: 'text', label: 'Text' },
		{ id: 'visual', label: 'Visual' }
	] as const;
	let list = $state<HTMLDivElement>();

	function stepTo(offset: number) {
		const count = changes.length;
		if (!count) return;
		current = current < 0 ? (offset > 0 ? 0 : count - 1) : (current + offset + count) % count;
	}

	$effect(() => {
		const item = list?.querySelector<HTMLElement>(`[data-change="${current}"]`);
		item?.scrollIntoView({ block: 'nearest', behavior: reducedMotion ? 'auto' : 'smooth' });
	});

	function page(change: Change) {
		return (
			change.changed[0]?.page ??
			(change.caret?.side === 'changed' ? change.caret.page : change.original[0]?.page)
		);
	}

	const action =
		'flex items-center justify-center gap-2 rounded-lg px-2 py-3 text-xs font-semibold text-muted enabled:hover:bg-panel-hover enabled:hover:text-white disabled:opacity-40 motion-safe:transition-colors';
</script>

<div class="space-y-6">
	<div>
		<h2 class="mb-3 text-sm font-semibold">Compare</h2>
		<div
			class="relative grid grid-cols-2 gap-2 rounded-xl bg-canvas p-1"
			role="group"
			aria-label="Compare"
		>
			<span
				aria-hidden="true"
				class="pointer-events-none absolute inset-y-1 left-1 w-[calc((100%-1rem)/2)] rounded-lg bg-panel-hover motion-safe:transition-transform motion-safe:duration-200 motion-safe:ease-[cubic-bezier(0.22,1,0.36,1)] {mode ===
				'visual'
					? 'translate-x-[calc(100%+0.5rem)]'
					: ''}"
			></span>
			{#each modes as option (option.id)}<button
					type="button"
					aria-pressed={mode === option.id}
					{disabled}
					class="relative z-10 rounded-lg px-2 py-3 text-xs font-semibold motion-safe:transition-colors {mode ===
					option.id
						? 'text-merge'
						: 'text-muted hover:text-white'}"
					onclick={() => (mode = option.id)}>{option.label}</button
				>{/each}
		</div>
	</div>

	<!-- Crop's sideways swap: Text from the left, Visual from the right. -->
	{#if mode === 'text'}
		{#if text !== 'waiting'}
			<div in:fly={{ x: -16, duration: reducedMotion ? 0 : 260, easing: cubicOut }}>
				<div class="mb-3 flex items-center justify-between">
					<h2 class="text-sm font-semibold">Changes</h2>
					{#if current >= 0 && changes.length}<span
							class="text-xs text-muted tabular-nums"
							role="status">{current + 1} of {changes.length}</span
						>{/if}
				</div>
				{#if text === 'reading'}
					<div class="space-y-1 rounded-xl bg-canvas p-1" aria-hidden="true">
						{#each [0.9, 0.7, 0.8] as share, index (index)}
							<div class="space-y-2 rounded-lg px-3 py-3">
								<div class="h-2 w-12 rounded-full bg-white/5 motion-safe:animate-pulse"></div>
								<div
									class="h-2.5 rounded-full bg-white/5 motion-safe:animate-pulse"
									style:width="{share * 100}%"
								></div>
							</div>
						{/each}
					</div>
				{:else if text === 'none' || text === 'unknown'}
					<div class="space-y-3 rounded-xl bg-canvas p-4">
						<p class="text-xs leading-relaxed text-muted">
							{text === 'none'
								? 'Neither PDF has text to compare.'
								: 'The text of these PDFs could not be read.'}
						</p>
						<button
							type="button"
							onclick={() => (mode = 'visual')}
							{disabled}
							class="{action} w-full bg-panel text-merge enabled:hover:text-merge"
							>Compare visually</button
						>
					</div>
				{:else if changes.length === 0}
					<div class="flex items-center gap-3 rounded-xl bg-canvas px-4 py-4 text-sm text-muted">
						<IconEqual size={18} class="shrink-0 text-merge" />The text is the same
					</div>
				{:else}
					<div class="space-y-2">
						<div
							class="grid grid-cols-2 gap-1 rounded-xl bg-canvas p-1"
							role="group"
							aria-label="Step through changes"
						>
							<button type="button" {disabled} onclick={() => stepTo(-1)} class={action}
								><IconChevronUp size={16} />Previous</button
							>
							<button type="button" {disabled} onclick={() => stepTo(1)} class={action}
								><IconChevronDown size={16} />Next</button
							>
						</div>
						<div
							bind:this={list}
							class="max-h-96 [scrollbar-width:thin] space-y-1 overflow-y-auto overscroll-contain rounded-xl bg-canvas p-1"
						>
							{#each changes as change, index (change.id)}
								<button
									type="button"
									data-change={index}
									aria-current={index === current}
									onclick={() => (current = index)}
									class="block w-full rounded-lg px-3 py-2.5 text-left motion-safe:transition-colors {index ===
									current
										? 'bg-panel-hover'
										: 'hover:bg-panel'}"
								>
									<span class="mb-1 flex items-center justify-between text-[11px] text-muted"
										><span
											>{change.removed && change.added
												? 'Changed'
												: change.added
													? 'Added'
													: 'Removed'}</span
										><span class="tabular-nums">Page {page(change)}</span></span
									>
									<span class="line-clamp-2 text-xs leading-relaxed break-words"
										>{#if change.removed}<del
												class="text-convert decoration-convert/50 {change.added ? 'mr-1' : ''}"
												>{change.removed}</del
											>{/if}{#if change.added}<ins class="text-merge no-underline"
												>{change.added}</ins
											>{/if}</span
									>
								</button>
							{/each}
						</div>
					</div>
				{/if}
			</div>
		{/if}
	{:else}
		<div in:fly={{ x: 16, duration: reducedMotion ? 0 : 260, easing: cubicOut }}>
			<ToggleSwitch bind:checked={highlight} label="Highlight differences" tone="merge" />
		</div>
	{/if}
</div>
