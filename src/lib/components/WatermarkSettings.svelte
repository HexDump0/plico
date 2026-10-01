<script lang="ts">
	import { tick } from 'svelte';
	import { cubicOut } from 'svelte/easing';
	import { fly, slide } from 'svelte/transition';
	import { IconPhotoPlus, IconReplace, IconX } from '@tabler/icons-svelte-runes';
	import type { FontFamily } from '$lib/pdf/standard-fonts';
	import { positionNames, type StampImage, type StampPosition } from '$lib/pdf/stamp-layout';
	import { formatSize } from '$lib/workspace.svelte';
	import PositionPicker from './PositionPicker.svelte';
	import StampTextSettings from './StampTextSettings.svelte';
	import ToggleSwitch from './ToggleSwitch.svelte';

	let {
		kind = $bindable<'text' | 'image'>(),
		text = $bindable<string>(),
		family = $bindable<FontFamily>(),
		bold = $bindable<boolean>(),
		size = $bindable<number>(),
		color = $bindable<string>(),
		image = $bindable<StampImage | null>(),
		imageWidth = $bindable<number>(),
		position = $bindable<StampPosition>(),
		tile = $bindable<boolean>(),
		rotation = $bindable<number>(),
		opacity = $bindable<number>(),
		undrawable,
		tooDense,
		reducedMotion,
		disabled = false
	}: {
		kind: 'text' | 'image';
		text: string;
		family: FontFamily;
		bold: boolean;
		size: number;
		color: string;
		image: StampImage | null;
		imageWidth: number;
		position: StampPosition;
		tile: boolean;
		rotation: number;
		opacity: number;
		undrawable: string | undefined;
		tooDense: boolean;
		reducedMotion: boolean;
		disabled?: boolean;
	} = $props();
	const id = $props.id();
	let picker = $state<HTMLInputElement>();
	let imageError = $state('');
	let dropping = $state(false);
	let kindSection = $state<HTMLDivElement>();
	let textField = $state<HTMLTextAreaElement>();
	let kindResize: Animation | undefined;
	const kinds = [
		{ id: 'text', label: 'Text' },
		{ id: 'image', label: 'Image' }
	] as const;

	async function choose(files: FileList | null | undefined) {
		const file = files?.[0];
		if (picker) picker.value = '';
		if (!file) return;
		if (!/^image\/(jpeg|png)$/.test(file.type) && !/\.(jpe?g|png)$/i.test(file.name)) {
			imageError = 'Choose a JPG or PNG image.';
			return;
		}
		const url = URL.createObjectURL(file);
		try {
			const element = new Image();
			element.src = url;
			await element.decode();
			if (!element.naturalWidth || !element.naturalHeight) throw new Error();
			if (image) URL.revokeObjectURL(image.url);
			image = { file, url, aspect: element.naturalHeight / element.naturalWidth };
			imageError = '';
		} catch {
			URL.revokeObjectURL(url);
			imageError = 'This image could not be read.';
		}
	}

	// One line until the text needs more, then it grows with each line.
	$effect(() => {
		void text;
		if (!textField) return;
		textField.style.height = 'auto';
		const border = textField.offsetHeight - textField.clientHeight;
		textField.style.height = `${Math.min(textField.scrollHeight + border, 144)}px`;
	});

	// The outgoing controls leave at once and the section eases between the two
	// heights, so everything below it moves in one smooth step.
	async function setKind(next: 'text' | 'image') {
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

	function clearImage() {
		if (image) URL.revokeObjectURL(image.url);
		image = null;
	}
</script>

<div class="space-y-6">
	<div
		class="relative grid grid-cols-2 gap-2 rounded-xl bg-canvas p-1"
		role="group"
		aria-label="Watermark type"
	>
		<span
			aria-hidden="true"
			class="pointer-events-none absolute inset-y-1 left-1 w-[calc((100%-1rem)/2)] rounded-lg bg-panel-hover motion-safe:transition-transform motion-safe:duration-200 motion-safe:ease-[cubic-bezier(0.22,1,0.36,1)] {kind ===
			'image'
				? 'translate-x-[calc(100%+0.5rem)]'
				: ''}"
		></span>
		{#each kinds as option (option.id)}<button
				type="button"
				aria-pressed={kind === option.id}
				{disabled}
				class="relative z-10 rounded-lg px-2 py-3 text-xs font-semibold motion-safe:transition-colors {kind ===
				option.id
					? 'text-brand'
					: 'text-muted hover:text-white'}"
				onclick={() => void setKind(option.id)}>{option.label}</button
			>{/each}
	</div>

	<div bind:this={kindSection}>
		{#if kind === 'text'}
			<div class="space-y-6" in:fly={{ y: 8, duration: reducedMotion ? 0 : 260, easing: cubicOut }}>
				<div>
					<label for="{id}-text" class="mb-3 block text-sm font-semibold">Text</label>
					<textarea
						id="{id}-text"
						bind:this={textField}
						bind:value={text}
						{disabled}
						rows="1"
						placeholder="CONFIDENTIAL"
						spellcheck="false"
						aria-invalid={!!undrawable}
						class="block w-full resize-none rounded-xl border bg-canvas px-3 py-3 text-sm text-white transition-colors duration-200 outline-none placeholder:text-white/30 disabled:opacity-50 {undrawable
							? 'border-convert/60'
							: 'border-white/10 focus:border-brand/50'}"></textarea>
					{#if undrawable}<p
							role="alert"
							transition:slide={{ duration: reducedMotion ? 0 : 180, easing: cubicOut }}
							class="pt-2 text-xs text-convert"
						>
							“{undrawable}” isn't available in the built-in PDF fonts
						</p>{/if}
				</div>
				<StampTextSettings
					bind:family
					bind:bold
					bind:size
					bind:color
					minSize={12}
					maxSize={160}
					{disabled}
				/>
			</div>
		{:else}
			<div class="space-y-6" in:fly={{ y: 8, duration: reducedMotion ? 0 : 260, easing: cubicOut }}>
				<div>
					<h2 class="mb-3 text-sm font-semibold">Image</h2>
					<input
						bind:this={picker}
						type="file"
						accept="image/jpeg,image/png,.jpg,.jpeg,.png"
						class="hidden"
						aria-label="Choose watermark image"
						onchange={() => void choose(picker?.files)}
					/>
					{#if image}
						<div
							class="flex items-center gap-3 rounded-xl border border-white/10 bg-canvas p-2 pr-2.5"
						>
							<img
								src={image.url}
								alt=""
								class="size-12 shrink-0 rounded-lg bg-white/10 object-contain p-1"
							/>
							<div class="min-w-0 flex-1">
								<p class="truncate text-sm font-medium" title={image.file.name}>
									{image.file.name}
								</p>
								<p class="text-xs text-muted">{formatSize(image.file.size)}</p>
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
								onclick={clearImage}
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
								void choose(event.dataTransfer?.files);
							}}
							class="flex w-full flex-col items-center gap-2 rounded-xl border border-dashed bg-canvas px-4 py-6 transition-colors duration-200 {dropping
								? 'border-brand/70 text-white'
								: 'border-white/15 text-muted hover:border-brand/50 hover:text-white'}"
						>
							<IconPhotoPlus size={26} stroke={1.5} />
							<span class="text-sm font-semibold">Choose image</span>
							<span class="text-xs text-muted">JPG or PNG</span>
						</button>
					{/if}
					{#if imageError}<p role="alert" class="pt-2 text-xs text-convert">{imageError}</p>{/if}
				</div>
				<div>
					<div class="mb-3 flex items-center justify-between">
						<label for="{id}-width" class="text-sm font-semibold">Size</label>
						<span class="text-xs text-muted tabular-nums">{imageWidth}% of page width</span>
					</div>
					<input
						id="{id}-width"
						type="range"
						min="5"
						max="100"
						step="5"
						bind:value={imageWidth}
						{disabled}
						class="w-full cursor-pointer accent-brand"
					/>
				</div>
			</div>
		{/if}
	</div>

	<div class="space-y-3">
		<div class="flex items-center justify-between">
			<h2 class="text-sm font-semibold">Position</h2>
			<span class="text-xs text-muted">{tile ? 'Repeated' : positionNames[position]}</span>
		</div>
		<PositionPicker bind:value={position} tiled={tile} {disabled} />
		<ToggleSwitch bind:checked={tile} label="Repeat across the page" tone="brand" />
		{#if tooDense}<p
				role="alert"
				transition:slide={{ duration: reducedMotion ? 0 : 180, easing: cubicOut }}
				class="text-xs text-convert"
			>
				Too small to repeat across this page
			</p>{/if}
	</div>

	<div>
		<div class="mb-3 flex items-center justify-between">
			<label for="{id}-rotation" class="text-sm font-semibold">Rotation</label>
			<button
				type="button"
				{disabled}
				onclick={() => (rotation = rotation === 45 ? 0 : 45)}
				title={rotation === 45 ? 'Straighten' : 'Set diagonal'}
				class="rounded-md px-1.5 py-0.5 text-xs text-muted tabular-nums transition-colors hover:bg-white/10 hover:text-white"
				>{rotation}°</button
			>
		</div>
		<input
			id="{id}-rotation"
			type="range"
			min="-90"
			max="90"
			step="5"
			bind:value={rotation}
			{disabled}
			class="w-full cursor-pointer accent-brand"
		/>
	</div>

	<div>
		<div class="mb-3 flex items-center justify-between">
			<label for="{id}-opacity" class="text-sm font-semibold">Opacity</label>
			<span class="text-xs text-muted tabular-nums">{opacity}%</span>
		</div>
		<input
			id="{id}-opacity"
			type="range"
			min="5"
			max="100"
			step="5"
			bind:value={opacity}
			{disabled}
			class="w-full cursor-pointer accent-brand"
		/>
	</div>
</div>
