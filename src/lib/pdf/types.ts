export type SplitOptions =
	| { mode: 'ranges'; ranges: { from: number; to: number }[]; combine: boolean }
	| { mode: 'fixed'; interval: number };

export type OrganizePage = { number: number; rotation: 0 | 90 | 180 | 270 };

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
	| { id: number; operation: 'merge'; files: ArrayBuffer[] }
	| { id: number; operation: 'split'; files: ArrayBuffer[]; options: SplitOptions }
	| { id: number; operation: 'organize'; files: ArrayBuffer[]; pages: OrganizePage[] }
	| {
			id: number;
			operation: 'compress';
			files: ArrayBuffer[];
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
			options: PdfImageOptions;
	  };

export type PdfOutput = {
	bytes: Uint8Array;
	format: 'pdf' | 'jpg' | 'png' | 'zip' | 'docx' | 'pptx' | 'xlsx';
};

export type PdfWorkerResponse =
	| { id: number; ok: true; bytes: ArrayBuffer; format: PdfOutput['format'] }
	| { id: number; ok: false; error: string };
