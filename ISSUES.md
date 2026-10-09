# Issues

Known problems in Plico, ordered by how much they hurt. The engine section is
the priority: it is the part that can corrupt a user's document silently.

Evidence comes from two places. `cargo test` runs 124 unit tests against
generated fixtures. `npm run test:corpus` merges every usable file in the pdf.js
test corpus (982 files) with a generated page, then checks a one-page split and
compression of every loadable file. Numbers below are from that run.

---

## Fixed

Recorded here because each one is a trap worth not falling into twice. All have
a regression test.

### Inherited page attributes were dropped

A page may leave out `/Resources`, `/MediaBox`, `/CropBox` and `/Rotate` and
inherit them from an ancestor in the page tree (ISO 32000-1, 7.7.3.4). Merging
reparents every page under one new root, so those ancestors go away.

The old code kept the _first_ input's page tree node and hung every page off it.
A page from the second input then read the first input's `/MediaBox`. Two A4
documents merged fine. An A4 and a US Letter silently resized half the result.
When the surviving node had no `/MediaBox` at all, appended pages ended up with
no resolvable `/MediaBox`, which is a required entry, and readers fall back to
whatever default they like.

`/Resources` going missing is worse than geometry: font and image lookups fail,
so the page renders blank.

Fixed by resolving all four attributes up each page's original `/Parent` chain
and writing them onto the page before it moves. Tests:
`keeps_each_page_geometry_when_inherited`, `keeps_inherited_resources`,
`every_page_defines_its_own_mediabox`.

### A dangling reference could delete every page in a document

lopdf's `renumber_objects_with` rewrites references using a map built from the
objects that exist. A reference to an object that was never written is not in
that map, so it is left alone while every real object moves underneath it. After
compaction it points at whatever now occupies that number.

`testing/pdfjs/test/pdfs/issue7229.pdf` has a page tree with
`/Kids [3 0 R 8 0 R]` where object 3 does not exist. Renumbering moved the page
tree itself onto number 3, so the page tree listed itself as its own first kid.
`get_pages()` walked into the cycle and returned nothing, and the document
contributed zero pages to the merge. No error, one page quietly gone.

Fixed by renumbering in `renumber()` rather than delegating, and pointing
unresolvable references at a reserved id that is never populated. That keeps
them dangling, which is what they were, and which the spec reads as null. Test:
`survives_a_page_tree_kid_that_does_not_exist`.

### Page order rested on undocumented behaviour

Pages were collected into a `BTreeMap` keyed by object id, so `/Kids` came out in
object id order. Page order and object id order are different sequences and
nothing in the spec ties them together.

It happened to work, because `renumber_objects_with` has an undocumented phase
that permutes page objects so page N holds the Nth smallest page id. Correct
output resting on an lopdf internal with no test covering it. Page order now
comes from `get_pages()` and is carried in a `Vec`. Test:
`keeps_page_order_when_ids_run_backwards`.

### Unreachable objects were never collected

Dropping each input's catalog and page tree orphaned everything only those
referenced. Nothing pruned them, so they were written to the output. A minimal
two page merge carried 2 unreachable objects out of 9.

`write_document` now calls `prune_objects()`, which sweeps from the trailer, so
it has to run after `/Root` is set. Test: `drops_unreachable_objects`.

### Permission-restricted PDFs were refused

The old check rejected anything with an `/Encrypt` dictionary. Many such files
only restrict permissions and open under an empty user password, which is what
every viewer does without asking. Plico told users to unlock a file their reader
opens instantly.

lopdf 0.44 already tries the empty password while loading and drops `/Encrypt`
when it works. The `decrypt("")` fallback that used to follow the load was dead
code: a document lopdf cannot decrypt loads as nothing but its `/Encrypt`
dictionary, so there is nothing left to decrypt afterwards. A password has to go
into `LoadOptions`. Test: `opens_a_permissions_only_pdf_without_asking`.

### An owner password decrypted every object into garbage

For revisions 2 to 4 lopdf authenticates an owner password and then derives the
file key from it as if it were the user password. The load succeeds, every
stream decrypts to noise, and the output still parses: split with the owner
password of a generated RC4 file produced pages of random bytes. `load_document`
now recovers the user password from the owner password (ISO 32000-1, 7.6.3.4,
algorithm 7) and hands lopdf that instead. Revisions 5 and 6 were already right.
Tests: `accepts_the_owner_password_for_split_and_organize`,
`accepts_the_owner_password_of_a_revision_2_pdf`.

### lopdf's AES-256 files were unreadable in poppler

`EncryptionVersion::V5` writes an `/Encrypt` dictionary without `/Length 256`
and a crypt filter without `/Length 32`. pdf.js and qpdf infer them; poppler
falls back to a short key and every stream decrypts to noise, with lopdf itself
reading the file back perfectly. `protect_pdf_bytes` adds both entries after
encrypting. Test: `protects_with_aes_256_and_opens_only_with_a_password`, plus
a manual check: 38 protected corpus files give poppler the same text as their
originals and qpdf no new warnings.

### Writing undid the dangling reference fix

`renumber()` keeps a reference to a missing object dangling by pointing it at a
reserved id past every object. `write_document` then compacted ids with lopdf's
`renumber_objects()`, which leaves references it has no mapping for alone, so
the reserved id was reused and the reference resolved to whatever landed there.
A page whose `/Annots` named a missing object came out of a merge annotated by
a content stream from the other file. Merge, Split and Organize all wrote
through this path. The final compaction now uses `renumber()` too, which also
remaps every trailer reference rather than only `/Root`. Test:
`a_reference_to_a_missing_object_stays_dangling_when_written`.

The same run found `scripts/raster-compare.mjs` calling `merge_pdfs` without the
arguments it gained for passwords and bookmarks. Every merge threw, was counted
as refused, and the harness passed while comparing nothing. Re-measured after
the fix: the same 9 failures listed below.

### Objects added in place could answer a dangling reference

`renumber()` keeps merges safe, but the tools that edit in place (stamps,
sign, flatten, redact, PDF/A, annotate) add objects without renumbering, and
lopdf numbers new objects from `max_id`, which counts only objects that
exist. In `ZapfDingbats.pdf` two popups name parents (ids 60 and 61) that
were never written; annotating gave them the new highlight and its
appearance stream as parents. `load_document` now raises `max_id` past every
referenced id, so dangling references stay dangling. Corpus baselines did not
move. Test: `objects_added_in_place_never_answer_a_reference_to_a_missing_one`.

### Identical resources were stored once per copy

Merging ten files made from one template embedded the template's fonts and
images ten times. `share_identical_objects` now makes identical streams and
identical font, font descriptor, graphics state and encoding dictionaries one
object, repeating until nothing changes, since two fonts only match once their
font files have become one. Pages, annotations and fields are never shared.
Digest matches are compared byte for byte. Merge, Split, Organize and Compress
all write through it; it replaced Compress's single-pass stream dedupe. Ten
copies of `160F-2019.pdf` merged went from 9.9 to 1.8 times one copy's size,
`tracemonkey.pdf` from 9.7 to 5.4. Corpus merge output went from 87.1% to
85.8% of input, compress from 67.2% to 67.1%. Merge, Split and Organize then
started writing object streams, which Compress and the in-place tools
already did, taking merge output to 77.3%. Test:
`merging_copies_of_one_document_stores_their_shared_objects_once`.

### Inherited resources were copied onto every page

A page tree node's direct `/Resources` dictionary was cloned onto each page
under it when pages took their inherited attributes, which would have
inflated a long document badly. It is now moved into one object first and
the pages share the reference. Test:
`inherited_resources_are_shared_by_reference_not_copied_onto_each_page`.

### Reference rewriting recursed

Rewriting references walked nested arrays and dictionaries recursively, so a
deep enough nesting could overflow the wasm stack, a trap rather than an
error. `rewrite_references` walks with a stack instead.

### lopdf loaded some damaged files with part of a page missing

Five corpus files load without an error and without part of a page, and every
tool wrote that page out blank or shifted: a stream with no `/Length` is left
empty (`operator-in-TJ-array.pdf`, `issue1293r.pdf`); one whose `/Length` is
too short fails to parse and lopdf keeps only its dictionary
(`xobject-image.pdf`, whose page contents became `<</Length 14>>`); an object
that fails to parse is left out, so what names it reads null
(`issue11549_reduced.pdf`, fonts with a space in `/BaseFont`); and a failed
decryption is ignored, leaving ciphertext as the content (`issue7665.pdf`,
AES-256 with `/CF` as a reference). `load_document` now looks for all four
and calls the file unreadable (`lost_in_loading`), which sends it to Repair.
`issue7229.pdf` is refused the same way now, since its xref lists the missing
kid as in use.

MuPDF's copies of `xobject-image.pdf`, `operator-in-TJ-array.pdf`,
`issue1293r.pdf` and `issue7229.pdf` (both pages) load in full. MuPDF will not
save `issue11549_reduced.pdf`, and its copy of `issue7665.pdf` keeps the same
encryption, so those two are refused outright. Checked on those six files
only; the corpus baselines have not been re-run, and will move: more files
refused, fewer raster failures. Tests:
`refuses_files_lopdf_loads_with_part_of_a_page_missing`,
`opens_a_file_whose_only_broken_object_is_unused`,
`refuses_an_encrypted_file_whose_streams_do_not_decrypt`.

### Smaller ones

`Document::load_mem` applied no decompression limit, so a small file could
inflate without bound before any Plico code ran. Now capped per stream at
256 MiB via `LoadOptions`.

Version selection compared strings, which ranks `1.10` below `1.5`. Now compares
`(major, minor)`. This was the `// TODO handle 1.9+` in the old code.

`output.max_id` was set to `objects.len()`, a count rather than a maximum. It was
harmless only because the following `renumber_objects()` recomputed it.

The catalog now loses `/StructTreeRoot`, `/MarkInfo`, `/PageLabels`, `/Metadata`
and `/Version` along with `/Outlines`. Each of those described the first input's
page set alone and became actively wrong once other pages were appended, rather
than merely incomplete. Stale XMP claiming PDF/A conformance is the clearest
case.

Compress left very large JPEGs untouched. A 9000 × 12000 phone photo decodes
to 324 MB, past the 256 MiB per-image cap, so a 26.3 MB PDF of two such pages
came out 0.1 MB smaller. JPEGs that are being scaled down now decode at 1/2,
1/4 or 1/8 size inside the inverse DCT (`jpeg-decoder`'s `scale`; zune-jpeg has
no reduced decode), then box-filter to the target: the same file is 1.0 MB at
Balanced and 0.3 MB at Strong. Progressive files still need their full-size
coefficients, so the cap still applies to those, and to images kept at full
size. The second decoder costs 20.6 KB of brotli wasm.

---

## Open, engine correctness

### Text outside Western European letters is drawn in bundled Noto fonts

Page numbers, watermarks, annotation text boxes, form values and Edit's text
use the standard 14 fonts when WinAnsiEncoding covers the whole piece of text,
so that output is unchanged and embeds nothing. Anything else is set in Noto
fonts from `static/fonts/` (`text.rs`): shaped with rustybuzz, reordered for
right-to-left scripts with unicode-bidi, and embedded as Type0 subsets with a
ToUnicode map. Checked by one unit test (Cyrillic, Hebrew and kanji: two
TrueType subsets, one CFF, ToUnicode for each) and a poppler render and text
extraction of Cyrillic, kanji, Arabic and Devanagari on `tracemonkey.pdf`. No
corpus harness has been run with embedded fonts.

Limits, each deliberate for now:

- Scripts: Latin, Greek, Cyrillic, Arabic, Hebrew, Devanagari, Bengali, Tamil,
  Gujarati, Kannada, Malayalam, Telugu, Thai and Chinese, Japanese and Korean.
  Arrows, maths and box shapes (U+2190 to U+25FF), which Noto Sans and Serif
  lack, come from Noto Sans Mono in every family (the `symbols` face,
  2026-10-05); before that, a watermark with "→" passed the page's check and
  was refused by the engine.
  Other characters (emoji, Ethiopic, Georgian, the other Indic scripts) are refused by name ("“😀” cannot be drawn
  with the fonts Plico has."). Adding a script is a font file plus a row in
  `src/lib/pdf/unicode-fonts.ts`.
- The CJK fonts come in one weight, so bold CJK text is drawn regular. Nothing
  fakes bold. Italic is drawn upright, as with the standard fonts.
- Han characters take the Japanese font beside kana, the Korean one beside
  hangul, and otherwise follow the browser's language (Simplified Chinese
  unless it asks for Traditional, Japanese or Korean).
- Text extraction follows glyph order, so a Devanagari vowel sign drawn before
  its consonant copies before it (poppler: "दुिनया"); every character is still
  there once. ActualText would fix it.
- The engine's wasm grew from 1.78 MB to 2.47 MB raw (705 KB brotli), mostly
  rustybuzz. The CJK fonts are 4.5 to 8.3 MB each, fetched only for text that
  needs them; `static/fonts/` is 26 MB in all.

Standard 14 fonts without embedding are still not allowed in PDF/A; PDF/A
embeds its own substitutes.

### Filled form fields are drawn in the standard fonts or Noto

`fill_form_bytes` draws each changed text or choice field in the standard 14
font nearest the one its `/DA` names, or in Noto when that cannot draw it (see
above), never in the form's own embedded font, so a Cyrillic or CJK form's
values do not match its look. Using the field's font needs
reverse-mapping Unicode through its encoding and checking a subset has the
glyphs, which `redact/fonts.rs` half does already. Readers that redraw fields
themselves (Acrobat, and anything when NeedAppearances is set) still show the
value in the form's real font.

Other limits, each deliberate for now: JavaScript format, keystroke and
calculate actions do not run, so a total field is not totalled and a date is
not reformatted; a rich text value (`/RV`) is dropped for the plain one; `/XFA`
is removed, so an XFA-aware reader shows the AcroForm fields rather than the
XFA copy of the form; a field with no widgets on a page cannot be reached from
the UI, which reads fields from page annotations.

### What Edit PDF changes, and its limits

Edit takes text out with redaction's rewrite (`text_only`) and draws the new
text into the page, so the limits of both apply, plus some of its own:

- The new text is drawn in the standard 14 font nearest the old one, or Noto
  for text it cannot draw, not in the document's own embedded font, so its
  shapes differ from the text beside it. Italic is detected but drawn upright: the stamp fonts have no
  italic or oblique faces yet. Drawing in the run's own font needs the same
  reverse mapping as "Filled form fields are drawn in the standard fonts".
- Text is edited one run at a time: a line, or the part of it between gaps
  over 1.2 em. Nothing reflows; a longer replacement grows to the right and
  wraps only at the page edge. Justified spacing, letter spacing and kerning
  inside the old run are not kept.
- Only upright, visible text can be picked: turned or vertical text and
  invisible OCR layers are not offered, and neither are lines whose baseline
  is off the page (printer's marks in `freeculture.pdf`, `issue19326.pdf`).
- A glyph goes once a quarter of it is under the band, so a superscript or
  footnote mark on the line that dips into the band goes with it, though it
  is a separate run and not in the replacement's text.
- Where the engine cannot take text out (a font with no widths, as in
  redaction), the run's ink is painted over in the colour sampled behind it,
  and the result says on which pages. The old text is still in the file
  there.
- A moved image is drawn again on top of the page, outside any clip, soft
  mask or transparency it was drawn under, so a clipped or faded image shows
  whole and opaque once moved. Deleting keeps nothing of it. Images under 4
  pt, mostly off the page, or filling most of it cannot be picked; vector
  drawings cannot be picked at all.
- An erased area's fill and a replaced line's background are sampled from the
  page as pdf.js drew it in the preview, so a gradient or a picture behind
  them gets one flat colour.

### What redaction removes, and where it falls back

Redaction removes rather than covers (see `AI/ARCHITECTURE.md`). Things a box
touches that go entirely, so the page changes beyond the box:

- A glyph goes whole once a box covers a quarter of it. Ordinary text loses
  at most the letters cut by the edge, but a giant glyph goes too: the
  one-glyph pages of `page_with_number.pdf` and its siblings (37 pages), the
  Type 3 shapes in `ContentStream*Type3insideType3.pdf`. Text used as a clip
  (`text_clip_cff_cid.pdf`, `pattern_text_embedded_font.pdf`) takes what it
  clips with it. The preview tints such glyphs.
- An annotation with no stored appearance goes whole, since there is nothing
  to draw into the page and keep the part outside the box: highlights,
  underlines and polygons pdf.js draws itself (`issue12337.pdf`,
  `bug1538111.pdf`, `issue20062.pdf`). So does a text or choice field in a
  form that asks readers to redraw its fields, whose stored look may show a
  value no viewer shows (`bug1669099.pdf`, `bug1844576.pdf`,
  `bug1844583.pdf`), and a widget whose box covers the area
  (`issue19083.pdf`). The preview outlines the first kind.

Pages drawn from a picture instead, 183 in the corpus, lose selectable text
on that page only: JBIG2 (most of the 183, the JBIG2 test suite), CCITT,
JPX and CMYK JPEG images under a box; text in a font without widths that are
not the standard 14 (`ZapfDingbats.pdf`), or in a predefined CJK CMap
(`issue13343.pdf`, 90ms-RKSJ-H); content that will not decode
(`bomb_giant.pdf`). Bundling pdf.js's CMaps and a JBIG2 or CCITT decoder
would bring most back in place.

Not redacted yet, so content under a box survives in these: soft masks set by
an ExtGState, tiling pattern cells, and Type 3 glyph procedures (the glyph
goes whole, its procedure is not opened). Structure elements inside form XObjects keep their `/ActualText`.
Subset fonts keep the outlines of removed glyphs, which says which letters
appeared but not where. Bookmarks, attachments and document-level JavaScript
are outside a page and untouched; "Remove metadata" covers Info and XMP.

lopdf 0.44 decodes the PNG Average predictor wrongly (it adds half the pixel
above to the whole pixel to the left), so anything that relies on it for
predicted streams gets garbage on those rows (`issue14814.pdf`). Redaction
undoes predictors itself. Compress escapes only because it caps decoding at
the image's exact size, which predictor bytes exceed; do not lift that cap
without the same fix.

### Stamping refuses a page tree it cannot fully read

`issue7229.pdf` lists a first page that only a repairing reader finds. Merge
rebuilds the page tree and drops it (see the raster section below). Stamping
edits the tree in place, so it would write the unreadable entry back and number
"Page 1 of 1" on what readers show as two pages; pdf.js then refuses the whole
file. It refuses instead: "Some pages of this PDF are damaged and could not be
read."

### Catalog structure survives only for the first input

`merge_documents` keeps the first input's catalog and discards the rest. Real
losses for inputs 2..N:

Form fields stop working. `/Annots` entries with `/Subtype /Widget` still sit on
the pages, but the `/AcroForm` that gives them meaning is gone, so they render as
inert decoration. Doing this properly also means uniquifying field names that
collide across inputs, which is why pdftk prefixes them.

Tagged PDF is destroyed. `/StructParents` on appended pages points into
structure trees that did not survive. The merged file is untagged, which is an
accessibility regression from either input.

Named destinations break. Link annotations that resolve a destination by name go
through the catalog's `/Names` or `/Dests`. Links carrying a direct destination
array do survive, because their page references get renumbered correctly.

PDF/A conformance is lost, since XMP, `/OutputIntents` and `/MarkInfo` are not
reconciled.

Fixing forms is the most valuable piece and the most self-contained. Structure
tree merging is a much larger job.

### Outlines are discarded

Deliberate, and currently the honest choice, since an outline covering only the
first input is more misleading than none. Merge can instead write a new outline
with one entry per input file, opening at its first page ("Add a bookmark for
each file", off by default). Each input's own outline is still dropped: the
real fix is to re-root those trees under the per-file entries, which is a
feature rather than a repair.

### PDF/A output that does not conform

`convert_to_pdfa_bytes` (2b and 3b) edits the file in place, so forms, tags,
links and outlines survive, and refuses what it cannot fix: unembedded fonts,
visible annotations with no appearance stream, attachments under PDF/A-2, and
dynamic XFA forms. It removes JavaScript and the other forbidden actions,
transfer functions and halftones, and digital signatures, which no rewrite can
keep valid. Hidden annotations keep their place with an empty appearance.
DeviceCMYK gets `ps_cmyk.icc`, which converts the way viewers already do, so
CMYK content keeps its on-screen look.

`npm run test:pdfa` (veraPDF) on the pdf.js corpus, 2026-10-01: 532 of 634
outputs conform. The other 99, grouped by what would fix them:

- Embedded font programs (most of them). Widths in the font dictionary that
  differ from the program (31), glyphs the program does not have (19) or
  `.notdef` drawn (7), programs veraPDF cannot parse and so treats as missing
  (17), and TrueType encoding and cmap rules (6.2.11.6, 23). Detecting these
  means parsing the font programs, which is the font phase in
  `AI/handoffs/2026-10-01-pdfa-fonts.md`. Until then they are written out
  claiming conformance.
- DeviceN spot colours without a /Colorants entry (8). Needs a single-colorant
  tint transform derived from the DeviceN one.
- Content naming resources that do not exist (7, e.g. `Embedded_font.pdf`):
  broken input that draws nothing there.
- JPEG 2000 images with channel counts or bit depths PDF/A excludes (3).
- One each: an inline image with `/I true` (needs content rewriting), an odd
  hex string inside content, a page box under 3 units, a non-UTF-8 name, an
  undefined content operator, inconsistent Separation definitions.

3 outputs crash veraPDF itself and are not counted either way.

### Some encryption is refused

`npm run test:corpus` unlocks the 7 password-protected pdf.js files using the
passwords from pdf.js's own test manifest. 3 unlock with page content identical
to lopdf's decrypted read. 4 are refused, never written out as noise:

- `issue6010_1.pdf`, `issue6010_2.pdf` keep `/Encrypt` as a direct dictionary,
  which lopdf does not treat as encryption. Detected by a leftover `/Encrypt`
  in the trailer.
- `saslprep-r6.pdf`: lopdf rejects the correct R6 password after SASLprep.
- `issue15893_reduced.pdf` does not load at all (one of the 49).

pdf.js opens all four, so the preview unlocks and the tool then reports "could
not be unlocked with that password". The wording is deliberately true in both
cases.

pdf-oxide, used for the Office conversions, accepts a password and then converts
encrypted files into empty documents. Protected PDFs are unlocked by the Rust
engine (`unlock_pdf`) before they reach it.

### The output has no `/ID`

Protected output does get one: `protect_pdf_bytes` writes a random `/ID`,
since readers expect it on encrypted files. Everything else below still
applies to the other operations.

`/ID` is strongly recommended in PDF 1.x and required in 2.0. Adding one means
either randomness or a content hash, and neither is in the dependency tree
today. Leaving it out keeps merges reproducible, which is worth something for a
local-first tool, so this is a trade rather than a straight bug.

---

## Open, performance and size

### Measured: wasm-opt makes the download bigger

The build passes `--no-opt`. I assumed enabling it was a free win and measured
instead. Raw module size against brotli, which is what actually ships:

| build      | raw     | brotli  |
| ---------- | ------- | ------- |
| `--no-opt` | 636 043 | 201 422 |
| `-O2`      | 594 537 | 207 746 |
| `-O3`      | 591 933 | 206 087 |
| `-Os`      | 588 578 | 206 149 |
| `-Oz`      | 580 383 | 206 618 |

Every level shrinks the raw module and every level compresses worse. `-Oz` takes
8.8% off raw and adds 2.6% to brotli. `--no-opt` is the right call for transfer
size and should stay. The remaining reason to run wasm-opt is runtime speed, and
nothing here has benchmarked that yet.

`codegen-units = 1` and `panic = "abort"` were measured and kept: 201 422 to
198 055 brotli, 1.7%.

### Peak memory is roughly twice the input plus the output

The worker concatenates all inputs into one buffer, wasm-bindgen copies that into
linear memory, lopdf parses it into an owned object graph, and `save_to` builds
the whole output in memory. wasm32 caps linear memory at 4 GiB with single
allocations practically well under 2 GiB, so a few large scanned PDFs will hit
it.

The structural fix is to copy object bodies verbatim and rewrite only the
reference tokens, the way qpdf preserves object streams, rather than fully
parsing and reserialising. Faster, lossless on stream bytes, and much lighter.
Streaming inputs through `File.stream()` would stop JS and wasm both holding the
full bytes.

Note that wasm linear memory never shrinks. After a large merge the worker holds
that memory for the session, so terminating it is the only way to reclaim it.

---

## Open, architecture

### Every catalogue tool exists

Summarize PDF, the last one, was added on 2026-10-06 (see "What Summarize
reads, and its limits" below). HTML to PDF and Markdown to PDF were added on
2026-10-05 (see "What HTML and Markdown to PDF draw" below).

Tools that exist in the catalogue but have no implementation yet now render a
"not available yet" panel in the workspace, rather than falling through to the
merge branch and telling a Sign or OCR user to choose two PDFs. The catalogue
still lists them so the roadmap stays visible; the panel is the honest version
of that.

Merge and Split now share the engine's page selection and page-tree rebuilding
path. Split supports ranges, combined ranges, and fixed-size parts. Multiple
outputs are packaged into a ZIP in the worker. The new corpus check extracts the
last page of every loadable file and checks content, page geometry, and
reachability: 924 checked, alongside the unchanged merge baseline above.

PDF to Word, PowerPoint, and Excel, plus the three reverse conversions, use
`pdf-oxide-wasm` in a separate worker. The Office engine loads only when one
of those tools runs. Its WASM asset is about 17.6 MB raw, so keeping it out of
the ordinary PDF worker matters. Conversion quality on real user files still
needs manual review, especially scanned PDFs and complex Office layouts.

PDF to Markdown runs on the same worker, converting page by page with
`toMarkdown` and joining pages with `---`. `toMarkdownAll` is avoided because
it writes the page break straight after the last line, which turns that line
into a setext heading (`paragraph_and_link.pdf`). Adjacent bold runs are
joined, since pdf-oxide bolds word by word. What remains is pdf-oxide's and
cannot be fixed from outside: two-column titles break into fragments,
line-end hyphens stay (`com-pile`), and bold italic comes out as runs of
asterisks. Over the pdf.js corpus through the real worker module: 852 of 982
convert, 114 have no text and are refused as such, 16 fail with pdf-oxide's
own error (3 of them encrypted, which the app unlocks first), and
`issue19517.pdf` panics, reported as "The converter could not read this file."

Organize pages accepts several PDFs at once and mixes their pages into one
output order, which makes it Merge plus page editing in one pass. Each input
has its own color, shown on the page badges and the legend. It uses the PDF
card drag interaction; its deletion zone is fixed to the left edge of the
viewport so it stays in place while pages scroll. The order badge also supports
keyboard reordering. Extract pages, Remove pages, and Rotate PDF stay
single-document tools on the same page-tree rebuilding path. Keep page order,
inheritance flattening, safe renumbering, and pruning in that shared layer.

Compress now repacks object streams, recompresses Flate streams, re-encodes
eligible JPEGs, and converts large 8-bit Flate RGB/grayscale images to JPEG
when that saves bytes. Its strongest setting also downsizes eligible images.
The pdf.js corpus compress check covers 924 files with unchanged page counts
and decoded page content. Strong output is 67.1% of input size; 919 files rebuild
smaller and 5 pass through. It was 67.2%, measured both before and after the
2026-09-20 module split, until identical objects were shared (2026-10-03).
Metadata and thumbnail removal are optional because a rewrite that guarantees
their removal can make an already compact PDF larger.
This is structural evidence, not a rendered image comparison. JPXDecode,
JBIG2Decode, font subsetting and duplicate resource sharing remain open size
wins. JBIG2 matters especially for scanned text. Anything render-dependent
needs a rasteriser. If you reach for one, note that mupdf is AGPL, which is a
licensing decision to make on purpose rather than discover. pdfium and qpdf
are permissive and both build to wasm.

### What HTML and Markdown to PDF draw, and their limits

The browser lays the document out and the engine draws what it measured
(`html-print.ts`, `print.rs`, decision 0004). Markdown becomes HTML first
(`markdown-print.ts`, marked 18). Checked by 6 engine unit tests, a Node run
of a hand-built layout through the wasm binding rendered by poppler, and a
Node run of the Markdown document. The browser half (the frame, columns,
measuring) had not run anywhere when this was written; the user tests it.

Deliberate limits:

- Every font becomes Noto Sans, Serif or Sans Mono, by the first name in the
  element's `font-family`. The document's own web fonts are never used: the
  frame fetches nothing, and the engine can only draw what the browser
  measured in the fonts it embeds.
- Only images inside the file (`data:` URLs) are drawn. Linked images are
  removed before layout and counted in the sidebar; relative paths have no
  folder to resolve against. CSS background images and gradients are not
  drawn; background colours are.
- Characters no bundled font has (emoji, ✓, rarer scripts) are drawn as
  pictures of what the browser drew, at 4× scale, so they are not selectable
  text. Which characters those are comes from the engine (`missing_characters`).
- Italic is slanted (no italic faces are bundled), as browsers fake it too.
- Not drawn: box shadows, outlines, transforms (boxes are drawn where their
  bounding box is), `::first-letter`/`::first-line`, counters in generated
  content, form controls' values, video, canvas, inline SVG styled by the
  page's CSS (SVG is drawn from its own markup at 3×).
- Paint order is document order, backgrounds before content; `z-index` and
  positioned stacking are ignored.
- A page wider than the paper is laid out wider and scaled down, to half size
  at most, as browsers print; scripts never run, so pages built by JavaScript
  come out as their static HTML.
- One page: up to 14,400 pt (200 in), the largest PDF page; longer documents
  continue on further pages of that height.

Untested assumptions worth checking first if something looks wrong: that
`getClientRects()` returns one box per column for a block split across
columns (backgrounds and borders across a page break), and that a `Range`'s
box height is the font's ascent plus descent (baselines; `Reader.ratio`
measures it the same way, so a different meaning would cancel out).

### What Summarize reads, and its limits

Qwen3 0.6B (Apache-2.0) runs in the browser through Transformers.js 4.3:
q4f16 weights on WebGPU with 16-bit shaders (578 MB with the tokenizer),
q4 weights with 32-bit maths on WebGPU without them (928 MB), 8-bit weights
on the CPU through WebAssembly with no WebGPU at all (627 MB). On the CPU it
runs on one thread, since the site is not cross-origin isolated: about 3
tokens a second on an i3-12100F. Chromium on Linux enables WebGPU only on
some GPUs; elsewhere it needs `chrome://flags/#enable-unsafe-webgpu`. The weights
come from a pinned onnx-community revision on Hugging Face, once, and stay in
Transformers.js's cache; the document never leaves. ONNX Runtime's 26 MB wasm
is served by Plico (the Summaries offline pack), never jsDelivr.

The text is Translate's paragraphs, so scanned pages need OCR first, and
two-column pages are read paragraph by paragraph down the page rather than
column by column. Short paragraphs repeated on three or more pages are
dropped as running heads. Parts of about 5,000 characters each give one to
three points; the overview is written from the points (or from the text when
it fits in one part). Long documents keep at most 5, 10 or 20 points, chosen
by how many of their words the whole document shares, not by the model.

A 0.6B model can state things the text does not say. Each point is traced to
the paragraph of its part that holds at least 30% of its words; a point
nothing holds shows its part's first page and no highlight, which is itself a
hint to check it. On the CPU, with no cross-origin isolation and so one
thread, a part takes tens of seconds.

Ask answers from excerpts, not the whole document: the paragraphs that best
match the question and the one before it (BM25 with each word counted once),
up to 10,000 characters on WebGPU and 5,000 on the CPU, plus the overview
when there is one and the last three exchanges. A question whose answer is
spread thinly across a long document, or worded unlike the text, finds the
wrong paragraphs; a document that fits the budget is given whole. Sources
are the excerpts sharing at least 20% of the answer's words, at most three.
Nothing is kept between visits; the conversation goes when the file does.

The user ran Summarize in a browser on 2026-10-06 (it reads), and asked for
tokens a second; quality, speed and memory are otherwise unmeasured. Running
it locally means a 600 MB download and a heavy run; ask first.

### What works offline, and its limits

Every page is prerendered and the service worker saves the core on the first
visit: 325 files, 16.5 MB uncompressed (the engine, pdf.js and its cmaps,
every route, every Noto font but CJK, the PDF/A fonts). Five packs are saved
only when downloaded from the Offline panel or first used: Summaries (the
26 MB runtime plus the model, which the panel saves into Transformers.js's
cache), Office conversion
(17.6 MB), Repair (10.4 MB), Text recognition (one 3.9 MB Tesseract engine for
the browser's wasm features, plus English), Translation (5.2 MB, the engine
only), and CJK fonts (23.2 MB). Translation pairs and OCR languages other than
English stay in IndexedDB, where they always were; the panel lists and removes
them but cannot add them. An engine missing offline says so instead of
failing with a fetch error.

The user reported it working in their browser (2026-10-08, from memory
rather than a fresh run). Still open: whether Tesseract's worker,
now started from its URL rather than a blob, is controlled by the service
worker in every browser; whether Vercel serves prerendered tool pages at
`/tools/merge` without a trailing-slash redirect (a redirected response is
stored as a plain one, but it is untested). A new release takes over at once
(`skipWaiting`), keeping the previous core for pages still open on it; two
releases in a row while a tab stays open could leave that tab's lazy chunks
unsaved. Safari has no install prompt, so Install app never shows there.

### Compare reads text in content order

Compare diffs the engine's glyphs in the order pages draw them, which is
usually reading order but not always: a two-column page drawn column by
column compares fine, one drawn line across both columns does not, and the
scroll sync drops anchors that run backwards up a page. Changes in font,
size or colour alone are not text changes; Visual shows them. The diff runs
on the main thread: about 0.4 s for 60,000 words with 300 edits, measured on
generated pages. Past 2,000 edits in one stretch of unmatched lines, that
stretch is reported as one change. Removing the original moves the changed
PDF into the Original side, since sides are positions in the file list.

### What Translate PDF changes, and its limits

Translate is Edit underneath (old glyphs out with `text_only`, the translation
drawn as a text box), so Edit's limits apply, plus its own. Checked headless
only: `tracemonkey.pdf` page 1 into French, Japanese and Arabic through the
real layout code, Bergamot and `edit_pdf`, rendered with poppler (21 blocks,
none overflowing, none painted over). Not run in a browser, and no corpus
harness exists for it yet.

- Paragraphs are guessed from line spacing, edges, size and weight
  (`continues` in `translate-layout.ts`). A first-line indent or a line that
  stops short after a full stop starts a paragraph; a bullet always does.
  Two-column text with uneven gutters, drop caps and text in tables whose
  cells line up can be joined or split wrongly.
- Each paragraph is drawn in one style: its commonest family and weight.
  Bold run-in labels, italics and links inside it are lost; first-line
  indents are not kept. Bergamot keeps `<b>` through translation (verified),
  so sending lines as HTML would fix the first.
- A translation is set at the original size while it fits the room down to
  the next thing below it, then shrinks to at most 60%; past that it overlaps.
  Paragraphs of one style on a page share the size most of them fit at
  (`setPage`), so body text stays even.
- Lines without two letters (page numbers, figures, formulas) and addresses,
  links and code-like tokens are left as they are. Superscript markers are
  separate runs and stay where the original words were.
- Wrapping Chinese, Japanese and Thai fills each line (the engine's `wrap`
  now breaks them anywhere, for every text box); a Latin word inside them can
  break mid-word. Thai breaks between any characters, not at syllables.
- Simplified and Traditional Chinese share the CJK font choice in
  `unicode-fonts.ts`, which follows the browser's language, not the target;
  Traditional output on a non-Taiwanese browser uses the Simplified subset
  and may refuse characters it lacks.
- Scanned pages have no text and are skipped with a note pointing to OCR PDF.
- Models come from a third-party mirror (decision 0003); if it goes away,
  every language needs a new host.

### What OCR reads, and its limits

OCR PDF writes Tesseract's words as invisible text; the page picture is left
as it was. Right-to-left scripts are drawn in logical order, so selection in
a reader can run backwards. Vertical CJK models are not offered. Pages are
not deskewed or turned upright: a sideways scan reads as noise, and Tesseract's
orientation detection needs a separate 10 MB model. A page counts as having
text from 32 glyphs, which an already-OCR'd file meets, so its old layer is
kept rather than replaced. The tesseract.js language-object bug that the
IndexedDB workaround avoids should be rechecked on any upgrade past 7.0.0.
`npm run test:ocr` has no baseline yet; it runs Tesseract in Node and needs a
lot of memory, so run it with little else open.

### What Scan finds, and its limits

Scan looks for four straight edges, so a curled page, a book spread or a
receipt with torn edges is found poorly or not at all; the corners are then
the whole photo and the editor is the fix. A sheet on a desk of nearly its
own colour has little edge to find. On a generated steep view one side
snapped to the photo's border, and a page running past the frame lost its top
to a text line. Photos above 12.5 MP are read smaller. The straightened
proportions assume a lens centred on an uncropped photo; a page within 10% of
the chosen sheet's shape is stretched to fill it. B&W is a global threshold
over evened light, so faint pencil can drop out, and it is Flate, not CCITT
G4 or JBIG2, which would be several times smaller. Nothing is deskewed below
the found edges, and HEIC photos read only where the browser decodes them.

### Error strings are product copy inside the engine

`"Choose at least two PDFs to merge."` cannot be translated, and it pushes a UI
rule into `merge_pdf_bytes`, which is the wrong place for it. Split validation
currently returns strings too. A structured error enum would let Svelte own the
wording across both operations.

---

## Open, testing

### Rendered output is now compared, and it found three classes of loss

`npm run test:raster` merges every corpus file with a marker page, renders
source and result with pdf.js at 36 DPI, and requires every page to survive
pixel-for-pixel. It runs pdf.js in Node against `@napi-rs/canvas`, which
pdfjs-dist already depends on, so the harness needs no new renderer and no AGPL
code. A pixel is "changed" if any channel moves by more than 32; a page fails
if more than 0.1% change. pdf.js is deterministic on this corpus (two renders
of the same file differ by 0.000%), so a diff means the merge changed the page.

Baseline on the pdf.js corpus: 921 of 982 files merge and render
byte-identical. The 9 differences fall into four buckets:

- lopdf stream-decoding loss (5): `xobject-image.pdf`, `operator-in-TJ-array.pdf`,
  `issue7665.pdf`, `issue1293r.pdf`, `issue11549_reduced.pdf`. Each has a
  content stream lopdf cannot decode (a wrong `/Length`, an empty decode, or a
  filter handled differently than pdf.js), so the merge writes back empty or
  altered bytes and the page renders blank or shifted. The structural harness
  passed all of these. Now refused on load and sent to Repair (see "lopdf
  loaded some damaged files with part of a page missing" under Fixed); this
  baseline predates that.
- malformed page tree (1): `issue7229.pdf`. lopdf counts one fewer page than
  pdf.js because the source's `/Kids` names a missing object, so the merge
  shipped one fewer page than a viewer shows. Now refused the same way; the
  repaired copy has both pages.
- geometry rounding (1): `freeculture.pdf` page 2 is one pixel shorter after
  merging, from lopdf's f32 float serialisation of an inherited `/CropBox`.
- identical content, different render (2): `issue13147.pdf`, `issue5954.pdf`
  have byte-identical content streams and resources after merging yet render
  differently in pdf.js. Settling which is right needs a second renderer.

### Stamps are checked by rendering, with 7 known failures

`npm run test:raster:stamp` renders every corpus file three ways with pdf.js:
as is, with an opacity 0 watermark, and with page numbers at the bottom right.
The invisible watermark must match the source; it goes through the same
wrapping and resource copying as a visible one. The numbered render must differ
from the invisible one only in the bottom right corner as displayed, which is
what exercises `/CropBox`, `/Rotate` and `/UserUnit`.

Baseline 2026-09-30: 921 files compared, 7 failures, all load/save losses
shared with merge above: the five stream decoding files, `freeculture.pdf`, and
`issue13147.pdf`. Stamping keeps each page's original streams, but lopdf has
already lost those bytes on load. `issue7229.pdf` is refused.

37 pages show no number, all explained: pages smaller than the margin and text
(`issue11878_reduced.pdf` is 3 by 3 points, several are under 35 points tall),
a form field whose widget covers the corner (`issue19083.pdf`), a fuzzed file,
and pages where pdf.js under Node stops drawing before the end of the content
(`images_1bit_grayscale.pdf`, `issue4706.pdf`, `issue7821.pdf`,
`issue20294_reduced.pdf`), where poppler shows the number in the right place.

### Crops are checked by rendering, with 11 known failures

`npm run test:raster:crop` crops every page of every corpus file to an
off-centre area (10% left, 5% top, 15% right, 20% bottom) with each edge
snapped to a whole pixel at the render scale, then requires the cropped render
to match that part of the source render pixel for pixel. Different margins on
every side mean a crop mapped through the wrong `/Rotate` keeps the wrong part
of the page and fails; feeding it a vertically mirrored area fails 13 of the
first 20 files, so it does catch that.

Baseline 2026-10-02: 920 files compared, 42 refused by the engine (lopdf will
not load them, or `issue7229.pdf`'s unreadable page), 11 failures:

- lopdf load/save loss shared with merge (7): the five stream-decoding files,
  `issue13147.pdf`, and `multiple-filters-length-zero.pdf`, whose plain load and
  save already differs from the source (0.09%, under merge's threshold).
- pdf.js shading edges (4): `issue10339_reduced.pdf`, `issue6769.pdf`,
  `issue6769_no_matrix.pdf`, `radial_gradients.pdf` differ by one row of
  antialiasing along a gradient's edge. The page is moved by whole pixels, but
  pdf.js does not rasterise shadings the same at every offset. Poppler renders
  the cropped and source pages identically (0.000%) for the first three and the
  last, so the crop itself is right.

Crop boxes are written rounded to a thousandth of a point. Before that, f32
fractions wrote values like `168.39998` for `168.4`; it changed none of the
failures above, but keeps the numbers short.

### Flattening is checked by rendering, with 10 known failures

`npm run test:raster:flatten` flattens every corpus file and requires each
page to render as it did before, since pdf.js draws annotation appearances
itself. Shifting every appearance by 5 points fails 14 of the first 150 files,
so it does catch placement.

Baseline 2026-10-02: 921 files compared, 186 of them with annotations to
flatten, 94 annotations left as they were (no stored appearance, or text and
choice fields in forms that set NeedAppearances), 59 refused (the same
unloadable, password and page-less files every tool refuses). 10 failures:

- lopdf load/save loss shared with merge (7): the five stream-decoding files,
  `freeculture.pdf`, `issue13147.pdf`.
- pdf.js only (3): poppler renders each identical before and after.
  `pr20043.pdf` has NoRotate notes on a turned page, which pdf.js ignores and
  the engine keeps upright, as the spec and poppler do. `bug1802506.pdf`
  (buttons) and `issue13242.pdf` (a highlight) are drawn by pdf.js in ways of
  its own that the stored appearance does not match.

Two things the corpus taught, both now handled. pdf.js draws form fields over
every other annotation whatever the /Annots order (`issue13003.pdf`, a checkbox
under a square), and poppler follows /Annots order; flattening follows pdf.js.
And a form with NeedAppearances makes even empty fields' stored appearances
suspect (`bug1669099.pdf`), so those are kept. Empty fields with nothing stored
and no /MK box are removed: 292 of the 297 appearance-less text fields in the
corpus, which otherwise made flattening a blank form report hundreds of items
it could not flatten.

### Redaction is checked by an independent reader and by rendering

`npm run test:raster:redact` puts a box over the middle of every page (the
same region however the page is turned), then reads the result with poppler,
which shares no code with the engine, and renders it with pdf.js. A word
poppler finds mostly under the box is a leak; the box must render solid; the
page outside the box, beyond a 24 pt margin, must render as before, except on
pages drawn from a picture.

Baseline 2026-10-03: 916 files, 1,714 pages, 83,100 words removed.

- One reported leak, a false positive: `issue6387.pdf`, vertical text, where
  poppler puts glyphs about 0.8 em above where pdf.js draws them. The render
  shows "風" below the box.
- 52 pages changed outside the box: the giant glyphs, clipping text and
  annotations listed under "What redaction removes", and the lopdf load/save
  losses shared with merge (`xobject-image.pdf`, `issue1293r.pdf`,
  `issue7665.pdf`, `issue13147.pdf`). Those lose content, never leak it: the
  rewritten page replaces the original bytes.

It found four real bugs on its way to this, each now a unit test: lopdf's
Average predictor, a one-pixel image stretched into a rule erased whole,
CIDs mapped with bfchar, and vertical glyph boxes too tall to count as covered.
It also found its own: poppler's `-bbox` is in media box space unless given
`-cropbox`.

`npm run test:corpus` also redacts every loadable file and checks structure
and that nothing the engine reads as text lies under the box: 923 files,
1,721 pages, 185 of them pictures (139 for images, 33 for text, 13 for
content).

### Form filling is checked by pdf.js, poppler and rendering, with 4 known failures

`npm run test:forms` fills every field a person could fill in every corpus
form through the wasm build: a short distinct text in each text field (cut to
its `/MaxLen`), another option in each choice field, each check box turned the
other way, another radio button chosen. pdf.js must read every value back, from
every widget; poppler's `pdftotext -raw` must find each visible text in the
flattened pages; and pdf.js renders of the filled and the filled-and-flattened
file must match like the flatten check's.

Baseline 2026-10-03: 97 forms and 787 fields filled, 643 texts found in the
flattened pages, 11 fields left fillable by flattening, none refused. 83 forms
rendered and compared. 4 failures, each understood:

- `issue15096.pdf` (2, one per widget): an incremental update rewrote the two
  radio buttons without `/Parent`, while the old parent still lists them as
  kids and holds `/V /Choice2`. pdf.js keeps reading that value. The file
  breaks the field tree; the engine follows the buttons' own dictionaries.
- `issue13003.pdf`: an opaque square annotation lies over the fields. pdf.js
  draws form fields above other annotations; flattening form fields only
  puts them into the page content, under the square that stays an annotation.
  Flatten's "Form fields" scope does the same.
- `bug1802506.pdf`: the buttons pdf.js draws its own way, already a pdf.js-only
  failure in the flatten check.

The 14 forms that set NeedAppearances are compared apart, since pdf.js redraws
their fields in its own way before flattening and draws the engine's
appearances after. 6 differ by more than 0.1% of pixels, the largest
(`bug1844583.pdf`, 9%) a page barely larger than its one field, where the two
fonts' slightly different metrics are most of the page.

### Editing is checked by poppler and rendering

`npm run test:edit` replaces the longest line the app would let a person edit
on every page of every corpus file with "Plico edit", found and placed by
`src/lib/pdf/edit-text.ts` itself and styled as the app styles it. Poppler,
which shares no code with the engine, must find no word left on the old line
(a word whose middle lies in the band the engine was asked to clear) unless
the engine said it painted that page over, and must find the new text; pdf.js
renders of source and result must match away from the line and its
replacement, beyond a 12 pt margin.

Baseline 2026-10-04: 921 files read, 492 edited, 1,168 pages
compared, 10,225 words replaced, none refused and no failures. 426 files have
no line to edit and 64 do not load; 6 pages were painted over for text in a
font without widths, and on 4 pages poppler reads no words at all. Lines read
as Helvetica 371, Times 744, Courier 56; 84 bold, 26 italic (drawn upright),
10 of unknown colour.

It found five real bugs on its way to green, each fixed: glyphs drawn with no
width on a run's edge stayed (`bug1157493.pdf`, `issue5039.pdf`; the band now
reaches a tenth of an em past the run), and lines whose baseline lies off the
page were offered, refused by the engine (`issue19326.pdf`) or replaced where
nothing shows (`freeculture.pdf`). One was its own: lopdf writes boxes back
with fewer digits, which can round a page's rendered size by a pixel.

### No fuzzing

`cargo-fuzz` over `merge_pdf_bytes`, seeded from the corpus. Two invariants:
never panic, and output always reparses with the expected page count. On
wasm32 a panic is an unrecoverable trap, so "does not panic" is a safety
requirement rather than a nicety. Much of the panic surface is in lopdf and
wants its own target.

### 49 corpus files will not load at all, 33 of them do after repair

lopdf rejects them outright (24 on an invalid trailer, 12 on an xref start, 9
with no pages found, 4 others), so the corpus harnesses skip them. lopdf 0.44
has no recovery of its own.

The app repairs them with MuPDF instead (`src/lib/pdf/repair.ts`, decision
0002): Repair PDF does it on request, and every other tool does it when a run
fails with one of `load_document`'s "could not be read" or "has no pages"
errors. Measured 2026-10-04 in Node with mupdf.js 1.28.1, each file opened,
saved and loaded again with lopdf: 33 load, and 32 of them have poppler's
page count (poppler cannot read the other, `bug1980958.pdf`). Of the 16 that
do not, 11 poppler cannot read either, `encrypted-attachment.pdf` loads but
asks for a password, and 4 fail (`issue21436.pdf`, `PDFBOX-4352-0.pdf`,
`poppler-742-0-fuzzed.pdf`, `poppler-937-0-fuzzed.pdf`). qpdf 12.4.1 (native)
recovers 30, none that MuPDF does not. Only page counts were compared, not
rendered pages.

The prebuilt qpdf wasm packages on npm were tried first and recover none of
them: `@neslinesli93/qpdf-wasm` (qpdf 12.2.0) fails with "expected n n obj"
where native qpdf reconstructs the xref, and `qpdf-wasm` is built with
pthreads, which needs cross-origin isolation. There is no corpus harness for
repair yet; one would run MuPDF and the engine in Node over the 49 and render
the results.

---

## Open, repo and tooling

### Five eslint errors, all pre-existing UI

`ToolsDialog.svelte` has four `svelte/no-dom-manipulating` and one
`svelte/prefer-svelte-reactivity`. These predate this work and sit in dialog and
motion code where a naive fix risks changing behaviour, so they are listed rather
than patched. `npm run lint` fails until they are dealt with.

### Worker lifecycle

Any error terminates the worker and rejects every pending request. Terminating is
right, both because a wasm trap may have left the allocator inconsistent and
because it is the only way to reclaim linear memory. The cost is that the next
merge pays full init latency.

Worth doing: warm the worker on idle instead of on first click, tell cancel apart
from transient error, and handle `onmessageerror`, which is currently unhandled.
