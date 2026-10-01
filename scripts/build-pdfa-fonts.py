"""Extracts the standard 14 substitutes PDF/A embeds, from URW base35.

URW base35 (https://github.com/ArtifexSoftware/urw-base35-fonts, AGPL-3.0 with
the PS-or-PDF font exception) are metric clones of Adobe's standard 14. For each
one this writes the bare CFF program, embedded as /FontFile3 /Type1C, and a
metrics file the engine reads: descriptor values, the built-in encoding, and
every glyph's advance width.

    python3 scripts/build-pdfa-fonts.py /usr/share/fonts/gsfonts

Needs fontTools. The output is committed; rerun only to change fonts.
"""

import sys
from pathlib import Path

from fontTools.ttLib import TTFont

FONTS = {
    "Helvetica": "NimbusSans-Regular",
    "Helvetica-Bold": "NimbusSans-Bold",
    "Helvetica-Oblique": "NimbusSans-Italic",
    "Helvetica-BoldOblique": "NimbusSans-BoldItalic",
    "Times-Roman": "NimbusRoman-Regular",
    "Times-Bold": "NimbusRoman-Bold",
    "Times-Italic": "NimbusRoman-Italic",
    "Times-BoldItalic": "NimbusRoman-BoldItalic",
    "Courier": "NimbusMonoPS-Regular",
    "Courier-Bold": "NimbusMonoPS-Bold",
    "Courier-Oblique": "NimbusMonoPS-Italic",
    "Courier-BoldOblique": "NimbusMonoPS-BoldItalic",
    "Symbol": "StandardSymbolsPS",
    "ZapfDingbats": "D050000L",
}

source = Path(sys.argv[1])
output = Path(__file__).resolve().parent.parent / "static" / "pdfa-fonts"
output.mkdir(parents=True, exist_ok=True)

for standard, urw in FONTS.items():
    font = TTFont(source / f"{urw}.otf")
    cff = font["CFF "]
    (output / f"{standard}.cff").write_bytes(font.reader["CFF "])

    top = cff.cff.topDictIndex[0]
    scale = 1000 / font["head"].unitsPerEm
    os2 = font["OS/2"]
    symbolic = standard in ("Symbol", "ZapfDingbats")
    italic = font["post"].italicAngle != 0
    # Fixed pitch 1, symbolic 4, non-symbolic 32, italic 64 (ISO 32000-1, 9.8.2).
    flags = (1 if font["post"].isFixedPitch else 0) | (4 if symbolic else 32) | (64 if italic else 0)
    bbox = [round(value * scale) for value in top.FontBBox]
    stem = round(getattr(top.Private, "StdVW", 80) * scale)

    lines = [
        f"FontName {top.rawDict.get('FullName', urw)}",
        f"Flags {flags}",
        f"FontBBox {' '.join(map(str, bbox))}",
        f"ItalicAngle {font['post'].italicAngle:g}",
        f"Ascent {round(os2.sTypoAscender * scale)}",
        f"Descent {round(os2.sTypoDescender * scale)}",
        f"CapHeight {round(getattr(os2, 'sCapHeight', os2.sTypoAscender) * scale)}",
        f"StemV {stem}",
    ]
    # The encoding that applies when a PDF names none. The symbolic fonts'
    # CFF encodings are empty; their real one is only in the OpenType cmap,
    # so the engine writes it out as /Differences.
    if symbolic:
        encoding = dict(sorted(font.getBestCmap().items()))
    else:
        from fontTools.encodings.StandardEncoding import StandardEncoding

        assert top.Encoding == "StandardEncoding"
        encoding = dict(enumerate(StandardEncoding))
    for code, name in encoding.items():
        if code < 256 and name and name != ".notdef":
            lines.append(f"C {code} {name}")
    widths = font["hmtx"].metrics
    for name in font.getGlyphOrder():
        lines.append(f"W {name} {round(widths[name][0] * scale)}")
    (output / f"{standard}.txt").write_text("\n".join(lines) + "\n")

(output / "README.md").write_text(
    "Standard 14 substitutes for PDF/A, extracted from URW base35 by\n"
    "`scripts/build-pdfa-fonts.py`. Copyright (URW)++ Design & Development and\n"
    "Artifex Software, AGPL-3.0 with the PS-or-PDF font exception\n"
    "(https://github.com/ArtifexSoftware/urw-base35-fonts).\n"
)
