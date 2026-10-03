<script lang="ts">
	import {
		IconArrowBackUp,
		IconChevronDown,
		IconChevronUp,
		IconSearch,
		IconTrash,
		IconX
	} from '@tabler/icons-svelte-runes';

	let {
		query = $bindable<string>(),
		fill = $bindable<'black' | 'white'>(),
		text,
		matches,
		match,
		unmarked,
		onstep,
		onmarkall,
		areas,
		pages,
		canUndo,
		onundo,
		onclear,
		disabled = false
	}: {
		query: string;
		fill: 'black' | 'white';
		/// Whether the PDF's text has been read, and whether it has any.
		text: 'reading' | 'ready' | 'none' | 'unknown';
		matches: number;
		/// The hit being looked at, from 0, or -1.
		match: number;
		/// Hits not yet under a box.
		unmarked: number;
		onstep: (offset: number) => void;
		onmarkall: () => void;
		areas: number;
		pages: number;
		canUndo: boolean;
		onundo: () => void;
		onclear: () => void;
		disabled?: boolean;
	} = $props();
	const id = $props.id();

	const fills = [
		{ id: 'black', label: 'Black', swatch: 'bg-black ring-white/25' },
		{ id: 'white', label: 'White', swatch: 'bg-white ring-transparent' }
	] as const;
	const searching = $derived(query.trim() !== '');
	const status = $derived(
		text === 'reading'
			? 'Reading text...'
			: text === 'none'
				? 'No text found'
				: text === 'unknown' || !searching
					? ''
					: matches === 0
						? 'No matches'
						: match >= 0
							? `${match + 1} of ${matches}`
							: `${matches} ${matches === 1 ? 'match' : 'matches'}`
	);
	const action =
		'flex items-center justify-center gap-2 rounded-lg px-2 py-3 text-xs font-semibold text-muted enabled:hover:bg-panel-hover enabled:hover:text-white disabled:opacity-40 motion-safe:transition-colors';
</script>

<div class="space-y-6">
	<div>
		<div class="mb-3 flex items-center justify-between">
			<label for="{id}-find" class="text-sm font-semibold">Find text</label>
			<span class="text-xs text-muted tabular-nums" role="status">{status}</span>
		</div>
		<div
			class="flex items-center gap-2 rounded-xl border border-white/10 bg-canvas px-3 transition-colors duration-200 focus-within:border-merge/50"
		>
			<IconSearch size={16} stroke={1.8} class="shrink-0 text-muted" aria-hidden="true" />
			<input
				id="{id}-find"
				type="search"
				autocomplete="off"
				spellcheck="false"
				bind:value={query}
				disabled={disabled || text === 'none' || text === 'unknown'}
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
				class="min-w-0 flex-1 bg-transparent py-3 text-sm text-white outline-none placeholder:text-white/30 disabled:opacity-50 [&::-webkit-search-cancel-button]:hidden"
			/>
			{#if query}<button
					type="button"
					aria-label="Clear search"
					onclick={() => (query = '')}
					class="flex size-6 shrink-0 items-center justify-center rounded-md text-muted hover:text-white motion-safe:transition-colors"
					><IconX size={15} /></button
				>{/if}
		</div>
		{#if searching && matches > 0}
			<div
				class="mt-2 grid grid-cols-[auto_auto_1fr] gap-1 rounded-xl bg-canvas p-1"
				role="group"
				aria-label="Matches"
			>
				<button
					type="button"
					aria-label="Previous match"
					title="Previous match"
					{disabled}
					onclick={() => onstep(-1)}
					class="{action} px-3"><IconChevronUp size={16} /></button
				>
				<button
					type="button"
					aria-label="Next match"
					title="Next match"
					{disabled}
					onclick={() => onstep(1)}
					class="{action} px-3"><IconChevronDown size={16} /></button
				>
				<button
					type="button"
					disabled={disabled || unmarked === 0}
					onclick={onmarkall}
					class="{action} text-merge enabled:hover:text-merge"
					>{unmarked === 0
						? 'All marked'
						: unmarked === matches
							? `Mark all ${matches}`
							: `Mark ${unmarked} more`}</button
				>
			</div>
		{/if}
	</div>

	<div>
		<div class="mb-3 flex items-center justify-between">
			<h2 class="text-sm font-semibold">Areas</h2>
			<span class="text-xs text-muted tabular-nums"
				>{areas === 0 ? 'None' : `${areas} on ${pages} ${pages === 1 ? 'page' : 'pages'}`}</span
			>
		</div>
		<div class="grid grid-cols-2 gap-1 rounded-xl bg-canvas p-1" role="group" aria-label="Areas">
			<button type="button" onclick={onundo} disabled={disabled || !canUndo} class={action}
				><IconArrowBackUp size={16} />Undo</button
			>
			<button type="button" onclick={onclear} disabled={disabled || areas === 0} class={action}
				><IconTrash size={16} />Clear all</button
			>
		</div>
	</div>

	<div>
		<h2 class="mb-3 text-sm font-semibold">Fill</h2>
		<div
			class="relative grid grid-cols-2 gap-2 rounded-xl bg-canvas p-1"
			role="group"
			aria-label="Fill"
		>
			<span
				aria-hidden="true"
				class="pointer-events-none absolute inset-y-1 left-1 w-[calc((100%-1rem)/2)] rounded-lg bg-panel-hover motion-safe:transition-transform motion-safe:duration-200 motion-safe:ease-[cubic-bezier(0.22,1,0.36,1)] {fill ===
				'white'
					? 'translate-x-[calc(100%+0.5rem)]'
					: ''}"
			></span>
			{#each fills as option (option.id)}<button
					type="button"
					aria-pressed={fill === option.id}
					{disabled}
					class="relative z-10 flex items-center justify-center gap-2 rounded-lg px-2 py-3 text-xs font-semibold motion-safe:transition-colors {fill ===
					option.id
						? 'text-merge'
						: 'text-muted hover:text-white'}"
					onclick={() => (fill = option.id)}
					><span aria-hidden="true" class="size-3 rounded-sm ring-1 {option.swatch}"
					></span>{option.label}</button
				>{/each}
		</div>
	</div>
</div>
