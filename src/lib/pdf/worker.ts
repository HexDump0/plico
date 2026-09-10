/// <reference lib="webworker" />

import init, { merge_pdfs } from './wasm/plico_engine.js';
import type { PdfWorkerRequest, PdfWorkerResponse } from './types';

const ready = init();

self.onmessage = async (event: MessageEvent<PdfWorkerRequest>) => {
	const { id, operation, files } = event.data;
	try {
		await ready;
		if (operation !== 'merge') throw new Error('This PDF operation is not available yet.');

		const lengths = Uint32Array.from(files, (file) => file.byteLength);
		const input = new Uint8Array(lengths.reduce((total, length) => total + length, 0));
		let offset = 0;
		for (const file of files) {
			input.set(new Uint8Array(file), offset);
			offset += file.byteLength;
		}

		const output = merge_pdfs(input, lengths).slice().buffer;
		const response: PdfWorkerResponse = { id, ok: true, bytes: output };
		self.postMessage(response, { transfer: [output] });
	} catch (error) {
		const response: PdfWorkerResponse = {
			id,
			ok: false,
			error: error instanceof Error ? error.message : String(error)
		};
		self.postMessage(response);
	}
};
