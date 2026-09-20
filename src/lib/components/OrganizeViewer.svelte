<script lang="ts">
	import {
		IconArrowLeft,
		IconArrowRight,
		IconRotate,
		IconRotateClockwise,
		IconTrash,
		IconX
	} from '@tabler/icons-svelte-runes';
	import type { PDFDocumentLoadingTask, PDFDocumentProxy } from 'pdfjs-dist';
	import type { OrganizePage } from '$lib/pdf/types';
	import OrganizeThumbnail from './OrganizeThumbnail.svelte';

	let {
		file,
		pages,
		mode,
		selected,
		processing,
		onpageschange,
		onselectionchange,
		onremove
	}: {
		file: File;
		pages: OrganizePage[];
		mode: 'organize' | 'extract' | 'remove' | 'rotate';
		selected: number[];
		processing: boolean;
		onpageschange: (pages: OrganizePage[]) => void;
		onselectionchange: (numbers: number[]) => void;
		onremove: () => void;
	} = $props();
	let pdf = $state<PDFDocumentProxy | null>(null);
	let status = $state('Loading PDF...');
	let dragging = $state<number | null>(null);
	let dropTarget = $state<number | null>(null);
	const remaining = $derived(pages.length);

	$effect(() => {
		const source = file;
		void mode;
		let cancelled = false;
		let loadingTask: PDFDocumentLoadingTask | undefined;
		pdf = null;
		status = 'Loading PDF...';
		async function load() {
			try {
				const pdfjs = await import('pdfjs-dist');
				const worker = await import('pdfjs-dist/build/pdf.worker.min.mjs?url');
				if (cancelled) return;
				pdfjs.GlobalWorkerOptions.workerSrc = worker.default;
				loadingTask = pdfjs.getDocument({ data: new Uint8Array(await source.arrayBuffer()) });
				loadingTask.onPassword = () => {
					if (!cancelled) status = 'Password-protected PDF';
					void loadingTask?.destroy();
				};
				const document = await loadingTask.promise;
				if (cancelled) return;
				pdf = document;
				status = '';
				onpageschange(
					Array.from({ length: document.numPages }, (_, index) => ({
						number: index + 1,
						rotation: 0 as const
					}))
				);
			} catch {
				if (!cancelled && status !== 'Password-protected PDF') status = 'Preview unavailable';
			}
		}
		void load();
		return () => {
			cancelled = true;
			void loadingTask?.destroy();
		};
	});

	function move(from: number, to: number) {
		if (
			mode !== 'organize' ||
			processing ||
			from < 0 ||
			from === to ||
			to < 0 ||
			to >= pages.length
		)
			return;
		const next = [...pages];
		const [page] = next.splice(from, 1);
		next.splice(to, 0, page);
		onpageschange(next);
	}

	function rotate(number: number, by: number) {
		if (processing) return;
		onpageschange(
			pages.map((page) =>
				page.number === number
					? { ...page, rotation: ((page.rotation + by + 360) % 360) as OrganizePage['rotation'] }
					: page
			)
		);
	}

	function remove(number: number) {
		if (processing || pages.length < 2) return;
		onpageschange(pages.filter((page) => page.number !== number));
	}

	function toggle(number: number) {
		if (processing) return;
		onselectionchange(
			selected.includes(number) ? selected.filter((item) => item !== number) : [...selected, number]
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
		if (processing || !pdf) return;
		onpageschange(
			Array.from({ length: pdf.numPages }, (_, index) => ({
				number: index + 1,
				rotation: 0 as const
			}))
		);
	}
</script>

<section
	aria-label={mode === 'organize'
		? 'Organize pages'
		: mode === 'extract'
			? 'Extract pages'
			: mode === 'remove'
				? 'Remove pages'
				: 'Rotate PDF'}
	class="mx-auto w-full max-w-7xl space-y-6"
>
	<div class="flex flex-wrap items-center justify-between gap-3">
		<div class="flex min-w-0 items-center gap-2">
			<p class="max-w-sm truncate text-sm font-semibold" title={file.name}>{file.name}</p>
			<span class="shrink-0 text-xs text-muted"
				>{pdf
					? mode === 'organize'
						? `${remaining} of ${pdf.numPages} pages`
						: mode === 'rotate'
							? `${pdf.numPages} pages`
							: `${selected.length} of ${pdf.numPages} selected`
					: status}</span
			>
			<button
				type="button"
				onclick={onremove}
				disabled={processing}
				class="flex size-8 shrink-0 items-center justify-center rounded-lg bg-white/10 hover:bg-merge hover:text-canvas disabled:opacity-50"
				aria-label="Remove PDF"
				title="Remove PDF"><IconX size={18} stroke={2.5} /></button
			>
		</div>
		{#if pdf && mode === 'organize'}<button
				type="button"
				onclick={reset}
				disabled={processing}
				class="rounded-lg px-3 py-2 text-xs font-semibold text-merge hover:bg-white/10 disabled:opacity-50"
				>Reset pages</button
			>{/if}
	</div>
	{#if pdf}
		<div class="flex flex-wrap items-center justify-between gap-2 text-xs text-muted">
			<p>
				{mode === 'organize'
					? 'Drag pages to reorder, or use the arrow buttons. Rotate or remove individual pages below.'
					: mode === 'extract'
						? 'Select the pages to include in the new PDF.'
						: mode === 'remove'
							? 'Select the pages to remove. At least one page must remain.'
							: 'Rotate pages below, or rotate every page at once.'}
			</p>
			{#if mode === 'extract' || mode === 'remove'}
				<div class="flex gap-2">
					<button
						type="button"
						onclick={() => onselectionchange(pages.map((page) => page.number))}
						disabled={processing}
						class="text-merge hover:underline disabled:opacity-50">Select all</button
					><button
						type="button"
						onclick={() => onselectionchange([])}
						disabled={processing}
						class="text-merge hover:underline disabled:opacity-50">Clear</button
					>
				</div>
			{:else if mode === 'rotate'}
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
			{#each pages as page, index (page.number)}
				<div
					class="min-w-0 rounded-xl border p-2.5 transition-colors {dropTarget === page.number ||
					selected.includes(page.number)
						? 'border-merge bg-merge/10'
						: 'border-white/10 bg-panel/70'}"
					draggable={mode === 'organize' && !processing}
					ondragstart={(event) => {
						dragging = page.number;
						event.dataTransfer?.setData('text/plain', String(page.number));
						if (event.dataTransfer) event.dataTransfer.effectAllowed = 'move';
					}}
					ondragover={(event) => {
						if (dragging === null) return;
						event.preventDefault();
						event.stopPropagation();
						dropTarget = page.number;
					}}
					ondrop={(event) => {
						if (dragging === null) return;
						event.preventDefault();
						event.stopPropagation();
						move(
							pages.findIndex((item) => item.number === dragging),
							index
						);
						dragging = null;
						dropTarget = null;
					}}
					ondragend={() => {
						dragging = null;
						dropTarget = null;
					}}
				>
					<OrganizeThumbnail {pdf} number={page.number} rotation={page.rotation} />
					<div class="mt-2 flex items-center justify-between gap-1 text-xs">
						<span class="font-semibold"
							>{mode === 'organize' ? `${index + 1}. ` : ''}Page {page.number}</span
						>{#if page.rotation}<span class="text-merge">{page.rotation}°</span>{/if}
					</div>
					{#if mode === 'extract' || mode === 'remove'}
						<label
							class="mt-2 flex cursor-pointer items-center gap-2 border-t border-white/10 pt-2 text-xs font-medium"
							><input
								type="checkbox"
								checked={selected.includes(page.number)}
								disabled={processing}
								onchange={() => toggle(page.number)}
								class="accent-merge"
							/>{mode === 'extract' ? 'Extract this page' : 'Remove this page'}</label
						>
					{:else}
						<div class="mt-2 flex items-center justify-between gap-1 border-t border-white/10 pt-2">
							{#if mode === 'organize'}<div class="flex gap-0.5">
									<button
										type="button"
										onclick={() => move(index, index - 1)}
										disabled={processing || index === 0}
										aria-label={`Move page ${page.number} left`}
										title="Move left"
										class="rounded-md p-1.5 hover:bg-white/10 disabled:opacity-30"
										><IconArrowLeft size={16} /></button
									>
									<button
										type="button"
										onclick={() => move(index, index + 1)}
										disabled={processing || index === pages.length - 1}
										aria-label={`Move page ${page.number} right`}
										title="Move right"
										class="rounded-md p-1.5 hover:bg-white/10 disabled:opacity-30"
										><IconArrowRight size={16} /></button
									>
								</div>{/if}
							<div class="flex gap-0.5">
								<button
									type="button"
									onclick={() => rotate(page.number, -90)}
									disabled={processing}
									aria-label={`Rotate page ${page.number} left`}
									title="Rotate left"
									class="rounded-md p-1.5 hover:bg-white/10 disabled:opacity-30"
									><IconRotate size={16} /></button
								>
								<button
									type="button"
									onclick={() => rotate(page.number, 90)}
									disabled={processing}
									aria-label={`Rotate page ${page.number} right`}
									title="Rotate right"
									class="rounded-md p-1.5 hover:bg-white/10 disabled:opacity-30"
									><IconRotateClockwise size={16} /></button
								>
								{#if mode === 'organize'}<button
										type="button"
										onclick={() => remove(page.number)}
										disabled={processing || pages.length === 1}
										aria-label={`Remove page ${page.number}`}
										title="Remove page"
										class="rounded-md p-1.5 hover:bg-convert/20 hover:text-convert disabled:opacity-30"
										><IconTrash size={16} /></button
									>{/if}
							</div>
						</div>
					{/if}
				</div>
			{/each}
		</div>
	{/if}
</section>
