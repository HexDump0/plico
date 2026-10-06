/// <reference types="@sveltejs/kit" />
/// <reference no-default-lib="true"/>
/// <reference lib="esnext" />
/// <reference lib="webworker" />

// Keeps Plico working without a network. Installing saves the core, which is
// every page and everything most tools load; packs, which a few tools need,
// are saved by the page on request or here the first time a tool fetches one.
// Saved files are served first, so a page loads the same online and off.

import { prerendered, version } from '$service-worker';
import {
	CORE_PREFIX,
	MANIFEST_URL,
	PACK_HEADER,
	PACK_IDS,
	PACKS,
	WANTED_URL,
	isStored,
	json,
	readJson,
	type OfflineFile,
	type OfflineManifest
} from '$lib/offline-files';

const sw = self as unknown as ServiceWorkerGlobalScope;
const CORE = `${CORE_PREFIX}${version}`;

async function tell(message: unknown) {
	for (const client of await sw.clients.matchAll({ includeUncontrolled: true }))
		client.postMessage(message);
}

async function previousCore() {
	const keys = (await caches.keys()).filter((key) => key.startsWith(CORE_PREFIX) && key !== CORE);
	return keys.length ? caches.open(keys[keys.length - 1]) : undefined;
}

// A redirected response cannot answer a navigation, so it is stored as a
// plain one.
const plain = (response: Response) =>
	response.redirected
		? new Response(response.body, { headers: response.headers, status: response.status })
		: response;

async function install() {
	const response = await fetch(MANIFEST_URL, { cache: 'no-store' });
	if (!response.ok) throw new Error(`${MANIFEST_URL}: ${response.status}`);
	const manifest: OfflineManifest = await response.json();
	const cache = await caches.open(CORE);
	// Files whose content did not change come from the last version's copy.
	const previous = await previousCore();
	const before = previous && (await readJson<OfflineManifest>(previous, MANIFEST_URL));
	const known = new Map(before?.core.map((file) => [file.url, file.hash]));
	const files: Partial<OfflineFile>[] = [...manifest.core, ...prerendered.map((url) => ({ url }))];
	const total = manifest.core.reduce((sum, file) => sum + file.size, 0);
	let received = 0;
	let next = 0;
	async function lane() {
		while (next < files.length) {
			const file = files[next++];
			const url = file.url!;
			const kept =
				file.hash && known.get(url) === file.hash ? await previous?.match(url) : undefined;
			const response = kept ?? (await fetch(url, { cache: 'reload' }));
			if (!response.ok) throw new Error(`${url}: ${response.status}`);
			await cache.put(url, plain(response));
			received += file.size ?? 0;
			void tell({ type: 'offline-progress', received, total });
		}
	}
	await Promise.all(Array.from({ length: 6 }, lane));
	// Written last: the core counts as saved once its list is there.
	await cache.put(MANIFEST_URL, json(manifest));
}

/// Drops pack files this version no longer serves, and marks packs that were
/// fully saved and changed, so the page fetches their new files.
async function reconcilePacks() {
	const manifest = await readJson<OfflineManifest>(await caches.open(CORE), MANIFEST_URL);
	if (!manifest) return;
	const packs = await caches.open(PACKS);
	const stored = await readJson<OfflineManifest>(packs, MANIFEST_URL);
	if (stored) {
		const wanted = new Set((await readJson<string[]>(packs, WANTED_URL)) ?? []);
		const current = new Map(
			Object.values(manifest.packs)
				.flat()
				.map((file) => [file.url, file.hash])
		);
		for (const id of PACK_IDS) {
			const files = stored.packs[id] ?? [];
			const changed = files.some((file) => current.get(file.url) !== file.hash);
			if (changed && (await isStored(packs, files))) wanted.add(id);
		}
		const hashes = new Map(
			Object.values(stored.packs)
				.flat()
				.map((file) => [file.url, file.hash])
		);
		for (const request of await packs.keys()) {
			const { pathname } = new URL(request.url);
			if (pathname === MANIFEST_URL || pathname === WANTED_URL) continue;
			const hash = current.get(pathname);
			if (!hash || hashes.get(pathname) !== hash) await packs.delete(request);
		}
		await packs.put(WANTED_URL, json([...wanted]));
	}
	await packs.put(MANIFEST_URL, json(manifest));
}

sw.addEventListener('install', (event) => {
	event.waitUntil(
		install().then(
			() => sw.skipWaiting(),
			async (error) => {
				await tell({ type: 'offline-failed' });
				throw error;
			}
		)
	);
});

sw.addEventListener('activate', (event) => {
	event.waitUntil(
		(async () => {
			// The newest older version stays, for pages still open on it.
			const old = (await caches.keys()).filter(
				(key) => key.startsWith(CORE_PREFIX) && key !== CORE
			);
			for (const key of old.slice(0, -1)) await caches.delete(key);
			await reconcilePacks();
			await sw.clients.claim();
			await tell({ type: 'offline-ready' });
		})()
	);
});

let packUrls: Promise<Set<string>> | undefined;

function packFileUrls() {
	return (packUrls ??= caches
		.open(CORE)
		.then((cache) => readJson<OfflineManifest>(cache, MANIFEST_URL))
		.then(
			(manifest) =>
				new Set(
					Object.values(manifest?.packs ?? {})
						.flat()
						.map((file) => file.url)
				)
		));
}

async function page(request: Request, url: URL) {
	const core = await caches.open(CORE);
	const saved = await core.match(url.pathname.replace(/(.)\/$/, '$1'));
	if (saved) return saved;
	try {
		return await fetch(request);
	} catch (error) {
		const home = await core.match('/');
		if (home) return home;
		throw error;
	}
}

async function asset(event: FetchEvent, url: URL) {
	const { request } = event;
	const core = await caches.open(CORE);
	const packs = await caches.open(PACKS);
	const saved =
		(await core.match(url.pathname)) ??
		(await packs.match(url.pathname)) ??
		// Hashed files a page still open on the last version may ask for.
		(url.pathname.startsWith('/_app/immutable/') ? await caches.match(url.pathname) : undefined);
	if (saved) return saved;
	const response = await fetch(request);
	if (response.ok && !request.headers.has(PACK_HEADER) && (await packFileUrls()).has(url.pathname))
		event.waitUntil(packs.put(url.pathname, response.clone()));
	return response;
}

sw.addEventListener('fetch', (event) => {
	const { request } = event;
	if (request.method !== 'GET') return;
	const url = new URL(request.url);
	if (url.origin !== sw.location.origin || url.pathname === '/_app/version.json') return;
	event.respondWith(request.mode === 'navigate' ? page(request, url) : asset(event, url));
});
