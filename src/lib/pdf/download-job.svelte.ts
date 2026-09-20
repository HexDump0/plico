import { tick } from 'svelte';
import type { PdfOutput } from './types';

const mimeTypes: Record<PdfOutput['format'], string> = {
	pdf: 'application/pdf',
	zip: 'application/zip',
	jpg: 'image/jpeg',
	png: 'image/png'
};

export class DownloadJob {
	processing = $state(false);
	error = $state('');
	result = $state('');
	format = $state<PdfOutput['format']>('pdf');
	size = $state(0);
	inputSize = $state(0);
	private controller: AbortController | undefined;

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
			const output = await process(controller.signal);
			if (controller.signal.aborted) return;
			const url = URL.createObjectURL(
				new Blob([output.bytes.slice().buffer], { type: mimeTypes[output.format] })
			);
			this.processing = false;
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
				this.controller = undefined;
			}
		}
	}
}
