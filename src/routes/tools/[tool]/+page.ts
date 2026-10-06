import { error } from '@sveltejs/kit';
import { findTool, quickTools, toolColumns } from '$lib/tool-catalog';

export function load({ params }) {
	if (!findTool(params.tool)) error(404, 'This tool could not be found.');
	return { toolId: params.tool };
}

// The old names `findTool` still answers to are built too.
export function entries() {
	return [
		...quickTools.map((tool) => tool.id),
		...toolColumns.flatMap((column) =>
			column.flatMap((group) => group.tools.map((tool) => tool.id))
		),
		'pdf-to-image',
		'pdf-to-images',
		'image-to-pdf',
		'images-to-pdf'
	].map((tool) => ({ tool }));
}
