# AGENTS.md

Working notes for Plico, an open-source local-first PDF toolbox. Files never
leave the browser: a SvelteKit front end hands bytes to a Rust engine compiled to
wasm, running in a worker.

Read `ISSUES.md` before engine work. It records what is broken, what was already
fixed and why, and which fixes exist only because a specific real file broke.

## Layout

```
rust/plico-engine/src/lib.rs                                public engine API and module map
rust/plico-engine/src/documents.rs                          page selection, assembly, and safe renumbering
rust/plico-engine/src/compression.rs                        compression pipeline and stream packing
rust/plico-engine/src/compression/image_transcode.rs        image recompression
rust/plico-engine/src/images.rs                             image-to-PDF input
rust/plico-engine/src/security.rs                           protect, unlock, and protection detection
rust/plico-engine/src/stamps.rs                             page numbers, watermarks and signatures drawn onto existing pages
rust/plico-engine/src/crop.rs                               crop boxes set on existing pages
rust/plico-engine/src/flatten.rs                            annotation and form appearances drawn into pages
rust/plico-engine/src/annotate.rs                           annotations with stored appearances, or drawn into pages
rust/plico-engine/src/forms.rs                              form field values and the appearances that show them
rust/plico-engine/src/edit.rs                               replacing a page's text, erasing areas, drawing additions in
rust/plico-engine/src/ocr.rs                                recognized words written under a page as invisible text
rust/plico-engine/src/scan.rs                               finding a sheet in a photo, straightening and cleaning it, scans as pages
rust/plico-engine/assets/glyphless.ttf                      the empty font OCR text is written in
rust/plico-engine/src/archive.rs                            PDF/A-2b and 3b conversion, in place
rust/plico-engine/src/redact.rs                             redaction: boxes, annotations, page pictures
rust/plico-engine/src/redact/content.rs                     content stream rewriting that removes what lies under a box
rust/plico-engine/src/redact/lexer.rs                       content and CMap tokenizer that keeps byte spans
rust/plico-engine/src/redact/fonts.rs                       glyph widths, extents and text for redaction and search
rust/plico-engine/src/redact/pixels.rs                      image pixels overwritten under a box
rust/plico-engine/assets/icc/                               ICC profiles for PDF/A (Ghostscript's, AGPL)
rust/plico-engine/src/bindings.rs                           browser/wasm entry points
rust/plico-engine/src/tests.rs                              generated-fixture unit tests
rust/plico-engine/tests/corpus.rs                           structural checks over a real PDF corpus
scripts/pdfa-check.mjs                                      veraPDF validation of PDF/A output over the corpus
src/lib/components/ToolWorkspace.svelte                     tool layout and settings
src/lib/components/DocumentCards.svelte                     card previews, ordering, and removal
src/lib/pdf/download-job.svelte.ts                          result and cancellation lifecycle
src/lib/pdf/processor.ts                                    main-thread side, owns the worker
src/lib/pdf/worker.ts                                       worker side, calls into wasm
src/lib/pdf/wasm/                                           generated, gitignored, never edit
src/lib/tool-catalog.ts                                     the ~40 advertised tools
scripts/raster-compare.mjs                                  rendered-output comparison over the corpus
scripts/raster-stamp.mjs                                    rendered check of page numbers and watermarks
scripts/raster-crop.mjs                                     rendered check of cropping
scripts/raster-flatten.mjs                                  rendered check of flattening
scripts/raster-redact.mjs                                   poppler text and rendered check of redaction
scripts/forms-check.mjs                                     pdf.js, poppler and rendered check of form filling
scripts/edit-check.mjs                                      poppler and rendered check of replacing text
src/lib/pdf/crop-area.ts                                    content bounds and padding, shared by preview and worker
src/lib/pdf/signature.ts                                    drawn, typed and uploaded signatures as trimmed PNGs
src/lib/pdf/annotations.ts                                  what Flatten will draw, read with pdf.js for the preview
src/lib/pdf/redact-text.ts                                  finding text, picking words, and what a box removes
src/lib/pdf/annotate.ts                                     annotation geometry and appearance, mirroring annotate.rs
src/lib/pdf/annotate-editor.svelte.ts                       Annotate's marks, selection, undo and style
src/lib/pdf/form-fields.ts                                  form fields read with pdf.js, and the fills sent for them
src/lib/pdf/edit-text.ts                                    lines Edit can replace, their look, and colours sampled from the page
src/lib/pdf/compare-text.ts                                 Compare's word diff, the changes it finds, and their markup
src/lib/pdf/ocr.svelte.ts                                   OCR language models, Tesseract workers, and reading pages
src/lib/pdf/scan.svelte.ts                                  Scan's photos: corners, turns, and previews kept up to date
scripts/ocr-check.mjs                                       Tesseract and poppler check of OCR on scans made from the corpus
src/lib/pdf/opened-pdf.svelte.ts                            a PDF opened with pdf.js and its page sizes, for Compare's two views
testing/                                                    local corpus, gitignored
```

## Build and test

The generated wasm is not committed. Without it, Vite fails with
`Failed to resolve import "./wasm/plico_engine.js"`. `predev`, `prebuild` and
`precheck` run `build:wasm` for you, and a no-op rebuild takes under half a
second, so normally you do not think about it.

```sh
npm run dev           # builds wasm first
npm run build:wasm    # by hand, if you want it
npm run check         # svelte-check
npm run lint          # prettier and eslint
npm run test:engine   # cargo test
npm run test:corpus   # needs a corpus, see below
npm run test:raster   # needs a corpus, renders and compares output
npm run test:raster:stamp  # needs a corpus, renders stamped output
npm run test:raster:crop   # needs a corpus, renders cropped output
npm run test:raster:flatten  # needs a corpus, renders flattened output
npm run test:raster:redact   # needs a corpus and poppler, checks redacted output
npm run test:forms    # needs a corpus and poppler, fills every corpus form
npm run test:edit     # needs a corpus and poppler, replaces a line on every page
npm run test:ocr      # needs a corpus and poppler, slow and memory-heavy, reads scans
npm run test:pdfa     # needs a corpus and veraPDF, validates PDF/A output
```

First-time requirements beyond node: `rustup target add wasm32-unknown-unknown`,
`brew install wasm-pack`, and Xcode Command Line Tools. The wasm target links
with the bundled `rust-lld`, but host build scripts and proc macros still need a
working `cc`.

The corpus test is ignored by default because it needs files on disk. It reads
`testing/pdfjs/test/pdfs` or `$PLICO_CORPUS`. pdf.js's corpus is a good default:

```sh
git clone --depth 1 https://github.com/mozilla/pdf.js testing/pdfjs
npm run test:corpus
```

Current baseline on that corpus: 982 files, 49 that lopdf will not load at all,
9 needing a real password, 924 merged and checked, output at 77.3% of input size.
If your change moves any of those numbers, say so.

The raster harness is currently red on 9 files. Do not be surprised by that; it
is the point. See `ISSUES.md` for which files and why. If your change moves that
number, say so.

`npm run test:pdfa` needs [veraPDF](https://verapdf.org) (Java) on PATH or in
`$VERAPDF`. Baseline 2026-10-01: 634 files converted to PDF/A-2b, 532 of them
conform; 290 are refused (245 unembedded fonts, 30 annotations without an
appearance, 12 attachments, 3 XFA forms). The 99 that do not conform are listed
in `ISSUES.md`. Every one of those claims a conformance it lacks, so this number
should only go down. If your change moves it, say so.

The flatten raster harness is red on 10 files: the same 7 load/save losses and
3 that only pdf.js draws differently; poppler renders those identically.

`npm run test:raster:redact` also needs poppler's `pdftotext`. Baseline
2026-10-03: 916 files and 1,714 pages redacted, 183 of them drawn from a
picture, 83,100 words under the box gone. Poppler reports one word still under
a box, a false positive in `issue6387.pdf` (vertical text, which poppler places
0.8 em above where it renders); 52 pages change outside the box, every one
explained in `ISSUES.md`. A leak is the failure that matters here: if your
change finds one, stop. If it moves either number, say so.

`npm run test:forms` also needs poppler's `pdftotext`. Baseline 2026-10-03: 97
forms and 787 fields filled and read back, none refused, red on 4 failures in
3 files, each explained in `ISSUES.md`. A value that reads back wrong or a text
missing from the flattened page is the failure that matters. If your change
moves either number, say so.

`npm run test:edit` also needs poppler's `pdftotext`. Baseline 2026-10-04:
921 files, 492 edited,
1,168 pages compared, 10,225 words replaced, none refused, no failures; 6 pages
painted over for text in a font without widths.
Old text left under its band is the failure that matters. If your change moves
any of those numbers, say so.

The crop raster harness is red on 11 files, 7 of them the same lopdf load/save
losses and 4 where pdf.js antialiases a shading edge differently once the page
moves; poppler renders those 4 identically. `ISSUES.md` has the list.

The stamp raster harness is red on 7 files, all lopdf load/save losses shared
with merge, and reports 37 pages where the number drew nothing visible, each
explained in `ISSUES.md`. The stamp corpus check numbers and watermarks 923
files and refuses `issue7229.pdf`.

## Engine invariants

Each of these is load-bearing and at least one was a real bug. Breaking them
tends to produce output that parses cleanly and is wrong, which is the worst
failure mode this codebase has.

Decrypt before renumbering. Below `/V 5` the encryption key is derived from each
object's number and generation, so moving objects first produces garbage.

Never hand lopdf an owner password below revision 5. It derives the key as if it
were the user password and decrypts into noise that parses cleanly. Recover the
user password first; `load_document` does.

Read page order and inherited attributes before renumbering. Both need the
input's own page tree intact.

Take page order from `get_pages()`, never from object ids. They are different
sequences. A `BTreeMap` keyed by `ObjectId` will silently reorder pages.

Flatten `/Resources`, `/MediaBox`, `/CropBox` and `/Rotate` onto each page before
reparenting it. Those four, and only those four, are inheritable
(ISO 32000-1, 7.7.3.4). Skip this and pages adopt another document's geometry.

Never let a reference to a missing object get remapped onto a real one. That is
what `renumber()` exists for. lopdf's own renumbering leaves such references
alone while everything moves underneath them, which turned one corpus file's page
tree into a cycle and lost every page in it.

Never call lopdf's `renumber_objects()`, including to compact ids before
writing. It reuses the id reserved for missing objects, so dangling references
start resolving to real ones. `write_document` compacts with `renumber()`.

Set `/Root` before pruning. `prune_objects()` sweeps from the trailer, so pruning
first deletes the entire document.

Give each input a disjoint id range, and reserve one id past each range for
unresolvable references.

PDF/A output must never claim a conformance it lacks. When the engine cannot
fix something it detects, it refuses with the reason; veraPDF over the corpus is
how undetected cases are found. `validatePdfA`-style self-checks are not
evidence.

Redaction removes, it never only covers. Anything under a box that the
engine cannot take out exactly sends the page to a picture; it is never
written out still there. A glyph's width that cannot be known is `None`,
never a guess, because a glyph placed by a guessed width can sit beside the
box that should have removed it. Operations that draw nothing under a box are
copied byte for byte, which is why redaction has its own lexer rather than
lopdf's parser, which re-encodes everything and drops inline images it cannot
decode.

Stamping and cropping edit pages in place: no page tree rebuild, no
renumbering. Lay a stamp or a crop out in the page's visible frame, which is
`/CropBox` (clipped to `/MediaBox`) turned by `/Rotate` and scaled by
`/UserUnit`, never raw `/MediaBox` coordinates. Wrap existing content in `q`/`Q` and close any states
the page left open, or the stamp is drawn through the page's last transform.
Never add to `/Resources` other pages share; give the page its own shallow copy.

Annotating edits pages in place too. Each appearance's `/BBox` is its
annotation's `/Rect` in page space, with the content turned into the frame
inside it, so readers draw it without fitting and it holds on turned pages.

Editing takes old text out, it does not paint over it: a white box over a
word leaves the word selectable and searchable underneath. Only where the
engine cannot take it out does it paint over, and then it says on which
pages. Replacing text removes glyphs only (`text_only`), never the paths and
images behind them.

New objects never take an id something already references. lopdf numbers
them from `max_id`, which only counts objects that exist, so a dangling
reference would come to name whatever was added next. `load_document` raises
`max_id` past every referenced id for that reason.

## lopdf notes

Pinned at 0.44 with `default-features = false, features = ["wasm_js"]`.

`renumber_objects_with` has an undocumented phase that permutes page objects so
page order matches id order. The engine no longer depends on that, and should
not start.

Every tool writes object streams. Merge, Split and Organize write them only
(`write_document`); the in-place tools and Compress also write a plain copy
and keep the smaller (`write_compressed`), which clones the document.

`Stream::compress` only acts when `/Filter` is absent, so already-compressed
streams are not touched. Most real streams arrive compressed, which means merge
cost is dominated by parsing and serialising rather than deflate.

`prune_objects()` is a real reachability sweep rooted at the trailer, so orphan
cycles are collected.

There is no helper for inherited attributes. That is on us.

When an output is reloaded, the cross-reference stream looks unreferenced,
because `startxref` reaches it rather than the object graph, and so do the
object streams lopdf unpacked. Filter `/Type /XRef` and `/Type /ObjStm` out
before asserting that nothing leaked.

## Conventions

Rust comments explain why, not what. If a line exists because a specific file in
the corpus broke, name the file.

Prefer a test over a claim. The unit tests use generated fixtures so they run
without a corpus; anything corpus-specific belongs in `tests/corpus.rs`.

Run `cargo fmt` and `cargo clippy --all-targets`. Both are clean today.

`npm run lint` currently fails on five pre-existing UI errors listed in
`ISSUES.md`. Do not let that count grow.

Error strings returned from the engine are user-facing copy today. That is a
known wart, not a pattern to copy.
