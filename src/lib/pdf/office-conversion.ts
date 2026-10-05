export const officeTools = {
	'pdf-to-word': { input: 'pdf', output: 'docx' },
	'pdf-to-powerpoint': { input: 'pdf', output: 'pptx' },
	'pdf-to-excel': { input: 'pdf', output: 'xlsx' },
	'pdf-to-markdown': { input: 'pdf', output: 'md' },
	'word-to-pdf': { input: 'docx', output: 'pdf' },
	'powerpoint-to-pdf': { input: 'pptx', output: 'pdf' },
	'excel-to-pdf': { input: 'xlsx', output: 'pdf' }
} as const;

export type OfficeOperation = keyof typeof officeTools;
export type OfficeInput = 'docx' | 'pptx' | 'xlsx';

// Laid out by the browser and drawn by the engine (`html-print.ts`), not
// converted by the Office engine.
export const printTools = {
	'html-to-pdf': { input: 'html', output: 'pdf' },
	'markdown-to-pdf': { input: 'md', output: 'pdf' }
} as const;

export type PrintOperation = keyof typeof printTools;
export type DocumentInput = OfficeInput | 'html' | 'md';

// `pages` counts from 1; empty means every page.
export type MarkdownOptions = { pages: number[]; images: boolean };

export function officeOperation(id: string): OfficeOperation | undefined {
	return id in officeTools ? (id as OfficeOperation) : undefined;
}

export function printOperation(id: string): PrintOperation | undefined {
	return id in printTools ? (id as PrintOperation) : undefined;
}

/// The file a tool takes, when it is not a PDF.
export function toolInput(id: string): 'pdf' | 'image' | DocumentInput | undefined {
	const office = officeOperation(id);
	if (office) return officeTools[office].input;
	const print = printOperation(id);
	return print ? printTools[print].input : undefined;
}

/// How a file type is named in copy.
export function formatName(format: 'pdf' | 'image' | DocumentInput) {
	return format === 'md' ? 'Markdown' : format === 'image' ? 'image' : format.toUpperCase();
}

export function acceptsFile(file: File, format: 'pdf' | 'image' | DocumentInput): boolean {
	if (format === 'image')
		return /\.(jpe?g|png)$/i.test(file.name) || /^(image\/jpeg|image\/png)$/.test(file.type);
	if (format === 'html') return /\.x?html?$/i.test(file.name) || file.type === 'text/html';
	if (format === 'md')
		return /\.(md|markdown|mdown|mkd)$/i.test(file.name) || file.type === 'text/markdown';
	return (
		file.name.toLowerCase().endsWith(`.${format}`) ||
		(format === 'pdf' && file.type === 'application/pdf')
	);
}

export const inputAccept = {
	pdf: 'application/pdf,.pdf',
	image: 'image/jpeg,image/png,.jpg,.jpeg,.png',
	docx: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document,.docx',
	pptx: 'application/vnd.openxmlformats-officedocument.presentationml.presentation,.pptx',
	xlsx: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet,.xlsx',
	html: 'text/html,.html,.htm,.xhtml',
	md: 'text/markdown,.md,.markdown,.mdown,.mkd'
} as const;
