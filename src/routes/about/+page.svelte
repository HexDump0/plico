<script lang="ts">
	import '@fontsource/fira-code/700.css';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import {
		IconCurrencyDollar,
		IconHexagon,
		IconLink,
		IconBrandDiscord,
		IconBrandX,
		IconUsersGroup
	} from '@tabler/icons-svelte-runes';
	import { linkMotion } from '$lib/motion/link';
	import SiteHeader from '$lib/components/SiteHeader.svelte';
	import AboutPrinciple from '$lib/components/AboutPrinciple.svelte';
	import CopyButton from '$lib/components/CopyButton.svelte';

	// Discord has no username DM link; /users/<id> opens the profile with a Message button.
	const PLASMA4_DISCORD_ID = '883838726386622524';
	const creators = [
		{
			name: 'HexDump0',
			avatar: '/hexdump0.png',
			links: [
				{ label: 'website', href: 'https://hexdump0.pw/', Icon: IconLink },
				{ label: 'on X', href: 'https://x.com/HexDump0', Icon: IconBrandX }
			]
		},
		{
			name: 'Plasma4',
			avatar: '/plasma4.png',
			links: [
				{ label: 'website', href: 'https://plasma4.org/', Icon: IconLink },
				{
					label: 'on Discord (1_e0)',
					href: `https://discord.com/users/${PLASMA4_DISCORD_ID}`,
					Icon: IconBrandDiscord
				}
			]
		}
	];
	// Donation addresses: paste each one between the quotes. Empty shows "Not set up yet".
	const donations = [{ currency: 'SOL', address: 'Hx15Q2Krg9uHzZ16orK4gjC1xcTJvJEzb5VnwU3SbzVa' }];
	// Keep both ends visible: people check the start and end of an address, not the middle.
	const shortAddress = (address: string) =>
		address.length > 18 ? `${address.slice(0, 7)}...${address.slice(-8)}` : address;
	const principles = [
		{
			id: 'local',
			title: 'Your files stay yours.',
			body: "We're building Plico around local processing. The idea is simple: work on your documents right on your device, without handing them over to a server."
		},
		{
			id: 'open',
			title: 'Good tools should be open.',
			body: 'Plico is an open-source project. We want people to be able to understand how it works, question the decisions, and help make it better.'
		}
	];
</script>

<svelte:head>
	<title>About - Plico</title>
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
					PDfs were one of the thing that were for all their convenience, very annoying to edit and
					modify. And the tools that were available (in my opinion) have terrible UI or push ads
					into your face every second. Plico was made a good, simple, open source alternative to the
					existing tools that runs entirely in your browser
				</p>
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
							<h3 class="min-w-0 flex-1 font-bold">
								{#if creator.name === 'HexDump0'}HexDump<span class="creator-zero">0</span
									>{:else}{creator.name}{/if}
							</h3>
							<div class="flex shrink-0 items-center">
								{#each creator.links as { label, href, Icon } (href)}
									<!-- External profile links; resolve() only applies to app routes. -->
									<!-- eslint-disable svelte/no-navigation-without-resolve -->
									<a
										{href}
										target="_blank"
										rel="noopener noreferrer"
										use:linkMotion
										class="flex size-9 items-center justify-center rounded-lg text-muted transition-colors duration-150 hover:text-brand focus-visible:text-brand"
										aria-label={`${creator.name} ${label}`}
										title={`${creator.name} ${label}`}
										><span class="flex"><Icon size={24} aria-hidden="true" /></span></a
									>
									<!-- eslint-enable svelte/no-navigation-without-resolve -->
								{/each}
							</div>
						</li>
					{/each}
				</ul>
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
				<div class="mt-6 divide-y divide-white/10 rounded-2xl bg-panel px-5 sm:px-6">
					{#each donations as { currency, address } (currency)}
						<div class="flex items-center justify-between gap-4 py-4">
							<span class="shrink-0 text-base font-bold tracking-wide">{currency}</span>
							{#if address}
								<CopyButton
									text={address}
									label={`Copy ${currency} address`}
									display={shortAddress(address)}
								/>
							{:else}
								<span class="text-sm text-muted">Not set up yet</span>
							{/if}
						</div>
					{/each}
				</div>
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
