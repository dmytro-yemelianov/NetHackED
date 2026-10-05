# Web and WebAssembly: One Engine, One Pack Pipeline

This article describes how NetHackED runs in the browser. It also explains why rule packs built in a browser and packs built by the CLI are byte-identical. It is meant for contributors who change the web clients, the wasm bindings or the deploys.

---

## 1. The pieces

```
crates/nethacked-pack  ── in-memory pack API (PackFiles → NhPack) ──┐
                                                                  ├── crates/nethacked-wasm ── wasm-pack ── web/pkg/
crates/nethacked-sim / agent / data  ── engine, commands, tables ───┘
                                                                        │
web/play/      clean 80×24 terminal (Cloudflare: nethacked.yemelianov.dev)┤
web/index.html developer client + web/packs.html pack manager (GitHub Pages)
```

- **`nethacked-wasm`** is a thin `wasm-bindgen` layer.
  - `WasmGameSession` wraps the engine's `AgentSession`. It uses the same command parser as the MCP, JSON-RPC and GraphQL frontends.
  - `WasmPack` wraps a resolved rule pack.
  - `WasmGameSession.newWithPack(seed, role, race, name, pack)` starts a game on a pack.
- **`web/play/`** is the clean game: one `<pre>` element with an 80×24 grid. Keyboard prompts imitate NetHack's (`What do you want to eat? [a-c or ?*]`). The map comes straight from the engine's `render_ascii()`, and the page only colours glyphs by class.
- **`web/index.html` and `web/packs.html`** form the developer client: canvas tiles, AI arena, benchmarks, and the pack manager and editor. They are plain ES modules with no bundler and no npm dependencies.

---

## 2. One pack pipeline for the CLI and the browser

The rule pack library used to be path-based: it read a directory and wrote a file. A browser has no filesystem, so the library now has an in-memory core, and the path functions are thin wrappers around it:

| In-memory core | Path wrapper (CLI) |
|---|---|
| `PackFiles` (relative path → text) | `read_pack_files_from_dir(&Path)` |
| `parse_pack_files(&PackFiles)` | `read_pack_dir(&Path)` |
| `build_from_files(&PackFiles)` | `build(&Path)` |
| `nhpack_to_bytes(&NhPack)` | `write_nhpack(&NhPack, &Path)` |
| `load_nhpack_bytes(&[u8])` | `load_nhpack(&Path)` |

Both paths run the same parse → resolve → validate → hash code, so a pack built in the browser is byte-identical to `nethacked-pack build` of the same files. This was checked with `cmp`, and `pack_files.rs` tests it on every CI run.

Browser uploads are messier than files on disk, so `PackFiles` normalises them first:
- Windows line endings and a UTF-8 BOM are removed.
- A folder upload such as `my-pack/pack.toml` is treated as the pack root.

Loading a pack checks more than its hash, because anyone can recompute a SHA-256. A loaded `.nhpack` must also:
- have an outer manifest that equals the manifest inside the hashed ruleset;
- have an `id` and `version` that are safe slugs, since they become file names in `nethacked-pack install`;
- pass full validation.

---

## 3. The pack editor's TOML handling

The web pack editor has a form mode and a raw TOML mode. Form mode rewrites a whole file from the entries it parsed, so it must never rewrite a line it does not fully understand. `web/js/toml-lite.js` accepts only lines it can write back exactly: `key = true | false | integer | "plain string"`, plus comments before the first entry.

Anything else makes that file read-only in form mode, and the user edits it as raw TOML instead. That covers trailing comments, escapes, literal strings, `1_000`, hex, floats, dotted keys, nested tables, arrays and inline tables. `web/js/toml-lite.test.mjs` covers each of these cases and runs in CI with `node --test`.

---

## 4. Deploys

| Target | Content | Built by | Deployed by |
|---|---|---|---|
| GitHub Pages (`dmytro-yemelianov.github.io/NetHackED/`) | the whole `web/` directory | `scripts/build-web.sh --release` | `.github/workflows/pages.yml`: builds on every PR, deploys on `main` |
| Cloudflare (`nethacked.yemelianov.dev`) | `web/play/`, `pkg/` and `packs/` only | `scripts/build-cf.sh` | `cd deploy/cloudflare && npx wrangler deploy` |

`scripts/build-web.sh` builds three things:
- the wasm package;
- every pack in `packs/examples/` into `web/packs/<id>.nhpack`, plus the pack sources and an `index.json` with each pack's hash;
- the pack JSON schemas, which the editor uses to type its form fields.

The Cloudflare site is an **assets-only Worker**: there is no Worker code, only static files on a custom domain. Its `_headers` file sets a strict Content-Security-Policy. Only same-origin scripts can run, and wasm compilation is allowed through `'wasm-unsafe-eval'`. This fits because the clean page has no inline scripts and no third-party code.

The page finds its assets through a single `<meta name="nethacked-base">` tag. Its value is `../` inside `web/play/` and `./` at the Cloudflare site root. `build-cf.sh` rewrites it.

---

## 5. Size

| Build | `.wasm` | gzip |
|---|---|---|
| Engine only (v0.2.0) | 645 KB | — |
| Engine + in-browser pack tooling (v0.3.0) | 1.7 MB | 557 KB |

The growth comes from `toml` and `toml_edit`, `sha2`, and the `Deserialize` impls for every data table. These are needed to build and verify packs in the browser. If first-load time becomes a problem, the pack tooling can be split into a second, lazily loaded wasm module. The clean game page would then only download the engine.

---

## 6. Testing the web clients

- **Rust side:** `cargo test -p nethacked-pack -p nethacked-wasm` runs natively and covers pack round-trips, rejection of garbage and tampered packs, starting a session on a pack, and the views the pages read.
- **TOML editor:** `node --test web/js/toml-lite.test.mjs`.
- **Pages:** smoke tests use headless Playwright against `python3 -m http.server -d web`, or `npx wrangler dev` in `deploy/cloudflare/` to serve with the production headers. They check that a game starts, keys move the hero, prompts and overlays work, `?pack=` loads a bundled pack, and the console stays clean.
