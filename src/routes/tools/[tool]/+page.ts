import { error } from '@sveltejs/kit';
import { findTool } from '$lib/tool-catalog';

export function load({ params }) {
	if (!findTool(params.tool)) error(404, 'This tool could not be found.');
	return { toolId: params.tool };
}
