<script lang="ts">
	import gsap from 'gsap';
	import { onMount } from 'svelte';
	import { IconPlus } from '@tabler/icons-svelte-runes';

	let { id, title, body }: { id: string; title: string; body: string } = $props();
	let expanded = $state(false);
	let panel: HTMLDivElement;
	let indicator: HTMLSpanElement;
	let animate: (() => void) | undefined;

	onMount(() => {
		const media = gsap.matchMedia();
		media.add(
			{
				motion: '(prefers-reduced-motion: no-preference)',
				reduced: '(prefers-reduced-motion: reduce)'
			},
			(context) => {
				gsap.set(panel, { height: expanded ? 'auto' : 0 });
				gsap.set(indicator, { rotation: expanded ? 45 : 0 });
				const duration = context.conditions?.reduced ? 0 : 0.28;
				context.add('toggle', () => {
					gsap.to(panel, {
						height: expanded ? panel.scrollHeight : 0,
						duration,
						ease: 'power2.inOut',
						overwrite: true,
						onComplete: () => {
							if (expanded) gsap.set(panel, { height: 'auto' });
						}
					});
					gsap.to(indicator, { rotation: expanded ? 45 : 0, duration, overwrite: true });
				});
				animate = () => context.toggle();
				return () => {
					animate = undefined;
				};
			}
		);
		return () => media.revert();
	});

	function toggle() {
		expanded = !expanded;
		if (animate) animate();
		else panel.style.height = expanded ? 'auto' : '0px';
	}
</script>

<div class="border-t border-white/10">
	<h3>
		<button
			id={`${id}-trigger`}
			class="flex min-h-16 w-full items-center justify-between gap-6 py-4 text-left text-sm font-medium transition-colors hover:text-brand sm:text-base"
			aria-expanded={expanded}
			aria-controls={id}
			onclick={toggle}
		>
			{title}<span bind:this={indicator} class="shrink-0 text-brand"
				><IconPlus size={18} aria-hidden="true" /></span
			>
		</button>
	</h3>
	<div
		bind:this={panel}
		{id}
		role="region"
		aria-labelledby={`${id}-trigger`}
		aria-hidden={!expanded}
		class="overflow-hidden"
		style="height: 0px"
	>
		<p class="pb-5 text-sm leading-7 text-muted">{body}</p>
	</div>
</div>
