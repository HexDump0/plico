<script lang="ts">
	import gsap from 'gsap';
	import { flushSync } from 'svelte';
	import { flip } from 'svelte/animate';
	import { cubicOut } from 'svelte/easing';
	import { fade } from 'svelte/transition';
	import { IconRotate, IconRotateClockwise, IconTrash, IconX } from '@tabler/icons-svelte-runes';
	import type { PDFDocumentProxy } from 'pdfjs-dist';
	import type { OrganizePage } from '$lib/pdf/types';
	import { pageKey, sourceColor } from '$lib/pdf/sources';
	import OrganizeThumbnail from './OrganizeThumbnail.svelte';

	let {
		sources,
		pages,
		processing,
		reducedMotion,
		onpageschange,
		onremovesource
	}: {
		sources: { key: string; file: File; pdf: PDFDocumentProxy | null }[];
		pages: OrganizePage[];
		processing: boolean;
		reducedMotion: boolean;
		onpageschange: (pages: OrganizePage[]) => void;
		onremovesource: (key: string) => void;
	} = $props();
	let dragOrder = $state<OrganizePage[] | null>(null);
	const visiblePages = $derived(dragOrder ?? pages);
	let dragged = $state<string | null>(null);
	let keyboardPicked = $state<string | null>(null);
	let dropTarget = $state<string | null>(null);
	let trashHovered = $state(false);
	let trashZone = $state<HTMLDivElement>();
	let orderAnnouncement = $state('');
	let activePointer = -1;

	function sourceOf(page: OrganizePage) {
		return sources.find((source) => source.key === page.source);
	}

	function colorOf(page: OrganizePage) {
		return sourceColor(
			Math.max(
				0,
				sources.findIndex((source) => source.key === page.source)
			)
		);
	}

	function deletePage(page: OrganizePage) {
		if (pages.length === 1) onremovesource(page.source);
		else onpageschange(pages.filter((entry) => pageKey(entry) !== pageKey(page)));
	}
	function remove(page: OrganizePage) {
		if (processing || dragged !== null) return;
		deletePage(page);
		orderAnnouncement = `Page ${page.number} removed.`;
	}
	function rotate(page: OrganizePage, by: number) {
		if (processing || dragged !== null) return;
		onpageschange(
			pages.map((entry) =>
				pageKey(entry) === pageKey(page)
					? {
							...entry,
							rotation: ((entry.rotation + by + 360) % 360) as OrganizePage['rotation']
						}
					: entry
			)
		);
	}
	function toggleKeyboardOrder(key: string) {
		if (processing || dragged !== null) return;
		if (keyboardPicked === key) {
			const position = visiblePages.findIndex((page) => pageKey(page) === key) + 1;
			if (dragOrder) onpageschange([...dragOrder]);
			dragOrder = null;
			keyboardPicked = null;
			orderAnnouncement = `${describe(key)} placed at position ${position} of ${pages.length}.`;
		} else if (keyboardPicked === null) {
			dragOrder = [...pages];
			keyboardPicked = key;
			orderAnnouncement = `${describe(key)} picked up. Use arrow keys to move, Enter to place, or Escape to cancel.`;
		}
	}
	function handleOrderKey(event: KeyboardEvent, key: string) {
		if (processing || dragged !== null) return;
		if (event.key === 'Escape' && keyboardPicked === key) {
			event.preventDefault();
			dragOrder = null;
			keyboardPicked = null;
			orderAnnouncement = `${describe(key)} returned to its original position.`;
			return;
		}
		if (
			keyboardPicked !== key ||
			!['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key)
		)
			return;
		event.preventDefault();
		const from = visiblePages.findIndex((page) => pageKey(page) === key);
		const direction = event.key === 'ArrowLeft' || event.key === 'ArrowUp' ? -1 : 1;
		const to = Math.max(0, Math.min(visiblePages.length - 1, from + direction));
		if (from === to) return;
		const next = [...visiblePages];
		const [page] = next.splice(from, 1);
		next.splice(to, 0, page);
		dragOrder = next;
		orderAnnouncement = `${describe(key)}, position ${to + 1} of ${next.length}.`;
	}
	function describe(key: string) {
		const page = pages.find((entry) => pageKey(entry) === key);
		return page ? `Page ${page.number}` : 'Page';
	}
	function cardFlip(
		node: Element,
		positions: { from: DOMRect; to: DOMRect },
		{ key }: { key: string }
	) {
		return dragged === key || reducedMotion
			? { duration: 0 }
			: flip(node, positions, { duration: 300, easing: cubicOut });
	}
	function dragCard(node: HTMLElement, key: string) {
		const surface = node.querySelector<HTMLElement>('[data-drag-surface]')!;
		let pointerId = -1;
		let downX = 0;
		let downY = 0;
		let baseX = 0;
		let baseY = 0;
		let targetX = 0;
		let targetY = 0;
		let x = 0;
		let y = 0;
		let vx = 0;
		let vy = 0;
		let angle = 0;
		let angularVelocity = 0;
		let lastFrame = 0;
		let frameId = 0;
		let active = false;
		let movingSlot = false;
		let startIndex = -1;
		const clamp = (value: number, limit: number) => Math.max(-limit, Math.min(limit, value));
		const indexOf = (value: string) => visiblePages.findIndex((page) => pageKey(page) === value);
		function layoutPosition(card: HTMLElement) {
			const parent = card.offsetParent;
			const bounds = parent?.getBoundingClientRect();
			return {
				left: (bounds?.left ?? 0) + card.offsetLeft,
				top: (bounds?.top ?? 0) + card.offsetTop
			};
		}
		function animate(time: number) {
			if (reducedMotion) {
				gsap.set(surface, { x: targetX, y: targetY, rotation: 0, scale: 1 });
				frameId = requestAnimationFrame(animate);
				return;
			}
			const dt = Math.min((time - lastFrame) / 1000, 0.032);
			lastFrame = time;
			vx += ((targetX - x) * 380 - vx * 28) * dt;
			vy += ((targetY - y) * 380 - vy * 28) * dt;
			x += vx * dt;
			y += vy * dt;
			const targetAngle = clamp((targetX - x) * 0.11 + vx * 0.012, 9);
			angularVelocity += ((targetAngle - angle) * 220 - angularVelocity * 18) * dt;
			angle += angularVelocity * dt;
			gsap.set(surface, { x, y, rotation: angle, scale: 1.025 });
			frameId = requestAnimationFrame(animate);
		}
		function nearestCard(clientX: number, clientY: number) {
			const list = node.parentElement;
			if (!list) return null;
			const bounds = list.getBoundingClientRect();
			if (
				clientX < bounds.left - 48 ||
				clientX > bounds.right + 48 ||
				clientY < bounds.top - 48 ||
				clientY > bounds.bottom + 48
			)
				return null;
			let nearest: string | null = null;
			let distance = Infinity;
			for (const card of list.querySelectorAll<HTMLElement>('[data-page-card]')) {
				const position = layoutPosition(card);
				const dx = (clientX - position.left - card.offsetWidth / 2) / card.offsetWidth;
				const dy = (clientY - position.top - card.offsetHeight / 2) / card.offsetHeight;
				const score = dx * dx + dy * dy;
				if (score < distance) {
					distance = score;
					nearest = card.dataset.pageCard ?? null;
				}
			}
			return nearest;
		}
		function moveSlot(to: number) {
			const from = indexOf(key);
			if (from < 0 || to < 0 || from === to) return;
			const before = layoutPosition(node);
			movingSlot = true;
			const next = [...visiblePages];
			const [page] = next.splice(from, 1);
			next.splice(to, 0, page);
			flushSync(() => (dragOrder = next));
			const after = layoutPosition(node);
			const dx = after.left - before.left;
			const dy = after.top - before.top;
			baseX -= dx;
			baseY -= dy;
			targetX -= dx;
			targetY -= dy;
			x -= dx;
			y -= dy;
			gsap.set(surface, { x, y, rotation: angle, scale: reducedMotion ? 1 : 1.025 });
			if (pointerId >= 0 && !node.hasPointerCapture(pointerId)) node.setPointerCapture(pointerId);
			movingSlot = false;
		}
		function isOverTrash(clientX: number, clientY: number) {
			if (!trashZone) return false;
			const bounds = trashZone.getBoundingClientRect();
			return (
				bounds.width > 0 &&
				bounds.height > 0 &&
				clientX >= bounds.left &&
				clientX <= bounds.right &&
				clientY >= bounds.top &&
				clientY <= bounds.bottom
			);
		}
		function finish(commit: boolean, clientX?: number, clientY?: number) {
			if (pointerId < 0) return;
			const id = pointerId;
			pointerId = -1;
			if (activePointer === id) activePointer = -1;
			if (node.hasPointerCapture(id)) node.releasePointerCapture(id);
			if (!active) return;
			active = false;
			cancelAnimationFrame(frameId);
			const removePage =
				commit &&
				!processing &&
				clientX !== undefined &&
				clientY !== undefined &&
				isOverTrash(clientX, clientY);
			const from = indexOf(key);
			if (!removePage && (!commit || dropTarget === null || processing) && from >= 0)
				moveSlot(startIndex);
			const finalIndex = indexOf(key);
			const shouldCommit =
				!removePage && commit && dropTarget !== null && !processing && finalIndex !== startIndex;
			const page = pages.find((entry) => pageKey(entry) === key);
			flushSync(() => {
				if (removePage && page) deletePage(page);
				else if (shouldCommit && dragOrder) onpageschange([...dragOrder]);
				dragOrder = null;
				dragged = null;
				dropTarget = null;
				trashHovered = false;
			});
			if (removePage) {
				if (page) orderAnnouncement = `Page ${page.number} removed.`;
				return;
			}
			if (shouldCommit)
				orderAnnouncement = `${describe(key)} moved to position ${finalIndex + 1} of ${pages.length}.`;
			if (reducedMotion) gsap.set(surface, { clearProps: 'transform' });
			else
				gsap.to(surface, {
					x: 0,
					y: 0,
					rotation: 0,
					scale: 1,
					duration: 0.48,
					ease: 'elastic.out(1, 0.58)',
					overwrite: true,
					onComplete: () => gsap.set(surface, { clearProps: 'transform' })
				});
		}
		function pointerDown(event: PointerEvent) {
			if (
				processing ||
				keyboardPicked !== null ||
				pointerId >= 0 ||
				activePointer >= 0 ||
				(event.pointerType === 'mouse' && event.button !== 0) ||
				(event.target instanceof Element && event.target.closest('button'))
			)
				return;
			pointerId = event.pointerId;
			activePointer = pointerId;
			downX = event.clientX;
			downY = event.clientY;
			node.setPointerCapture(pointerId);
		}
		function pointerMove(event: PointerEvent) {
			if (event.pointerId !== pointerId) return;
			const dx = event.clientX - downX;
			const dy = event.clientY - downY;
			if (!active) {
				if (Math.hypot(dx, dy) < 6) return;
				active = true;
				startIndex = indexOf(key);
				flushSync(() => {
					dragOrder = [...pages];
					dragged = key;
				});
				gsap.killTweensOf(surface);
				baseX = x = parseFloat(String(gsap.getProperty(surface, 'x'))) || 0;
				baseY = y = parseFloat(String(gsap.getProperty(surface, 'y'))) || 0;
				angle = parseFloat(String(gsap.getProperty(surface, 'rotation'))) || 0;
				vx = vy = angularVelocity = 0;
				lastFrame = performance.now();
				frameId = requestAnimationFrame(animate);
			}
			targetX = baseX + dx;
			targetY = baseY + dy;
			trashHovered = isOverTrash(event.clientX, event.clientY);
			if (trashHovered) {
				dropTarget = null;
				return;
			}
			const position = layoutPosition(node);
			const nearest = nearestCard(
				position.left + targetX + node.offsetWidth / 2,
				position.top + targetY + node.offsetHeight / 2
			);
			dropTarget = nearest;
			if (nearest !== null && nearest !== key) {
				moveSlot(indexOf(nearest));
				dropTarget = key;
			}
		}
		function pointerUp(event: PointerEvent) {
			if (event.pointerId === pointerId) finish(true, event.clientX, event.clientY);
		}
		function pointerCancel(event: PointerEvent) {
			if (event.pointerId === pointerId) finish(false);
		}
		function preventNativeDrag(event: DragEvent) {
			event.preventDefault();
		}
		function lostPointerCapture() {
			if (!movingSlot && pointerId >= 0 && !node.hasPointerCapture(pointerId)) finish(false);
		}
		node.addEventListener('pointerdown', pointerDown);
		node.addEventListener('pointermove', pointerMove);
		node.addEventListener('pointerup', pointerUp);
		node.addEventListener('pointercancel', pointerCancel);
		node.addEventListener('dragstart', preventNativeDrag);
		node.addEventListener('lostpointercapture', lostPointerCapture);
		return {
			destroy() {
				if (activePointer === pointerId) activePointer = -1;
				cancelAnimationFrame(frameId);
				gsap.killTweensOf(surface);
				node.removeEventListener('pointerdown', pointerDown);
				node.removeEventListener('pointermove', pointerMove);
				node.removeEventListener('pointerup', pointerUp);
				node.removeEventListener('pointercancel', pointerCancel);
				node.removeEventListener('dragstart', preventNativeDrag);
				node.removeEventListener('lostpointercapture', lostPointerCapture);
				if (dragged === key) {
					dragOrder = null;
					dragged = null;
					trashHovered = false;
				}
				if (dropTarget === key) dropTarget = null;
			}
		};
	}
</script>

<div class="relative flex min-w-0 flex-1 flex-col">
	{#if dragged !== null}
		<div
			bind:this={trashZone}
			transition:fade={{ duration: reducedMotion ? 0 : 160 }}
			aria-hidden="true"
			class="pointer-events-none fixed inset-y-0 left-0 top-36 bottom-2 z-30 flex w-24 flex-col items-center justify-center gap-3 rounded-r-2xl border-2 border-l-0 border-dashed text-center text-xs font-semibold transition-colors sm:w-28 {trashHovered
				? 'border-red-400 bg-red-500/25 text-red-100'
				: 'border-red-500/50 bg-red-500/[0.08] text-red-300'}"
		>
			<IconTrash size={30} stroke={1.8} />
		</div>
	{/if}
	<ol
		aria-label="Page order"
		class="my-auto flex flex-wrap items-center justify-center gap-5 py-16 sm:gap-7 lg:py-20"
	>
		{#each visiblePages as page, index (pageKey(page))}
			{@const key = pageKey(page)}
			{@const source = sourceOf(page)}
			{@const color = colorOf(page)}
			<li
				use:dragCard={key}
				data-page-card={key}
				animate:cardFlip={{ key }}
				class="group relative w-[calc((100%-1.25rem)/2)] min-w-0 rounded-xl sm:w-64 xl:w-72 {processing
					? ''
					: 'cursor-grab touch-none select-none active:cursor-grabbing'} {dragged === key
					? 'z-20'
					: ''}"
			>
				{#if dragged === key}<div
						class="pointer-events-none absolute inset-x-0 top-0 aspect-3/4 rounded-xl border-2 border-dashed {color.border} {color.soft}"
					></div>{/if}
				<div data-drag-surface class="relative origin-[50%_12%]">
					<div
						class="relative overflow-hidden rounded-xl border-2 bg-panel shadow-lg shadow-black/20 transition-[border-color,box-shadow] duration-200 {dragged ===
						key
							? `${color.strong} shadow-2xl shadow-black/60`
							: dropTarget === key && dragged !== null
								? `${color.strong} shadow-lg`
								: `${color.border} group-hover:border-white/25`}"
					>
						{#if source?.pdf}<OrganizeThumbnail
								pdf={source.pdf}
								number={page.number}
								rotation={page.rotation}
							/>{/if}
						<button
							type="button"
							disabled={processing ||
								dragged !== null ||
								(keyboardPicked !== null && keyboardPicked !== key)}
							onclick={() => toggleKeyboardOrder(key)}
							onkeydown={(event) => handleOrderKey(event, key)}
							aria-label={`Order ${index + 1}: page ${page.number}. ${keyboardPicked === key ? 'Use arrow keys to move, Enter to place, or Escape to cancel.' : 'Press Enter to reorder.'}`}
							aria-pressed={keyboardPicked === key}
							class="{color.dot} absolute top-2 left-2 flex min-w-7 items-center justify-center rounded-md px-1.5 py-1 text-[11px] font-bold text-canvas backdrop-blur-sm focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-white"
							>{index + 1}</button
						>
						<button
							type="button"
							disabled={processing || dragged !== null || keyboardPicked !== null}
							onclick={() => remove(page)}
							aria-label={pages.length === 1 ? 'Remove PDF' : `Remove page ${page.number}`}
							title={pages.length === 1 ? 'Remove PDF' : 'Remove page'}
							class="absolute top-2 right-2 flex size-8 items-center justify-center rounded-lg bg-canvas/80 text-white backdrop-blur-sm transition-colors hover:bg-convert hover:text-canvas disabled:opacity-40"
							><IconX size={18} stroke={2.5} /></button
						>
						<div class="absolute right-2 bottom-2 flex gap-1">
							<button
								type="button"
								disabled={processing || dragged !== null || keyboardPicked !== null}
								onclick={() => rotate(page, -90)}
								aria-label={`Rotate page ${page.number} left`}
								title="Rotate left"
								class="flex size-8 items-center justify-center rounded-lg bg-canvas/80 text-white backdrop-blur-sm transition-colors hover:bg-merge hover:text-canvas disabled:opacity-40"
								><IconRotate size={17} /></button
							>
							<button
								type="button"
								disabled={processing || dragged !== null || keyboardPicked !== null}
								onclick={() => rotate(page, 90)}
								aria-label={`Rotate page ${page.number} right`}
								title="Rotate right"
								class="flex size-8 items-center justify-center rounded-lg bg-canvas/80 text-white backdrop-blur-sm transition-colors hover:bg-merge hover:text-canvas disabled:opacity-40"
								><IconRotateClockwise size={17} /></button
							>
						</div>
					</div>
				</div>
			</li>
		{/each}
	</ol>
	<p class="sr-only" aria-live="polite">{orderAnnouncement}</p>
</div>
