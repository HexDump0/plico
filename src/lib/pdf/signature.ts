// Turns a drawn, typed or uploaded signature into the transparent PNG the
// engine stamps, trimmed to the ink so the box on the page fits it closely.

import type { StampImage } from './stamp-layout';

/// Points in CSS pixels of the pad, with the time each was reached so the
/// line can thin where the pen moved fast, like ink does.
export type Stroke = { x: number; y: number; time: number }[];

export const signatureFonts = [
	{ id: 'great-vibes', label: 'Formal', css: "'Great Vibes', cursive" },
	{ id: 'dancing-script', label: 'Casual', css: "'Dancing Script', cursive" },
	{ id: 'homemade-apple', label: 'Pen', css: "'Homemade Apple', cursive" }
] as const;
export type SignatureFont = (typeof signatureFonts)[number]['id'];

const INK_WIDTH = 2.6;
/// Output pixels per pad pixel, so the stamped signature stays sharp when
/// printed or zoomed.
const SCALE = 4;

/// Each segment's width, eased from the last so speed changes never show as
/// steps. Slow is full width, fast thins to about 40%.
function widths(stroke: Stroke) {
	let width = INK_WIDTH;
	return stroke.map((point, index) => {
		if (index === 0) return width;
		const previous = stroke[index - 1];
		const speed =
			Math.hypot(point.x - previous.x, point.y - previous.y) /
			Math.max(1, point.time - previous.time);
		const target = INK_WIDTH * Math.max(0.4, 1 - speed * 0.35);
		width += (target - width) * 0.35;
		return width;
	});
}

/// Draws strokes through the midpoints between samples, which smooths the
/// polyline a pointer reports into the curve the hand made.
export function drawStrokes(
	context: CanvasRenderingContext2D,
	strokes: Stroke[],
	color: string,
	scale = 1
) {
	context.save();
	context.scale(scale, scale);
	context.strokeStyle = color;
	context.fillStyle = color;
	context.lineCap = 'round';
	context.lineJoin = 'round';
	for (const stroke of strokes) {
		if (stroke.length === 1) {
			context.beginPath();
			context.arc(stroke[0].x, stroke[0].y, INK_WIDTH / 2, 0, Math.PI * 2);
			context.fill();
			continue;
		}
		const size = widths(stroke);
		let [fromX, fromY] = [stroke[0].x, stroke[0].y];
		for (let index = 1; index < stroke.length; index++) {
			const point = stroke[index];
			const next = stroke[index + 1];
			const [toX, toY] = next
				? [(point.x + next.x) / 2, (point.y + next.y) / 2]
				: [point.x, point.y];
			context.beginPath();
			context.lineWidth = size[index];
			context.moveTo(fromX, fromY);
			context.quadraticCurveTo(point.x, point.y, toX, toY);
			context.stroke();
			[fromX, fromY] = [toX, toY];
		}
	}
	context.restore();
}

export async function strokesImage(strokes: Stroke[], color: string) {
	const points = strokes.flat();
	if (points.length === 0) return null;
	const pad = INK_WIDTH * 2;
	const left = Math.min(...points.map((point) => point.x)) - pad;
	const top = Math.min(...points.map((point) => point.y)) - pad;
	const right = Math.max(...points.map((point) => point.x)) + pad;
	const bottom = Math.max(...points.map((point) => point.y)) + pad;
	const canvas = document.createElement('canvas');
	canvas.width = Math.ceil((right - left) * SCALE);
	canvas.height = Math.ceil((bottom - top) * SCALE);
	const context = canvas.getContext('2d');
	if (!context) return null;
	context.translate(-left * SCALE, -top * SCALE);
	drawStrokes(context, strokes, color, SCALE);
	return toImage(canvas, 'signature.png');
}

export async function typedImage(text: string, font: SignatureFont, color: string) {
	const name = text.trim();
	if (!name) return null;
	const css = signatureFonts.find((option) => option.id === font)!.css;
	const size = 160;
	await document.fonts.load(`${size}px ${css}`, name);
	const canvas = document.createElement('canvas');
	// Script capitals and tails reach well past the em box, so draw on a
	// generous canvas and trim to the ink afterwards.
	canvas.width = Math.ceil(size * (name.length * 0.9 + 2));
	canvas.height = size * 3;
	const context = canvas.getContext('2d');
	if (!context) return null;
	context.font = `${size}px ${css}`;
	context.fillStyle = color;
	context.textBaseline = 'alphabetic';
	context.fillText(name, size, size * 2);
	return toImage(trim(canvas, size / 10), 'signature.png');
}

/// An uploaded signature, optionally with its paper made transparent so
/// only the ink lands on the page.
export async function uploadedImage(file: File, clearPaper: boolean) {
	const url = URL.createObjectURL(file);
	try {
		const element = new Image();
		element.src = url;
		await element.decode();
		if (!element.naturalWidth || !element.naturalHeight) return null;
		const canvas = document.createElement('canvas');
		canvas.width = element.naturalWidth;
		canvas.height = element.naturalHeight;
		const context = canvas.getContext('2d', { willReadFrequently: true });
		if (!context) return null;
		context.drawImage(element, 0, 0);
		if (!clearPaper) return toImage(canvas, file.name.replace(/\.\w+$/, '') + '.png');
		const image = context.getImageData(0, 0, canvas.width, canvas.height);
		const { data } = image;
		// Light paper fades out, and ink keeps its strength with a soft edge
		// between, so antialiased strokes do not get a halo.
		for (let i = 0; i < data.length; i += 4) {
			const light = Math.min(data[i], data[i + 1], data[i + 2]);
			data[i + 3] = Math.round(data[i + 3] * Math.min(1, Math.max(0, (225 - light) / 60)));
		}
		context.putImageData(image, 0, 0);
		return toImage(trim(canvas, 4), file.name.replace(/\.\w+$/, '') + '.png');
	} catch {
		return null;
	} finally {
		URL.revokeObjectURL(url);
	}
}

/// The canvas cut down to what is not transparent, plus `pad` pixels.
function trim(canvas: HTMLCanvasElement, pad: number) {
	const context = canvas.getContext('2d', { willReadFrequently: true });
	if (!context) return canvas;
	const { data, width, height } = context.getImageData(0, 0, canvas.width, canvas.height);
	let [left, top, right, bottom] = [width, height, -1, -1];
	for (let y = 0; y < height; y++) {
		for (let x = 0; x < width; x++) {
			if (data[(y * width + x) * 4 + 3] > 8) {
				if (x < left) left = x;
				if (x > right) right = x;
				if (y < top) top = y;
				bottom = y;
			}
		}
	}
	if (right < 0) return canvas;
	const [x, y] = [Math.max(0, left - pad), Math.max(0, top - pad)];
	const trimmed = document.createElement('canvas');
	trimmed.width = Math.min(width, right + pad + 1) - x;
	trimmed.height = Math.min(height, bottom + pad + 1) - y;
	trimmed.getContext('2d')?.drawImage(canvas, -x, -y);
	return trimmed;
}

async function toImage(canvas: HTMLCanvasElement, name: string): Promise<StampImage | null> {
	const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, 'image/png'));
	if (!blob) return null;
	const file = new File([blob], name, { type: 'image/png' });
	return { file, url: URL.createObjectURL(file), aspect: canvas.height / canvas.width };
}
