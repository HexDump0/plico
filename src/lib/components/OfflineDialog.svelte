<script lang="ts">
	import gsap from 'gsap';
	import { cubicOut } from 'svelte/easing';
	import { scale, slide } from 'svelte/transition';
	import {
		IconCloudCheck,
		IconCloudDownload,
		IconCloudOff,
		IconDownload,
		IconFileTypeDocx,
		IconLanguage,
		IconLanguageHiragana,
		IconRefresh,
		IconSparkles,
		IconTextScan2,
		IconTool,
		IconX
	} from '@tabler/icons-svelte-runes';
	import { offline, type PackId, type SaveState } from '$lib/offline.svelte';
	import { formatSize } from '$lib/workspace.svelte';

	let dialog: HTMLDialogElement;
	let reducedMotion = $state(false);

	export function open() {
		reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
		void offline.refresh();
		dialog.showModal();
		if (!reducedMotion) {
			gsap.fromTo(
				dialog,
				{ y: 10, opacity: 0 },
				{ y: 0, opacity: 1, duration: 0.2, ease: 'power2.out', clearProps: 'all' }
			);
		}
	}

	// Each pack has its own colour so its share of the bar reads at a glance.
	// Whole class names so Tailwind's scanner sees them.
	const packs: {
		id: PackId;
		label: string;
		detail: string;
		icon: typeof IconTool;
		tone: string;
		solid: string;
		soft: string;
		border: string;
	}[] = [
		{
			id: 'office',
			label: 'Office conversion',
			detail: 'Word, PowerPoint, Excel, Markdown',
			icon: IconFileTypeDocx,
			tone: 'text-convert',
			solid: 'bg-convert',
			soft: 'bg-convert/15',
			border: 'border-convert'
		},
		{
			id: 'repair',
			label: 'Repair',
			detail: 'Damaged files, in any tool',
			icon: IconTool,
			tone: 'text-merge',
			solid: 'bg-merge',
			soft: 'bg-merge/15',
			border: 'border-merge'
		},
		{
			id: 'ocr',
			label: 'Text recognition',
			detail: 'OCR and Scan, with English',
			icon: IconTextScan2,
			tone: 'text-compress',
			solid: 'bg-compress',
			soft: 'bg-compress/15',
			border: 'border-compress'
		},
		{
			id: 'translate',
			label: 'Translation',
			detail: 'Translate PDF',
			icon: IconLanguage,
			tone: 'text-split',
			solid: 'bg-split',
			soft: 'bg-split/15',
			border: 'border-split'
		},
		{
			id: 'summarize',
			label: 'Summaries',
			detail: 'Summarize PDF, with its model',
			icon: IconSparkles,
			tone: 'text-brand',
			solid: 'bg-brand',
			soft: 'bg-brand/15',
			border: 'border-brand'
		},
		{
			id: 'cjk',
			label: 'CJK fonts',
			detail: 'Chinese, Japanese, Korean text',
			icon: IconLanguageHiragana,
			tone: 'text-muted',
			solid: 'bg-muted',
			soft: 'bg-muted/15',
			border: 'border-muted'
		}
	];

	const percent = (state: SaveState | undefined) =>
		state?.total ? Math.min(99, Math.floor((state.received / state.total) * 100)) : 0;

	const core = $derived(offline.core);
	const coreState = $derived(
		core.status === 'ready' ? (offline.online ? 'ready' : 'offline') : core.status
	);
	const title = $derived(
		coreState === 'ready'
			? 'Ready offline'
			: coreState === 'offline'
				? 'Working offline'
				: coreState === 'failed'
					? "Couldn't save for offline use"
					: 'Saving for offline use'
	);

	// The bar is all of Plico: the core, then each pack, by real bytes saved.
	const segments = $derived([
		{
			id: 'core',
			solid: 'bg-white',
			bytes: core.status === 'ready' ? offline.coreSize : core.received,
			total: offline.coreSize || core.total
		},
		...packs.map((pack) => {
			const state = offline.packs.get(pack.id);
			return {
				id: pack.id,
				solid: pack.solid,
				bytes: state?.status === 'ready' ? state.total : (state?.received ?? 0),
				total: state?.total ?? 0
			};
		})
	]);
	const everything = $derived(segments.reduce((sum, segment) => sum + segment.total, 0));
	const saved = $derived(segments.reduce((sum, segment) => sum + segment.bytes, 0));
</script>

<dialog
	bind:this={dialog}
	aria-labelledby="offline-title"
	class="m-auto max-h-[calc(100dvh-2rem)] w-[calc(100%-2rem)] max-w-2xl overflow-y-auto overscroll-contain rounded-3xl bg-panel p-5 text-white backdrop:bg-black/50 sm:p-8"
	onclick={(event) => {
		if (event.target === dialog) {
			const rect = dialog.getBoundingClientRect();
			if (
				event.clientX < rect.left ||
				event.clientX > rect.right ||
				event.clientY < rect.top ||
				event.clientY > rect.bottom
			)
				dialog.close();
		}
	}}
	onclose={() => gsap.killTweensOf(dialog)}
>
	<div class="mb-6 flex items-center gap-3">
		<h2 id="offline-title" class="flex-1 text-3xl font-semibold tracking-tight">Offline</h2>
		{#if offline.installable}
			<button
				type="button"
				onclick={() => void offline.install()}
				class="h-12 rounded-xl bg-brand px-5 font-semibold text-panel transition-colors hover:bg-violet-300"
				>Install app</button
			>
		{/if}
		<button
			aria-label="Close"
			onclick={() => dialog.close()}
			class="flex size-12 items-center justify-center rounded-xl bg-canvas transition-colors hover:bg-panel-hover"
			><IconX size={22} aria-hidden="true" /></button
		>
	</div>

	<div class="rounded-2xl bg-canvas p-5">
		<div class="flex items-center gap-4">
			<span class="grid size-11 shrink-0 place-items-center rounded-xl bg-white/[0.06]">
				{#each [{ state: 'ready', Icon: IconCloudCheck }, { state: 'offline', Icon: IconCloudOff }, { state: 'saving', Icon: IconCloudDownload }] as { state, Icon } (state)}
					<Icon
						size={24}
						stroke={1.8}
						aria-hidden="true"
						class="col-start-1 row-start-1 motion-safe:transition-[opacity,transform] motion-safe:duration-200 {coreState ===
							state ||
						(state === 'saving' && coreState === 'failed')
							? 'scale-100 opacity-100'
							: 'scale-50 opacity-0'}"
					/>
				{/each}
			</span>
			<p class="min-w-0 flex-1 font-semibold" role="status">{title}</p>
			{#if core.status === 'failed'}
				<button
					type="button"
					disabled={!offline.online}
					onclick={() => void offline.retry()}
					class="flex shrink-0 items-center gap-1.5 rounded-lg px-3 py-2 text-sm font-semibold text-convert enabled:hover:bg-convert/10 disabled:opacity-40 motion-safe:transition-colors"
					><IconRefresh size={16} />Retry</button
				>
			{:else if core.status === 'saving'}
				<span class="text-2xl font-semibold tabular-nums">{percent(core)}%</span>
			{:else}
				<div class="text-right">
					<p class="text-2xl leading-tight font-semibold tabular-nums">{formatSize(saved)}</p>
					<p class="text-xs text-muted">on this device</p>
				</div>
			{/if}
		</div>
		<div
			class="mt-5 flex h-2 gap-0.5 overflow-hidden rounded-full bg-white/[0.06]"
			aria-hidden="true"
		>
			{#each segments as segment (segment.id)}
				{#if segment.bytes > 0 && everything > 0}<span
						class="h-full shrink-0 rounded-full {segment.solid} motion-safe:transition-[width] motion-safe:duration-300"
						style:width="{(segment.bytes / everything) * 100}%"
					></span>{/if}
			{/each}
		</div>
	</div>

	<h3 class="mt-8 mb-3 text-lg font-semibold">Downloads</h3>
	<div class="grid grid-cols-2 gap-3 sm:grid-cols-3">
		{#each packs as pack (pack.id)}
			{@const state = offline.packs.get(pack.id)}
			<div class="relative">
				{#if state?.status === 'ready'}
					<div
						in:scale={{ duration: reducedMotion ? 0 : 200, start: 0.96, easing: cubicOut }}
						class="flex h-full min-h-32 flex-col gap-3 rounded-xl p-4 text-panel {pack.solid}"
					>
						<pack.icon size={24} stroke={1.8} aria-hidden="true" />
						<div class="mt-auto">
							<p class="text-sm font-bold">{pack.label}</p>
							<p class="mt-0.5 text-xs text-panel/70">{pack.detail}</p>
						</div>
					</div>
					<button
						type="button"
						onclick={() => void offline.remove(pack.id)}
						aria-label="Remove {pack.label}, {formatSize(state.total)}"
						title="Remove"
						class="absolute top-2 right-2 flex size-8 items-center justify-center rounded-lg bg-canvas/80 text-white backdrop-blur-sm transition-colors hover:bg-canvas"
						><IconX size={16} /></button
					>
				{:else}
					<button
						type="button"
						disabled={!state || state.status === 'saving' || !offline.online}
						onclick={() => void offline.download(pack.id)}
						aria-label="{state?.status === 'failed'
							? 'Retry'
							: 'Download'} {pack.label}, {formatSize(state?.total ?? 0)}"
						title={offline.online ? undefined : 'Needs a connection'}
						class="group relative isolate flex h-full min-h-32 w-full flex-col gap-3 overflow-hidden rounded-xl border-2 border-dashed bg-canvas p-4 text-left motion-safe:transition-colors {state?.status ===
							'saving' || state?.status === 'failed'
							? state.status === 'failed'
								? 'border-convert/60'
								: pack.border
							: 'border-white/10 enabled:hover:border-white/25'} disabled:cursor-default"
					>
						<!-- Real bytes received; the tile turns solid once they are all in. -->
						{#if state?.status === 'saving'}<span
								aria-hidden="true"
								class="absolute inset-y-0 left-0 -z-10 {pack.soft} motion-safe:transition-[width] motion-safe:duration-300"
								style:width="{percent(state)}%"
							></span>{/if}
						<div class="flex items-start justify-between gap-2">
							<pack.icon size={24} stroke={1.8} class={pack.tone} aria-hidden="true" />
							{#if state?.status === 'saving'}
								<span class="text-xs font-semibold tabular-nums" role="status"
									>{percent(state)}%</span
								>
							{:else if state?.status === 'failed'}
								<span class="flex items-center gap-1 text-xs font-semibold text-convert"
									><IconRefresh size={14} />Retry</span
								>
							{:else if state}
								<span
									class="flex items-center gap-1 text-xs text-muted tabular-nums group-enabled:group-hover:text-white motion-safe:transition-colors"
									>{formatSize(state.total)}<IconDownload size={14} /></span
								>
							{/if}
						</div>
						<div class="mt-auto {offline.online ? '' : 'opacity-50'}">
							<p class="text-sm font-bold">{pack.label}</p>
							<p class="mt-0.5 text-xs text-muted">{pack.detail}</p>
						</div>
					</button>
				{/if}
			</div>
		{/each}
	</div>

	{#if offline.models.length}
		<div transition:slide={{ duration: reducedMotion ? 0 : 200, easing: cubicOut }}>
			<h3 class="mt-8 mb-3 text-lg font-semibold">Languages</h3>
			<ul class="flex flex-wrap gap-2">
				{#each offline.models as model (`${model.kind}:${model.id}`)}
					<li
						out:scale={{ duration: reducedMotion ? 0 : 160, start: 0.9, easing: cubicOut }}
						class="flex h-10 max-w-full items-center gap-2 rounded-xl bg-canvas pr-1 pl-3 text-sm"
					>
						<span
							class="text-xs font-semibold {model.kind === 'ocr' ? 'text-compress' : 'text-split'}"
							>{model.kind === 'ocr' ? 'OCR' : 'Translate'}</span
						>
						<span class="min-w-0 truncate">{model.name}</span>
						<span class="shrink-0 text-xs text-muted tabular-nums">{formatSize(model.size)}</span>
						<button
							type="button"
							onclick={() => void offline.removeModel(model)}
							aria-label="Remove {model.name}"
							title="Remove"
							class="flex size-8 shrink-0 items-center justify-center rounded-lg text-muted transition-colors hover:bg-panel-hover hover:text-white"
							><IconX size={15} /></button
						>
					</li>
				{/each}
			</ul>
		</div>
	{/if}
</dialog>
