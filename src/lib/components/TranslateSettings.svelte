<script lang="ts">
	import { tick } from 'svelte';
	import { cubicOut } from 'svelte/easing';
	import { fade, scale, slide } from 'svelte/transition';
	import { IconArrowsUpDown, IconRefresh, IconSearch } from '@tabler/icons-svelte-runes';
	import {
		SOURCE_LANGUAGES,
		TARGET_LANGUAGES,
		modelPairs,
		translateLanguage,
		translateLanguageName,
		type TranslateModels
	} from '$lib/pdf/translate.svelte';
	import { resolve } from '$app/paths';
	import PageScope from './PageScope.svelte';
	import ToggleSwitch from './ToggleSwitch.svelte';

	let {
		from = $bindable<string>(),
		to = $bindable<string>(),
		pages = $bindable<string>(),
		keepOriginal = $bindable<boolean>(),
		showOriginal = $bindable<boolean>(),
		translated,
		detected,
		models,
		pageCount,
		pagesInvalid,
		noText,
		reducedMotion,
		disabled = false
	}: {
		from: string;
		to: string;
		pages: string;
		keepOriginal: boolean;
		showOriginal: boolean;
		/// Whether the preview has translations to compare with the original.
		translated: boolean;
		/// The language the document was found to be in, if it was.
		detected: string | undefined;
		models: TranslateModels;
		pageCount: number;
		pagesInvalid: boolean;
		/// The document was read and has no text, as a scan does.
		noText: boolean;
		reducedMotion: boolean;
		disabled?: boolean;
	} = $props();
	const id = $props.id();

	type Side = 'from' | 'to';
	let open = $state<Side | null>(null);
	let search = $state('');
	let active = $state(0);
	let turns = $state(0);
	let field = $state<HTMLInputElement>();
	let list = $state<HTMLUListElement>();
	let card = $state<HTMLDivElement>();

	const pairs = $derived(modelPairs(from, to));
	// Each side's row shows the model that side needs: into English from the
	// source, out of it into the target.
	const sideModel = $derived({
		from: pairs.find((pair) => pair.endsWith('-en')),
		to: pairs.find((pair) => pair.startsWith('en-'))
	});
	const swappable = $derived(
		!!translateLanguage(to)?.from && !!translateLanguage(from)?.to && from !== to
	);
	const choices = $derived.by(() => {
		const needle = search.trim().toLowerCase();
		return (open === 'to' ? TARGET_LANGUAGES : SOURCE_LANGUAGES).filter(
			(language) =>
				!needle ||
				language.name.toLowerCase().includes(needle) ||
				language.code.toLowerCase().startsWith(needle)
		);
	});
	$effect(() => {
		void search;
		active = 0;
	});

	function percent(pair: string | undefined) {
		const state = pair ? models.states.get(pair) : undefined;
		if (state?.status !== 'loading' || !state.total) return 0;
		return Math.min(99, Math.floor((state.received / state.total) * 100));
	}

	async function show(side: Side) {
		if (disabled) return;
		open = side;
		search = '';
		await tick();
		const current = choices.findIndex(
			(language) => language.code === (side === 'from' ? from : to)
		);
		active = Math.max(0, current);
		field?.focus();
		list?.querySelector(`[data-index="${active}"]`)?.scrollIntoView({ block: 'nearest' });
	}

	function close() {
		open = null;
		search = '';
	}

	function choose(code: string) {
		if (open === 'from') {
			// Picking the target as the source swaps them, as people expect.
			if (code === to) to = from !== code && translateLanguage(from)?.to ? from : to;
			from = code;
		} else if (open === 'to') {
			if (code === from) from = to !== code && translateLanguage(to)?.from ? to : from;
			to = code;
		}
		close();
	}

	function swap() {
		if (!swappable) return;
		[from, to] = [to, from];
		turns++;
	}

	async function onSearchKey(event: KeyboardEvent) {
		if (event.key === 'ArrowDown') active = Math.min(choices.length - 1, active + 1);
		else if (event.key === 'ArrowUp') active = Math.max(0, active - 1);
		else if (event.key === 'Enter' && choices[active]) choose(choices[active].code);
		else if (event.key === 'Escape') close();
		else return;
		event.preventDefault();
		await tick();
		list?.querySelector(`[data-index="${active}"]`)?.scrollIntoView({ block: 'nearest' });
	}
</script>

<svelte:window
	onpointerdown={(event) => {
		if (open && card && !card.contains(event.target as Node)) close();
	}}
/>

{#snippet row(side: Side)}
	{@const code = side === 'from' ? from : to}
	{@const model = sideModel[side]}
	{@const state = model ? models.states.get(model) : undefined}
	<div class="relative isolate flex min-h-12 items-center overflow-hidden rounded-lg pr-10">
		<!-- Real bytes received; it fills the row, then fades once stored. -->
		{#if state?.status === 'loading'}<span
				aria-hidden="true"
				out:fade={{ duration: reducedMotion ? 0 : 300 }}
				class="absolute inset-y-0 left-0 -z-10 bg-compress/10 motion-safe:transition-[width] motion-safe:duration-300"
				style:width="{percent(model)}%"
			></span>{/if}
		<button
			type="button"
			{disabled}
			aria-haspopup="listbox"
			aria-expanded={open === side}
			aria-controls={open === side ? `${id}-languages` : undefined}
			onclick={() => (open === side ? close() : void show(side))}
			class="flex min-w-0 flex-1 items-center gap-3 self-stretch rounded-lg px-3 text-left text-sm enabled:hover:bg-white/[0.03] disabled:opacity-60 motion-safe:transition-colors"
		>
			<span class="w-9 shrink-0 text-xs text-muted">{side === 'from' ? 'From' : 'To'}</span>
			<span class="min-w-0 flex-1 truncate">{translateLanguageName(code)}</span>
			{#if state?.status === 'loading'}
				<span class="text-xs text-muted tabular-nums" role="status">{percent(model)}%</span>
			{:else if side === 'from' && detected === from}
				<span class="text-xs text-muted">Detected</span>
			{/if}
		</button>
		{#if state?.status === 'failed'}
			<button
				type="button"
				onclick={() => model && void models.ensure(model).catch(() => {})}
				class="mr-2 flex shrink-0 items-center gap-1.5 rounded-md px-2 py-1.5 text-xs font-semibold text-convert hover:text-white motion-safe:transition-colors"
				><IconRefresh size={14} />Retry</button
			>
		{/if}
	</div>
{/snippet}

<div class="space-y-6">
	<div>
		<h2 class="mb-3 text-sm font-semibold">Languages</h2>
		<div bind:this={card} class="relative">
			<div class="relative rounded-xl bg-canvas p-1">
				{@render row('from')}
				<div class="mx-3 h-px bg-white/[0.06]" aria-hidden="true"></div>
				{@render row('to')}
				<!-- Sits on the line between the two, out of the way of their text. -->
				<button
					type="button"
					disabled={disabled || !swappable}
					onclick={swap}
					aria-label="Swap languages"
					title="Swap languages"
					class="absolute top-1/2 right-2 z-10 flex size-8 -translate-y-1/2 items-center justify-center rounded-full bg-panel text-muted ring-1 ring-white/10 enabled:hover:bg-panel-hover enabled:hover:text-white disabled:opacity-40 motion-safe:transition-colors"
				>
					<IconArrowsUpDown
						size={16}
						class="motion-safe:transition-transform motion-safe:duration-300 motion-safe:ease-[cubic-bezier(0.22,1,0.36,1)]"
						style="transform: rotate({turns * 180}deg)"
					/>
				</button>
			</div>
			{#if open}
				<div
					in:scale={{
						duration: reducedMotion ? 0 : 160,
						start: 0.96,
						opacity: 0,
						easing: cubicOut
					}}
					out:fade={{ duration: reducedMotion ? 0 : 100 }}
					class="absolute inset-x-0 z-30 mt-2 origin-top overflow-hidden rounded-xl border border-white/10 bg-panel-hover shadow-2xl shadow-black/50"
					style:top={open === 'from' ? '3.25rem' : '100%'}
				>
					<div class="flex items-center gap-2 border-b border-white/[0.06] px-3">
						<IconSearch size={16} stroke={1.8} class="shrink-0 text-muted" aria-hidden="true" />
						<input
							bind:this={field}
							bind:value={search}
							type="search"
							role="combobox"
							autocomplete="off"
							spellcheck="false"
							placeholder="Search languages"
							aria-label={open === 'from' ? 'Translate from' : 'Translate to'}
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
						class="max-h-64 [scrollbar-width:thin] overflow-y-auto overscroll-contain p-1.5"
					>
						{#each choices as language, index (language.code)}
							{@const chosen = language.code === (open === 'from' ? from : to)}
							<li
								id="{id}-language-{index}"
								data-index={index}
								role="option"
								aria-selected={chosen}
								onpointerenter={() => (active = index)}
								onpointerdown={(event) => event.preventDefault()}
								onclick={() => choose(language.code)}
								onkeydown={() => {}}
								class="flex cursor-pointer items-center gap-3 rounded-lg px-3 py-2.5 text-sm motion-safe:transition-colors {index ===
								active
									? 'bg-white/[0.06]'
									: ''} {chosen ? 'text-compress' : 'text-white'}"
							>
								<span class="min-w-0 flex-1 truncate">{language.name}</span>
								{#if open === 'from' && language.code === detected}
									<span class="text-xs text-muted">Detected</span>
								{/if}
							</li>
						{:else}
							<li class="px-3 py-2.5 text-sm text-muted">No languages found</li>
						{/each}
					</ul>
				</div>
			{/if}
		</div>
	</div>

	<div class="space-y-3">
		<h2 class="text-sm font-semibold">Pages</h2>
		<PageScope
			bind:pages
			{pageCount}
			invalid={pagesInvalid}
			label="Pages to translate"
			{reducedMotion}
			{disabled}
		/>
		<ToggleSwitch bind:checked={keepOriginal} label="Keep original pages" tone="compress" />
	</div>

	{#if translated}
		<div transition:slide={{ duration: reducedMotion ? 0 : 220, easing: cubicOut }}>
			<ToggleSwitch bind:checked={showOriginal} label="Show original" tone="compress" />
		</div>
	{/if}

	{#if noText}
		<p class="text-xs leading-relaxed text-muted" role="status">
			This PDF has no text to translate. Scanned pages need
			<a
				href={resolve('/tools/[tool]', { tool: 'ocr' })}
				class="font-semibold text-compress hover:text-white motion-safe:transition-colors"
				>OCR PDF</a
			> first.
		</p>
	{/if}
</div>
