// The Noto fonts loaded into the page, so previews draw and measure text the
// engine embeds in the same fonts it uses. Measuring reads `loaded`, so a
// layout made before the fonts arrive is made again once they have.

import type { FontFamily } from './standard-fonts';
import { facesFor, type FontFace as Face } from './unicode-fonts';

let loaded = $state(0);
// Not state: which loads have begun never changes what is drawn.
const started: Record<string, boolean> = {};

function load(face: Face) {
	if (started[face.group] || typeof document === 'undefined') return;
	started[face.group] = true;
	// A face with one weight serves bold too, as the engine draws it.
	const weights: [string, string][] = [
		['400', face.regular],
		['700', face.bold ?? face.regular]
	];
	for (const [weight, file] of weights) {
		const font = new FontFace(familyName(face), `url(/fonts/${file})`, { weight });
		document.fonts.add(font);
		font.load().then(
			() => loaded++,
			() => {}
		);
	}
}

function familyName(face: Face) {
	return `Plico ${face.group}`;
}

/// The CSS font stack the engine's fonts for `text` make, loading them.
export function fontStack(text: string, family: FontFamily) {
	const faces = facesFor(text, family);
	faces.forEach(load);
	return faces.map((face) => `'${familyName(face)}'`).join(', ');
}

let context: CanvasRenderingContext2D | null | undefined;

/// Width in points at `size`, measured in the loaded fonts.
export function embeddedWidth(text: string, family: FontFamily, bold: boolean, size: number) {
	const stack = fontStack(text, family);
	void loaded;
	context ??=
		typeof document === 'undefined' ? null : document.createElement('canvas').getContext('2d');
	if (!context) return 0;
	context.font = `${bold ? 700 : 400} 100px ${stack}`;
	return (context.measureText(text).width / 100) * size;
}

/// Resolves once the fonts that draw `text` in `family` have loaded, so it
/// measures as the engine will draw it. Laying out before then measures in
/// whatever font the browser falls back to.
export async function fontsLoaded(text: string, family: FontFamily) {
	if (typeof document === 'undefined') return;
	const stack = fontStack(text, family);
	await Promise.all(
		['400', '700'].map((weight) => document.fonts.load(`${weight} 16px ${stack}`, text))
	).catch(() => {});
}
