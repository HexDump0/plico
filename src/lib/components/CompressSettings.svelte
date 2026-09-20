<script lang="ts">
	import { slide } from 'svelte/transition';

	let advancedOpen = $state(false);
	let {
		level = $bindable<'light' | 'balanced' | 'strong'>(),
		removeMetadata = $bindable<boolean>(),
		removeThumbnails = $bindable<boolean>()
	}: {
		level: 'light' | 'balanced' | 'strong';
		removeMetadata: boolean;
		removeThumbnails: boolean;
	} = $props();
	const levels = [
		{ id: 'light', label: 'Light' },
		{
			id: 'balanced',
			label: 'Balanced'
		},
		{
			id: 'strong',
			label: 'High'
		}
	] as const;
</script>

<div class="space-y-6">
	<div>
		<h2 class="mb-3 text-sm font-semibold">Compression</h2>
		<div
			class="grid grid-cols-3 gap-1 rounded-xl bg-canvas p-1"
			role="group"
			aria-label="Compression level"
		>
			{#each levels as option (option.id)}<button
					type="button"
					aria-pressed={level === option.id}
					class="rounded-lg px-2 py-3 text-xs font-semibold motion-safe:transition-colors {level ===
					option.id
						? 'bg-panel-hover text-compress'
						: 'text-muted hover:text-white'}"
					onclick={() => (level = option.id)}>{option.label}</button
				>{/each}
		</div>
	</div>
	<div>
		<button
			type="button"
			aria-expanded={advancedOpen}
			aria-controls="compress-advanced-options"
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
			<div
				id="compress-advanced-options"
				class="space-y-2 pt-4"
				transition:slide={{ duration: 300 }}
			>
				<label
					class="flex cursor-pointer items-center justify-between gap-4 rounded-xl bg-canvas/50 px-3.5 py-3 text-sm hover:border-white/15 motion-safe:transition-colors"
				>
					<span>Remove metadata</span>
					<input type="checkbox" role="switch" class="peer sr-only" bind:checked={removeMetadata} />
					<span
						aria-hidden="true"
						class="relative h-6 w-10 shrink-0 rounded-full bg-white/15 peer-checked:bg-compress peer-focus-visible:outline-2 peer-focus-visible:outline-offset-2 peer-focus-visible:outline-compress motion-safe:transition-colors"
					>
						<span
							class="absolute top-1 left-1 size-4 rounded-full bg-white motion-safe:transition-transform {removeMetadata
								? 'translate-x-4'
								: ''}"
						></span>
					</span>
				</label>
				<label
					class="flex cursor-pointer items-center justify-between gap-4 rounded-xl bg-canvas/50 px-3.5 py-3 text-sm hover:border-white/15 motion-safe:transition-colors"
				>
					<span>Remove page thumbnails</span>
					<input
						type="checkbox"
						role="switch"
						class="peer sr-only"
						bind:checked={removeThumbnails}
					/>
					<span
						aria-hidden="true"
						class="relative h-6 w-10 shrink-0 rounded-full bg-white/15 peer-checked:bg-compress peer-focus-visible:outline-2 peer-focus-visible:outline-offset-2 peer-focus-visible:outline-compress motion-safe:transition-colors"
					>
						<span
							class="absolute top-1 left-1 size-4 rounded-full bg-white motion-safe:transition-transform {removeThumbnails
								? 'translate-x-4'
								: ''}"
						></span>
					</span>
				</label>
			</div>
		{/if}
	</div>
</div>
