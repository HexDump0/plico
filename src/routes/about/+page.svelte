<script lang="ts">
	import '@fontsource/fira-code/700.css';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import {
		IconArrowUpRight,
		IconCurrencyDollar,
		IconHexagon,
		IconLink,
		IconBrandX,
		IconUsersGroup
	} from '@tabler/icons-svelte-runes';
	import { linkMotion } from '$lib/motion/link';
	let linkNotice = $state('');
	import SiteHeader from '$lib/components/SiteHeader.svelte';
	import AboutPrinciple from '$lib/components/AboutPrinciple.svelte';

	const creators = [
		{ name: 'HexDump0', role: 'Frontend & design', avatar: '/hexdump0.png' },
		{ name: 'Plasma4', role: 'Backend', avatar: '/plasma4.png' }
	];
	const principles = [
		{
			id: 'local',
			title: 'Your files stay yours.',
			body: 'We’re building Plico around local processing. The idea is simple: work on your documents right on your device, without handing them over to a server.'
		},
		{
			id: 'open',
			title: 'Good tools should be open.',
			body: 'Plico is an open-source project. We want people to be able to understand how it works, question the decisions, and help make it better.'
		},
		{
			id: 'simple',
			title: 'Less between you and done.',
			body: 'Choose your files, pick a tool, save the result. We’re keeping the interface focused on those everyday tasks, with clear controls and fewer interruptions.'
		}
	];
</script>

<svelte:head>
	<title>About — Plico</title>
	<meta
		name="description"
		content="Meet the people behind Plico, an open-source PDF toolbox being built for simple, private, local document processing."
	/>
</svelte:head>

<div class="about-shell flex min-h-svh flex-col">
	<SiteHeader onselect={(tool) => goto(resolve('/tools/[tool]', { tool: tool.id }))} />
	<main
		id="main-content"
		class="mx-auto grid w-full max-w-7xl flex-1 content-center gap-12 px-6 pt-10 pb-16 sm:px-10 sm:pt-14 sm:pb-20 lg:grid-cols-12 lg:gap-20 lg:px-16 lg:py-20"
	>
		<section aria-labelledby="about-heading" class="min-w-0 lg:col-span-8">
			<h1 id="about-heading" class="mb-6 flex items-center gap-3 text-3xl font-bold tracking-tight">
				<IconHexagon size={28} class="shrink-0 text-brand" stroke={2} aria-hidden="true" />Why
				Plico?
			</h1>
			<div class="rounded-3xl bg-panel p-6 sm:p-9 lg:p-10">
				<h2
					class="max-w-lg text-3xl leading-tight font-semibold tracking-tight sm:text-4xl xl:text-5xl"
				>
					PDF tools.<br />Made for people.
				</h2>
				<p class="mt-6 max-w-2xl text-base leading-7 text-muted">
					Lorem ipsum dolor sit amet consectetur adipisicing elit. Ad voluptatibus veniam illum
					numquam dolorum? Temporibus sint consequuntur quas. Iure esse fugiat saepe fuga minima
					sunt qui amet nesciunt ex delectus?
				</p>
				<div class="mt-8">
					{#each principles as principle (principle.id)}<AboutPrinciple {...principle} />{/each}
				</div>
			</div>
		</section>
		<div class="space-y-10 lg:order-first lg:col-span-4 lg:space-y-12">
			<section aria-labelledby="creators-heading">
				<h2 id="creators-heading" class="flex items-center gap-3 text-3xl font-bold tracking-tight">
					<IconUsersGroup size={28} class="shrink-0 text-brand" stroke={2} aria-hidden="true" />Made
					by
				</h2>
				<ul class="mt-6 grid gap-3 sm:grid-cols-2 lg:grid-cols-1">
					{#each creators as creator (creator.name)}
						<li class="flex items-center gap-3 rounded-2xl bg-panel p-4">
							<img
								src={creator.avatar}
								alt=""
								width="48"
								height="48"
								class="size-12 shrink-0 rounded-full"
							/>
							<div class="min-w-0 flex-1">
								<h3 class="font-bold">
									{#if creator.name === 'HexDump0'}HexDump<span class="creator-zero">0</span
										>{:else}{creator.name}{/if}
								</h3>
								<p class="mt-1 text-xs text-muted">{creator.role}</p>
							</div>
							<div class="flex shrink-0 items-center">
								<button
									type="button"
									use:linkMotion
									class="flex size-9 items-center justify-center rounded-lg text-muted transition-colors duration-150 hover:text-brand focus-visible:text-brand"
									aria-label={`${creator.name} website (not configured)`}
									title="Website link not configured"
									onclick={() =>
										(linkNotice = `${creator.name}’s website link hasn’t been added yet.`)}
									><span class="flex"><IconLink size={24} aria-hidden="true" /></span></button
								>
								<button
									type="button"
									use:linkMotion
									class="flex size-9 items-center justify-center rounded-lg text-muted transition-colors duration-150 hover:text-brand focus-visible:text-brand"
									aria-label={`${creator.name} on X (not configured)`}
									title="X link not configured"
									onclick={() => (linkNotice = `${creator.name}’s X link hasn’t been added yet.`)}
									><span class="flex"><IconBrandX size={24} aria-hidden="true" /></span></button
								>
							</div>
						</li>
					{/each}
				</ul>
				<p role="status" class="text-sm leading-6 text-muted" class:mt-3={linkNotice !== ''}>
					{linkNotice}
				</p>
			</section>
			<section
				id="support"
				tabindex="-1"
				aria-labelledby="support-heading"
				class="scroll-mt-8 rounded-2xl"
			>
				<h2 id="support-heading" class="flex items-center gap-3 text-3xl font-bold tracking-tight">
					<IconCurrencyDollar
						size={28}
						class="shrink-0 text-brand"
						stroke={2}
						aria-hidden="true"
					/>Donate
				</h2>
				<div class="mt-6 divide-y divide-white/10 rounded-2xl bg-panel px-6">
					{#each ['XMR', 'ETH'] as currency (currency)}
						<div class="flex items-center justify-between gap-4 py-5">
							<span class="text-lg font-semibold">{currency}</span><span class="text-sm text-muted"
								>Not set up yet</span
							>
						</div>
					{/each}
				</div>
				<p class="mt-4 text-sm leading-6 text-muted">
					Donation details will appear here when they’re ready.
				</p>
			</section>
		</div>
	</main>
</div>

<style>
	.creator-zero {
		font-family: 'Fira Code', monospace;
	}
	.about-shell {
		background-image:
			linear-gradient(rgb(11 11 13 / 55%), rgb(11 11 13 / 55%)), url('/hero-contours.svg');
		background-repeat: no-repeat;
		background-position: right bottom;
		background-size: auto 85%;
	}
	@media (max-width: 63.999rem) {
		.about-shell {
			background-image:
				linear-gradient(rgb(11 11 13 / 85%), rgb(11 11 13 / 85%)), url('/hero-contours.svg');
			background-size: auto 55%;
		}
	}
</style>
