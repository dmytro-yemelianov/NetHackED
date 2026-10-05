# Web Playable Build and Rule Pack Tooling — Design

**Date:** 2026-10-05
**Branch:** `feat/web-packs`
**Status:** Approved in conversation; written spec pending review.

## Intent

The user wants:
- the game playable at a public URL;
- a browser tool for viewing and managing rule packs;
- a few CLI additions for packs.

What the user stated:
- Hosting is GitHub Pages, and the repository is made public.
- Pack scope covers all four areas: load and play in the browser; view, inspect and diff; edit and export in the browser; CLI improvements.

What I assumed (the user can correct these):
- The site is static. There is no backend.
- Uploaded packs stay in the visitor's browser, in IndexedDB. Packs are never uploaded to a server.
- The current benchmark and RL panels in `web/index.html` stay and keep working.

Success criteria:
1. On every push to `main`, a deployed site appears at `https://dmytro-yemelianov.github.io/NetRust/`.
2. On that site, a visitor can start a game with any of these rulesets:
   - vanilla;
   - a bundled example pack;
   - an uploaded `.nrpack` file;
   - an uploaded set of pack files.
3. The pack manager lists packs. For each pack it shows:
   - the manifest, the hash, and its monsters, items and roles;
   - validation diagnostics;
   - a diff against vanilla.
   It can also edit patch values and download a built `.nrpack`.
4. A `.nrpack` built in the browser is byte-identical to one built by the CLI from the same files, and has the same hash.
5. New CLI commands: `netrust-pack list`, `netrust-pack info` and `netrust-pack install`. `netrust-tui --pack <name>` resolves packs installed by name.
6. The existing gates stay green: fmt, clippy `-D warnings`, `--locked` tests, golden determinism, the wasm build, doc links, the static-table guard, and Lean.

## Current state

- `web/index.html` is about 2,100 lines with inline JS and CSS. It loads `./pkg/netrust_wasm.js`, built locally with `wasm-pack build crates/netrust-wasm --target web --out-dir ../../web/pkg`. Nothing deploys it.
- `netrust-wasm` exposes `WasmGameSession` (vanilla ruleset only), `TournamentRun`, `run_tournament_benchmark` and `run_tactical_trajectory`.
- `netrust-pack` is both a library and a CLI. The library is path-based:
  - `read_pack_dir(&Path)`, `build(&Path)`, `load_nrpack(&Path)`, `write_nrpack`;
  - plus the pure `resolve`, `validate`, `diff` and `ruleset_hash`.
  - `clap` is an unconditional dependency.
- A `.nrpack` is canonical JSON (`NrPack { format, manifest, ruleset, hash }`) with a SHA-256 integrity hash.
- `SimulationWorld` holds an `Arc<Ruleset>`. The TUI already loads packs with `--pack <path>`.
- The repo is private, and GitHub Pages is not enabled.

## Design

### A. `netrust-pack`: in-memory API, wasm-compatible

- Add `PackFiles`: an in-memory map from relative path (`pack.toml`, `monsters.toml`, `items.toml`, `roles.toml`, `i18n/<locale>.toml`) to UTF-8 text.
- `parse_pack_files(&PackFiles) -> Result<PackDir, PackError>`. This holds the parsing logic that currently lives in `read_pack_dir`.
- `read_pack_dir(&Path)` becomes a thin wrapper: it reads the files into `PackFiles` and calls `parse_pack_files`.
- `build_from_files(&PackFiles) -> Result<(NrPack, Report), PackError>`. `build(&Path)` wraps it.
- `nrpack_to_bytes(&NrPack) -> Vec<u8>`. This is the same pretty canonical JSON that `write_nrpack` writes. `write_nrpack` wraps it.
- `load_nrpack_bytes(&[u8]) -> Result<(Arc<Ruleset>, RulesetRef, NrPack), PackError>`. It checks the format and hash and reindexes the ruleset. `load_nrpack(&Path)` wraps it and returns its current tuple.
- `export_vanilla_files() -> PackFiles`. The `export-vanilla` CLI writes these files out.
- `clap` moves behind a `cli` feature, enabled by default and required by the `netrust-pack` binary (`required-features = ["cli"]`). `run_simulation` stays in the library.
- The path-based functions stay compiled on every target with no `cfg` gating. `std::fs` compiles on wasm32-unknown-unknown and only fails at runtime. The wasm crate calls only the in-memory API.

### B. `netrust-wasm`: pack bindings

New `#[wasm_bindgen]` items:

- `WasmPack` (opaque handle) with:
  - `WasmPack::from_nrpack(bytes: &[u8]) -> Result<WasmPack, JsError>`
  - `WasmPack::from_files(files_json: &str) -> Result<WasmPack, JsError>`. The argument is a JSON object mapping path to text. This builds the pack in memory.
  - `WasmPack::vanilla() -> WasmPack`
  - `manifest_json()`, `hash()`, `report_json()` (diagnostics with severity and location), `ruleset_json()` (monsters, items, roles for the tables), `diff_vs_vanilla_json()`, and `nrpack_bytes() -> Vec<u8>` (for download)
- `vanilla_pack_files_json() -> String`. These are the files from `export_vanilla`, used to seed the editor's "new pack from vanilla".
- `WasmGameSession::new_with_pack(seed, role, race, gender, alignment, name, pack: &WasmPack)`. It mirrors `new_with_character` but uses the pack's `Arc<Ruleset>` and records its `RulesetRef`.

Errors cross the boundary as `JsError` with the `PackError` display text.

### C. Website (`web/`)

Plain ES modules with no JS build step. Files:

| File | Purpose |
|---|---|
| `web/index.html` | Game page (existing UI). Inline JS/CSS move to modules and a stylesheet. A pack picker is added. |
| `web/packs.html` | Pack manager page |
| `web/css/app.css` | Shared styles, taken from the current inline `<style>` |
| `web/js/wasm.js` | One-time `init()` of `pkg/netrust_wasm.js`, shared by both pages |
| `web/js/game.js` | Game session UI: render, input, HUD (moved from `index.html`) |
| `web/js/bench.js` | Benchmark and RL panels (moved from `index.html`) |
| `web/js/pack-store.js` | Pack catalog. Bundled packs come from `packs/index.json`. Uploaded and edited packs are stored in IndexedDB as `.nrpack` bytes plus their source files when they exist. Records are keyed by hash. |
| `web/js/packs.js` | Pack manager UI |
| `web/packs/` | Generated at deploy: `index.json` (id, version, hash, title, file) and the bundled `*.nrpack` files built from `packs/examples/*` |

**Game page.** A "Ruleset" select lists vanilla, the bundled packs and the stored packs. A link next to it opens the pack manager. Starting a game with a pack calls `new_with_pack`. The HUD shows the active pack id and the first 12 characters of its hash. The selection is remembered in `localStorage`.

**Pack manager.** On the left, a list of packs: bundled, stored and "+ New from vanilla". It also accepts uploads: drop a `.nrpack`, or pick several pack files or a folder (`webkitdirectory`). On the right, tabs for the selected pack:
1. **Overview.** Manifest, hash, entry counts and validation status. Buttons: Play, Download `.nrpack`, Delete (stored packs only).
2. **Monsters / Items / Roles.** Sortable, filterable tables of the resolved ruleset. Rows changed from vanilla are highlighted.
3. **Diff.** `diff_vs_vanilla` as a table with table, entry, field, before and after columns.
4. **Diagnostics.** Errors and warnings from the report.
5. **Edit.** Shown only for packs that have source files: bundled packs (their sources are shipped next to the `.nrpack`), packs uploaded as files, and new packs. It has two modes:
   - a form editor for the patch entries (add, remove, or change fields on the monster, item and role patches);
   - a raw TOML editor per file.
   Each edit rebuilds through `WasmPack::from_files`, debounced at 300 ms, and refreshes the diagnostics and diff. "Save" stores the result in IndexedDB. "Download .nrpack" saves the built pack. "Download sources" saves each TOML file of the pack as its own download. No zip library is used.

The form editor's field list comes from the patch schemas, which `netrust-pack schema` already emits. The deploy step writes them to `web/packs/schema/*.json`, and the editor renders inputs from the JSON schema properties: numbers, booleans, strings and enums. Unknown or complex fields can still be edited in raw mode.

### D. CLI additions (`netrust-pack`) and TUI

- `netrust-pack list [--dir <packs-dir>]` lists installed packs (id, version, hash, path). The default dir is `$NETRUST_PACKS_DIR`, or else `~/.netrust/packs`.
- `netrust-pack info <path-or-name>` prints the manifest, hash, entry counts, validation summary and the number of diffs against vanilla.
- `netrust-pack install <path> [--dir <packs-dir>]` validates, builds the pack if it is a directory, and copies `<id>-<version>.nrpack` into the packs dir. It refuses to overwrite a different hash under the same `id` and `version` unless `--force` is given.
- `netrust-tui --pack <arg>`: if `<arg>` is an existing path, it loads that as today. Otherwise it resolves `<arg>` as an installed pack `id`, taking the highest version. It errors if nothing is found.

### E. Deploy and making the repo public

- `.github/workflows/pages.yml`:
  - **On push to `main` and on manual dispatch:**
    1. Check out, set up the Rust toolchain (1.88.0, target wasm32), and install `wasm-pack`.
    2. `wasm-pack build crates/netrust-wasm --target web --release --out-dir ../../web/pkg`.
    3. `cargo run -p netrust-pack -- build` for each `packs/examples/*`, writing to `web/packs/`. Then generate `web/packs/index.json`, copy each pack's source TOML to `web/packs/<id>/`, and write the schemas to `web/packs/schema/`.
    4. Upload `web/` as the Pages artifact and deploy with `actions/deploy-pages`.
  - **On PRs:** run steps 1–3 only (a build check), with no deploy.
- `web/pkg/` and the generated `web/packs/*` stay gitignored. Checked-in example sources stay under `packs/examples/`.
- The site must work under the `/NetRust/` base path. All URLs in the pages are relative.
- **Before making the repo public** (one-time manual step, done with the user's go-ahead given in this conversation):
  1. Scan the full history for secrets. Run `gitleaks` if it is available. Otherwise use `git log -p` with grep patterns for keys, tokens, `.env` and private keys.
  2. Check for large or unwanted tracked files (for example `.venv/`, local absolute paths, personal notes under `.superpowers/`).
  3. Report the findings. If anything is found, stop and ask before rewriting history. If the scan is clean, run `gh repo edit --visibility public --accept-visibility-change-consequences`. Then enable Pages with source "GitHub Actions" (`gh api -X POST repos/{owner}/{repo}/pages -f build_type=workflow`).
- Update the README "Web" section with the live URL and the local build command.

### F. Testing

- **`netrust-pack` unit tests:**
  - `build_from_files` on the files read from `packs/examples/hard-mode` gives the same `NrPack` and hash as `build(&Path)`.
  - `nrpack_to_bytes` and then `load_nrpack_bytes` round-trip.
  - A tampered hash is rejected.
  - `export_vanilla_files` rebuilt with no changes gives a ruleset whose hash equals vanilla's.
- **CLI tests** (`assert_cmd`-style, or direct function tests if that crate isn't already a dependency):
  - `install` then `list` shows the pack; `info` prints the hash.
  - `install` refuses a hash conflict.
  - TUI `parse_args` resolves a pack by name from a temp packs dir.
- **`netrust-wasm` tests:** native `#[test]`s on the rlib for `WasmPack::from_files`, `from_nrpack`, `diff_vs_vanilla_json` and `new_with_pack` (the game starts and `get_observation_json` parses). Add `wasm-bindgen-test` only if the native tests can't cover a boundary concern.
- **CI:** the existing wasm build job keeps building. `pages.yml` on PRs checks that the site build succeeds.
- **Manual or local smoke test** with headless Playwright against `python3 -m http.server -d web`:
  - the game page loads and a vanilla game starts;
  - the bundled `hard-mode` pack starts a game;
  - the pack manager shows tables and the diff;
  - an edit changes the diff;
  - the download produces a `.nrpack` that `netrust-pack validate` accepts.
- Golden determinism must not change. No simulation logic is touched.

## Out of scope

- A server-side pack registry, accounts or sharing links.
- Base packs other than `vanilla`. The pack format already rejects them.
- Editing dungeon generation or anything outside the monster, item, role and i18n patches.
- A JS framework or bundler.

## Risks

- **Wasm size growth from `toml` and `sha2`.** Both are small. Check the release `.wasm` size before and after, and report it.
- **Moving the inline `index.html` JS into modules could break the existing panels.** Do it as a pure move first, smoke-test it, then add features.
- **Making the repo public is irreversible in practice**, since forks and caches persist. Mitigation: the history scan is a hard gate before the visibility change.
