<script lang="ts">
	import gsap from 'gsap';
	import { flushSync, onDestroy, onMount, tick, untrack } from 'svelte';
	import { flip } from 'svelte/animate';
	import { cubicIn, cubicOut } from 'svelte/easing';
	import { fade } from 'svelte/transition';
	import {
		IconArrowRight,
		IconPlus,
		IconX,
		IconSortAscendingLetters,
		IconSortDescendingLetters,
		IconDownload,
		IconLoader2,
		IconTrash
	} from '@tabler/icons-svelte-runes';
	import type { CatalogTool } from '$lib/tool-catalog';
	import type { SplitRange } from '$lib/split-ranges';
	import type { SplitOptions } from '$lib/pdf/types';
	import { getWorkspace, formatSize } from '$lib/workspace.svelte';
	import { processCompressPdf, processPdfs, processSplitPdf } from '$lib/pdf/processor';
	import PdfDropzone from './PdfDropzone.svelte';
	import PdfPreview from './PdfPreview.svelte';
	import CompressSettings from './CompressSettings.svelte';
	import SplitSettings from './SplitSettings.svelte';
	import SplitViewer from './SplitViewer.svelte';
	let { tool }: { tool: CatalogTool } = $props();
	const workspace = getWorkspace();
	const isMerge = $derived(tool.id === 'merge');
	const isSplit = $derived(tool.id === 'split');
	const isCompress = $derived(tool.id === 'compress');
	const canOrder = $derived(isMerge || isCompress);
	const accent = $derived(
		isMerge ? 'text-merge' : isSplit ? 'text-split' : isCompress ? 'text-compress' : 'text-brand'
	);
	let input = $state<HTMLInputElement>();
	let dragged = $state<File | null>(null);
	let dragOrder = $state<File[] | null>(null);
	let dropTarget = $state<File | null>(null);
	let trashHovered = $state(false);
	let trashZone = $state<HTMLDivElement>();
	let orderAnnouncement = $state('');
	let activePointer = -1;
	let sortAscending = $state(true);
	let reducedMotion = $state(false);
	const visibleFiles = $derived(dragOrder ?? workspace.files);
	const cards = $derived([...visibleFiles, null]);
	onMount(() => {
		const preference = window.matchMedia('(prefers-reduced-motion: reduce)');
		const update = () => {
			reducedMotion = preference.matches;
		};
		update();
		preference.addEventListener('change', update);
		return () => preference.removeEventListener('change', update);
	});
	let pageCount = $state(0);
	let splitRanges = $state<SplitRange[]>([{ id: 0, from: 1, to: 1 }]);
	let splitMode = $state<'ranges' | 'fixed'>('ranges');
	let splitInterval = $state(1);
	let splitCombine = $state(false);
	let compressLevel = $state<'light' | 'balanced' | 'strong'>('balanced');
	let compressRemoveMetadata = $state(false);
	let compressRemoveThumbnails = $state(false);
	let filename = $state('plico-merged');
	let downloadLink: HTMLAnchorElement;
	let processing = $state(false);
	let error = $state('');
	let result = $state('');
	let resultFormat = $state<'pdf' | 'zip'>('pdf');
	let resultSize = $state(0);
	let resultInputSize = $state(0);
	let controller: AbortController | undefined;
	const currentFile = $derived(workspace.files[0]);
	const splitSignature = $derived(
		JSON.stringify({
			mode: splitMode,
			interval: splitInterval,
			combine: splitCombine,
			ranges: splitRanges.map(({ from, to }) => [from, to])
		})
	);
	const compressSignature = $derived(
		JSON.stringify([compressLevel, compressRemoveMetadata, compressRemoveThumbnails])
	);
	const compressOptions = $derived({
		imageQuality: compressLevel === 'light' ? 0 : compressLevel === 'balanced' ? 80 : 50,
		maxImageDimension: compressLevel === 'strong' ? 1600 : compressLevel === 'balanced' ? 2400 : 0,
		removeMetadata: compressRemoveMetadata,
		removeThumbnails: compressRemoveThumbnails
	});
	const splitValid = $derived(
		!!currentFile &&
			pageCount > 0 &&
			(splitMode === 'ranges'
				? splitRanges.length > 0 &&
					splitRanges.every(
						({ from, to }) =>
							Number.isInteger(from) &&
							Number.isInteger(to) &&
							from >= 1 &&
							to >= from &&
							to <= pageCount
					)
				: Number.isInteger(splitInterval) && splitInterval > 0)
	);
	const actionDisabled = $derived(
		isMerge
			? workspace.files.length < 2 || processing || !!dragged
			: isSplit
				? !splitValid || processing
				: isCompress
					? workspace.files.length === 0 || processing || !!dragged
					: true
	);
	const actionUnavailable = $derived(
		isMerge
			? workspace.files.length < 2 || !!dragged
			: isSplit
				? !splitValid
				: isCompress
					? workspace.files.length === 0 || !!dragged
					: true
	);
	const downloadName = $derived(
		isSplit
			? `${currentFile?.name.replace(/\.pdf$/i, '') || 'document'}-split.${resultFormat}`
			: isCompress
				? `${currentFile?.name.replace(/\.pdf$/i, '') || 'document'}-compressed.${resultFormat}`
				: `${filename.trim().replace(/\.pdf$/i, '') || 'plico-merged'}.pdf`
	);
	$effect(() => {
		void workspace.files;
		void filename;
		void splitSignature;
		void compressSignature;
		untrack(clearResult);
	});
	$effect(() => {
		void currentFile;
		pageCount = 0;
		splitRanges = [{ id: 0, from: 1, to: 1 }];
		splitMode = 'ranges';
		splitInterval = 1;
		splitCombine = false;
	});
	function clearResult() {
		controller?.abort();
		controller = undefined;
		processing = false;
		error = '';
		if (result) URL.revokeObjectURL(result);
		result = '';
		resultFormat = 'pdf';
		resultSize = 0;
		resultInputSize = 0;
	}
	onDestroy(clearResult);
	async function merge() {
		if (processing || dragged || workspace.files.length < 2) return;
		clearResult();
		const job = new AbortController();
		controller = job;
		processing = true;
		try {
			const bytes = await processPdfs('merge', [...workspace.files], job.signal);
			if (job.signal.aborted) return;
			const url = URL.createObjectURL(
				new Blob([bytes.slice().buffer], { type: 'application/pdf' })
			);
			processing = false;
			result = url;
			await tick();
			if (!job.signal.aborted && result === url) downloadLink?.click();
		} catch (cause) {
			if (!job.signal.aborted)
				error = cause instanceof Error ? cause.message : 'Could not merge these PDFs.';
		} finally {
			if (controller === job) {
				processing = false;
				controller = undefined;
			}
		}
	}
	async function split() {
		if (processing || !splitValid || !currentFile) return;
		clearResult();
		const job = new AbortController();
		controller = job;
		processing = true;
		const options: SplitOptions =
			splitMode === 'ranges'
				? {
						mode: 'ranges',
						ranges: splitRanges.map(({ from, to }) => ({ from, to })),
						combine: splitCombine
					}
				: { mode: 'fixed', interval: splitInterval };
		try {
			const output = await processSplitPdf(currentFile, options, job.signal);
			if (job.signal.aborted) return;
			const mime = output.format === 'zip' ? 'application/zip' : 'application/pdf';
			const url = URL.createObjectURL(new Blob([output.bytes.slice().buffer], { type: mime }));
			processing = false;
			resultFormat = output.format;
			result = url;
			await tick();
			if (!job.signal.aborted && result === url) downloadLink?.click();
		} catch (cause) {
			if (!job.signal.aborted)
				error = cause instanceof Error ? cause.message : 'Could not split this PDF.';
		} finally {
			if (controller === job) {
				processing = false;
				controller = undefined;
			}
		}
	}
	async function compress() {
		if (processing || dragged || workspace.files.length === 0) return;
		clearResult();
		const job = new AbortController();
		controller = job;
		processing = true;
		try {
			const output = await processCompressPdf([...workspace.files], compressOptions, job.signal);
			if (job.signal.aborted) return;
			const mime = output.format === 'zip' ? 'application/zip' : 'application/pdf';
			const url = URL.createObjectURL(new Blob([output.bytes.slice().buffer], { type: mime }));
			processing = false;
			resultFormat = output.format;
			resultSize = output.bytes.byteLength;
			resultInputSize = workspace.files.reduce((total, file) => total + file.size, 0);
			result = url;
			await tick();
			if (!job.signal.aborted && result === url) downloadLink?.click();
		} catch (cause) {
			if (!job.signal.aborted)
				error = cause instanceof Error ? cause.message : 'Could not compress these PDFs.';
		} finally {
			if (controller === job) {
				processing = false;
				controller = undefined;
			}
		}
	}
	function add(files: FileList) {
		workspace.add(files);
		if (input) input.value = '';
	}
	function replace(files: FileList) {
		const file = Array.from(files).find(
			(item) => item.type === 'application/pdf' || /\.pdf$/i.test(item.name)
		);
		if (file) {
			workspace.files = [file];
			workspace.error = '';
		} else if (files.length) {
			workspace.error = 'Please choose a PDF.';
		}
		if (input) input.value = '';
	}
	function sortByFilename() {
		const direction = sortAscending ? 1 : -1;
		workspace.files = [...workspace.files].sort(
			(a, b) => a.name.localeCompare(b.name, undefined, { numeric: true }) * direction
		);
		sortAscending = !sortAscending;
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
		node.addEventListener('pointerdown', pointerDown);
		node.addEventListener('pointermove', pointerMove);
		node.addEventListener('pointerup', pointerUp);
		node.addEventListener('pointercancel', pointerCancel);
		node.addEventListener('lostpointercapture', () => {
			if (!movingSlot && pointerId >= 0 && !node.hasPointerCapture(pointerId)) finish(false);
		});
		return {
			destroy() {
				if (activePointer === pointerId) activePointer = -1;
				cancelAnimationFrame(frameId);
				gsap.killTweensOf(surface);
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

<main id="main-content" class="flex flex-1 flex-col">
	<div
		class="grid flex-1 grid-cols-[minmax(0,1fr)] lg:grid-cols-[minmax(0,1fr)_22rem] xl:grid-cols-[minmax(0,1fr)_24rem]"
	>
		<section
			aria-label="Documents"
			class="relative isolate flex min-w-0 flex-col overflow-hidden px-6 pt-6 sm:px-10 lg:px-16 lg:pt-10 {isSplit
				? 'pb-2 lg:pb-16'
				: 'pb-12 lg:pb-20'}"
			ondragover={(event) => event.preventDefault()}
			ondrop={(event) => {
				if (!event.defaultPrevented && !dragged && event.dataTransfer?.files.length) {
					event.preventDefault();
					if (!processing) {
						if (isSplit) replace(event.dataTransfer.files);
						else workspace.add(event.dataTransfer.files);
					}
				}
			}}
		>
			<img
				src="/hero-contours.svg"
				alt=""
				aria-hidden="true"
				class="pointer-events-none absolute -right-48 -bottom-48 -z-10 w-240 max-w-none opacity-[0.07] select-none"
			/>
			{#if dragged}
				<div
					bind:this={trashZone}
					transition:fade={{ duration: reducedMotion ? 0 : 160 }}
					aria-hidden="true"
					class="pointer-events-none absolute top-10 bottom-10 -left-6 z-10 hidden w-32 flex-col items-center justify-center gap-3 rounded-2xl border-2 border-dashed text-center text-xs font-semibold transition-colors lg:flex {trashHovered
						? 'border-red-400 bg-red-500/25 text-red-100'
						: 'border-red-500/50 bg-red-500/[0.08] text-red-300'}"
				>
					<IconTrash size={30} stroke={1.8} />
				</div>
			{/if}
			{#if canOrder && workspace.files.length > 1}
				<button
					transition:fade={{ duration: reducedMotion ? 0 : 100 }}
					disabled={processing || !!dragged}
					class="absolute top-6 right-6 z-10 flex size-11 items-center justify-center rounded-xl border-2 border-white/10 bg-panel text-muted transition-colors disabled:opacity-40 sm:right-10 lg:top-10 lg:right-16 {isCompress
						? 'hover:border-compress/40 hover:text-compress'
						: 'hover:border-merge/40 hover:text-merge'}"
					aria-label={`Sort filenames ${sortAscending ? 'ascending' : 'descending'}`}
					title={`Sort filenames ${sortAscending ? 'ascending' : 'descending'}`}
					onclick={sortByFilename}
					>{#if sortAscending}<IconSortAscendingLetters
							size={20}
						/>{:else}<IconSortDescendingLetters size={20} />{/if}</button
				>
			{/if}
			<div class="grid min-h-0 flex-1">
				{#if workspace.files.length === 0}
					<div
						in:fade={{ duration: reducedMotion ? 0 : 260, easing: cubicOut }}
						out:fade={{ duration: reducedMotion ? 0 : 150, easing: cubicIn }}
						class="col-start-1 row-start-1 mx-auto flex w-full max-w-xl flex-col justify-center py-12 lg:py-20"
					>
						<div class="h-80"><PdfDropzone selectedTool={tool} emptyOnly /></div>
					</div>
				{:else}
					<div
						in:fade={{ duration: reducedMotion ? 0 : 260, easing: cubicOut }}
						out:fade={{ duration: reducedMotion ? 0 : 150, easing: cubicIn }}
						class="col-start-1 row-start-1 flex min-w-0 flex-col"
					>
						{#if !isSplit}<input
								bind:this={input}
								type="file"
								accept="application/pdf,.pdf"
								multiple
								class="hidden"
								aria-label="Add PDF files"
								onchange={() => input?.files && add(input.files)}
							/>{/if}
						{#if isSplit && currentFile}
							<div class="my-auto w-full py-6 lg:py-10">
								{#key currentFile}<SplitViewer
										file={currentFile}
										ranges={splitRanges}
										mode={splitMode}
										interval={splitInterval}
										onrangechange={(id, from, to) =>
											(splitRanges = splitRanges.map((range) =>
												range.id === id ? { ...range, from, to } : range
											))}
										onload={(count) => {
											pageCount = count;
											if (
												splitRanges.length === 1 &&
												splitRanges[0].from === 1 &&
												splitRanges[0].to === 1
											)
												splitRanges[0].to = count;
										}}
										onremove={() => workspace.remove(currentFile)}
									/>{/key}
							</div>
						{:else}
							<ol
								aria-label="PDF order"
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
											: ''} {file && dragged === file ? 'z-20' : ''}"
									>
										{#if file}
											{#if dragged === file}<div
													class="pointer-events-none absolute inset-x-0 top-0 aspect-[3/4] rounded-xl border-2 border-dashed {isCompress
														? 'border-compress/35 bg-compress/5'
														: 'border-merge/35 bg-merge/5'}"
												></div>{/if}
											<div data-drag-surface class="relative origin-[50%_12%]">
												<div
													class="relative overflow-hidden rounded-xl border-2 bg-panel shadow-lg shadow-black/20 transition-[border-color,box-shadow] duration-200 {dragged ===
													file
														? isCompress
															? 'border-compress/70 shadow-2xl shadow-black/60'
															: 'border-merge/70 shadow-2xl shadow-black/60'
														: dropTarget === file && dragged
															? isCompress
																? 'border-compress/70 shadow-lg shadow-compress/10'
																: 'border-merge/70 shadow-lg shadow-merge/10'
															: 'border-white/10 group-hover:border-white/25'}"
												>
													<PdfPreview {file} />
													{#if !isMerge}<span
															class="absolute top-2 left-2 flex min-w-7 items-center justify-center rounded-md bg-canvas/80 px-1.5 py-1 text-[11px] font-bold text-white backdrop-blur-sm"
															>{index + 1}</span
														>{/if}<button
														disabled={processing || !!dragged}
														class="absolute top-2 right-2 flex size-8 items-center justify-center rounded-lg bg-canvas/80 text-white backdrop-blur-sm transition-colors hover:bg-convert hover:text-canvas disabled:opacity-40"
														aria-label={`Remove ${file.name}`}
														onclick={() => workspace.remove(file)}
														><IconX size={18} stroke={2.5} /></button
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
												disabled={processing || !!dragged}
												onclick={() => input?.click()}
												class="group flex aspect-[3/4] w-full flex-col items-center justify-center gap-4 rounded-xl border-2 border-dashed transition-[border-color,background-color,color] disabled:opacity-40 {isCompress
													? 'border-compress/25 bg-compress/[0.03] text-compress/75 hover:border-compress/60 hover:bg-compress/[0.07] hover:text-compress'
													: 'border-merge/25 bg-merge/[0.03] text-merge/75 hover:border-merge/60 hover:bg-merge/[0.07] hover:text-merge'}"
											>
												<span
													class="flex size-12 items-center justify-center rounded-full motion-safe:transition-transform motion-safe:group-hover:scale-110 {isCompress
														? 'bg-compress/10'
														: 'bg-merge/10'}"><IconPlus size={24} stroke={1.5} /></span
												>
											</button>
										{/if}
									</li>
								{/each}
							</ol>
							{#if canOrder}<p class="sr-only" aria-live="polite">{orderAnnouncement}</p>{/if}
						{/if}
						{#if workspace.error}<p role="alert" class="mt-4 text-sm text-convert">
								{workspace.error}
							</p>{/if}
					</div>
				{/if}
			</div>
		</section>
		<aside
			aria-label={`${tool.label} settings`}
			class="m-6 flex flex-col rounded-2xl bg-panel lg:sticky lg:top-6 lg:ml-0 lg:h-[calc(100svh-9rem)] lg:self-start lg:overflow-y-auto"
		>
			<div class="flex-1 space-y-7 p-6 sm:p-8">
				<h1 class="flex items-center gap-3 text-xl font-semibold tracking-tight">
					<tool.icon size={24} stroke={1.7} class={`shrink-0 ${accent}`} />{tool.label}
				</h1>
				{#if isMerge}
					<label class="block text-sm font-medium"
						>Output filename
						<div
							class="mt-3 flex items-center rounded-xl border border-white/10 bg-canvas px-3 focus-within:border-white/25"
						>
							<input
								bind:value={filename}
								class="min-w-0 flex-1 bg-transparent py-3 text-sm outline-none"
								aria-label="Output filename"
								placeholder="plico-merged"
							/><span class="text-xs text-muted">.pdf</span>
						</div></label
					>
				{:else if isSplit}
					{#key currentFile}<SplitSettings
							{pageCount}
							{reducedMotion}
							bind:ranges={splitRanges}
							bind:mode={splitMode}
							bind:interval={splitInterval}
							bind:combine={splitCombine}
						/>{/key}
				{:else if isCompress}
					<CompressSettings
						bind:level={compressLevel}
						bind:removeMetadata={compressRemoveMetadata}
						bind:removeThumbnails={compressRemoveThumbnails}
					/>
				{/if}
			</div>
			<div class="shrink-0 space-y-4 p-6 sm:p-8" aria-live="polite">
				<a
					bind:this={downloadLink}
					href={result}
					rel="external"
					download={downloadName}
					class="hidden"
					tabindex="-1"
					aria-hidden="true">Download PDF</a
				>
				{#if isCompress && result && resultInputSize > 0}
					<p
						class="flex flex-wrap items-center justify-center gap-x-1.5 gap-y-0.5 text-center text-xs text-muted"
					>
						<span class="inline-flex items-center gap-1.5">
							<span>{formatSize(resultInputSize)}</span>
							<span class="sr-only">to</span>
							<IconArrowRight size={14} stroke={1.75} aria-hidden="true" />
							<span>{formatSize(resultSize)}</span>
						</span>
						{#if resultSize < resultInputSize}
							<span>({Math.round((1 - resultSize / resultInputSize) * 100)}% smaller)</span>
						{/if}
					</p>
				{/if}
				<button
					disabled={actionDisabled}
					onclick={() =>
						result
							? downloadLink?.click()
							: isSplit
								? void split()
								: isCompress
									? void compress()
									: void merge()}
					aria-label={result
						? `Download ${resultFormat.toUpperCase()} again`
						: processing
							? isSplit
								? 'Splitting PDF'
								: isCompress
									? 'Compressing PDF'
									: 'Merging PDF'
							: tool.label}
					class="group relative isolate flex min-h-14 w-full items-center justify-center overflow-hidden rounded-xl bg-brand px-4 py-4 text-sm font-bold text-canvas transition-[background-color,transform] duration-200 enabled:hover:bg-violet-300 disabled:cursor-not-allowed motion-safe:enabled:active:scale-[0.985] {actionUnavailable
						? 'opacity-40'
						: ''}"
				>
					<span
						class="pointer-events-none absolute inset-0 origin-left bg-merge motion-safe:transition-transform motion-safe:duration-500 motion-safe:ease-[cubic-bezier(0.22,1,0.36,1)] {result
							? 'scale-x-100'
							: 'scale-x-0'}"
						aria-hidden="true"
					></span>
					<span class="relative z-10 grid place-items-center">
						<span
							aria-hidden={!!result}
							class="col-start-1 row-start-1 flex items-center justify-center gap-3 whitespace-nowrap motion-safe:transition-[opacity,transform] motion-safe:duration-200 {result
								? '-translate-y-2 opacity-0'
								: 'translate-y-0 opacity-100'}"
							>{#if processing}<IconLoader2 class="animate-spin" size={20} />{isSplit
									? 'Splitting...'
									: isCompress
										? 'Compressing...'
										: 'Merging...'}{:else}
								{tool.label}<IconArrowRight size={20} />{/if}</span
						>
						<span
							aria-hidden={!result}
							class="col-start-1 row-start-1 flex items-center justify-center gap-3 whitespace-nowrap motion-safe:transition-[opacity,transform] motion-safe:duration-300 {result
								? 'translate-y-0 opacity-100 motion-safe:delay-100'
								: 'translate-y-2 opacity-0'}"
							><IconDownload size={20} />Download {resultFormat.toUpperCase()}</span
						>
					</span></button
				>
				{#if error}<p role="alert" class="text-sm text-convert">{error}</p>{/if}
			</div>
		</aside>
	</div>
</main>
