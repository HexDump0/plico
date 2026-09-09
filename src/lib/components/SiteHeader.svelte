<script lang="ts">
	import { resolve } from '$app/paths';
	import {
		IconChevronDown,
		IconBrandGithub,
		IconCurrencyDollar,
		IconX
	} from '@tabler/icons-svelte-runes';
	import logo from '$lib/assets/plico.svg';
	import { tools, type ToolId } from '$lib/tools';
	import gsap from 'gsap';

	let { onselect }: { onselect: (tool: ToolId) => void } = $props();
	let dialog: HTMLDialogElement;
	let content = $state<'tools' | 'about' | 'github' | 'donate'>('tools');
	const titles = {
		tools: 'PDF tools',
		about: 'A little about Plico',
		github: 'Open by design',
		donate: 'Support Plico'
	};

	export function openTools() {
		open('tools');
	}

	function open(view: typeof content) {
		content = view;
		dialog.showModal();
		if (!window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
			gsap.fromTo(
				dialog,
				{ y: 10, opacity: 0 },
				{ y: 0, opacity: 1, duration: 0.2, ease: 'power2.out', clearProps: 'all' }
			);
		}
	}
</script>

<header
	class="relative z-20 mx-auto flex h-20 w-full shrink-0 items-center justify-between px-6 sm:h-24 sm:px-10 lg:grid lg:h-24 lg:grid-cols-3 lg:px-20"
>
	<a
		href={resolve('/')}
		class="flex shrink-0 items-center gap-2 rounded-md text-2xl font-bold lg:justify-self-start"
		aria-label="Plico home"><img src={logo} width="26" height="30" alt="" />Plico</a
	>
	<nav
		aria-label="Main navigation"
		class="flex items-center gap-4 text-sm font-medium sm:gap-8 sm:text-base lg:gap-9 lg:justify-self-center"
	>
		<button
			class="flex min-h-11 items-center gap-1 rounded-md transition-colors hover:text-brand"
			onclick={openTools}
			aria-haspopup="dialog">Tools <IconChevronDown size={18} aria-hidden="true" /></button
		>
		<button
			class="min-h-11 rounded-md transition-colors hover:text-brand"
			onclick={() => open('about')}
			aria-haspopup="dialog">About</button
		>
	</nav>
	<div
		class="hidden items-center gap-8 text-base font-medium sm:flex lg:gap-10 lg:justify-self-end"
	>
		<button
			class="flex min-h-11 items-center gap-1.5 rounded-md transition-colors hover:text-brand"
			onclick={() => open('github')}
			aria-haspopup="dialog"
			><IconBrandGithub size={21} aria-hidden="true" /><span class="hidden md:inline">Github</span
			><span class="sr-only md:hidden">Github</span></button
		>
		<button
			class="flex min-h-11 items-center gap-1 rounded-xl bg-brand px-4 py-2.5 text-panel transition-colors hover:bg-violet-300"
			onclick={() => open('donate')}
			aria-haspopup="dialog"><IconCurrencyDollar size={18} aria-hidden="true" />Donate</button
		>
	</div>
</header>

<dialog
	bind:this={dialog}
	aria-labelledby="dialog-title"
	class="fixed inset-4 m-auto w-auto max-w-lg overflow-y-auto rounded-3xl border border-white/10 bg-panel p-6 text-white shadow-2xl backdrop:bg-canvas/80 backdrop:backdrop-blur-sm sm:p-8"
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
	<div class="mb-6 flex items-center justify-between gap-4">
		<h2 id="dialog-title" class="text-xl font-bold">{titles[content]}</h2>
		<button
			class="rounded-lg p-2 text-muted hover:bg-white/5 hover:text-white"
			onclick={() => dialog.close()}
			aria-label="Close dialog"><IconX size={22} aria-hidden="true" /></button
		>
	</div>
	{#if content === 'tools'}
		<div class="grid gap-2">
			{#each tools as tool (tool.id)}
				<button
					class="flex items-center gap-4 rounded-xl p-4 text-left transition-colors hover:bg-white/5"
					onclick={() => {
						onselect(tool.id);
						dialog.close();
					}}
					><tool.icon class={tool.color} size={24} aria-hidden="true" /><span
						><span class="block font-semibold {tool.color}">{tool.label}</span><span
							class="mt-1 block text-sm text-muted">{tool.description}</span
						></span
					></button
				>
			{/each}
		</div>
		<p class="mt-5 text-sm leading-relaxed text-muted">
			Choose a tool to set up your files. Processing is coming next.
		</p>
	{:else if content === 'about'}
		<p class="text-base leading-relaxed text-muted">
			Plico is an open-source PDF toolbox being built around a simple idea: working with your
			documents should feel easy, and your files should stay yours.
		</p>
		<p class="mt-4 text-base leading-relaxed text-muted">
			Local processing. A thoughtful interface. No unnecessary steps.
		</p>
	{:else if content === 'github'}
		<p class="leading-relaxed text-muted">
			Plico is being built in the open. A link to the public repository will be available here when
			it launches.
		</p>
	{:else}
		<p class="leading-relaxed text-muted">
			Thanks for wanting to support Plico. Donations aren’t set up yet.
		</p>
	{/if}
	{#if content === 'about'}
		<div class="mt-6 flex gap-6 sm:hidden">
			<button class="text-sm text-brand" onclick={() => (content = 'github')}>Github</button><button
				class="text-sm text-brand"
				onclick={() => (content = 'donate')}>Donate</button
			>
		</div>
	{/if}
</dialog>
