import { SvelteMap } from 'svelte/reactivity';
import { openScanPhoto, scanPhoto } from './processor';
import type { ScanLook, ScanPage } from './types';

/// The whole photo, for a page that was not found.
export const FULL_PHOTO = [0, 0, 1, 0, 1, 1, 0, 1];

// Card previews are drawn this long; the cards show them at about 210 CSS
// pixels, so this stays sharp on a dense screen.
const PREVIEW_SIDE = 720;

export type ScanPhoto = {
	status: 'opening' | 'ready' | 'failed';
	/// An upright copy of the photo, at most 1600 pixels long, to show and
	/// to preview from.
	proxy?: Blob;
	url?: string;
	/// The photo's upright size.
	width: number;
	height: number;
	/// Where the page was found, or `null`.
	found: number[] | null;
	corners: number[];
	turns: number;
	preview?: {
		url: string;
		key: string;
		width: number;
		height: number;
		/// The turn it was drawn at, so a newer turn can be shown on it at once.
		turns: number;
		failed?: boolean;
	};
};

function sameCorners(a: number[], b: number[]) {
	return a.every((value, index) => Math.abs(value - b[index]) < 1e-4);
}

/// Every photo being scanned: its page's corners and turn, and a preview of
/// it with the current look, kept up to date one at a time.
export class ScanPhotos {
	photos = new SvelteMap<File, ScanPhoto>();
	look = $state<ScanLook>('document');
	/// The photo on screen in the editor, whose preview comes first.
	focus = $state<File | null>(null);
	private order: File[] = [];
	private opening = false;
	private previewing = false;
	private destroyed = false;

	get(file: File) {
		return this.photos.get(file);
	}

	page(file: File): ScanPage {
		const photo = this.photos.get(file);
		return { corners: photo?.corners ?? FULL_PHOTO, turns: photo?.turns ?? 0 };
	}

	get ready() {
		return this.order.every((file) => this.photos.get(file)?.status === 'ready');
	}

	/// Follows the workspace's files: opens new photos and forgets removed ones.
	sync(files: File[]) {
		this.order = [...files];
		for (const [file, photo] of this.photos)
			if (!files.includes(file)) {
				this.release(photo);
				this.photos.delete(file);
			}
		for (const file of files)
			if (!this.photos.has(file))
				this.photos.set(file, {
					status: 'opening',
					width: 0,
					height: 0,
					found: null,
					corners: FULL_PHOTO,
					turns: 0
				});
		void this.open();
		void this.refresh();
	}

	setCorners(file: File, corners: number[]) {
		this.update(file, { corners: [...corners] });
	}

	turn(file: File, quarter: number) {
		const photo = this.photos.get(file);
		if (photo) this.update(file, { turns: (photo.turns + quarter + 4) % 4 });
	}

	/// Whether the corners are the page as found, or the whole photo when none was.
	isFound(file: File) {
		const photo = this.photos.get(file);
		return !!photo?.found && sameCorners(photo.corners, photo.found);
	}

	isWhole(file: File) {
		const photo = this.photos.get(file);
		return !photo || sameCorners(photo.corners, FULL_PHOTO);
	}

	setLook(look: ScanLook) {
		this.look = look;
		void this.refresh();
	}

	destroy() {
		this.destroyed = true;
		for (const photo of this.photos.values()) this.release(photo);
		this.photos.clear();
	}

	private update(file: File, change: Partial<ScanPhoto>) {
		const photo = this.photos.get(file);
		if (!photo) return;
		this.photos.set(file, { ...photo, ...change });
		void this.refresh();
	}

	private key(photo: ScanPhoto) {
		return JSON.stringify([photo.corners.map((value) => value.toFixed(4)), photo.turns, this.look]);
	}

	private release(photo: ScanPhoto) {
		if (photo.url) URL.revokeObjectURL(photo.url);
		if (photo.preview?.url) URL.revokeObjectURL(photo.preview.url);
	}

	// One at a time: each photo is decoded at full size in the worker.
	private async open() {
		if (this.opening) return;
		this.opening = true;
		try {
			for (;;) {
				const file = this.order.find((item) => this.photos.get(item)?.status === 'opening');
				if (!file || this.destroyed) break;
				try {
					const opened = await openScanPhoto(file);
					const photo = this.photos.get(file);
					if (!photo || this.destroyed) continue;
					this.photos.set(file, {
						...photo,
						status: 'ready',
						proxy: opened.proxy,
						url: URL.createObjectURL(opened.proxy),
						width: opened.width,
						height: opened.height,
						found: opened.corners,
						corners: opened.corners ?? FULL_PHOTO
					});
				} catch {
					const photo = this.photos.get(file);
					if (photo) this.photos.set(file, { ...photo, status: 'failed' });
				}
				void this.refresh();
			}
		} finally {
			this.opening = false;
		}
	}

	private stale(file: File) {
		const photo = this.photos.get(file);
		return photo?.status === 'ready' && photo.preview?.key !== this.key(photo);
	}

	/// Brings previews up to date, the photo in the editor first.
	async refresh() {
		if (this.previewing) return;
		this.previewing = true;
		try {
			for (;;) {
				if (this.destroyed) break;
				const file =
					this.focus && this.stale(this.focus)
						? this.focus
						: this.order.find((item) => this.stale(item));
				if (!file) break;
				const photo = this.photos.get(file)!;
				const key = this.key(photo);
				try {
					const output = await scanPhoto(photo.proxy!, photo, this.look, PREVIEW_SIDE);
					const current = this.photos.get(file);
					if (!current || this.destroyed) continue;
					const blob = new Blob([output.bytes.slice().buffer], {
						type: output.format === 'png' ? 'image/png' : 'image/jpeg'
					});
					const bitmap = await createImageBitmap(blob);
					const size = { width: bitmap.width, height: bitmap.height };
					bitmap.close();
					const previous = current.preview?.url;
					// The old picture stays until the new one has crossfaded over it.
					if (previous) setTimeout(() => URL.revokeObjectURL(previous), 1000);
					this.photos.set(file, {
						...current,
						preview: { url: URL.createObjectURL(blob), key, turns: photo.turns, ...size }
					});
				} catch (error) {
					// A cancelled job stops the worker under a preview; try again later.
					if (error instanceof DOMException || /cancel/i.test(String(error))) break;
					const current = this.photos.get(file);
					if (current)
						this.photos.set(file, {
							...current,
							preview: {
								url: current.preview?.url ?? '',
								key,
								width: current.preview?.width ?? 0,
								height: current.preview?.height ?? 0,
								turns: current.preview?.turns ?? photo.turns,
								failed: true
							}
						});
				}
			}
		} finally {
			this.previewing = false;
		}
	}
}

/// A transform that shows a preview drawn at one turn as if drawn at the
/// photo's current one, fitted to a box of `aspect` (width over height).
export function pendingTurn(photo: ScanPhoto, aspect: number) {
	const preview = photo.preview;
	if (!preview?.width) return { degrees: 0, scale: 1 };
	let quarter = (((photo.turns - preview.turns) % 4) + 4) % 4;
	if (quarter > 2) quarter -= 4;
	if (quarter % 2 === 0) return { degrees: quarter * 90, scale: 1 };
	const { width, height } = preview;
	const fit = Math.min(aspect / width, 1 / height);
	const turned = Math.min(aspect / height, 1 / width);
	return { degrees: quarter * 90, scale: turned / fit };
}
