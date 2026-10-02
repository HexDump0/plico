// What Flatten will draw into each page, read with pdf.js for the preview.
// The rules follow `flatten.rs`: links and popups stay, so do annotations that
// carry a file or media, and so does anything shown only on paper. Hidden
// annotations are removed but show nothing, so they are not marked, and nor
// is anything with no stored appearance, which the engine leaves alone. pdf.js
// makes up a look for a few annotation types and for fields a form asks it
// to redraw; the engine keeps those and says so, so the marks can overcount.

import type { PDFDocumentProxy } from 'pdfjs-dist';
import type { CropArea } from './types';

/// Where an annotation sits, as fractions of the page as displayed from its
/// top left, and whether it is a form field.
export type AnnotationMark = { area: CropArea; field: boolean };

const HIDDEN = 2;
const NO_VIEW = 32;
const STAYS = new Set([
	'Link',
	'Popup',
	'FileAttachment',
	'Sound',
	'Movie',
	'Screen',
	'3D',
	'RichMedia'
]);

/// Every page's marks, in page order.
export async function flattenMarks(pdf: PDFDocumentProxy): Promise<AnnotationMark[][]> {
	const pages: AnnotationMark[][] = [];
	for (let number = 1; number <= pdf.numPages; number++) {
		const page = await pdf.getPage(number);
		const viewport = page.getViewport({ scale: 1 });
		const annotations: {
			subtype: string;
			rect: number[];
			annotationFlags?: number;
			hasAppearance?: boolean;
		}[] = await page.getAnnotations();
		pages.push(
			annotations
				.filter(
					({ subtype, annotationFlags = 0, hasAppearance }) =>
						hasAppearance && !STAYS.has(subtype) && !(annotationFlags & (HIDDEN | NO_VIEW))
				)
				.map(({ subtype, rect }) => {
					const [x0, y0] = viewport.convertToViewportPoint(rect[0], rect[1]);
					const [x1, y1] = viewport.convertToViewportPoint(rect[2], rect[3]);
					return {
						field: subtype === 'Widget',
						area: [
							Math.min(x0, x1) / viewport.width,
							Math.min(y0, y1) / viewport.height,
							Math.max(x0, x1) / viewport.width,
							Math.max(y0, y1) / viewport.height
						]
					};
				})
		);
	}
	return pages;
}
