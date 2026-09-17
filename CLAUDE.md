# CLAUDE.md

See [AGENTS.md](./AGENTS.md). It is the single source for how this repo is
built, tested and structured, and it applies here unchanged.

Two things worth knowing before you touch anything:

The wasm engine is generated and not committed. If Vite reports
`Failed to resolve import "./wasm/plico_engine.js"`, run `npm run build:wasm`.
The `predev`, `prebuild` and `precheck` hooks do this for you.

`ISSUES.md` lists open problems and the bugs already fixed, with the reason each
fix exists. Read it before changing `rust/plico-engine/src/lib.rs`. The engine
invariants section of AGENTS.md is the short version, and every entry in it was a
real bug that produced output which parsed cleanly and was wrong.
