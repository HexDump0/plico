import { BLANK_SOURCE, sourceKey } from './sources';
import type {
	AnnotateOptions,
	CompressOptions,
	CropOptions,
	EditOptions,
	FillFormOptions,
	OcrText,
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
	ScanLook,
	ScanPage,
	ScanPaper,
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

/// A page drawn for recognition. `file` opens `document` in the worker and is
/// left out for the pages after, which read the copy already open.
export async function ocrPageImage(
	file: File | null,
	password: string,
	document: number,
	page: number,
	signal?: AbortSignal
) {
	const files = file ? [await file.arrayBuffer()] : [];
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const response = await send(
		{ id: ++requestId, operation: 'ocr-render', files, passwords: [password], document, page },
		signal
	);
	if (!('image' in response)) throw new Error('The page could not be drawn.');
	return {
		image: new Uint8Array(response.image),
		width: response.width,
		height: response.height,
		dpi: response.dpi
	};
}

/// Lets the worker drop the PDF it kept open for recognition.
export function closeOcrDocument(document: number) {
	if (!worker) return;
	void send({ id: ++requestId, operation: 'ocr-close', files: [], document }).catch(() => {});
}

/// The PDF with `words` written under its pages as invisible text.
export async function processOcr(
	file: File,
	password: string,
	words: OcrText[],
	signal?: AbortSignal
) {
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const buffer = await file.arrayBuffer();
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	return submit(
		{ id: ++requestId, operation: 'ocr', files: [buffer], passwords: [password], words },
		signal
	);
}

/// A photo opened for scanning: a smaller upright copy to show, the photo's
/// upright size, and the page found in it, if any.
export async function openScanPhoto(file: File) {
	const response = await send({
		id: ++requestId,
		operation: 'scan-open',
		files: [await file.arrayBuffer()]
	});
	if (!('proxy' in response)) throw new Error('This photo could not be read.');
	return {
		proxy: new Blob([response.proxy], { type: 'image/jpeg' }),
		width: response.width,
		height: response.height,
		corners: response.corners.length === 8 ? response.corners : null
	};
}

/// The page in a camera frame, or `null`.
export async function findScanPage(frame: ImageData) {
	const response = await send({
		id: ++requestId,
		operation: 'scan-find',
		files: [frame.data.buffer as ArrayBuffer],
		width: frame.width,
		height: frame.height
	});
	return 'corners' in response && response.corners.length === 8 ? response.corners : null;
}

/// One photo straightened, cleaned up and encoded: a JPEG, or a PNG for
/// black and white.
export async function scanPhoto(
	photo: Blob,
	page: ScanPage,
	look: ScanLook,
	side: number,
	signal?: AbortSignal
) {
	const buffer = await photo.arrayBuffer();
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	// A plain copy: a reactive proxy cannot be posted to the worker.
	const plain = { corners: [...page.corners], turns: page.turns };
	return submit(
		{ id: ++requestId, operation: 'scan-page', files: [buffer], page: plain, look, side },
		signal
	);
}

/// Scanned pages, in order, written as one PDF.
export async function processScan(images: Uint8Array[], paper: ScanPaper, signal?: AbortSignal) {
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const files = images.map((image) => image.slice().buffer);
	return submit({ id: ++requestId, operation: 'scan', files, paper: { ...paper } }, signal);
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
