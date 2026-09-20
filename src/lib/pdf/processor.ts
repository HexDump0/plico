import type {
	CompressOptions,
	ImagePdfOptions,
	OrganizePage,
	PdfOutput,
	PdfImageOptions,
	PdfWorkerRequest,
	PdfWorkerResponse,
	SplitOptions
} from './types';

let worker: Worker | undefined;
let requestId = 0;
const pending = new Map<
	number,
	{ resolve: (output: PdfOutput) => void; reject: (error: Error) => void }
>();

function getWorker() {
	if (worker) return worker;
	worker = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' });
	worker.onmessage = (event: MessageEvent<PdfWorkerResponse>) => {
		const request = pending.get(event.data.id);
		if (!request) return;
		pending.delete(event.data.id);
		if (event.data.ok)
			request.resolve({ bytes: new Uint8Array(event.data.bytes), format: event.data.format });
		else request.reject(new Error(event.data.error));
	};
	worker.onerror = () => stopWorker('The local PDF engine stopped unexpectedly.');
	return worker;
}

function stopWorker(message: string) {
	worker?.terminate();
	worker = undefined;
	for (const request of pending.values()) request.reject(new Error(message));
	pending.clear();
}

function submit(request: PdfWorkerRequest, signal?: AbortSignal) {
	return new Promise<PdfOutput>((resolve, reject) => {
		const abort = () => {
			stopWorker('The operation was cancelled.');
			reject(new DOMException('The operation was cancelled.', 'AbortError'));
		};
		signal?.addEventListener('abort', abort, { once: true });
		pending.set(request.id, {
			resolve: (output) => {
				signal?.removeEventListener('abort', abort);
				resolve(output);
			},
			reject: (error) => {
				signal?.removeEventListener('abort', abort);
				reject(error);
			}
		});
		getWorker().postMessage(request, { transfer: request.files });
	});
}

function pdfOrZip(output: PdfOutput): PdfOutput & { format: 'pdf' | 'zip' } {
	if (output.format !== 'pdf' && output.format !== 'zip') {
		throw new Error('The PDF engine returned an unexpected file format.');
	}
	return output as PdfOutput & { format: 'pdf' | 'zip' };
}

export async function processPdfs(operation: 'merge', files: File[], signal?: AbortSignal) {
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const buffers = await Promise.all(files.map((file) => file.arrayBuffer()));
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const output = await submit({ id: ++requestId, operation, files: buffers }, signal);
	return output.bytes;
}

export async function processSplitPdf(file: File, options: SplitOptions, signal?: AbortSignal) {
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const buffer = await file.arrayBuffer();
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	return pdfOrZip(
		await submit({ id: ++requestId, operation: 'split', files: [buffer], options }, signal)
	);
}

export async function processOrganizePdf(file: File, pages: OrganizePage[], signal?: AbortSignal) {
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const buffer = await file.arrayBuffer();
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	return pdfOrZip(
		await submit({ id: ++requestId, operation: 'organize', files: [buffer], pages }, signal)
	);
}

export async function processCompressPdf(
	files: File[],
	options: CompressOptions,
	signal?: AbortSignal
) {
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const buffers = await Promise.all(files.map((file) => file.arrayBuffer()));
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	return pdfOrZip(
		await submit(
			{
				id: ++requestId,
				operation: 'compress',
				files: buffers,
				names: files.map((file) => file.name),
				options
			},
			signal
		)
	);
}

export async function processImagesToPdf(
	files: File[],
	options: ImagePdfOptions = { pageWidth: 595.28, pageHeight: 841.89, margin: 18 },
	signal?: AbortSignal
) {
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const buffers = await Promise.all(files.map((file) => file.arrayBuffer()));
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	return submit({ id: ++requestId, operation: 'images-to-pdf', files: buffers, options }, signal);
}

export async function processPdfToImages(
	file: File,
	options: PdfImageOptions,
	signal?: AbortSignal
) {
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const buffer = await file.arrayBuffer();
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	return submit({ id: ++requestId, operation: 'pdf-to-images', files: [buffer], options }, signal);
}
