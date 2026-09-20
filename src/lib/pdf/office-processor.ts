import { officeTools, type OfficeOperation } from './office-conversion';
import type { PdfOutput } from './types';

type WorkerResponse =
	{ id: number; ok: true; bytes: ArrayBuffer } | { id: number; ok: false; error: string };

export async function processOfficeFile(
	operation: OfficeOperation,
	file: File,
	signal?: AbortSignal
): Promise<PdfOutput> {
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const input = await file.arrayBuffer();
	if (signal?.aborted) throw new DOMException('The operation was cancelled.', 'AbortError');
	const worker = new Worker(new URL('./office-worker.ts', import.meta.url), { type: 'module' });
	return new Promise((resolve, reject) => {
		const abort = () => finish(new DOMException('The operation was cancelled.', 'AbortError'));
		function finish(error?: Error, output?: PdfOutput) {
			signal?.removeEventListener('abort', abort);
			worker.terminate();
			if (error) reject(error);
			else if (output) resolve(output);
		}
		signal?.addEventListener('abort', abort, { once: true });
		worker.onerror = () => finish(new Error('The local Office converter stopped unexpectedly.'));
		worker.onmessageerror = () =>
			finish(new Error('The local Office converter returned an unreadable result.'));
		worker.onmessage = (event: MessageEvent<WorkerResponse>) => {
			if (event.data.id !== 1) return;
			if (!event.data.ok) finish(new Error(event.data.error));
			else
				finish(undefined, {
					bytes: new Uint8Array(event.data.bytes),
					format: officeTools[operation].output
				});
		};
		worker.postMessage({ id: 1, operation, input }, { transfer: [input] });
	});
}
