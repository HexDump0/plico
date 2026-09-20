import adapter from '@sveltejs/adapter-auto';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig, type Plugin } from 'vite';
import tailwindcss from '@tailwindcss/vite';
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

export default defineConfig({
	plugins: [
		tailwindcss(),
		pdfjsAssets(),
		sveltekit({
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
