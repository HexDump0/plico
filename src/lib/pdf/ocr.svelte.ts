// Reading text from page pictures with Tesseract, on this device.
//
// Language models are fetched once (English from this site, the rest from
// jsDelivr when picked; only the model travels, never the document) and kept
// in IndexedDB, where Tesseract's worker reads them: tesseract.js 7 cannot
// take model bytes directly, since it initialises a language object under its
// data rather than its code. Pages are drawn by the PDF worker and read by a
// small pool of Tesseract workers.

import { SvelteMap } from 'svelte/reactivity';
import type { CropArea, OcrText } from './types';
import { closeOcrDocument, ocrPageImage } from './processor';

export const OCR_LANGUAGES = [
	['afr', 'Afrikaans'],
	['amh', 'Amharic'],
	['ara', 'Arabic'],
	['asm', 'Assamese'],
	['aze', 'Azerbaijani'],
	['aze_cyrl', 'Azerbaijani (Cyrillic)'],
	['bel', 'Belarusian'],
	['ben', 'Bengali'],
	['bod', 'Tibetan'],
	['bos', 'Bosnian'],
	['bul', 'Bulgarian'],
	['cat', 'Catalan'],
	['ceb', 'Cebuano'],
	['ces', 'Czech'],
	['chi_sim', 'Chinese (Simplified)'],
	['chi_tra', 'Chinese (Traditional)'],
	['chr', 'Cherokee'],
	['cym', 'Welsh'],
	['dan', 'Danish'],
	['deu', 'German'],
	['dzo', 'Dzongkha'],
	['ell', 'Greek'],
	['eng', 'English'],
	['enm', 'Middle English'],
	['epo', 'Esperanto'],
	['est', 'Estonian'],
	['eus', 'Basque'],
	['fas', 'Persian'],
	['fin', 'Finnish'],
	['fra', 'French'],
	['frk', 'German (Fraktur)'],
	['frm', 'Middle French'],
	['gle', 'Irish'],
	['glg', 'Galician'],
	['grc', 'Ancient Greek'],
	['guj', 'Gujarati'],
	['hat', 'Haitian Creole'],
	['heb', 'Hebrew'],
	['hin', 'Hindi'],
	['hrv', 'Croatian'],
	['hun', 'Hungarian'],
	['iku', 'Inuktitut'],
	['ind', 'Indonesian'],
	['isl', 'Icelandic'],
	['ita', 'Italian'],
	['ita_old', 'Old Italian'],
	['jav', 'Javanese'],
	['jpn', 'Japanese'],
	['kan', 'Kannada'],
	['kat', 'Georgian'],
	['kat_old', 'Old Georgian'],
	['kaz', 'Kazakh'],
	['khm', 'Khmer'],
	['kir', 'Kyrgyz'],
	['kor', 'Korean'],
	['kur', 'Kurdish'],
	['lao', 'Lao'],
	['lat', 'Latin'],
	['lav', 'Latvian'],
	['lit', 'Lithuanian'],
	['mal', 'Malayalam'],
	['mar', 'Marathi'],
	['mkd', 'Macedonian'],
	['mlt', 'Maltese'],
	['msa', 'Malay'],
	['mya', 'Burmese'],
	['nep', 'Nepali'],
	['nld', 'Dutch'],
	['nor', 'Norwegian'],
	['ori', 'Odia'],
	['pan', 'Punjabi'],
	['pol', 'Polish'],
	['por', 'Portuguese'],
	['pus', 'Pashto'],
	['ron', 'Romanian'],
	['rus', 'Russian'],
	['san', 'Sanskrit'],
	['sin', 'Sinhala'],
	['slk', 'Slovak'],
	['slv', 'Slovenian'],
	['spa', 'Spanish'],
	['spa_old', 'Old Spanish'],
	['sqi', 'Albanian'],
	['srp', 'Serbian'],
	['srp_latn', 'Serbian (Latin)'],
	['swa', 'Swahili'],
	['swe', 'Swedish'],
	['syr', 'Syriac'],
	['tam', 'Tamil'],
	['tel', 'Telugu'],
	['tgk', 'Tajik'],
	['tgl', 'Tagalog'],
	['tha', 'Thai'],
	['tir', 'Tigrinya'],
	['tur', 'Turkish'],
	['uig', 'Uyghur'],
	['ukr', 'Ukrainian'],
	['urd', 'Urdu'],
	['uzb', 'Uzbek'],
	['uzb_cyrl', 'Uzbek (Cyrillic)'],
	['vie', 'Vietnamese'],
	['yid', 'Yiddish']
].map(([code, name]) => ({ code, name }));

export function languageName(code: string) {
	return OCR_LANGUAGES.find((language) => language.code === code)?.name ?? code;
}

// tesseract.js keeps models in idb-keyval's default store, by this key.
const DATABASE = 'keyval-store';
const STORE = 'keyval';
const modelKey = (code: string) => `./${code}.traineddata`;

function modelUrl(code: string) {
	return code === 'eng'
		? new URL('/tessdata/eng.traineddata.gz', location.href).href
		: `https://cdn.jsdelivr.net/npm/@tesseract.js-data/${code}/4.0.0_best_int/${code}.traineddata.gz`;
}

function database() {
	return new Promise<IDBDatabase>((resolve, reject) => {
		const request = indexedDB.open(DATABASE);
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

const isGzip = (bytes: Uint8Array) => bytes[0] === 0x1f && bytes[1] === 0x8b;

async function gunzip(bytes: Uint8Array) {
	const stream = new Blob([bytes.slice().buffer])
		.stream()
		.pipeThrough(new DecompressionStream('gzip'));
	return new Uint8Array(await new Response(stream).arrayBuffer());
}

export type ModelState =
	| { status: 'loading'; received: number; total: number }
	| { status: 'ready' }
	| { status: 'failed' };

/// The language models on this device, and the ones on their way.
export class OcrModels {
	states = new SvelteMap<string, ModelState>();
	private pending: Record<string, Promise<void> | undefined> = {};

	/// Resolves once the model is stored, fetching it first when it is not.
	ensure(code: string): Promise<void> {
		if (this.states.get(code)?.status === 'ready') return Promise.resolve();
		return (this.pending[code] ??= this.load(code).finally(() => delete this.pending[code]));
	}

	private async load(code: string) {
		this.states.set(code, { status: 'loading', received: 0, total: 0 });
		try {
			const stored = await withStore('readonly', (store) => store.count(modelKey(code))).catch(
				() => 0
			);
			if (!stored) await this.download(code);
			this.states.set(code, { status: 'ready' });
		} catch (cause) {
			this.states.set(code, { status: 'failed' });
			throw new Error(`The ${languageName(code)} model could not be downloaded.`, { cause });
		}
	}

	private async download(code: string) {
		const response = await fetch(modelUrl(code));
		if (!response.ok || !response.body) throw new Error(`HTTP ${response.status}`);
		// A host that compresses the transfer reports the compressed length,
		// so the count can overrun it; it is capped where it is shown.
		const total = Number(response.headers.get('content-length')) || 0;
		const reader = response.body.getReader();
		const chunks: Uint8Array[] = [];
		let received = 0;
		for (;;) {
			const { done, value } = await reader.read();
			if (done) break;
			chunks.push(value);
			received += value.length;
			this.states.set(code, { status: 'loading', received, total });
		}
		const bytes = new Uint8Array(received);
		let offset = 0;
		for (const chunk of chunks) {
			bytes.set(chunk, offset);
			offset += chunk.length;
		}
		// Stored unpacked, so each worker skips Tesseract's own slower gunzip.
		const model = isGzip(bytes) ? await gunzip(bytes) : bytes;
		await withStore('readwrite', (store) => store.put(model, modelKey(code)));
	}
}

type Tesseract = typeof import('tesseract.js');
type TesseractWorker = Awaited<ReturnType<Tesseract['createWorker']>>;
type Block = NonNullable<
	Awaited<ReturnType<TesseractWorker['recognize']>>['data']['blocks']
>[number];

/// Where tesseract.js finds its worker and engine: this site's own copies,
/// the fastest engine build the browser runs.
async function assetPaths() {
	const [worker, detect] = await Promise.all([
		import('tesseract.js/dist/worker.min.js?url'),
		import('wasm-feature-detect')
	]);
	const core = (await detect.relaxedSimd())
		? await import('tesseract.js-core/tesseract-core-relaxedsimd-lstm.wasm.js?url')
		: (await detect.simd())
			? await import('tesseract.js-core/tesseract-core-simd-lstm.wasm.js?url')
			: await import('tesseract.js-core/tesseract-core-lstm.wasm.js?url');
	// The worker loads both through importScripts from a blob, which has no
	// base to resolve a path against.
	return {
		worker: new URL(worker.default, location.href).href,
		core: new URL(core.default, location.href).href
	};
}

type Lane = { worker: TesseractWorker; progress?: (share: number) => void };

/// Tesseract workers for one set of languages, kept between runs.
export class OcrReader {
	private lanes: Lane[] = [];
	private languages = '';
	private starting: Promise<void> | undefined;

	/// At least `count` workers ready for `languages`, whose models must be
	/// stored already.
	async prepare(languages: string[], count: number) {
		const key = languages.join('+');
		if (key !== this.languages) {
			await this.starting?.catch(() => {});
			this.destroy();
			this.languages = key;
		}
		while (this.starting) await this.starting.catch(() => {});
		if (this.lanes.length >= count) return;
		this.starting = this.start(languages, count - this.lanes.length).finally(
			() => (this.starting = undefined)
		);
		await this.starting;
	}

	private async start(languages: string[], count: number) {
		const [{ createWorker, OEM }, paths] = await Promise.all([
			import('tesseract.js'),
			assetPaths()
		]);
		const key = languages.join('+');
		const started = await Promise.all(
			Array.from({ length: count }, async () => {
				const lane = {} as Lane;
				lane.worker = await createWorker(languages, OEM.LSTM_ONLY, {
					workerPath: paths.worker,
					corePath: paths.core,
					// Models come from the store `OcrModels` fills. Should it be
					// unavailable, the worker fetches them itself from here.
					cacheMethod: 'readOnly',
					langPath: key === 'eng' ? `${location.origin}/tessdata` : '',
					logger: (message) => {
						if (message.status === 'recognizing text') lane.progress?.(message.progress);
					},
					errorHandler: () => {}
				});
				return lane;
			})
		);
		if (key !== this.languages) {
			for (const lane of started) void lane.worker.terminate();
			return;
		}
		this.lanes.push(...started);
	}

	get size() {
		return this.lanes.length;
	}

	/// The words on a grey PGM drawn at `dpi`, read by lane `index`.
	async read(
		index: number,
		image: Uint8Array,
		dpi: number,
		progress: (share: number) => void
	): Promise<Block[]> {
		const lane = this.lanes[index];
		if (!lane) throw new Error('The text reader stopped.');
		lane.progress = progress;
		try {
			await lane.worker.setParameters({ user_defined_dpi: String(dpi) });
			const { data } = await lane.worker.recognize(
				image as never,
				{},
				{ blocks: true, text: false }
			);
			return data.blocks ?? [];
		} finally {
			lane.progress = undefined;
		}
	}

	destroy() {
		for (const lane of this.lanes) void lane.worker.terminate();
		this.lanes = [];
		this.languages = '';
	}
}

/// A recognized word, with its box on the page as fractions from the top
/// left, for showing and for leaving out what the page already says.
export type OcrWord = OcrText & { box: CropArea; confidence: number };

// Tesseract calls specks of a photograph words now and then. These keep a
// word only if it is confident, or has a letter or digit and is not unsure.
const KEEP_CONFIDENT = 50;
const KEEP_READABLE = 20;
const readable = /[\p{L}\p{N}]/u;

/// Every word on the page, line by line, measured as the engine writes them.
export function pageWords(page: number, blocks: Block[], width: number, height: number) {
	const words: OcrWord[] = [];
	for (const block of blocks)
		for (const paragraph of block.paragraphs)
			for (const line of paragraph.lines) {
				const { baseline, bbox, rowAttributes } = line;
				const run = baseline.x1 - baseline.x0;
				const slope = run > 0 ? (baseline.y1 - baseline.y0) / run : 0;
				const angle = Math.atan(-slope);
				// Ascenders to descenders, which the glyphless font's ascent and
				// descent split as the line does.
				const size = rowAttributes?.rowHeight || bbox.y1 - bbox.y0;
				const kept = line.words.filter(
					(word) =>
						word.text.trim() &&
						(word.confidence >= KEEP_CONFIDENT ||
							(word.confidence >= KEEP_READABLE && readable.test(word.text)))
				);
				kept.forEach((word, index) => {
					const { x0, y0, x1, y1 } = word.bbox;
					words.push({
						page,
						text: word.text.trim(),
						left: x0 / width,
						width: (x1 - x0) / width,
						baseline: (baseline.y0 + slope * (x0 - baseline.x0)) / height,
						size: size / height,
						angle,
						space: index < kept.length - 1,
						box: [x0 / width, y0 / height, x1 / width, y1 / height],
						confidence: word.confidence
					});
				});
			}
	return words;
}

export type OcrPageState =
	{ status: 'reading'; progress: number } | { status: 'done'; words: OcrWord[] };

let documents = 0;

/// Reads `pages` of the file, as many at once as the reader has workers,
/// reporting each page as it starts, moves on and finishes.
export async function readPages(
	file: File,
	password: string,
	pages: number[],
	reader: OcrReader,
	onpage: (page: number, state: OcrPageState) => void,
	signal: AbortSignal
) {
	if (!pages.length) return;
	const document = ++documents;
	let next = 0;
	let opened: Promise<unknown> | undefined;
	const lane = async (index: number) => {
		while (next < pages.length && !signal.aborted) {
			const page = pages[next++];
			onpage(page, { status: 'reading', progress: 0 });
			// The first request carries the file; the rest wait until it has
			// opened, then read the worker's copy.
			const drawn = opened
				? opened.then(() => ocrPageImage(null, password, document, page, signal))
				: (opened = ocrPageImage(file, password, document, page, signal));
			const { image, width, height, dpi } = await drawn;
			if (signal.aborted) return;
			const blocks = await reader.read(index, image, dpi, (progress) =>
				onpage(page, { status: 'reading', progress })
			);
			if (signal.aborted) return;
			onpage(page, { status: 'done', words: pageWords(page, blocks, width, height) });
		}
	};
	try {
		await Promise.all(Array.from({ length: reader.size }, (_, index) => lane(index)));
	} finally {
		closeOcrDocument(document);
	}
}

/// The word as the engine takes it.
export function ocrText({
	page,
	text,
	left,
	width,
	baseline,
	size,
	angle,
	space
}: OcrWord): OcrText {
	return { page, text, left, width, baseline, size, angle, space };
}
