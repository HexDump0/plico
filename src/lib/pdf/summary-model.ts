// The model Summarize runs: Qwen3 0.6B (Apache-2.0), converted for
// Transformers.js by onnx-community, pinned to one revision so the files are
// always these. WebGPU runs 4-bit weights, with 16-bit maths where the GPU has
// it (`shader-f16`; older GPUs such as AMD's Polaris do not) and 32-bit
// otherwise. Without WebGPU, the CPU runs the 8-bit weights, which
// WebAssembly handles far better.
// Transformers.js keeps the files in its own cache, by their URLs.

export const MODEL_ID = 'onnx-community/Qwen3-0.6B-ONNX';
export const MODEL_REVISION = 'da1453100cf3ff33ef56d17983fc7a8648706db6';
export const MODEL_NAME = 'Qwen3 0.6B';
export const MODEL_CACHE = 'transformers-cache';

export type ModelBuild = { device: 'webgpu' | 'wasm'; dtype: 'q4f16' | 'q4' | 'q8' };

// Everything Transformers.js fetches besides the weights, in bytes.
const SHARED = {
	'config.json': 912,
	'generation_config.json': 219,
	'tokenizer.json': 9117040,
	'tokenizer_config.json': 9705,
	'chat_template.jinja': 4116
};
const WEIGHTS = {
	q4f16: ['onnx/model_q4f16.onnx', 569789750],
	q4: ['onnx/model_q4.onnx', 919096585],
	q8: ['onnx/model_quantized.onnx', 617687575]
} as const;

export const modelUrl = (file: string) =>
	`https://huggingface.co/${MODEL_ID}/resolve/${MODEL_REVISION}/${file}`;

export function modelFiles(dtype: ModelBuild['dtype']) {
	const [weights, size] = WEIGHTS[dtype];
	return [...Object.entries(SHARED), [weights, size] as const].map(([file, size]) => ({
		url: modelUrl(file),
		size
	}));
}

export const modelBytes = (dtype: ModelBuild['dtype']) =>
	modelFiles(dtype).reduce((total, file) => total + file.size, 0);

export async function modelBuild(): Promise<ModelBuild> {
	const { gpu } = navigator as Navigator & {
		gpu?: { requestAdapter(): Promise<{ features: ReadonlySet<string> } | null> };
	};
	const adapter = await gpu?.requestAdapter().catch(() => null);
	if (!adapter) return { device: 'wasm', dtype: 'q8' };
	return { device: 'webgpu', dtype: adapter.features.has('shader-f16') ? 'q4f16' : 'q4' };
}

/// Which builds are in the cache, and their sizes.
export async function storedBuilds() {
	if (typeof caches === 'undefined' || !(await caches.has(MODEL_CACHE))) return [];
	const cache = await caches.open(MODEL_CACHE);
	const builds: { dtype: ModelBuild['dtype']; size: number }[] = [];
	for (const dtype of ['q4f16', 'q4', 'q8'] as const) {
		const [weights] = WEIGHTS[dtype];
		if (await cache.match(modelUrl(weights))) builds.push({ dtype, size: modelBytes(dtype) });
	}
	return builds;
}

/// Removes a build's weights, and the shared files once no build is left.
export async function removeBuild(dtype: ModelBuild['dtype']) {
	const cache = await caches.open(MODEL_CACHE);
	await cache.delete(modelUrl(WEIGHTS[dtype][0]));
	if (!(await storedBuilds()).length)
		for (const file of Object.keys(SHARED)) await cache.delete(modelUrl(file));
}
