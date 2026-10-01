// PDF/A conformance check over the PDF corpus.
//
// The corpus test converts every loadable file to PDF/A-2b and keeps the
// outputs; veraPDF, the PDF Association's reference validator, then checks
// each one. The engine refusing a file is fine. A file it writes that veraPDF
// rejects is the failure this exists to count: it claims a conformance it
// does not have.
//
// Needs veraPDF (https://verapdf.org, Java). Put `verapdf` on PATH or point
// VERAPDF at the launcher. Run from the repo root:
//
// ```sh
// npm run test:pdfa
// ```

import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const output = fs.mkdtempSync(path.join(os.tmpdir(), 'plico-pdfa-'));
const verapdf = process.env.VERAPDF ?? 'verapdf';

execFileSync(
	'cargo',
	[
		'test',
		'--release',
		'--test',
		'corpus',
		'converts_every_loadable_document_to_pdfa',
		'--',
		'--ignored',
		'--nocapture'
	],
	{
		cwd: path.join(root, 'rust/plico-engine'),
		env: { ...process.env, PLICO_PDFA_OUT: output },
		stdio: 'inherit'
	}
);

const files = fs
	.readdirSync(output)
	.filter((name) => name.endsWith('.pdf'))
	.map((name) => path.join(output, name));
let report;
try {
	report = execFileSync(
		verapdf,
		[
			'--processes',
			String(os.availableParallelism()),
			'--flavour',
			'2b',
			'--format',
			'mrr',
			'--maxfailuresdisplayed',
			'0',
			...files
		],
		{ maxBuffer: 1 << 30, stdio: ['ignore', 'pipe', 'ignore'] }
	).toString();
} catch (error) {
	// veraPDF exits non-zero whenever a file fails validation.
	report = error.stdout?.toString() ?? '';
	if (!report) throw error;
}

const jobs = report.split('<job>').slice(1);
const failing = new Map();
let compliant = 0;
let unvalidated = 0;
for (const job of jobs) {
	const name = path.basename(job.match(/<name>([^<]+)<\/name>/)?.[1] ?? '?');
	if (job.includes('isCompliant="true"')) {
		compliant++;
		continue;
	}
	if (!job.includes('isCompliant="false"')) {
		unvalidated++;
		continue;
	}
	for (const [, clause, test, description] of job.matchAll(
		/<rule [^>]*clause="([^"]+)" testNumber="(\d+)" status="failed"[^>]*>\s*<description>([^<]*)/g
	)) {
		const key = `${clause}-${test}`;
		const entry = failing.get(key) ?? { description: description.trim(), files: [] };
		entry.files.push(name);
		failing.set(key, entry);
	}
}

console.log(
	`\nveraPDF: ${compliant} of ${files.length} outputs conform to PDF/A-2b, ` +
		`${files.length - compliant - unvalidated} do not, ${unvalidated} could not be validated`
);
for (const [rule, { description, files: names }] of [...failing].sort(
	(a, b) => b[1].files.length - a[1].files.length
)) {
	console.log(`${String(names.length).padStart(4)}  ${rule}  ${description.slice(0, 90)}`);
	console.log(`        ${names.slice(0, 5).join(' ')}`);
}
fs.rmSync(output, { recursive: true });
