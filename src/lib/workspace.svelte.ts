import { createContext } from 'svelte';

export class Workspace {
	files = $state<File[]>([]);
	error = $state('');
	add(incoming: FileList | File[]) {
		const candidates = Array.from(incoming);
		const pdfs = candidates.filter(
			(file) => file.type === 'application/pdf' || /\.pdf$/i.test(file.name)
		);
		this.error =
			pdfs.length === candidates.length ? '' : 'Please choose PDFs. Other file types were skipped.';
		this.files = [
			...this.files,
			...pdfs.filter(
				(file, index) =>
					![...this.files, ...pdfs.slice(0, index)].some(
						(existing) =>
							existing.name === file.name &&
							existing.size === file.size &&
							existing.lastModified === file.lastModified
					)
			)
		];
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
