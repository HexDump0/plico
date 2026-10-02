import type { CropArea } from './types';

/// Darker than this in any channel counts as content, so the faint haze
/// antialiasing leaves around a page edge does not.
const INK = 240;
/// The smallest page the engine will crop to, in points.
const MIN_SIDE = 3;

export const FULL_PAGE: CropArea = [0, 0, 1, 1];

/// The part of a page rendered on white that holds anything else, as
/// fractions from its top left, or undefined for a blank page.
export function contentBounds({
	data,
	width,
	height
}: {
	data: Uint8ClampedArray;
	width: number;
	height: number;
}): CropArea | undefined {
	let [left, top, right, bottom] = [width, height, -1, -1];
	for (let y = 0; y < height; y++) {
		const row = y * width * 4;
		for (let x = 0; x < width; x++) {
			const i = row + x * 4;
			// Transparent pixels show the white page underneath.
			if (data[i + 3] > 16 && (data[i] < INK || data[i + 1] < INK || data[i + 2] < INK)) {
				if (x < left) left = x;
				if (x > right) right = x;
				if (y < top) top = y;
				bottom = y;
			}
		}
	}
	if (right < 0) return undefined;
	return [left / width, top / height, (right + 1) / width, (bottom + 1) / height];
}

/// `bounds` with `padding` points added on every side of a page `width` by
/// `height` points, kept on the page and never smaller than the engine allows.
export function padArea(
	[left, top, right, bottom]: CropArea,
	padding: number,
	width: number,
	height: number
): CropArea {
	const grow = (start: number, end: number, length: number) => {
		const pad = padding / length;
		const short = Math.max(0, MIN_SIDE / length - (end - start + 2 * pad)) / 2;
		const from = Math.max(0, start - pad - short);
		const to = Math.min(1, end + pad + short);
		return [from, to] as const;
	};
	const [x0, x1] = grow(left, right, width);
	const [y0, y1] = grow(top, bottom, height);
	return [x0, y0, x1, y1];
}

export function isFullPage([left, top, right, bottom]: CropArea) {
	const edge = 1e-4;
	return left <= edge && top <= edge && right >= 1 - edge && bottom >= 1 - edge;
}

/// The area's size on a page `width` by `height` points, in inches where
/// paper is measured in inches and millimetres everywhere else.
export function cropSize([left, top, right, bottom]: CropArea, width: number, height: number) {
	const [across, down] = [(right - left) * width, (bottom - top) * height];
	if (/^en-(US|LR|MM)\b/i.test(navigator.language))
		return `${(across / 72).toFixed(1)} × ${(down / 72).toFixed(1)} in`;
	return `${Math.round((across / 72) * 25.4)} × ${Math.round((down / 72) * 25.4)} mm`;
}
