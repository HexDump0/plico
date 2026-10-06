/// <reference lib="webworker" />

import type { WasmPdfDocument } from 'pdf-oxide-wasm/web';
import { offlineReason } from '$lib/offline-files';
import type { MarkdownOptions, OfficeOperation } from './office-conversion';

type Request = {
	id: number;
	operation: OfficeOperation;
	input: ArrayBuffer;
	markdown?: MarkdownOptions;
};
type Response =
	| { id: number; ready: true }
	| { id: number; ok: true; bytes: ArrayBuffer }
	| { id: number; ok: false; error: string };

function toMarkdown(document: WasmPdfDocument, { pages, images }: MarkdownOptions) {
	const numbers = pages.length
		? pages
		: Array.from({ length: document.pageCount() }, (_, index) => index + 1);
	// Page by page because toMarkdownAll puts each page break straight after
	// the page's last line, which Markdown reads as a heading underline
	// (paragraph_and_link.pdf in the pdf.js corpus).
	const text = numbers
		.map((number) =>
			document
				.toMarkdown(number - 1, true, images, true)
				.replace(/^\n+/, '')
				.trimEnd()
		)
		.filter(Boolean)
		.join('\n\n---\n\n')
		// pdf-oxide bolds word by word (`**and** **Subject**`); join adjacent runs.
		.replace(/(?<!\*)\*\*([ \t]+)\*\*(?!\*)/g, '$1');
	if (!text) throw new Error('No text found in this PDF. Scanned pages need OCR PDF first.');
	return new TextEncoder().encode(`${text}\n`);
}

self.onmessage = async (event: MessageEvent<Request>) => {
	const { id, operation, input, markdown } = event.data;
	try {
		// This import keeps the large Office engine out of the ordinary PDF worker.
		const { default: init, WasmPdfDocument } = await import('pdf-oxide-wasm/web');
		await init().catch((cause) => {
			throw new Error(offlineReason('The Office converter') ?? String(cause), { cause });
		});
		self.postMessage({ id, ready: true } satisfies Response);
		const data = new Uint8Array(input);
		const document =
			operation === 'word-to-pdf'
				? WasmPdfDocument.openFromDocxBytes(data)
				: operation === 'powerpoint-to-pdf'
					? WasmPdfDocument.openFromPptxBytes(data)
					: operation === 'excel-to-pdf'
						? WasmPdfDocument.openFromXlsxBytes(data)
						: new WasmPdfDocument(data);
		try {
			const bytes =
				operation === 'pdf-to-word'
					? document.toDocxBytes()
					: operation === 'pdf-to-powerpoint'
						? document.toPptxBytes()
						: operation === 'pdf-to-excel'
							? document.toXlsxBytes()
							: operation === 'pdf-to-markdown'
								? toMarkdown(document, markdown ?? { pages: [], images: false })
								: document.saveToBytes();
			const output = bytes.slice().buffer;
			const response: Response = { id, ok: true, bytes: output };
			self.postMessage(response, { transfer: [output] });
		} finally {
			// After a panic the document is still marked borrowed and free()
			// throws, which would hide the real error. The worker is discarded
			// after every request anyway.
			try {
				document.free();
			} catch {
				// Nothing to recover.
			}
		}
	} catch (cause) {
		const response: Response = {
			id,
			ok: false,
			// A panic inside pdf-oxide surfaces as a bare "unreachable" trap
			// (issue19517.pdf in the pdf.js corpus).
			error:
				cause instanceof WebAssembly.RuntimeError
					? 'The converter could not read this file.'
					: cause instanceof Error
						? cause.message
						: String(cause)
		};
		self.postMessage(response);
	}
};
