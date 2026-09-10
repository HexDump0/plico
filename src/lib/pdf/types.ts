export type PdfOperation = 'merge';

export type PdfWorkerRequest = {
	id: number;
	operation: PdfOperation;
	files: ArrayBuffer[];
};

export type PdfWorkerResponse =
	{ id: number; ok: true; bytes: ArrayBuffer } | { id: number; ok: false; error: string };
