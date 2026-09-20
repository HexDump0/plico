export const officeTools = {
	'pdf-to-word': { input: 'pdf', output: 'docx' },
	'pdf-to-powerpoint': { input: 'pdf', output: 'pptx' },
	'pdf-to-excel': { input: 'pdf', output: 'xlsx' },
	'word-to-pdf': { input: 'docx', output: 'pdf' },
	'powerpoint-to-pdf': { input: 'pptx', output: 'pdf' },
	'excel-to-pdf': { input: 'xlsx', output: 'pdf' }
} as const;

export type OfficeOperation = keyof typeof officeTools;
export type OfficeInput = 'docx' | 'pptx' | 'xlsx';

export function officeOperation(id: string): OfficeOperation | undefined {
	return id in officeTools ? (id as OfficeOperation) : undefined;
}

export function acceptsFile(file: File, format: 'pdf' | 'image' | OfficeInput): boolean {
	if (format === 'image')
		return /\.(jpe?g|png)$/i.test(file.name) || /^(image\/jpeg|image\/png)$/.test(file.type);
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
	xlsx: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet,.xlsx'
} as const;
