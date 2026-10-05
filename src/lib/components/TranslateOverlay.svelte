<script lang="ts">
	import { tick } from 'svelte';
	import { cubicOut } from 'svelte/easing';
	import { fade } from 'svelte/transition';
	import { fontStack } from '$lib/pdf/font-faces.svelte';
	import { hexColor, LEADING, TEXT_PADDING } from '$lib/pdf/annotate';
	import type { PreviewPage } from '$lib/pdf/stamp-layout';
	import { capHeight, type FontFamily } from '$lib/pdf/standard-fonts';
	import { translationMark, type SetBlock, type TranslateBlock } from '$lib/pdf/translate-layout';
	import type { CropArea } from '$lib/pdf/types';
	import { needsEmbedding } from '$lib/pdf/unicode-fonts';
	import AnnotationShape from './AnnotationShape.svelte';

	let {
		page,
		blocks,
		texts,
		sets,
		translating,
		showOriginal,
		rtl,
		editable,
		reducedMotion,
		onedit,
		oncover
	}: {
		page: PreviewPage;
		blocks: TranslateBlock[];
		/// Each block's translation, or undefined while it has none.
		texts: (string | undefined)[];
		sets: (SetBlock | undefined)[];
		/// Whether this page's paragraphs are with the translator.
		translating: boolean;
		showOriginal: boolean;
		rtl: boolean;
		editable: boolean;
		reducedMotion: boolean;
		onedit: (block: TranslateBlock, text: string) => void;
		/// The page's colour behind each block, once the page is drawn.
		oncover: (block: TranslateBlock, color: number) => void;
	} = $props();

	let root = $state<HTMLDivElement>();
	let editing = $state<TranslateBlock | null>(null);
	let draft = $state('');
	let field = $state<HTMLTextAreaElement>();
	let covers = $state<Record<string, number>>({});

	const percent = (value: number) => `${(value * 100).toFixed(3)}%`;
	const place = ([left, top, right, bottom]: CropArea) =>
		`left:${percent(left)};top:${percent(top)};width:${percent(right - left)};height:${percent(bottom - top)}`;
	// As AnnotateOverlay sets its text boxes: the line box's ascent and descent
	// in ems, to put the textarea's first baseline where the engine puts it.
	const families: Record<FontFamily, { css: string; ascent: number; descent: number }> = {
		helvetica: {
			css: "Helvetica, Arial, 'Liberation Sans', sans-serif",
			ascent: 0.905,
			descent: 0.212
		},
		times: {
			css: "'Times New Roman', Times, 'Liberation Serif', serif",
			ascent: 0.891,
			descent: 0.216
		},
		courier: {
			css: "'Courier New', Courier, 'Liberation Mono', monospace",
			ascent: 0.833,
			descent: 0.3
		}
	};
	let pixels = $state(0);
	const zoom = $derived(pixels / page.width);

	// Five bits a channel, as `commonColor` in edit-text.ts buckets them.
	function paper(canvas: HTMLCanvasElement, [left, top, right, bottom]: CropArea) {
		const context = canvas.getContext('2d', { willReadFrequently: true });
		if (!context) return undefined;
		const x0 = Math.max(0, Math.floor(left * canvas.width));
		const y0 = Math.max(0, Math.floor(top * canvas.height));
		const x1 = Math.min(canvas.width, Math.ceil(right * canvas.width));
		const y1 = Math.min(canvas.height, Math.ceil(bottom * canvas.height));
		if (x1 <= x0 || y1 <= y0) return undefined;
		let data: Uint8ClampedArray;
		try {
			data = context.getImageData(x0, y0, x1 - x0, y1 - y0).data;
		} catch {
			return undefined;
		}
		const counts: Record<number, number> = {};
		let [best, most] = [-1, 0];
		for (let at = 0; at < data.length; at += 4) {
			// A page not drawn yet is see-through.
			if (data[at + 3] < 255) return undefined;
			const key = ((data[at] >> 3) << 10) | ((data[at + 1] >> 3) << 5) | (data[at + 2] >> 3);
			const count = (counts[key] ?? 0) + 1;
			counts[key] = count;
			if (count > most) [best, most] = [key, count];
		}
		if (best < 0) return undefined;
		const channel = (shift: number) => (((best >> shift) & 31) << 3) | 4;
		return (channel(10) << 16) | (channel(5) << 8) | channel(0);
	}

	// Sampled once the page is drawn, which may be after the overlay is.
	$effect(() => {
		const wanted = blocks.filter((block, index) => sets[index] && covers[block.id] === undefined);
		if (!wanted.length || !root) return;
		let frame = 0;
		let tries = 0;
		const sample = () => {
			const canvas = root?.parentElement?.querySelector('canvas');
			const found: Record<string, number> = {};
			if (canvas?.width)
				for (const block of wanted) {
					const color = paper(canvas, block.box);
					if (color !== undefined) found[block.id] = color;
				}
			if (Object.keys(found).length) {
				covers = { ...covers, ...found };
				for (const block of wanted)
					if (found[block.id] !== undefined) oncover(block, found[block.id]);
			} else if (tries++ < 60) frame = requestAnimationFrame(sample);
		};
		sample();
		return () => cancelAnimationFrame(frame);
	});

	async function edit(block: TranslateBlock, text: string) {
		if (!editable) return;
		editing = block;
		draft = text;
		await tick();
		field?.focus();
		field?.setSelectionRange(draft.length, draft.length);
	}

	function finish(keep: boolean) {
		if (editing && keep && draft.trim()) onedit(editing, draft.replace(/\s+/g, ' ').trim());
		editing = null;
	}

	const editingIndex = $derived(editing ? blocks.indexOf(editing) : -1);
	const editingSet = $derived(editingIndex >= 0 ? sets[editingIndex] : undefined);
</script>

<div bind:this={root} bind:clientWidth={pixels} class="absolute inset-0 overflow-hidden">
	{#if translating}
		<!-- Each paragraph breathes while it is with the translator. -->
		{#each blocks as block, index (block.id)}
			{#if !texts[index]}
				<div
					aria-hidden="true"
					out:fade={{ duration: reducedMotion ? 0 : 240 }}
					class="pointer-events-none absolute rounded-[3px] bg-compress/15 mix-blend-multiply motion-safe:animate-pulse"
					style={place(block.box)}
				></div>
			{/if}
		{/each}
	{/if}
	{#if !showOriginal}
		{#each blocks as block, index (block.id)}
			{@const set = sets[index]}
			{@const text = texts[index]}
			{#if set && text}
				<!-- Arrives from the top down, as the page is read. -->
				<div
					class="contents"
					in:fade={{
						duration: reducedMotion ? 0 : 280,
						delay: reducedMotion ? 0 : block.box[1] * 360,
						easing: cubicOut
					}}
				>
					{#each block.runs as run, line (line)}
						<div
							aria-hidden="true"
							class="pointer-events-none absolute"
							style="{place(run.ink)};background:{hexColor(covers[block.id] ?? 0xffffff)}"
						></div>
					{/each}
					{#if editing !== block}
						<AnnotationShape {page} mark={translationMark(block, text, set)} />
					{/if}
					<button
						type="button"
						disabled={!editable}
						aria-label="Edit the translation of “{block.text.slice(0, 60)}”"
						onclick={() => void edit(block, text)}
						class="absolute cursor-text rounded-[3px] outline-offset-2 focus-visible:outline-2 focus-visible:outline-compress enabled:hover:outline-1 enabled:hover:outline-compress/60 disabled:cursor-default"
						style={place([
							Math.min(block.box[0], set.area[0]),
							Math.min(block.box[1], set.area[1]),
							Math.max(block.box[2], set.area[2]),
							Math.max(block.box[3], set.area[3]) - (LEADING * set.size) / page.height
						])}
					></button>
				</div>
			{/if}
		{/each}
	{/if}
	{#if editing && editingSet}
		{@const family = families[editing.family]}
		{@const [left, top, right, bottom] = editingSet.area}
		{@const shift =
			(capHeight(editing.family, editing.bold, needsEmbedding(draft)) -
				family.ascent -
				(LEADING - family.ascent - family.descent) / 2) *
			editingSet.size *
			zoom}
		<!-- Set in the font the engine draws, from the same first baseline, on
		the page's own paper so the original stays hidden while it is typed. -->
		<textarea
			bind:this={field}
			bind:value={draft}
			aria-label="Translation"
			spellcheck="false"
			dir={rtl ? 'rtl' : 'auto'}
			onblur={() => finish(true)}
			onkeydown={(event) => {
				if (event.key === 'Escape') {
					event.preventDefault();
					finish(false);
				} else if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
					event.preventDefault();
					finish(true);
				}
			}}
			class="absolute resize-none overflow-hidden outline-1 outline-compress outline-dashed focus-visible:outline-1 focus-visible:outline-compress"
			style:left={percent(left)}
			style:top="calc({percent(top)} + {shift}px)"
			style:width={percent(right - left)}
			style:height="calc({percent(bottom - top)} - {shift}px)"
			style:padding="{TEXT_PADDING * zoom}px"
			style:font-family={needsEmbedding(draft) ? fontStack(draft, editing.family) : family.css}
			style:font-weight={editing.bold ? 700 : 400}
			style:font-size="{editingSet.size * zoom}px"
			style:line-height={LEADING}
			style:text-align={editingSet.align}
			style:color={hexColor(editing.color)}
			style:caret-color={hexColor(editing.color)}
			style:background={hexColor(covers[editing.id] ?? 0xffffff)}></textarea>
	{/if}
</div>
