# Issues

Known problems in Plico, ordered by how much they hurt. The engine section is
the priority: it is the part that can corrupt a user's document silently.

Evidence comes from two places. `cargo test` runs 80 unit tests against
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

---

## Open, engine correctness

### Page numbers and watermarks only draw Western European text

Stamps use the standard 14 fonts (Helvetica, Times, Courier, regular and bold),
which every reader has, so nothing is embedded. They only cover
WinAnsiEncoding. Text outside it is refused by name ("“日” cannot be drawn with
the built-in PDF fonts."), never drawn as the wrong glyphs. Widths come from
Adobe's Core 14 AFM files, so placement is exact for what is supported. Other
scripts need an embedded, subset font, which is a size and licensing decision.

Standard 14 fonts without embedding are also not allowed in PDF/A, which does
not matter until PDF/A output exists.

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

### Reference rewriting recurses

`remap_object` recurses through arrays and dictionaries. A deeply nested array
could overflow the wasm stack, which is a trap rather than a catchable error.
lopdf's own parser and `traverse_objects` have the same shape, so a document that
breaks this breaks during parsing first. Worth an explicit depth cap anyway.

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

### Object streams are not used on write

An earlier read of this said lopdf writes a classic cross-reference table. That
was wrong. `save_to` already emits a cross-reference _stream_
(`/Type /XRef /W [1 4 2]`). The remaining win is object streams, which pack
non-stream objects such as page and annotation dictionaries into one compressed
blob. `Document::save_modern()` turns both on. Worth measuring against the
corpus, guarded so the header version is at least 1.5.

### Identical resources are never shared

Merging ten invoices generated from one template embeds the same font ten times.
A bottom-up hash over the object graph would let identical subtrees collapse to
one. Neither lopdf nor pdf-lib does this. mutool does. For template-heavy merges
this is a step change rather than a percentage.

Current corpus output is 87.1% of input size, from pruning and compression
alone.

### Inherited attributes are copied by value

`inheritable_attributes` writes the resolved value onto each page. When the
ancestor held `/Resources` as a direct dictionary rather than a reference, that
dictionary is cloned onto every page under it. A 500 page document with a large
direct `/Resources` at the root would inflate badly.

No sign of it in the corpus, which came out at 87.1% overall, so this is a
latent risk rather than an observed one. The fix is to promote a direct value to
one indirect object per ancestor and share the reference.

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

### Twenty-two tools exist, the catalogue advertises about forty

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
and decoded page content. Strong output is 67.2% of input size; 919 files rebuild
smaller and 5 pass through. The same figures were measured from a clean checkout
of the pre-refactor commit and after the 2026-09-20 module split.
Metadata and thumbnail removal are optional because a rewrite that guarantees
their removal can make an already compact PDF larger.
This is structural evidence, not a rendered image comparison. JPXDecode,
JBIG2Decode, font subsetting and duplicate resource sharing remain open size
wins. JBIG2 matters especially for scanned text. Anything render-dependent
needs a rasteriser. If you reach for one, note that mupdf is AGPL, which is a
licensing decision to make on purpose rather than discover. pdfium and qpdf
are permissive and both build to wasm.

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
  passed all of these. The fix is to detect a failed decode and refuse the
  merge rather than silently ship an empty page.
- malformed page tree (1): `issue7229.pdf`. lopdf counts one fewer page than
  pdf.js because the source's `/Kids` names a missing object, so the merge
  ships one fewer page than a viewer shows.
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

### No fuzzing

`cargo-fuzz` over `merge_pdf_bytes`, seeded from the corpus. Two invariants:
never panic, and output always reparses with the expected page count. On
wasm32 a panic is an unrecoverable trap, so "does not panic" is a safety
requirement rather than a nicety. Much of the panic surface is in lopdf and
wants its own target.

### 49 corpus files will not load at all

lopdf rejects them outright, so the harness skips them. Some are deliberately
broken. Others may be files that qpdf and pdfium read fine, which would point at
lopdf's parser. Nobody has checked which is which.

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
