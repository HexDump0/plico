<script lang="ts">
	import gsap from 'gsap';
	import { onMount, onDestroy, tick, untrack } from 'svelte';
	import {
		IconUpload,
		IconFileTypePdf,
		IconPhoto,
		IconX,
		IconPlus
	} from '@tabler/icons-svelte-runes';
	import type { CatalogTool } from '$lib/tool-catalog';
	import { getWorkspace, formatSize } from '$lib/workspace.svelte';
	import { toolInput, formatName, inputAccept, acceptsFile } from '$lib/pdf/office-conversion';
	import { focusMoved } from '$lib/focus';
	let {
		selectedTool = null,
		emptyOnly = false,
		onneedstool = () => {},
		onready
	}: {
		selectedTool?: CatalogTool | null;
		emptyOnly?: boolean;
		onneedstool?: () => void;
		onready?: () => void;
	} = $props();
	const workspace = getWorkspace();
	const isImageTool = $derived(
		selectedTool?.id === 'jpg-to-pdf' ||
			selectedTool?.id === 'scan-to-pdf' ||
			selectedTool?.id === 'png-to-pdf' ||
			selectedTool?.id === 'image-to-pdf' ||
			selectedTool?.id === 'images-to-pdf'
	);
	// Office, HTML and Markdown tools each take one file of their own type.
	const documentInput = $derived(toolInput(selectedTool?.id ?? ''));
	const inputType = $derived(documentInput ?? (isImageTool ? 'image' : 'pdf'));
	$effect(() => workspace.use(inputType));
	const single = $derived(
		!!documentInput ||
			selectedTool?.id === 'split' ||
			selectedTool?.id === 'extract' ||
			selectedTool?.id === 'remove' ||
			selectedTool?.id === 'rotate' ||
			selectedTool?.id === 'pdf-to-jpg' ||
			selectedTool?.id === 'pdf-to-png' ||
			selectedTool?.id === 'pdf-to-image' ||
			selectedTool?.id === 'pdf-to-images'
	);
	let input: HTMLInputElement;
	let list = $state<HTMLDivElement>();
	let reducedMotion = $state(false);
	// Files added to a list that is already showing scroll into view as they land.
	let listedCount = 0;
	$effect(() => {
		const count = workspace.files.length;
		if (list && listedCount > 0 && count > listedCount)
			list.scrollTo({
				top: list.scrollHeight,
				behavior: untrack(() => reducedMotion) ? 'instant' : 'smooth'
			});
		listedCount = count;
	});
	let dragging = $state(false);
	let depth = 0;
	let transitioning = $state(false);
	let emptyPanel = $state<HTMLButtonElement>();
	let filePanel = $state<HTMLDivElement>();
	let motion: gsap.core.Timeline | undefined;
	let disposed = false;
	let pendingRemoval: File | undefined;
	let removalMotion: gsap.core.Timeline | undefined;
	let removingRow: HTMLElement | undefined;
	// Where the focused remove button was, so focus can stay in the list
	// (or move to the upload area) once its row is gone.
	let refocusAt = -1;
	async function restoreFocus() {
		const index = refocusAt;
		refocusAt = -1;
		if (index < 0) return;
		await tick();
		if (focusMoved()) return;
		const buttons = list ? [...list.querySelectorAll<HTMLElement>('[data-remove]')] : [];
		const target = buttons[Math.min(index, buttons.length - 1)] ?? emptyPanel;
		if (target?.isConnected) target.focus();
	}
	function finishRemoval() {
		removalMotion?.kill();
		removalMotion = undefined;
		if (pendingRemoval) {
			workspace.remove(pendingRemoval);
			void restoreFocus();
		}
		pendingRemoval = undefined;
		if (removingRow) gsap.set(removingRow, { clearProps: 'all' });
		removingRow = undefined;
		if (filePanel) gsap.set(filePanel, { clearProps: 'opacity,transform' });
	}
	function remove(file: File, button: HTMLButtonElement) {
		finishTransition();
		finishRemoval();
		if (!workspace.files.includes(file)) return;
		refocusAt =
			document.activeElement === button && list
				? [...list.querySelectorAll('[data-remove]')].indexOf(button)
				: -1;
		if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
			workspace.remove(file);
			void restoreFocus();
			return;
		}
		pendingRemoval = file;
		removingRow = button.closest('li')!;
		const lastFile = workspace.files.length === 1;
		removalMotion = gsap.timeline({
			onComplete: async () => {
				finishRemoval();
				if (!lastFile) return;
				await tick();
				if (disposed || workspace.files.length || !emptyPanel) return;
				motion = gsap
					.timeline({ onComplete: finishTransition })
					.fromTo(
						emptyPanel,
						{ opacity: 0, scale: 0.98 },
						{ opacity: 1, scale: 1, duration: 0.24, ease: 'power2.out' }
					);
			}
		});
		if (lastFile) {
			removalMotion.to(filePanel!, { opacity: 0, y: -8, duration: 0.16, ease: 'power2.in' });
		} else {
			removalMotion.to(removingRow, { opacity: 0, x: 8, duration: 0.12, ease: 'power2.in' }).to(
				removingRow,
				{
					height: 0,
					paddingTop: 0,
					paddingBottom: 0,
					marginTop: 0,
					marginBottom: 0,
					overflow: 'hidden',
					duration: 0.2,
					ease: 'power2.inOut'
				},
				0.08
			);
		}
	}
	function finishTransition() {
		motion?.kill();
		motion = undefined;
		if (filePanel) gsap.set(filePanel, { clearProps: 'opacity,transform' });
		if (emptyPanel) gsap.set(emptyPanel, { clearProps: 'opacity,transform' });
		transitioning = false;
	}
	onMount(() => {
		const preference = window.matchMedia('(prefers-reduced-motion: reduce)');
		reducedMotion = preference.matches;
		const changed = () => {
			reducedMotion = preference.matches;
			if (preference.matches) {
				finishTransition();
				finishRemoval();
			}
		};
		preference.addEventListener('change', changed);
		return () => preference.removeEventListener('change', changed);
	});
	onDestroy(() => {
		disposed = true;
		motion?.kill();
		finishRemoval();
	});
	async function add(files: FileList) {
		finishRemoval();
		finishTransition();
		const wasEmpty = workspace.files.length === 0;
		const firstMatch = single
			? Array.from(files).find((file) => acceptsFile(file, inputType))
			: undefined;
		workspace.add(firstMatch ? [firstMatch] : files, inputType);
		input.value = '';
		if (emptyOnly) return;
		if (!wasEmpty || !workspace.files.length) return;
		if (selectedTool && onready) {
			onready();
			return;
		}
		if (!selectedTool) onneedstool();
		if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
		transitioning = true;
		await tick();
		if (disposed || !transitioning || !emptyPanel || !filePanel) return;
		motion = gsap
			.timeline({ onComplete: finishTransition })
			.to(emptyPanel, { opacity: 0, scale: 0.98, duration: 0.16, ease: 'power2.in' })
			.fromTo(
				filePanel,
				{ opacity: 0, y: 8 },
				{ opacity: 1, y: 0, duration: 0.28, ease: 'power2.out' },
				0.1
			);
	}
</script>

<input
	bind:this={input}
	type="file"
	accept={inputAccept[inputType]}
	multiple={!single}
	class="hidden"
	aria-label={single
		? `Choose a ${formatName(inputType)} file`
		: isImageTool
			? 'Choose image files'
			: 'Choose PDF files'}
	onchange={() => input.files && add(input.files)}
/>
<section
	aria-label={`${formatName(inputType)} file selection`}
	class="flex h-full min-h-64 w-full flex-col rounded-2xl bg-panel p-5 sm:p-6 {dragging
		? 'ring-2 ring-brand'
		: 'motion-safe:transition-[box-shadow] motion-safe:duration-200'}"
	ondragenter={(event) => {
		event.preventDefault();
		depth++;
		dragging = true;
	}}
	ondragover={(event) => event.preventDefault()}
	ondragleave={() => {
		depth--;
		if (depth <= 0) dragging = false;
	}}
	ondrop={(event) => {
		event.preventDefault();
		dragging = false;
		depth = 0;
		if (event.dataTransfer) add(event.dataTransfer.files);
	}}
>
	<div class="relative flex min-h-0 flex-1 flex-col">
		{#if emptyOnly || workspace.files.length === 0 || transitioning}
			<button
				bind:this={emptyPanel}
				inert={transitioning}
				class="flex min-h-52 flex-1 flex-col items-center justify-center gap-5 rounded-xl border-2 border-dashed border-muted/50 p-4 text-muted transition-colors hover:border-brand hover:bg-brand/5 hover:text-brand {transitioning
					? 'absolute inset-0'
					: ''}"
				onclick={() => input.click()}
			>
				<IconUpload size={40} stroke={1.5} /><span class="grid text-center text-xl font-medium"
					><span
						aria-hidden={dragging}
						class="col-start-1 row-start-1 {dragging ? 'invisible' : ''}"
						><span class="pointer-coarse:hidden">Drop in</span><span
							class="hidden pointer-coarse:inline">Tap to choose</span
						>
						{isImageTool
							? 'your images'
							: documentInput
								? `your ${formatName(inputType)} file`
								: 'your PDFs'}</span
					><span
						aria-hidden={!dragging}
						class="col-start-1 row-start-1 {dragging ? '' : 'invisible'}">You can let go btw</span
					></span
				>
			</button>
		{/if}
		{#if !emptyOnly && workspace.files.length}
			<div bind:this={filePanel} class="flex min-h-0 flex-1 flex-col">
				<h2 class="mb-3 text-sm font-semibold tabular-nums">
					{workspace.files.length}
					{isImageTool
						? workspace.files.length === 1
							? 'image'
							: 'images'
						: workspace.files.length === 1
							? 'PDF'
							: 'PDFs'} added
				</h2>
				<div bind:this={list} class="min-h-0 flex-1 overflow-y-auto">
					<ul class="space-y-2">
						{#each workspace.files as file (file)}
							<li class="flex items-center gap-3 rounded-xl bg-canvas/50 p-3">
								{#if isImageTool}
									<IconPhoto class="shrink-0 text-convert" size={26} />
								{:else}
									<IconFileTypePdf class="shrink-0 text-brand" size={26} />
								{/if}
								<div class="min-w-0 flex-1">
									<p class="truncate text-sm" title={file.name}>{file.name}</p>
									<p class="mt-1 text-xs text-muted">{formatSize(file.size)}</p>
								</div>
								<button
									class="rounded-md p-2 text-muted hover:text-white pointer-coarse:p-3"
									aria-label={`Remove ${file.name}`}
									data-remove
									onclick={(event) => remove(file, event.currentTarget)}><IconX size={18} /></button
								>
							</li>
						{/each}
					</ul>
					<button
						class="mt-2 flex min-h-11 w-full items-center justify-center gap-1.5 rounded-xl p-3 text-sm font-medium text-brand transition-colors hover:bg-brand/10"
						onclick={() => input.click()}><IconPlus size={18} aria-hidden="true" />Add files</button
					>
				</div>
			</div>
		{/if}
	</div>
	{#if workspace.error}<p role="status" class="mt-3 text-center text-xs text-convert">
			{workspace.error}
		</p>{/if}
</section>
