<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import SiteHeader from '$lib/components/SiteHeader.svelte';
	import ToolWorkspace from '$lib/components/ToolWorkspace.svelte';
	import { findTool } from '$lib/tool-catalog';
	let { data } = $props();
	const tool = $derived(findTool(data.toolId)!);
</script>

<svelte:head
	><title>Plico · {tool.label}</title><meta
		name="description"
		content={`${tool.label} with Plico. Keep your documents on your device.`}
	/></svelte:head
>
<div class="tool-shell flex min-h-svh flex-col">
	<SiteHeader onselect={(item) => goto(resolve('/tools/[tool]', { tool: item.id }))} />
	{#key tool.id}<ToolWorkspace {tool} />{/key}
</div>
