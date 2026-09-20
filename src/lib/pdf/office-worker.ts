/// <reference lib="webworker" />

import type { OfficeOperation } from './office-conversion';

type Request = { id: number; operation: OfficeOperation; input: ArrayBuffer };
type Response =
	{ id: number; ok: true; bytes: ArrayBuffer } | { id: number; ok: false; error: string };

self.onmessage = async (event: MessageEvent<Request>) => {
	const { id, operation, input } = event.data;
	try {
		// This import keeps the large Office engine out of the ordinary PDF worker.
		const { default: init, WasmPdfDocument } = await import('pdf-oxide-wasm/web');
		await init();
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
							: document.saveToBytes();
			const output = bytes.slice().buffer;
			const response: Response = { id, ok: true, bytes: output };
			self.postMessage(response, { transfer: [output] });
		} finally {
			document.free();
		}
	} catch (cause) {
		const response: Response = {
			id,
			ok: false,
			error: cause instanceof Error ? cause.message : String(cause)
		};
		self.postMessage(response);
	}
};
