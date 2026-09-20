/// <reference lib="webworker" />

import { zipSync } from 'fflate';
import init, { merge_pdfs, split_pdf_every, split_pdf_ranges } from './wasm/plico_engine.js';
import type { PdfWorkerRequest, PdfWorkerResponse } from './types';

const ready = init();

self.onmessage = async (event: MessageEvent<PdfWorkerRequest>) => {
	const { id, operation, files } = event.data;
	try {
		await ready;
		if (operation === 'merge') {
			const lengths = Uint32Array.from(files, (file) => file.byteLength);
			const input = new Uint8Array(lengths.reduce((total, length) => total + length, 0));
			let offset = 0;
			for (const file of files) {
				input.set(new Uint8Array(file), offset);
				offset += file.byteLength;
			}

			const output = merge_pdfs(input, lengths).slice().buffer;
			const response: PdfWorkerResponse = { id, ok: true, bytes: output, format: 'pdf' };
			self.postMessage(response, { transfer: [output] });
			return;
		}

		if (files.length !== 1) throw new Error('Choose one PDF to split.');
		const input = new Uint8Array(files[0]);
		const { options } = event.data;
		const parts =
			options.mode === 'ranges'
				? (split_pdf_ranges(
						input,
						Uint32Array.from(options.ranges.flatMap(({ from, to }) => [from, to])),
						options.combine
					) as Uint8Array[])
				: (split_pdf_every(input, options.interval) as Uint8Array[]);
		if (parts.length === 0) throw new Error('No PDF pages were produced.');

		const entries: Record<string, [Uint8Array, { level: 0 }]> = {};
		if (parts.length > 1) {
			const digits = Math.max(2, String(parts.length).length);
			for (const [index, bytes] of parts.entries()) {
				const prefix = `part-${String(index + 1).padStart(digits, '0')}`;
				const name =
					options.mode === 'ranges'
						? `${prefix}-pages-${options.ranges[index].from}-${options.ranges[index].to}.pdf`
						: `${prefix}.pdf`;
				entries[name] = [bytes, { level: 0 }];
			}
		}
		const format = parts.length === 1 ? 'pdf' : 'zip';
		const bytes = parts.length === 1 ? parts[0] : zipSync(entries);
		const output = bytes.slice().buffer;
		const response: PdfWorkerResponse = { id, ok: true, bytes: output, format };
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
