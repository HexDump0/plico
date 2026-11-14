<script lang="ts">
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import {
		IconChevronDown,
		IconBrandGithub,
		IconCloudCheck,
		IconCloudDownload,
		IconCloudOff,
		IconCurrencyDollar
	} from '@tabler/icons-svelte-runes';
	import { offline } from '$lib/offline.svelte';
	import OfflineDialog from './OfflineDialog.svelte';
	import { linkMotion } from '$lib/motion/link';
	import logo from '$lib/assets/plico.svg';
	import type { CatalogTool } from '$lib/tool-catalog';
	import ToolsDialog from './ToolsDialog.svelte';
	let toolsDialog: ToolsDialog;
	let offlineDialog: OfflineDialog;

	let { onselect }: { onselect: (tool: CatalogTool) => void } = $props();
	let toolsButton: HTMLButtonElement;
	const onAbout = $derived(page.url.pathname === resolve('/about'));

	const offlineState = $derived(
		!offline.online ? 'offline' : offline.core.status === 'ready' ? 'ready' : 'saving'
	);
	// The ring shows real bytes saved, only while the core is being saved.
	const ring = $derived(
		offline.core.status === 'saving' && offline.core.total
			? Math.min(1, offline.core.received / offline.core.total)
			: undefined
	);

	export function openTools(source?: HTMLElement) {
		toolsDialog.open(source ?? toolsButton);
	}

	// Already on About: scroll to the section in place rather than re-navigating to the hash.
	function scrollToSupport(event: MouseEvent) {
		if (!onAbout) return;
		event.preventDefault();
		const support = document.getElementById('support');
		support?.scrollIntoView({
			behavior: window.matchMedia('(prefers-reduced-motion: reduce)').matches
				? 'instant'
				: 'smooth',
			block: 'center'
		});
		support?.focus({ preventScroll: true });
	}
</script>

<header
	class="relative z-20 mx-auto flex h-20 w-full shrink-0 items-center justify-between px-6 sm:h-24 sm:px-10 lg:grid lg:h-24 lg:grid-cols-3 lg:px-20"
>
	<a
		use:linkMotion={{ hover: false, pressScale: 0.95 }}
		href={resolve('/')}
		class="flex shrink-0 items-center gap-2 rounded-md text-2xl font-bold lg:justify-self-start"
		aria-label="Plico home"
		><span class="flex items-center gap-2"
			><img src={logo} width="26" height="30" alt="" />Plico</span
		></a
	>
	<nav
		aria-label="Main navigation"
		class="flex items-center gap-4 text-sm font-medium sm:gap-8 sm:text-base lg:gap-9 lg:justify-self-center"
	>
		<button
			use:linkMotion={{ hover: false, pressScale: 0.95 }}
			bind:this={toolsButton}
			class="flex min-h-11 items-center gap-1 rounded-md transition-colors hover:text-brand"
			onclick={(event) => openTools(event.currentTarget)}
			aria-haspopup="dialog"
			><span class="flex items-center gap-1"
				>Tools <IconChevronDown size={18} aria-hidden="true" /></span
			></button
		>
		<a
			use:linkMotion={{ hover: false, pressScale: 0.95 }}
			href={resolve('/about')}
			aria-current={onAbout ? 'page' : undefined}
			class="grid min-h-11 items-center rounded-xl transition-colors hover:text-brand"
		>
			<span class="z-10 col-start-1 row-start-1 px-4">About</span>
			{#if onAbout}
				<span
					aria-hidden="true"
					class="about-selection col-start-1 row-start-1 h-full w-full rounded-xl bg-panel-hover"
				></span>
			{/if}
		</a>
	</nav>
	<div
		class="items-center gap-8 text-base font-medium lg:gap-10 lg:justify-self-end {offline.available
			? 'flex'
			: 'hidden sm:flex'}"
	>
		{#if offline.available}
			<button
				use:linkMotion={{ hover: false, pressScale: 0.95 }}
				class="flex min-h-11 items-center rounded-md transition-colors hover:text-brand"
				onclick={(event) => offlineDialog.open(event.currentTarget)}
				aria-haspopup="dialog"
				title="Offline"
				><span class="flex items-center gap-1.5"
					><span class="relative grid size-8 place-items-center" aria-hidden="true">
						{#if ring !== undefined}<svg
								viewBox="0 0 32 32"
								class="absolute inset-0 -rotate-90"
								fill="none"
								stroke-width="2"
								><circle cx="16" cy="16" r="15" class="stroke-white/10" /><circle
									cx="16"
									cy="16"
									r="15"
									pathLength="1"
									stroke-dasharray="1"
									stroke-dashoffset={1 - ring}
									stroke-linecap="round"
									class="stroke-brand motion-safe:transition-[stroke-dashoffset] motion-safe:duration-300"
								/></svg
							>{/if}
						{#each [{ state: 'ready', Icon: IconCloudCheck }, { state: 'offline', Icon: IconCloudOff }, { state: 'saving', Icon: IconCloudDownload }] as { state, Icon } (state)}
							<Icon
								size={ring === undefined ? 21 : 16}
								class="col-start-1 row-start-1 motion-safe:transition-[opacity,transform] motion-safe:duration-200 {offlineState ===
								state
									? 'scale-100 opacity-100'
									: 'scale-50 opacity-0'}"
							/>
						{/each}
					</span><span class="sr-only">Offline</span></span
				></button
			>
		{/if}
		<a
			use:linkMotion={{ hover: false, pressScale: 0.95 }}
			href="https://github.com/hexdump0/plico"
			target="_blank"
			rel="noopener noreferrer"
			class="hidden min-h-11 items-center gap-1.5 rounded-md transition-colors hover:text-brand sm:flex"
			><span class="flex items-center gap-1.5"
				><IconBrandGithub size={21} aria-hidden="true" /><span class="hidden md:inline">Github</span
				><span class="sr-only md:hidden">Github</span></span
			></a
		>
		<!-- The path is resolved; the lint rule only accepts a bare resolve() call, not one with a hash. -->
		<!-- eslint-disable svelte/no-navigation-without-resolve -->
		<a
			use:linkMotion={{ hover: false, pressScale: 0.95 }}
			href={`${resolve('/about')}#support`}
			onclick={scrollToSupport}
			class="hidden min-h-11 items-center gap-1 rounded-xl bg-brand px-4 py-2.5 text-panel transition-colors hover:bg-violet-300 sm:flex"
			><span class="flex items-center gap-1"
				><IconCurrencyDollar size={18} aria-hidden="true" />Donate</span
			></a
		>
		<!-- eslint-enable svelte/no-navigation-without-resolve -->
	</div>
</header>

<ToolsDialog bind:this={toolsDialog} {onselect} selectionTarget={() => toolsButton} />
<OfflineDialog bind:this={offlineDialog} />
