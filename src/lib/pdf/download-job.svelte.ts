import { tick } from 'svelte';
import { isDamagedPdfError } from './repair';
import type { PdfOutput } from './types';

const mimeTypes: Record<PdfOutput['format'], string> = {
	pdf: 'application/pdf',
	zip: 'application/zip',
	jpg: 'image/jpeg',
	png: 'image/png',
	docx: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
	pptx: 'application/vnd.openxmlformats-officedocument.presentationml.presentation',
	xlsx: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
	md: 'text/markdown;charset=utf-8'
};

export class DownloadJob {
	processing = $state(false);
	error = $state('');
	result = $state('');
	format = $state<PdfOutput['format']>('pdf');
	size = $state(0);
	inputSize = $state(0);
	blob: Blob | undefined;
	// Set while the files a run could not open are being repaired before it
	// runs again.
	repairing = $state(false);
	/// Repairs the files a run could not open and says whether any changed.
	recover: ((signal: AbortSignal) => Promise<boolean>) | undefined;
	private controller: AbortController | undefined;

	clear() {
		this.controller?.abort();
		this.controller = undefined;
		this.processing = false;
		this.repairing = false;
		this.error = '';
		if (this.result) URL.revokeObjectURL(this.result);
		this.result = '';
		this.blob = undefined;
		this.format = 'pdf';
		this.size = 0;
		this.inputSize = 0;
	}

	async run(
		process: (signal: AbortSignal) => Promise<PdfOutput>,
		fallbackError: string,
		download: () => void,
		inputSize = 0
	) {
		if (this.processing) return;
		this.clear();
		const controller = new AbortController();
		this.controller = controller;
		this.processing = true;
		try {
			const output = await process(controller.signal).catch(async (cause) => {
				if (!this.recover || !isDamagedPdfError(cause) || controller.signal.aborted) throw cause;
				this.repairing = true;
				const repaired = await this.recover(controller.signal);
				this.repairing = false;
				if (!repaired) throw cause;
				return process(controller.signal);
			});
			if (controller.signal.aborted) return;
			const blob = new Blob([output.bytes.slice().buffer], { type: mimeTypes[output.format] });
			const url = URL.createObjectURL(blob);
			this.processing = false;
			this.blob = blob;
			this.format = output.format;
			this.size = output.bytes.byteLength;
			this.inputSize = inputSize;
			this.result = url;
			await tick();
			if (!controller.signal.aborted && this.result === url) download();
		} catch (cause) {
			if (!controller.signal.aborted)
				this.error = cause instanceof Error ? cause.message : fallbackError;
		} finally {
			if (this.controller === controller) {
				this.processing = false;
				this.repairing = false;
				this.controller = undefined;
			}
		}
	}
}
