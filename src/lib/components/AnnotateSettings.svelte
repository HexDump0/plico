<script lang="ts">
	import { tick, untrack } from 'svelte';
	import { cubicOut } from 'svelte/easing';
	import { fly, slide } from 'svelte/transition';
	import {
		IconArrowBackUp,
		IconArrowNarrowRight,
		IconCheck,
		IconCircle,
		IconColorPicker,
		IconCursorText,
		IconEraser,
		IconHighlight,
		IconLine,
		IconMessage,
		IconPencil,
		IconPhoto,
		IconPhotoPlus,
		IconPointer,
		IconReplace,
		IconRestore,
		IconShape,
		IconSquare,
		IconStrikethrough,
		IconTextSize,
		IconTrash,
		IconUnderline,
		IconWaveSine,
		IconX
	} from '@tabler/icons-svelte-runes';
	import { groupOf, hexColor, scaled, translate, type AnnotateTool } from '$lib/pdf/annotate';
	import type { AnnotateEditor } from '$lib/pdf/annotate-editor.svelte';
	import { formatSize } from '$lib/workspace.svelte';
	import StampTextSettings from './StampTextSettings.svelte';
	import ToggleSwitch from './ToggleSwitch.svelte';

	let {
		editor,
		page,
		reducedMotion,
		disabled = false
	}: {
		editor: AnnotateEditor;
		/// The page on screen, where a chosen picture goes first.
		page: number;
		reducedMotion: boolean;
		disabled?: boolean;
	} = $props();
	const id = $props.id();

	const annotateTools = [
		{ id: 'select', label: 'Select', icon: IconPointer },
		{ id: 'markup', label: 'Mark up text', icon: IconHighlight },
		{ id: 'ink', label: 'Draw', icon: IconPencil },
		{ id: 'shape', label: 'Shapes', icon: IconShape },
		{ id: 'text', label: 'Text box', icon: IconTextSize },
		{ id: 'note', label: 'Note', icon: IconMessage },
		{ id: 'image', label: 'Image', icon: IconPhoto }
	] as const;
	// Text both edits what the page says and adds to it.
	const editTools = [
		{ id: 'select', label: 'Select', icon: IconPointer },
		{ id: 'text', label: 'Text', icon: IconCursorText },
		{ id: 'image', label: 'Image', icon: IconPhoto },
		{ id: 'shape', label: 'Shapes', icon: IconShape },
		{ id: 'ink', label: 'Draw', icon: IconPencil },
		{ id: 'erase', label: 'Erase', icon: IconEraser }
	] as const;
	const tools = $derived(editor.mode === 'edit' ? editTools : annotateTools);
	const markups = [
		{ id: 'highlight', label: 'Highlight', icon: IconHighlight },
		{ id: 'underline', label: 'Underline', icon: IconUnderline },
		{ id: 'strikeout', label: 'Strikethrough', icon: IconStrikethrough },
		{ id: 'squiggly', label: 'Squiggly', icon: IconWaveSine }
	] as const;
	const shapes = [
		{ id: 'rectangle', label: 'Rectangle', icon: IconSquare },
		{ id: 'ellipse', label: 'Ellipse', icon: IconCircle },
		{ id: 'line', label: 'Line', icon: IconLine },
		{ id: 'arrow', label: 'Arrow', icon: IconArrowNarrowRight }
	] as const;
	const colors = [
		{ value: '#facc15', label: 'Yellow' },
		{ value: '#4ade80', label: 'Green' },
		{ value: '#60a5fa', label: 'Blue' },
		{ value: '#dc2626', label: 'Red' },
		{ value: '#000000', label: 'Black' }
	];
	const widths = [1, 2, 4, 8];

	const group = $derived(editor.group);
	const values = $derived(editor.values);
	const toolIndex = $derived(tools.findIndex((tool) => tool.id === editor.tool));
	const customColor = $derived(!colors.some((color) => color.value === values.color));
	const isShapeArea = $derived(values.kind === 'rectangle' || values.kind === 'ellipse');
	const image = $derived(editor.image === null ? undefined : editor.images[editor.image]);
	const styleName = $derived(
		group === 'markup'
			? markups.find((option) => option.id === values.kind)?.label
			: group === 'shape'
				? shapes.find((option) => option.id === values.kind)?.label
				: undefined
	);

	let picker = $state<HTMLInputElement>();
	let imageError = $state('');
	let dropping = $state(false);
	let section = $state<HTMLDivElement>();
	let resize: Animation | undefined;

	// As in Watermark: the outgoing controls leave at once and the section
	// eases between the two heights.
	let shownGroup = untrack(() => group);
	$effect.pre(() => {
		const next = group;
		untrack(() => {
			if (next === shownGroup) return;
			shownGroup = next;
			const from = section?.offsetHeight;
			resize?.cancel();
			void tick().then(() => {
				if (reducedMotion || !section || from === undefined) return;
				const to = section.offsetHeight;
				if (from === to) return;
				resize = section.animate(
					[
						{ height: `${from}px`, overflow: 'hidden' },
						{ height: `${to}px`, overflow: 'hidden' }
					],
					{ duration: 320, easing: 'cubic-bezier(0.22, 1, 0.36, 1)' }
				);
			});
		});
	});

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
			imageError = '';
			editor.addImage({ file, url, aspect: element.naturalHeight / element.naturalWidth }, page);
		} catch {
			URL.revokeObjectURL(url);
			imageError = 'This image could not be read.';
		}
	}

	// Ctrl+Z undoes; with a mark selected, arrows move it two points (twenty
	// with Shift), plus and minus resize it, Delete removes it, or deletes
	// the text of a line the page had.
	function key(event: KeyboardEvent) {
		if (disabled) return;
		if ((event.target as HTMLElement).closest('input, textarea, select, [contenteditable]')) return;
		if ((event.ctrlKey || event.metaKey) && !event.shiftKey && event.key.toLowerCase() === 'z') {
			event.preventDefault();
			editor.undo();
			return;
		}
		const mark = editor.selectedMark;
		const size = mark && editor.sizes[mark.page];
		if (!mark || !size || event.ctrlKey || event.metaKey || event.altKey) return;
		const fixed = groupOf(mark.kind) === 'markup';
		const step = event.shiftKey ? 20 : 2;
		const moves: Record<string, [number, number]> = {
			ArrowLeft: [-step / size.width, 0],
			ArrowRight: [step / size.width, 0],
			ArrowUp: [0, -step / size.height],
			ArrowDown: [0, step / size.height]
		};
		if (moves[event.key] && !fixed) {
			event.preventDefault();
			editor.snapshot();
			editor.change(mark.id, translate(mark, ...moves[event.key], size, editor.aspect(mark)));
		} else if (['+', '=', '-'].includes(event.key) && !fixed && mark.kind !== 'note') {
			event.preventDefault();
			editor.snapshot();
			const factor = event.key === '-' ? 0.9 : 1.1;
			editor.change(mark.id, scaled(mark, factor, size, editor.aspect(mark)));
		} else if (event.key === 'Delete' || event.key === 'Backspace') {
			event.preventDefault();
			// What the page had is deleted, not restored; its remove button
			// restores it.
			if (mark.kind === 'picture' || mark.replaces) editor.discard(mark.id);
			else editor.remove(mark.id);
		} else if (event.key === 'Escape') {
			editor.select(null);
			(document.activeElement as HTMLElement | null)?.blur();
		}
	}

	// Whether dark marks read better than light ones on a colour.
	const lightColor = (color: number) =>
		0.299 * (color >> 16) + 0.587 * ((color >> 8) & 255) + 0.114 * (color & 255) > 150;

	const pill =
		'pointer-events-none absolute inset-y-1 left-1 rounded-lg bg-panel-hover motion-safe:transition-transform motion-safe:duration-200 motion-safe:ease-[cubic-bezier(0.22,1,0.36,1)]';
	const action =
		'flex items-center justify-center gap-2 rounded-lg px-2 py-3 text-xs font-semibold text-muted enabled:hover:bg-panel-hover enabled:hover:text-white disabled:opacity-40 motion-safe:transition-colors';
</script>

<svelte:window onkeydown={key} />

{#snippet choices(
	label: string,
	options: readonly { id: string; label: string; icon: typeof IconPointer }[],
	current: string | undefined,
	onpick: (id: string) => void
)}
	{@const index = Math.max(
		0,
		options.findIndex((option) => option.id === current)
	)}
	<div
		class="relative grid gap-1 rounded-xl bg-canvas p-1"
		style:grid-template-columns="repeat({options.length}, minmax(0, 1fr))"
		role="group"
		aria-label={label}
	>
		<span
			aria-hidden="true"
			class={pill}
			style:width="calc((100% - 0.5rem - {options.length - 1} * 0.25rem) / {options.length})"
			style:transform="translateX(calc({index} * (100% + 0.25rem)))"
		></span>
		{#each options as option (option.id)}<button
				type="button"
				aria-pressed={current === option.id}
				aria-label={option.label}
				title={option.label}
				{disabled}
				class="relative z-10 flex items-center justify-center rounded-lg py-2.5 motion-safe:transition-colors {current ===
				option.id
					? 'text-brand'
					: 'text-muted hover:text-white'}"
				onclick={() => onpick(option.id)}><option.icon size={18} stroke={1.8} /></button
			>{/each}
	</div>
{/snippet}

{#snippet colorPicker()}
	<div>
		<h2 class="mb-3 text-sm font-semibold">Color</h2>
		<div class="flex items-center gap-2.5" role="group" aria-label="Color">
			{#each colors as color (color.value)}
				<button
					type="button"
					aria-pressed={values.color === color.value}
					aria-label={color.label}
					title={color.label}
					{disabled}
					onclick={() => editor.setStyle({ color: color.value })}
					style:background-color={color.value}
					class="flex size-8 items-center justify-center rounded-full ring-offset-2 ring-offset-panel motion-safe:transition-shadow {values.color ===
					color.value
						? 'ring-2 ring-brand'
						: 'ring-1 ring-white/15 hover:ring-white/40'}"
				>
					{#if values.color === color.value}<IconCheck
							size={15}
							stroke={3}
							class={color.value === '#facc15' || color.value === '#4ade80'
								? 'text-canvas'
								: 'text-white'}
						/>{/if}
				</button>
			{/each}
			<label
				title="Custom color"
				class="relative flex size-8 cursor-pointer items-center justify-center rounded-full ring-offset-2 ring-offset-panel has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-offset-6 has-[:focus-visible]:outline-brand motion-safe:transition-shadow {customColor
					? 'ring-2 ring-brand'
					: 'ring-1 ring-white/15 hover:ring-white/40'}"
				style:background={customColor
					? values.color
					: 'conic-gradient(from 180deg, #f87171, #fbbf24, #34d399, #60a5fa, #a78bfa, #f472b6, #f87171)'}
			>
				<input
					type="color"
					value={values.color}
					{disabled}
					aria-label="Custom color"
					oninput={(event) => editor.setStyle({ color: event.currentTarget.value })}
					class="absolute inset-0 size-full cursor-pointer opacity-0"
				/>
			</label>
		</div>
	</div>
{/snippet}

{#snippet fillPicker()}
	{@const matching = values.match ?? true}
	{@const white = !matching && values.color === '#ffffff'}
	{@const custom = !matching && !white}
	{@const matched = editor.selectedMark?.kind === 'erase' ? editor.selectedMark.matched : null}
	<div>
		<div class="mb-3 flex items-center justify-between">
			<h2 class="text-sm font-semibold">Fill</h2>
			<span class="text-xs text-muted">{matching ? 'Page color' : white ? 'White' : 'Custom'}</span>
		</div>
		<div class="flex items-center gap-2.5" role="group" aria-label="Fill">
			<button
				type="button"
				aria-pressed={matching}
				aria-label="Match the page"
				title="Match the page"
				{disabled}
				onclick={() => editor.setStyle({ match: true })}
				style:background-color={matched === null ? undefined : hexColor(matched)}
				class="flex size-8 items-center justify-center rounded-full ring-offset-2 ring-offset-panel motion-safe:transition-shadow {matched ===
				null
					? 'bg-canvas text-white'
					: lightColor(matched)
						? 'text-canvas'
						: 'text-white'} {matching
					? 'ring-2 ring-brand'
					: 'ring-1 ring-white/15 hover:ring-white/40'}"
				><IconColorPicker size={15} stroke={2} /></button
			>
			<button
				type="button"
				aria-pressed={white}
				aria-label="White"
				title="White"
				{disabled}
				onclick={() => editor.setStyle({ color: '#ffffff', match: false })}
				class="flex size-8 items-center justify-center rounded-full bg-white ring-offset-2 ring-offset-panel motion-safe:transition-shadow {white
					? 'ring-2 ring-brand'
					: 'ring-1 ring-white/15 hover:ring-white/40'}"
			>
				{#if white}<IconCheck size={15} stroke={3} class="text-canvas" />{/if}
			</button>
			<label
				title="Custom color"
				class="relative flex size-8 cursor-pointer items-center justify-center rounded-full ring-offset-2 ring-offset-panel has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-offset-6 has-[:focus-visible]:outline-brand motion-safe:transition-shadow {custom
					? 'ring-2 ring-brand'
					: 'ring-1 ring-white/15 hover:ring-white/40'}"
				style:background={custom
					? values.color
					: 'conic-gradient(from 180deg, #f87171, #fbbf24, #34d399, #60a5fa, #a78bfa, #f472b6, #f87171)'}
			>
				<input
					type="color"
					value={values.color}
					{disabled}
					aria-label="Custom color"
					oninput={(event) => editor.setStyle({ color: event.currentTarget.value, match: false })}
					class="absolute inset-0 size-full cursor-pointer opacity-0"
				/>
			</label>
		</div>
	</div>
{/snippet}

{#snippet widthPicker()}
	<div>
		<div class="mb-3 flex items-center justify-between">
			<h2 class="text-sm font-semibold">Width</h2>
			<span class="text-xs text-muted tabular-nums">{values.width} pt</span>
		</div>
		<div
			class="relative grid grid-cols-4 gap-1 rounded-xl bg-canvas p-1"
			role="group"
			aria-label="Width"
		>
			<span
				aria-hidden="true"
				class="{pill} w-[calc((100%-1.25rem)/4)]"
				style:transform="translateX(calc({Math.max(0, widths.indexOf(values.width ?? 2))} * (100% + 0.25rem)))"
			></span>
			{#each widths as width (width)}<button
					type="button"
					aria-pressed={values.width === width}
					aria-label="{width} pt"
					title="{width} pt"
					{disabled}
					class="group relative z-10 flex h-10 items-center justify-center rounded-lg"
					onclick={() => editor.setStyle({ width })}
					><span
						aria-hidden="true"
						class="w-7 rounded-full motion-safe:transition-colors {values.width === width
							? 'bg-brand'
							: 'bg-muted group-hover:bg-white'}"
						style:height="{width}px"
					></span></button
				>{/each}
		</div>
	</div>
{/snippet}

{#snippet opacityPicker()}
	<div>
		<div class="mb-3 flex items-center justify-between">
			<label for="{id}-opacity" class="text-sm font-semibold">Opacity</label>
			<span class="text-xs text-muted tabular-nums">{values.opacity}%</span>
		</div>
		<input
			id="{id}-opacity"
			type="range"
			min="10"
			max="100"
			step="5"
			value={values.opacity}
			{disabled}
			oninput={(event) => editor.setStyle({ opacity: Number(event.currentTarget.value) })}
			class="w-full cursor-pointer accent-brand"
		/>
	</div>
{/snippet}

<div class="space-y-6">
	<div class="space-y-3">
		<div class="flex items-center justify-between">
			<h2 class="text-sm font-semibold">Tool</h2>
			<span class="text-xs text-muted">{tools[toolIndex].label}</span>
		</div>
		{@render choices('Tool', tools, editor.tool, (tool) => editor.setTool(tool as AnnotateTool))}
	</div>

	<div bind:this={section}>
		{#key group}
			{#if group}
				<div
					class="space-y-6"
					in:fly={{ y: 8, duration: reducedMotion ? 0 : 260, easing: cubicOut }}
				>
					{#if group === 'markup' || group === 'shape'}
						<div class="space-y-3">
							<div class="flex items-center justify-between">
								<h2 class="text-sm font-semibold">Style</h2>
								<span class="text-xs text-muted">{styleName}</span>
							</div>
							{@render choices(
								'Style',
								group === 'markup' ? markups : shapes,
								values.kind,
								(kind) => editor.setStyle({ kind: kind as typeof values.kind })
							)}
						</div>
					{/if}
					{#if group === 'text'}
						<StampTextSettings
							bind:family={
								() => values.family ?? 'helvetica', (family) => editor.setStyle({ family })
							}
							bind:bold={() => values.bold ?? false, (bold) => editor.setStyle({ bold })}
							bind:size={() => values.size ?? 14, (size) => editor.setStyle({ size })}
							bind:color={() => values.color ?? '#000000', (color) => editor.setStyle({ color })}
							minSize={6}
							maxSize={72}
							{disabled}
						/>
						{#if !editor.selectedMark?.replaces}
							<ToggleSwitch
								bind:checked={
									() => values.background ?? false, (background) => editor.setStyle({ background })
								}
								label="White background"
								tone="brand"
							/>
						{/if}
					{:else if group === 'erase'}
						{@render fillPicker()}
					{:else if group === 'picture' && editor.selectedMark?.kind === 'picture'}
						{@const picture = editor.selectedMark}
						<div>
							<div class="mb-3 flex items-center justify-between">
								<h2 class="text-sm font-semibold">Image</h2>
								<span class="text-xs text-muted">{picture.deleted ? 'Deleted' : 'In the PDF'}</span>
							</div>
							<div
								class="grid grid-cols-2 gap-1 rounded-xl bg-canvas p-1"
								role="group"
								aria-label="Image"
							>
								<button
									type="button"
									onclick={() => editor.discard(picture.id)}
									disabled={disabled || picture.deleted}
									class={action}><IconTrash size={16} />Delete</button
								>
								<button
									type="button"
									onclick={() => editor.remove(picture.id)}
									{disabled}
									class={action}><IconRestore size={16} />Restore</button
								>
							</div>
						</div>
					{:else if group === 'image'}
						<div class="space-y-3">
							<input
								bind:this={picker}
								type="file"
								accept="image/jpeg,image/png,.jpg,.jpeg,.png"
								class="hidden"
								aria-label="Choose image"
								onchange={() => void choose(picker?.files)}
							/>
							{#if image}
								<div
									class="flex items-center gap-3 rounded-xl border border-white/10 bg-canvas p-2 pr-2.5"
								>
									<img
										src={image.url}
										alt=""
										class="size-12 shrink-0 rounded-lg bg-white object-contain p-1"
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
										aria-label="Choose another image"
										title="Choose another image"
										class="flex size-8 shrink-0 items-center justify-center rounded-lg text-muted transition-colors hover:bg-white/10 hover:text-white"
										><IconReplace size={17} /></button
									>
									<button
										type="button"
										{disabled}
										onclick={() => (editor.image = null)}
										aria-label="Stop placing this image"
										title="Stop placing this image"
										class="flex size-8 shrink-0 items-center justify-center rounded-lg text-muted transition-colors hover:bg-brand hover:text-canvas"
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
							{#if imageError}<p role="alert" class="text-xs text-convert">{imageError}</p>{/if}
						</div>
						{@render opacityPicker()}
					{:else}
						{@render colorPicker()}
						{#if group === 'ink' || group === 'shape'}
							{@render widthPicker()}
						{/if}
						{#if group === 'shape' && isShapeArea}
							<div transition:slide={{ duration: reducedMotion ? 0 : 220, easing: cubicOut }}>
								<ToggleSwitch
									bind:checked={() => values.fill ?? false, (fill) => editor.setStyle({ fill })}
									label="Fill"
									tone="brand"
								/>
							</div>
						{/if}
						{#if group === 'ink' || group === 'shape'}
							{@render opacityPicker()}
						{/if}
					{/if}
				</div>
			{/if}
		{/key}
	</div>

	<div>
		<h2 class="mb-3 text-sm font-semibold">{editor.mode === 'edit' ? 'Edits' : 'Annotations'}</h2>
		<div
			class="grid grid-cols-2 gap-1 rounded-xl bg-canvas p-1"
			role="group"
			aria-label={editor.mode === 'edit' ? 'Edits' : 'Annotations'}
		>
			<button
				type="button"
				onclick={() => editor.undo()}
				disabled={disabled || editor.history.length === 0}
				class={action}><IconArrowBackUp size={16} />Undo</button
			>
			<button
				type="button"
				onclick={() => editor.clear()}
				disabled={disabled || editor.marks.length === 0}
				class={action}><IconTrash size={16} />Clear all</button
			>
		</div>
		{#if editor.undrawable}<p
				role="alert"
				transition:slide={{ duration: reducedMotion ? 0 : 180, easing: cubicOut }}
				class="pt-2 text-xs text-convert"
			>
				“{editor.undrawable}” can't be drawn with the built-in PDF fonts
			</p>{/if}
	</div>
</div>
