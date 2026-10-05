# NetHackED: NetHack Mechanics in Rust, with Lean 4 Models

*NetHackED — from Yemelianov (Emelyanov Dmytro)*

NetHackED is a Rust reimplementation of selected **NetHack** mechanics (derived from the NetHack 5.0.0 source), accompanied by **Lean 4** models of those mechanics in `NetMechanics/`.

**Play now:** [nethacked.yemelianov.dev](https://nethacked.yemelianov.dev/) (clean keyboard terminal) · [developer web client](https://dmytro-yemelianov.github.io/NetHackED/) · [pack manager](https://dmytro-yemelianov.github.io/NetHackED/packs.html) · terminal and SSH: see [Playing NetHackED](docs/playing-nethacked.md)

![NetHackED in the browser](docs/images/play-pixel.png)


What is and is not claimed:

* The Lean 4 models of selected mechanics are machine-checked (`lake build`). They are simplified abstractions, not a transcription of NetHack 5.0.
* The Rust engine is an **independent, hand-written implementation**, tested against those models with property tests (`proptest`). The Lean models and the Rust code are **not formally linked**; nothing machine-checks that the Rust code satisfies the Lean theorems.
* Fidelity to the NetHack C source is **partial**; known divergences are listed in [docs/formal-mechanics-spec.md](docs/formal-mechanics-spec.md).

Frontends: a **WebAssembly (WASM)** browser terminal, a human **terminal TUI**, a **Model Context Protocol (MCP)** server, a line-delimited **JSON** stdio stream, and a **GraphQL** API for autonomous agents and reinforcement learning.

> `NetHack-5.0.0/` is a local, git-ignored reference copy of the NetHack source; obtain it from nethack.org. Paths into it in this repository are plain code paths, not links.

---

## Documentation Suite & Knowledge Base

The repository includes a comprehensive, formal architectural reference suite in [docs/](docs/):

1. **[docs/ontology-and-taxonomy.md](docs/ontology-and-taxonomy.md)**:
   * **Domain Ontology**: Spatial, Dynamic, and Abstract entity graphs.
   * **Taxonomies**: Complete 17-class item classification, 4-tier epistemic/identification states, intrinsic/extrinsic sources, combat modalities, damage types, dungeon branches, and turn action costs.

2. **[docs/formal-mechanics-spec.md](docs/formal-mechanics-spec.md)**:
   * **Mathematical Specifications**: Exact formulas and state machine recurrences for BUC transitions, Acyclic container hierarchies, Bag of Holding weight scaling, Encumbrance tiers, Turn speed scheduler, Grid/Door DFAs, and AD&D descending AC combat formulas.
   * **Legacy C Cross-References**: Mapping of formulas to legacy routines in `NetHack-5.0.0/src/`, and a list of known divergences from C.

3. **[docs/c-to-rust-migration-architecture.md](docs/c-to-rust-migration-architecture.md)**:
   * **Architectural Blueprint**: Eliminating global ambient state (`ga`..`gz`, `sv*`, `u`), transitioning from intrusive pointers (`union vptrs`) to generational handle arenas (`slotmap`), event-driven presentation decoupling, and a single seeded ChaCha8 RNG (cross-platform replay not yet tested).
   * **Crate Graph**: Multi-crate workspace organization (`nethacked-types`, `nethacked-core`, `nethacked-arena`, `nethacked-data`, `nethacked-dungeon`, `nethacked-sim`, `nethacked-agent`, `nethacked-tui`, `nethacked-wasm`, plus `nethacked-py` and `nethacked-i18n`).

4. **[docs/lean4-verification-guide.md](docs/lean4-verification-guide.md)**:
   * **Verification Reference**: Formal catalog of machine-checked theorems in Lean 4.
   * **Property Bridge**: Hand-maintained mapping of selected Lean 4 theorems to Rust property-based tests (`proptest`), plus known limitations of the Lean models.

5. **[docs/agent-and-mcp-integration.md](docs/agent-and-mcp-integration.md)**:
   * **AI Agent & LLM Guide**: Connecting autonomous agents and subagents via the Model Context Protocol (MCP), JSON line streaming, and GraphQL.
   * **Tool Catalog**: `nethacked_get_observation`, `nethacked_step`, `nethacked_inspect_tile`, `nethacked_render_map`, `nethacked_reset_game`, `nethacked_get_roles`, `nethacked_reset_with_character`.

6. **[docs/architecture.md](docs/architecture.md)**:
   * **Architecture Article**: How NetHackED is built today compared with the original NetHack 5.0 C architecture: crate layering, entity arenas, action/event model, deterministic RNG, level persistence, measured performance, reliability measures, NetHack fidelity status, and known limitations.

7. **[docs/rule-packs.md](docs/rule-packs.md)**:
   * **Rule Pack Author Guide**: Declarative monster, item, and role modding, format specification, validation rules, CLI tool, and replay-deterministic execution.

8. **[docs/playing-nethacked.md](docs/playing-nethacked.md)**:
   * **Player Guide**: Playing in the browser (clean terminal and developer client), in a terminal, and over SSH, with keys, URL options, and a hardened `ssh play@host` setup.

9. **[docs/web-and-wasm.md](docs/web-and-wasm.md)**:
   * **Web & WebAssembly Article**: How the browser builds work, the shared CLI/browser rule pack pipeline (byte-identical builds), the pack editor's lossless TOML handling, GitHub Pages and Cloudflare deploys, and wasm size.

---

## Repository Structure

```
NetHackED/
├── LICENSE                                # NetHack General Public License (NGPL)
├── docs/                                  # Specifications, ontologies, guides
│   ├── ontology-and-taxonomy.md
│   ├── formal-mechanics-spec.md           # Mathematical rules + known divergences from C
│   ├── c-to-rust-migration-architecture.md
│   ├── lean4-verification-guide.md        # Lean theorem catalog, proptest bridge, limitations
│   ├── agent-and-mcp-integration.md       # MCP, JSON stream, GraphQL, bones server
│   ├── master-roadmap-to-nethack-parity.md
│   └── superpowers/                       # Design specs and implementation plans
├── NetMechanics/                          # 40 Lean modules (BUC, Inventory, Combat, FOV, Raycast, Polymorph, Nutrition, Religion, Traps, ...)
├── NetMechanics.lean                      # Lean 4 root module
├── Main.lean                              # Demo executable (prints example evaluations; not a verification harness)
├── lakefile.toml                          # Lake build definition
├── crates/
│   ├── nethacked-types/                     # Coordinates, tiles, ontology enums
│   ├── nethacked-arena/                     # Generational handles (SlotMap) for actors/items
│   ├── nethacked-dungeon/                   # Grid level, FOV, procedural + special level generators
│   ├── nethacked-data/                      # Bestiary, item catalog, roles & races
│   ├── nethacked-sim/                       # Seeded simulation world, AI, containers, altars
│   ├── nethacked-core/                      # Rust mirror of modeled mechanics + proptests
│   ├── nethacked-agent/                     # Agent session, MCP / JSON / GraphQL servers, bones server
│   │   └── src/bin/                       # nethacked-mcp, nethacked-jsonrpc, nethacked-graphql,
│   │                                      # nethacked-benchmark, nethacked-demo, nethacked-bones-server
│   ├── nethacked-tui/                       # Terminal UI (binary: nethacked)
│   ├── nethacked-wasm/                      # WebAssembly bindings and browser simulation runner
│   ├── nethacked-py/                        # PyO3 bindings used by the Gym environment
│   └── nethacked-i18n/                      # Localized strings (English, Ukrainian)
├── python/                                # Gymnasium environment (nethacked_gym), train_reinforce.py, demo_rl.py
├── web/                                   # Browser client
│   ├── index.html                         # Playable client + agent telemetry inspector
│   ├── benchmark_report.json              # Generated by nethacked-benchmark
│   ├── policy_weights.json                # Generated by python/train_reinforce.py
│   └── pkg/                               # wasm-pack output (generated; see below)
├── examples/                              # Agent integration examples (mcp_agent_client.py)
├── scripts/                               # Repository tooling (check-doc-links.sh)
├── Cargo.toml                             # Rust workspace manifest
└── NetHack-5.0.0/                         # Local, git-ignored copy of the NetHack source (reference only)
```

---

## Verification & Execution

### 1. Build the Lean 4 Models
```bash
lake build
./.lake/build/bin/netmechanics   # demo executable (Main.lean)
```
`lake build` type-checks every theorem in `NetMechanics/`. See [docs/lean4-verification-guide.md](docs/lean4-verification-guide.md) for what the theorems do and do not establish.

### 2. Run Rust Unit & Property-Based Tests
```bash
cargo test --workspace --exclude nethacked-py
```
The `proptest` suite in `crates/nethacked-core/tests/proptest_mechanics.rs` checks the Rust code against properties mirrored by hand from selected Lean theorems.

### 3. Regenerate Generated Data
```bash
cargo run --release --bin nethacked-benchmark   # writes web/benchmark_report.json (run from the repo root)
python3 python/train_reinforce.py                   # writes web/policy_weights.json (needs the python/ extras)
```

### 4. Check Documentation Links
```bash
scripts/check-doc-links.sh
```

### 5. Run the CI Checks Locally
```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --exclude nethacked-py --locked
cargo build -p nethacked-wasm --target wasm32-unknown-unknown --locked   # needs: rustup target add wasm32-unknown-unknown
bash scripts/check-doc-links.sh
```

---

## Frontends & Interaction Options

### 1. Play in Web Browser (WebAssembly Terminal)
**Play online:** https://dmytro-yemelianov.github.io/NetHackED/ — rule pack manager: https://dmytro-yemelianov.github.io/NetHackED/packs.html

No backend server required. NetHackED compiles to a WebAssembly module that the page loads from `web/pkg/`. Build it first (requires [`wasm-pack`](https://rustwasm.github.io/wasm-pack/)):
```bash
# Build the web client: wasm package, bundled rule packs and pack schemas
scripts/build-web.sh            # add --release for an optimized build

# Serve the web/ directory locally
python3 -m http.server 8080 --directory web
```
Then visit **`http://localhost:8080`** in your browser:
* Retro CRT terminal aesthetic with green glow and scanlines.
* Interactive Character Creator modal (`Valkyrie`, `Wizard`, `Barbarian`, `Rogue`, `Knight`, `Monk`, `Healer`, `Tourist`, `Archaeologist`).
* Keyboard controls (Vi-keys `h/j/k/l/y/u/b/n`, arrows, `.`, `,`, `p` to pay, `P` to pray, `S` to sacrifice).
* Live JSON Telemetry pane for autonomous agent inspection.

**Rule packs in the browser.** Pick a ruleset (vanilla, a bundled example pack, or one you uploaded) in the character dialog. The pack manager (`packs.html`) lists packs; shows monsters, items and roles, validation diagnostics and a diff against vanilla; accepts `.nhpack` files, pack `.toml` files or a pack folder; and edits patches in a form or as raw TOML, then downloads the built `.nhpack`. Uploaded packs stay in your browser (IndexedDB).

**Rule packs on the command line.**
```bash
nethacked-pack install packs/examples/hard-mode   # build + copy into ~/.nethacked/packs (or $NETHACKED_PACKS_DIR)
nethacked-pack list                               # id, version, hash, path of installed packs
nethacked-pack info hard-mode                     # manifest, hash, counts, diff size vs vanilla
nethacked-tui --pack hard-mode                    # play an installed pack by id (or pass a path)
```

### 2. Play in Terminal (Human Interactive TUI)
```bash
cargo run --bin nethacked
# Or load a custom rule pack (.nhpack or directory)
cargo run --bin nethacked -- --pack packs/examples/hard-mode
```
* Character selection prompt upon boot.
* Standard NetHack keyboard controls: Vi-keys, arrows, `o` (open), `c` (close), `K` (kick), `z` (zap wand), `,` (pick up), `p` (pay shopkeeper), `P` (pray at altar), `S` (sacrifice item), `d` (drop), `w` (wield), `<` (ascend), `>` (descend), `.` (wait), `Esc` (quit), `q` (quaff).

### 3. Model Context Protocol (MCP) Server for LLMs
For LLM pair programming and autonomous game agents (compatible with Claude Desktop, Cursor, and Antigravity):
```bash
cargo run --bin nethacked-mcp
```
* Discover and call tools: `nethacked_get_observation`, `nethacked_step`, `nethacked_inspect_tile`, `nethacked_render_map`, `nethacked_reset_game`, `nethacked_get_roles`, `nethacked_reset_with_character`.
* JSON-RPC 2.0 compliant: standard error codes, no reply to notifications, `ping` supported.

Run the bundled Python MCP client example:
```bash
python3 examples/mcp_agent_client.py
```

### 4. Autonomous Agent Loop Example (Rust)
Demonstrates an agent perceiving structured FOV state and navigating the generated dungeon:
```bash
cargo run --example autonomous_bot -p nethacked-agent
```

### 5. Streaming JSON Line-Protocol for RL Agents
```bash
cargo run --bin nethacked-jsonrpc
```
Feeds line-delimited JSON state snapshots and consumes JSON action commands via stdio.

### 6. Networked GraphQL Server & GraphiQL Explorer
```bash
cargo run --bin nethacked-graphql
```
* **Endpoint**: `http://127.0.0.1:4000/graphql` (binds to loopback by default; `PORT` changes the port)
* **Interactive GraphiQL Explorer**: Open `http://localhost:4000/graphql` to execute GraphQL queries (`playerState`, `observationJson`, `asciiMap`, `inspectTile`, `bestiary`, `itemCatalog`, `roles`, `races`) and mutations (`stepAction`, `resetGame`, `resetWithCharacter`).

**Binding and authentication (GraphQL and bones servers).** Both servers bind to `127.0.0.1` by default. Override the address with `--bind <addr>` or the `NETHACKED_BIND` environment variable (the flag wins), e.g. `cargo run --bin nethacked-graphql -- --bind 0.0.0.0:4000`. If `NETHACKED_TOKEN` is set, all GraphQL mutations and the bones server's `POST /api/v1/bones` and `POST /api/v1/reset` require the header `Authorization: Bearer <token>`. Set a token before exposing either server beyond localhost.

### 7. Networked Bones Server
```bash
cargo run --bin nethacked-bones-server   # default 127.0.0.1:7777
```
Stores and serves bones files and gravestone records over HTTP; see [docs/agent-and-mcp-integration.md](docs/agent-and-mcp-integration.md).

---

## License & Credits

NetHackED is distributed under the **NetHack General Public License (NGPL)**; see [LICENSE](LICENSE). Third-party components (the pixel-ssh renderer, MIT, and its CC BY-SA 4.0 bitmap fonts) are listed in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). It is derived from **NetHack** by the NetHack DevTeam, and NetHackED is not affiliated with or endorsed by the NetHack DevTeam.
