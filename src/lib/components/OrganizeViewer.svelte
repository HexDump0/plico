<script lang="ts">
	import { onDestroy, untrack } from 'svelte';
	import { fade } from 'svelte/transition';
	import {
		IconPlus,
		IconRefresh,
		IconRotate,
		IconRotateClockwise,
		IconTrash
	} from '@tabler/icons-svelte-runes';
	import type { PDFDocumentLoadingTask, PDFDocumentProxy } from 'pdfjs-dist';
	import type { OrganizePage } from '$lib/pdf/types';
	import { pageKey, sourceColor, sourceKey } from '$lib/pdf/sources';
	import OrganizeThumbnail from './OrganizeThumbnail.svelte';
	import OrganizePageCards from './OrganizePageCards.svelte';

	type Source = {
		key: string;
		file: File;
		pdf: PDFDocumentProxy | null;
		task: PDFDocumentLoadingTask | null;
		loading: boolean;
		failed: boolean;
		status: string;
	};

	let {
		files,
		pages,
		mode,
		selected,
		processing,
		reducedMotion,
		onpageschange,
		onselectionchange,
		onadd,
		onremove
	}: {
		files: File[];
		pages: OrganizePage[];
		mode: 'organize' | 'extract' | 'remove' | 'rotate';
		selected: string[];
		processing: boolean;
		reducedMotion: boolean;
		onpageschange: (pages: OrganizePage[]) => void;
		onselectionchange: (keys: string[]) => void;
		onadd: () => void;
		onremove: (file: File) => void;
	} = $props();

	let sources = $state<Source[]>([]);
	// Extract, Remove, and Rotate stay single-document tools; only Organize
	// builds one output from several inputs.
	const activeFiles = $derived(mode === 'organize' ? files : files.slice(0, 1));
	const loadedCount = $derived(sources.filter((source) => source.pdf).length);
	const status = $derived(
		sources.length === 0
			? ''
			: loadedCount > 0
				? ''
				: (sources.find((source) => source.failed)?.status ?? 'Loading PDFs...')
	);

	$effect(() => {
		const wanted = activeFiles;
		const keys = wanted.map((file) => sourceKey(file));
		const previous = untrack(() => sources);
		const next = wanted.map((file) => {
			const key = sourceKey(file);
			return (
				previous.find((source) => source.key === key) ?? {
					key,
					file,
					pdf: null,
					task: null,
					loading: false,
					failed: false,
					status: ''
				}
			);
		});
		sources = next;
		for (const stale of previous) {
			if (!next.includes(stale)) void stale.task?.destroy();
		}
		const known = new Set(keys);
		untrack(() => {
			if (pages.some((page) => !known.has(page.source)))
				onpageschange(pages.filter((page) => known.has(page.source)));
		});
		for (const source of next) {
			if (!source.pdf && !source.failed && !source.loading) void load(source);
		}
	});

	async function load(source: Source) {
		const start = sources.find((entry) => entry.key === source.key);
		if (!start || start.pdf || start.failed || start.loading) return;
		start.loading = true;
		try {
			const pdfjs = await import('pdfjs-dist');
			const worker = await import('pdfjs-dist/build/pdf.worker.min.mjs?url');
			pdfjs.GlobalWorkerOptions.workerSrc = worker.default;
			const task: PDFDocumentLoadingTask = pdfjs.getDocument({
				data: new Uint8Array(await source.file.arrayBuffer())
			});
			task.onPassword = () => {
				const current = sources.find((entry) => entry.key === source.key);
				if (current) {
					current.failed = true;
					current.status = 'Password-protected PDF';
				}
				void task.destroy();
			};
			const pdf = await task.promise;
			const current = sources.find((entry) => entry.key === source.key);
			if (!current) {
				void task.destroy();
				return;
			}
			current.task = task;
			current.pdf = pdf;
			if (!pages.some((page) => page.source === source.key))
				onpageschange([
					...pages,
					...Array.from({ length: pdf.numPages }, (_, index) => ({
						source: source.key,
						number: index + 1,
						rotation: 0 as const
					}))
				]);
		} catch {
			const current = sources.find((entry) => entry.key === source.key);
			if (current) {
				current.loading = false;
				if (!current.failed) {
					current.failed = true;
					current.status = 'Preview unavailable';
				}
			}
		}
	}

	function sourceOf(page: OrganizePage) {
		return sources.find((source) => source.key === page.source);
	}

	function colorOf(page: OrganizePage) {
		return sourceColor(
			Math.max(
				0,
				sources.findIndex((source) => source.key === page.source)
			)
		);
	}

	function pagesOf(key: string) {
		return pages.filter((page) => page.source === key).length;
	}

	function removeSource(key: string) {
		const source = sources.find((entry) => entry.key === key);
		if (source) onremove(source.file);
	}

	function rotate(page: OrganizePage, by: number) {
		if (processing) return;
		onpageschange(
			pages.map((entry) =>
				pageKey(entry) === pageKey(page)
					? {
							...entry,
							rotation: ((entry.rotation + by + 360) % 360) as OrganizePage['rotation']
						}
					: entry
			)
		);
	}

	function toggle(page: OrganizePage) {
		if (processing) return;
		const key = pageKey(page);
		onselectionchange(
			selected.includes(key) ? selected.filter((item) => item !== key) : [...selected, key]
		);
	}

	function rotateAll(by: number) {
		if (processing) return;
		onpageschange(
			pages.map((page) => ({
				...page,
				rotation: ((page.rotation + by + 360) % 360) as OrganizePage['rotation']
			}))
		);
	}

	function reset() {
		if (processing) return;
		onpageschange(
			sources.flatMap((source) =>
				source.pdf
					? Array.from({ length: source.pdf.numPages }, (_, index) => ({
							source: source.key,
							number: index + 1,
							rotation: 0 as const
						}))
					: []
			)
		);
	}

	onDestroy(() => {
		for (const source of sources) void source.task?.destroy();
	});
</script>

<section
	aria-label={mode === 'organize'
		? 'Organize pages'
		: mode === 'extract'
			? 'Extract pages'
			: mode === 'remove'
				? 'Remove pages'
				: 'Rotate PDF'}
	class="mx-auto flex w-full max-w-7xl flex-1 flex-col gap-6"
>
	{#if mode === 'organize'}
		<div class="flex flex-wrap items-start justify-between gap-3">
			<ul
				class="flex min-w-0 flex-wrap items-center gap-x-4 gap-y-2"
				aria-label="PDFs in this project"
			>
				{#each sources as source, index (source.key)}
					<li
						in:fade={{ duration: reducedMotion ? 0 : 240 }}
						out:fade={{ duration: reducedMotion ? 0 : 140 }}
						class="flex min-w-0 items-center gap-2 text-sm"
					>
						<span class="{sourceColor(index).dot} size-2.5 shrink-0 rounded-full" aria-hidden="true"
						></span>
						<span class="max-w-40 truncate font-medium" title={source.file.name}
							>{source.file.name}</span
						>
						<span class="shrink-0 text-xs text-muted"
							>{source.pdf
								? `${pagesOf(source.key)}/${source.pdf.numPages} pages`
								: source.status || 'Loading...'}</span
						>
						<button
							type="button"
							onclick={() => removeSource(source.key)}
							disabled={processing}
							class="flex size-7 shrink-0 items-center justify-center rounded-lg text-muted transition-colors hover:bg-convert/20 hover:text-convert disabled:opacity-40"
							aria-label={`Remove ${source.file.name}`}
							title="Remove PDF"><IconTrash size={16} stroke={1.8} /></button
						>
					</li>
				{/each}
			</ul>
			<div class="flex shrink-0 items-center gap-2">
				<button
					type="button"
					onclick={onadd}
					disabled={processing}
					class="flex items-center gap-1.5 rounded-xl border-2 border-white/10 bg-panel px-3 py-2 text-xs font-semibold text-muted transition-colors hover:border-merge/40 hover:text-merge disabled:opacity-40"
					><IconPlus size={16} /> Add PDF</button
				>
				<button
					type="button"
					onclick={reset}
					disabled={processing}
					class="flex size-10 shrink-0 items-center justify-center rounded-xl border-2 border-white/10 bg-panel text-muted transition-colors hover:border-merge/40 hover:text-merge disabled:opacity-40"
					aria-label="Reset pages"
					title="Reset pages"><IconRefresh size={18} /></button
				>
			</div>
		</div>
		{#if pages.length}
			<OrganizePageCards
				{sources}
				{pages}
				{processing}
				{reducedMotion}
				{onpageschange}
				onremovesource={removeSource}
			/>
		{:else}
			<p role="status" class="py-16 text-center text-sm text-muted">{status}</p>
		{/if}
	{:else}
		<div class="flex flex-wrap items-center justify-between gap-3">
			<div class="flex min-w-0 items-center gap-2">
				<p class="max-w-sm truncate text-sm font-semibold" title={files[0]?.name}>
					{files[0]?.name}
				</p>
				<span class="shrink-0 text-xs text-muted"
					>{loadedCount
						? mode === 'rotate'
							? `${pages.length} pages`
							: `${selected.length} of ${pages.length} selected`
						: status}</span
				>
				<button
					type="button"
					onclick={() => files[0] && onremove(files[0])}
					disabled={processing}
					class="flex size-8 shrink-0 items-center justify-center rounded-lg bg-white/10 text-muted transition-colors hover:bg-convert hover:text-canvas disabled:opacity-50"
					aria-label="Remove PDF"
					title="Remove PDF"><IconTrash size={18} stroke={1.8} /></button
				>
			</div>
		</div>
		<div class="flex flex-wrap items-center justify-between gap-2 text-xs text-muted">
			<p>
				{mode === 'extract'
					? 'Select the pages to include in the new PDF.'
					: mode === 'remove'
						? 'Select the pages to remove. At least one page must remain.'
						: 'Rotate pages below, or rotate every page at once.'}
			</p>
			{#if mode === 'extract' || mode === 'remove'}
				<div class="flex gap-2">
					<button
						type="button"
						onclick={() => onselectionchange(pages.map(pageKey))}
						disabled={processing}
						class="text-merge hover:underline disabled:opacity-50">Select all</button
					><button
						type="button"
						onclick={() => onselectionchange([])}
						disabled={processing}
						class="text-merge hover:underline disabled:opacity-50">Clear</button
					>
				</div>
			{:else}
				<div class="flex gap-2">
					<button
						type="button"
						onclick={() => rotateAll(-90)}
						disabled={processing}
						class="text-merge hover:underline disabled:opacity-50">Rotate all left</button
					><button
						type="button"
						onclick={() => rotateAll(90)}
						disabled={processing}
						class="text-merge hover:underline disabled:opacity-50">Rotate all right</button
					><button
						type="button"
						onclick={reset}
						disabled={processing}
						class="text-merge hover:underline disabled:opacity-50">Reset</button
					>
				</div>
			{/if}
		</div>
		<div class="grid grid-cols-2 gap-3 sm:grid-cols-3 xl:grid-cols-4 2xl:grid-cols-5">
			{#each pages as page (pageKey(page))}
				{@const source = sourceOf(page)}
				{@const color = colorOf(page)}
				<div
					class="relative min-w-0 overflow-hidden rounded-xl border-2 bg-panel shadow-lg shadow-black/20 {selected.includes(
						pageKey(page)
					)
						? color.strong
						: color.border}"
				>
					{#if source?.pdf}<OrganizeThumbnail
							pdf={source.pdf}
							number={page.number}
							rotation={page.rotation}
						/>{/if}
					<span
						class="{color.dot} absolute top-2 left-2 flex min-w-7 items-center justify-center rounded-md px-1.5 py-1 text-[11px] font-bold text-canvas backdrop-blur-sm"
						>{page.number}</span
					>
					{#if mode === 'extract' || mode === 'remove'}
						<label class="absolute inset-0 cursor-pointer"
							><span class="sr-only"
								>{mode === 'extract' ? 'Extract' : 'Remove'} page {page.number}</span
							><input
								type="checkbox"
								checked={selected.includes(pageKey(page))}
								disabled={processing}
								onchange={() => toggle(page)}
								class="absolute top-2 right-2 size-6 accent-merge"
							/></label
						>
					{:else}
						<div class="absolute right-2 bottom-2 flex gap-1">
							<button
								type="button"
								onclick={() => rotate(page, -90)}
								disabled={processing}
								aria-label={`Rotate page ${page.number} left`}
								title="Rotate left"
								class="flex size-8 items-center justify-center rounded-lg bg-canvas/80 text-white backdrop-blur-sm hover:bg-merge hover:text-canvas disabled:opacity-40"
								><IconRotate size={17} /></button
							>
							<button
								type="button"
								onclick={() => rotate(page, 90)}
								disabled={processing}
								aria-label={`Rotate page ${page.number} right`}
								title="Rotate right"
								class="flex size-8 items-center justify-center rounded-lg bg-canvas/80 text-white backdrop-blur-sm hover:bg-merge hover:text-canvas disabled:opacity-40"
								><IconRotateClockwise size={17} /></button
							>
						</div>
					{/if}
				</div>
			{/each}
		</div>
	{/if}
</section>
