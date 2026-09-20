import { cubicOut } from 'svelte/easing';
import { slide, type TransitionConfig } from 'svelte/transition';

export function rangeReveal(
	node: Element,
	{ reducedMotion, preview = false }: { reducedMotion: boolean; preview?: boolean }
): TransitionConfig {
	const motion = slide(node, {
		duration: reducedMotion ? 0 : preview ? 260 : 220,
		easing: cubicOut
	});
	if (!preview || reducedMotion) return motion;
	return {
		...motion,
		css: (t, u) =>
			(motion.css?.(t, u) ?? '').replace(
				/opacity:[^;]+;/,
				`opacity: ${Math.max(0, Math.min(1, (t - 0.75) * 4))};`
			)
	};
}

export function rangeCollapse(
	node: Element,
	{ reducedMotion, preview = false }: { reducedMotion: boolean; preview?: boolean }
): TransitionConfig {
	const motion = slide(node, {
		duration: reducedMotion ? 0 : preview ? 240 : 200,
		easing: cubicOut
	});
	return {
		...motion,
		css: (t, u) => (motion.css?.(t, u) ?? '').replace(/opacity:[^;]+;/, 'opacity: 1;')
	};
}
