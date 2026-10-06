<script lang="ts">
	import '@fontsource-variable/plus-jakarta-sans';
	import '../app.css';
	import favicon from '$lib/assets/plico.svg';
	import RouteMotion from '$lib/components/RouteMotion.svelte';
	import { setWorkspace, Workspace } from '$lib/workspace.svelte';
	import { offline } from '$lib/offline.svelte';
	import { onMount } from 'svelte';
	setWorkspace(new Workspace());
	onMount(() => offline.start());

	let { children } = $props();
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
</svelte:head>

<RouteMotion />
{@render children()}
