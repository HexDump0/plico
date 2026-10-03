<script lang="ts">
	import { IconCheck } from '@tabler/icons-svelte-runes';
	import type { FormValue, FormWidget } from '$lib/pdf/form-fields';
	import type { PreviewPage } from '$lib/pdf/stamp-layout';
	import type { CropArea } from '$lib/pdf/types';

	let {
		page,
		widgets,
		drawn,
		canvases,
		values,
		invalid,
		editable,
		onchange
	}: {
		page: PreviewPage;
		widgets: FormWidget[];
		/// Where everything pdf.js draws apart from the page sits, by id.
		drawn: { id: string; area: CropArea }[];
		/// The canvases it drew them on, once the page has rendered.
		canvases?: Map<string, HTMLCanvasElement | HTMLCanvasElement[]>;
		values: Record<string, FormValue>;
		/// A field whose text cannot be drawn.
		invalid?: string;
		editable: boolean;
		onchange: (name: string, value: FormValue) => void;
	} = $props();
	const id = $props.id();

	// Pinned to the field's rectangle, which is the page's coordinates turned
	// to fractions, so it follows every zoom.
	const percent = (value: number) => `${(value * 100).toFixed(4)}%`;
	// Points as a share of the page's width, which the overlay's container
	// query width scales to whatever the page is drawn at.
	const points = (value: number) => `${((value / page.width) * 100).toFixed(4)}cqw`;
	const fonts = {
		helvetica: 'Helvetica, Arial, sans-serif',
		times: '"Times New Roman", Times, serif',
		courier: '"Courier New", Courier, monospace'
	};

	const interactive = $derived(new Set(widgets.map((widget) => `${widget.id}R`)));
	// Read-only fields, push buttons and the like, which pdf.js draws on
	// canvases of their own once forms render apart.
	const placed = $derived(
		drawn.flatMap(({ id: key, area }) => {
			const canvas = canvases?.get(key);
			return canvas && !Array.isArray(canvas) && !interactive.has(key)
				? [{ key, area, canvas }]
				: [];
		})
	);

	/// The size text is shown at, as the engine sizes a field set to fit.
	function fontSize(widget: FormWidget) {
		if (widget.size > 0) return widget.size;
		const height = (widget.area[3] - widget.area[1]) * page.height;
		const line = Math.max(4, (height - 2 - 2 * widget.borderWidth) / 1.35);
		return widget.multiline || widget.kind === 'list' ? Math.min(12, line) : line;
	}

	function frame(widget: FormWidget) {
		const [left, top, right, bottom] = widget.area;
		return [
			`left: ${percent(left)}`,
			`top: ${percent(top)}`,
			`width: ${percent(right - left)}`,
			`height: ${percent(bottom - top)}`,
			`background-color: ${widget.background ?? 'transparent'}`,
			widget.border
				? `border: ${points(widget.borderWidth)} solid ${widget.border}`
				: 'border: 0 solid transparent'
		].join('; ');
	}

	function text(widget: FormWidget) {
		const size = fontSize(widget);
		const styles = [
			`font-size: ${points(size)}`,
			`font-family: ${widget.comb ? fonts.courier : fonts[widget.font]}`,
			`font-weight: ${widget.bold ? 700 : 400}`,
			`color: ${widget.color}`,
			`text-align: ${widget.comb ? 'left' : widget.align}`,
			`line-height: 1.2`
		];
		if (widget.comb) {
			// Courier's characters are 0.6 em wide; the rest of each cell is
			// spacing, half of it before the first.
			const cell = ((widget.area[2] - widget.area[0]) * page.width) / widget.maxLength;
			const gap = Math.max(0, cell - 0.6 * size);
			styles.push(`letter-spacing: ${points(gap)}`, `padding: 0 0 0 ${points(gap / 2)}`);
		} else {
			styles.push(`padding: ${points(widget.multiline ? 2 : 0)} ${points(2)}`);
		}
		return styles.join('; ');
	}

	function mount(node: HTMLElement, canvas: HTMLCanvasElement) {
		const show = (next: HTMLCanvasElement) => {
			next.style.width = next.style.height = '100%';
			next.style.display = 'block';
			node.replaceChildren(next);
		};
		show(canvas);
		return { update: show };
	}

	function stateCanvas(widget: FormWidget, on: boolean) {
		const found = canvases?.get(`${widget.id}R`);
		if (!Array.isArray(found)) return undefined;
		return found.find(
			(canvas) => canvas.getAttribute('data-canvas-name') === (on ? 'checked' : 'unchecked')
		);
	}

	const field =
		'absolute box-border bg-brand/10 outline-none hover:bg-brand/20 focus:bg-brand/15 focus:outline-2 focus:outline-offset-0 focus:outline-brand disabled:cursor-default disabled:hover:bg-brand/10 motion-safe:transition-colors';
</script>

<div class="absolute inset-0" style:container-type="inline-size">
	{#each placed as entry (entry.key)}
		{@const [left, top, right, bottom] = entry.area}
		<div
			aria-hidden="true"
			class="pointer-events-none absolute"
			style:left={percent(left)}
			style:top={percent(top)}
			style:width={percent(right - left)}
			style:height={percent(bottom - top)}
			use:mount={entry.canvas}
		></div>
	{/each}
	{#each widgets as widget (widget.id)}
		{@const value = values[widget.name]}
		{@const wrong = invalid === widget.name}
		<div class="absolute" style={frame(widget)}>
			{#if widget.kind === 'text' && widget.multiline}
				<textarea
					aria-label={widget.label}
					title={widget.label}
					required={widget.required}
					maxlength={widget.maxLength || undefined}
					disabled={!editable}
					spellcheck="false"
					value={value as string}
					oninput={(event) => onchange(widget.name, event.currentTarget.value)}
					class="{field} inset-0 size-full resize-none [scrollbar-width:none] overflow-auto {wrong
						? 'outline-2 outline-convert'
						: ''}"
					style={text(widget)}></textarea>
			{:else if widget.kind === 'text'}
				<input
					type="text"
					aria-label={widget.label}
					title={widget.label}
					required={widget.required}
					maxlength={widget.maxLength || undefined}
					disabled={!editable}
					autocomplete="off"
					spellcheck="false"
					value={value as string}
					oninput={(event) => onchange(widget.name, event.currentTarget.value)}
					class="{field} inset-0 size-full {wrong ? 'outline-2 outline-convert' : ''}"
					style={text(widget)}
				/>
			{:else if widget.kind === 'combo' && widget.editable}
				<input
					type="text"
					list="{id}-{widget.id}"
					aria-label={widget.label}
					title={widget.label}
					required={widget.required}
					disabled={!editable}
					autocomplete="off"
					value={value as string}
					oninput={(event) => onchange(widget.name, event.currentTarget.value)}
					class="{field} inset-0 size-full {wrong ? 'outline-2 outline-convert' : ''}"
					style={text(widget)}
				/>
				<datalist id="{id}-{widget.id}">
					{#each widget.options as option (option.value)}<option value={option.value}
							>{option.label}</option
						>{/each}
				</datalist>
			{:else if widget.kind === 'combo'}
				<select
					aria-label={widget.label}
					title={widget.label}
					required={widget.required}
					disabled={!editable}
					value={value as string}
					onchange={(event) => onchange(widget.name, event.currentTarget.value)}
					class="{field} inset-0 size-full cursor-pointer appearance-none {wrong
						? 'outline-2 outline-convert'
						: ''}"
					style={text(widget)}
				>
					{#if value === ''}<option value="" hidden></option>{/if}
					{#each widget.options as option (option.value)}<option value={option.value}
							>{option.label}</option
						>{/each}
				</select>
			{:else if widget.kind === 'list'}
				<select
					multiple={widget.multiple}
					size={Math.max(2, widget.options.length)}
					aria-label={widget.label}
					title={widget.label}
					required={widget.required}
					disabled={!editable}
					onchange={(event) =>
						onchange(
							widget.name,
							Array.from(event.currentTarget.selectedOptions, (option) => option.value)
						)}
					class="{field} inset-0 size-full [scrollbar-width:none] overflow-auto {wrong
						? 'outline-2 outline-convert'
						: ''}"
					style={text(widget)}
				>
					{#each widget.options as option (option.value)}<option
							value={option.value}
							selected={(value as string[]).includes(option.value)}
							class="checked:bg-[#99bfd9] checked:text-black">{option.label}</option
						>{/each}
				</select>
			{:else}
				{@const on = value === widget.onValue}
				{@const canvas = stateCanvas(widget, on)}
				<button
					type="button"
					role={widget.kind === 'check' ? 'checkbox' : 'radio'}
					aria-checked={on}
					aria-label={widget.label}
					title={widget.label}
					disabled={!editable}
					onclick={() =>
						onchange(widget.name, widget.kind === 'check' && on ? 'Off' : widget.onValue)}
					class="{field} inset-0 grid size-full cursor-pointer place-items-center {widget.kind ===
					'radio'
						? 'rounded-full'
						: ''}"
				>
					{#if canvas}<span class="absolute inset-0" use:mount={canvas}></span>
					{:else if on && widget.kind === 'radio'}<span
							class="size-[45%] rounded-full"
							style:background-color={widget.color}
						></span>
					{:else if on}<IconCheck
							class="size-[80%]"
							stroke={3}
							style="color: {widget.color}"
						/>{/if}
				</button>
			{/if}
		</div>
	{/each}
</div>
