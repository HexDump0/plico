// The summarizing model's worker, seen from the page: loaded once, then one
// prompt at a time, its text streamed back as it is written.

import type { ModelBuild } from './summary-model';
import type {
	ChatMessage,
	SummarizeWorkerRequest,
	SummarizeWorkerResponse
} from './summarize-worker';

export type { ChatMessage } from './summarize-worker';

type Unsent<T> = T extends { id: number } ? Omit<T, 'id'> : never;

type Call = {
	resolve: (text: string) => void;
	reject: (error: Error) => void;
	update?: (loaded: number, total: number) => void;
	text?: (text: string, speed: number) => void;
};

export class Summarizer {
	private worker: Worker | undefined;
	private calls: Record<number, Call | undefined> = {};
	private next = 0;
	private loaded: Promise<string> | undefined;

	private call(
		request: Unsent<SummarizeWorkerRequest>,
		handlers: Omit<Call, 'resolve' | 'reject'>
	) {
		if (!this.worker) {
			this.worker = new Worker(new URL('./summarize-worker.ts', import.meta.url), {
				type: 'module'
			});
			this.worker.onmessage = ({ data }: MessageEvent<SummarizeWorkerResponse>) => {
				const call = this.calls[data.id];
				if (!call) return;
				if (data.type === 'progress') call.update?.(data.loaded, data.total);
				else if (data.type === 'text') call.text?.(data.text, data.speed);
				else {
					delete this.calls[data.id];
					if (data.type === 'done') call.resolve(data.text);
					else call.reject(new Error(data.error));
				}
			};
			this.worker.onerror = () => this.fail(new Error('The summary model stopped.'));
		}
		const id = ++this.next;
		return new Promise<string>((resolve, reject) => {
			this.calls[id] = { resolve, reject, ...handlers };
			this.worker!.postMessage({ ...request, id });
		});
	}

	private fail(error: Error) {
		for (const call of Object.values(this.calls)) call?.reject(error);
		this.calls = {};
		this.worker?.terminate();
		this.worker = undefined;
		this.loaded = undefined;
	}

	/// Downloads the model if it is not stored, and starts it, once.
	load(build: ModelBuild, update: (loaded: number, total: number) => void) {
		this.loaded ??= this.call({ operation: 'load', ...build }, { update });
		this.loaded.catch(() => (this.loaded = undefined));
		return this.loaded;
	}

	/// The model's next message; `text` sees it as it is written, with the
	/// tokens a second it is being written at.
	generate(
		messages: ChatMessage[],
		maxTokens: number,
		text: (text: string, speed: number) => void
	) {
		return this.call({ operation: 'generate', messages, maxTokens }, { text });
	}

	/// Ends the answer being written, which then resolves with what it has.
	interrupt() {
		this.worker?.postMessage({ operation: 'interrupt' } satisfies SummarizeWorkerRequest);
	}

	destroy() {
		this.fail(new DOMException('Cancelled', 'AbortError') as Error);
	}
}
