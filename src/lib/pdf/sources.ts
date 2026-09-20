// Identifies an input PDF by content metadata, so pages keep their source
// while other files are added or removed around it. Matches the duplicate
// check in `Workspace.add`.
export function sourceKey(file: File) {
	return `${file.name}\u0000${file.size}\u0000${file.lastModified}`;
}

export function pageKey(page: { source: string; number: number }) {
	return `${page.source}\u0000${page.number}`;
}

/// One color per input PDF. Written as whole class names so Tailwind's scanner
/// sees them.
export const sourceColors = [
	{
		dot: 'bg-merge',
		border: 'border-merge/40',
		strong: 'border-merge/70',
		soft: 'bg-merge/10',
		text: 'text-merge'
	},
	{
		dot: 'bg-convert',
		border: 'border-convert/40',
		strong: 'border-convert/70',
		soft: 'bg-convert/10',
		text: 'text-convert'
	},
	{
		dot: 'bg-split',
		border: 'border-split/40',
		strong: 'border-split/70',
		soft: 'bg-split/10',
		text: 'text-split'
	},
	{
		dot: 'bg-compress',
		border: 'border-compress/40',
		strong: 'border-compress/70',
		soft: 'bg-compress/10',
		text: 'text-compress'
	},
	{
		dot: 'bg-brand',
		border: 'border-brand/40',
		strong: 'border-brand/70',
		soft: 'bg-brand/10',
		text: 'text-brand'
	}
] as const;

export function sourceColor(index: number) {
	return sourceColors[index % sourceColors.length];
}
