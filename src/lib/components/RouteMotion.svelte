<script lang="ts">
	import { onNavigate } from '$app/navigation';
	import { onMount } from 'svelte';
	import gsap from 'gsap';
	import { quickTools, toolColumns } from '$lib/tool-catalog';

	let active: ViewTransition | undefined;
	let motion: gsap.core.Timeline | undefined;
	const toolOrder = [
		...quickTools,
		...toolColumns.flatMap((column) => column.flatMap((group) => group.tools))
	].map((tool) => tool.id);
	const properties = [
		'--hero-x',
		'--about-x',
		'--strip-y',
		'--about-selection',
		'--detail-old-x',
		'--detail-new-x'
	];

	function spring(progress: number) {
		const frequency = 8;
		const time = frequency * progress;
		return (1 - (1 + time) * Math.exp(-time)) / (1 - (1 + frequency) * Math.exp(-frequency));
	}

	function finish() {
		motion?.kill();
		motion = undefined;
		active?.skipTransition();
		active = undefined;
		delete document.documentElement.dataset.pageTransition;
		for (const property of properties) document.documentElement.style.removeProperty(property);
	}

	onMount(() => {
		const preference = window.matchMedia('(prefers-reduced-motion: reduce)');
		const reduceMotion = () => {
			if (preference.matches) finish();
		};
		preference.addEventListener('change', reduceMotion);
		window.addEventListener('resize', finish);
		return () => {
			finish();
			preference.removeEventListener('change', reduceMotion);
			window.removeEventListener('resize', finish);
		};
	});

	onNavigate((navigation) => {
		finish();
		const from = navigation.from?.route.id;
		const to = navigation.to?.route.id;
		const isDetail = (route: string | null | undefined) =>
			route === '/about' || route === '/tools/[tool]';
		const betweenPages = (from === '/' && isDetail(to)) || (isDetail(from) && to === '/');
		const betweenToolAndAbout =
			(from === '/tools/[tool]' && to === '/about') ||
			(from === '/about' && to === '/tools/[tool]');
		const betweenTools =
			from === '/tools/[tool]' &&
			to === '/tools/[tool]' &&
			navigation.from?.params?.tool !== navigation.to?.params?.tool;
		if (
			(!betweenPages && !betweenToolAndAbout && !betweenTools) ||
			!document.startViewTransition ||
			window.matchMedia('(prefers-reduced-motion: reduce)').matches
		)
			return;

		const enteringDetail = to !== '/';
		const root = document.documentElement;
		const distance = window.innerWidth;
		const fromToolIndex = toolOrder.indexOf(navigation.from?.params?.tool ?? '');
		const toToolIndex = toolOrder.indexOf(navigation.to?.params?.tool ?? '');
		let detailDirection = to === '/about' ? 1 : -1;
		if (betweenTools) detailDirection = toToolIndex < fromToolIndex ? -1 : 1;
		const strip = document.querySelector('.feature-strip')?.getBoundingClientRect();
		const stripDistance = strip
			? Math.max(strip.height, window.innerHeight - strip.top)
			: window.innerHeight;
		root.dataset.pageTransition = betweenToolAndAbout || betweenTools ? 'detail-swap' : 'home-swap';
		gsap.set(root, {
			'--hero-x': enteringDetail ? '0px' : `${-distance}px`,
			'--about-x': enteringDetail ? `${distance}px` : '0px',
			'--strip-y': enteringDetail ? '0px' : `${stripDistance}px`,
			'--about-selection': from === '/about' ? 1 : 0,
			'--detail-old-x': '0px',
			'--detail-new-x': `${detailDirection * distance}px`
		});

		return new Promise<void>((resume) => {
			const transition = document.startViewTransition(async () => {
				resume();
				await navigation.complete;
			});
			active = transition;
			void transition.ready
				.then(() => {
					if (active !== transition) return;
					if (betweenToolAndAbout || betweenTools) {
						motion = gsap.timeline({ onComplete: finish });
						motion.to(
							root,
							{
								'--detail-old-x': `${detailDirection * -distance}px`,
								'--detail-new-x': '0px',
								'--about-selection': to === '/about' ? 1 : 0,
								duration: 0.58,
								ease: spring
							},
							0
						);
						return;
					}
					const incomingStrip = document.querySelector('.feature-strip')?.getBoundingClientRect();
					if (!enteringDetail && incomingStrip) {
						gsap.set(root, {
							'--strip-y': `${Math.max(incomingStrip.height, window.innerHeight - incomingStrip.top)}px`
						});
					}
					motion = gsap.timeline({ onComplete: finish });
					motion.to(
						root,
						{ '--about-selection': to === '/about' ? 1 : 0, duration: 0.24, ease: 'power2.out' },
						0
					);
					motion.to(
						root,
						{
							'--hero-x': enteringDetail ? `${-distance}px` : '0px',
							'--about-x': enteringDetail ? '0px' : `${distance}px`,
							duration: 0.64,
							ease: spring
						},
						0
					);
					motion.to(
						root,
						{
							'--strip-y': enteringDetail ? `${stripDistance}px` : '0px',
							duration: enteringDetail ? 0.38 : 0.5,
							ease: enteringDetail ? 'power2.inOut' : spring
						},
						enteringDetail ? 0 : 0.1
					);
				})
				.catch(() => {
					if (active === transition) finish();
				});
			void transition.finished
				.finally(() => {
					resume();
					if (active === transition) finish();
				})
				.catch(() => {});
		});
	});
</script>

<style>
	:global(html[data-page-transition] .about-selection) {
		view-transition-name: about-selection;
	}
	:global(html[data-page-transition]::view-transition-group(about-selection)) {
		z-index: 1;
	}
	:global(html[data-page-transition]::view-transition-old(about-selection)),
	:global(html[data-page-transition]::view-transition-new(about-selection)) {
		opacity: var(--about-selection);
		transform: scale(calc(0.94 + var(--about-selection) * 0.06));
	}
	:global(html[data-page-transition='home-swap'] .hero-stage) {
		view-transition-name: hero-content;
	}
	:global(html[data-page-transition] .about-shell main),
	:global(html[data-page-transition] .tool-shell main) {
		view-transition-name: about-content;
	}
	:global(html[data-page-transition='home-swap'] .feature-strip) {
		view-transition-name: feature-strip;
	}
	:global(html[data-page-transition] header) {
		view-transition-name: site-header;
	}

	:global(html[data-page-transition]::view-transition) {
		pointer-events: none;
	}
	:global(html[data-page-transition]::view-transition-group(*)) {
		animation: none;
	}
	:global(html[data-page-transition]::view-transition-old(*)),
	:global(html[data-page-transition]::view-transition-new(*)) {
		animation: none;
		mix-blend-mode: normal;
	}
	:global(html[data-page-transition]::view-transition-old(root)),
	:global(html[data-page-transition]::view-transition-old(site-header)) {
		display: none;
	}
	:global(html[data-page-transition]::view-transition-group(root)) {
		animation: hold-transition 1s linear;
	}
	:global(html[data-page-transition]::view-transition-group(site-header)) {
		z-index: 2;
	}
	:global(html[data-page-transition='home-swap']::view-transition-old(hero-content)),
	:global(html[data-page-transition='home-swap']::view-transition-new(hero-content)) {
		transform: translateX(var(--hero-x));
	}
	:global(html[data-page-transition='home-swap']::view-transition-old(about-content)),
	:global(html[data-page-transition='home-swap']::view-transition-new(about-content)) {
		transform: translateX(var(--about-x));
	}
	:global(html[data-page-transition='home-swap']::view-transition-old(feature-strip)),
	:global(html[data-page-transition='home-swap']::view-transition-new(feature-strip)) {
		transform: translateY(var(--strip-y));
	}
	:global(html[data-page-transition='detail-swap']::view-transition-old(about-content)) {
		transform: translateX(var(--detail-old-x));
	}
	:global(html[data-page-transition='detail-swap']::view-transition-new(about-content)) {
		transform: translateX(var(--detail-new-x));
	}
	@keyframes -global-hold-transition {
		from {
			opacity: 1;
		}
		to {
			opacity: 1;
		}
	}
</style>
