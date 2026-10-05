import { createContext } from 'svelte';
import { SvelteMap } from 'svelte/reactivity';
import { acceptsFile, formatName, type DocumentInput } from './pdf/office-conversion';
import { useRepairedCopy } from './pdf/processor';

export type AcceptedFileType = 'pdf' | 'image' | DocumentInput;
/// `checking` keeps the unlock form in place while pdf.js tries a password;
/// `unlocked` holds it just long enough to fade out before the preview shows.
export type LockState = 'locked' | 'incorrect' | 'checking' | 'unlocked';

export class Workspace {
	private pdfFiles = $state<File[]>([]);
	private imageFiles = $state<File[]>([]);
	private officeFiles = $state<Record<DocumentInput, File[]>>({
		docx: [],
		pptx: [],
		xlsx: [],
		html: [],
		md: []
	});
	private pdfError = $state('');
	private imageError = $state('');
	private officeErrors = $state<Record<DocumentInput, string>>({
		docx: '',
		pptx: '',
		xlsx: '',
		html: '',
		md: ''
	});
	private activeType = $state<AcceptedFileType>('pdf');
	// Memory only, like the files themselves: nothing here survives a reload.
	private passwords = new SvelteMap<File, string>();
	private locks = new SvelteMap<File, LockState>();
	// Repaired copies of files the engine could not open, shown in their place.
	private repairs = new SvelteMap<File, File>();
	// Not reactive on purpose: the tool being left must not redraw with it
	// before the route transition captures it.
	private carried: File | undefined;
	get files() {
		return this.activeType === 'image'
			? this.imageFiles
			: this.activeType === 'pdf'
				? this.pdfFiles
				: this.officeFiles[this.activeType];
	}
	set files(files: File[]) {
		if (this.activeType === 'image') this.imageFiles = files;
		else if (this.activeType === 'pdf') this.pdfFiles = files;
		else this.officeFiles[this.activeType] = files;
		this.forgetRemoved();
	}
	get error() {
		return this.activeType === 'image'
			? this.imageError
			: this.activeType === 'pdf'
				? this.pdfError
				: this.officeErrors[this.activeType];
	}
	set error(error: string) {
		if (this.activeType === 'image') this.imageError = error;
		else if (this.activeType === 'pdf') this.pdfError = error;
		else this.officeErrors[this.activeType] = error;
	}
	use(type: AcceptedFileType) {
		this.activeType = type;
	}
	add(incoming: FileList | File[], accept: AcceptedFileType = 'pdf') {
		this.use(accept);
		const candidates = Array.from(incoming);
		const allowed = candidates.filter((file) => acceptsFile(file, accept));
		this.error =
			allowed.length === candidates.length
				? ''
				: `Please choose ${accept === 'image' ? 'JPG or PNG images' : accept === 'pdf' ? 'PDFs' : `${formatName(accept)} files`}. Other file types were skipped.`;
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
	/// A result taken on to the next tool, which picks it up as it opens.
	carry(file: File) {
		this.carried = file;
	}
	receive() {
		if (!this.carried) return;
		this.pdfFiles = [this.carried];
		this.pdfError = '';
		this.carried = undefined;
		this.forgetRemoved();
	}
	clear() {
		this.files = [];
		this.error = '';
	}
	passwordFor(file: File) {
		return this.passwords.get(file) ?? '';
	}
	passwordsFor(files: File[]) {
		return files.map((file) => this.passwordFor(file));
	}
	unlock(file: File, password: string) {
		this.locks.set(file, 'checking');
		// The same wrong password again changes nothing that would reload the
		// preview, so answer it here, in a later tick so the form sees it fail.
		if (this.passwords.get(file) === password) {
			setTimeout(() => this.locks.has(file) && this.locks.set(file, 'incorrect'));
			return;
		}
		this.passwords.set(file, password);
	}
	lockState(file: File) {
		return this.locks.get(file);
	}
	setLock(file: File, state: LockState | undefined) {
		if (state) this.locks.set(file, state);
		else this.locks.delete(file);
	}
	/// Called whenever pdf.js opens a file. Only a password just typed fades the
	/// form out; a file that never asked for one clears silently.
	opened(file: File) {
		if (this.locks.get(file) !== 'checking') {
			this.locks.delete(file);
			return;
		}
		this.locks.set(file, 'unlocked');
		setTimeout(() => {
			if (this.locks.get(file) === 'unlocked') this.locks.delete(file);
		}, 200);
	}
	repaired(file: File, copy: File) {
		this.repairs.set(file, copy);
		useRepairedCopy(file, copy);
	}
	repairedCopy(file: File) {
		return this.repairs.get(file);
	}
	get hasLockedFiles() {
		return this.files.some((file) => {
			const lock = this.locks.get(file);
			return lock !== undefined && lock !== 'unlocked';
		});
	}
	private forgetRemoved() {
		const kept = [...this.pdfFiles, ...this.imageFiles, ...Object.values(this.officeFiles).flat()];
		for (const file of this.passwords.keys()) if (!kept.includes(file)) this.passwords.delete(file);
		for (const file of this.locks.keys()) if (!kept.includes(file)) this.locks.delete(file);
		for (const file of this.repairs.keys()) if (!kept.includes(file)) this.repairs.delete(file);
	}
	remove(file: File) {
		if (this.pdfFiles.includes(file)) this.pdfFiles = this.pdfFiles.filter((item) => item !== file);
		if (this.imageFiles.includes(file))
			this.imageFiles = this.imageFiles.filter((item) => item !== file);
		for (const format of ['docx', 'pptx', 'xlsx', 'html', 'md'] as const) {
			if (this.officeFiles[format].includes(file))
				this.officeFiles[format] = this.officeFiles[format].filter((item) => item !== file);
		}
		this.forgetRemoved();
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
