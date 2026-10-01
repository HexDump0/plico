<script lang="ts">
	import { IconBold, IconCheck } from '@tabler/icons-svelte-runes';
	import type { FontFamily } from '$lib/pdf/standard-fonts';

	let {
		family = $bindable<FontFamily>(),
		bold = $bindable<boolean>(),
		size = $bindable<number>(),
		color = $bindable<string>(),
		minSize,
		maxSize,
		disabled = false
	}: {
		family: FontFamily;
		bold: boolean;
		size: number;
		color: string;
		minSize: number;
		maxSize: number;
		disabled?: boolean;
	} = $props();
	const id = $props.id();

	const families = [
		{ id: 'helvetica', label: 'Sans', css: "Helvetica, Arial, 'Liberation Sans', sans-serif" },
		{ id: 'times', label: 'Serif', css: "'Times New Roman', Times, 'Liberation Serif', serif" },
		{ id: 'courier', label: 'Mono', css: "'Courier New', Courier, 'Liberation Mono', monospace" }
	] as const;

	const swatches = [
		{ value: '#000000', label: 'Black' },
		{ value: '#6b7280', label: 'Gray' },
		{ value: '#dc2626', label: 'Red' },
		{ value: '#2563eb', label: 'Blue' },
		{ value: '#ffffff', label: 'White' }
	];
	const custom = $derived(!swatches.some((swatch) => swatch.value === color));
</script>

<div class="space-y-5">
	<div>
		<h2 class="mb-3 text-sm font-semibold">Font</h2>
		<div class="flex rounded-xl bg-canvas p-1">
			<div class="grid flex-1 grid-cols-3 gap-1" role="group" aria-label="Font">
				{#each families as option (option.id)}<button
						type="button"
						aria-pressed={family === option.id}
						{disabled}
						style:font-family={option.css}
						class="rounded-lg px-2 py-2.5 text-[13px] motion-safe:transition-colors {family ===
						option.id
							? 'bg-panel-hover text-brand'
							: 'text-muted hover:text-white'} {bold ? 'font-bold' : 'font-medium'}"
						onclick={() => (family = option.id)}>{option.label}</button
					>{/each}
			</div>
			<span aria-hidden="true" class="mx-1 my-2 w-px bg-white/10"></span>
			<button
				type="button"
				aria-pressed={bold}
				aria-label="Bold"
				title="Bold"
				{disabled}
				onclick={() => (bold = !bold)}
				class="flex w-10 shrink-0 items-center justify-center rounded-lg motion-safe:transition-colors {bold
					? 'bg-panel-hover text-brand'
					: 'text-muted hover:text-white'}"><IconBold size={16} stroke={2.25} /></button
			>
		</div>
	</div>

	<div>
		<div class="mb-3 flex items-center justify-between">
			<label for="{id}-size" class="text-sm font-semibold">Size</label>
			<span class="text-xs text-muted tabular-nums">{size} pt</span>
		</div>
		<input
			id="{id}-size"
			type="range"
			min={minSize}
			max={maxSize}
			step="1"
			bind:value={size}
			{disabled}
			class="w-full cursor-pointer accent-brand"
		/>
	</div>

	<div>
		<h2 class="mb-3 text-sm font-semibold">Color</h2>
		<div class="flex items-center gap-2.5" role="group" aria-label="Color">
			{#each swatches as swatch (swatch.value)}
				<button
					type="button"
					aria-pressed={color === swatch.value}
					aria-label={swatch.label}
					title={swatch.label}
					{disabled}
					onclick={() => (color = swatch.value)}
					style:background-color={swatch.value}
					class="flex size-8 items-center justify-center rounded-full ring-offset-2 ring-offset-panel motion-safe:transition-shadow {color ===
					swatch.value
						? 'ring-2 ring-brand'
						: 'ring-1 ring-white/15 hover:ring-white/40'}"
				>
					{#if color === swatch.value}<IconCheck
							size={15}
							stroke={3}
							class={swatch.value === '#ffffff' ? 'text-canvas' : 'text-white'}
						/>{/if}
				</button>
			{/each}
			<label
				title="Custom color"
				class="relative flex size-8 cursor-pointer items-center justify-center rounded-full ring-offset-2 ring-offset-panel focus-within:outline-2 focus-within:outline-offset-6 focus-within:outline-brand motion-safe:transition-shadow {custom
					? 'ring-2 ring-brand'
					: 'ring-1 ring-white/15 hover:ring-white/40'}"
				style:background={custom
					? color
					: 'conic-gradient(from 180deg, #f87171, #fbbf24, #34d399, #60a5fa, #a78bfa, #f472b6, #f87171)'}
			>
				<input
					type="color"
					value={color}
					{disabled}
					aria-label="Custom color"
					oninput={(event) => (color = event.currentTarget.value)}
					class="absolute inset-0 size-full cursor-pointer opacity-0"
				/>
			</label>
		</div>
	</div>
</div>
