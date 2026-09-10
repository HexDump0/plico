import gsap from 'gsap';

function spring(progress: number) {
	const frequency = 9;
	const time = frequency * progress;
	return (1 - (1 + time) * Math.exp(-time)) / (1 - (1 + frequency) * Math.exp(-frequency));
}

export function linkMotion(
	node: HTMLElement,
	{ hover = true, pressScale = 0.9 }: { hover?: boolean; pressScale?: number } = {}
) {
	const content = node.querySelector('span');
	if (!content) return;
	const media = gsap.matchMedia();

	media.add('(prefers-reduced-motion: no-preference)', (context) => {
		let hovered = false;
		let focused = false;
		let pressed = false;

		context.add('animate', () => {
			const highlighted = hover && (hovered || focused);
			gsap.to(content, {
				scale: pressed ? pressScale : highlighted ? 1.08 : 1,
				y: !pressed && highlighted ? -1 : 0,
				duration: pressed ? 0.12 : 0.28,
				ease: pressed ? 'power2.out' : spring,
				overwrite: true
			});
		});

		const enter = (event: PointerEvent) => {
			hovered = event.pointerType === 'mouse';
			context.animate();
		};
		const leave = () => {
			hovered = false;
			context.animate();
		};
		const focus = () => {
			focused = node.matches(':focus-visible');
			context.animate();
		};
		const blur = () => {
			focused = false;
			pressed = false;
			context.animate();
		};
		const press = (event: PointerEvent) => {
			if (event.button !== 0) return;
			pressed = true;
			context.animate();
		};
		const release = () => {
			if (!pressed) return;
			pressed = false;
			context.animate();
		};
		const keydown = (event: KeyboardEvent) => {
			if (event.repeat || !['Enter', ' '].includes(event.key)) return;
			if (event.key === ' ' && node.tagName === 'A') return;
			pressed = true;
			context.animate();
		};
		const keyup = (event: KeyboardEvent) => {
			if (['Enter', ' '].includes(event.key)) release();
		};
		const events = new AbortController();
		const options = { signal: events.signal };
		node.addEventListener('pointerenter', enter, options);
		node.addEventListener('pointerleave', leave, options);
		node.addEventListener('pointerdown', press, options);
		node.addEventListener('focus', focus, options);
		node.addEventListener('blur', blur, options);
		node.addEventListener('keydown', keydown, options);
		node.addEventListener('keyup', keyup, options);
		window.addEventListener('pointerup', release, options);
		window.addEventListener('pointercancel', release, options);
		window.addEventListener('blur', blur, options);
		return () => events.abort();
	});

	return { destroy: () => media.revert() };
}
