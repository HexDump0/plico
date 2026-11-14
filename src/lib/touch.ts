import type { Attachment } from 'svelte/attachments';

/// Drawing surfaces take every touch (`touch-action: none`) so a finger draws
/// rather than scrolls. On a phone a page fills the width, so that left nothing
/// to scroll by. With this attached, a second finger turns the touch into a
/// two-finger scroll: `cancel` drops whatever the first finger had started,
/// and the page follows the midpoint of the fingers until they all lift.
///
/// It listens in the capture phase, so the surface's own handlers never see the
/// second finger, nor either finger while scrolling.
export function twoFingerScroll(cancel: () => void): Attachment<HTMLElement> {
	return (node) => {
		const touches = new Map<number, [number, number]>();
		let scrolling = false;
		let last: [number, number] = [0, 0];
		let sideways: HTMLElement | null = null;

		const midpoint = (): [number, number] => {
			let x = 0;
			let y = 0;
			for (const [px, py] of touches.values()) {
				x += px;
				y += py;
			}
			return [x / touches.size, y / touches.size];
		};

		// A zoomed page scrolls sideways inside its preview, and down with the window.
		const sidewaysScroller = () => {
			for (let at = node.parentElement; at; at = at.parentElement) {
				const overflow = getComputedStyle(at).overflowX;
				if ((overflow === 'auto' || overflow === 'scroll') && at.scrollWidth > at.clientWidth)
					return at;
			}
			return null;
		};

		function down(event: PointerEvent) {
			if (event.pointerType !== 'touch') return;
			// The first finger of a new touch: anything still listed lifted unseen.
			if (event.isPrimary) {
				touches.clear();
				scrolling = false;
			}
			touches.set(event.pointerId, [event.clientX, event.clientY]);
			if (!scrolling && touches.size < 2) return;
			event.preventDefault();
			event.stopPropagation();
			if (!scrolling) {
				scrolling = true;
				cancel();
				sideways = sidewaysScroller();
				// Both fingers report here from now on, wherever they move.
				for (const id of touches.keys()) {
					try {
						node.setPointerCapture(id);
					} catch {
						// A finger that already lifted cannot be captured.
					}
				}
			} else {
				node.setPointerCapture(event.pointerId);
			}
			last = midpoint();
		}

		function move(event: PointerEvent) {
			if (!touches.has(event.pointerId)) return;
			touches.set(event.pointerId, [event.clientX, event.clientY]);
			if (!scrolling) return;
			event.preventDefault();
			event.stopPropagation();
			const [x, y] = midpoint();
			const [dx, dy] = [x - last[0], y - last[1]];
			last = [x, y];
			if (sideways) sideways.scrollLeft -= dx;
			window.scrollBy(0, -dy);
		}

		function up(event: PointerEvent) {
			if (!touches.delete(event.pointerId)) return;
			if (scrolling) {
				event.stopPropagation();
				if (touches.size) last = midpoint();
				else scrolling = false;
			}
		}

		node.addEventListener('pointerdown', down, { capture: true });
		node.addEventListener('pointermove', move, { capture: true });
		node.addEventListener('pointerup', up, { capture: true });
		node.addEventListener('pointercancel', up, { capture: true });
		return () => {
			node.removeEventListener('pointerdown', down, { capture: true });
			node.removeEventListener('pointermove', move, { capture: true });
			node.removeEventListener('pointerup', up, { capture: true });
			node.removeEventListener('pointercancel', up, { capture: true });
		};
	};
}
