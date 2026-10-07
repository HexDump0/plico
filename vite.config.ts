import adapter from '@sveltejs/adapter-auto';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig, type Plugin } from 'vite';
import tailwindcss from '@tailwindcss/vite';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';

function pdfjsAssets(): Plugin {
	return {
		name: 'pdfjs-assets',
		configureServer(server) {
			server.middlewares.use((req, res, next) => {
				const match = req.url?.match(/^\/pdfjs\/(cmaps|standard_fonts)\/(.+)$/);
				if (match) {
					const [, folder, file] = match;
					const filePath = path.resolve('node_modules/pdfjs-dist', folder, file);
					if (fs.existsSync(filePath)) {
						res.setHeader('Content-Type', 'application/octet-stream');
						return fs.createReadStream(filePath).pipe(res);
					}
				}
				next();
			});
		},
		generateBundle() {
			for (const folder of ['cmaps', 'standard_fonts'] as const) {
				const dir = path.resolve('node_modules/pdfjs-dist', folder);
				if (!fs.existsSync(dir)) continue;
				for (const file of fs.readdirSync(dir)) {
					const filePath = path.join(dir, file);
					if (fs.statSync(filePath).isFile()) {
						this.emitFile({
							type: 'asset',
							fileName: `pdfjs/${folder}/${file}`,
							source: fs.readFileSync(filePath)
						});
					}
				}
			}
		}
	};
}

// Files the service worker should not keep: build bookkeeping, licences and
// notes, and `version.json`, which SvelteKit fetches to notice a new release.
const notOffline =
	/^\/(\.vite\/|_app\/version\.json$|_app\/offline\.json$|service-worker\.js$|robots\.txt$)|\/(README\.md|OFL\.txt)$|\.(map|br|gz)$/;

const packPatterns: [string, RegExp][] = [
	['office', /\/pdf_oxide_bg-[^/]+\.wasm$/],
	['repair', /\/mupdf-wasm-[^/]+\.wasm$/],
	[
		'ocr',
		/\/tesseract-core-[^/]+\.wasm\.[^/]+\.js$|\/assets\/worker\.min\.[^/]+\.js$|^\/tessdata\/eng\.traineddata\.gz$/
	],
	['translate', /\/bergamot-translator-worker-[^/]+\.wasm$/],
	['summarize', /\/ort-wasm-simd-threaded\.asyncify-[^/]+\.wasm$/],
	['cjk', /^\/fonts\/NotoSans(JP|KR|SC|TC)-/]
];

/// Lists every file the built site serves, with its size and a content hash,
/// as `_app/offline.json`. The service worker saves the core from it and the
/// page offers the packs; the hash lets an update keep files that did not
/// change, since `static/` files keep their names.
function offlineManifest(): Plugin {
	function walk(dir: string, prefix = ''): [string, string][] {
		if (!fs.existsSync(dir)) return [];
		return fs.readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
			const file = path.join(dir, entry.name);
			const url = `${prefix}/${entry.name}`;
			return entry.isDirectory() ? walk(file, url) : [[url, file] as [string, string]];
		});
	}
	return {
		name: 'offline-manifest',
		apply: 'build',
		writeBundle: {
			order: 'post',
			handler(options) {
				if (this.environment.name !== 'client' || !options.dir) return;
				const files = new Map([...walk('static'), ...walk(options.dir)]);
				const core: unknown[] = [];
				const packs: Record<string, unknown[]> = Object.fromEntries(
					packPatterns.map(([id]) => [id, []])
				);
				for (const [url, file] of [...files].sort(([a], [b]) => a.localeCompare(b))) {
					if (notOffline.test(url)) continue;
					const bytes = fs.readFileSync(file);
					const engine = url.match(/tesseract-core-(?:(relaxedsimd|simd)-)?lstm/);
					const entry = {
						url,
						size: bytes.length,
						hash: createHash('sha256').update(bytes).digest('hex').slice(0, 16),
						variant: engine ? (engine[1] ?? 'lstm') : undefined
					};
					const pack = packPatterns.find(([, pattern]) => pattern.test(url))?.[0];
					(pack ? packs[pack] : core).push(entry);
				}
				fs.writeFileSync(
					path.join(options.dir, '_app/offline.json'),
					JSON.stringify({ core, packs })
				);
			}
		}
	};
}

export default defineConfig({
	// Every worker is started with `type: 'module'`, and MuPDF's uses top-level
	// await, which the default iife format cannot hold.
	worker: { format: 'es' },
	resolve: {
		alias: [
			// ONNX Runtime's exports hide its wasm, which Summarize serves itself.
			// A pattern, since a plain alias does not match with `?url` after it.
			{
				find: /^onnxruntime-web-wasm(?=\?|$)/,
				replacement: path.resolve(
					'node_modules/onnxruntime-web/dist/ort-wasm-simd-threaded.asyncify.wasm'
				)
			}
		]
	},
	ssr: {
		// GSAP's ESM entry is not loadable through Vercel's CommonJS function wrapper.
		// Bundle it into the server output so Vite normalizes the module format.
		noExternal: ['gsap']
	},
	plugins: [
		tailwindcss(),
		pdfjsAssets(),
		offlineManifest(),
		sveltekit({
			// Registered by `offline.svelte.ts`, only in a production build.
			serviceWorker: { register: false },
			compilerOptions: {
				// Force runes mode for the project, except for libraries. Can be removed in svelte 6.
				runes: ({ filename }) =>
					filename.split(/[/\\]/).includes('node_modules') ? undefined : true
			},

			// adapter-auto only supports some environments, see https://svelte.dev/docs/kit/adapter-auto for a list.
			// If your environment is not supported, or you settled on a specific environment, switch out the adapter.
			// See https://svelte.dev/docs/kit/adapters for more information about adapters.
			adapter: adapter()
		})
	]
});
