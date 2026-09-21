import {
	IconCopy,
	IconFileZip,
	IconScissors,
	IconEdit,
	IconSignature,
	IconLayoutGrid,
	IconFileExport,
	IconFileMinus,
	IconRotateClockwise,
	IconHighlight,
	IconCrop,
	IconNumbers,
	IconRubberStamp,
	IconForms,
	IconPhoto,
	IconPhotoPlus,
	IconFileTypeDocx,
	IconPresentation,
	IconTable,
	IconHtml,
	IconScan,
	IconArchive,
	IconMarkdown,
	IconTool,
	IconTextScan2,
	IconStack2,
	IconTextCaption,
	IconLanguage,
	IconLock,
	IconLockOpen,
	IconEraser,
	IconFileDiff
} from '@tabler/icons-svelte-runes';
import type { ToolId } from './tools';

export interface CatalogTool {
	id: string;
	label: string;
	icon: typeof IconCopy;
	heroTool?: ToolId;
}

const supportedToolIds = new Set([
	'merge',
	'compress',
	'split',
	'organize',
	'extract',
	'remove',
	'rotate',
	'pdf-to-jpg',
	'pdf-to-png',
	'jpg-to-pdf',
	'png-to-pdf',
	'word-to-pdf',
	'powerpoint-to-pdf',
	'excel-to-pdf',
	'pdf-to-word',
	'pdf-to-powerpoint',
	'pdf-to-excel'
]);

export function isToolSupported(id: string): boolean {
	const tool = findTool(id);
	return !!tool && supportedToolIds.has(tool.id);
}

export function findTool(id: string) {
	const normalizedId =
		id === 'pdf-to-image' || id === 'pdf-to-images'
			? 'pdf-to-jpg'
			: id === 'image-to-pdf' || id === 'images-to-pdf'
				? 'jpg-to-pdf'
				: id;
	return [
		...quickTools,
		...toolColumns.flatMap((column) => column.flatMap((group) => group.tools))
	].find((tool) => tool.id === normalizedId);
}

export function toolCategoryColor(id: string) {
	const quick = quickTools.find((tool) => tool.id === id);
	if (quick) return quick.color.replace('bg-', 'text-');
	return (
		toolColumns
			.flatMap((column) => column)
			.find((category) => category.tools.some((tool) => tool.id === id))?.color ?? 'text-brand'
	);
}

export const quickTools = [
	{ id: 'merge', label: 'Merge PDF', icon: IconCopy, color: 'bg-merge', heroTool: 'merge' },
	{
		id: 'compress',
		label: 'Compress PDF',
		icon: IconFileZip,
		color: 'bg-compress',
		heroTool: 'compress'
	},
	{ id: 'split', label: 'Split PDF', icon: IconScissors, color: 'bg-split', heroTool: 'split' },
	{ id: 'edit', label: 'Edit PDF', icon: IconEdit, color: 'bg-brand' },
	{ id: 'sign', label: 'Sign PDF', icon: IconSignature, color: 'bg-convert' }
] satisfies (CatalogTool & { color: string })[];

export const toolColumns = [
	[
		{
			name: 'Organize',
			color: 'text-merge',
			tools: [
				{ id: 'organize', label: 'Organize pages', icon: IconLayoutGrid },
				{ id: 'extract', label: 'Extract pages', icon: IconFileExport },
				{ id: 'remove', label: 'Remove pages', icon: IconFileMinus },
				{ id: 'rotate', label: 'Rotate PDF', icon: IconRotateClockwise }
			]
		},
		{
			name: 'Edit',
			color: 'text-brand',
			tools: [
				{ id: 'annotate', label: 'Annotate PDF', icon: IconHighlight },
				{ id: 'crop', label: 'Crop PDF', icon: IconCrop },
				{ id: 'page-numbers', label: 'Add page numbers', icon: IconNumbers },
				{ id: 'watermark', label: 'Add watermark', icon: IconRubberStamp },
				{ id: 'forms', label: 'PDF forms', icon: IconForms }
			]
		}
	],
	[
		{
			name: 'Convert to PDF',
			color: 'text-convert',
			tools: [
				{ id: 'jpg-to-pdf', label: 'JPG to PDF', icon: IconPhoto },
				{ id: 'png-to-pdf', label: 'PNG to PDF', icon: IconPhotoPlus },
				{ id: 'word-to-pdf', label: 'Word to PDF', icon: IconFileTypeDocx },
				{ id: 'powerpoint-to-pdf', label: 'PowerPoint to PDF', icon: IconPresentation },
				{ id: 'excel-to-pdf', label: 'Excel to PDF', icon: IconTable },
				{ id: 'html-to-pdf', label: 'HTML to PDF', icon: IconHtml },
				{ id: 'scan-to-pdf', label: 'Scan to PDF', icon: IconScan }
			]
		},
		{
			name: 'Optimize & read',
			color: 'text-compress',
			tools: [
				{ id: 'repair', label: 'Repair PDF', icon: IconTool },
				{ id: 'ocr', label: 'OCR PDF', icon: IconTextScan2 },
				{ id: 'flatten', label: 'Flatten PDF', icon: IconStack2 },
				{ id: 'summarize', label: 'Summarize PDF', icon: IconTextCaption },
				{ id: 'translate', label: 'Translate PDF', icon: IconLanguage }
			]
		}
	],
	[
		{
			name: 'Convert from PDF',
			color: 'text-split',
			tools: [
				{ id: 'pdf-to-jpg', label: 'PDF to JPG', icon: IconPhoto },
				{ id: 'pdf-to-png', label: 'PDF to PNG', icon: IconPhotoPlus },
				{ id: 'pdf-to-word', label: 'PDF to Word', icon: IconFileTypeDocx },
				{ id: 'pdf-to-powerpoint', label: 'PDF to PowerPoint', icon: IconPresentation },
				{ id: 'pdf-to-excel', label: 'PDF to Excel', icon: IconTable },
				{ id: 'pdf-to-pdfa', label: 'PDF to PDF/A', icon: IconArchive },
				{ id: 'pdf-to-markdown', label: 'PDF to Markdown', icon: IconMarkdown }
			]
		},
		{
			name: 'Security',
			color: 'text-merge',
			tools: [
				{ id: 'protect', label: 'Protect PDF', icon: IconLock },
				{ id: 'unlock', label: 'Unlock PDF', icon: IconLockOpen },
				{ id: 'redact', label: 'Redact PDF', icon: IconEraser },
				{ id: 'compare', label: 'Compare PDF', icon: IconFileDiff }
			]
		}
	]
];
