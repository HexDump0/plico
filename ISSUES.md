# Issues

Known problems in Plico, ordered by how much they hurt. The engine section is
the priority: it is the part that can corrupt a user's document silently.

Evidence comes from two places. `cargo test` runs 30 unit tests against
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

`load_document` now tries `decrypt("")` first. Decryption has to happen before
renumbering, because below `/V 5` the encryption key is derived from each
object's number and generation.

Not validated by the corpus: all 9 encrypted files in pdf.js need a real
password, so the empty-password path never runs there. The change follows the
spec but wants a fixture of its own.

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
first input is more misleading than none. The real fix is to re-root each
input's outline tree under one synthetic parent, which is a feature rather than
a repair.

### The output has no `/ID`

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

### Redundant copies of the whole output

`worker.ts:22` calls `.slice()` on what `merge_pdfs` returns. wasm-bindgen's glue
for a returned `Vec<u8>` already ends in `.slice()` followed by
`__wbindgen_free`, so that value is a fresh JS-owned copy. The extra call
duplicates the entire output for nothing. If you remove it, assert
`byteOffset === 0 && byteLength === buffer.byteLength` so a future wasm-bindgen
change cannot quietly make the `transfer` unsound.

`PdfDropzone.svelte:96` does `new Blob([bytes.slice().buffer])`. `Blob` copies
its input anyway.

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

### Fifteen tools exist, the catalogue advertises about forty

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

Organize pages, Extract pages, Remove pages, and Rotate PDF use the same page-tree
rebuilding path. Their UIs preview pages with pdf.js; Organize pages allows drag
and arrow reordering. Keep page order, inheritance flattening, safe renumbering,
and pruning in that shared layer.

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

### Nothing compares rendered output

The corpus harness checks structure. Page counts, geometry, reachability, and
that the result reparses. Two pages can satisfy all of that and still look
wrong.

The highest-value thing left to build is a rasterising comparison. Merge with
Plico and with `qpdf --empty --pages a.pdf b.pdf --`, render both with pdfium or
mupdf at low DPI, compare perceptual hashes. Note that a structural check would
have passed the inheritance bug above, which was silently producing wrong-sized
blank pages.

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
