export function parsePageRange(input: string, maxPages: number): number[] | undefined {
	if (!input.trim() || maxPages <= 0) return undefined;
	const pages: number[] = [];
	for (const part of input.split(',')) {
		const trimmed = part.trim();
		if (!trimmed) continue;
		if (trimmed.includes('-')) {
			const [startStr, endStr] = trimmed.split('-');
			const start = parseInt(startStr?.trim() ?? '', 10);
			const end = parseInt(endStr?.trim() ?? '', 10);
			if (Number.isInteger(start) && Number.isInteger(end) && start >= 1 && end >= start) {
				for (let p = start; p <= Math.min(end, maxPages); p++) {
					if (!pages.includes(p)) pages.push(p);
				}
			}
		} else {
			const p = parseInt(trimmed, 10);
			if (Number.isInteger(p) && p >= 1 && p <= maxPages) {
				if (!pages.includes(p)) pages.push(p);
			}
		}
	}
	return pages.length > 0 ? pages.sort((a, b) => a - b) : undefined;
}
