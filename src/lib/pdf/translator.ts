// Bergamot's worker, seen from the page: models loaded once, texts sent in
// batches, and a way to stop it mid-batch.

import type { TranslateModels } from './translate.svelte';
import type { TranslateWorkerRequest, TranslateWorkerResponse } from './translate-worker';

// Omit, applied to each kind of request.
type Unsent<T> = T extends unknown ? Omit<T, 'id'> : never;

type Call = { resolve: (texts: string[]) => void; reject: (error: Error) => void };

/// Bergamot in its worker, kept between runs with the models it has loaded.
export class Translator {
	private worker: Worker | undefined;
	private loaded: Record<string, Promise<void> | undefined> = {};
	private calls: Record<number, Call | undefined> = {};
	private next = 0;

	private call(request: Unsent<TranslateWorkerRequest>, transfer: Transferable[] = []) {
		if (!this.worker) {
			this.worker = new Worker(new URL('./translate-worker.ts', import.meta.url), {
				type: 'module'
			});
			this.worker.onmessage = ({ data }: MessageEvent<TranslateWorkerResponse>) => {
				const call = this.calls[data.id];
				delete this.calls[data.id];
				if (!call) return;
				if (data.ok) call.resolve(data.texts ?? []);
				else call.reject(new Error(data.error));
			};
			this.worker.onerror = () => this.fail(new Error('The translator stopped.'));
		}
		const id = ++this.next;
		return new Promise<string[]>((resolve, reject) => {
			this.calls[id] = { resolve, reject };
			this.worker!.postMessage({ ...request, id }, transfer);
		});
	}

	private fail(error: Error) {
		for (const call of Object.values(this.calls)) call?.reject(error);
		this.calls = {};
		this.worker?.terminate();
		this.worker = undefined;
		this.loaded = {};
	}

	/// Loads the stored models for `pairs` into the worker, once.
	prepare(models: TranslateModels, pairs: string[]) {
		return Promise.all(
			pairs.map((pair) => {
				let loading = this.loaded[pair];
				if (!loading) {
					loading = models.read(pair).then(async ({ model, shortlist, vocabs }) => {
						await this.call({ operation: 'load', pair, model, shortlist, vocabs }, [
							model,
							shortlist,
							...vocabs
						]);
					});
					loading.catch(() => delete this.loaded[pair]);
					this.loaded[pair] = loading;
				}
				return loading;
			})
		);
	}

	/// `texts` through `pairs`, whose models `prepare` has loaded.
	translate(pairs: string[], texts: string[], html = false) {
		if (!texts.length) return Promise.resolve([]);
		return this.call({ operation: 'translate', pairs, texts, html });
	}

	/// Stops whatever it is doing; the next call starts a new worker.
	destroy() {
		this.fail(new DOMException('Cancelled', 'AbortError') as Error);
	}
}
