import type { FontFamily } from './standard-fonts';
import type { StampPosition } from './stamp-layout';

export type SplitOptions =
	| { mode: 'ranges'; ranges: { from: number; to: number }[]; combine: boolean }
	| { mode: 'fixed'; interval: number };

// `source` identifies the input PDF, so one organize pass can mix pages from
// several documents. See `sources.ts`.
// A blank page has `source` BLANK_SOURCE, a `number` unique among blanks, and
// its size in points as the page would be shown before `rotation`.
export type OrganizePage = {
	source: string;
	number: number;
	rotation: 0 | 90 | 180 | 270;
	size?: { width: number; height: number };
};

// The worker has the files in hand and only needs their position.
export type OrganizeInstruction = { source: number; number: number; rotation: number };

export type CompressOptions = {
	imageQuality: number;
	maxImageDimension: number;
	removeMetadata: boolean;
	removeThumbnails: boolean;
};

// A width and height of 0 fits each page to its image.
export type ImagePdfOptions = {
	pageWidth: number;
	pageHeight: number;
	margin: number;
	orientation: 'auto' | 'portrait' | 'landscape';
};

export type PdfImageOptions = {
	format: 'jpg' | 'png';
	dpi: number;
	quality: number;
	pages?: number[];
};

export type PdfWorkerRequest =
	| {
			id: number;
			operation: 'merge';
			files: ArrayBuffer[];
			passwords: string[];
			bookmarks: string[];
	  }
	| { id: number; operation: 'unlock'; files: ArrayBuffer[]; passwords: string[] }
	| {
			id: number;
			operation: 'pdfa';
			files: ArrayBuffer[];
			passwords: string[];
			part: 2 | 3;
			fontBase: string;
	  }
	| { id: number; operation: 'protection'; files: ArrayBuffer[] }
	| {
			id: number;
			operation: 'protect';
			files: ArrayBuffer[];
			passwords: string[];
			options: ProtectOptions;
	  }
	| {
			id: number;
			operation: 'split';
			files: ArrayBuffer[];
			passwords: string[];
			options: SplitOptions;
	  }
	| {
			id: number;
			operation: 'organize';
			files: ArrayBuffer[];
			passwords: string[];
			pages: OrganizeInstruction[];
			blanks: number[];
	  }
	| {
			id: number;
			operation: 'compress';
			files: ArrayBuffer[];
			passwords: string[];
			names: string[];
			options: CompressOptions;
	  }
	| {
			id: number;
			operation: 'images-to-pdf';
			files: ArrayBuffer[];
			options: ImagePdfOptions;
	  }
	| {
			id: number;
			operation: 'page-numbers';
			files: ArrayBuffer[];
			passwords: string[];
			options: PageNumberOptions;
	  }
	| {
			id: number;
			operation: 'watermark';
			// The PDF, then the watermark image when there is one.
			files: ArrayBuffer[];
			passwords: string[];
			options: WatermarkOptions;
	  }
	| {
			id: number;
			operation: 'flatten';
			files: ArrayBuffer[];
			passwords: string[];
			formsOnly: boolean;
	  }
	| {
			id: number;
			operation: 'fill-form';
			files: ArrayBuffer[];
			passwords: string[];
			options: FillFormOptions;
	  }
	| {
			id: number;
			operation: 'sign';
			// The PDF, then the signature image.
			files: ArrayBuffer[];
			passwords: string[];
			options: SignOptions;
	  }
	| {
			id: number;
			operation: 'redact';
			files: ArrayBuffer[];
			passwords: string[];
			options: RedactOptions;
	  }
	| { id: number; operation: 'redact-text'; files: ArrayBuffer[]; passwords: string[] }
	| {
			id: number;
			operation: 'edit';
			// The PDF, then each image an addition draws.
			files: ArrayBuffer[];
			passwords: string[];
			options: EditOptions;
	  }
	| {
			id: number;
			operation: 'annotate';
			// The PDF, then each image an annotation draws.
			files: ArrayBuffer[];
			passwords: string[];
			options: AnnotateOptions;
	  }
	| {
			id: number;
			operation: 'crop';
			files: ArrayBuffer[];
			passwords: string[];
			options: CropOptions;
	  }
	| {
			id: number;
			// The PDF on a page's first request, then nothing while `document`
			// stays open in the worker.
			operation: 'ocr-render';
			files: ArrayBuffer[];
			passwords: string[];
			document: number;
			page: number;
	  }
	| { id: number; operation: 'ocr-close'; files: ArrayBuffer[]; document: number }
	| {
			id: number;
			operation: 'ocr';
			files: ArrayBuffer[];
			passwords: string[];
			words: OcrText[];
	  }
	| {
			id: number;
			operation: 'pdf-to-images';
			files: ArrayBuffer[];
			passwords: string[];
			options: PdfImageOptions;
	  };

export type PdfOutput = {
	bytes: Uint8Array;
	format: 'pdf' | 'jpg' | 'png' | 'zip' | 'docx' | 'pptx' | 'xlsx' | 'md';
};

export type PdfWorkerResponse =
	// `value` carries a count some operations report beside their file,
	// `pictured` the pages Redact drew from a picture, and `covered` the pages
	// where Edit painted over what it could not take out.
	| {
			id: number;
			ok: true;
			bytes: ArrayBuffer;
			format: PdfOutput['format'];
			value?: number;
			pictured?: PicturedPage[];
			covered?: CoveredPage[];
	  }
	| { id: number; ok: true; value: number }
	| { id: number; ok: true; glyphs: PageGlyphs[] }
	// A page as recognition reads it: a grey PGM, `width` by `height` pixels
	// drawn at `dpi`.
	| { id: number; ok: true; image: ArrayBuffer; width: number; height: number; dpi: number }
	| { id: number; ok: false; error: string };

// An empty `userPassword` opens without asking; an empty `ownerPassword` is
// replaced by a random one, so the permissions cannot be lifted.
export type ProtectOptions = {
	userPassword: string;
	ownerPassword: string;
	allowPrinting: boolean;
	allowCopying: boolean;
	allowEditing: boolean;
};

/// What opening a PDF takes, as `pdf_protection` reports it.
export type Protection = 'none' | 'restricted' | 'password';

/// Text drawn with a standard PDF font; `color` is 0xRRGGBB.
export type StampText = {
	family: FontFamily;
	bold: boolean;
	size: number;
	color: number;
};

/// `pages` lists the pages to stamp, from 1; empty stamps every page.
export type PageNumberOptions = StampText & {
	template: string;
	firstNumber: number;
	pages: number[];
	position: StampPosition;
	margin: number;
	opacity: number;
};

/// An image watermark travels as the request's second file, and `imageWidth`
/// is a fraction of the page width. Without one, `text` is drawn.
export type WatermarkOptions = StampText & {
	text: string;
	imageWidth: number;
	pages: number[];
	position: StampPosition;
	margin: number;
	rotation: number;
	opacity: number;
	behind: boolean;
	tile: boolean;
};

/// Left, top, right and bottom edges as fractions of the page as it is
/// displayed, measured from its top left.
export type CropArea = [number, number, number, number];

/// `pages` lists the pages to crop, from 1. Manual crops each to the same
/// area; auto trims each to what it draws, keeping `padding` points around it.
export type CropOptions =
	| { mode: 'manual'; pages: number[]; area: CropArea }
	| { mode: 'auto'; pages: number[]; padding: number };

/// The signature goes on each page in `pages`, from 1, at `place`: its left
/// edge, top edge and width as fractions of the page as displayed, from the
/// top left. Its height follows from the image.
export type SignOptions = { pages: number[]; place: [number, number, number] };

/// A point as a fraction of the page as displayed, from its top left.
export type PagePoint = [number, number];

/// What an annotation draws. Boxes are left, top, right and bottom fractions
/// like `CropArea`; widths and sizes are in points; fills are 0xRRGGBB or
/// null. `image` indexes the images sent after the PDF.
export type AnnotationMark =
	| { kind: 'highlight' | 'underline' | 'strikeout' | 'squiggly'; boxes: CropArea[] }
	| { kind: 'ink'; strokes: PagePoint[][]; width: number }
	| { kind: 'rectangle' | 'ellipse'; area: CropArea; width: number; fill: number | null }
	| { kind: 'line' | 'arrow'; from: PagePoint; to: PagePoint; width: number }
	| {
			kind: 'text';
			area: CropArea;
			text: string;
			family: 'helvetica' | 'times' | 'courier';
			bold: boolean;
			size: number;
			fill: number | null;
	  }
	| { kind: 'note'; at: PagePoint }
	| { kind: 'image'; place: [number, number, number]; image: number };

/// `color` is 0xRRGGBB; `comment` is what readers show when it is opened.
export type PageAnnotation = AnnotationMark & {
	page: number;
	color: number;
	opacity: number;
	comment: string;
};

/// `remove` holds the object numbers of annotations already in the file to
/// delete, as pdf.js reports them in `id` ("41R" is 41). `flatten` draws the
/// annotations into the pages instead.
export type AnnotateOptions = {
	annotations: PageAnnotation[];
	remove: number[];
	flatten: boolean;
};

/// A field's new value, sent for each of its widgets by object number as
/// pdf.js reports it in `id` ("41R" is 41): text, chosen export values, or
/// whether that check box or radio button widget is on.
export type FormFill =
	| { widget: number; kind: 'text'; text: string }
	| { widget: number; kind: 'choices'; choices: string[] }
	| { widget: number; kind: 'button'; on: boolean };

/// `flatten` draws the filled fields into the pages.
export type FillFormOptions = { fills: FormFill[]; flatten: boolean };

/// One redaction box: left, top, right and bottom as fractions of the page as
/// displayed, from its top left.
export type RedactArea = { page: number; area: CropArea };

/// A box as the tool edits it.
export type RedactMark = RedactArea & { id: number };

/// `color` is 0xRRGGBB. `asImages` draws every redacted page from a picture.
export type RedactOptions = {
	areas: RedactArea[];
	color: number;
	removeMetadata: boolean;
	asImages: boolean;
};

/// A page Redact drew from a picture, and why: by choice, or because text in a
/// font that cannot be measured, an image that cannot be decoded, or content
/// that cannot be read lay under a box.
export type PicturedPage = { page: number; reason: 'chosen' | 'text' | 'image' | 'content' };

/// A page's glyphs as the engine reads them: four fractions per glyph as in
/// `CropArea`, the page's text, and where each glyph's text ends in it. Then
/// how each is drawn: font size in points and baseline as a fraction of the
/// page's height, two per glyph; fill as 0xRRGGBB, or 0xFFFFFFFF when
/// unknown; and its look, the nearest standard family (0 Helvetica, 1 Times,
/// 2 Courier) plus 4 for bold, 8 italic, 16 invisible and 32 upright. Last,
/// where each image the page draws sits, four fractions each as in `boxes`.
export type PageGlyphs = {
	boxes: Float32Array;
	text: string;
	ends: Uint32Array;
	metrics: Float32Array;
	colors: Uint32Array;
	looks: Uint8Array;
	images: Float32Array;
};

/// Text Edit takes out: whatever glyphs `area` covers, as redaction decides.
/// When they cannot be taken out, `shown`, where they reach, is painted in
/// `cover`, the colour behind them, 0xRRGGBB.
export type TextRemoval = { page: number; area: CropArea; shown: CropArea; cover: number };

/// An area Edit empties: everything under it is taken out and it is painted
/// in `fill`, 0xRRGGBB.
export type Erasure = RedactArea & { fill: number };

/// An image the page draws, from where `redaction_text` reports it, moved
/// to `to`, or taken away when `to` is null.
export type ImageEdit = { page: number; from: CropArea; to: CropArea | null };

/// `images` are moved or taken away first, then `replace` takes text out,
/// `erase` empties areas, and `additions` are drawn into the pages after
/// all of them.
export type EditOptions = {
	images: ImageEdit[];
	replace: TextRemoval[];
	erase: Erasure[];
	additions: PageAnnotation[];
};

/// A page where Edit painted over an area rather than taking out what was
/// under it, and why.
export type CoveredPage = { page: number; reason: 'text' | 'image' | 'content' };

/// A recognized word as the engine writes it under the page: `left` and
/// `width` are fractions of the visible page's width, `baseline` (where it
/// meets the left edge) and `size` (the line's height, ascenders to
/// descenders) fractions of its height, `angle` how far the baseline turns
/// counterclockwise in radians, and `space` whether a word follows on its line.
export type OcrText = {
	page: number;
	text: string;
	left: number;
	width: number;
	baseline: number;
	size: number;
	angle: number;
	space: boolean;
};
