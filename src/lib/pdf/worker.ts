/// <reference lib="webworker" />

import { zipSync } from 'fflate';
import init, {
	compress_pdf,
	images_to_pdf,
	merge_pdfs,
	split_pdf_every,
	split_pdf_ranges
} from './wasm/plico_engine.js';
import type { PdfImageOptions, PdfOutput, PdfWorkerRequest, PdfWorkerResponse } from './types';

const ready = init();

type PackedOutput = PdfOutput;

function packageOutputs(
	parts: Uint8Array[],
	nameFor: (index: number) => string,
	singleFormat: Exclude<PdfOutput['format'], 'zip'> = 'pdf'
): PackedOutput {
	if (parts.length === 1) return { format: singleFormat, bytes: parts[0] };
	const entries: Record<string, [Uint8Array, { level: 0 }]> = {};
	parts.forEach((bytes, index) => {
		entries[nameFor(index)] = [bytes, { level: 0 }];
	});
	return { format: 'zip', bytes: zipSync(entries) };
}

async function exportPdfImages(
	input: ArrayBuffer,
	options: PdfImageOptions
): Promise<PackedOutput> {
	if (!Number.isFinite(options.dpi) || options.dpi < 36 || options.dpi > 300) {
		throw new Error('Choose a resolution between 36 and 300 DPI.');
	}
	if (!Number.isFinite(options.quality) || options.quality < 1 || options.quality > 100) {
		throw new Error('Choose a JPG quality between 1 and 100.');
	}
	if (options.format !== 'jpg' && options.format !== 'png') {
		throw new Error('Choose JPG or PNG output.');
	}
	if (typeof OffscreenCanvas === 'undefined' || !OffscreenCanvas.prototype.convertToBlob) {
		throw new Error('This browser cannot export PDF pages to images.');
	}

	const pdfjs = await import('pdfjs-dist');
	const workerUrl = await import('pdfjs-dist/build/pdf.worker.min.mjs?url');
	pdfjs.GlobalWorkerOptions.workerSrc = workerUrl.default;
	const task = pdfjs.getDocument({ data: new Uint8Array(input) });
	try {
		const pdf = await task.promise;
		const pages = options.pages ?? Array.from({ length: pdf.numPages }, (_, index) => index + 1);
		if (
			pages.length === 0 ||
			new Set(pages).size !== pages.length ||
			pages.some((page) => !Number.isInteger(page) || page < 1 || page > pdf.numPages)
		) {
			throw new Error(`Choose unique pages between 1 and ${pdf.numPages}.`);
		}

		const mime = options.format === 'jpg' ? 'image/jpeg' : 'image/png';
		const parts: Uint8Array[] = [];
		for (const number of pages) {
			const page = await pdf.getPage(number);
			const viewport = page.getViewport({ scale: options.dpi / 72 });
			const width = Math.ceil(viewport.width);
			const height = Math.ceil(viewport.height);
			if (width <= 0 || height <= 0 || width * height > 40_000_000) {
				throw new Error(`Page ${number} is too large at this resolution. Choose a lower DPI.`);
			}
			const canvas = new OffscreenCanvas(width, height);
			// PDF.js accepts OffscreenCanvas at runtime, but its public type only lists HTMLCanvasElement.
			await page.render({
				canvas: canvas as unknown as HTMLCanvasElement,
				viewport,
				background: 'rgb(255,255,255)'
			}).promise;
			const blob = await canvas.convertToBlob({
				type: mime,
				quality: options.quality / 100
			});
			if (blob.type !== mime)
				throw new Error(`${options.format.toUpperCase()} export is unsupported in this browser.`);
			parts.push(new Uint8Array(await blob.arrayBuffer()));
			page.cleanup();
		}
		return packageOutputs(
			parts,
			(index) =>
				`page-${String(pages[index]).padStart(Math.max(2, String(pdf.numPages).length), '0')}.${options.format}`,
			options.format
		);
	} finally {
		await task.destroy();
	}
}

self.onmessage = async (event: MessageEvent<PdfWorkerRequest>) => {
	const request = event.data;
	const { id } = request;
	try {
		await ready;
		if (request.operation === 'images-to-pdf') {
			const { files, options } = request;
			const lengths = Uint32Array.from(files, (file) => file.byteLength);
			const input = new Uint8Array(lengths.reduce((total, length) => total + length, 0));
			let offset = 0;
			for (const file of files) {
				input.set(new Uint8Array(file), offset);
				offset += file.byteLength;
			}
			postOutput(id, {
				format: 'pdf',
				bytes: images_to_pdf(input, lengths, options.pageWidth, options.pageHeight, options.margin)
			});
			return;
		}

		if (request.operation === 'pdf-to-images') {
			if (request.files.length !== 1) throw new Error('Choose one PDF to convert.');
			postOutput(id, await exportPdfImages(request.files[0], request.options));
			return;
		}
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
