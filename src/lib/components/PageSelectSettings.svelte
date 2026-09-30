<script lang="ts">
	import { cubicOut } from 'svelte/easing';
	import { slide } from 'svelte/transition';
	import { parsePageRange } from '$lib/pdf/page-range';
	import { pageKey } from '$lib/pdf/sources';
	import type { OrganizePage } from '$lib/pdf/types';

	let {
		pages,
		selected = $bindable<string[]>(),
		mode,
		processing,
		reducedMotion,
		output = $bindable<'pdf' | 'separate' | 'images'>('pdf'),
		imageFormat = $bindable<'jpg' | 'png'>('jpg'),
		dpi = $bindable<number>(150)
	}: {
		pages: OrganizePage[];
		selected: string[];
		mode: 'extract' | 'remove';
		processing: boolean;
		reducedMotion: boolean;
		output?: 'pdf' | 'separate' | 'images';
		imageFormat?: 'jpg' | 'png';
		dpi?: number;
	} = $props();
	const outputs = [
		{ id: 'pdf', label: 'One PDF' },
		{ id: 'separate', label: 'Separate' },
		{ id: 'images', label: 'Images' }
	] as const;
	const formats = [
		{ id: 'jpg', label: 'JPG' },
		{ id: 'png', label: 'PNG' }
	] as const;
	const resolutions = [
		{ dpi: 72, label: '72 DPI' },
		{ dpi: 150, label: '150 DPI' },
		{ dpi: 300, label: '300 DPI' }
	] as const;

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
	{#if mode === 'extract'}
		<div>
			<h2 class="mb-3 text-sm font-semibold">Extract as</h2>
			<div
				class="relative grid grid-cols-3 gap-2 rounded-xl bg-canvas p-1"
				role="group"
				aria-label="Extract as"
			>
				<span
					aria-hidden="true"
					class="pointer-events-none absolute inset-y-1 left-1 w-[calc((100%-1.5rem)/3)] rounded-lg bg-panel-hover motion-safe:transition-transform motion-safe:duration-200 motion-safe:ease-[cubic-bezier(0.22,1,0.36,1)] {output ===
					'separate'
						? 'translate-x-[calc(100%+0.5rem)]'
						: output === 'images'
							? 'translate-x-[calc(200%+1rem)]'
							: ''}"
				></span>
				{#each outputs as option (option.id)}<button
						type="button"
						aria-pressed={output === option.id}
						disabled={processing}
						class="relative z-10 rounded-lg px-2 py-3 text-xs font-semibold motion-safe:transition-colors {output ===
						option.id
							? 'text-merge'
							: 'text-muted hover:text-white'}"
						onclick={() => (output = option.id)}>{option.label}</button
					>{/each}
			</div>
		</div>
		{#if output === 'images'}
			<div
				transition:slide={{ duration: reducedMotion ? 0 : 220, easing: cubicOut }}
				class="space-y-6"
			>
				<div>
					<h2 class="mb-3 text-sm font-semibold">Image format</h2>
					<div
						class="grid grid-cols-2 gap-1 rounded-xl bg-canvas p-1"
						role="group"
						aria-label="Image format"
					>
						{#each formats as option (option.id)}<button
								type="button"
								aria-pressed={imageFormat === option.id}
								disabled={processing}
								class="rounded-lg px-2 py-3 text-xs font-semibold motion-safe:transition-colors {imageFormat ===
								option.id
									? 'bg-panel-hover text-merge'
									: 'text-muted hover:text-white'}"
								onclick={() => (imageFormat = option.id)}>{option.label}</button
							>{/each}
					</div>
				</div>
				<div>
					<div class="mb-3 flex items-center justify-between">
						<h2 class="text-sm font-semibold">Resolution</h2>
						<span class="text-xs text-muted"
							>{dpi === 300 ? 'Print' : dpi === 150 ? 'Standard' : 'Screen'}</span
						>
					</div>
					<div
						class="grid grid-cols-3 gap-1 rounded-xl bg-canvas p-1"
						role="group"
						aria-label="Resolution"
					>
						{#each resolutions as option (option.dpi)}<button
								type="button"
								aria-pressed={dpi === option.dpi}
								disabled={processing}
								class="rounded-lg px-2 py-3 text-xs font-semibold motion-safe:transition-colors {dpi ===
								option.dpi
									? 'bg-panel-hover text-merge'
									: 'text-muted hover:text-white'}"
								onclick={() => (dpi = option.dpi)}>{option.label}</button
							>{/each}
					</div>
				</div>
			</div>
		{/if}
	{/if}
	{#if unmatched}<p role="alert" class="text-xs text-convert">
			Enter pages between 1 and {pages.length}.
		</p>{:else if nothingLeft}<p role="alert" class="text-xs text-convert">
			At least one page must remain.
		</p>{/if}
</div>
