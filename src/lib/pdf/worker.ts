/// <reference lib="webworker" />

import { zipSync } from 'fflate';
import init, {
	compress_pdf,
	images_to_pdf,
	merge_pdfs,
	organize_pdfs,
	split_pdf_every,
	split_pdf_ranges,
	unlock_pdf,
	convert_to_pdfa,
	pdfa_standard_fonts,
	pdf_protection,
	protect_pdf,
	add_page_numbers,
	add_watermark,
	crop_pdf,
	flatten_pdf,
	sign_pdf,
	redact_pdf,
	redaction_text
} from './wasm/plico_engine.js';
import { contentBounds, padArea } from './crop-area';
import type {
	CropOptions,
	PageGlyphs,
	PdfImageOptions,
	PdfOutput,
	PdfWorkerRequest,
	PdfWorkerResponse,
	PicturedPage,
	RedactOptions
} from './types';

const ready = init();

const fontIndex = { helvetica: 0, times: 1, courier: 2 } as const;

type PackedOutput = PdfOutput;

function packageOutputs(
	parts: Uint8Array[],
	nameFor: (index: number) => string,
	singleFormat: Exclude<PdfOutput['format'], 'zip'> = 'pdf'
): PackedOutput {
	if (parts.length === 1) return { format: singleFormat, bytes: parts[0] };
	const entries: Record<string, [Uint8Array, { level: 0 }]> = {};
	parts.forEach((bytes, index) => {
		entries[nameFor(index)] = [bytes, { level: 0 }];
	});
	return { format: 'zip', bytes: zipSync(entries) };
}

class WorkerCanvasFactory {
	#enableHWA = false;
	constructor(options?: { enableHWA?: boolean }) {
		this.#enableHWA = options?.enableHWA ?? false;
	}
	create(width: number, height: number) {
		if (width <= 0 || height <= 0) throw new Error('Invalid canvas size');
		const canvas = new OffscreenCanvas(width, height);
		return {
			canvas,
			context: canvas.getContext('2d', { willReadFrequently: !this.#enableHWA })
		};
	}
	reset(
		canvasAndContext: {
			canvas: OffscreenCanvas;
			context: OffscreenCanvasRenderingContext2D | null;
		},
		width: number,
		height: number
	) {
		if (!canvasAndContext?.canvas) throw new Error('Canvas is not specified');
		if (width <= 0 || height <= 0) throw new Error('Invalid canvas size');
		canvasAndContext.canvas.width = width;
		canvasAndContext.canvas.height = height;
	}
	destroy(canvasAndContext: {
		canvas: OffscreenCanvas | null;
		context: OffscreenCanvasRenderingContext2D | null;
	}) {
		if (!canvasAndContext?.canvas) throw new Error('Canvas is not specified');
		canvasAndContext.canvas.width = 0;
		canvasAndContext.canvas.height = 0;
		canvasAndContext.canvas = null;
		canvasAndContext.context = null;
	}
	_createCanvas(width: number, height: number) {
		return new OffscreenCanvas(width, height);
	}
}

class WorkerFilterFactory {
	addFilter() {
		return 'none';
	}
	addHCMFilter() {
		return 'none';
	}
	addAlphaFilter() {
		return 'none';
	}
	addLuminosityFilter() {
		return 'none';
	}
	addKnockoutFilter() {
		return 'none';
	}
	addHighlightHCMFilter() {
		return 'none';
	}
	addSelectionHCMFilter() {
		return 'none';
	}
	addSelectionFilter() {
		return 'none';
	}
	createSelectionStyle() {
		return null;
	}
	destroy() {}
}

async function openPdf(input: ArrayBuffer, password: string) {
	const pdfjs = await import('pdfjs-dist');
	const workerUrl = await import('pdfjs-dist/build/pdf.worker.min.mjs?url');
	pdfjs.GlobalWorkerOptions.workerSrc = workerUrl.default;
	return pdfjs.getDocument({
		data: new Uint8Array(input),
		password: password || undefined,
		disableFontFace: true,
		cMapUrl: '/pdfjs/cmaps/',
		cMapPacked: true,
		standardFontDataUrl: '/pdfjs/standard_fonts/',
		CanvasFactory: WorkerCanvasFactory,
		FilterFactory: WorkerFilterFactory
	});
}

/// Renders each page small enough to stay quick, a point or so per pixel,
/// and finds what it draws. Blank pages are left out.
async function trimmedAreas(
	input: ArrayBuffer,
	password: string,
	options: Extract<CropOptions, { mode: 'auto' }>
) {
	if (!Number.isFinite(options.padding) || options.padding < 0 || options.padding > 144) {
		throw new Error('Choose a padding between 0 and 2 inches.');
	}
	if (typeof OffscreenCanvas === 'undefined') {
		throw new Error('This browser cannot find page margins.');
	}
	// pdf.js may take ownership of what it is given, and the engine still
	// needs these bytes.
	const task = await openPdf(input.slice(0), password);
	try {
		const pdf = await task.promise;
		const pages: number[] = [];
		const areas: number[] = [];
		for (const number of options.pages) {
			if (!Number.isInteger(number) || number < 1 || number > pdf.numPages) {
				throw new Error(`Pages must be between 1 and ${pdf.numPages}.`);
			}
			const page = await pdf.getPage(number);
			const size = page.getViewport({ scale: 1 });
			const viewport = page.getViewport({
				scale: Math.min(1, Math.sqrt(1_500_000 / (size.width * size.height)))
			});
			const canvas = new OffscreenCanvas(
				Math.max(1, Math.ceil(viewport.width)),
				Math.max(1, Math.ceil(viewport.height))
			);
			const context = canvas.getContext('2d', { willReadFrequently: true });
			if (!context) throw new Error('This browser cannot find page margins.');
			await page.render({
				canvas: canvas as unknown as HTMLCanvasElement,
				viewport,
				background: 'rgb(255,255,255)'
			}).promise;
			const bounds = contentBounds(context.getImageData(0, 0, canvas.width, canvas.height));
			page.cleanup();
			if (!bounds) continue;
			pages.push(number);
			areas.push(...padArea(bounds, options.padding, size.width, size.height));
		}
		if (pages.length === 0) throw new Error('These pages are blank, so there is nothing to trim.');
		return { pages, areas };
	} finally {
		await task.destroy();
	}
}

/// Pictures of `pages` as the reader sees them, with their boxes painted in,
/// for pages the engine cannot redact in place. Rendered at up to 300 DPI and
/// written as whichever of PNG and JPEG is smaller: PNG for text and line
/// art, JPEG for photographs.
async function redactionPictures(
	input: ArrayBuffer,
	password: string,
	pages: number[],
	options: RedactOptions
) {
	if (typeof OffscreenCanvas === 'undefined') {
		throw new Error('This browser cannot redact this PDF.');
	}
	const fill = `#${options.color.toString(16).padStart(6, '0')}`;
	// pdf.js may take ownership of what it is given.
	const task = await openPdf(input.slice(0), password);
	try {
		const pdf = await task.promise;
		const pictures: Uint8Array[] = [];
		for (const number of pages) {
			const page = await pdf.getPage(number);
			const size = page.getViewport({ scale: 1 });
			const viewport = page.getViewport({
				scale: Math.min(300 / 72, Math.sqrt(30_000_000 / (size.width * size.height)))
			});
			const canvas = new OffscreenCanvas(
				Math.max(1, Math.ceil(viewport.width)),
				Math.max(1, Math.ceil(viewport.height))
			);
			const context = canvas.getContext('2d');
			if (!context) throw new Error('This browser cannot redact this PDF.');
			await page.render({
				canvas: canvas as unknown as HTMLCanvasElement,
				viewport,
				background: 'rgb(255,255,255)'
			}).promise;
			page.cleanup();
			// Out to whole pixels, so no pixel under a box keeps any of what
			// it showed.
			context.fillStyle = fill;
			for (const { area } of options.areas.filter((entry) => entry.page === number)) {
				const left = Math.floor(area[0] * canvas.width);
				const top = Math.floor(area[1] * canvas.height);
				context.fillRect(
					left,
					top,
					Math.ceil(area[2] * canvas.width) - left,
					Math.ceil(area[3] * canvas.height) - top
				);
			}
			const [png, jpeg] = await Promise.all([
				canvas.convertToBlob({ type: 'image/png' }),
				canvas.convertToBlob({ type: 'image/jpeg', quality: 0.92 })
			]);
			const smaller = jpeg.size < png.size ? jpeg : png;
			pictures.push(new Uint8Array(await smaller.arrayBuffer()));
		}
		return pictures;
	} finally {
		await task.destroy();
	}
}

const unremovable = ['text', 'image', 'content'] as const;

/// Redacts in place where it can, and draws from a picture each page the
/// engine says it cannot, or every redacted page when asked to.
async function redact(input: ArrayBuffer, password: string, options: RedactOptions) {
	const bytes = new Uint8Array(input);
	const pages = Uint32Array.from(options.areas, (entry) => entry.page);
	const areas = Float32Array.from(options.areas.flatMap((entry) => entry.area));
	const run = (imagePages: number[], images: Uint8Array[]) =>
		redact_pdf(
			bytes,
			password,
			pages,
			areas,
			options.color,
			options.removeMetadata,
			Uint32Array.from(imagePages),
			images
		) as [Uint8Array | undefined, Uint32Array, Uint8Array?];

	let pictured: PicturedPage[] = options.asImages
		? [...new Set(options.areas.map((entry) => entry.page))]
				.sort((a, b) => a - b)
				.map((page) => ({ page, reason: 'chosen' }))
		: [];
	const images = (list: PicturedPage[]) =>
		redactionPictures(
			input,
			password,
			list.map((entry) => entry.page),
			options
		);
	const [first, listed, reasons] = run(
		pictured.map((entry) => entry.page),
		pictured.length ? await images(pictured) : []
	);
	if (first) return { bytes: first, pictured };
	pictured = Array.from(listed, (page, index) => ({
		page,
		reason: unremovable[reasons?.[index] ?? 2] ?? 'content'
	}));
	const [output] = run(
		pictured.map((entry) => entry.page),
		await images(pictured)
	);
	if (!output) throw new Error('Some pages of this PDF could not be redacted.');
	return { bytes: output, pictured };
}

async function exportPdfImages(
	input: ArrayBuffer,
	password: string,
	options: PdfImageOptions
): Promise<PackedOutput> {
	if (!Number.isFinite(options.dpi) || options.dpi < 36 || options.dpi > 300) {
		throw new Error('Choose a resolution between 36 and 300 DPI.');
	}
	if (!Number.isFinite(options.quality) || options.quality < 1 || options.quality > 100) {
		throw new Error('Choose a JPG quality between 1 and 100.');
	}
	if (options.format !== 'jpg' && options.format !== 'png') {
		throw new Error('Choose JPG or PNG output.');
	}
	if (typeof OffscreenCanvas === 'undefined' || !OffscreenCanvas.prototype.convertToBlob) {
		throw new Error('This browser cannot export PDF pages to images.');
	}

	const task = await openPdf(input, password);
	try {
		const pdf = await task.promise;
		const pages = options.pages ?? Array.from({ length: pdf.numPages }, (_, index) => index + 1);
		if (
			pages.length === 0 ||
			new Set(pages).size !== pages.length ||
			pages.some((page) => !Number.isInteger(page) || page < 1 || page > pdf.numPages)
		) {
			throw new Error(`Choose unique pages between 1 and ${pdf.numPages}.`);
		}

		const mime = options.format === 'jpg' ? 'image/jpeg' : 'image/png';
		const parts: Uint8Array[] = [];
		for (const number of pages) {
			const page = await pdf.getPage(number);
			const viewport = page.getViewport({ scale: options.dpi / 72 });
			const width = Math.ceil(viewport.width);
			const height = Math.ceil(viewport.height);
			if (width <= 0 || height <= 0 || width * height > 40_000_000) {
				throw new Error(`Page ${number} is too large at this resolution. Choose a lower DPI.`);
			}
			const canvas = new OffscreenCanvas(width, height);
			// PDF.js accepts OffscreenCanvas at runtime, but its public type only lists HTMLCanvasElement.
			await page.render({
				canvas: canvas as unknown as HTMLCanvasElement,
				viewport,
				background: 'rgb(255,255,255)'
			}).promise;
			const blob = await canvas.convertToBlob({
				type: mime,
				quality: options.quality / 100
			});
			if (blob.type !== mime)
				throw new Error(`${options.format.toUpperCase()} export is unsupported in this browser.`);
			parts.push(new Uint8Array(await blob.arrayBuffer()));
			page.cleanup();
		}
		return packageOutputs(
			parts,
			(index) =>
				`page-${String(pages[index]).padStart(Math.max(2, String(pdf.numPages).length), '0')}.${options.format}`,
			options.format
		);
	} finally {
		await task.destroy();
	}
}

function transferable(bytes: Uint8Array): ArrayBuffer {
	const { buffer, byteOffset, byteLength } = bytes;
	// wasm-bindgen's glue for a returned Vec<u8> ends in .slice() followed by
	// __wbindgen_free, so this view already owns its whole buffer. Copy only if
	// that stops being true, rather than transferring a buffer we share.
	if (buffer instanceof ArrayBuffer && byteOffset === 0 && byteLength === buffer.byteLength)
		return buffer;
	return bytes.slice().buffer;
}

self.onmessage = async (event: MessageEvent<PdfWorkerRequest>) => {
	const request = event.data;
	const { id } = request;
	try {
		await ready;
		if (request.operation === 'images-to-pdf') {
			const { files, options } = request;
			const lengths = Uint32Array.from(files, (file) => file.byteLength);
			const input = new Uint8Array(lengths.reduce((total, length) => total + length, 0));
			let offset = 0;
			for (const file of files) {
				input.set(new Uint8Array(file), offset);
				offset += file.byteLength;
			}
			postOutput(id, {
				format: 'pdf',
				bytes: images_to_pdf(
					input,
					lengths,
					options.pageWidth,
					options.pageHeight,
					options.margin,
					options.orientation === 'portrait' ? 1 : options.orientation === 'landscape' ? 2 : 0
				)
			});
			return;
		}

		if (request.operation === 'pdf-to-images') {
			if (request.files.length !== 1) throw new Error('Choose one PDF to convert.');
			postOutput(
				id,
				await exportPdfImages(request.files[0], request.passwords[0] ?? '', request.options)
			);
			return;
		}
		if (request.operation === 'protection') {
			const response: PdfWorkerResponse = {
				id,
				ok: true,
				value: pdf_protection(new Uint8Array(request.files[0]))
			};
			self.postMessage(response);
			return;
		}
		if (request.operation === 'protect') {
			const { options } = request;
			postOutput(id, {
				format: 'pdf',
				bytes: protect_pdf(
					new Uint8Array(request.files[0]),
					request.passwords[0] ?? '',
					options.userPassword,
					options.ownerPassword,
					options.allowPrinting,
					options.allowCopying,
					options.allowEditing
				)
			});
			return;
		}
		if (request.operation === 'page-numbers') {
			const { options } = request;
			postOutput(id, {
				format: 'pdf',
				bytes: add_page_numbers(
					new Uint8Array(request.files[0]),
					request.passwords[0] ?? '',
					Uint32Array.from(options.pages),
					options.firstNumber,
					options.template,
					options.position,
					options.margin,
					fontIndex[options.family],
					options.bold,
					options.size,
					options.color,
					options.opacity
				)
			});
			return;
		}
		if (request.operation === 'watermark') {
			const { options } = request;
			postOutput(id, {
				format: 'pdf',
				bytes: add_watermark(
					new Uint8Array(request.files[0]),
					request.passwords[0] ?? '',
					Uint32Array.from(options.pages),
					options.text,
					fontIndex[options.family],
					options.bold,
					options.size,
					options.color,
					request.files[1] ? new Uint8Array(request.files[1]) : new Uint8Array(),
					options.imageWidth,
					options.position,
					options.margin,
					options.rotation,
					options.opacity,
					options.behind,
					options.tile
				)
			});
			return;
		}
		if (request.operation === 'flatten') {
			const [bytes, kept] = flatten_pdf(
				new Uint8Array(request.files[0]),
				request.passwords[0] ?? '',
				request.formsOnly
			) as [Uint8Array, number];
			postOutput(id, { format: 'pdf', bytes }, kept);
			return;
		}
		if (request.operation === 'sign') {
			const { options } = request;
			postOutput(id, {
				format: 'pdf',
				bytes: sign_pdf(
					new Uint8Array(request.files[0]),
					request.passwords[0] ?? '',
					new Uint8Array(request.files[1]),
					Uint32Array.from(options.pages),
					Float32Array.from(options.pages.flatMap(() => options.place))
				)
			});
			return;
		}
		if (request.operation === 'redact') {
			const { bytes, pictured } = await redact(
				request.files[0],
				request.passwords[0] ?? '',
				request.options
			);
			const output = bytes.slice().buffer;
			const response: PdfWorkerResponse = { id, ok: true, bytes: output, format: 'pdf', pictured };
			self.postMessage(response, { transfer: [output] });
			return;
		}
		if (request.operation === 'redact-text') {
			const pages = redaction_text(
				new Uint8Array(request.files[0]),
				request.passwords[0] ?? ''
			) as [Float32Array, string, Uint32Array][];
			const glyphs: PageGlyphs[] = pages.map(([boxes, text, ends]) => ({ boxes, text, ends }));
			const response: PdfWorkerResponse = { id, ok: true, glyphs };
			self.postMessage(response, {
				transfer: glyphs.flatMap((page) => [page.boxes.buffer, page.ends.buffer])
			});
			return;
		}
		if (request.operation === 'crop') {
			const { options } = request;
			const password = request.passwords[0] ?? '';
			const { pages, areas } =
				options.mode === 'auto'
					? await trimmedAreas(request.files[0], password, options)
					: { pages: options.pages, areas: options.pages.flatMap(() => options.area) };
			postOutput(id, {
				format: 'pdf',
				bytes: crop_pdf(
					new Uint8Array(request.files[0]),
					password,
					Uint32Array.from(pages),
					Float32Array.from(areas)
				)
			});
			return;
		}
		if (request.operation === 'pdfa') {
			const input = new Uint8Array(request.files[0]);
			const password = request.passwords[0] ?? '';
			const names: string[] = pdfa_standard_fonts(input, password);
			const fetched = await Promise.all(
				names.map(async (name) => {
					const [program, metrics] = await Promise.all(
						[`${name}.cff`, `${name}.txt`].map(async (file) => {
							const response = await fetch(new URL(file, request.fontBase));
							if (!response.ok) throw new Error('The PDF/A fonts could not be loaded.');
							return response;
						})
					);
					return {
						program: new Uint8Array(await program.arrayBuffer()),
						metrics: await metrics.text()
					};
				})
			);
			const lengths = Uint32Array.from(fetched, (font) => font.program.byteLength);
			const programs = new Uint8Array(lengths.reduce((total, length) => total + length, 0));
			let offset = 0;
			for (const font of fetched) {
				programs.set(font.program, offset);
				offset += font.program.byteLength;
			}
			postOutput(id, {
				format: 'pdf',
				bytes: convert_to_pdfa(
					input,
					password,
					request.part,
					names,
					programs,
					lengths,
					fetched.map((font) => font.metrics)
				)
			});
			return;
		}
		if (request.operation === 'unlock') {
			postOutput(id, {
				format: 'pdf',
				bytes: unlock_pdf(new Uint8Array(request.files[0]), request.passwords[0] ?? '')
			});
			return;
		}
		if (request.operation === 'merge') {
			const { files } = request;
			const lengths = Uint32Array.from(files, (file) => file.byteLength);
			const input = new Uint8Array(lengths.reduce((total, length) => total + length, 0));
			let offset = 0;
			for (const file of files) {
				input.set(new Uint8Array(file), offset);
				offset += file.byteLength;
			}

			const output = transferable(merge_pdfs(input, lengths, request.passwords, request.bookmarks));
			const response: PdfWorkerResponse = { id, ok: true, bytes: output, format: 'pdf' };
			self.postMessage(response, { transfer: [output] });
			return;
		}

		if (request.operation === 'split') {
			if (request.files.length !== 1) throw new Error('Choose one PDF to split.');
			const input = new Uint8Array(request.files[0]);
			const password = request.passwords[0] ?? '';
			const { options } = request;
			const parts =
				options.mode === 'ranges'
					? (split_pdf_ranges(
							input,
							password,
							Uint32Array.from(options.ranges.flatMap(({ from, to }) => [from, to])),
							options.combine
						) as Uint8Array[])
					: (split_pdf_every(input, password, options.interval) as Uint8Array[]);
			if (parts.length === 0) throw new Error('No PDF pages were produced.');

			const packed = packageOutputs(parts, (index) => {
				const digits = Math.max(2, String(parts.length).length);
				const prefix = `part-${String(index + 1).padStart(digits, '0')}`;
				return options.mode === 'ranges'
					? options.ranges[index].from === options.ranges[index].to
						? `${prefix}-page-${options.ranges[index].from}.pdf`
						: `${prefix}-pages-${options.ranges[index].from}-${options.ranges[index].to}.pdf`
					: `${prefix}.pdf`;
			});
			postOutput(id, packed);
			return;
		}

		if (request.operation === 'organize') {
			const { files, pages } = request;
			const lengths = Uint32Array.from(files, (file) => file.byteLength);
			const input = new Uint8Array(lengths.reduce((total, length) => total + length, 0));
			let offset = 0;
			for (const file of files) {
				input.set(new Uint8Array(file), offset);
				offset += file.byteLength;
			}
			postOutput(id, {
				format: 'pdf',
				bytes: organize_pdfs(
					input,
					lengths,
					request.passwords,
					Uint32Array.from(
						pages.flatMap(({ source, number, rotation }) => [source, number, rotation])
					),
					Float32Array.from(request.blanks)
				)
			});
			return;
		}

		const { files, options } = request;
		const parts = files.map((file, index) =>
			compress_pdf(
				new Uint8Array(file),
				request.passwords[index] ?? '',
				options.imageQuality,
				options.maxImageDimension,
				options.removeMetadata,
				options.removeThumbnails
			)
		);
		if (parts.length === 0) throw new Error('No PDF was produced.');

		const packed = packageOutputs(parts, (index) => {
			const digits = Math.max(2, String(parts.length).length);
			const base =
				request.names[index]
					?.replace(/\.pdf$/i, '')
					.replace(/[^\p{L}\p{N}._ -]/gu, '_')
					.slice(0, 100) || 'document';
			return `part-${String(index + 1).padStart(digits, '0')}-${base}-compressed.pdf`;
		});
		postOutput(id, packed);
	} catch (error) {
		const response: PdfWorkerResponse = {
			id,
			ok: false,
			error: error instanceof Error ? error.message : String(error)
		};
		self.postMessage(response);
	}
};

function postOutput(id: number, packed: PackedOutput, value?: number) {
	const output = packed.bytes.slice().buffer;
	const response: PdfWorkerResponse = { id, ok: true, bytes: output, format: packed.format, value };
	self.postMessage(response, { transfer: [output] });
}
