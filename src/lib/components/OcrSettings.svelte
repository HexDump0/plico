<script lang="ts">
	import { tick } from 'svelte';
	import { cubicOut } from 'svelte/easing';
	import { fade, scale, slide } from 'svelte/transition';
	import {
		IconChevronDown,
		IconChevronUp,
		IconPlus,
		IconRefresh,
		IconSearch,
		IconX
	} from '@tabler/icons-svelte-runes';
	import { OCR_LANGUAGES, languageName, type OcrModels } from '$lib/pdf/ocr.svelte';
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
	let adding = $state(false);
	let search = $state('');
	let active = $state(0);
	let field = $state<HTMLInputElement>();
	let picker = $state<HTMLDivElement>();
	let list = $state<HTMLUListElement>();

	const choices = $derived.by(() => {
		const needle = search.trim().toLowerCase();
		return OCR_LANGUAGES.filter(
			(language) =>
				!languages.includes(language.code) &&
				(!needle ||
					language.name.toLowerCase().includes(needle) ||
					language.code.startsWith(needle))
		);
	});
	$effect(() => {
		void search;
		active = 0;
	});

	function percent(code: string) {
		const state = models.states.get(code);
		if (state?.status !== 'loading' || !state.total) return 0;
		return Math.min(99, Math.floor((state.received / state.total) * 100));
	}

	async function openPicker() {
		adding = true;
		search = '';
		await tick();
		field?.focus();
	}

	function closePicker() {
		adding = false;
		search = '';
	}

	function choose(code: string) {
		languages = [...languages, code];
		void models.ensure(code).catch(() => {});
		closePicker();
	}

	async function onSearchKey(event: KeyboardEvent) {
		if (event.key === 'ArrowDown') active = Math.min(choices.length - 1, active + 1);
		else if (event.key === 'ArrowUp') active = Math.max(0, active - 1);
		else if (event.key === 'Enter' && choices[active]) choose(choices[active].code);
		else if (event.key === 'Escape') closePicker();
		else return;
		event.preventDefault();
		await tick();
		list?.querySelector(`[data-index="${active}"]`)?.scrollIntoView({ block: 'nearest' });
	}

	const action =
		'flex items-center justify-center gap-2 rounded-lg px-2 py-3 text-xs font-semibold text-muted enabled:hover:bg-panel-hover enabled:hover:text-white disabled:opacity-40 motion-safe:transition-colors';
</script>

<svelte:window
	onpointerdown={(event) => {
		if (adding && picker && !picker.contains(event.target as Node)) closePicker();
	}}
/>

<div class="space-y-6">
	<div>
		<h2 class="mb-3 text-sm font-semibold">Languages</h2>
		<div class="space-y-1 rounded-xl bg-canvas p-1">
			{#each languages as code (code)}
				{@const state = models.states.get(code)}
				<div
					transition:slide={{ duration: reducedMotion ? 0 : 200, easing: cubicOut }}
					class="relative isolate flex min-h-11 items-center gap-3 overflow-hidden rounded-lg px-3 text-sm"
				>
					<!-- Real bytes received; it fills the row, then fades once stored. -->
					{#if state?.status === 'loading'}<span
							aria-hidden="true"
							out:fade={{ duration: reducedMotion ? 0 : 300 }}
							class="absolute inset-y-0 left-0 -z-10 bg-compress/10 motion-safe:transition-[width] motion-safe:duration-300"
							style:width="{percent(code)}%"
						></span>{/if}
					<span class="min-w-0 flex-1 truncate">{languageName(code)}</span>
					{#if state?.status === 'loading'}
						<span class="text-xs text-muted tabular-nums" role="status"
							>{state.total ? `${percent(code)}%` : 'Downloading'}</span
						>
					{:else if state?.status === 'failed'}
						<button
							type="button"
							onclick={() => void models.ensure(code).catch(() => {})}
							class="flex items-center gap-1.5 text-xs font-semibold text-convert hover:text-white motion-safe:transition-colors"
							><IconRefresh size={14} />Retry</button
						>
					{/if}
					{#if languages.length > 1 && state?.status !== 'loading'}<button
							type="button"
							{disabled}
							onclick={() => (languages = languages.filter((other) => other !== code))}
							aria-label="Remove {languageName(code)}"
							title="Remove {languageName(code)}"
							class="-mr-1 flex size-7 shrink-0 items-center justify-center rounded-md text-muted enabled:hover:bg-panel-hover enabled:hover:text-white disabled:opacity-40 motion-safe:transition-colors"
							><IconX size={15} /></button
						>{/if}
				</div>
			{/each}
			<div bind:this={picker} class="relative">
				{#if adding}
					<div
						in:fade={{ duration: reducedMotion ? 0 : 140 }}
						class="flex items-center gap-2 rounded-lg bg-panel px-3"
					>
						<IconSearch size={16} stroke={1.8} class="shrink-0 text-muted" aria-hidden="true" />
						<input
							bind:this={field}
							bind:value={search}
							type="search"
							role="combobox"
							autocomplete="off"
							spellcheck="false"
							placeholder="Search languages"
							aria-label="Add a language"
							aria-expanded="true"
							aria-controls="{id}-languages"
							aria-activedescendant={choices[active] ? `${id}-language-${active}` : undefined}
							onkeydown={onSearchKey}
							class="min-w-0 flex-1 bg-transparent py-3 text-sm text-white outline-none placeholder:text-white/30 [&::-webkit-search-cancel-button]:hidden"
						/>
					</div>
					<ul
						bind:this={list}
						id="{id}-languages"
						role="listbox"
						aria-label="Languages"
						in:scale={{
							duration: reducedMotion ? 0 : 160,
							start: 0.96,
							opacity: 0,
							easing: cubicOut
						}}
						out:fade={{ duration: reducedMotion ? 0 : 100 }}
						class="absolute inset-x-0 top-full z-30 mt-2 max-h-64 origin-top [scrollbar-width:thin] overflow-y-auto overscroll-contain rounded-xl border border-white/10 bg-panel-hover p-1.5 shadow-2xl shadow-black/50"
					>
						{#each choices as language, index (language.code)}
							<li
								id="{id}-language-{index}"
								data-index={index}
								role="option"
								aria-selected={index === active}
								onpointerenter={() => (active = index)}
								onpointerdown={(event) => event.preventDefault()}
								onclick={() => choose(language.code)}
								onkeydown={() => {}}
								class="cursor-pointer truncate rounded-lg px-3 py-2.5 text-sm text-white motion-safe:transition-colors {index ===
								active
									? 'bg-white/[0.06]'
									: ''}"
							>
								{language.name}
							</li>
						{:else}
							<li class="px-3 py-2.5 text-sm text-muted">No languages found</li>
						{/each}
					</ul>
				{:else}
					<button type="button" {disabled} onclick={() => void openPicker()} class="{action} w-full"
						><IconPlus size={16} />Add language</button
					>
				{/if}
			</div>
		</div>
	</div>

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
