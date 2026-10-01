<script lang="ts" generics="T extends string">
	import { tick } from 'svelte';
	import { cubicOut } from 'svelte/easing';
	import { fade, scale } from 'svelte/transition';
	import { IconCheck, IconChevronDown } from '@tabler/icons-svelte-runes';

	let {
		value = $bindable<T>(),
		options,
		label,
		labelFont,
		reducedMotion,
		disabled = false
	}: {
		value: T;
		/// `hint` is a short muted description shown beside the label in the list.
		options: { value: T; label: string; hint?: string }[];
		label: string;
		/// Font for the option labels, when they preview how something will look.
		labelFont?: string;
		reducedMotion: boolean;
		disabled?: boolean;
	} = $props();
	const id = $props.id();
	let open = $state(false);
	let active = $state(0);
	let root = $state<HTMLDivElement>();
	let button = $state<HTMLButtonElement>();
	let list = $state<HTMLUListElement>();
	const selected = $derived(options.find((option) => option.value === value));

	async function show() {
		if (disabled) return;
		active = Math.max(
			0,
			options.findIndex((option) => option.value === value)
		);
		open = true;
		await tick();
		list?.focus();
	}

	function close(refocus = true) {
		open = false;
		if (refocus) button?.focus();
	}

	function choose(index: number) {
		value = options[index].value;
		close();
	}

	function onListKey(event: KeyboardEvent) {
		if (event.key === 'ArrowDown') active = Math.min(options.length - 1, active + 1);
		else if (event.key === 'ArrowUp') active = Math.max(0, active - 1);
		else if (event.key === 'Home') active = 0;
		else if (event.key === 'End') active = options.length - 1;
		else if (event.key === 'Enter' || event.key === ' ') choose(active);
		else if (event.key === 'Escape') close();
		else if (event.key === 'Tab') return close(false);
		else return;
		event.preventDefault();
	}
</script>

<svelte:window
	onpointerdown={(event) => {
		if (open && root && !root.contains(event.target as Node)) close(false);
	}}
/>

<div bind:this={root} class="relative">
	<button
		bind:this={button}
		type="button"
		{disabled}
		aria-haspopup="listbox"
		aria-expanded={open}
		aria-controls="{id}-list"
		aria-label="{label}: {selected?.label ?? ''}"
		onclick={() => (open ? close() : void show())}
		onkeydown={(event) => {
			if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
				event.preventDefault();
				void show();
			}
		}}
		class="flex w-full items-center justify-between gap-3 rounded-xl border bg-canvas px-3.5 py-3 text-left text-sm transition-colors duration-200 disabled:opacity-50 {open
			? 'border-white/25'
			: 'border-white/10 enabled:hover:border-white/20'}"
	>
		<span class="truncate" style:font-family={labelFont}>{selected?.label}</span>
		<IconChevronDown
			size={16}
			stroke={1.75}
			aria-hidden="true"
			class="shrink-0 text-muted motion-safe:transition-transform motion-safe:duration-200 {open
				? 'rotate-180'
				: ''}"
		/>
	</button>
	{#if open}
		<ul
			bind:this={list}
			id="{id}-list"
			role="listbox"
			tabindex="-1"
			aria-label={label}
			aria-activedescendant="{id}-option-{active}"
			onkeydown={onListKey}
			in:scale={{ duration: reducedMotion ? 0 : 160, start: 0.96, opacity: 0, easing: cubicOut }}
			out:fade={{ duration: reducedMotion ? 0 : 100 }}
			class="absolute inset-x-0 top-full z-30 mt-2 origin-top rounded-xl border border-white/10 bg-panel-hover p-1.5 shadow-2xl shadow-black/50 outline-none"
		>
			{#each options as option, index (option.value)}
				<li
					id="{id}-option-{index}"
					role="option"
					aria-selected={option.value === value}
					onpointerenter={() => (active = index)}
					onclick={() => choose(index)}
					onkeydown={() => {}}
					class="flex cursor-pointer items-center gap-3 rounded-lg px-3 py-2.5 text-sm motion-safe:transition-colors {index ===
					active
						? 'bg-white/[0.06]'
						: ''}"
				>
					<span class="min-w-0 flex-1 truncate text-white" style:font-family={labelFont}
						>{option.label}</span
					>
					{#if option.hint}<span class="text-xs text-muted">{option.hint}</span>{/if}
					{#if option.value === value}<IconCheck
							size={16}
							stroke={2.25}
							aria-hidden="true"
							class="shrink-0 text-brand"
						/>{:else}<span class="size-4 shrink-0"></span>{/if}
				</li>
			{/each}
		</ul>
	{/if}
</div>
