/// <reference lib="webworker" />

import { zipSync } from 'fflate';
import init, {
	compress_pdf,
	merge_pdfs,
	split_pdf_every,
	split_pdf_ranges
} from './wasm/plico_engine.js';
import type { PdfWorkerRequest, PdfWorkerResponse } from './types';

const ready = init();

type PackedOutput = { format: 'pdf' | 'zip'; bytes: Uint8Array };

function packageOutputs(parts: Uint8Array[], nameFor: (index: number) => string): PackedOutput {
	if (parts.length === 1) return { format: 'pdf', bytes: parts[0] };
	const entries: Record<string, [Uint8Array, { level: 0 }]> = {};
	parts.forEach((bytes, index) => {
		entries[nameFor(index)] = [bytes, { level: 0 }];
	});
	return { format: 'zip', bytes: zipSync(entries) };
}

self.onmessage = async (event: MessageEvent<PdfWorkerRequest>) => {
	const request = event.data;
	const { id } = request;
	try {
		await ready;
		if (request.operation === 'merge') {
			const { files } = request;
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

		if (request.operation === 'split') {
			if (request.files.length !== 1) throw new Error('Choose one PDF to split.');
			const input = new Uint8Array(request.files[0]);
			const { options } = request;
			const parts =
				options.mode === 'ranges'
					? (split_pdf_ranges(
							input,
							Uint32Array.from(options.ranges.flatMap(({ from, to }) => [from, to])),
							options.combine
						) as Uint8Array[])
					: (split_pdf_every(input, options.interval) as Uint8Array[]);
			if (parts.length === 0) throw new Error('No PDF pages were produced.');

			const packed = packageOutputs(parts, (index) => {
				const digits = Math.max(2, String(parts.length).length);
				const prefix = `part-${String(index + 1).padStart(digits, '0')}`;
				return options.mode === 'ranges'
					? `${prefix}-pages-${options.ranges[index].from}-${options.ranges[index].to}.pdf`
					: `${prefix}.pdf`;
			});
			postOutput(id, packed);
			return;
		}

		const { files, options } = request;
		const parts = files.map((file) =>
			compress_pdf(
				new Uint8Array(file),
				options.imageQuality,
				options.maxImageDimension,
				options.removeMetadata,
				options.removeThumbnails
			)
		);
		if (parts.length === 0) throw new Error('No PDF was produced.');

		const packed = packageOutputs(parts, (index) => {
			const digits = Math.max(2, String(parts.length).length);
			const base =
				request.names[index]
					?.replace(/\.pdf$/i, '')
					.replace(/[^\p{L}\p{N}._ -]/gu, '_')
					.slice(0, 100) || 'document';
			return `part-${String(index + 1).padStart(digits, '0')}-${base}-compressed.pdf`;
		});
		postOutput(id, packed);
	} catch (error) {
		const response: PdfWorkerResponse = {
			id,
			ok: false,
			error: error instanceof Error ? error.message : String(error)
		};
		self.postMessage(response);
	}
};

function postOutput(id: number, packed: PackedOutput) {
	const output = packed.bytes.slice().buffer;
	const response: PdfWorkerResponse = { id, ok: true, bytes: output, format: packed.format };
	self.postMessage(response, { transfer: [output] });
}
