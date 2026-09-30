<script lang="ts">
	import { positionNames, type StampPosition } from '$lib/pdf/stamp-layout';

	let {
		value = $bindable<StampPosition>(),
		positions = [0, 1, 2, 3, 4, 5, 6, 7, 8],
		tiled = false,
		disabled = false
	}: {
		value: StampPosition;
		positions?: StampPosition[];
		tiled?: boolean;
		disabled?: boolean;
	} = $props();

	const cells = Array.from({ length: 9 }, (_, index) => index as StampPosition);
	const buttons: HTMLButtonElement[] = [];
	const inactive = $derived(disabled || tiled);

	// Arrow keys move to the nearest allowed spot in that direction, so a
	// picker without a middle row jumps straight between top and bottom.
	function move(event: KeyboardEvent, from: StampPosition) {
		const step = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] }[
			event.key
		];
		if (!step) return;
		event.preventDefault();
		let [column, row] = [from % 3, Math.floor(from / 3)];
		for (;;) {
			column += step[0];
			row += step[1];
			if (column < 0 || column > 2 || row < 0 || row > 2) return;
			const next = (row * 3 + column) as StampPosition;
			if (positions.includes(next)) {
				value = next;
				buttons[next]?.focus();
				return;
			}
		}
	}
</script>

<div class="flex justify-center rounded-xl bg-canvas px-4 py-5">
	<div
		role="radiogroup"
		aria-label="Position"
		aria-disabled={inactive}
		class="relative grid aspect-[3/4] w-28 grid-cols-3 grid-rows-3 rounded-md border border-white/10 bg-white/[0.03] p-1.5"
	>
		{#each cells as cell (cell)}
			{@const allowed = positions.includes(cell)}
			{@const selected = !tiled && value === cell}
			{#if allowed}
				<button
					bind:this={buttons[cell]}
					type="button"
					role="radio"
					aria-checked={selected}
					aria-label={positionNames[cell]}
					title={inactive ? undefined : positionNames[cell]}
					tabindex={selected || (tiled && cell === 4) ? 0 : -1}
					disabled={inactive}
					onclick={() => (value = cell)}
					onkeydown={(event) => move(event, cell)}
					class="group flex rounded-[5px] p-1 disabled:cursor-default {cell % 3 === 0
						? 'justify-start'
						: cell % 3 === 1
							? 'justify-center'
							: 'justify-end'} {cell < 3 ? 'items-start' : cell < 6 ? 'items-center' : 'items-end'}"
				>
					<span
						aria-hidden="true"
						class="block h-1.5 rounded-full motion-safe:transition-[width,background-color,opacity] motion-safe:duration-200 {selected
							? 'w-7 bg-brand'
							: tiled
								? 'w-5 bg-brand/55'
								: 'w-4 bg-white/20 group-enabled:group-hover:bg-white/45'}"
					></span>
				</button>
			{:else}
				<span aria-hidden="true"></span>
			{/if}
		{/each}
	</div>
</div>
