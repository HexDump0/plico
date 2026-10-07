<script lang="ts">
	import { IconArrowUp, IconPlayerStopFilled } from '@tabler/icons-svelte-runes';

	let {
		answering,
		disabled = false,
		onask,
		onstop
	}: {
		answering: boolean;
		disabled?: boolean;
		onask: (question: string) => void;
		onstop: () => void;
	} = $props();

	let question = $state('');
	let field = $state<HTMLTextAreaElement>();

	// Grows with what is typed, up to a few lines.
	function fit() {
		if (!field) return;
		field.style.height = 'auto';
		field.style.height = `${Math.min(field.scrollHeight, 128)}px`;
	}

	function send() {
		if (disabled || answering || !question.trim()) return;
		onask(question);
		question = '';
		requestAnimationFrame(fit);
	}
</script>

<form
	class="flex min-h-14 items-end gap-2 rounded-xl bg-canvas p-2 ring-white/20 focus-within:ring-1 motion-safe:transition-shadow"
	onsubmit={(event) => {
		event.preventDefault();
		send();
	}}
>
	<textarea
		bind:this={field}
		bind:value={question}
		{disabled}
		rows="1"
		placeholder="Ask about this PDF"
		aria-label="Ask about this PDF"
		oninput={fit}
		onkeydown={(event) => {
			if (event.key === 'Enter' && !event.shiftKey && !event.isComposing) {
				event.preventDefault();
				send();
			}
		}}
		class="max-h-32 min-w-0 flex-1 resize-none self-center bg-transparent px-2 py-1.5 text-sm text-white outline-none placeholder:text-white/30 focus-visible:outline-none disabled:opacity-40"
	></textarea>
	{#if answering}
		<button
			type="button"
			onclick={onstop}
			aria-label="Stop answering"
			title="Stop"
			class="grid size-10 shrink-0 place-items-center rounded-lg bg-panel-hover text-white hover:bg-white/15 motion-safe:transition-colors"
			><IconPlayerStopFilled size={16} /></button
		>
	{:else}
		<button
			type="submit"
			disabled={disabled || !question.trim()}
			aria-label="Ask"
			title="Ask"
			class="grid size-10 shrink-0 place-items-center rounded-lg bg-compress text-canvas enabled:hover:brightness-110 disabled:opacity-40 motion-safe:transition-[filter,opacity] motion-safe:enabled:active:scale-95"
			><IconArrowUp size={20} stroke={2.2} /></button
		>
	{/if}
</form>
