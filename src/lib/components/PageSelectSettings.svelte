<script lang="ts">
	import { parsePageRange } from '$lib/pdf/page-range';
	import { pageKey } from '$lib/pdf/sources';
	import type { OrganizePage } from '$lib/pdf/types';

	let {
		pages,
		selected = $bindable<string[]>(),
		mode,
		processing
	}: {
		pages: OrganizePage[];
		selected: string[];
		mode: 'extract' | 'remove';
		processing: boolean;
	} = $props();

	let text = $state('');
	let editing = $state(false);
	const numbers = $derived(
		pages.filter((page) => selected.includes(pageKey(page))).map((page) => page.number)
	);
	const unmatched = $derived(text.trim() !== '' && numbers.length === 0);
	const nothingLeft = $derived(
		mode === 'remove' && pages.length > 0 && numbers.length === pages.length
	);

	$effect(() => {
		const formatted = format(numbers);
		if (!editing) text = formatted;
	});

	function format(list: number[]) {
		const parts: string[] = [];
		for (let index = 0; index < list.length; index++) {
			const start = list[index];
			while (list[index + 1] === list[index] + 1) index++;
			parts.push(start === list[index] ? `${start}` : `${start}-${list[index]}`);
		}
		return parts.join(', ');
	}

	function apply(value: string) {
		text = value;
		const chosen = new Set(parsePageRange(value, pages.length) ?? []);
		selected = pages.filter((page) => chosen.has(page.number)).map(pageKey);
	}
</script>

<div class="space-y-6">
	<div>
		<div class="mb-3 flex items-center justify-between">
			<label for="page-selection" class="text-sm font-semibold"
				>{mode === 'extract' ? 'Pages to extract' : 'Pages to remove'}</label
			>
			{#if pages.length}<span class="text-xs text-muted">{numbers.length} of {pages.length}</span
				>{/if}
		</div>
		<div
			class="flex items-center rounded-xl border border-white/10 bg-canvas px-3 {mode === 'extract'
				? 'focus-within:border-merge/50'
				: 'focus-within:border-convert/50'}"
		>
			<input
				id="page-selection"
				type="text"
				inputmode="numeric"
				autocomplete="off"
				value={text}
				disabled={processing || pages.length === 0}
				placeholder="e.g. 1-3, 5"
				oninput={(event) => apply(event.currentTarget.value)}
				onfocus={() => (editing = true)}
				onblur={() => {
					editing = false;
					text = format(numbers);
				}}
				class="min-w-0 flex-1 bg-transparent py-3 text-sm text-white outline-none placeholder:text-white/25 disabled:opacity-50"
			/>
		</div>
	</div>
	{#if unmatched}<p role="alert" class="text-xs text-convert">
			Enter pages between 1 and {pages.length}.
		</p>{:else if nothingLeft}<p role="alert" class="text-xs text-convert">
			At least one page must remain.
		</p>{/if}
</div>
