<script lang="ts">
	import { onNavigate } from '$app/navigation';
	import { onMount } from 'svelte';
	import gsap from 'gsap';

	let active: ViewTransition | undefined;
	let motion: gsap.core.Timeline | undefined;
	const properties = ['--hero-x', '--about-x', '--strip-y', '--about-selection'];

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
		const betweenPages = (from === '/' && to === '/about') || (from === '/about' && to === '/');
		if (
			!betweenPages ||
			!document.startViewTransition ||
			window.matchMedia('(prefers-reduced-motion: reduce)').matches
		)
			return;

		const enteringAbout = to === '/about';
		const root = document.documentElement;
		const distance = window.innerWidth;
		const strip = document.querySelector('.feature-strip')?.getBoundingClientRect();
		const stripDistance = strip
			? Math.max(strip.height, window.innerHeight - strip.top)
			: window.innerHeight;
		root.dataset.pageTransition = 'active';
		gsap.set(root, {
			'--hero-x': enteringAbout ? '0px' : `${-distance}px`,
			'--about-x': enteringAbout ? `${distance}px` : '0px',
			'--strip-y': enteringAbout ? '0px' : `${stripDistance}px`,
			'--about-selection': enteringAbout ? 0 : 1
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
					const incomingStrip = document.querySelector('.feature-strip')?.getBoundingClientRect();
					if (!enteringAbout && incomingStrip) {
						gsap.set(root, {
							'--strip-y': `${Math.max(incomingStrip.height, window.innerHeight - incomingStrip.top)}px`
						});
					}
					motion = gsap.timeline({ onComplete: finish });
					motion.to(
						root,
						{ '--about-selection': enteringAbout ? 1 : 0, duration: 0.24, ease: 'power2.out' },
						0
					);
					motion.to(
						root,
						{
							'--hero-x': enteringAbout ? `${-distance}px` : '0px',
							'--about-x': enteringAbout ? '0px' : `${distance}px`,
							duration: 0.64,
							ease: spring
						},
						0
					);
					motion.to(
						root,
						{
							'--strip-y': enteringAbout ? `${stripDistance}px` : '0px',
							duration: enteringAbout ? 0.38 : 0.5,
							ease: enteringAbout ? 'power2.inOut' : spring
						},
						enteringAbout ? 0 : 0.1
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
	:global(html[data-page-transition] .hero-stage) {
		view-transition-name: hero-content;
	}
	:global(html[data-page-transition] .about-shell main) {
		view-transition-name: about-content;
	}
	:global(html[data-page-transition] .feature-strip) {
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
	:global(html[data-page-transition]::view-transition-old(hero-content)),
	:global(html[data-page-transition]::view-transition-new(hero-content)) {
		transform: translateX(var(--hero-x));
	}
	:global(html[data-page-transition]::view-transition-old(about-content)),
	:global(html[data-page-transition]::view-transition-new(about-content)) {
		transform: translateX(var(--about-x));
	}
	:global(html[data-page-transition]::view-transition-old(feature-strip)),
	:global(html[data-page-transition]::view-transition-new(feature-strip)) {
		transform: translateY(var(--strip-y));
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
