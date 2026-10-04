/// <reference lib="webworker" />

type Request = { id: number; input: ArrayBuffer; password: string };
export type RepairResponse =
	| { id: number; ok: true; bytes: ArrayBuffer; pages: number; warnings: string[] }
	| { id: number; ok: false; error: string };

// Errors written for people; anything MuPDF throws means the file was beyond it.
class Refusal extends Error {}

self.onmessage = async (event: MessageEvent<Request>) => {
	const { id, input, password } = event.data;
	const warnings = new Set<string>();
	try {
		// MuPDF is a ~10 MB module, so it loads only once a file needs repairing.
		const mupdf = await import('mupdf');
		mupdf.setLog({
			warning: (message) => warnings.size < 500 && warnings.add(message),
			error: (message) => warnings.size < 500 && warnings.add(message)
		});
		const document = mupdf.Document.openDocument(new Uint8Array(input), 'application/pdf');
		try {
			const pdf = document.asPDF();
			if (!pdf) throw new Refusal('This file is not a PDF.');
			if (pdf.needsPassword() && !pdf.authenticatePassword(password))
				throw new Refusal('Unlock this PDF before repairing it.');
			const pages = pdf.countPages();
			if (pages === 0) throw new Refusal('This PDF is too damaged to repair.');
			// Saving keeps the file's encryption, so a repaired copy asks for the
			// same password and Unlock still has something to remove.
			const bytes = pdf.saveToBuffer('').asUint8Array().slice().buffer;
			const response: RepairResponse = { id, ok: true, bytes, pages, warnings: [...warnings] };
			self.postMessage(response, { transfer: [bytes] });
		} finally {
			document.destroy();
		}
	} catch (cause) {
		const response: RepairResponse = {
			id,
			ok: false,
			error: cause instanceof Refusal ? cause.message : 'This PDF is too damaged to repair.'
		};
		self.postMessage(response);
	}
};
