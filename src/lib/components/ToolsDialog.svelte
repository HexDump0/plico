<script lang="ts">
	import { onDestroy, tick } from 'svelte';
	import gsap from 'gsap';
	import { IconSearch, IconX } from '@tabler/icons-svelte-runes';
	import { quickTools, toolColumns, type CatalogTool } from '$lib/tool-catalog';
	import type { ToolId } from '$lib/tools';

	let { onselect }: { onselect: (tool: ToolId) => void } = $props();
	let dialog: HTMLDialogElement;
	let search: HTMLInputElement;
	let query = $state('');
	let notice = $state('');
	let closing = false;
	let previousOverflow: string | undefined;
	let motion: gsap.core.Tween | undefined;
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

	export async function open() {
		if (dialog.open) return;
		query = '';
		notice = '';
		closing = false;
		await tick();
		previousOverflow = document.body.style.overflow;
		document.body.style.overflow = 'hidden';
		dialog.showModal();
		dialog.scrollTop = 0;
		search.focus({ preventScroll: true });
		motion?.kill();
		if (!window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
			motion = gsap.fromTo(
				dialog,
				{ y: 12, opacity: 0 },
				{
					y: 0,
					opacity: 1,
					duration: 0.22,
					ease: 'power2.out',
					clearProps: 'transform,opacity'
				}
			);
		}
	}

	function restore() {
		motion?.kill();
		dialog.style.removeProperty('transform');
		dialog.style.removeProperty('opacity');
		if (previousOverflow !== undefined) document.body.style.overflow = previousOverflow;
		previousOverflow = undefined;
		closing = false;
	}

	function close() {
		if (closing) return;
		closing = true;
		motion?.kill();
		if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
			dialog.close();
			return;
		}
		motion = gsap.to(dialog, {
			y: 6,
			opacity: 0,
			duration: 0.14,
			ease: 'power2.in',
			onComplete: () => dialog.close()
		});
	}

	function select(tool: CatalogTool) {
		if (tool.heroTool) {
			onselect(tool.heroTool);
			close();
		} else {
			notice = `${tool.label} is coming soon.`;
		}
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
	class="m-auto max-h-[calc(100dvh-2rem)] w-[calc(100%-2rem)] max-w-300 overflow-y-auto overscroll-contain rounded-3xl bg-panel p-5 text-white backdrop:bg-black/50 sm:p-8 lg:p-12"
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
				oninput={() => (notice = '')}
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
														class="text-tool-label transition-colors duration-150 group-hover:text-inherit "
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
