// The summarizing model in a worker of its own, through Transformers.js
// (Apache-2.0). The weights come from the pinned revision in
// `summary-model.ts` and stay in Transformers.js's cache; only the model
// travels, never the document. ONNX Runtime's wasm is this site's own copy,
// where Transformers.js would otherwise fetch it from jsDelivr.

import {
	AutoModelForCausalLM,
	AutoTokenizer,
	InterruptableStoppingCriteria,
	TextStreamer,
	env,
	type PreTrainedModel,
	type PreTrainedTokenizer
} from '@huggingface/transformers';
import ortWasm from 'onnxruntime-web-wasm?url';
import { offlineReason } from '$lib/offline-files';
import { MODEL_ID, MODEL_REVISION, type ModelBuild } from './summary-model';

export type ChatMessage = { role: 'system' | 'user' | 'assistant'; content: string };

export type SummarizeWorkerRequest =
	| ({ id: number; operation: 'load' } & ModelBuild)
	| { id: number; operation: 'generate'; messages: ChatMessage[]; maxTokens: number }
	| { operation: 'interrupt' };

export type SummarizeWorkerResponse =
	| { id: number; type: 'progress'; loaded: number; total: number }
	/// `speed` is tokens a second since the first, 0 until there are two.
	| { id: number; type: 'text'; text: string; speed: number }
	| { id: number; type: 'done'; text: string }
	| { id: number; type: 'error'; error: string };

env.allowLocalModels = false;
// The service worker keeps the wasm; a second copy in Transformers.js's
// cache would only double it.
env.useWasmCache = false;
env.backends.onnx.wasm!.wasmPaths = { wasm: new URL(ortWasm, location.href).href };

let loading: Promise<[PreTrainedTokenizer, PreTrainedModel]> | undefined;
const stopping = new InterruptableStoppingCriteria();
const post = (message: SummarizeWorkerResponse) => postMessage(message);

async function load(id: number, { device, dtype }: ModelBuild) {
	const files: Record<string, { loaded: number; total: number }> = {};
	const progress_callback = (event: {
		status: string;
		file?: string;
		loaded?: number;
		total?: number;
	}) => {
		if (event.status !== 'progress' || !event.file) return;
		files[event.file] = { loaded: event.loaded ?? 0, total: event.total ?? 0 };
		const all = Object.values(files);
		post({
			id,
			type: 'progress',
			loaded: all.reduce((sum, file) => sum + file.loaded, 0),
			total: all.reduce((sum, file) => sum + file.total, 0)
		});
	};
	const options = { revision: MODEL_REVISION, progress_callback };
	const [tokenizer, model] = await Promise.all([
		AutoTokenizer.from_pretrained(MODEL_ID, options),
		AutoModelForCausalLM.from_pretrained(MODEL_ID, { ...options, device, dtype })
	]);
	// One token through the model compiles WebGPU's shaders now rather than
	// during the first summary.
	await model.generate({ ...tokenizer('a'), max_new_tokens: 1 });
	return [tokenizer, model] as [PreTrainedTokenizer, PreTrainedModel];
}

async function generate(id: number, messages: ChatMessage[], maxTokens: number) {
	if (!loading) throw new Error('The model is not loaded.');
	const [tokenizer, model] = await loading;
	// Extra options reach the template, where Qwen3 reads `enable_thinking`;
	// summaries need no reasoning written out first.
	const options = { add_generation_prompt: true, return_dict: true, enable_thinking: false };
	const inputs = tokenizer.apply_chat_template(messages, options) as Record<string, unknown>;
	let text = '';
	let tokens = 0;
	let first = 0;
	let speed = 0;
	const streamer = new TextStreamer(tokenizer, {
		skip_prompt: true,
		skip_special_tokens: true,
		// The first token waits for the whole prompt to be read, so the rate
		// counts from it.
		token_callback_function: () => {
			if (tokens++ === 0) first = performance.now();
			else speed = ((tokens - 1) * 1000) / (performance.now() - first);
		},
		callback_function: (part: string) => {
			text += part;
			post({ id, type: 'text', text, speed });
		}
	});
	stopping.reset();
	await model.generate({
		...inputs,
		max_new_tokens: maxTokens,
		do_sample: false,
		repetition_penalty: 1.1,
		streamer,
		stopping_criteria: stopping
	});
	return text;
}

self.onmessage = async ({ data }: MessageEvent<SummarizeWorkerRequest>) => {
	if (data.operation === 'interrupt') {
		stopping.interrupt();
		return;
	}
	try {
		if (data.operation === 'load') {
			loading ??= load(data.id, data);
			loading.catch(() => (loading = undefined));
			await loading;
			post({ id: data.id, type: 'done', text: '' });
		} else {
			const text = await generate(data.id, data.messages, data.maxTokens);
			post({ id: data.id, type: 'done', text });
		}
	} catch (error) {
		const reason = data.operation === 'load' ? offlineReason('The summary model') : undefined;
		post({
			id: data.id,
			type: 'error',
			error: reason ?? (error instanceof Error ? error.message : String(error))
		});
	}
};
