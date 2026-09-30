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
			operation: 'pdf-to-images';
			files: ArrayBuffer[];
			passwords: string[];
			options: PdfImageOptions;
	  };

export type PdfOutput = {
	bytes: Uint8Array;
	format: 'pdf' | 'jpg' | 'png' | 'zip' | 'docx' | 'pptx' | 'xlsx';
};

export type PdfWorkerResponse =
	| { id: number; ok: true; bytes: ArrayBuffer; format: PdfOutput['format'] }
	| { id: number; ok: true; value: number }
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
