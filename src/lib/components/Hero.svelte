<script lang="ts">
	import { onMount } from 'svelte';
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

	onMount(() => {
		const motion = gsap.matchMedia();
		motion.add(
			'(prefers-reduced-motion: no-preference)',
			() => {
				gsap
					.timeline({ defaults: { ease: 'power2.out', clearProps: 'transform,opacity' } })
					.from('.hero-copy > *', { y: 14, opacity: 0, duration: 0.55, stagger: 0.08 })
					.from('.drop-zone', { y: 12, opacity: 0, duration: 0.5 }, 0.1)
					.from('.tool-chip', { x: -8, opacity: 0, duration: 0.4, stagger: 0.045 }, 0.2)
					.from('.connections', { opacity: 0, duration: 0.55 }, 0.35);
			},
			root
		);
		return () => {
			motion.revert();
			gsap.killTweensOf(root.querySelectorAll('[data-connection]'));
		};
	});
</script>

<div bind:this={root} class="hero-shell relative isolate mx-auto max-w-[120rem] overflow-clip">
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
	<SiteHeader bind:this={header} onselect={selectTool} />
	<main id="main-content" class="hero-main">
		<section
			aria-labelledby="hero-title"
			class="hero-stage relative px-6 pt-10 pb-16 sm:px-10 sm:pt-14 lg:h-[clamp(28rem,28.65vw,34.375rem)] lg:p-0"
		>
			<div
				class="hero-copy relative z-10 mb-10 sm:mb-14 lg:absolute lg:top-[44%] lg:left-[6.1458%] lg:mb-0 lg:-translate-y-1/2"
			>
				<h1
					id="hero-title"
					class="text-[clamp(1.875rem,5vw,3rem)] leading-[1.18] font-bold tracking-[-0.025em] lg:text-[clamp(2rem,2.3vw,2.75rem)] lg:tracking-[-0.02em]"
				>
					All the PDF tools<br />You would ever want
				</h1>
				<p class="mt-4 text-sm text-subtle sm:text-base">With a really cool UI</p>
			</div>
			<div
				class="tool-scene relative z-10 lg:absolute lg:top-[42%] lg:right-[5.15625%] lg:aspect-[989/446] lg:w-[48%] lg:max-w-[55rem] lg:-translate-y-1/2"
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
							class="tool-chip flex min-h-11 items-center justify-center gap-2 rounded-xl bg-panel px-4 py-3 text-sm font-bold whitespace-nowrap transition-[background-color,box-shadow,translate] duration-200 hover:-translate-y-0.5 hover:bg-panel-hover sm:gap-3 sm:px-5 sm:text-base lg:absolute lg:h-[12.556%] lg:min-h-10 lg:w-[18.2%] lg:gap-1 lg:px-0 lg:py-0 lg:text-[clamp(0.8125rem,0.82vw,1rem)] xl:gap-2.5 {tool.color} {selectedTool ===
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
								class="size-5 shrink-0 xl:size-[1.375rem]"
								stroke={1.8}
								aria-hidden="true"
							/>{tool.label}</button
						>
					{/each}
					<button
						class="tool-chip flex min-h-11 items-center justify-center gap-2 rounded-xl bg-panel px-4 py-3 text-sm font-bold whitespace-nowrap text-brand transition-[background-color,translate] duration-200 hover:-translate-y-0.5 hover:bg-panel-hover sm:gap-3 sm:px-5 sm:text-base lg:absolute lg:h-[12.556%] lg:min-h-10 lg:w-[18.2%] lg:gap-1 lg:px-0 lg:py-0 lg:text-[clamp(0.8125rem,0.82vw,1rem)] xl:gap-2.5"
						style:--chip-left="11.73%"
						style:--chip-top="87.44%"
						onclick={() => header.openTools()}
						aria-haspopup="dialog"
						onpointerenter={() => animateConnection('more')}
						onfocus={() => animateConnection('more')}
						><IconPlus
							class="size-5 shrink-0 xl:size-[1.375rem]"
							stroke={1.8}
							aria-hidden="true"
						/>More Tools</button
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
	@media (min-width: 1024px) and (min-height: 850px) {
		.hero-shell {
			display: flex;
			height: 100svh;
			flex-direction: column;
		}
		.hero-main {
			display: flex;
			min-height: 0;
			flex: 1;
			flex-direction: column;
		}
		.hero-stage {
			min-height: 0;
			height: auto;
			flex: 1;
		}
	}
</style>
