// Bergamot, the engine Firefox translates pages with (MPL-2.0), in a worker of
// its own. Its Emscripten glue is a classic script that expects to run as the
// worker's top level, so it is evaluated here as one; the wasm is this site's
// own asset. Models arrive as bytes from the page, which keeps them stored.

import glue from '@mkljczk/bergamot-translator/worker/bergamot-translator-worker.js?raw';
import wasmUrl from '@mkljczk/bergamot-translator/worker/bergamot-translator-worker.wasm?url';

export type TranslateWorkerRequest =
	| {
			id: number;
			operation: 'load';
			pair: string;
			model: ArrayBuffer;
			shortlist: ArrayBuffer;
			vocabs: ArrayBuffer[];
	  }
	// Through each pair in turn: one, or two when going by way of English.
	| { id: number; operation: 'translate'; pairs: string[]; texts: string[]; html: boolean };

export type TranslateWorkerResponse =
	{ id: number; ok: true; texts?: string[] } | { id: number; ok: false; error: string };

type Vector<T> = { push_back(value: T): void; get(index: number): T; delete(): void };
type Memory = { getByteArrayView(): Int8Array };
type Bergamot = {
	asm: Record<string, (...args: number[]) => number>;
	AlignedMemory: new (size: number, alignment: number) => Memory;
	AlignedMemoryList: new () => Vector<Memory>;
	VectorString: new () => Vector<string>;
	VectorResponseOptions: new () => Vector<{
		alignment: boolean;
		html: boolean;
		qualityScores: boolean;
	}>;
	TranslationModel: new (
		config: string,
		model: Memory,
		shortlist: Memory,
		vocabs: Vector<Memory>,
		quality: null
	) => object;
	BlockingService: new (options: { cacheSize: number }) => {
		translate(model: object, input: Vector<string>, options: unknown): Vector<Response>;
		translateViaPivoting(
			first: object,
			second: object,
			input: Vector<string>,
			options: unknown
		): Vector<Response>;
	};
};
type Response = { getTranslatedText(): string };

// The matrix routines the wasm imports, which it also carries itself under
// these names.
const gemm = {
	int8_prepare_a: 'int8PrepareAFallback',
	int8_prepare_b: 'int8PrepareBFallback',
	int8_prepare_b_from_transposed: 'int8PrepareBFromTransposedFallback',
	int8_prepare_b_from_quantized_transposed: 'int8PrepareBFromQuantizedTransposedFallback',
	int8_prepare_bias: 'int8PrepareBiasFallback',
	int8_multiply_and_add_bias: 'int8MultiplyAndAddBiasFallback',
	int8_select_columns_of_b: 'int8SelectColumnsOfBFallback'
};

// Firefox's settings for these models: greedy decoding over int8 weights.
const CONFIG = `beam-size: 1
normalize: 1.0
word-penalty: 0
cpu-threads: 0
gemm-precision: int8shiftAlphaAll
skip-cost: true
alignment: soft
quiet: true
quiet-translation: true
max-length-break: 128
mini-batch-words: 1024
workspace: 128
max-length-factor: 2.0
`;

let module: Promise<Bergamot> | undefined;
let service: InstanceType<Bergamot['BlockingService']> | undefined;
const models = new Map<string, object>();

function start() {
	return (module ??= new Promise<Bergamot>((resolve, reject) => {
		const scope = self as unknown as { Module: Partial<Bergamot> & Record<string, unknown> };
		scope.Module = {
			instantiateWasm(
				imports: WebAssembly.Imports,
				accept: (instance: WebAssembly.Instance) => void
			) {
				const linked = Object.fromEntries(
					Object.entries(gemm).map(([name, fallback]) => [
						name,
						(...args: number[]) => (scope.Module as Bergamot).asm[fallback](...args)
					])
				);
				WebAssembly.instantiateStreaming(fetch(wasmUrl), { ...imports, wasm_gemm: linked })
					.then(({ instance }) => accept(instance))
					.catch(reject);
				return {};
			},
			onRuntimeInitialized: () => resolve(scope.Module as Bergamot)
		};
		try {
			// Indirectly, so its `var`s land on the worker's global scope.
			(0, eval)(glue);
		} catch (error) {
			reject(error);
		}
	}));
}

function memory(bergamot: Bergamot, buffer: ArrayBuffer, alignment: number) {
	const bytes = new Int8Array(buffer);
	const aligned = new bergamot.AlignedMemory(bytes.byteLength, alignment);
	aligned.getByteArrayView().set(bytes);
	return aligned;
}

async function load(
	pair: string,
	model: ArrayBuffer,
	shortlist: ArrayBuffer,
	vocabs: ArrayBuffer[]
) {
	if (models.has(pair)) return;
	const bergamot = await start();
	const list = new bergamot.AlignedMemoryList();
	for (const vocab of vocabs) list.push_back(memory(bergamot, vocab, 64));
	models.set(
		pair,
		new bergamot.TranslationModel(
			CONFIG,
			memory(bergamot, model, 256),
			memory(bergamot, shortlist, 64),
			list,
			null
		)
	);
}

async function translate(pairs: string[], texts: string[], html: boolean) {
	const bergamot = await start();
	service ??= new bergamot.BlockingService({ cacheSize: 0 });
	const loaded = pairs.map((pair) => models.get(pair));
	if (loaded.some((model) => !model)) throw new Error('A translation model is not loaded.');
	const input = new bergamot.VectorString();
	const options = new bergamot.VectorResponseOptions();
	for (const text of texts) {
		input.push_back(text);
		options.push_back({ alignment: false, html, qualityScores: false });
	}
	const responses =
		loaded.length > 1
			? service.translateViaPivoting(loaded[0]!, loaded[1]!, input, options)
			: service.translate(loaded[0]!, input, options);
	input.delete();
	options.delete();
	const translated = texts.map((_, index) => responses.get(index).getTranslatedText());
	responses.delete();
	return translated;
}

self.onmessage = async ({ data }: MessageEvent<TranslateWorkerRequest>) => {
	try {
		if (data.operation === 'load') {
			await load(data.pair, data.model, data.shortlist, data.vocabs);
			postMessage({ id: data.id, ok: true } satisfies TranslateWorkerResponse);
		} else {
			const texts = await translate(data.pairs, data.texts, data.html);
			postMessage({ id: data.id, ok: true, texts } satisfies TranslateWorkerResponse);
		}
	} catch (error) {
		postMessage({
			id: data.id,
			ok: false,
			error: error instanceof Error ? error.message : String(error)
		} satisfies TranslateWorkerResponse);
	}
};
