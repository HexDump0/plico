"""Builds the font OCR text is written in: one empty glyph, half an em wide.

Recognized text is drawn invisibly (render mode 3) over the page picture, so
it needs a font that readers measure and copy from, never one they draw. Every
character maps to glyph 1 through the PDF's own CIDToGIDMap and ToUnicode,
which is what lets one tiny font carry any script. Glyph 0 is .notdef, which
PDF/A forbids text from using, hence the second glyph.

    python3 scripts/build-glyphless-font.py

Needs fontTools. The output is committed; rerun only to change the font.
"""

from pathlib import Path

from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen

UNITS = 1000
ADVANCE = 500
ASCENT = 800
DESCENT = -200

empty = TTGlyphPen(None).glyph()
builder = FontBuilder(UNITS, isTTF=True)
builder.setupGlyphOrder([".notdef", "glyph1"])
builder.setupCharacterMap({0x20: "glyph1"})
builder.setupGlyf({".notdef": empty, "glyph1": empty})
builder.setupHorizontalMetrics({".notdef": (ADVANCE, 0), "glyph1": (ADVANCE, 0)})
builder.setupHorizontalHeader(ascent=ASCENT, descent=DESCENT)
builder.setupNameTable({"familyName": "GlyphLessFont", "styleName": "Regular"})
builder.setupOS2(
    sTypoAscender=ASCENT,
    sTypoDescender=DESCENT,
    usWinAscent=ASCENT,
    usWinDescent=-DESCENT,
    sCapHeight=700,
    sxHeight=500,
)
builder.setupPost()
builder.setupHead(unitsPerEm=UNITS)

output = Path(__file__).resolve().parent.parent / "rust/plico-engine/assets/glyphless.ttf"
builder.save(output)
print(output, output.stat().st_size, "bytes")
