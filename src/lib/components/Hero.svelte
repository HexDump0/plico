<script lang="ts">
	import gsap from 'gsap';
	import { IconPlus } from '@tabler/icons-svelte-runes';
	import { tools, type ToolId } from '$lib/tools';
	import SiteHeader from './SiteHeader.svelte';
	import PdfDropzone from './PdfDropzone.svelte';
	import FeatureStrip from './FeatureStrip.svelte';

	let root: HTMLDivElement;
	let header: SiteHeader;
	let selectedTool = $state<ToolId | null>(null);
	const positions = [
		{ left: 14.26, top: 0, path: 'M321 28C362.5 28 362.5 108 404 108' },
		{ left: 3.94, top: 21.08, path: 'M219 122C311.5 122 311.5 173 404 173' },
		{ left: 0, top: 43.05, path: 'M180 220C292 220 292 238 404 238' },
		{ left: 3.44, top: 64.8, path: 'M214 317C309 317 309 303 404 303' }
	];

	function selectTool(tool: ToolId) {
		selectedTool = selectedTool === tool ? null : tool;
	}

	function animateConnection(id: string) {
		if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
		gsap.fromTo(
			root.querySelector(`[data-connection="${id}"]`),
			{ strokeDashoffset: 39 },
			{ strokeDashoffset: 0, duration: 0.7, ease: 'power2.out', overwrite: true }
		);
	}
</script>

<div
	bind:this={root}
	class="hero-shell relative isolate mx-auto flex min-h-svh flex-col overflow-clip"
>
	<div class="hero-art pointer-events-none absolute -z-10 select-none" aria-hidden="true">
		<img
			src="/hero-contours.svg"
			alt=""
			width="2048"
			height="1536"
			fetchpriority="high"
			class="h-full w-full opacity-40 sm:opacity-45"
		/>
	</div>
	<SiteHeader bind:this={header} onselect={(tool) => (selectedTool = tool)} />
	<main id="main-content" class="hero-main flex flex-1 flex-col">
		<section
			aria-labelledby="hero-title"
			class="hero-stage relative grid flex-1 items-center gap-10 px-6 py-10 sm:gap-14 sm:px-10 sm:py-14 lg:grid-cols-12 lg:gap-8 lg:px-16 lg:py-20 xl:px-24"
		>
			<div class="hero-copy relative z-10 min-w-0 lg:col-span-5">
				<h1 id="hero-title" class="text-5xl leading-tight font-bold tracking-tight">
					All the PDF tools<br />You would ever want
				</h1>
				<p class="mt-3 text-lg text-subtle font-bold">Blablabla idk what to write here</p>
			</div>
			<div
				class="tool-scene relative z-10 w-full min-w-0 lg:col-span-7 lg:aspect-[989/446] lg:max-w-4xl lg:justify-self-end"
			>
				<svg
					class="connections pointer-events-none absolute inset-0 hidden h-full w-full overflow-visible lg:block"
					viewBox="0 0 989 446"
					fill="none"
					stroke-width="2"
					stroke-dasharray="21 18"
					aria-hidden="true"
				>
					{#each tools as tool, index (tool.id)}
						<path
							d={positions[index].path}
							class={tool.color}
							stroke="currentColor"
							data-connection={tool.id}
						/>
					{/each}
					<path
						d="M296 418C350 418 350 368 404 368"
						class="text-brand"
						stroke="currentColor"
						data-connection="more"
					/>
				</svg>
				<div class="mb-6 flex flex-wrap gap-2 sm:gap-3 lg:contents" aria-label="Choose a PDF tool">
					{#each tools as tool, index (tool.id)}
						<button
							class="tool-chip flex min-h-11 items-center justify-center gap-2 rounded-xl bg-panel px-4 py-3 text-sm font-bold whitespace-nowrap transition-all duration-200 hover:-translate-y-0.5 hover:bg-panel-hover sm:gap-3 sm:px-5 sm:text-base lg:absolute lg:h-[12.556%] lg:min-h-10 lg:w-[18.2%] lg:gap-1 lg:px-0 lg:py-0 lg:text-sm xl:gap-3 xl:text-base {tool.color} {selectedTool ===
							tool.id
								? 'ring-2 ring-current'
								: ''}"
							style:--chip-left={`${positions[index].left}%`}
							style:--chip-top={`${positions[index].top}%`}
							aria-pressed={selectedTool === tool.id}
							onclick={() => selectTool(tool.id)}
							onpointerenter={() => animateConnection(tool.id)}
							onfocus={() => animateConnection(tool.id)}
							><tool.icon
								class="size-5 shrink-0 xl:size-6"
								stroke={1.8}
								aria-hidden="true"
							/>{tool.label}</button
						>
					{/each}
					<button
						class="tool-chip flex min-h-11 items-center justify-center gap-2 rounded-xl bg-panel px-4 py-3 text-sm font-bold whitespace-nowrap text-brand transition-all duration-200 hover:-translate-y-0.5 hover:bg-panel-hover sm:gap-3 sm:px-5 sm:text-base lg:absolute lg:h-[12.556%] lg:min-h-10 lg:w-[18.2%] lg:gap-1 lg:px-0 lg:py-0 lg:text-sm xl:gap-3 xl:text-base"
						style:--chip-left="11.73%"
						style:--chip-top="87.44%"
						onclick={() => header.openTools()}
						aria-haspopup="dialog"
						onpointerenter={() => animateConnection('more')}
						onfocus={() => animateConnection('more')}
						><IconPlus class="size-5 shrink-0 xl:size-6" stroke={1.8} aria-hidden="true" />More
						Tools</button
					>
				</div>
				<div class="h-80 sm:h-88 lg:absolute lg:top-[13.9%] lg:right-0 lg:h-[84.08%] lg:w-[59.15%]">
					<PdfDropzone {selectedTool} />
				</div>
			</div>
		</section>
		<FeatureStrip />
	</main>
</div>

<style>
	.hero-art {
		width: 1000px;
		height: 750px;
		right: -500px;
		top: 110px;
	}
	@media (min-width: 640px) {
		.hero-art {
			width: 1400px;
			height: 1050px;
			right: -620px;
			top: 10px;
		}
	}
	@media (min-width: 1024px) {
		.hero-art {
			width: 75%;
			height: auto;
			aspect-ratio: 1561 / 1170;
			left: 30%;
			right: auto;
			top: 0;
		}
		.tool-chip {
			left: var(--chip-left);
			top: var(--chip-top);
		}
	}
</style>
