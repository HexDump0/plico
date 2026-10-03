// The fields Fill a form shows over each page, read with pdf.js, and what to
// send the engine once they change. Values are kept by field name, as pdf.js
// and readers do: widgets sharing a name show one value. Button values are
// pdf.js's, export values rather than appearance states, so the engine is
// told which widget to turn on instead.

import type { PDFDocumentProxy } from 'pdfjs-dist';
import type { CropArea, FormFill } from './types';
import { undrawable, type FontFamily } from './standard-fonts';

/// Text, the chosen export value of a combo box, a list's chosen export
/// values, or for buttons the value of the widget turned on, or 'Off'.
export type FormValue = string | string[];

export type FormWidget = {
	/// Object number, as pdf.js names it in `id`: "41R" is 41.
	id: number;
	page: number;
	/// Left, top, right and bottom as fractions of the page as displayed.
	area: CropArea;
	name: string;
	/// What assistive technology announces: the field's tooltip, or its name.
	label: string;
	kind: 'text' | 'combo' | 'list' | 'check' | 'radio';
	required: boolean;
	multiline: boolean;
	/// One character to a cell, across `maxLength` cells.
	comb: boolean;
	/// Characters the field takes, or 0 for any number.
	maxLength: number;
	align: 'left' | 'center' | 'right';
	/// Points, or 0 to fit the field.
	size: number;
	font: FontFamily;
	bold: boolean;
	color: string;
	background: string | null;
	border: string | null;
	/// Points.
	borderWidth: number;
	options: { value: string; label: string }[];
	multiple: boolean;
	/// A combo box that also takes typed text.
	editable: boolean;
	/// What a check box or radio button gives its field when on.
	onValue: string;
};

/// A page's fields, and where everything pdf.js draws apart from the page
/// sits, so the canvases it draws them on can be placed.
export type FormPage = { widgets: FormWidget[]; drawn: { id: string; area: CropArea }[] };

export type FormFields = {
	pages: FormPage[];
	/// Each field's value when the PDF was opened, by name.
	initial: Record<string, FormValue>;
	/// Built with XFA alone, which pdf.js does not show as fields.
	xfa: boolean;
};

type PdfjsAnnotation = {
	id: string;
	subtype: string;
	rect: number[];
	annotationFlags?: number;
	fieldName?: string;
	fieldType?: string;
	fieldValue?: string | string[] | null;
	fieldFlags?: number;
	alternativeText?: string;
	readOnly?: boolean;
	required?: boolean;
	password?: boolean;
	hidden?: boolean;
	multiLine?: boolean;
	comb?: boolean;
	maxLen?: number;
	textAlignment?: number | null;
	defaultAppearanceData?: { fontSize: number; fontName: string; fontColor: Uint8ClampedArray };
	backgroundColor?: Uint8ClampedArray | null;
	borderColor?: Uint8ClampedArray | null;
	borderStyle?: { width: number };
	checkBox?: boolean;
	radioButton?: boolean;
	pushButton?: boolean;
	exportValue?: string;
	buttonValue?: string | null;
	options?: { exportValue: string | null; displayValue: string | null }[];
	combo?: boolean;
	multiSelect?: boolean;
};

const HIDDEN = 2;
const NO_VIEW = 32;
const EDIT = 1 << 18;

const css = (color: Uint8ClampedArray | null | undefined) =>
	color ? `rgb(${color[0]} ${color[1]} ${color[2]})` : null;

/// The standard font nearest a form font, by name, as the engine picks it.
export function standardFamily(name: string): { font: FontFamily; bold: boolean } {
	const lower = name.toLowerCase().replace(/^[a-z]{6}\+/, '');
	const has = (part: string) => lower.includes(part);
	const font: FontFamily =
		has('cour') || has('mono') || lower === 'cobo'
			? 'courier'
			: has('times') ||
				  lower === 'tiro' ||
				  lower === 'tibo' ||
				  (has('serif') && !has('sans')) ||
				  ['roman', 'georgia', 'garamond', 'minion', 'cambria', 'book'].some(has)
				? 'times'
				: 'helvetica';
	const bold =
		has('bold') || has('black') || has('heavy') || ['hebo', 'tibo', 'cobo'].includes(lower);
	return { font, bold };
}

export async function readFormFields(pdf: PDFDocumentProxy): Promise<FormFields> {
	const pages: FormPage[] = [];
	const initial: Record<string, FormValue> = {};
	for (let number = 1; number <= pdf.numPages; number++) {
		const page = await pdf.getPage(number);
		const viewport = page.getViewport({ scale: 1 });
		const annotations: PdfjsAnnotation[] = await page.getAnnotations();
		const areaOf = (rect: number[]): CropArea => {
			const [x0, y0] = viewport.convertToViewportPoint(rect[0], rect[1]);
			const [x1, y1] = viewport.convertToViewportPoint(rect[2], rect[3]);
			return [
				Math.min(x0, x1) / viewport.width,
				Math.min(y0, y1) / viewport.height,
				Math.max(x0, x1) / viewport.width,
				Math.max(y0, y1) / viewport.height
			];
		};
		const widgets: FormWidget[] = [];
		for (const annotation of annotations) {
			const widget = toWidget(annotation, number, areaOf(annotation.rect));
			if (!widget) continue;
			widgets.push(widget);
			const value = valueOf(annotation, widget);
			// A check box shows its own state; the field is on if any is.
			if (!(widget.name in initial) || (widget.kind === 'check' && value !== 'Off'))
				initial[widget.name] = value;
		}
		pages.push({
			widgets,
			drawn: annotations.map((annotation) => ({
				id: annotation.id,
				area: areaOf(annotation.rect)
			}))
		});
		page.cleanup();
	}
	return { pages, initial, xfa: !!pdf.isPureXfa };
}

function toWidget(annotation: PdfjsAnnotation, page: number, area: CropArea): FormWidget | null {
	const id = /^\d+R$/.test(annotation.id) ? Number.parseInt(annotation.id, 10) : NaN;
	if (
		annotation.subtype !== 'Widget' ||
		!Number.isInteger(id) ||
		!annotation.fieldName ||
		annotation.readOnly ||
		annotation.password ||
		annotation.pushButton ||
		annotation.hidden ||
		(annotation.annotationFlags ?? 0) & (HIDDEN | NO_VIEW)
	)
		return null;
	const kind =
		annotation.fieldType === 'Tx'
			? 'text'
			: annotation.fieldType === 'Ch'
				? annotation.combo
					? 'combo'
					: 'list'
				: annotation.fieldType === 'Btn' && annotation.checkBox
					? 'check'
					: annotation.fieldType === 'Btn' && annotation.radioButton
						? 'radio'
						: null;
	if (!kind) return null;
	const onValue =
		(kind === 'check' ? annotation.exportValue : annotation.buttonValue) ??
		(kind === 'check' ? 'Yes' : '');
	if (kind === 'radio' && !onValue) return null;
	const appearance = annotation.defaultAppearanceData;
	const { font, bold } = standardFamily(appearance?.fontName ?? '');
	return {
		id,
		page,
		area,
		name: annotation.fieldName,
		label:
			annotation.alternativeText?.trim() ||
			annotation.fieldName
				.split('.')
				.at(-1)
				?.replace(/\[\d+\]$/, '') ||
			annotation.fieldName,
		kind,
		required: !!annotation.required,
		multiline: !!annotation.multiLine,
		comb: !!annotation.comb,
		maxLength: annotation.maxLen ?? 0,
		align: (['left', 'center', 'right'] as const)[annotation.textAlignment ?? 0] ?? 'left',
		size: appearance?.fontSize ?? 0,
		font,
		bold,
		color: css(appearance?.fontColor) ?? '#000',
		background: css(annotation.backgroundColor),
		border: css(annotation.borderColor),
		borderWidth: annotation.borderColor ? (annotation.borderStyle?.width ?? 1) : 0,
		options: (annotation.options ?? [])
			.filter((option) => option.exportValue !== null)
			.map((option) => ({
				value: option.exportValue as string,
				label: option.displayValue ?? (option.exportValue as string)
			})),
		multiple: !!annotation.multiSelect,
		editable: !!((annotation.fieldFlags ?? 0) & EDIT),
		onValue
	};
}

function valueOf(annotation: PdfjsAnnotation, widget: FormWidget): FormValue {
	const value = annotation.fieldValue;
	switch (widget.kind) {
		case 'text':
			return typeof value === 'string' ? value : '';
		case 'combo':
			return (Array.isArray(value) ? value[0] : value) ?? '';
		case 'list':
			return Array.isArray(value) ? value : value ? [value] : [];
		default:
			return typeof value === 'string' && value === widget.onValue ? value : 'Off';
	}
}

/// Every field empty: no text, nothing chosen, every button off.
export function blankValues(fields: FormFields): Record<string, FormValue> {
	const values: Record<string, FormValue> = {};
	for (const widget of fields.pages.flatMap((page) => page.widgets))
		values[widget.name] =
			widget.kind === 'list' ? [] : widget.kind === 'check' || widget.kind === 'radio' ? 'Off' : '';
	return values;
}

const same = (a: FormValue | undefined, b: FormValue | undefined) =>
	JSON.stringify(a ?? null) === JSON.stringify(b ?? null);

/// Names of the fields whose value differs from the one the PDF opened with.
export function changedFields(fields: FormFields, values: Record<string, FormValue>) {
	return Object.keys(values).filter((name) => !same(values[name], fields.initial[name]));
}

/// What to send for every changed field: text or choices for each widget,
/// and for buttons each widget on or off, which also reaches fields that only
/// share the name.
export function formFills(fields: FormFields, values: Record<string, FormValue>): FormFill[] {
	const changed = new Set(changedFields(fields, values));
	const fills: FormFill[] = [];
	for (const widget of fields.pages.flatMap((page) => page.widgets)) {
		if (!changed.has(widget.name)) continue;
		const value = values[widget.name];
		if (widget.kind === 'check' || widget.kind === 'radio')
			fills.push({ widget: widget.id, kind: 'button', on: value === widget.onValue });
		else if (widget.kind === 'list')
			fills.push({ widget: widget.id, kind: 'choices', choices: [...(value as string[])] });
		else if (
			widget.kind === 'text' ||
			(widget.editable && !widget.options.some((option) => option.value === value))
		)
			fills.push({ widget: widget.id, kind: 'text', text: value as string });
		else
			fills.push({
				widget: widget.id,
				kind: 'choices',
				choices: value === '' ? [] : [value as string]
			});
	}
	return fills;
}

/// The first changed field that would draw a character the standard fonts
/// cannot, and that character: its text, the option a combo box shows, or
/// any of a list's options, since a list shows them all.
export function undrawableField(
	fields: FormFields,
	values: Record<string, FormValue>
): { name: string; label: string; character: string } | undefined {
	const changed = new Set(changedFields(fields, values));
	for (const widget of fields.pages.flatMap((page) => page.widgets)) {
		if (!changed.has(widget.name)) continue;
		const value = values[widget.name];
		const shown =
			widget.kind === 'text'
				? [value as string]
				: widget.kind === 'combo'
					? [widget.options.find((option) => option.value === value)?.label ?? (value as string)]
					: widget.kind === 'list'
						? widget.options.map((option) => option.label)
						: [];
		const character = shown.map((text) => undrawable(text)).find(Boolean);
		if (character) return { name: widget.name, label: widget.label, character };
	}
	return undefined;
}
