<script lang="ts">
	import gsap from 'gsap';
	import { flushSync } from 'svelte';
	import { flip } from 'svelte/animate';
	import { cubicOut } from 'svelte/easing';
	import { fade } from 'svelte/transition';
	import {
		IconPlus,
		IconX,
		IconSortAscendingLetters,
		IconSortDescendingLetters
	} from '@tabler/icons-svelte-runes';
	import { IconFileTypeDocx, IconFileTypePpt, IconFileTypeXls } from '@tabler/icons-svelte-runes';
	import { getWorkspace, formatSize } from '$lib/workspace.svelte';
	import PdfPreview from './PdfPreview.svelte';
	import DragTrashZone from './DragTrashZone.svelte';

	let {
		mode,
		accent = 'text-brand',
		officeFormat,
		processing,
		reducedMotion,
		dragged = $bindable<File | null>(null),
		keyboardPicked = $bindable<File | null>(null),
		onadd,
		onload
	}: {
		mode: 'merge' | 'compress' | 'image' | 'single';
		accent?: string;
		officeFormat?: 'docx' | 'pptx' | 'xlsx';
		processing: boolean;
		reducedMotion: boolean;
		dragged: File | null;
		keyboardPicked: File | null;
		onadd: () => void;
		onload: (count: number) => void;
	} = $props();
	const workspace = getWorkspace();
	const isMerge = $derived(mode === 'merge');
	const isCompress = $derived(mode === 'compress');
	const isPdfToImage = $derived(mode === 'single');
	const isImageToPdf = $derived(mode === 'image');
	let dragOrder = $state<File[] | null>(null);
	const canOrder = $derived(isMerge || isCompress || isImageToPdf);
	const visibleFiles = $derived(dragOrder ?? workspace.files);
	const cards = $derived(
		isPdfToImage ? (workspace.files[0] ? [workspace.files[0]] : []) : [...visibleFiles, null]
	);
	let dropTarget = $state<File | null>(null);
	let trashHovered = $state(false);
	let trashZone = $state<HTMLDivElement>();
	let orderAnnouncement = $state('');
	let sortAscending = $state(true);
	let activePointer = -1;
	function sortByFilename() {
		const direction = sortAscending ? 1 : -1;
		workspace.files = [...workspace.files].sort(
			(a, b) => a.name.localeCompare(b.name, undefined, { numeric: true }) * direction
		);
		sortAscending = !sortAscending;
	}
	function toggleKeyboardOrder(file: File) {
		if (processing || dragged) return;
		if (keyboardPicked === file) {
			const position = visibleFiles.indexOf(file) + 1;
			if (dragOrder) workspace.files = [...dragOrder];
			dragOrder = null;
			keyboardPicked = null;
			orderAnnouncement = `${file.name} placed at position ${position} of ${workspace.files.length}.`;
		} else if (!keyboardPicked) {
			dragOrder = [...workspace.files];
			keyboardPicked = file;
			orderAnnouncement = `${file.name} picked up. Use arrow keys to move, Enter to place, or Escape to cancel.`;
		}
	}
	function handleOrderKey(event: KeyboardEvent, file: File) {
		if (processing || dragged) return;
		if (event.key === 'Escape' && keyboardPicked === file) {
			event.preventDefault();
			dragOrder = null;
			keyboardPicked = null;
			orderAnnouncement = `${file.name} returned to its original position.`;
			return;
		}
		if (
			keyboardPicked !== file ||
			!['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key)
		)
			return;
		event.preventDefault();
		const from = visibleFiles.indexOf(file);
		const direction = event.key === 'ArrowLeft' || event.key === 'ArrowUp' ? -1 : 1;
		const to = Math.max(0, Math.min(visibleFiles.length - 1, from + direction));
		if (from === to) return;
		const next = [...visibleFiles];
		next.splice(from, 1);
		next.splice(to, 0, file);
		dragOrder = next;
		orderAnnouncement = `${file.name}, position ${to + 1} of ${next.length}.`;
	}
	function cardFlip(
		node: Element,
		positions: { from: DOMRect; to: DOMRect },
		{ file }: { file: File | null }
	) {
		return (file !== null && dragged === file) || reducedMotion
			? { duration: 0 }
			: flip(node, positions, { duration: 300, easing: cubicOut });
	}
	function dragCard(node: HTMLElement, item: File | null) {
		if (!item) return;
		const file = item;
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
			let nearest = -1;
			let distance = Infinity;
			for (const [index, card] of Array.from(
				list.querySelectorAll<HTMLElement>('[data-pdf-card]')
			).entries()) {
				const position = layoutPosition(card);
				const dx = (clientX - position.left - card.offsetWidth / 2) / card.offsetWidth;
				const dy = (clientY - position.top - card.offsetHeight / 2) / card.offsetHeight;
				const score = dx * dx + dy * dy;
				if (score < distance) {
					distance = score;
					nearest = index;
				}
			}
			return visibleFiles[nearest] ?? null;
		}
		function moveSlot(to: number) {
			const from = visibleFiles.indexOf(file);
			if (from < 0 || to < 0 || from === to) return;
			const before = layoutPosition(node);
			movingSlot = true;
			const next = [...visibleFiles];
			const [moved] = next.splice(from, 1);
			next.splice(to, 0, moved);
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
			if (bounds.width === 0 || bounds.height === 0) return false;
			return (
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
			const removeFile =
				commit &&
				!processing &&
				clientX !== undefined &&
				clientY !== undefined &&
				isOverTrash(clientX, clientY);
			const from = visibleFiles.indexOf(file);
			if (!removeFile && (!commit || !dropTarget || processing) && from >= 0) moveSlot(startIndex);
			const finalIndex = visibleFiles.indexOf(file);
			const shouldCommit =
				!removeFile && commit && !!dropTarget && !processing && finalIndex !== startIndex;
			flushSync(() => {
				if (removeFile) workspace.remove(file);
				else if (shouldCommit && dragOrder) workspace.files = [...dragOrder];
				dragOrder = null;
				dragged = null;
				dropTarget = null;
				trashHovered = false;
			});
			if (removeFile) {
				orderAnnouncement = `${file.name} removed.`;
				return;
			}
			if (shouldCommit) {
				orderAnnouncement = `${file.name} moved to position ${finalIndex + 1} of ${workspace.files.length}.`;
			}
			if (reducedMotion) {
				gsap.set(surface, { clearProps: 'transform' });
			} else {
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
		}
		function pointerDown(event: PointerEvent) {
			if (
				!canOrder ||
				processing ||
				keyboardPicked ||
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
				startIndex = visibleFiles.indexOf(file);
				flushSync(() => {
					dragOrder = [...workspace.files];
					dragged = file;
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
			if (nearest && nearest !== file) {
				moveSlot(visibleFiles.indexOf(nearest));
				dropTarget = file;
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
				if (dragged === file) {
					dragOrder = null;
					dragged = null;
					trashHovered = false;
				}
				if (dropTarget === file) dropTarget = null;
			}
		};
	}
</script>

<div class="relative flex min-w-0 flex-1 flex-col">
	{#if dragged}
		<DragTrashZone bind:zone={trashZone} hovered={trashHovered} {reducedMotion} />
	{/if}
	{#if canOrder && workspace.files.length > 1}
		<button
			transition:fade={{ duration: reducedMotion ? 0 : 100 }}
			disabled={processing || !!dragged || !!keyboardPicked}
			class="absolute top-0 right-0 z-10 flex size-11 items-center justify-center rounded-xl border-2 border-white/10 bg-panel text-muted transition-colors disabled:opacity-40 {isCompress
				? 'hover:border-compress/40 hover:text-compress'
				: isImageToPdf
					? 'hover:border-convert/40 hover:text-convert'
					: 'hover:border-merge/40 hover:text-merge'}"
			aria-label={`Sort filenames ${sortAscending ? 'ascending' : 'descending'}`}
			title={`Sort filenames ${sortAscending ? 'ascending' : 'descending'}`}
			onclick={sortByFilename}
			>{#if sortAscending}<IconSortAscendingLetters size={20} />{:else}<IconSortDescendingLetters
					size={20}
				/>{/if}</button
		>
	{/if}
	<ol
		aria-label={isImageToPdf ? 'Image order' : 'PDF order'}
		class="my-auto flex flex-wrap items-center justify-center gap-5 py-16 sm:gap-7 lg:py-24"
	>
		{#each cards as file, index (file ?? 'add')}
			<li
				use:dragCard={file}
				data-pdf-card={file ? '' : undefined}
				animate:cardFlip={{ file }}
				class="group relative w-[calc((100%-1.25rem)/2)] min-w-0 rounded-xl sm:w-52 {file &&
				canOrder &&
				!processing
					? 'cursor-grab touch-none select-none active:cursor-grabbing'
					: ''} {file && dragged === file ? 'z-40' : ''}"
			>
				{#if file}
					{#if dragged === file}<div
							class="pointer-events-none absolute inset-x-0 top-0 aspect-[3/4] rounded-xl border-2 border-dashed {isCompress
								? 'border-compress/35 bg-compress/5'
								: isImageToPdf
									? 'border-convert/35 bg-convert/5'
									: 'border-merge/35 bg-merge/5'}"
						></div>{/if}
					<div data-drag-surface class="relative origin-[50%_12%]">
						<div
							class="relative overflow-hidden rounded-xl border-2 bg-panel shadow-lg shadow-black/20 transition-[border-color,box-shadow] duration-200 {dragged ===
							file
								? isCompress
									? 'border-compress/70 shadow-2xl shadow-black/60'
									: isImageToPdf
										? 'border-convert/70 shadow-2xl shadow-black/60'
										: 'border-merge/70 shadow-2xl shadow-black/60'
								: dropTarget === file && dragged
									? isCompress
										? 'border-compress/70 shadow-lg shadow-compress/10'
										: isImageToPdf
											? 'border-convert/70 shadow-lg shadow-convert/10'
											: 'border-merge/70 shadow-lg shadow-merge/10'
									: 'border-white/10 group-hover:border-white/25'}"
						>
							{#if officeFormat}<div
									class="flex aspect-[3/4] items-center justify-center bg-canvas/50 {accent}"
								>
									{#if officeFormat === 'docx'}<IconFileTypeDocx size={72} stroke={1.25} />
									{:else if officeFormat === 'pptx'}<IconFileTypePpt size={72} stroke={1.25} />
									{:else}<IconFileTypeXls size={72} stroke={1.25} />{/if}
								</div>{:else}<PdfPreview
									{file}
									onload={(count) => {
										onload(count);
									}}
								/>{/if}
							{#if canOrder}<button
									type="button"
									class="absolute top-2 left-2 flex min-w-7 items-center justify-center rounded-md bg-canvas/80 px-1.5 py-1 text-[11px] font-bold text-white backdrop-blur-sm focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-white"
									aria-label={`Order ${index + 1}: ${file.name}. ${keyboardPicked === file ? 'Use arrow keys to move, Enter to place, or Escape to cancel.' : 'Press Enter to reorder.'}`}
									aria-pressed={keyboardPicked === file}
									disabled={processing ||
										!!dragged ||
										(!!keyboardPicked && keyboardPicked !== file)}
									onclick={() => toggleKeyboardOrder(file)}
									onkeydown={(event) => handleOrderKey(event, file)}>{index + 1}</button
								>{:else if !isMerge && !isPdfToImage}<span
									class="absolute top-2 left-2 flex min-w-7 items-center justify-center rounded-md bg-canvas/80 px-1.5 py-1 text-[11px] font-bold text-white backdrop-blur-sm"
									>{index + 1}</span
								>{/if}<button
								disabled={processing || !!dragged || !!keyboardPicked}
								class="absolute top-2 right-2 flex size-8 items-center justify-center rounded-lg bg-canvas/80 text-white backdrop-blur-sm transition-colors hover:bg-convert hover:text-canvas disabled:opacity-40"
								aria-label={`Remove ${file.name}`}
								onclick={() => workspace.remove(file)}><IconX size={18} stroke={2.5} /></button
							>
							<span
								class="absolute right-2 bottom-2 rounded-md bg-canvas/80 px-1.5 py-1 text-[10px] font-semibold text-white backdrop-blur-sm"
								>{formatSize(file.size)}</span
							>
						</div>
						<p class="mt-2 truncate px-1 text-xs font-medium" title={file.name}>
							{file.name}
						</p>
					</div>
				{:else}
					<button
						disabled={processing || !!dragged || !!keyboardPicked}
						onclick={onadd}
						class="group flex aspect-[3/4] w-full flex-col items-center justify-center gap-4 rounded-xl border-2 border-dashed transition-[border-color,background-color,color] disabled:opacity-40 {isCompress
							? 'border-compress/25 bg-compress/[0.03] text-compress/75 hover:border-compress/60 hover:bg-compress/[0.07] hover:text-compress'
							: isImageToPdf
								? 'border-convert/25 bg-convert/[0.03] text-convert/75 hover:border-convert/60 hover:bg-convert/[0.07] hover:text-convert'
								: 'border-merge/25 bg-merge/[0.03] text-merge/75 hover:border-merge/60 hover:bg-merge/[0.07] hover:text-merge'}"
					>
						<span
							class="flex size-12 items-center justify-center rounded-full motion-safe:transition-transform motion-safe:group-hover:scale-110 {isCompress
								? 'bg-compress/10'
								: isImageToPdf
									? 'bg-convert/10'
									: 'bg-merge/10'}"><IconPlus size={24} stroke={1.5} /></span
						>
					</button>
				{/if}
			</li>
		{/each}
	</ol>
	{#if canOrder}<p class="sr-only" aria-live="polite">{orderAnnouncement}</p>{/if}
</div>
