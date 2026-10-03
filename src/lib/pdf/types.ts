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
			operation: 'crop';
			files: ArrayBuffer[];
			passwords: string[];
			options: CropOptions;
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
	// `value` carries a count some operations report beside their file, and
	// `pictured` the pages Redact drew from a picture.
	| {
			id: number;
			ok: true;
			bytes: ArrayBuffer;
			format: PdfOutput['format'];
			value?: number;
			pictured?: PicturedPage[];
	  }
	| { id: number; ok: true; value: number }
	| { id: number; ok: true; glyphs: PageGlyphs[] }
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
/// `CropArea`, the page's text, and where each glyph's text ends in it.
export type PageGlyphs = { boxes: Float32Array; text: string; ends: Uint32Array };
