import type { PdfOperation, PdfWorkerRequest, PdfWorkerResponse } from './types';

let worker: Worker | undefined;
let requestId = 0;
const pending = new Map<
	number,
	{ resolve: (bytes: Uint8Array) => void; reject: (error: Error) => void }
>();

function getWorker() {
	if (worker) return worker;
	worker = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' });
	worker.onmessage = (event: MessageEvent<PdfWorkerResponse>) => {
		const request = pending.get(event.data.id);
		if (!request) return;
		pending.delete(event.data.id);
		if (event.data.ok) request.resolve(new Uint8Array(event.data.bytes));
		else request.reject(new Error(event.data.error));
	};
	worker.onerror = () => stopWorker('The local PDF engine stopped unexpectedly.');
	return worker;
}

function stopWorker(message: string) {
	worker?.terminate();
	worker = undefined;
	for (const request of pending.values()) request.reject(new Error(message));
	pending.clear();
}

export async function processPdfs(operation: PdfOperation, files: File[], signal?: AbortSignal) {
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const buffers = await Promise.all(files.map((file) => file.arrayBuffer()));
	const id = ++requestId;
	const request: PdfWorkerRequest = { id, operation, files: buffers };

	return new Promise<Uint8Array>((resolve, reject) => {
		const abort = () => {
			stopWorker('The operation was cancelled.');
			reject(new DOMException('The operation was cancelled.', 'AbortError'));
		};
		signal?.addEventListener('abort', abort, { once: true });
		pending.set(id, {
			resolve: (bytes) => {
				signal?.removeEventListener('abort', abort);
				resolve(bytes);
			},
			reject: (error) => {
				signal?.removeEventListener('abort', abort);
				reject(error);
			}
		});
		getWorker().postMessage(request, { transfer: buffers });
	});
}
