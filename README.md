# Plico

An open-source, local-first PDF toolbox. Files are processed in your browser by a
Rust engine compiled to WebAssembly, and never uploaded.

## Development

You need Node, plus a Rust toolchain that can target wasm:

```sh
rustup target add wasm32-unknown-unknown
brew install wasm-pack
```

Then:

```sh
npm install
npm run dev
```

The wasm engine is generated and not committed. `npm run dev`, `npm run build`
and `npm run check` build it first, so you should not have to think about it. To
build it on its own:

```sh
npm run build:wasm
```

If Vite reports `Failed to resolve import "./wasm/plico_engine.js"`, that build
has not run.

Useful checks:

```sh
npm run check
npm run lint
npm run build
npm run test:engine
```

Format the project with:

```sh
npm run format
```

## The PDF engine

The engine lives in `rust/plico-engine`. `npm run test:engine` runs its unit
tests against generated fixtures.

There is also a structural test over a directory of real PDFs, which is ignored
by default because it needs files on disk:

```sh
git clone --depth 1 https://github.com/mozilla/pdf.js testing/pdfjs
npm run test:corpus
```

It merges every usable file in the corpus and checks page counts, page geometry
and object reachability in the result.

See [AGENTS.md](./AGENTS.md) for the engine's invariants and [ISSUES.md](./ISSUES.md)
for what is known to be broken.
