import { createContext } from 'svelte';

export type AcceptedFileType = 'pdf' | 'image' | 'any';

export class Workspace {
	files = $state<File[]>([]);
	error = $state('');
	add(incoming: FileList | File[], accept: AcceptedFileType = 'pdf') {
		const candidates = Array.from(incoming);
		const allowed = candidates.filter((file) => {
			const isPdf = file.type === 'application/pdf' || /\.pdf$/i.test(file.name);
			const isImage =
				file.type === 'image/jpeg' ||
				file.type === 'image/png' ||
				/\.(jpe?g|png)$/i.test(file.name);
			if (accept === 'image') return isImage;
			if (accept === 'any') return isPdf || isImage;
			return isPdf;
		});
		this.error =
			allowed.length === candidates.length
				? ''
				: accept === 'image'
					? 'Please choose JPG or PNG images. Other file types were skipped.'
					: 'Please choose PDFs. Other file types were skipped.';
		this.files = [
			...this.files,
			...allowed.filter(
				(file, index) =>
					![...this.files, ...allowed.slice(0, index)].some(
						(existing) =>
							existing.name === file.name &&
							existing.size === file.size &&
							existing.lastModified === file.lastModified
					)
			)
		];
	}
	clear() {
		this.files = [];
		this.error = '';
	}
	remove(file: File) {
		this.files = this.files.filter((item) => item !== file);
	}
	move(from: number, to: number) {
		if (from < 0 || to < 0 || from >= this.files.length || to >= this.files.length) return;
		const next = [...this.files];
		const [file] = next.splice(from, 1);
		next.splice(to, 0, file);
		this.files = next;
	}
}
export const [getWorkspace, setWorkspace] = createContext<Workspace>();
export function formatSize(bytes: number) {
	if (bytes === 0) return '0 KB';
	return bytes < 1024 * 1024
		? `${Math.max(1, Math.round(bytes / 1024))} KB`
		: `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}
