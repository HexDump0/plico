// The page's side of working offline: registering the service worker, how
// far it has saved the core, the packs and language models on this device,
// and installing Plico as an app. Only a production build registers the
// worker, so in development `available` stays false and nothing is shown.

import { dev, version } from '$app/environment';
import { SvelteMap } from 'svelte/reactivity';
import {
	CORE_PREFIX,
	MANIFEST_URL,
	PACK_HEADER,
	PACK_IDS,
	PACKS,
	WANTED_URL,
	json,
	packFiles,
	readJson,
	storedBytes,
	type OfflineFile,
	type OfflineManifest,
	type PackId
} from './offline-files';

export type { PackId } from './offline-files';

type InstallPrompt = Event & {
	prompt(): Promise<void>;
	userChoice: Promise<{ outcome: 'accepted' | 'dismissed' }>;
};

export type SaveState = {
	status: 'missing' | 'partial' | 'saving' | 'ready' | 'failed';
	received: number;
	total: number;
};

export type StoredModel = { kind: 'ocr' | 'translate'; id: string; name: string; size: number };

const CORE = `${CORE_PREFIX}${version}`;
const sum = (files: OfflineFile[]) => files.reduce((total, file) => total + file.size, 0);

class Offline {
	available = $state(false);
	core = $state<SaveState>({ status: 'saving', received: 0, total: 0 });
	coreSize = $state(0);
	packs = new SvelteMap<PackId, SaveState>();
	models = $state.raw<StoredModel[]>([]);
	online = $state(true);
	installable = $state(false);
	private prompt: InstallPrompt | undefined;
	private manifest: OfflineManifest | undefined;
	private variant: string | undefined;

	start() {
		if (dev || this.available || !('serviceWorker' in navigator) || !('caches' in window)) return;
		this.available = true;
		this.online = navigator.onLine;
		addEventListener('online', () => {
			this.online = true;
			void this.resume();
		});
		addEventListener('offline', () => (this.online = false));
		addEventListener('beforeinstallprompt', (event) => {
			event.preventDefault();
			this.prompt = event as InstallPrompt;
			this.installable = true;
		});
		addEventListener('appinstalled', () => {
			this.prompt = undefined;
			this.installable = false;
		});
		navigator.serviceWorker.addEventListener('message', (event) => {
			const { type, received, total } = event.data ?? {};
			if (type === 'offline-progress') this.core = { status: 'saving', received, total };
			else if (type === 'offline-failed') this.core = { ...this.core, status: 'failed' };
			else if (type === 'offline-ready') void this.refresh();
		});
		// After the page has loaded, so saving the core does not compete with it.
		const register = () =>
			navigator.serviceWorker.register('/service-worker.js').then(
				() => this.refresh(),
				() => (this.core = { ...this.core, status: 'failed' })
			);
		if (document.readyState === 'complete') void register();
		else addEventListener('load', () => void register(), { once: true });
	}

	async refresh() {
		const core = (await caches.has(CORE)) ? await caches.open(CORE) : undefined;
		const saved = core && (await readJson<OfflineManifest>(core, MANIFEST_URL));
		if (saved) this.core = { status: 'ready', received: 0, total: 0 };
		else if (this.core.status !== 'failed') {
			const registration = await navigator.serviceWorker.getRegistration();
			if (!registration?.installing) this.core = { ...this.core, status: 'failed' };
		}
		this.manifest = saved || (await this.fetchManifest());
		if (this.manifest) {
			this.coreSize = sum(this.manifest.core);
			this.variant ??= await (await import('./pdf/ocr.svelte')).engineVariant();
			const packs = await caches.open(PACKS);
			for (const id of PACK_IDS) {
				if (this.packs.get(id)?.status === 'saving') continue;
				const files = packFiles(this.manifest.packs[id] ?? [], this.variant);
				const total = sum(files);
				const received = await storedBytes(packs, files);
				this.packs.set(id, {
					status: received >= total ? 'ready' : received ? 'partial' : 'missing',
					received,
					total
				});
			}
		}
		this.models = await storedModels();
		void this.resume();
	}

	private async fetchManifest() {
		const response = await fetch(MANIFEST_URL).catch(() => undefined);
		return response?.ok ? ((await response.json()) as OfflineManifest) : undefined;
	}

	/// Saves the packs an update changed again, once there is a connection.
	private async resume() {
		if (!navigator.onLine || !this.manifest) return;
		const wanted = (await readJson<PackId[]>(await caches.open(PACKS), WANTED_URL)) ?? [];
		for (const id of wanted) if (this.packs.get(id)?.status !== 'ready') void this.download(id);
	}

	private async forget(id: PackId) {
		const cache = await caches.open(PACKS);
		const wanted = (await readJson<PackId[]>(cache, WANTED_URL)) ?? [];
		if (wanted.includes(id))
			await cache.put(WANTED_URL, json(wanted.filter((other) => other !== id)));
	}

	async retry() {
		this.core = { status: 'saving', received: 0, total: 0 };
		const registration = await navigator.serviceWorker.getRegistration();
		await (
			registration ? registration.update() : navigator.serviceWorker.register('/service-worker.js')
		)
			.then(() => this.refresh())
			.catch(() => (this.core = { ...this.core, status: 'failed' }));
	}

	async download(id: PackId) {
		if (!this.manifest || this.packs.get(id)?.status === 'saving') return;
		const files = packFiles(this.manifest.packs[id] ?? [], this.variant);
		const total = sum(files);
		let received = 0;
		const report = () => this.packs.set(id, { status: 'saving', received, total });
		report();
		void navigator.storage?.persist?.();
		try {
			const cache = await caches.open(PACKS);
			for (const file of files) {
				if (!(await cache.match(file.url))) {
					const response = await fetch(file.url, {
						cache: 'reload',
						headers: { [PACK_HEADER]: '1' }
					});
					if (!response.ok || !response.body) throw new Error(`HTTP ${response.status}`);
					const reader = response.body.getReader();
					const chunks: Uint8Array<ArrayBuffer>[] = [];
					for (;;) {
						const { done, value } = await reader.read();
						if (done) break;
						chunks.push(value);
						received += value.length;
						report();
					}
					const type = response.headers.get('content-type') ?? '';
					await cache.put(
						file.url,
						new Response(new Blob(chunks, { type }), { headers: { 'content-type': type } })
					);
				} else received += file.size;
				report();
			}
			await this.forget(id);
			this.packs.set(id, { status: 'ready', received: total, total });
		} catch {
			this.packs.set(id, { status: 'failed', received, total });
		}
	}

	async remove(id: PackId) {
		const cache = await caches.open(PACKS);
		for (const file of this.manifest?.packs[id] ?? []) await cache.delete(file.url);
		await this.forget(id);
		const total = this.packs.get(id)?.total ?? 0;
		this.packs.set(id, { status: 'missing', received: 0, total });
	}

	async removeModel(model: StoredModel) {
		if (model.kind === 'ocr') await (await import('./pdf/ocr.svelte')).removeLanguage(model.id);
		else await (await import('./pdf/translate.svelte')).removePair(model.id);
		this.models = this.models.filter((other) => other !== model);
	}

	async install() {
		const prompt = this.prompt;
		if (!prompt) return;
		await prompt.prompt();
		if ((await prompt.userChoice).outcome !== 'accepted') return;
		this.prompt = undefined;
		this.installable = false;
		void navigator.storage?.persist?.();
	}
}

async function storedModels(): Promise<StoredModel[]> {
	const [ocr, translate] = await Promise.all([
		import('./pdf/ocr.svelte'),
		import('./pdf/translate.svelte')
	]);
	const [languages, pairs] = await Promise.all([ocr.storedLanguages(), translate.storedPairs()]);
	return [
		...languages.map(({ code, size }) => ({
			kind: 'ocr' as const,
			id: code,
			name: ocr.languageName(code),
			size
		})),
		...pairs.map(({ pair, size }) => ({
			kind: 'translate' as const,
			id: pair,
			name: translate.pairName(pair),
			size
		}))
	];
}

export const offline = new Offline();
