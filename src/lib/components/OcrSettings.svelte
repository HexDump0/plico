<script lang="ts">
	import { cubicOut } from 'svelte/easing';
	import { slide } from 'svelte/transition';
	import { IconChevronDown, IconChevronUp, IconSearch } from '@tabler/icons-svelte-runes';
	import type { OcrModels } from '$lib/pdf/ocr.svelte';
	import OcrLanguages from './OcrLanguages.svelte';
	import PageScope from './PageScope.svelte';
	import ToggleSwitch from './ToggleSwitch.svelte';

	let {
		languages = $bindable<string[]>(),
		pages = $bindable<string>(),
		skipText = $bindable<boolean>(),
		query = $bindable<string>(),
		models,
		pageCount,
		pagesInvalid,
		searchable,
		matches,
		match,
		onstep,
		reducedMotion,
		disabled = false
	}: {
		languages: string[];
		pages: string;
		skipText: boolean;
		query: string;
		models: OcrModels;
		pageCount: number;
		pagesInvalid: boolean;
		/// Whether the result's text has been read, so it can be searched.
		searchable: boolean;
		matches: number;
		/// The hit being looked at, from 0, or -1.
		match: number;
		onstep: (offset: number) => void;
		reducedMotion: boolean;
		disabled?: boolean;
	} = $props();
	const id = $props.id();
</script>

<div class="space-y-6">
	<OcrLanguages bind:languages {models} {reducedMotion} {disabled} />

	<div class="space-y-3">
		<h2 class="text-sm font-semibold">Pages</h2>
		<PageScope
			bind:pages
			{pageCount}
			invalid={pagesInvalid}
			label="Pages to read"
			{reducedMotion}
			{disabled}
		/>
		<ToggleSwitch bind:checked={skipText} label="Skip pages that have text" tone="compress" />
	</div>

	{#if searchable}
		<div transition:slide={{ duration: reducedMotion ? 0 : 220, easing: cubicOut }}>
			<label for="{id}-find" class="mb-3 block text-sm font-semibold">Find text</label>
			<div
				class="flex items-center gap-2 rounded-xl border border-white/10 bg-canvas pr-1 pl-3 transition-colors duration-200 focus-within:border-compress/50"
			>
				<IconSearch size={16} stroke={1.8} class="shrink-0 text-muted" aria-hidden="true" />
				<input
					id="{id}-find"
					type="search"
					autocomplete="off"
					spellcheck="false"
					bind:value={query}
					placeholder="Word or phrase"
					onkeydown={(event) => {
						if (event.key === 'Enter' && matches) {
							event.preventDefault();
							onstep(event.shiftKey ? -1 : 1);
						} else if (event.key === 'Escape' && query) {
							event.preventDefault();
							query = '';
						}
					}}
					class="min-w-0 flex-1 bg-transparent py-3 text-sm text-white outline-none placeholder:text-white/30 [&::-webkit-search-cancel-button]:hidden"
				/>
				{#if query.trim()}
					<span class="shrink-0 text-xs text-muted tabular-nums" role="status"
						>{matches === 0 ? 'None' : match >= 0 ? `${match + 1}/${matches}` : matches}</span
					>
					<button
						type="button"
						aria-label="Previous match"
						title="Previous match"
						disabled={!matches}
						onclick={() => onstep(-1)}
						class="flex size-8 shrink-0 items-center justify-center rounded-lg text-muted enabled:hover:bg-panel-hover enabled:hover:text-white disabled:opacity-40 motion-safe:transition-colors"
						><IconChevronUp size={16} /></button
					>
					<button
						type="button"
						aria-label="Next match"
						title="Next match"
						disabled={!matches}
						onclick={() => onstep(1)}
						class="flex size-8 shrink-0 items-center justify-center rounded-lg text-muted enabled:hover:bg-panel-hover enabled:hover:text-white disabled:opacity-40 motion-safe:transition-colors"
						><IconChevronDown size={16} /></button
					>
				{/if}
			</div>
		</div>
	{/if}
</div>
