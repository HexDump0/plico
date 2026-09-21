# AGENTS.md

Working notes for Plico, an open-source local-first PDF toolbox. Files never
leave the browser: a SvelteKit front end hands bytes to a Rust engine compiled to
wasm, running in a worker.

Read `ISSUES.md` before engine work. It records what is broken, what was already
fixed and why, and which fixes exist only because a specific real file broke.

## Layout

```
rust/plico-engine/src/lib.rs      public engine API and module map
rust/plico-engine/src/documents.rs page selection, assembly, and safe renumbering
rust/plico-engine/src/compression.rs compression pipeline and stream packing
rust/plico-engine/src/compression/image_transcode.rs image recompression
rust/plico-engine/src/images.rs   image-to-PDF input
rust/plico-engine/src/bindings.rs browser/wasm entry points
rust/plico-engine/src/tests.rs    generated-fixture unit tests
rust/plico-engine/tests/corpus.rs structural checks over a real PDF corpus
src/lib/components/ToolWorkspace.svelte tool layout and settings
src/lib/components/DocumentCards.svelte card previews, ordering, and removal
src/lib/pdf/download-job.svelte.ts result and cancellation lifecycle
src/lib/pdf/processor.ts          main-thread side, owns the worker
src/lib/pdf/worker.ts             worker side, calls into wasm
src/lib/pdf/wasm/                 generated, gitignored, never edit
src/lib/tool-catalog.ts           the ~40 advertised tools
scripts/raster-compare.mjs        rendered-output comparison over the corpus
testing/                          local corpus, gitignored
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

## Engine invariants

Each of these is load-bearing and at least one was a real bug. Breaking them
tends to produce output that parses cleanly and is wrong, which is the worst
failure mode this codebase has.

Decrypt before renumbering. Below `/V 5` the encryption key is derived from each
object's number and generation, so moving objects first produces garbage.

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

Set `/Root` before pruning. `prune_objects()` sweeps from the trailer, so pruning
first deletes the entire document.

Give each input a disjoint id range, and reserve one id past each range for
unresolvable references.

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
