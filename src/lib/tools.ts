import { IconRefresh, IconDownload, IconCopy, IconScissors } from '@tabler/icons-svelte-runes';

export const tools = [
	{
		id: 'convert',
		label: 'Convert',
		icon: IconRefresh,
		color: 'text-convert',
		description: 'Change your file format.'
	},
	{
		id: 'compress',
		label: 'Compress',
		icon: IconDownload,
		color: 'text-compress',
		description: 'Make your PDFs smaller.'
	},
	{
		id: 'merge',
		label: 'Merge',
		icon: IconCopy,
		color: 'text-merge',
		description: 'Bring PDFs together.'
	},
	{
		id: 'split',
		label: 'Split',
		icon: IconScissors,
		color: 'text-split',
		description: 'Separate the pages you need.'
	}
] as const;

export type ToolId = (typeof tools)[number]['id'];
