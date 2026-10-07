// Translating text with Bergamot, the engine Firefox translates pages with,
// on this device.
//
// Models are Mozilla's, from a mirror on Hugging Face, since Mozilla's own
// buckets refuse other sites; the mirror is pinned to one revision, so the
// files are always the ones listed here. Only the model travels, never the
// document. Each pair is fetched once and kept in IndexedDB. There is a model
// to and from English for each language, so other pairs go by way of English.

import { SvelteMap } from 'svelte/reactivity';

const MIRROR =
	'https://huggingface.co/midudev/firefox-translations/resolve/6f6acd72c26975a06a8e61b72d1fbacbd184c23f';

/// Each pair's files at that revision, in bytes: the model, its shortlist,
/// then one vocabulary shared by both sides or one for each.
const MODEL_SIZES: Record<string, number[]> = {
	'af-en': [31561787, 4082680, 809319],
	'ar-en': [31561787, 4627200, 860035],
	'bg-en': [31561787, 4355920, 922482],
	'bn-en': [17141051, 4793596, 981300],
	'ca-en': [31561787, 4359552, 810273],
	'cs-en': [31561787, 4990588, 815154],
	'da-en': [17141051, 4489412, 796018],
	'de-en': [31561787, 4945796, 810073],
	'el-en': [17141051, 4295860, 894960],
	'en-af': [31561787, 3193340, 809195],
	'en-ar': [31561787, 3139692, 863591],
	'en-az': [17141051, 3332384, 835729],
	'en-bg': [31561787, 3075948, 922482],
	'en-bn': [17141051, 3207488, 981226],
	'en-bs': [31561787, 3214628, 823444],
	'en-ca': [31561787, 4344092, 810653],
	'en-cs': [31561787, 3572436, 815154],
	'en-da': [17141051, 3864788, 795349],
	'en-de': [31561787, 4347672, 810073],
	'en-el': [17141051, 2856724, 896168],
	'en-es': [31561787, 4198436, 816054],
	'en-et': [31561787, 3488368, 817574],
	'en-eu': [31561787, 3149580, 820820],
	'en-fa': [17141051, 3631332, 843133],
	'en-fi': [31561787, 3490092, 818770],
	'en-fr': [31561787, 4372936, 814404],
	'en-gl': [31561787, 3007680, 827026],
	'en-gu': [17141051, 3179500, 965095],
	'en-he': [17141051, 3066048, 845017],
	'en-hi': [17141051, 3504820, 925360],
	'en-hr': [31561787, 3232644, 820766],
	'en-hu': [31561787, 3419024, 832855],
	'en-id': [17141051, 3515428, 773211],
	'en-is': [31561787, 3837564, 809991],
	'en-it': [31561787, 4133192, 812724],
	'en-ja': [43849787, 4128360, 796275, 827144],
	'en-kn': [17141051, 2808304, 1065180],
	'en-ko': [43849787, 6449612, 791280, 815353],
	'en-lt': [31561787, 3421640, 806827],
	'en-lv': [31561787, 3359096, 816287],
	'en-ml': [17141051, 2532960, 1109000],
	'en-mr': [31561787, 2731352, 1009700],
	'en-ms': [17141051, 3936428, 802440],
	'en-nb': [17141051, 4234572, 802190],
	'en-nl': [31561787, 4030624, 807708],
	'en-no': [17141051, 4234572, 802190],
	'en-pl': [31561787, 3540052, 821612],
	'en-pt': [31561787, 3970340, 816726],
	'en-ro': [17141051, 3739880, 801522],
	'en-ru': [31561787, 2774540, 904455],
	'en-sk': [31561787, 3356348, 808888],
	'en-sl': [31561787, 3428300, 803212],
	'en-sr': [31561787, 2542096, 922599],
	'en-sv': [17141051, 3948180, 793868],
	'en-ta': [31561787, 2746824, 1097357],
	'en-te': [17141051, 2832156, 1056424],
	'en-th': [31561787, 3016612, 993517],
	'en-tr': [17141051, 3261300, 798793],
	'en-uk': [31561787, 3054364, 922077],
	'en-ur': [31561787, 3067772, 842549],
	'en-vi': [31561787, 4597684, 785200],
	'en-zh': [43849787, 4485184, 806952, 772004],
	'en-zh-Hant': [43849787, 4057188, 803694, 751671],
	'es-en': [31561787, 4636248, 816054],
	'et-en': [31561787, 4625968, 817574],
	'eu-en': [31561787, 3738856, 826064],
	'fa-en': [17141051, 3896420, 843153],
	'fi-en': [31561787, 5195132, 818770],
	'fr-en': [31561787, 4824120, 814404],
	'gl-en': [31561787, 4367596, 827023],
	'gu-en': [17141051, 4197836, 965143],
	'hbs-en': [31561787, 5155408, 817335],
	'he-en': [17141051, 4636028, 845177],
	'hi-en': [17141051, 4605984, 925436],
	'hu-en': [31561787, 4847184, 832895],
	'id-en': [17141051, 4050348, 772941],
	'is-en': [31561787, 3951884, 808315],
	'it-en': [31561787, 4713616, 812724],
	'ja-en': [43977787, 9348172, 1443222],
	'kn-en': [17141051, 4655776, 1065636],
	'ko-en': [43977787, 8617516, 1410063],
	'lt-en': [17141051, 4724124, 807623],
	'lv-en': [17141051, 4214476, 815278],
	'ml-en': [17141051, 5017972, 1109047],
	'mr-en': [31561787, 4534544, 1009482],
	'ms-en': [17141051, 4371896, 802191],
	'nb-en': [17141051, 4517876, 802367],
	'nl-en': [31561787, 4862412, 807708],
	'no-en': [17141051, 4517876, 802367],
	'pl-en': [31561787, 4964208, 821612],
	'pt-en': [31561787, 4634492, 816726],
	'ro-en': [17141051, 4691708, 800832],
	'ru-en': [31561787, 4397908, 952371],
	'sk-en': [17141051, 4913324, 808082],
	'sl-en': [31561787, 4320484, 803078],
	'sq-en': [17141051, 4256516, 820200],
	'sv-en': [17141051, 4645680, 793049],
	'ta-en': [31561787, 4940884, 1104678],
	'te-en': [17141051, 4815756, 1056197],
	'th-en': [31561787, 4487056, 995429],
	'tr-en': [17140836, 4662492, 811353],
	'uk-en': [17141051, 3914120, 886410],
	'ur-en': [31561787, 3518528, 843115],
	'vi-en': [17141051, 3910184, 757250],
	'zh-Hant-en': [43849787, 6385944, 769669, 812572],
	'zh-en': [43977787, 9219192, 1359697]
};

export type TranslateLanguage = {
	code: string;
	name: string;
	/// What its models are called: some have one model out of English but
	/// none into it, or the reverse, or share one with their neighbours.
	from?: string;
	to?: string;
	/// What language detection calls it.
	detected?: string;
	/// Written right to left.
	rtl?: boolean;
};

export const TRANSLATE_LANGUAGES: TranslateLanguage[] = [
	{ code: 'af', name: 'Afrikaans', from: 'af', to: 'af', detected: 'afr' },
	{ code: 'sq', name: 'Albanian', from: 'sq', detected: 'als' },
	{ code: 'ar', name: 'Arabic', from: 'ar', to: 'ar', detected: 'arb', rtl: true },
	{ code: 'az', name: 'Azerbaijani', to: 'az', detected: 'azj' },
	{ code: 'eu', name: 'Basque', from: 'eu', to: 'eu', detected: 'eus' },
	{ code: 'bn', name: 'Bengali', from: 'bn', to: 'bn', detected: 'ben' },
	{ code: 'bs', name: 'Bosnian', from: 'hbs', to: 'bs', detected: 'bos' },
	{ code: 'bg', name: 'Bulgarian', from: 'bg', to: 'bg', detected: 'bul' },
	{ code: 'ca', name: 'Catalan', from: 'ca', to: 'ca', detected: 'cat' },
	{ code: 'zh', name: 'Chinese (Simplified)', from: 'zh', to: 'zh', detected: 'cmn' },
	{ code: 'zh-Hant', name: 'Chinese (Traditional)', from: 'zh-Hant', to: 'zh-Hant' },
	{ code: 'hr', name: 'Croatian', from: 'hbs', to: 'hr', detected: 'hrv' },
	{ code: 'cs', name: 'Czech', from: 'cs', to: 'cs', detected: 'ces' },
	{ code: 'da', name: 'Danish', from: 'da', to: 'da', detected: 'dan' },
	{ code: 'nl', name: 'Dutch', from: 'nl', to: 'nl', detected: 'nld' },
	{ code: 'en', name: 'English', from: 'en', to: 'en', detected: 'eng' },
	{ code: 'et', name: 'Estonian', from: 'et', to: 'et', detected: 'est' },
	{ code: 'fi', name: 'Finnish', from: 'fi', to: 'fi', detected: 'fin' },
	{ code: 'fr', name: 'French', from: 'fr', to: 'fr', detected: 'fra' },
	{ code: 'gl', name: 'Galician', from: 'gl', to: 'gl', detected: 'glg' },
	{ code: 'de', name: 'German', from: 'de', to: 'de', detected: 'deu' },
	{ code: 'el', name: 'Greek', from: 'el', to: 'el', detected: 'ell' },
	{ code: 'gu', name: 'Gujarati', from: 'gu', to: 'gu', detected: 'guj' },
	{ code: 'he', name: 'Hebrew', from: 'he', to: 'he', detected: 'heb', rtl: true },
	{ code: 'hi', name: 'Hindi', from: 'hi', to: 'hi', detected: 'hin' },
	{ code: 'hu', name: 'Hungarian', from: 'hu', to: 'hu', detected: 'hun' },
	{ code: 'is', name: 'Icelandic', from: 'is', to: 'is', detected: 'isl' },
	{ code: 'id', name: 'Indonesian', from: 'id', to: 'id', detected: 'ind' },
	{ code: 'it', name: 'Italian', from: 'it', to: 'it', detected: 'ita' },
	{ code: 'ja', name: 'Japanese', from: 'ja', to: 'ja', detected: 'jpn' },
	{ code: 'kn', name: 'Kannada', from: 'kn', to: 'kn', detected: 'kan' },
	{ code: 'ko', name: 'Korean', from: 'ko', to: 'ko', detected: 'kor' },
	{ code: 'lv', name: 'Latvian', from: 'lv', to: 'lv', detected: 'lvs' },
	{ code: 'lt', name: 'Lithuanian', from: 'lt', to: 'lt', detected: 'lit' },
	{ code: 'ms', name: 'Malay', from: 'ms', to: 'ms', detected: 'zlm' },
	{ code: 'ml', name: 'Malayalam', from: 'ml', to: 'ml', detected: 'mal' },
	{ code: 'mr', name: 'Marathi', from: 'mr', to: 'mr', detected: 'mar' },
	{ code: 'nb', name: 'Norwegian', from: 'nb', to: 'nb', detected: 'nob' },
	{ code: 'fa', name: 'Persian', from: 'fa', to: 'fa', detected: 'pes', rtl: true },
	{ code: 'pl', name: 'Polish', from: 'pl', to: 'pl', detected: 'pol' },
	{ code: 'pt', name: 'Portuguese', from: 'pt', to: 'pt', detected: 'por' },
	{ code: 'ro', name: 'Romanian', from: 'ro', to: 'ro', detected: 'ron' },
	{ code: 'ru', name: 'Russian', from: 'ru', to: 'ru', detected: 'rus' },
	{ code: 'sr', name: 'Serbian', from: 'hbs', to: 'sr', detected: 'srp' },
	{ code: 'sk', name: 'Slovak', from: 'sk', to: 'sk', detected: 'slk' },
	{ code: 'sl', name: 'Slovenian', from: 'sl', to: 'sl', detected: 'slv' },
	{ code: 'es', name: 'Spanish', from: 'es', to: 'es', detected: 'spa' },
	{ code: 'sv', name: 'Swedish', from: 'sv', to: 'sv', detected: 'swe' },
	{ code: 'ta', name: 'Tamil', from: 'ta', to: 'ta', detected: 'tam' },
	{ code: 'te', name: 'Telugu', from: 'te', to: 'te', detected: 'tel' },
	{ code: 'th', name: 'Thai', from: 'th', to: 'th', detected: 'tha' },
	{ code: 'tr', name: 'Turkish', from: 'tr', to: 'tr', detected: 'tur' },
	{ code: 'uk', name: 'Ukrainian', from: 'uk', to: 'uk', detected: 'ukr' },
	{ code: 'ur', name: 'Urdu', from: 'ur', to: 'ur', detected: 'urd', rtl: true },
	{ code: 'vi', name: 'Vietnamese', from: 'vi', to: 'vi', detected: 'vie' }
];

export const SOURCE_LANGUAGES = TRANSLATE_LANGUAGES.filter((language) => language.from);
export const TARGET_LANGUAGES = TRANSLATE_LANGUAGES.filter((language) => language.to);

export function translateLanguage(code: string) {
	return TRANSLATE_LANGUAGES.find((language) => language.code === code);
}

export function translateLanguageName(code: string) {
	return translateLanguage(code)?.name ?? code;
}

/// The models that take `from` to `to`, in order; empty when they are the same.
export function modelPairs(from: string, to: string): string[] {
	const source = translateLanguage(from)?.from;
	const target = translateLanguage(to)?.to;
	if (!source || !target || from === to) return [];
	if (source === 'en') return [`en-${target}`];
	if (target === 'en') return [`${source}-en`];
	return [`${source}-en`, `en-${target}`];
}

/// The target language the reader most likely reads: their browser's, when
/// there is a model for it, else English.
export function preferredTarget(not?: string) {
	const languages = typeof navigator === 'undefined' ? [] : navigator.languages;
	for (const tag of languages) {
		const lower = tag.toLowerCase();
		const code = /^zh-(tw|hk|mo|hant)/.test(lower)
			? 'zh-Hant'
			: lower.startsWith('zh')
				? 'zh'
				: lower === 'no' || lower.startsWith('nn') || lower.startsWith('nb')
					? 'nb'
					: lower.split('-')[0];
		if (code !== not && TARGET_LANGUAGES.some((language) => language.code === code)) return code;
	}
	return not === 'en' ? 'fr' : 'en';
}

/// The language most of `text` is in, when it is one there is a model from.
export async function detectLanguage(text: string): Promise<string | undefined> {
	const { franc } = await import('franc');
	const detected = franc(text.slice(0, 6000), {
		only: SOURCE_LANGUAGES.flatMap((language) => (language.detected ? [language.detected] : []))
	});
	return SOURCE_LANGUAGES.find((language) => language.detected === detected)?.code;
}

function files(pair: string) {
	// Every pair has English on one side.
	const [from, to] = pair.startsWith('en-') ? ['en', pair.slice(3)] : [pair.slice(0, -3), 'en'];
	const tag = `${from}${to}`.replace('zh-Hant', 'zh_hant');
	const sizes = MODEL_SIZES[pair];
	const vocabs =
		sizes.length > 3 ? [`srcvocab.${tag}.spm`, `trgvocab.${tag}.spm`] : [`vocab.${tag}.spm`];
	return [`model.${tag}.intgemm.alphas.bin`, `lex.50.50.${tag}.s2t.bin`, ...vocabs].map(
		(name, index) => ({ url: `${MIRROR}/${pair}/${name}`, size: sizes[index] })
	);
}

/// How many bytes the models for these pairs are, for saying so before
/// they download.
export function modelBytes(pairs: string[]) {
	return pairs.reduce(
		(total, pair) => total + (MODEL_SIZES[pair]?.reduce((a, b) => a + b, 0) ?? 0),
		0
	);
}

const DATABASE = 'plico-translate';
const STORE = 'models';
// The revision is part of the key, so a new mirror revision fetches anew.
const modelKey = (pair: string) => `${pair}@${MIRROR.slice(-12)}`;

type StoredModel = { model: ArrayBuffer; shortlist: ArrayBuffer; vocabs: ArrayBuffer[] };

function database() {
	return new Promise<IDBDatabase>((resolve, reject) => {
		const request = indexedDB.open(DATABASE, 1);
		request.onupgradeneeded = () => request.result.createObjectStore(STORE);
		request.onsuccess = () => resolve(request.result);
		request.onerror = () => reject(request.error);
	});
}

async function withStore<T>(
	mode: IDBTransactionMode,
	use: (store: IDBObjectStore) => IDBRequest<T>
) {
	const db = await database();
	try {
		return await new Promise<T>((resolve, reject) => {
			const request = use(db.transaction(STORE, mode).objectStore(STORE));
			request.onsuccess = () => resolve(request.result);
			request.onerror = () => reject(request.error);
		});
	} finally {
		db.close();
	}
}

export type TranslateModelState =
	| { status: 'loading'; received: number; total: number }
	| { status: 'ready' }
	| { status: 'failed' };

/// The translation models on this device, and the ones on their way.
export class TranslateModels {
	states = new SvelteMap<string, TranslateModelState>();
	private pending: Record<string, Promise<void> | undefined> = {};

	/// Resolves once the pair's model is stored, fetching it first when it is not.
	/// The store is asked each time, since the Offline panel can remove it.
	ensure(pair: string): Promise<void> {
		return (this.pending[pair] ??= this.load(pair).finally(() => delete this.pending[pair]));
	}

	private async load(pair: string) {
		try {
			const stored = await withStore('readonly', (store) => store.count(modelKey(pair))).catch(
				() => 0
			);
			if (!stored) {
				this.states.set(pair, { status: 'loading', received: 0, total: modelBytes([pair]) });
				await this.download(pair);
			}
			this.states.set(pair, { status: 'ready' });
		} catch (cause) {
			this.states.set(pair, { status: 'failed' });
			throw new Error('The translation model could not be downloaded.', { cause });
		}
	}

	private async download(pair: string) {
		const total = modelBytes([pair]);
		const received = files(pair).map(() => 0);
		const report = () =>
			this.states.set(pair, {
				status: 'loading',
				received: received.reduce((a, b) => a + b, 0),
				total
			});
		// All at once, so one slow file does not hold the others.
		const buffers = await Promise.all(
			files(pair).map(async ({ url, size }, index) => {
				const response = await fetch(url);
				if (!response.ok || !response.body) throw new Error(`HTTP ${response.status}`);
				const bytes = new Uint8Array(size);
				const reader = response.body.getReader();
				let offset = 0;
				for (;;) {
					const { done, value } = await reader.read();
					if (done) break;
					if (offset + value.length > size)
						throw new Error('A model file is not the expected size.');
					bytes.set(value, offset);
					offset += value.length;
					received[index] = offset;
					report();
				}
				if (offset !== size) throw new Error('A model file is not the expected size.');
				return bytes.buffer;
			})
		);
		const [model, shortlist, ...vocabs] = buffers;
		await withStore('readwrite', (store) =>
			store.put({ model, shortlist, vocabs } satisfies StoredModel, modelKey(pair))
		);
	}

	/// The stored files of a pair `ensure` has resolved for.
	async read(pair: string) {
		const stored = await withStore<StoredModel | undefined>('readonly', (store) =>
			store.get(modelKey(pair))
		);
		if (!stored) throw new Error('The translation model is missing.');
		return stored;
	}
}

/// What a pair translates between, for the Offline panel. Some models serve
/// several languages, Bosnian, Croatian and Serbian one.
export function pairName(pair: string) {
	const [from, to] = pair.startsWith('en-') ? ['en', pair.slice(3)] : [pair.slice(0, -3), 'en'];
	const names = (side: 'from' | 'to', code: string) =>
		TRANSLATE_LANGUAGES.filter((language) => language[side] === code)
			.map((language) => language.name)
			.join(', ') || code;
	return `${names('from', from)} to ${names('to', to)}`;
}

/// The pairs stored on this device and their sizes.
export async function storedPairs() {
	const keys = await withStore('readonly', (store) => store.getAllKeys()).catch(() => []);
	const revision = modelKey('');
	return keys
		.map(String)
		.filter((key) => key.endsWith(revision))
		.map((key) => key.slice(0, -revision.length))
		.filter((pair) => MODEL_SIZES[pair])
		.map((pair) => ({ pair, size: modelBytes([pair]) }));
}

export function removePair(pair: string) {
	return withStore('readwrite', (store) => store.delete(modelKey(pair)));
}
