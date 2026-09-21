import type { PdfOutput } from './types';

const mimeTypes: Record<PdfOutput['format'], string> = {
	pdf: 'application/pdf',
	zip: 'application/zip',
	jpg: 'image/jpeg',
	png: 'image/png',
	docx: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
	pptx: 'application/vnd.openxmlformats-officedocument.presentationml.presentation',
	xlsx: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet'
};

export class DownloadJob {
	processing = $state(false);
	error = $state('');
	result = $state('');
	format = $state<PdfOutput['format']>('pdf');
	size = $state(0);
	inputSize = $state(0);
	private controller: AbortController | undefined;
	private blob = $state<Blob | undefined>(undefined);

	// The produced file itself, kept so a result can become the next tool's
	// input instead of living only as a revocable object URL.
	get resultBlob() {
		return this.blob;
	}

	clear() {
		this.controller?.abort();
		this.controller = undefined;
		this.processing = false;
		this.error = '';
		if (this.result) URL.revokeObjectURL(this.result);
		this.result = '';
		this.format = 'pdf';
		this.size = 0;
		this.inputSize = 0;
		this.blob = undefined;
	}

	async run(
		process: (signal: AbortSignal) => Promise<PdfOutput>,
		fallbackError: string,
		inputSize = 0
	) {
		if (this.processing) return;
		this.clear();
		const controller = new AbortController();
		this.controller = controller;
		this.processing = true;
		try {
			const output = await process(controller.signal);
			if (controller.signal.aborted) return;
			const blob = new Blob([output.bytes.slice().buffer], { type: mimeTypes[output.format] });
			this.blob = blob;
			const url = URL.createObjectURL(blob);
			this.processing = false;
			this.format = output.format;
			this.size = output.bytes.byteLength;
			this.inputSize = inputSize;
			this.result = url;
		} catch (cause) {
			if (!controller.signal.aborted)
				this.error = cause instanceof Error ? cause.message : fallbackError;
		} finally {
			if (this.controller === controller) {
				this.processing = false;
				this.controller = undefined;
			}
		}
	}
}
