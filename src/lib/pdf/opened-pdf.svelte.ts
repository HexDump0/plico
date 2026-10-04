import type { PDFDocumentLoadingTask, PDFDocumentProxy } from 'pdfjs-dist';
import { getWorkspace } from '$lib/workspace.svelte';

export type PageSize = { width: number; height: number };

/// A PDF opened with pdf.js for as long as the component that made it, and
/// each page's size in points as the reader sees it. Pages not yet measured
/// borrow the first page's. A locked file waits for its password in the
/// workspace, as every preview does.
export class OpenedPdf {
	pdf = $state.raw<PDFDocumentProxy | null>(null);
	sizes = $state.raw<PageSize[]>([]);
	status = $state('');

	constructor(file: () => File | undefined, onload: (pdf: PDFDocumentProxy) => void) {
		const workspace = getWorkspace();
		$effect(() => {
			const source = file();
			const secret = source ? workspace.passwordFor(source) : '';
			let cancelled = false;
			let loadingTask: PDFDocumentLoadingTask | undefined;
			this.pdf = null;
			this.sizes = [];
			this.status = source ? 'Loading PDF...' : '';
			if (!source) return;
			const load = async () => {
				try {
					const pdfjs = await import('pdfjs-dist');
					const worker = await import('pdfjs-dist/build/pdf.worker.min.mjs?url');
					if (cancelled) return;
					pdfjs.GlobalWorkerOptions.workerSrc = worker.default;
					const data = new Uint8Array(await source.arrayBuffer());
					if (cancelled) return;
					loadingTask = pdfjs.getDocument({
						data,
						password: secret || undefined,
						cMapUrl: '/pdfjs/cmaps/',
						cMapPacked: true,
						standardFontDataUrl: '/pdfjs/standard_fonts/'
					});
					loadingTask.onPassword = () => {
						if (!cancelled) {
							this.status = '';
							workspace.setLock(source, secret ? 'incorrect' : 'locked');
						}
						void loadingTask?.destroy();
					};
					const document = await loadingTask.promise;
					if (cancelled) return;
					const first = (await document.getPage(1)).getViewport({ scale: 1 });
					if (cancelled) return;
					workspace.opened(source);
					const measured = Array.from({ length: document.numPages }, () => ({
						width: first.width,
						height: first.height
					}));
					this.sizes = measured;
					this.pdf = document;
					this.status = '';
					onload(document);
					for (let number = 2; number <= document.numPages; number++) {
						const { width, height } = (await document.getPage(number)).getViewport({ scale: 1 });
						if (cancelled) return;
						measured[number - 1] = { width, height };
						if (number % 25 === 0 || number === document.numPages) this.sizes = [...measured];
					}
				} catch {
					if (!cancelled && !workspace.lockState(source)) this.status = 'Preview unavailable';
				}
			};
			void load();
			return () => {
				cancelled = true;
				void loadingTask?.destroy();
			};
		});
	}
}
