export type SplitOptions =
	| { mode: 'ranges'; ranges: { from: number; to: number }[]; combine: boolean }
	| { mode: 'fixed'; interval: number };

export type CompressOptions = {
	imageQuality: number;
	maxImageDimension: number;
	removeMetadata: boolean;
	removeThumbnails: boolean;
};

export type PdfWorkerRequest =
	| { id: number; operation: 'merge'; files: ArrayBuffer[] }
	| { id: number; operation: 'split'; files: ArrayBuffer[]; options: SplitOptions }
	| {
			id: number;
			operation: 'compress';
			files: ArrayBuffer[];
			names: string[];
			options: CompressOptions;
	  };

export type PdfOutput = { bytes: Uint8Array; format: 'pdf' | 'zip' };

export type PdfWorkerResponse =
	| { id: number; ok: true; bytes: ArrayBuffer; format: PdfOutput['format'] }
	| { id: number; ok: false; error: string };
