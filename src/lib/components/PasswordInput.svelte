<script lang="ts">
	import { IconEye, IconEyeOff } from '@tabler/icons-svelte-runes';

	let {
		value = $bindable<string>(),
		label,
		placeholder = '',
		invalid = false,
		disabled = false,
		onenter
	}: {
		value: string;
		label: string;
		placeholder?: string;
		invalid?: boolean;
		disabled?: boolean;
		onenter?: () => void;
	} = $props();
	let visible = $state(false);
</script>

<label class="block text-sm font-semibold"
	>{label}
	<div
		class="mt-3 flex items-center gap-1 rounded-xl border bg-canvas pr-1.5 pl-3 transition-colors duration-200 {invalid
			? 'border-convert/60'
			: 'border-white/10 focus-within:border-white/25'}"
	>
		<input
			bind:value
			type={visible ? 'text' : 'password'}
			{placeholder}
			{disabled}
			autocomplete="off"
			spellcheck="false"
			aria-invalid={invalid}
			onkeydown={(event) => {
				if (event.key === 'Enter' && onenter) {
					event.preventDefault();
					onenter();
				}
			}}
			class="min-w-0 flex-1 bg-transparent py-3 text-sm font-normal text-white outline-none placeholder:text-white/35 disabled:opacity-50"
		/><button
			type="button"
			onclick={() => (visible = !visible)}
			aria-label={visible ? `Hide ${label.toLowerCase()}` : `Show ${label.toLowerCase()}`}
			class="flex size-8 shrink-0 items-center justify-center rounded-lg text-muted transition-colors hover:text-white"
			>{#if visible}<IconEyeOff size={17} />{:else}<IconEye size={17} />{/if}</button
		>
	</div></label
>
