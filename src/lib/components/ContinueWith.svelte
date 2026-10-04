<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { IconLayoutGrid } from '@tabler/icons-svelte-runes';
	import { nextTools, toolCategoryColor, type CatalogTool } from '$lib/tool-catalog';
	import { getWorkspace } from '$lib/workspace.svelte';
	import ToolsDialog from './ToolsDialog.svelte';

	let { tool, result }: { tool: CatalogTool; result: () => File | undefined } = $props();
	const workspace = getWorkspace();
	const suggestions = $derived(nextTools(tool.id));
	let toolsDialog: ToolsDialog;
	let moreButton = $state<HTMLButtonElement>();

	const shortLabel = (label: string) => label.replace(/ PDF$/, '').replace(/^Add /, '');

	function open(next: CatalogTool) {
		const file = result();
		if (!file) return;
		workspace.carry(file);
		// The same tool does not remount, so it takes the result in place.
		if (next.id === tool.id) workspace.receive();
		else goto(resolve('/tools/[tool]', { tool: next.id }));
	}
</script>

<div>
	<p class="text-sm font-semibold">Continue with</p>
	<div class="mt-3 grid grid-cols-2 gap-2">
		{#each suggestions as next (next.id)}
			<button
				type="button"
				onclick={() => open(next)}
				class="group flex min-h-11 min-w-0 items-center gap-2.5 rounded-xl border-2 border-white/10 px-3 py-2 text-left text-sm font-medium transition-[border-color,transform] duration-150 hover:border-white/20 motion-safe:active:scale-[0.98] {toolCategoryColor(
					next.id
				)}"
			>
				<next.icon size={18} stroke={1.8} class="shrink-0" aria-hidden="true" /><span
					class="truncate text-tool-label transition-colors duration-150 group-hover:text-inherit"
					>{shortLabel(next.label)}</span
				>
			</button>
		{/each}
		<button
			bind:this={moreButton}
			type="button"
			onclick={() => toolsDialog.open(moreButton)}
			class="group flex min-h-11 min-w-0 items-center gap-2.5 rounded-xl border-2 border-white/10 px-3 py-2 text-left text-sm font-medium text-muted transition-[border-color,transform] duration-150 hover:border-white/20 motion-safe:active:scale-[0.98]"
		>
			<IconLayoutGrid size={18} stroke={1.8} class="shrink-0" aria-hidden="true" /><span
				class="truncate transition-colors duration-150 group-hover:text-white">More tools</span
			>
		</button>
	</div>
</div>
<ToolsDialog bind:this={toolsDialog} onselect={open} selectionTarget={() => moreButton} />
