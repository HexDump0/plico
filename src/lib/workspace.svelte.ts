import { createContext } from 'svelte';
import { acceptsFile, type OfficeInput } from './pdf/office-conversion';

export type AcceptedFileType = 'pdf' | 'image' | OfficeInput;

export class Workspace {
	private pdfFiles = $state<File[]>([]);
	private imageFiles = $state<File[]>([]);
	private officeFiles = $state<Record<OfficeInput, File[]>>({ docx: [], pptx: [], xlsx: [] });
	private pdfError = $state('');
	private imageError = $state('');
	private officeErrors = $state<Record<OfficeInput, string>>({ docx: '', pptx: '', xlsx: '' });
	private activeType = $state<AcceptedFileType>('pdf');
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
				: `Please choose ${accept === 'image' ? 'JPG or PNG images' : accept === 'pdf' ? 'PDFs' : `${accept.toUpperCase()} files`}. Other file types were skipped.`;
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
		if (this.pdfFiles.includes(file)) this.pdfFiles = this.pdfFiles.filter((item) => item !== file);
		if (this.imageFiles.includes(file))
			this.imageFiles = this.imageFiles.filter((item) => item !== file);
		for (const format of ['docx', 'pptx', 'xlsx'] as const) {
			if (this.officeFiles[format].includes(file))
				this.officeFiles[format] = this.officeFiles[format].filter((item) => item !== file);
		}
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
