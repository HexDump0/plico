<script lang="ts">
	import gsap from 'gsap';
	import { onMount, onDestroy, tick } from 'svelte';
	import {
		IconUpload,
		IconFileTypePdf,
		IconPhoto,
		IconX,
		IconPlus
	} from '@tabler/icons-svelte-runes';
	import type { CatalogTool } from '$lib/tool-catalog';
	import { getWorkspace, formatSize } from '$lib/workspace.svelte';
	import {
		officeOperation,
		officeTools,
		inputAccept,
		acceptsFile
	} from '$lib/pdf/office-conversion';
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
			selectedTool?.id === 'png-to-pdf' ||
			selectedTool?.id === 'image-to-pdf' ||
			selectedTool?.id === 'images-to-pdf'
	);
	const officeTool = $derived(officeOperation(selectedTool?.id ?? ''));
	const inputType = $derived(
		officeTool ? officeTools[officeTool].input : isImageTool ? 'image' : 'pdf'
	);
	$effect(() => workspace.use(inputType));
	const single = $derived(
		!!officeTool ||
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
	function finishRemoval() {
		removalMotion?.kill();
		removalMotion = undefined;
		if (pendingRemoval) workspace.remove(pendingRemoval);
		pendingRemoval = undefined;
		if (removingRow) gsap.set(removingRow, { clearProps: 'all' });
		removingRow = undefined;
		if (filePanel) gsap.set(filePanel, { clearProps: 'opacity,transform' });
	}
	function remove(file: File, button: HTMLButtonElement) {
		finishTransition();
		finishRemoval();
		if (!workspace.files.includes(file)) return;
		if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
			workspace.remove(file);
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
	export function flash() {
		// not really a flash
		// if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
		if (!emptyPanel) return;
		gsap.killTweensOf(emptyPanel);
		const computed = getComputedStyle(emptyPanel);
		const r = parseFloat(computed.borderTopLeftRadius) || 12;
		// const flashTint = lighten(tint(), 0.35);
		gsap.set(emptyPanel, { transition: 'none' });
		gsap
			.timeline({
				onComplete: () => {
					if (emptyPanel)
						gsap.set(emptyPanel, {
							clearProps: 'transform,borderRadius,transition'
						});
				}
			})

			// "stomp" down: squash flat, pinch the corners, tilt
			.to(emptyPanel, {
				scale: 0.92,
				scaleY: 0.74,
				rotate: -1,
				borderRadius: `${r * 1.9}px ${r * 0.5}px ${r * 1.9}px ${r * 0.5}px`,
				duration: 0.14,
				ease: 'power2.in'
			})
			// little teeny recoil after: overshoot back, corners should sway then settle
			.to(emptyPanel, {
				scale: 1,
				scaleY: 1,
				rotate: 0,
				borderRadius: `${r}px`,
				duration: 0.7,
				ease: 'elastic.out(1, 0.35)'
			});
	}
	onMount(() => {
		const preference = window.matchMedia('(prefers-reduced-motion: reduce)');
		const changed = () => {
			if (preference.matches) {
				finishTransition();
				finishRemoval();
			}
		};
		preference.addEventListener('change', changed);
		input.addEventListener('cancel', flash);
		return () => {
			preference.removeEventListener('change', changed);
			input.removeEventListener('cancel', flash);
		};
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
		? `Choose a ${inputType.toUpperCase()} file`
		: isImageTool
			? 'Choose image files'
			: 'Choose PDF files'}
	onchange={() => input.files && add(input.files)}
/>
<section
	aria-label={`${inputType.toUpperCase()} file selection`}
	class="flex h-full min-h-64 w-full flex-col rounded-2xl bg-panel p-5 sm:p-6 {dragging
		? 'ring-2 ring-brand'
		: ''}"
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
				<IconUpload size={40} stroke={1.5} /><span class="text-xl font-medium"
					>{dragging
						? 'You can let go btw'
						: isImageTool
							? 'Drop in your images'
							: officeTool
								? `Drop in your ${inputType.toUpperCase()} file`
								: 'Drop in your PDFs'}</span
				>
			</button>
		{/if}
		{#if !emptyOnly && workspace.files.length}
			<div bind:this={filePanel} class="flex min-h-0 flex-1 flex-col">
				<div class="mb-3 flex items-center justify-between gap-2">
					<h2 class="text-sm font-semibold">
						{workspace.files.length}
						{isImageTool
							? workspace.files.length === 1
								? 'image'
								: 'images'
							: workspace.files.length === 1
								? 'PDF'
								: 'PDFs'} added
					</h2>
					<button
						class="flex items-center gap-1 rounded-lg p-2 text-sm text-brand hover:bg-brand/10"
						onclick={() => input.click()}><IconPlus size={18} /> Add files</button
					>
				</div>
				<ul class="min-h-0 flex-1 space-y-2 overflow-y-auto">
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
								class="rounded-md p-2 text-muted hover:text-white"
								aria-label={`Remove ${file.name}`}
								onclick={(event) => remove(file, event.currentTarget)}><IconX size={18} /></button
							>
						</li>
					{/each}
				</ul>
			</div>
		{/if}
	</div>
	{#if workspace.error}<p role="status" class="mt-3 text-center text-xs text-convert">
			{workspace.error}
		</p>{/if}
</section>
