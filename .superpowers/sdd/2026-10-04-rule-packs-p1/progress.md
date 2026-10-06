# Progress ledger

## 2026-10-06 — Project navigation links

- [complete] Add compact, keyboard-accessible links from the browser clients to the Yemelianov portfolio and the NetHackED source repository.
- Scope: static HTML, CSS, and focus guards only; no simulation, WASM, or ruleset changes.
- Validation: format, Clippy, workspace tests, golden determinism, wasm target build, documentation/data checks, and Lean passed. Keyboard Enter smoke tests passed on the clean Cloudflare page and developer pages at 320px and 390px widths.
- Deployment build: the installed `wasm-pack` is x86_64 and fails under the current arm64 CommandLine Tools. The equivalent native `cargo --release` plus `wasm-bindgen` build completed, and the normal Cloudflare packaging script completed through a local compatible shim.
