import { tick } from 'svelte';

/// True when focus sits on something other than the page itself, which is
/// where it lands after the focused element is removed.
export function focusMoved() {
	const focused = document.activeElement;
	return !!focused && focused !== document.body;
}

/// Runs `remove` and, if focus was on one of the `selector` controls inside
/// `list`, moves it to the control that now sits in the same place, or the
/// last one if the removed control was last. Without this the focused button
/// goes with its item and focus drops back to the start of the page.
export async function removeKeepingFocus(
	list: ParentNode | null | undefined,
	selector: string,
	remove: () => void
) {
	const focused = document.activeElement;
	const before = list ? [...list.querySelectorAll<HTMLElement>(selector)] : [];
	const index = focused instanceof HTMLElement ? before.indexOf(focused) : -1;
	remove();
	if (index < 0) return;
	await tick();
	if (focusMoved()) return;
	const after = list ? [...list.querySelectorAll<HTMLElement>(selector)] : [];
	const target = after[Math.min(index, after.length - 1)];
	if (target?.isConnected) target.focus();
}
