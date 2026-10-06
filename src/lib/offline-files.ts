// What the service worker keeps, shared between it and the page. The build
// writes the list (`offlineManifest` in vite.config.ts): the core every page
// needs, saved on the first visit, and packs, the large engines and fonts only
// some tools load, saved when someone downloads them or first uses them.

export const PACK_IDS = ['office', 'repair', 'ocr', 'translate', 'cjk'] as const;
export type PackId = (typeof PACK_IDS)[number];

export type OfflineFile = {
	url: string;
	size: number;
	hash: string;
	/// Tesseract ships one engine per wasm feature level; a browser needs one.
	variant?: string;
};

export type OfflineManifest = { core: OfflineFile[]; packs: Record<PackId, OfflineFile[]> };

export const MANIFEST_URL = '/_app/offline.json';
export const CORE_PREFIX = 'plico-core-';
export const PACKS = 'plico-packs';
/// Kept in the packs cache: the packs an update changed, which the page
/// downloads again.
export const WANTED_URL = '/_app/offline-wanted.json';
/// Sent by the page on its own pack downloads, so the worker does not store
/// the same response a second time.
export const PACK_HEADER = 'x-plico-pack';

/// A pack's files for this browser, or with no variant, every file but the
/// variants, which `isStored` accepts any one of.
export function packFiles(files: OfflineFile[], variant?: string) {
	return files.filter((file) => !file.variant || file.variant === variant);
}

export async function storedBytes(cache: Cache, files: OfflineFile[]) {
	let stored = 0;
	for (const file of files) if (await cache.match(file.url)) stored += file.size;
	return stored;
}

export async function isStored(cache: Cache, files: OfflineFile[]) {
	for (const file of packFiles(files)) if (!(await cache.match(file.url))) return false;
	const variants = files.filter((file) => file.variant);
	if (!variants.length) return true;
	for (const file of variants) if (await cache.match(file.url)) return true;
	return false;
}

export async function readJson<T>(cache: Cache, url: string): Promise<T | undefined> {
	const response = await cache.match(url);
	return response ? ((await response.json()) as T) : undefined;
}

export const json = (value: unknown) =>
	new Response(JSON.stringify(value), { headers: { 'content-type': 'application/json' } });

/// What to say when an engine failed to load without a connection, which
/// almost always means its pack was never saved. Workers use it too.
export function offlineReason(engine: string) {
	return typeof navigator !== 'undefined' && !navigator.onLine
		? `${engine} hasn't been downloaded for offline use.`
		: undefined;
}

/// Every font but the CJK ones is in the core, so only they can be missing
/// offline.
export const CJK_OFFLINE =
	"Chinese, Japanese and Korean fonts haven't been downloaded for offline use.";
