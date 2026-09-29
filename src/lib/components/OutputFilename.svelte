<script lang="ts">
	let {
		value = $bindable<string>(),
		placeholder,
		extension
	}: {
		value: string;
		placeholder: string;
		extension: string;
	} = $props();
</script>

<label class="block text-sm font-semibold"
	>Output filename
	<div
		class="mt-3 flex items-center gap-2 rounded-xl border border-white/10 bg-canvas px-3 focus-within:border-white/25"
	>
		<input
			bind:value
			{placeholder}
			title={value || placeholder}
			autocomplete="off"
			spellcheck="false"
			class="min-w-0 flex-1 bg-transparent py-3 text-sm font-normal text-ellipsis outline-none placeholder:text-white/35"
		/><span class="shrink-0 text-xs font-normal text-muted">.{extension}</span>
	</div></label
>
