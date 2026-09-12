<script lang="ts">
	import '@fontsource-variable/plus-jakarta-sans';
	import '../app.css';
	import favicon from '$lib/assets/plico.svg';
	import RouteMotion from '$lib/components/RouteMotion.svelte';
	import { setWorkspace, Workspace } from '$lib/workspace.svelte';
	setWorkspace(new Workspace());

	let { children } = $props();
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
</svelte:head>

<RouteMotion />
{@render children()}
