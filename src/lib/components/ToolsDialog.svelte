<script lang="ts">
	import { tick } from 'svelte';
	import gsap from 'gsap';
	import { DialogMotion } from '$lib/motion/dialog';
	import { spring } from '$lib/motion/link';
	import { IconSearch, IconX } from '@tabler/icons-svelte-runes';
	import { quickTools, toolColumns, type CatalogTool } from '$lib/tool-catalog';
	import { searchTools } from '$lib/tool-search';
	let {
		onselect,
		selectionTarget
	}: {
		onselect: (tool: CatalogTool) => void;
		selectionTarget: () => HTMLElement | undefined;
	} = $props();
	let dialog: HTMLDialogElement;
	let search: HTMLInputElement;
	let query = $state('');
	const panel = new DialogMotion();
	let results: HTMLDivElement;
	let departing: HTMLDivElement;
	let searchMotion: gsap.core.Timeline | undefined;
	let searchVersion = 0;

	function resultElements() {
		return Array.from(results.querySelectorAll<HTMLElement>('[data-result-key]'));
	}

	// Svelte action is better here...TODO cleanup
	// 	let ghostLayer: { clear(): void; add(node: HTMLElement): void } | undefined;
	// function ghostLayerAction(node: HTMLDivElement) {
	//   const layer = {
	//     clear: () => node.replaceChildren(),
	//     add: (ghost: HTMLElement) => node.appendChild(ghost)
	//   };
	//   ghostLayer = layer;
	//   return { destroy: () => (ghostLayer = undefined) };
	// }

	function resetSearchMotion() {
		searchVersion += 1;
		searchMotion?.kill();
		departing?.replaceChildren(); // first we need to
		if (results) gsap.set(resultElements(), { clearProps: 'transform,opacity' });
	}

	async function filterTools(value: string) {
		if (panel.closing || query === value) return;
		const version = ++searchVersion;
		searchMotion?.kill();
		dialog.style.height = `${dialog.offsetHeight}px`;
		const previous = new Map<string, { element: HTMLElement; rect: DOMRect; opacity: number }>();
		for (const element of resultElements()) {
			const key = element.dataset.resultKey!;
			if (!previous.has(key)) {
				previous.set(key, {
					element,
					rect: element.getBoundingClientRect(),
					opacity: Number(getComputedStyle(element).opacity)
				});
			}
		}
		departing.replaceChildren();
		gsap.set(resultElements(), { clearProps: 'transform,opacity' });
		query = value;
		await tick();
		if (version !== searchVersion || panel.closing || !dialog.open) return;
		if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;

		const current = resultElements();
		const keys = new Set(current.map((element) => element.dataset.resultKey!));
		const bounds = departing.getBoundingClientRect();
		searchMotion = gsap.timeline({ onComplete: () => departing.replaceChildren() });
		for (const [key, before] of previous) {
			if (keys.has(key) || before.opacity === 0) continue;
			const ghost = before.element.cloneNode(true) as HTMLElement;
			Object.assign(ghost.style, {
				position: 'absolute',
				left: `${before.rect.left - bounds.left}px`,
				top: `${before.rect.top - bounds.top}px`,
				width: `${before.rect.width}px`,
				height: `${before.rect.height}px`,
				margin: '0',
				listStyle: 'none',
				transform: 'none',
				opacity: String(before.opacity)
			});
			departing.appendChild(ghost);
			searchMotion.to(ghost, { opacity: 0, duration: 0.07, ease: 'sine.out' }, 0);
		}
		for (const element of current) {
			const before = previous.get(element.dataset.resultKey!);
			const rect = element.getBoundingClientRect();
			const deltaX = before ? before.rect.left - rect.left : 0;
			const deltaY = before ? before.rect.top - rect.top : 0;
			const x = Math.abs(deltaX) > 3 ? deltaX : 0;
			const y = Math.abs(deltaY) > 3 ? deltaY : 0;
			const moving = x !== 0 || y !== 0;
			searchMotion.fromTo(
				element,
				{
					...(moving ? { x, y, force3D: false } : {}),
					opacity: before?.opacity ?? 0
				},
				{
					...(moving ? { x: 0, y: 0, force3D: false } : {}),
					opacity: 1,
					duration: moving ? 0.22 : 0.1,
					ease: moving ? spring : 'sine.out',
					clearProps: moving ? 'transform,opacity' : 'opacity'
				},
				0
			);
		}
	}

	const shortcuts = $derived(searchTools(quickTools, query));
	const columns = $derived(
		toolColumns.map((column) =>
			column
				.map((category) => ({
					...category,
					tools: searchTools(category.tools, query, category.name)
				}))
				.filter((category) => category.tools.length)
		)
	);
	const resultCount = $derived(
		shortcuts.length + columns.flat().reduce((count, category) => count + category.tools.length, 0)
	);

	export async function open(source?: HTMLElement) {
		if (dialog.open) return;
		query = '';
		await tick();
		panel.open(source);
		search.focus({ preventScroll: true });
	}

	function reset() {
		resetSearchMotion();
		dialog.style.removeProperty('height');
	}

	function select(tool: CatalogTool) {
		if (panel.closing) return;
		dialog.addEventListener('close', () => onselect(tool), { once: true });
		panel.close(selectionTarget());
	}
</script>

<dialog
	bind:this={dialog}
	{@attach panel.attach}
	aria-labelledby="tools-title"
	class="grows m-auto max-h-[calc(100dvh-2rem)] w-[calc(100%-2rem)] max-w-300 overflow-y-auto overscroll-contain rounded-3xl bg-panel p-5 text-white sm:p-8 lg:p-12"
	onclose={reset}
>
	<div
		class="mb-6 grid grid-cols-[1fr_auto] items-center gap-4 sm:grid-cols-[1fr_minmax(0,23.5rem)_auto]"
	>
		<h2 id="tools-title" class="text-3xl font-semibold tracking-tight">Tools</h2>
		<label
			class="order-3 col-span-2 flex h-12 min-w-0 items-center gap-3 rounded-xl border border-transparent bg-canvas px-4 text-muted focus-within:border-white/25 sm:order-none sm:col-span-1"
		>
			<IconSearch size={22} stroke={1.8} class="shrink-0" aria-hidden="true" />
			<input
				bind:this={search}
				value={query}
				oninput={(event) => filterTools(event.currentTarget.value)}
				type="search"
				aria-label="Find a tool"
				placeholder="Find a tool..."
				autocomplete="off"
				class="w-full min-w-0 bg-transparent text-base text-white outline-none placeholder:text-muted focus:outline-none focus-visible:outline-none"
			/>
		</label>
		<button
			aria-label="Close tools"
			onclick={() => panel.close()}
			class="flex size-12 items-center justify-center rounded-xl bg-canvas transition-colors hover:bg-panel-hover"
			><IconX size={22} aria-hidden="true" /></button
		>
	</div>
	<div bind:this={results} class="relative min-h-0 sm:min-h-148">
		<span class="sr-only" role="status">{resultCount} tools found</span>
		{#if shortcuts.length}
			<div
				class="mb-6 grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-5 lg:gap-4"
				aria-label="Quick tools"
			>
				{#each shortcuts as tool (tool.id)}
					<button
						data-result-key={`quick-${tool.id}`}
						onclick={() => select(tool)}
						class="flex min-h-16 items-center gap-3 rounded-xl px-4 py-3 text-left text-sm font-bold text-panel transition-[filter] duration-150 hover:brightness-110 active:scale-[0.98] lg:px-5 lg:text-base {tool.color}"
					>
						<tool.icon size={24} stroke={1.8} class="shrink-0" aria-hidden="true" />{tool.label}
					</button>
				{/each}
			</div>
		{/if}
		{#if resultCount}
			<div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-3 lg:gap-9">
				{#each columns as column, index (index)}
					{#if column.length}
						<div class="flex min-w-0 flex-col gap-6">
							{#each column as category (category.name)}
								<section aria-label={category.name}>
									<h3
										data-result-key={`category-${category.name}`}
										class="mb-2 text-lg font-semibold"
									>
										{category.name}
									</h3>
									<ul>
										{#each category.tools as tool (tool.id)}
											<li data-result-key={tool.id}>
												<button
													onclick={() => select(tool)}
													class="group -ml-2 flex min-h-11 w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-base lg:min-h-8.5 lg:py-1 {category.color}"
												>
													<tool.icon size={20} stroke={1.8} class="shrink-0" aria-hidden="true" />
													<span
														class="text-tool-label transition-colors duration-150 group-hover:text-inherit"
														>{tool.label}</span
													>
												</button>
											</li>
										{/each}
									</ul>
								</section>
							{/each}
						</div>
					{/if}
				{/each}
			</div>
		{:else}
			<div
				data-result-key="empty"
				class="flex min-h-64 flex-col items-center justify-center gap-3 text-center"
			>
				<p class="text-lg font-medium">No tools found</p>
				<p class="text-muted">Try a format or action, like "JPG" or "rotate".</p>
				<button
					class="mt-2 rounded-lg px-3 py-2 text-brand hover:bg-brand/10"
					onclick={() => {
						filterTools('');
						search.focus();
					}}>Clear search</button
				>
			</div>
		{/if}
		<div
			bind:this={departing}
			class="pointer-events-none absolute inset-0"
			aria-hidden="true"
			inert
		></div>
	</div>
</dialog>

<style>
	dialog {
		scrollbar-gutter: stable;
	}
</style>
