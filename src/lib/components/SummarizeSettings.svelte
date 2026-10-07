<script lang="ts">
	import { tick } from 'svelte';
	import { cubicOut } from 'svelte/easing';
	import { fade, fly, slide } from 'svelte/transition';
	import { IconCheck, IconSparkles } from '@tabler/icons-svelte-runes';
	import { resolve } from '$app/paths';
	import type { SummarizeDocument, SummaryLength } from '$lib/pdf/summarize.svelte';
	import { formatSize } from '$lib/workspace.svelte';
	import PageScope from './PageScope.svelte';
	import SummarizeChat from './SummarizeChat.svelte';

	let {
		summary,
		title,
		pageCount,
		processing,
		reducedMotion,
		disabled = false,
		onask
	}: {
		summary: SummarizeDocument;
		/// The file's name.
		title: string;
		pageCount: number;
		processing: boolean;
		reducedMotion: boolean;
		disabled?: boolean;
		onask: (question: string) => void;
	} = $props();

	const modes = [
		{ id: 'summary', label: 'Summary' },
		{ id: 'ask', label: 'Ask' }
	] as const;

	let list = $state<HTMLOListElement>();

	const lengths: { id: SummaryLength; label: string }[] = [
		{ id: 'short', label: 'Short' },
		{ id: 'medium', label: 'Medium' },
		{ id: 'long', label: 'Long' }
	];
	const slot = (index: number) =>
		['', 'translate-x-[calc(100%+0.5rem)]', 'translate-x-[calc(200%+1rem)]'][index];

	const model = $derived(summary.model);
	const percent = $derived(
		model.status === 'loading' && model.total
			? Math.min(99, Math.floor((model.received / model.total) * 100))
			: 0
	);
	// The overview is written last; until then its place waits, breathing.
	const waiting = $derived(processing && summary.stage !== 'writing' && !summary.overview);
	const shown = $derived(processing || summary.summarized);

	async function pick(index: number) {
		summary.pick(index);
		await tick();
		list?.querySelector(`[data-point="${index}"]`)?.scrollIntoView({ block: 'nearest' });
	}
</script>

<div class="space-y-6">
	<div
		class="relative grid grid-cols-2 gap-2 rounded-xl bg-canvas p-1"
		role="group"
		aria-label="Mode"
	>
		<span
			aria-hidden="true"
			class="pointer-events-none absolute inset-y-1 left-1 w-[calc((100%-1rem)/2)] rounded-lg bg-panel-hover motion-safe:transition-transform motion-safe:duration-200 motion-safe:ease-[cubic-bezier(0.22,1,0.36,1)] {summary.mode ===
			'ask'
				? 'translate-x-[calc(100%+0.5rem)]'
				: ''}"
		></span>
		{#each modes as option (option.id)}<button
				type="button"
				aria-pressed={summary.mode === option.id}
				disabled={disabled || summary.answering}
				class="relative z-10 rounded-lg px-2 py-3 text-xs font-semibold motion-safe:transition-colors {summary.mode ===
				option.id
					? 'text-compress'
					: 'text-muted enabled:hover:text-white'}"
				onclick={() => {
					summary.mode = option.id;
					summary.focus = null;
					summary.current = -1;
				}}>{option.label}</button
			>{/each}
	</div>

	{#if summary.mode === 'summary'}
		<div in:fly={{ x: -16, duration: reducedMotion ? 0 : 260, easing: cubicOut }}>
			<h2 class="mb-3 text-sm font-semibold">Length</h2>
			<div
				class="relative grid grid-cols-3 gap-2 rounded-xl bg-canvas p-1"
				role="group"
				aria-label="Length"
			>
				<span
					aria-hidden="true"
					class="pointer-events-none absolute inset-y-1 left-1 w-[calc((100%-1.5rem)/3)] rounded-lg bg-panel-hover motion-safe:transition-transform motion-safe:duration-200 motion-safe:ease-[cubic-bezier(0.22,1,0.36,1)] {slot(
						lengths.findIndex((option) => option.id === summary.length)
					)}"
				></span>
				{#each lengths as option (option.id)}<button
						type="button"
						aria-pressed={summary.length === option.id}
						{disabled}
						class="relative z-10 rounded-lg px-2 py-3 text-xs font-semibold motion-safe:transition-colors {summary.length ===
						option.id
							? 'text-compress'
							: 'text-muted enabled:hover:text-white'}"
						onclick={() => (summary.length = option.id)}>{option.label}</button
					>{/each}
			</div>
		</div>
	{/if}

	<div>
		<h2 class="mb-3 text-sm font-semibold">Pages</h2>
		<PageScope
			bind:pages={summary.pages}
			{pageCount}
			invalid={summary.pagesInvalid}
			label={summary.mode === 'ask' ? 'Pages to ask about' : 'Pages to summarize'}
			{reducedMotion}
			{disabled}
		/>
	</div>

	<div>
		<h2 class="mb-3 text-sm font-semibold">Model</h2>
		<div
			class="relative isolate flex min-h-12 items-center gap-3 overflow-hidden rounded-xl bg-canvas px-4 text-sm"
		>
			<!-- Real bytes received; it fills the row, then fades once stored. -->
			{#if model.status === 'loading'}<span
					aria-hidden="true"
					out:fade={{ duration: reducedMotion ? 0 : 300 }}
					class="absolute inset-y-0 left-0 -z-10 bg-compress/10 motion-safe:transition-[width] motion-safe:duration-300"
					style:width="{percent}%"
				></span>{/if}
			<IconSparkles size={18} stroke={1.8} class="shrink-0 text-compress" aria-hidden="true" />
			<span class="flex min-w-0 flex-1 items-center gap-2">
				<span class="truncate">{summary.modelName}</span>
				{#if summary.device}<span
						class="shrink-0 rounded-md bg-white/[0.06] px-1.5 py-0.5 text-[11px] font-semibold text-muted"
						title={summary.device === 'webgpu'
							? 'Runs on the graphics card'
							: 'Runs on the processor'}>{summary.device === 'webgpu' ? 'GPU' : 'CPU'}</span
					>{/if}
			</span>
			{#if model.status === 'loading'}
				<span class="text-xs text-muted tabular-nums" role="status">{percent}%</span>
			{:else if model.status === 'ready'}
				<span class="flex items-center gap-1 text-xs text-muted"
					><IconCheck size={14} class="text-compress" aria-hidden="true" />On this device</span
				>
			{:else if model.status !== 'checking'}
				<span class="text-xs text-muted tabular-nums">{formatSize(model.total)}</span>
			{/if}
		</div>
	</div>

	{#if summary.glyphs && !summary.hasText}
		<p class="text-xs leading-relaxed text-muted" role="status">
			This PDF has no text to summarize. Scanned pages need
			<a
				href={resolve('/tools/[tool]', { tool: 'ocr' })}
				class="font-semibold text-compress hover:text-white motion-safe:transition-colors"
				>OCR PDF</a
			> first.
		</p>
	{/if}

	{#if summary.mode === 'ask'}
		<div in:fly={{ x: 16, duration: reducedMotion ? 0 : 260, easing: cubicOut }}>
			<SummarizeChat
				{summary}
				{title}
				{reducedMotion}
				disabled={disabled || !summary.hasText}
				{onask}
			/>
		</div>
	{:else if shown}
		<div
			transition:slide={{ duration: reducedMotion ? 0 : 220, easing: cubicOut }}
			class="space-y-6"
		>
			<div>
				<h2 class="mb-3 text-sm font-semibold">Overview</h2>
				<div class="rounded-xl bg-canvas p-4">
					{#if waiting}
						<div class="space-y-2.5" aria-hidden="true">
							{#each [1, 0.92, 0.6] as share, index (index)}
								<div
									class="h-2.5 rounded-full bg-white/5 motion-safe:animate-pulse"
									style:width="{share * 100}%"
								></div>
							{/each}
						</div>
					{:else}
						<p class="text-sm leading-relaxed text-tool-label">
							{summary.overview}{#if processing && summary.stage === 'writing'}<span
									aria-hidden="true"
									class="ml-0.5 inline-block h-3.5 w-0.5 translate-y-0.5 bg-compress motion-safe:animate-pulse"
								></span>{/if}
						</p>
					{/if}
				</div>
			</div>

			<div>
				<h2 class="mb-3 text-sm font-semibold">Key points</h2>
				<ol
					bind:this={list}
					class="max-h-[28rem] [scrollbar-width:thin] space-y-1 overflow-y-auto overscroll-contain rounded-xl bg-canvas p-1"
				>
					{#each summary.points as point, index (index)}
						<li data-point={index} in:fade={{ duration: reducedMotion ? 0 : 200 }}>
							<button
								type="button"
								aria-current={index === summary.current}
								onclick={() => void pick(index)}
								class="flex w-full gap-3 rounded-lg px-3 py-2.5 text-left motion-safe:transition-colors {index ===
								summary.current
									? 'bg-panel-hover'
									: 'hover:bg-panel'}"
							>
								<span
									class="mt-px flex size-5 shrink-0 items-center justify-center rounded-md text-[11px] font-bold tabular-nums motion-safe:transition-colors {index ===
									summary.current
										? 'bg-compress text-canvas'
										: 'bg-compress/15 text-compress'}">{index + 1}</span
								>
								<span class="min-w-0 flex-1">
									<span class="block text-xs leading-relaxed break-words">{point.text}</span>
									<span class="mt-1 block text-[11px] text-muted tabular-nums"
										>Page {point.page}</span
									>
								</span>
							</button>
						</li>
					{:else}
						<li class="space-y-2 px-3 py-3" aria-hidden="true">
							<div class="h-2.5 w-4/5 rounded-full bg-white/5 motion-safe:animate-pulse"></div>
							<div class="h-2 w-12 rounded-full bg-white/5 motion-safe:animate-pulse"></div>
						</li>
					{/each}
				</ol>
			</div>
		</div>
	{/if}
</div>
