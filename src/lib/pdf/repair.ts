import { pdfCondition } from './processor';
import type { RepairResponse } from './repair-worker';

type Repaired = Extract<RepairResponse, { ok: true }>;

export type Repair = {
	file: File;
	/// What MuPDF had to fix, in words; empty when it found nothing wrong.
	findings: string[];
	/// Whether the engine could not open the file as it was chosen.
	damaged: boolean;
};

// MuPDF reports what it fixes as warnings, many per file and repeated; these
// group them into what changed.
const kinds: [RegExp, (count: number) => string][] = [
	[/repair|xref|startxref|object offset/i, () => 'Rebuilt the cross-reference table'],
	[/version marker|PDF header/i, () => 'Fixed the file header'],
	[/stream length/i, () => 'Corrected stream lengths'],
	[/'endobj'|'endstream'|not closed/i, () => 'Closed unfinished objects'],
	[
		/ignoring (broken|object with invalid)/i,
		(count) => `Left out ${count} damaged ${count === 1 ? 'object' : 'objects'}`
	],
	[/treating as end of file|truncated/i, () => 'Recovered cut-off data']
];

export function describeRepair(warnings: string[]) {
	return kinds.flatMap(([pattern, describe]) => {
		const count = warnings.filter((warning) => pattern.test(warning)).length;
		return count ? [describe(count)] : [];
	});
}

/// Errors the engine gives when it cannot open a file at all, which a repair
/// may fix. `load_document` words every one of them this way.
export function isDamagedPdfError(error: unknown) {
	return error instanceof Error && /^PDF \d+ (could not be read|has no pages)/.test(error.message);
}

export async function repairPdf(
	file: File,
	password: string,
	signal?: AbortSignal
): Promise<Repair> {
	const cancelled = () => new DOMException('The operation was cancelled.', 'AbortError');
	if (signal?.aborted) throw cancelled();
	const damaged = (await pdfCondition(file, password, signal)) === 0;
	const input = await file.arrayBuffer();
	if (signal?.aborted) throw cancelled();
	const worker = new Worker(new URL('./repair-worker.ts', import.meta.url), { type: 'module' });
	const repaired = await new Promise<Repaired>((resolve, reject) => {
		const abort = () => finish(cancelled());
		function finish(error?: Error, output?: Repaired) {
			signal?.removeEventListener('abort', abort);
			worker.terminate();
			if (error) reject(error);
			else if (output) resolve(output);
		}
		signal?.addEventListener('abort', abort, { once: true });
		worker.onerror = () => finish(new Error('The local repair engine stopped unexpectedly.'));
		worker.onmessageerror = () =>
			finish(new Error('The local repair engine returned an unreadable result.'));
		worker.onmessage = (event: MessageEvent<RepairResponse>) => {
			if (event.data.id !== 1) return;
			if (!event.data.ok) finish(new Error(event.data.error));
			else finish(undefined, event.data);
		};
		worker.postMessage({ id: 1, input, password }, { transfer: [input] });
	});
	const copy = new File([repaired.bytes], file.name, {
		type: 'application/pdf',
		lastModified: file.lastModified
	});
	// MuPDF reads more than lopdf does; a copy only MuPDF can open is no repair.
	if ((await pdfCondition(await copy.arrayBuffer(), password, signal)) === 0)
		throw new Error('This PDF is too damaged to repair.');
	const findings = describeRepair(repaired.warnings);
	// Some files MuPDF reads without complaint still trip lopdf; rewriting
	// them is the repair (bug1539074.pdf in the pdf.js corpus).
	if (damaged && findings.length === 0) findings.push('Rewrote the file structure');
	return { file: copy, findings, damaged };
}
