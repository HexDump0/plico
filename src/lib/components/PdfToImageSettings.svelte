<script lang="ts">
	import { slide } from 'svelte/transition';

	let advancedOpen = $state(false);
	let {
		format = $bindable<'jpg' | 'png'>(),
		dpi = $bindable<number>(),
		quality = $bindable<number>(),
		pageRange = $bindable<string>(),
		pageCount = 0
	}: {
		format: 'jpg' | 'png';
		dpi: number;
		quality: number;
		pageRange: string;
		pageCount?: number;
	} = $props();

	const formats = [
		{ id: 'jpg', label: 'JPG' },
		{ id: 'png', label: 'PNG' }
	] as const;

	const resolutions = [
		{ dpi: 72, label: '72 DPI' },
		{ dpi: 150, label: '150 DPI' },
		{ dpi: 300, label: '300 DPI' }
	] as const;
</script>

<div class="space-y-6">
	<div>
		<h2 class="mb-3 text-sm font-semibold">Image format</h2>
		<div
			class="grid grid-cols-2 gap-1 rounded-xl bg-canvas p-1"
			role="group"
			aria-label="Image format"
		>
			{#each formats as option (option.id)}
				<button
					type="button"
					aria-pressed={format === option.id}
					class="rounded-lg px-2 py-3 text-xs font-semibold motion-safe:transition-colors {format ===
					option.id
						? 'bg-panel-hover text-split'
						: 'text-muted hover:text-white'}"
					onclick={() => (format = option.id)}>{option.label}</button
				>
			{/each}
		</div>
	</div>

	<div>
		<div class="mb-3 flex items-center justify-between">
			<h2 class="text-sm font-semibold">Resolution</h2>
			<span class="text-xs text-muted"
				>{dpi === 300 ? 'Print' : dpi === 150 ? 'Standard' : 'Screen'}</span
			>
		</div>
		<div
			class="grid grid-cols-3 gap-1 rounded-xl bg-canvas p-1"
			role="group"
			aria-label="Resolution"
		>
			{#each resolutions as option (option.dpi)}
				<button
					type="button"
					aria-pressed={dpi === option.dpi}
					class="rounded-lg px-2 py-3 text-xs font-semibold motion-safe:transition-colors {dpi ===
					option.dpi
						? 'bg-panel-hover text-split'
						: 'text-muted hover:text-white'}"
					onclick={() => (dpi = option.dpi)}>{option.label}</button
				>
			{/each}
		</div>
	</div>

	<div>
		<button
			type="button"
			aria-expanded={advancedOpen}
			aria-controls="image-advanced-options"
			class="flex w-full items-center justify-between text-sm font-medium text-muted hover:text-white"
			onclick={() => (advancedOpen = !advancedOpen)}
		>
			Advanced options
			<svg
				aria-hidden="true"
				class="size-4 motion-safe:transition-transform {advancedOpen ? 'rotate-180' : ''}"
				viewBox="0 0 24 24"
				fill="none"
				stroke="currentColor"
				stroke-width="1.75"
				stroke-linecap="round"
				stroke-linejoin="round"
			>
				<path d="m6 9 6 6 6-6" />
			</svg>
		</button>

		{#if advancedOpen}
			<div id="image-advanced-options" class="space-y-4 pt-4" transition:slide={{ duration: 250 }}>
				{#if format === 'jpg'}
					<div class="space-y-2">
						<div class="flex items-center justify-between text-xs font-medium text-muted">
							<span>JPG quality</span>
							<span class="font-mono text-white">{quality}%</span>
						</div>
						<input
							type="range"
							min="30"
							max="100"
							step="5"
							bind:value={quality}
							aria-label="JPG image quality"
							class="w-full cursor-pointer accent-split"
						/>
						<div class="flex justify-between text-[11px] text-muted">
							<span>Smaller file</span>
							<span>High quality</span>
						</div>
					</div>
				{/if}

				<div class="space-y-1.5">
					<label for="page-range-input" class="block text-xs font-medium text-muted">
						Page range
						{#if pageCount > 0}
							<span class="text-[11px] text-muted/70">({pageCount} pages total)</span>
						{/if}
					</label>
					<div
						class="flex items-center rounded-xl border border-white/10 bg-canvas px-3 focus-within:border-split/50"
					>
						<input
							id="page-range-input"
							type="text"
							bind:value={pageRange}
							placeholder="All pages (e.g. 1-3, 5)"
							aria-label="Page range"
							class="min-w-0 flex-1 bg-transparent py-2.5 text-xs text-white outline-none placeholder:text-white/25"
						/>
					</div>
				</div>
			</div>
		{/if}
	</div>
</div>
