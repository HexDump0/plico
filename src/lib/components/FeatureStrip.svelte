<script lang="ts">
	import {
		IconCloudOff,
		IconBrandOpenSource,
		IconBrandRust,
		IconLock
	} from '@tabler/icons-svelte-runes';

	const features = [
		{ icon: IconCloudOff, title: 'Fully local', body: 'Your files never leave this device.' },
		{ icon: IconBrandOpenSource, title: 'Open source', body: 'Read, audit and contribute.' },
		{ icon: IconBrandRust, title: 'Built with Rust', body: 'Fast, lightweight processing.' },
		{ icon: IconLock, title: 'Private by default', body: 'No server-side file storage.' }
	];
</script>

<section
	class="feature-strip relative z-10 shrink-0 rounded-t-4xl bg-brand text-panel lg:mx-auto lg:w-7/8 lg:max-w-432"
>
	<div
		class="feature-grid mx-auto grid grid-cols-2 gap-y-10 px-4 py-10 sm:px-8 lg:min-h-52 lg:grid-cols-4 lg:items-center lg:gap-y-0 lg:px-6 lg:py-8 xl:min-h-56 2xl:min-h-60"
	>
		{#each features as feature, index (feature.title)}
			<div
				class="feature flex flex-col items-center px-3 text-center lg:justify-start lg:px-4 {index %
					2 ===
				1
					? 'border-l-2 border-panel/20'
					: ''} {index > 0 ? 'lg:border-l-2 lg:border-panel/20' : ''} lg:min-h-36"
			>
				<feature.icon
					class="mb-4 size-10 sm:size-12 lg:mb-4 lg:size-12"
					stroke={1.8}
					aria-hidden="true"
				/>
				<h2 class="text-base font-bold sm:text-xl 2xl:text-2xl">
					{feature.title}
				</h2>
				<p
					class="mt-2 max-w-72 text-xs leading-relaxed text-panel/75 sm:text-sm lg:text-base xl:text-lg"
				>
					{feature.body}
				</p>
			</div>
		{/each}
	</div>
</section>

<style>
	.feature-strip ::selection {
		background: #000;
		color: var(--color-brand);
	}
</style>
