// Where a page number or watermark lands on a page, mirroring `Stamper::draw`
// in `rust/plico-engine/src/stamps.rs` so the preview shows what the engine
// writes. Coordinates are points on the page as the reader sees it, origin at
// the bottom left, y up.

import { capHeight, textWidth, type FontFamily } from './standard-fonts';
import { needsEmbedding } from './unicode-fonts';

/// Row by row from the top left: 0 top left, 4 centre, 8 bottom right. The
/// same indices the wasm bindings take.
export type StampPosition = 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8;

export const positionNames = [
	'Top left',
	'Top center',
	'Top right',
	'Middle left',
	'Center',
	'Middle right',
	'Bottom left',
	'Bottom center',
	'Bottom right'
] as const;

/// A page's size in points as the reader sees it, after crop and rotation.
export type PreviewPage = { number: number; width: number; height: number };

export type StampMark =
	| { kind: 'text'; text: string; family: FontFamily; bold: boolean; size: number }
	/// `width` is a fraction of the page width; `aspect` is height over width.
	| { kind: 'image'; aspect: number; width: number };

export type StampPlacement = {
	position: StampPosition;
	margin: number;
	rotation: number;
	tile: boolean;
};

export type StampLayout = {
	width: number;
	height: number;
	/// Text lines relative to the mark's centre, unrotated: where each
	/// baseline starts and how wide the line is.
	lines: { text: string; x: number; y: number; width: number }[];
	centers: [number, number][];
	/// Set when tiling would need more repeats than the engine allows.
	tooDense: boolean;
};

const LEADING = 1.2;
const MAX_TILES = 1000;

export function stampLayout(
	mark: StampMark,
	pageWidth: number,
	pageHeight: number,
	placement: StampPlacement
): StampLayout {
	const texts =
		mark.kind === 'text' ? mark.text.split('\n').map((line) => line.replace(/\r$/, '')) : [];
	const embedded = mark.kind === 'text' && needsEmbedding(mark.text);
	const widths =
		mark.kind === 'text'
			? texts.map((line) => textWidth(line, mark.family, mark.bold, mark.size, embedded))
			: [];
	const cap = mark.kind === 'text' ? capHeight(mark.family, mark.bold, embedded) * mark.size : 0;
	const [width, height] =
		mark.kind === 'text'
			? [Math.max(0, ...widths), cap + LEADING * mark.size * (texts.length - 1)]
			: [pageWidth * mark.width, pageWidth * mark.width * mark.aspect];

	const radians = (placement.rotation * Math.PI) / 180;
	const [sin, cos] = [Math.sin(radians), Math.cos(radians)];
	const halfWidth = Math.abs((width / 2) * cos) + Math.abs((height / 2) * sin);
	const halfHeight = Math.abs((width / 2) * sin) + Math.abs((height / 2) * cos);
	const across = (placement.position % 3) - 1;
	const up = 1 - Math.floor(placement.position / 3);

	let centers: [number, number][];
	let tooDense = false;
	if (placement.tile) {
		const tiles = tileCenters(pageWidth, pageHeight, halfWidth, halfHeight, placement.margin);
		centers = tiles ?? [];
		tooDense = !tiles;
	} else {
		const offset = (direction: number, length: number, half: number) =>
			direction < 0
				? placement.margin + half
				: direction > 0
					? length - placement.margin - half
					: length / 2;
		centers = [[offset(across, pageWidth, halfWidth), offset(up, pageHeight, halfHeight)]];
	}

	const align = placement.tile ? 0 : across;
	const lines =
		mark.kind === 'text'
			? texts.map((text, index) => ({
					text,
					width: widths[index],
					x: align < 0 ? -width / 2 : align > 0 ? width / 2 - widths[index] : -widths[index] / 2,
					y: height / 2 - cap - index * LEADING * mark.size
				}))
			: [];
	return { width, height, lines, centers, tooDense };
}

function tileCenters(
	pageWidth: number,
	pageHeight: number,
	halfWidth: number,
	halfHeight: number,
	gap: number
): [number, number][] | undefined {
	const stepX = 2 * halfWidth + gap;
	const stepY = 2 * halfHeight + gap;
	if (stepX < 1 || stepY < 1) return undefined;
	const columns = Math.ceil((pageWidth / 2 + halfWidth) / stepX) + 1;
	const rows = Math.ceil((pageHeight / 2 + halfHeight) / stepY);
	if ((2 * columns + 1) * (2 * rows + 1) > 4 * MAX_TILES) return undefined;
	const [middleX, middleY] = [pageWidth / 2, pageHeight / 2];
	const centers: [number, number][] = [];
	for (let row = -rows; row <= rows; row++) {
		const shift = row % 2 === 0 ? 0 : 0.5;
		for (let column = -columns; column <= columns; column++) {
			const x = middleX + (column + shift) * stepX;
			const y = middleY + row * stepY;
			if (
				Math.abs(x - middleX) < middleX + halfWidth &&
				Math.abs(y - middleY) < middleY + halfHeight
			)
				centers.push([x, y]);
		}
	}
	return centers.length > MAX_TILES ? undefined : centers;
}

/// A chosen watermark image, with a URL for the preview and its upright
/// height over width.
export type StampImage = { file: File; url: string; aspect: number };
