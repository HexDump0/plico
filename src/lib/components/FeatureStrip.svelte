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
	class="feature-strip relative z-10 shrink-0 rounded-t-[2rem] bg-brand text-panel lg:mx-auto lg:w-[90%] lg:max-w-[108rem] lg:rounded-t-[2.5rem] xl:w-[88%]"
>
	<div
		class="feature-grid mx-auto grid max-w-[120rem] grid-cols-2 gap-y-10 px-4 py-10 sm:px-8 lg:min-h-[clamp(13rem,12.5vw,15rem)] lg:grid-cols-4 lg:items-center lg:gap-y-0 lg:px-6 lg:py-8"
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
				<h2 class="text-base font-bold sm:text-xl lg:text-[clamp(1.25rem,1.35vw,1.625rem)]">
					{feature.title}
				</h2>
				<p
					class="mt-2 max-w-72 text-xs leading-relaxed text-panel/75 sm:text-sm lg:text-[1.0625rem] xl:text-lg"
				>
					{feature.body}
				</p>
			</div>
		{/each}
	</div>
</section>
