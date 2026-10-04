import { BLANK_SOURCE, sourceKey } from './sources';
import type {
	AnnotateOptions,
	CompressOptions,
	CropOptions,
	EditOptions,
	FillFormOptions,
	ImagePdfOptions,
	OrganizePage,
	PageGlyphs,
	PageNumberOptions,
	PdfOutput,
	PdfImageOptions,
	PdfWorkerRequest,
	PdfWorkerResponse,
	Protection,
	ProtectOptions,
	RedactOptions,
	SignOptions,
	SplitOptions,
	WatermarkOptions
} from './types';

let worker: Worker | undefined;
let requestId = 0;
type PdfWorkerSuccess = Extract<PdfWorkerResponse, { ok: true }>;
const pending = new Map<
	number,
	{ resolve: (response: PdfWorkerSuccess) => void; reject: (error: Error) => void }
>();

function getWorker() {
	if (worker) return worker;
	worker = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' });
	worker.onmessage = (event: MessageEvent<PdfWorkerResponse>) => {
		const request = pending.get(event.data.id);
		if (!request) return;
		pending.delete(event.data.id);
		if (event.data.ok) request.resolve(event.data);
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

async function submit(request: PdfWorkerRequest, signal?: AbortSignal): Promise<PdfOutput> {
	const response = await send(request, signal);
	if (!('bytes' in response)) throw new Error('The PDF engine returned no file.');
	return { bytes: new Uint8Array(response.bytes), format: response.format };
}

function send(request: PdfWorkerRequest, signal?: AbortSignal) {
	return new Promise<PdfWorkerSuccess>((resolve, reject) => {
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

// Passwords pair with files by index; an empty string means the file opens
// without one. They stay in memory and only travel to the local worker.
// `bookmarks` holds one outline title per file, or nothing for no outline.
export async function processPdfs(
	operation: 'merge',
	files: File[],
	passwords: string[],
	bookmarks: string[],
	signal?: AbortSignal
) {
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const buffers = await Promise.all(files.map((file) => file.arrayBuffer()));
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const output = await submit(
		{ id: ++requestId, operation, files: buffers, passwords, bookmarks },
		signal
	);
	return output.bytes;
}

export async function pdfProtection(file: File, signal?: AbortSignal): Promise<Protection> {
	const buffer = await file.arrayBuffer();
	const response = await send(
		{ id: ++requestId, operation: 'protection', files: [buffer] },
		signal
	);
	const value = 'value' in response ? response.value : -1;
	return value === 2 ? 'password' : value === 1 ? 'restricted' : 'none';
}

export async function processProtectPdf(
	file: File,
	password: string,
	options: ProtectOptions,
	signal?: AbortSignal
) {
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const buffer = await file.arrayBuffer();
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	return submit(
		{ id: ++requestId, operation: 'protect', files: [buffer], passwords: [password], options },
		signal
	);
}

export async function processPageNumbers(
	file: File,
	password: string,
	options: PageNumberOptions,
	signal?: AbortSignal
) {
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const buffer = await file.arrayBuffer();
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	return submit(
		{ id: ++requestId, operation: 'page-numbers', files: [buffer], passwords: [password], options },
		signal
	);
}

export async function processWatermark(
	file: File,
	password: string,
	options: WatermarkOptions,
	image: File | undefined,
	signal?: AbortSignal
) {
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const buffers = await Promise.all(
		[file, ...(image ? [image] : [])].map((item) => item.arrayBuffer())
	);
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	return submit(
		{ id: ++requestId, operation: 'watermark', files: buffers, passwords: [password], options },
		signal
	);
}

/// The flattened PDF, and how many annotations in scope were left as they
/// were because they had nothing stored to draw.
export async function processFlatten(
	file: File,
	password: string,
	formsOnly: boolean,
	signal?: AbortSignal
) {
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const buffer = await file.arrayBuffer();
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const response = await send(
		{ id: ++requestId, operation: 'flatten', files: [buffer], passwords: [password], formsOnly },
		signal
	);
	if (!('bytes' in response)) throw new Error('The PDF engine returned no file.');
	return {
		bytes: new Uint8Array(response.bytes),
		format: 'pdf' as const,
		kept: response.value ?? 0
	};
}

/// The filled PDF, and when flattened, how many fields were left as they were
/// because they had nothing stored to draw.
export async function processFillForm(
	file: File,
	password: string,
	options: FillFormOptions,
	signal?: AbortSignal
) {
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const buffer = await file.arrayBuffer();
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const response = await send(
		{ id: ++requestId, operation: 'fill-form', files: [buffer], passwords: [password], options },
		signal
	);
	if (!('bytes' in response)) throw new Error('The PDF engine returned no file.');
	return {
		bytes: new Uint8Array(response.bytes),
		format: 'pdf' as const,
		kept: response.value ?? 0
	};
}

export async function processSign(
	file: File,
	password: string,
	signature: File,
	options: SignOptions,
	signal?: AbortSignal
) {
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const buffers = await Promise.all([file.arrayBuffer(), signature.arrayBuffer()]);
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	return submit(
		{ id: ++requestId, operation: 'sign', files: buffers, passwords: [password], options },
		signal
	);
}

/// `images` are the pictures image annotations draw, by their `image` index.
export async function processAnnotate(
	file: File,
	password: string,
	options: AnnotateOptions,
	images: File[] = [],
	signal?: AbortSignal
) {
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const buffers = await Promise.all([file, ...images].map((source) => source.arrayBuffer()));
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	return submit(
		{ id: ++requestId, operation: 'annotate', files: buffers, passwords: [password], options },
		signal
	);
}

export async function processCrop(
	file: File,
	password: string,
	options: CropOptions,
	signal?: AbortSignal
) {
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const buffer = await file.arrayBuffer();
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	return submit(
		{ id: ++requestId, operation: 'crop', files: [buffer], passwords: [password], options },
		signal
	);
}

/// The redacted PDF, and the pages drawn from a picture.
export async function processRedact(
	file: File,
	password: string,
	options: RedactOptions,
	signal?: AbortSignal
) {
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const buffer = await file.arrayBuffer();
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const response = await send(
		{ id: ++requestId, operation: 'redact', files: [buffer], passwords: [password], options },
		signal
	);
	if (!('bytes' in response)) throw new Error('The PDF engine returned no file.');
	return {
		bytes: new Uint8Array(response.bytes),
		format: 'pdf' as const,
		pictured: response.pictured ?? []
	};
}

export async function processEdit(
	file: File,
	password: string,
	options: EditOptions,
	images: File[] = [],
	signal?: AbortSignal
) {
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const buffers = await Promise.all([file, ...images].map((source) => source.arrayBuffer()));
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const response = await send(
		{ id: ++requestId, operation: 'edit', files: buffers, passwords: [password], options },
		signal
	);
	if (!('bytes' in response)) throw new Error('The PDF engine returned no file.');
	return {
		bytes: new Uint8Array(response.bytes),
		format: 'pdf' as const,
		covered: response.covered ?? []
	};
}

/// Where every glyph sits and what it reads, page by page.
export async function redactionText(
	file: File,
	password: string,
	signal?: AbortSignal
): Promise<PageGlyphs[]> {
	const buffer = await file.arrayBuffer();
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const response = await send(
		{ id: ++requestId, operation: 'redact-text', files: [buffer], passwords: [password] },
		signal
	);
	if (!('glyphs' in response)) throw new Error('The PDF engine returned no text.');
	return response.glyphs;
}

// Writes a protected PDF back without its encryption, for consumers that
// cannot be trusted to decrypt it themselves.
export async function unlockPdf(file: File, password: string, signal?: AbortSignal) {
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const buffer = await file.arrayBuffer();
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const output = await submit(
		{ id: ++requestId, operation: 'unlock', files: [buffer], passwords: [password] },
		signal
	);
	return output.bytes;
}

export async function convertToPdfA(
	file: File,
	password: string,
	part: 2 | 3,
	signal?: AbortSignal
) {
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const buffer = await file.arrayBuffer();
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const output = await submit(
		{
			id: ++requestId,
			operation: 'pdfa',
			files: [buffer],
			passwords: [password],
			part,
			// Standard font substitutes, fetched by the worker only when a file needs them.
			fontBase: new URL('/pdfa-fonts/', location.href).href
		},
		signal
	);
	return output.bytes;
}

export async function processSplitPdf(
	file: File,
	password: string,
	options: SplitOptions,
	signal?: AbortSignal
) {
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const buffer = await file.arrayBuffer();
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	return pdfOrZip(
		await submit(
			{ id: ++requestId, operation: 'split', files: [buffer], passwords: [password], options },
			signal
		)
	);
}

export async function processOrganizePdf(
	files: File[],
	passwords: string[],
	pages: OrganizePage[],
	signal?: AbortSignal
) {
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const buffers = await Promise.all(files.map((file) => file.arrayBuffer()));
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const sources = new Map(files.map((file, index) => [sourceKey(file), index]));
	// The engine marks a blank page with the largest u32 as its source and
	// reads its size from `blanks` by index.
	const blanks: number[] = [];
	const instructions = pages.map((page) => {
		if (page.source === BLANK_SOURCE && page.size) {
			blanks.push(page.size.width, page.size.height);
			return { source: 0xffffffff, number: blanks.length / 2 - 1, rotation: page.rotation };
		}
		const source = sources.get(page.source);
		if (source === undefined) throw new Error('A page belongs to a PDF that is no longer loaded.');
		return { source, number: page.number, rotation: page.rotation };
	});
	return pdfOrZip(
		await submit(
			{
				id: ++requestId,
				operation: 'organize',
				files: buffers,
				passwords,
				pages: instructions,
				blanks
			},
			signal
		)
	);
}

export async function processCompressPdf(
	files: File[],
	passwords: string[],
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
				passwords,
				names: files.map((file) => file.name),
				options
			},
			signal
		)
	);
}

export async function processImagesToPdf(
	files: File[],
	options: ImagePdfOptions = {
		pageWidth: 595.28,
		pageHeight: 841.89,
		margin: 18,
		orientation: 'auto'
	},
	signal?: AbortSignal
) {
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const buffers = await Promise.all(files.map((file) => file.arrayBuffer()));
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	return submit({ id: ++requestId, operation: 'images-to-pdf', files: buffers, options }, signal);
}

export async function processPdfToImages(
	file: File,
	password: string,
	options: PdfImageOptions,
	signal?: AbortSignal
) {
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const buffer = await file.arrayBuffer();
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	return submit(
		{
			id: ++requestId,
			operation: 'pdf-to-images',
			files: [buffer],
			passwords: [password],
			options
		},
		signal
	);
}
