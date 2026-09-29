// Scrolls the page while a drag holds the pointer near the top or bottom of the
// visible part of `area`, the region cards are dragged within. Pointer capture
// and `touch-action: none` leave no other way to scroll mid-drag. `onscroll`
// receives how far the page actually moved so the dragged card can stay under
// the pointer.
export function edgeScroll(area: HTMLElement | null, onscroll: (delta: number) => void) {
	let pointerY: number | null = null;
	let frame = 0;
	let lastFrame = 0;

	function speed(y: number) {
		const bounds = area?.getBoundingClientRect();
		const top = Math.max(0, bounds?.top ?? 0);
		const bottom = Math.min(window.innerHeight, bounds?.bottom ?? window.innerHeight);
		if (bottom <= top) return 0;
		const zone = Math.min(96, (bottom - top) * 0.15);
		const fromTop = top + zone - y;
		const fromBottom = y - (bottom - zone);
		if (fromTop > 0) return -1100 * Math.min(1, fromTop / zone) ** 2;
		if (fromBottom > 0) return 1100 * Math.min(1, fromBottom / zone) ** 2;
		return 0;
	}

	function step(time: number) {
		const velocity = pointerY === null ? 0 : speed(pointerY);
		if (!velocity) {
			frame = 0;
			return;
		}
		const dt = Math.min((time - lastFrame) / 1000, 0.05);
		lastFrame = time;
		const before = window.scrollY;
		window.scrollBy(0, velocity * dt);
		const delta = window.scrollY - before;
		if (delta) onscroll(delta);
		frame = requestAnimationFrame(step);
	}

	return {
		update(y: number | null) {
			pointerY = y;
			if (!frame && y !== null && speed(y)) {
				lastFrame = performance.now();
				frame = requestAnimationFrame(step);
			}
		},
		stop() {
			pointerY = null;
			cancelAnimationFrame(frame);
			frame = 0;
		}
	};
}
