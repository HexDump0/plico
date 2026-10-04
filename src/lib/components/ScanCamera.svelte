<script lang="ts">
	import gsap from 'gsap';
	import { onDestroy, onMount } from 'svelte';
	import { cubicOut } from 'svelte/easing';
	import { fade, scale } from 'svelte/transition';
	import { IconCameraRotate, IconCheck, IconX } from '@tabler/icons-svelte-runes';
	import { findScanPage } from '$lib/pdf/processor';

	let {
		reducedMotion,
		oncapture,
		onclose
	}: {
		reducedMotion: boolean;
		oncapture: (file: File) => void;
		onclose: () => void;
	} = $props();

	let video = $state<HTMLVideoElement>();
	let viewport = $state<HTMLDivElement>();
	let stack = $state<HTMLDivElement>();
	let shutter = $state<HTMLButtonElement>();
	let stream: MediaStream | undefined;
	let facing = $state<'environment' | 'user'>('environment');
	let cameras = $state(0);
	let status = $state<'starting' | 'live' | 'blocked' | 'missing' | 'failed'>('starting');
	let frameWidth = $state(0);
	let frameHeight = $state(0);
	let boxWidth = $state(0);
	let boxHeight = $state(0);
	// The page outline as drawn, easing toward the latest one found.
	let outline = $state<number[] | null>(null);
	let target: number[] | null = null;
	let misses = 0;
	let captured = $state(0);
	let lastShot = $state('');
	let flash = $state<HTMLDivElement>();
	let capturing = $state(false);
	let destroyed = false;
	let raf = 0;
	const shots: string[] = [];

	// Where the frame sits in the box, as `object-contain` draws it.
	const frame = $derived.by(() => {
		if (!frameWidth || !frameHeight || !boxWidth || !boxHeight) return null;
		const fit = Math.min(boxWidth / frameWidth, boxHeight / frameHeight);
		const width = frameWidth * fit;
		const height = frameHeight * fit;
		return { left: (boxWidth - width) / 2, top: (boxHeight - height) / 2, width, height };
	});
	const points = $derived(
		outline
			? [0, 1, 2, 3]
					.map((corner) => `${outline![corner * 2]},${outline![corner * 2 + 1]}`)
					.join(' ')
			: ''
	);

	async function start() {
		stream?.getTracks().forEach((track) => track.stop());
		status = 'starting';
		outline = target = null;
		try {
			stream = await navigator.mediaDevices.getUserMedia({
				video: {
					facingMode: { ideal: facing },
					width: { ideal: 3840 },
					height: { ideal: 2160 }
				},
				audio: false
			});
			if (destroyed) {
				stream.getTracks().forEach((track) => track.stop());
				return;
			}
			if (!video) return;
			video.srcObject = stream;
			await video.play();
			frameWidth = video.videoWidth;
			frameHeight = video.videoHeight;
			status = 'live';
			cameras = (await navigator.mediaDevices.enumerateDevices()).filter(
				(device) => device.kind === 'videoinput'
			).length;
			shutter?.focus();
		} catch (error) {
			const name = error instanceof DOMException ? error.name : '';
			status =
				name === 'NotAllowedError' || name === 'SecurityError'
					? 'blocked'
					: name === 'NotFoundError' || name === 'OverconstrainedError'
						? 'missing'
						: 'failed';
		}
	}

	// Looks for the page a few times a second, on a small copy of the frame.
	async function watch() {
		const probe = new OffscreenCanvas(1, 1);
		const context = probe.getContext('2d', { willReadFrequently: true });
		while (!destroyed) {
			if (status === 'live' && video && video.videoWidth && context) {
				const fit = 480 / Math.max(video.videoWidth, video.videoHeight);
				probe.width = Math.round(video.videoWidth * fit);
				probe.height = Math.round(video.videoHeight * fit);
				context.drawImage(video, 0, 0, probe.width, probe.height);
				try {
					const corners = await findScanPage(context.getImageData(0, 0, probe.width, probe.height));
					if (corners) {
						target = corners;
						misses = 0;
					} else if (++misses > 3) target = null;
				} catch {
					target = null;
				}
			}
			await new Promise((resolve) => setTimeout(resolve, 120));
		}
	}

	function ease() {
		if (!target) outline = null;
		else if (!outline || reducedMotion) outline = [...target];
		else {
			const goal = target;
			outline = outline.map((value, at) => value + (goal[at] - value) * 0.3);
		}
		raf = requestAnimationFrame(ease);
	}

	async function capture() {
		if (!video || status !== 'live' || capturing) return;
		capturing = true;
		const canvas = document.createElement('canvas');
		canvas.width = video.videoWidth;
		canvas.height = video.videoHeight;
		canvas.getContext('2d')?.drawImage(video, 0, 0);
		if (flash && !reducedMotion)
			gsap.fromTo(flash, { opacity: 0.85 }, { opacity: 0, duration: 0.35, ease: 'power2.out' });
		const blob = await new Promise<Blob | null>((resolve) =>
			canvas.toBlob(resolve, 'image/jpeg', 0.92)
		);
		capturing = false;
		if (!blob || destroyed) return;
		const number = String(captured + 1).padStart(2, '0');
		const stamp = new Date().toISOString().slice(0, 19).replace(/[-:]/g, '').replace('T', '-');
		oncapture(new File([blob], `scan-${stamp}-${number}.jpg`, { type: 'image/jpeg' }));
		const url = URL.createObjectURL(blob);
		shots.push(url);
		fly(url);
		lastShot = url;
		captured++;
	}

	// The shot shrinks from the frame into the stack.
	function fly(url: string) {
		if (reducedMotion || !viewport || !stack || !frame) return;
		const from = viewport.getBoundingClientRect();
		const to = stack.getBoundingClientRect();
		const image = document.createElement('img');
		image.src = url;
		image.alt = '';
		Object.assign(image.style, {
			position: 'fixed',
			left: `${from.left + frame.left}px`,
			top: `${from.top + frame.top}px`,
			width: `${frame.width}px`,
			height: `${frame.height}px`,
			borderRadius: '6px',
			zIndex: '60',
			pointerEvents: 'none',
			transformOrigin: '0 0'
		});
		document.body.append(image);
		gsap.to(image, {
			x: to.left - from.left - frame.left,
			y: to.top - from.top - frame.top,
			scaleX: to.width / frame.width,
			scaleY: to.height / frame.height,
			opacity: 0.4,
			duration: 0.5,
			ease: 'power3.inOut',
			onComplete: () => image.remove()
		});
	}

	onMount(() => {
		if (!navigator.mediaDevices?.getUserMedia) status = 'missing';
		else void start();
		void watch();
		raf = requestAnimationFrame(ease);
	});
	onDestroy(() => {
		destroyed = true;
		cancelAnimationFrame(raf);
		stream?.getTracks().forEach((track) => track.stop());
		for (const url of shots) URL.revokeObjectURL(url);
	});
</script>

<svelte:window
	onkeydown={(event) => {
		if (event.key === 'Escape') onclose();
	}}
/>

<div
	role="dialog"
	aria-modal="true"
	aria-label="Camera"
	class="fixed inset-0 z-50 flex flex-col bg-canvas"
	transition:fade={{ duration: reducedMotion ? 0 : 200 }}
>
	<div
		bind:this={viewport}
		bind:clientWidth={boxWidth}
		bind:clientHeight={boxHeight}
		class="relative min-h-0 flex-1 overflow-hidden"
	>
		<video bind:this={video} playsinline muted class="absolute inset-0 size-full object-contain"
		></video>
		{#if frame && outline}
			<svg
				viewBox="0 0 1 1"
				preserveAspectRatio="none"
				aria-hidden="true"
				transition:fade={{ duration: reducedMotion ? 0 : 160 }}
				class="pointer-events-none absolute overflow-visible"
				style:left="{frame.left}px"
				style:top="{frame.top}px"
				style:width="{frame.width}px"
				style:height="{frame.height}px"
			>
				<polygon
					{points}
					class="fill-convert/15 stroke-convert"
					stroke-width="3"
					vector-effect="non-scaling-stroke"
					stroke-linejoin="round"
				/>
			</svg>
		{/if}
		<div
			bind:this={flash}
			aria-hidden="true"
			class="pointer-events-none absolute inset-0 bg-white opacity-0"
		></div>
		{#if status !== 'live' && status !== 'starting'}
			<div
				class="absolute inset-0 grid place-items-center px-6"
				in:fade={{ duration: reducedMotion ? 0 : 200 }}
			>
				<p role="alert" class="max-w-sm text-center text-sm leading-relaxed text-muted">
					{status === 'blocked'
						? "Camera access is blocked. Allow it in your browser's site settings, or add photos instead."
						: status === 'missing'
							? 'No camera was found.'
							: 'The camera could not be started.'}
				</p>
			</div>
		{/if}
		<button
			type="button"
			aria-label="Close camera"
			class="absolute top-4 right-4 flex size-11 items-center justify-center rounded-xl bg-canvas/70 text-white backdrop-blur-sm transition-colors hover:bg-convert hover:text-canvas"
			onclick={onclose}><IconX size={20} /></button
		>
		{#if cameras > 1}
			<button
				type="button"
				aria-label="Switch camera"
				class="absolute top-4 left-4 flex size-11 items-center justify-center rounded-xl bg-canvas/70 text-white backdrop-blur-sm transition-colors hover:bg-convert hover:text-canvas"
				onclick={() => {
					facing = facing === 'environment' ? 'user' : 'environment';
					void start();
				}}><IconCameraRotate size={20} /></button
			>
		{/if}
	</div>
	<div
		class="grid grid-cols-3 items-center px-6 py-5 pb-[max(1.25rem,env(safe-area-inset-bottom))]"
	>
		<div bind:this={stack} class="relative size-14 justify-self-start">
			{#if lastShot}
				{#key lastShot}<img
						src={lastShot}
						alt=""
						in:scale={{
							duration: reducedMotion ? 0 : 200,
							delay: reducedMotion ? 0 : 380,
							start: 0.8,
							easing: cubicOut
						}}
						class="absolute inset-0 size-full rounded-lg border-2 border-white/20 object-cover"
					/>{/key}
				<span
					class="absolute -top-2 -right-2 flex min-w-6 items-center justify-center rounded-full bg-convert px-1.5 py-0.5 text-[11px] font-bold text-canvas tabular-nums"
					>{captured}</span
				>
			{/if}
		</div>
		<button
			bind:this={shutter}
			type="button"
			aria-label="Take photo"
			disabled={status !== 'live' || capturing}
			class="group flex size-18 items-center justify-center justify-self-center rounded-full border-4 border-white/90 disabled:opacity-40"
			onclick={() => void capture()}
			><span
				class="size-14 rounded-full bg-white group-enabled:group-hover:scale-95 group-enabled:group-active:scale-85 motion-safe:transition-transform motion-safe:duration-150"
			></span></button
		>
		<button
			type="button"
			class="flex items-center gap-2 justify-self-end rounded-xl bg-convert px-4 py-3 text-sm font-bold text-canvas transition-[filter] hover:brightness-110"
			onclick={onclose}><IconCheck size={18} />Done</button
		>
	</div>
</div>
