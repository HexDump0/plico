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
src/lib/pdf/crop-area.ts                                    content bounds and padding, shared by preview and worker
src/lib/pdf/signature.ts                                    drawn, typed and uploaded signatures as trimmed PNGs
src/lib/pdf/annotations.ts                                  what Flatten will draw, read with pdf.js for the preview
src/lib/pdf/redact-text.ts                                  finding text, picking words, and what a box removes
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
9 needing a real password, 924 merged and checked, output at 87.1% of input size.
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

## lopdf notes

Pinned at 0.44 with `default-features = false, features = ["wasm_js"]`.

`renumber_objects_with` has an undocumented phase that permutes page objects so
page order matches id order. The engine no longer depends on that, and should
not start.

`save_to` already writes a cross-reference _stream_, not a classic table. Object
streams are the remaining size win and live behind `save_modern()`.

`Stream::compress` only acts when `/Filter` is absent, so already-compressed
streams are not touched. Most real streams arrive compressed, which means merge
cost is dominated by parsing and serialising rather than deflate.

`prune_objects()` is a real reachability sweep rooted at the trailer, so orphan
cycles are collected.

There is no helper for inherited attributes. That is on us.

When a merged file is reloaded, the cross-reference stream object looks
unreferenced, because `startxref` reaches it rather than the object graph. Filter
`/Type /XRef` out before asserting that nothing leaked.

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
