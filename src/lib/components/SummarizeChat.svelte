<script lang="ts">
	import { tick } from 'svelte';
	import { cubicOut } from 'svelte/easing';
	import { fade, fly } from 'svelte/transition';
	import { IconDownload, IconTrash } from '@tabler/icons-svelte-runes';
	import type { SummarizeDocument } from '$lib/pdf/summarize.svelte';

	let {
		summary,
		title,
		reducedMotion,
		disabled = false,
		onask
	}: {
		summary: SummarizeDocument;
		/// The file's name, for the saved conversation.
		title: string;
		reducedMotion: boolean;
		disabled?: boolean;
		onask: (question: string) => void;
	} = $props();

	let list = $state<HTMLOListElement>();

	const starters = [
		'What is this about?',
		'What are the key numbers and dates?',
		'Who is involved?'
	];

	const model = $derived(summary.model);
	const last = $derived(summary.chat.at(-1));
	const percent = $derived(
		model.status === 'loading' && model.total
			? Math.min(99, Math.floor((model.received / model.total) * 100))
			: 0
	);

	// Follows the answer as it is written, unless someone has scrolled up.
	let following = true;
	$effect(() => {
		void summary.chat.length;
		void last?.answer;
		void last?.status;
		if (!list || !following) return;
		void tick().then(() => list?.scrollTo({ top: list.scrollHeight }));
	});

	function save() {
		const blob = new Blob([summary.chatMarkdown(title)], { type: 'text/markdown;charset=utf-8' });
		const link = document.createElement('a');
		link.href = URL.createObjectURL(blob);
		link.download = `${title.replace(/\.[^.]+$/, '') || 'document'}-questions.md`;
		link.click();
		setTimeout(() => URL.revokeObjectURL(link.href), 1000);
	}

	const tool =
		'flex size-8 items-center justify-center rounded-lg text-muted enabled:hover:bg-panel-hover enabled:hover:text-white disabled:opacity-40 motion-safe:transition-colors';
</script>

<div>
	<div class="mb-3 flex min-h-8 items-center justify-between gap-2">
		<h2 class="text-sm font-semibold">Conversation</h2>
		{#if summary.chat.length}
			<div class="flex items-center gap-1" transition:fade={{ duration: reducedMotion ? 0 : 140 }}>
				<button
					type="button"
					class={tool}
					disabled={summary.answering || !summary.chat.some((turn) => turn.answer)}
					onclick={save}
					aria-label="Save conversation"
					title="Save conversation"><IconDownload size={16} /></button
				>
				<button
					type="button"
					class={tool}
					onclick={() => summary.clearChat()}
					aria-label="Clear conversation"
					title="Clear conversation"><IconTrash size={16} /></button
				>
			</div>
		{/if}
	</div>

	{#if summary.chat.length === 0}
		<div class="flex flex-wrap gap-2">
			{#each starters as starter (starter)}
				<button
					type="button"
					{disabled}
					onclick={() => onask(starter)}
					class="rounded-lg bg-canvas px-3 py-2 text-left text-xs text-tool-label enabled:hover:bg-panel-hover enabled:hover:text-white disabled:opacity-40 motion-safe:transition-colors"
					>{starter}</button
				>
			{/each}
		</div>
	{:else}
		<ol
			bind:this={list}
			onscroll={() => {
				if (list) following = list.scrollHeight - list.scrollTop - list.clientHeight < 24;
			}}
			class="max-h-[32rem] [scrollbar-width:thin] space-y-5 overflow-y-auto overscroll-contain rounded-xl bg-canvas p-3"
			aria-live="polite"
		>
			{#each summary.chat as turn, index (index)}
				<li in:fly={{ y: 8, duration: reducedMotion ? 0 : 220, easing: cubicOut }}>
					<div class="flex justify-end">
						<p
							class="max-w-[85%] rounded-xl rounded-br-md bg-panel-hover px-3 py-2 text-xs leading-relaxed break-words"
						>
							{turn.question}
						</p>
					</div>
					<div class="mt-3 pr-2">
						{#if turn.status === 'starting'}
							<p class="text-xs text-muted" role="status">
								{summary.stage === 'model' && model.status === 'loading'
									? `Downloading model ${percent}%`
									: 'Starting model...'}
							</p>
						{:else if turn.status === 'reading'}
							<!-- The model reads the excerpts before it writes a word. -->
							<p class="flex items-center gap-1 py-1" role="status" aria-label="Reading">
								{#each [0, 1, 2] as dot (dot)}<span
										class="size-1.5 rounded-full bg-compress/70 motion-safe:animate-pulse"
										style:animation-delay="{dot * 160}ms"
									></span>{/each}
							</p>
						{:else if turn.status === 'failed'}
							<p class="text-xs text-convert" role="alert">{turn.error}</p>
						{:else}
							<p class="text-xs leading-relaxed whitespace-pre-line text-tool-label">
								{turn.answer.trim()}{#if turn.status === 'writing'}<span
										aria-hidden="true"
										class="ml-0.5 inline-block h-3 w-0.5 translate-y-0.5 bg-compress motion-safe:animate-pulse"
									></span>{/if}
							</p>
						{/if}
						{#if turn.status === 'writing' || turn.status === 'done' || turn.status === 'stopped'}
							<div class="mt-2 flex flex-wrap items-center gap-1.5">
								{#each turn.sources as source, at (at)}
									<button
										type="button"
										in:fade={{ duration: reducedMotion ? 0 : 160 }}
										onclick={() => summary.show(source)}
										class="rounded-md px-1.5 py-1 text-[11px] font-semibold tabular-nums motion-safe:transition-colors {summary
											.focus?.box === source.box
											? 'bg-compress text-canvas'
											: 'bg-compress/15 text-compress hover:bg-compress hover:text-canvas'}"
										>Page {source.page}</button
									>
								{/each}
								<span class="ml-auto text-[11px] text-muted tabular-nums">
									{#if turn.status === 'writing' && summary.speed > 0}{Math.round(summary.speed)} tok/s{:else if turn.status === 'stopped'}Stopped{/if}
								</span>
							</div>
						{/if}
					</div>
				</li>
			{/each}
		</ol>
	{/if}
</div>
