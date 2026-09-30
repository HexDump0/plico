export type SplitOptions =
	| { mode: 'ranges'; ranges: { from: number; to: number }[]; combine: boolean }
	| { mode: 'fixed'; interval: number };

// `source` identifies the input PDF, so one organize pass can mix pages from
// several documents. See `sources.ts`.
export type OrganizePage = { source: string; number: number; rotation: 0 | 90 | 180 | 270 };

// The worker has the files in hand and only needs their position.
export type OrganizeInstruction = { source: number; number: number; rotation: number };

export type CompressOptions = {
	imageQuality: number;
	maxImageDimension: number;
	removeMetadata: boolean;
	removeThumbnails: boolean;
};

export type ImagePdfOptions = {
	pageWidth: number;
	pageHeight: number;
	margin: number;
};

export type PdfImageOptions = {
	format: 'jpg' | 'png';
	dpi: number;
	quality: number;
	pages?: number[];
};

export type PdfWorkerRequest =
	| { id: number; operation: 'merge'; files: ArrayBuffer[]; passwords: string[] }
	| { id: number; operation: 'unlock'; files: ArrayBuffer[]; passwords: string[] }
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
	| { id: number; ok: false; error: string };
