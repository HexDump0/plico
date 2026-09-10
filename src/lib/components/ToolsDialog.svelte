<script lang="ts">
	import { onDestroy, tick } from 'svelte';
	import gsap from 'gsap';
	import { IconSearch, IconX } from '@tabler/icons-svelte-runes';
	import { quickTools, toolColumns, type CatalogTool } from '$lib/tool-catalog';
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
	let closing = false;
	let previousOverflow: string | undefined;
	let motion: gsap.core.Timeline | undefined;
	let trigger: HTMLElement | undefined;

	function spring(progress: number) {
		const frequency = 9;
		const time = frequency * progress;
		const settled = 1 - (1 + frequency) * Math.exp(-frequency);
		return (1 - (1 + time) * Math.exp(-time)) / settled;
	}

	function origin() {
		const panel = dialog.getBoundingClientRect();
		const source = trigger?.getBoundingClientRect();
		const width = Math.min(source?.width ?? 80, panel.width);
		const height = Math.min(source?.height ?? 44, panel.height);
		const insetX = (panel.width - width) / 2;
		const insetY = (panel.height - height) / 2;
		const left = panel.left - Number(gsap.getProperty(dialog, 'x'));
		const top = panel.top - Number(gsap.getProperty(dialog, 'y'));
		return {
			x: source ? source.left + source.width / 2 - left - panel.width / 2 : 0,
			y: source ? source.top + source.height / 2 - top - panel.height / 2 : 0,
			clipPath: `inset(${insetY}px ${insetX}px round 12px)`
		};
	}
	const words = $derived(query.toLowerCase().trim().split(/\s+/).filter(Boolean));
	const matches = (tool: CatalogTool, category = '') =>
		words.every((word) => `${tool.label} ${category}`.toLowerCase().includes(word));
	const shortcuts = $derived(quickTools.filter((tool) => matches(tool)));
	const columns = $derived(
		toolColumns.map((column) =>
			column
				.map((category) => ({
					...category,
					tools: category.tools.filter((tool) => matches(tool, category.name))
				}))
				.filter((category) => category.tools.length)
		)
	);
	const resultCount = $derived(
		shortcuts.length + columns.flat().reduce((count, category) => count + category.tools.length, 0)
	);

	export async function open(source?: HTMLElement) {
		if (dialog.open) return;
		trigger = source;
		query = '';
		closing = false;
		await tick();
		previousOverflow = document.body.style.overflow;
		document.body.style.overflow = 'hidden';
		dialog.showModal();
		dialog.scrollTop = 0;
		search.focus({ preventScroll: true });
		motion?.kill();
		if (!window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
			motion = gsap
				.timeline()
				.fromTo(
					dialog,
					{ ...origin(), '--backdrop-opacity': 0, filter: 'blur(0px)' },
					{
						x: 0,
						y: 0,
						clipPath: 'inset(0px 0px round 24px)',
						'--backdrop-opacity': 0.5,
						duration: 0.42,
						ease: spring,
						clearProps: 'transform,clipPath,--backdrop-opacity'
					}
				)
				.to(dialog, { filter: 'blur(1px)', duration: 0.05, ease: 'sine.out' }, 0)
				.to(
					dialog,
					{
						filter: 'blur(0px)',
						duration: 0.37,
						ease: 'power2.out',
						clearProps: 'filter'
					},
					0.05
				);
		}
	}

	function restore() {
		motion?.kill();
		dialog.style.removeProperty('transform');
		dialog.style.removeProperty('opacity');
		dialog.style.removeProperty('clip-path');
		dialog.style.removeProperty('--backdrop-opacity');
		dialog.style.removeProperty('--content-opacity');
		dialog.style.removeProperty('filter');
		if (previousOverflow !== undefined) document.body.style.overflow = previousOverflow;
		previousOverflow = undefined;
		closing = false;
		trigger?.focus({ preventScroll: true });
		trigger = undefined;
	}

	function close() {
		if (closing) return;
		closing = true;
		motion?.kill();
		if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
			dialog.close();
			return;
		}
		motion = gsap
			.timeline({ onComplete: () => dialog.close() })
			.to(dialog, {
				...origin(),
				'--backdrop-opacity': 0,
				duration: 0.38,
				ease: spring
			})
			.to(dialog, { filter: 'blur(0.8px)', duration: 0.04, ease: 'sine.out' }, 0)
			.to(dialog, { filter: 'blur(0px)', duration: 0.34, ease: 'power2.out' }, 0.04)
			.to(dialog, { '--content-opacity': 0, duration: 0.1, ease: 'sine.inOut' }, 0.02)
			.to(dialog, { opacity: 0, duration: 0.14, ease: 'sine.inOut' }, 0.16);
	}

	function select(tool: CatalogTool) {
		if (closing) return;
		trigger = selectionTarget() ?? trigger;
		onselect(tool);
		close();
	}

	function outside(event: MouseEvent) {
		if (event.target !== dialog) return;
		const { left, right, top, bottom } = dialog.getBoundingClientRect();
		if (
			event.clientX < left ||
			event.clientX > right ||
			event.clientY < top ||
			event.clientY > bottom
		)
			close();
	}

	onDestroy(() => {
		if (dialog) restore();
	});
</script>

<dialog
	bind:this={dialog}
	aria-labelledby="tools-title"
	class="m-auto max-h-[calc(100dvh-2rem)] w-[calc(100%-2rem)] max-w-300 overflow-y-auto overscroll-contain rounded-3xl bg-panel p-5 text-white sm:p-8 lg:p-12"
	onclick={outside}
	oncancel={(event) => {
		event.preventDefault();
		close();
	}}
	onclose={restore}
>
	<div
		class="mb-6 grid grid-cols-[1fr_auto] items-center gap-4 sm:grid-cols-[1fr_minmax(0,23.5rem)_auto]"
	>
		<h2 id="tools-title" class="text-3xl font-semibold tracking-tight">Tools</h2>
		<label
			class="order-3 col-span-2 flex h-12 min-w-0 items-center gap-3 rounded-xl bg-canvas px-4 text-muted focus-within:ring-2 focus-within:ring-brand sm:order-none sm:col-span-1"
		>
			<IconSearch size={22} stroke={1.8} class="shrink-0" aria-hidden="true" />
			<input
				bind:this={search}
				bind:value={query}
				type="search"
				aria-label="Find a tool"
				placeholder="Find a tool..."
				autocomplete="off"
				class="w-full min-w-0 bg-transparent text-base text-white outline-none placeholder:text-muted focus:outline-none focus-visible:outline-none"
			/>
		</label>
		<button
			aria-label="Close tools"
			onclick={close}
			class="flex size-12 items-center justify-center rounded-xl bg-canvas transition-colors hover:bg-panel-hover"
			><IconX size={22} aria-hidden="true" /></button
		>
	</div>
	<div class="min-h-0 sm:min-h-148">
		{#if shortcuts.length}
			<div
				class="mb-6 grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-5 lg:gap-4"
				aria-label="Quick tools"
			>
				{#each shortcuts as tool (tool.id)}
					<button
						onclick={() => select(tool)}
						class="flex min-h-16 items-center gap-3 rounded-xl px-4 py-3 text-left text-sm font-bold text-panel transition-[filter,transform] duration-150 hover:brightness-110 active:scale-[0.98] lg:px-5 lg:text-base {tool.color}"
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
									<h3 class="mb-2 text-lg font-semibold">{category.name}</h3>
									<ul>
										{#each category.tools as tool (tool.id)}
											<li>
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
			<div class="flex min-h-64 flex-col items-center justify-center gap-3 text-center">
				<p class="text-lg font-medium">No tools found</p>
				<p class="text-muted">Try a format or action, like “JPG” or “rotate”.</p>
				<button
					class="mt-2 rounded-lg px-3 py-2 text-brand hover:bg-brand/10"
					onclick={() => {
						query = '';
						search.focus();
					}}>Clear search</button
				>
			</div>
		{/if}
	</div>
</dialog>

<style>
	dialog {
		--backdrop-opacity: 0.5;
		--content-opacity: 1;
	}

	dialog > div {
		opacity: var(--content-opacity);
	}

	dialog::backdrop {
		background: rgb(0 0 0 / var(--backdrop-opacity));
	}
</style>
