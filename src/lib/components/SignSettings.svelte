<script lang="ts">
	import '@fontsource/great-vibes/400.css';
	import '@fontsource/dancing-script/400.css';
	import '@fontsource/homemade-apple/400.css';
	import { onDestroy, tick } from 'svelte';
	import { cubicOut } from 'svelte/easing';
	import { fly, slide } from 'svelte/transition';
	import { IconCheck, IconPhotoPlus, IconReplace, IconX } from '@tabler/icons-svelte-runes';
	import type { StampImage } from '$lib/pdf/stamp-layout';
	import {
		signatureFonts,
		strokesImage,
		typedImage,
		uploadedImage,
		type SignatureFont,
		type Stroke
	} from '$lib/pdf/signature';
	import { formatSize } from '$lib/workspace.svelte';
	import SignaturePad from './SignaturePad.svelte';
	import ToggleSwitch from './ToggleSwitch.svelte';

	let {
		image = $bindable<StampImage | null>(),
		pages = $bindable<'one' | 'all' | 'custom'>(),
		range = $bindable<string>(),
		page,
		pageCount,
		rangeInvalid,
		reducedMotion,
		disabled = false
	}: {
		/// The signature as it will be stamped, or null until there is one.
		image: StampImage | null;
		pages: 'one' | 'all' | 'custom';
		range: string;
		/// The page the signature sits on when only one page is signed.
		page: number;
		pageCount: number;
		rangeInvalid: boolean;
		reducedMotion: boolean;
		disabled?: boolean;
	} = $props();
	const id = $props.id();

	let kind = $state<'draw' | 'type' | 'upload'>('draw');
	let strokes = $state<Stroke[]>([]);
	let name = $state('');
	let font = $state<SignatureFont>('great-vibes');
	let color = $state('#111827');
	let upload = $state<File | null>(null);
	let clearPaper = $state(true);
	let uploadError = $state('');
	let picker = $state<HTMLInputElement>();
	let dropping = $state(false);
	let kindSection = $state<HTMLDivElement>();
	let rangeInput = $state<HTMLInputElement>();
	let kindResize: Animation | undefined;

	const kinds = [
		{ id: 'draw', label: 'Draw' },
		{ id: 'type', label: 'Type' },
		{ id: 'upload', label: 'Upload' }
	] as const;
	const scopes = [
		{ id: 'one', label: 'This page' },
		{ id: 'all', label: 'All pages' },
		{ id: 'custom', label: 'Custom' }
	] as const;
	const inks = [
		{ value: '#111827', label: 'Black' },
		{ value: '#1d4ed8', label: 'Blue' },
		{ value: '#b91c1c', label: 'Red' }
	];
	const customInk = $derived(!inks.some((ink) => ink.value === color));
	const slot = (index: number) =>
		['', 'translate-x-[calc(100%+0.5rem)]', 'translate-x-[calc(200%+1rem)]'][index];

	// Each change makes a new image; only the newest one is kept, so a slow
	// render finishing late cannot replace a newer signature.
	let generation = 0;
	async function refresh() {
		const current = ++generation;
		const next =
			kind === 'draw'
				? await strokesImage(strokes, color)
				: kind === 'type'
					? await typedImage(name, font, color)
					: upload
						? await uploadedImage(upload, clearPaper)
						: null;
		if (current !== generation) {
			if (next) URL.revokeObjectURL(next.url);
			return;
		}
		if (kind === 'upload' && upload && !next) uploadError = 'This image could not be read.';
		if (image) URL.revokeObjectURL(image.url);
		image = next;
	}

	// Typing and the color picker report every change; waiting for a pause
	// keeps the image from being rebuilt for each one.
	let pending = 0;
	$effect(() => {
		void name;
		void font;
		void color;
		void clearPaper;
		void kind;
		clearTimeout(pending);
		pending = window.setTimeout(() => void refresh(), 150);
		return () => clearTimeout(pending);
	});
	onDestroy(() => {
		generation++;
		if (image) URL.revokeObjectURL(image.url);
	});

	function choose(files: FileList | null | undefined) {
		const file = files?.[0];
		if (picker) picker.value = '';
		if (!file) return;
		if (!/^image\/(jpeg|png)$/.test(file.type) && !/\.(jpe?g|png)$/i.test(file.name)) {
			uploadError = 'Choose a JPG or PNG image.';
			return;
		}
		uploadError = '';
		upload = file;
		void refresh();
	}

	// As in Watermark: the outgoing controls leave at once and the section
	// eases between the two heights.
	async function setKind(next: typeof kind) {
		if (next === kind) return;
		const from = kindSection?.offsetHeight;
		kindResize?.cancel();
		kind = next;
		await tick();
		if (reducedMotion || !kindSection || from === undefined) return;
		const to = kindSection.offsetHeight;
		if (from === to) return;
		kindResize = kindSection.animate(
			[
				{ height: `${from}px`, overflow: 'hidden' },
				{ height: `${to}px`, overflow: 'hidden' }
			],
			{ duration: 320, easing: 'cubic-bezier(0.22, 1, 0.36, 1)' }
		);
	}

	async function setPages(next: typeof pages) {
		pages = next;
		if (next !== 'custom') range = '';
		else {
			await tick();
			rangeInput?.focus();
		}
	}
</script>

{#snippet inkPicker()}
	<div>
		<h2 class="mb-3 text-sm font-semibold">Color</h2>
		<div class="flex items-center gap-2.5" role="group" aria-label="Color">
			{#each inks as ink (ink.value)}
				<button
					type="button"
					aria-pressed={color === ink.value}
					aria-label={ink.label}
					title={ink.label}
					{disabled}
					onclick={() => (color = ink.value)}
					style:background-color={ink.value}
					class="flex size-8 items-center justify-center rounded-full ring-offset-2 ring-offset-panel motion-safe:transition-shadow {color ===
					ink.value
						? 'ring-2 ring-convert'
						: 'ring-1 ring-white/15 hover:ring-white/40'}"
				>
					{#if color === ink.value}<IconCheck size={15} stroke={3} class="text-white" />{/if}
				</button>
			{/each}
			<label
				title="Custom color"
				class="relative flex size-8 cursor-pointer items-center justify-center rounded-full ring-offset-2 ring-offset-panel has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-offset-6 has-[:focus-visible]:outline-convert motion-safe:transition-shadow {customInk
					? 'ring-2 ring-convert'
					: 'ring-1 ring-white/15 hover:ring-white/40'}"
				style:background={customInk
					? color
					: 'conic-gradient(from 180deg, #f87171, #fbbf24, #34d399, #60a5fa, #a78bfa, #f472b6, #f87171)'}
			>
				<input
					type="color"
					value={color}
					{disabled}
					aria-label="Custom color"
					oninput={(event) => (color = event.currentTarget.value)}
					class="absolute inset-0 size-full cursor-pointer opacity-0"
				/>
			</label>
		</div>
	</div>
{/snippet}

<div class="space-y-6">
	<div
		class="relative grid grid-cols-3 gap-2 rounded-xl bg-canvas p-1"
		role="group"
		aria-label="Signature"
	>
		<span
			aria-hidden="true"
			class="pointer-events-none absolute inset-y-1 left-1 w-[calc((100%-1.5rem)/3)] rounded-lg bg-panel-hover motion-safe:transition-transform motion-safe:duration-200 motion-safe:ease-[cubic-bezier(0.22,1,0.36,1)] {slot(
				kinds.findIndex((option) => option.id === kind)
			)}"
		></span>
		{#each kinds as option (option.id)}<button
				type="button"
				aria-pressed={kind === option.id}
				{disabled}
				class="relative z-10 rounded-lg px-2 py-3 text-xs font-semibold motion-safe:transition-colors {kind ===
				option.id
					? 'text-convert'
					: 'text-muted hover:text-white'}"
				onclick={() => void setKind(option.id)}>{option.label}</button
			>{/each}
	</div>

	<div bind:this={kindSection}>
		{#if kind === 'draw'}
			<div class="space-y-6" in:fly={{ y: 8, duration: reducedMotion ? 0 : 260, easing: cubicOut }}>
				<SignaturePad bind:strokes {color} {disabled} onfinish={() => void refresh()} />
				{@render inkPicker()}
			</div>
		{:else if kind === 'type'}
			<div class="space-y-6" in:fly={{ y: 8, duration: reducedMotion ? 0 : 260, easing: cubicOut }}>
				<div>
					<label for="{id}-name" class="mb-3 block text-sm font-semibold">Name</label>
					<input
						id="{id}-name"
						type="text"
						bind:value={name}
						{disabled}
						autocomplete="name"
						spellcheck="false"
						placeholder="Your name"
						class="block w-full rounded-xl border border-white/10 bg-canvas px-3 py-3 text-sm text-white outline-none placeholder:text-white/30 focus:border-convert/50 disabled:opacity-50"
					/>
				</div>
				<div>
					<h2 class="mb-3 text-sm font-semibold">Style</h2>
					<div class="grid gap-1 rounded-xl bg-canvas p-1" role="group" aria-label="Style">
						{#each signatureFonts as option (option.id)}<button
								type="button"
								aria-pressed={font === option.id}
								aria-label={option.label}
								{disabled}
								style:font-family={option.css}
								class="truncate rounded-lg px-3 py-2 text-left text-2xl leading-normal motion-safe:transition-colors {font ===
								option.id
									? 'bg-panel-hover text-white'
									: 'text-muted hover:text-white'}"
								onclick={() => (font = option.id)}>{name.trim() || 'Your name'}</button
							>{/each}
					</div>
				</div>
				{@render inkPicker()}
			</div>
		{:else}
			<div class="space-y-4" in:fly={{ y: 8, duration: reducedMotion ? 0 : 260, easing: cubicOut }}>
				<input
					bind:this={picker}
					type="file"
					accept="image/jpeg,image/png,.jpg,.jpeg,.png"
					class="hidden"
					aria-label="Choose signature image"
					onchange={() => choose(picker?.files)}
				/>
				{#if upload}
					<div
						class="flex items-center gap-3 rounded-xl border border-white/10 bg-canvas p-2 pr-2.5"
					>
						{#if image}<img
								src={image.url}
								alt=""
								class="size-12 shrink-0 rounded-lg bg-white object-contain p-1"
							/>{/if}
						<div class="min-w-0 flex-1">
							<p class="truncate text-sm font-medium" title={upload.name}>{upload.name}</p>
							<p class="text-xs text-muted">{formatSize(upload.size)}</p>
						</div>
						<button
							type="button"
							{disabled}
							onclick={() => picker?.click()}
							aria-label="Replace image"
							title="Replace image"
							class="flex size-8 shrink-0 items-center justify-center rounded-lg text-muted transition-colors hover:bg-white/10 hover:text-white"
							><IconReplace size={17} /></button
						>
						<button
							type="button"
							{disabled}
							onclick={() => {
								upload = null;
								void refresh();
							}}
							aria-label="Remove image"
							title="Remove image"
							class="flex size-8 shrink-0 items-center justify-center rounded-lg text-muted transition-colors hover:bg-convert hover:text-canvas"
							><IconX size={17} stroke={2.25} /></button
						>
					</div>
				{:else}
					<button
						type="button"
						{disabled}
						onclick={() => picker?.click()}
						ondragover={(event) => {
							event.preventDefault();
							event.stopPropagation();
							dropping = true;
						}}
						ondragleave={() => (dropping = false)}
						ondrop={(event) => {
							event.preventDefault();
							event.stopPropagation();
							dropping = false;
							choose(event.dataTransfer?.files);
						}}
						class="flex w-full flex-col items-center gap-2 rounded-xl border border-dashed bg-canvas px-4 py-6 transition-colors duration-200 {dropping
							? 'border-convert/70 text-white'
							: 'border-white/15 text-muted hover:border-convert/50 hover:text-white'}"
					>
						<IconPhotoPlus size={26} stroke={1.5} />
						<span class="text-sm font-semibold">Choose image</span>
						<span class="text-xs text-muted">JPG or PNG</span>
					</button>
				{/if}
				{#if uploadError}<p role="alert" class="text-xs text-convert">{uploadError}</p>{/if}
				<ToggleSwitch bind:checked={clearPaper} label="Remove white background" tone="convert" />
			</div>
		{/if}
	</div>

	<div class="space-y-3">
		<div class="flex items-center justify-between">
			<h2 class="text-sm font-semibold">Pages</h2>
			{#if pages === 'one' && pageCount}<span class="text-xs text-muted tabular-nums"
					>Page {page}</span
				>{/if}
		</div>
		<div
			class="relative grid grid-cols-3 gap-2 rounded-xl bg-canvas p-1"
			role="group"
			aria-label="Pages"
		>
			<span
				aria-hidden="true"
				class="pointer-events-none absolute inset-y-1 left-1 w-[calc((100%-1.5rem)/3)] rounded-lg bg-panel-hover motion-safe:transition-transform motion-safe:duration-200 motion-safe:ease-[cubic-bezier(0.22,1,0.36,1)] {slot(
					scopes.findIndex((option) => option.id === pages)
				)}"
			></span>
			{#each scopes as option (option.id)}<button
					type="button"
					aria-pressed={pages === option.id}
					{disabled}
					class="relative z-10 rounded-lg px-2 py-3 text-xs font-semibold whitespace-nowrap motion-safe:transition-colors {pages ===
					option.id
						? 'text-convert'
						: 'text-muted hover:text-white'}"
					onclick={() => void setPages(option.id)}>{option.label}</button
				>{/each}
		</div>
		{#if pages === 'custom'}
			<div transition:slide={{ duration: reducedMotion ? 0 : 220, easing: cubicOut }}>
				<div
					class="flex items-center gap-2 rounded-xl border bg-canvas px-3 transition-colors duration-200 {rangeInvalid
						? 'border-convert/60'
						: 'border-white/10 focus-within:border-convert/50'}"
				>
					<input
						bind:this={rangeInput}
						type="text"
						inputmode="numeric"
						autocomplete="off"
						bind:value={range}
						{disabled}
						placeholder={pageCount > 1 ? `e.g. 1, ${pageCount}` : 'e.g. 1'}
						aria-label="Pages to sign"
						aria-invalid={rangeInvalid}
						class="min-w-0 flex-1 bg-transparent py-3 text-sm text-white outline-none placeholder:text-white/30 disabled:opacity-50"
					/>{#if pageCount}<span class="shrink-0 text-xs text-muted">of {pageCount}</span>{/if}
				</div>
				{#if rangeInvalid}<p role="alert" class="pt-2 text-xs text-convert">
						Enter pages between 1 and {pageCount}.
					</p>{/if}
			</div>
		{/if}
	</div>
</div>
