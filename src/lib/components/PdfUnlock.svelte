<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { cubicOut } from 'svelte/easing';
	import { slide } from 'svelte/transition';
	import {
		IconArrowRight,
		IconEye,
		IconEyeOff,
		IconLoader2,
		IconLock,
		IconLockExclamation
	} from '@tabler/icons-svelte-runes';
	import type { LockState } from '$lib/workspace.svelte';

	let {
		lock,
		name = '',
		compact = false,
		focus = false,
		onunlock
	}: {
		lock: LockState;
		name?: string;
		compact?: boolean;
		focus?: boolean;
		onunlock: (password: string) => void;
	} = $props();

	const uid = $props.id();
	let password = $state('');
	let visible = $state(false);
	let edited = $state(false);
	let reducedMotion = $state(false);
	let input = $state<HTMLInputElement>();
	const checking = $derived(lock === 'checking');
	const opened = $derived(lock === 'unlocked');
	// The error belongs to the attempt, not the field: it clears as soon as the
	// password is changed.
	const wrong = $derived(lock === 'incorrect' && !edited);

	onMount(() => {
		reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
		if (focus) input?.focus();
	});

	$effect(() => {
		if (lock !== 'incorrect') return;
		edited = false;
		void tick().then(() => {
			if (!input) return;
			input.focus();
			input.setSelectionRange(input.value.length, input.value.length);
		});
	});

	function submit(event: SubmitEvent) {
		event.preventDefault();
		if (password && !checking) onunlock(password);
	}
</script>

<form
	onsubmit={submit}
	class="mx-auto flex w-full flex-col items-center text-center {compact
		? 'max-w-44 gap-2.5'
		: 'max-w-sm gap-4'} {opened ? 'opacity-0 transition-opacity duration-200' : ''}"
>
	<span class="grid" aria-hidden="true">
		<IconLock
			size={compact ? 26 : 36}
			stroke={1.4}
			class="col-start-1 row-start-1 text-muted transition-opacity duration-200 {wrong
				? 'opacity-0'
				: ''}"
		/>
		<IconLockExclamation
			size={compact ? 26 : 36}
			stroke={1.4}
			class="col-start-1 row-start-1 text-convert transition-opacity duration-200 {wrong
				? ''
				: 'opacity-0'}"
		/>
	</span>
	<div class="w-full min-w-0">
		<p class="font-semibold {compact ? 'text-xs' : 'text-sm'}">Password protected</p>
		{#if name}<p class="mt-1 truncate text-xs text-muted" title={name}>{name}</p>{/if}
	</div>
	<div class="w-full">
		<div
			class="flex w-full items-center gap-1 border transition-colors duration-200 {compact
				? 'h-9 rounded-lg pr-1 pl-2.5'
				: 'h-11 rounded-xl pr-1.5 pl-3'} {wrong
				? 'border-convert/60 bg-convert/5'
				: 'border-white/10 bg-canvas focus-within:border-white/25'}"
		>
			<input
				bind:this={input}
				bind:value={password}
				oninput={() => (edited = true)}
				type={visible ? 'text' : 'password'}
				placeholder="Password"
				autocomplete="off"
				spellcheck="false"
				disabled={checking || opened}
				aria-label={name ? `Password for ${name}` : 'Password'}
				aria-invalid={wrong}
				aria-describedby={wrong ? `${uid}-error` : undefined}
				class="min-w-0 flex-1 bg-transparent text-sm text-white outline-none placeholder:text-white/35 disabled:opacity-60"
			/>
			{#if !compact}<button
					type="button"
					onclick={() => (visible = !visible)}
					aria-label={visible ? 'Hide password' : 'Show password'}
					class="flex size-8 shrink-0 items-center justify-center rounded-lg text-muted transition-colors hover:text-white"
					>{#if visible}<IconEyeOff size={17} />{:else}<IconEye size={17} />{/if}</button
				>{/if}
			<button
				type="submit"
				disabled={!password || checking || opened}
				aria-label="Unlock"
				class="flex shrink-0 items-center justify-center rounded-lg bg-white/10 text-white transition-colors enabled:hover:bg-white/20 disabled:opacity-40 {compact
					? 'size-7'
					: 'size-8'}"
				>{#if checking}<IconLoader2 class="animate-spin" size={16} />{:else}<IconArrowRight
						size={16}
					/>{/if}</button
			>
		</div>
		{#if wrong}<p
				id={`${uid}-error`}
				role="alert"
				transition:slide={{ duration: reducedMotion ? 0 : 180, easing: cubicOut }}
				class="pt-2 text-xs text-convert"
			>
				Incorrect password
			</p>{/if}
	</div>
</form>
