# NetRust: NetHack Mechanics in Rust, with Lean 4 Models

NetRust is a Rust reimplementation of selected **NetHack** mechanics (derived from the NetHack 5.0.0 source), accompanied by **Lean 4** models of those mechanics in `NetMechanics/`.

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
   * **Crate Graph**: Multi-crate workspace organization (`netrust-types`, `netrust-core`, `netrust-arena`, `netrust-data`, `netrust-dungeon`, `netrust-sim`, `netrust-agent`, `netrust-tui`, `netrust-wasm`, plus `netrust-py` and `netrust-i18n`).

4. **[docs/lean4-verification-guide.md](docs/lean4-verification-guide.md)**:
   * **Verification Reference**: Formal catalog of machine-checked theorems in Lean 4.
   * **Property Bridge**: Hand-maintained mapping of selected Lean 4 theorems to Rust property-based tests (`proptest`), plus known limitations of the Lean models.

5. **[docs/agent-and-mcp-integration.md](docs/agent-and-mcp-integration.md)**:
   * **AI Agent & LLM Guide**: Connecting autonomous agents and subagents via the Model Context Protocol (MCP), JSON-RPC streaming, and GraphQL.
   * **Tool Catalog**: `netrust_get_observation`, `netrust_step`, `netrust_inspect_tile`, `netrust_render_map`, `netrust_reset_game`, `netrust_get_roles`, `netrust_reset_with_character`.

---

## Repository Structure

```
NetRust/
├── LICENSE                                # NetHack General Public License (NGPL)
├── docs/                                  # Specifications, ontologies, guides
│   ├── ontology-and-taxonomy.md
│   ├── formal-mechanics-spec.md           # Mathematical rules + known divergences from C
│   ├── c-to-rust-migration-architecture.md
│   ├── lean4-verification-guide.md        # Lean theorem catalog, proptest bridge, limitations
│   ├── agent-and-mcp-integration.md       # MCP, JSON stream, GraphQL, bones server
│   ├── master-roadmap-to-nethack-parity.md
│   └── superpowers/                       # Design specs and implementation plans
├── NetMechanics/                          # 39 Lean modules (BUC, Inventory, Combat, FOV, Raycast, Polymorph, Nutrition, Religion, Traps, ...)
├── NetMechanics.lean                      # Lean 4 root module
├── Main.lean                              # Demo executable (prints example evaluations; not a verification harness)
├── lakefile.toml                          # Lake build definition
├── crates/
│   ├── netrust-types/                     # Coordinates, tiles, ontology enums
│   ├── netrust-arena/                     # Generational handles (SlotMap) for actors/items
│   ├── netrust-dungeon/                   # Grid level, FOV, procedural + special level generators
│   ├── netrust-data/                      # Bestiary, item catalog, roles & races
│   ├── netrust-sim/                       # Seeded simulation world, AI, containers, altars
│   ├── netrust-core/                      # Rust mirror of modeled mechanics + proptests
│   ├── netrust-agent/                     # Agent session, MCP / JSON / GraphQL servers, bones server
│   │   └── src/bin/                       # netrust-mcp, netrust-jsonrpc, netrust-graphql,
│   │                                      # netrust-benchmark, netrust-demo, netrust-bones-server
│   ├── netrust-tui/                       # Terminal UI (binary: netrust)
│   ├── netrust-wasm/                      # WebAssembly bindings and browser simulation runner
│   ├── netrust-py/                        # PyO3 bindings used by the Gym environment
│   └── netrust-i18n/                      # Localized strings (English, Ukrainian)
├── python/                                # Gymnasium environment (netrust_gym), train_ppo.py, demo_rl.py
├── web/                                   # Browser client
│   ├── index.html                         # Playable client + agent telemetry inspector
│   ├── benchmark_report.json              # Generated by netrust-benchmark
│   ├── policy_weights.json                # Generated by python/train_ppo.py
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
cargo test --workspace --exclude netrust-py
```
The `proptest` suite in `crates/netrust-core/tests/proptest_mechanics.rs` checks the Rust code against properties mirrored by hand from selected Lean theorems.

### 3. Regenerate Generated Data
```bash
cargo run --release --bin netrust-benchmark   # writes web/benchmark_report.json (run from the repo root)
python3 python/train_ppo.py                   # writes web/policy_weights.json (needs the python/ extras)
```

### 4. Check Documentation Links
```bash
scripts/check-doc-links.sh
```

---

## Frontends & Interaction Options

### 1. Play in Web Browser (WebAssembly Terminal)
No backend server required. NetRust compiles to a WebAssembly module that the page loads from `web/pkg/`. Build it first (requires [`wasm-pack`](https://rustwasm.github.io/wasm-pack/)):
```bash
# Build the web client
wasm-pack build crates/netrust-wasm --target web --out-dir ../../web/pkg

# Serve the web/ directory locally
python3 -m http.server 8080 --directory web
```
Then visit **`http://localhost:8080`** in your browser:
* Retro CRT terminal aesthetic with green glow and scanlines.
* Interactive Character Creator modal (`Valkyrie`, `Wizard`, `Barbarian`, `Rogue`, `Knight`, `Monk`, `Healer`, `Tourist`, `Archaeologist`).
* Keyboard controls (Vi-keys `h/j/k/l/y/u/b/n`, arrows, `.`, `,`, `p` to pay, `P` to pray, `S` to sacrifice).
* Live JSON Telemetry pane for autonomous agent inspection.

### 2. Play in Terminal (Human Interactive TUI)
```bash
cargo run --bin netrust
```
* Character selection prompt upon boot.
* Standard NetHack keyboard controls: Vi-keys, arrows, `o` (open), `c` (close), `K` (kick), `z` (zap wand), `,` (pick up), `p` (pay shopkeeper), `P` (pray at altar), `S` (sacrifice item), `d` (drop), `w` (wield), `<` (ascend), `>` (descend), `.` (wait), `q` (quit).

### 3. Model Context Protocol (MCP) Server for LLMs
For LLM pair programming and autonomous game agents (compatible with Claude Desktop, Cursor, and Antigravity):
```bash
cargo run --bin netrust-mcp
```
* Discover and call tools: `netrust_get_observation`, `netrust_step`, `netrust_inspect_tile`, `netrust_render_map`, `netrust_reset_game`, `netrust_get_roles`, `netrust_reset_with_character`.
* JSON-RPC 2.0 compliant: standard error codes, no reply to notifications, `ping` supported.

Run the bundled Python MCP client example:
```bash
python3 examples/mcp_agent_client.py
```

### 4. Autonomous Agent Loop Example (Rust)
Demonstrates an agent perceiving structured FOV state and navigating the verified dungeon:
```bash
cargo run --example autonomous_bot -p netrust-agent
```

### 5. Streaming JSON Line-Protocol for RL Agents
```bash
cargo run --bin netrust-jsonrpc
```
Feeds line-delimited JSON state snapshots and consumes JSON action commands via stdio.

### 6. Networked GraphQL Server & GraphiQL Explorer
```bash
cargo run --bin netrust-graphql
```
* **Endpoint**: `http://127.0.0.1:4000/graphql` (binds to loopback by default; `PORT` changes the port)
* **Interactive GraphiQL Explorer**: Open `http://localhost:4000/graphql` to execute GraphQL queries (`playerState`, `observationJson`, `asciiMap`, `inspectTile`, `bestiary`, `itemCatalog`, `roles`, `races`) and mutations (`stepAction`, `resetGame`, `resetWithCharacter`).

**Binding and authentication (GraphQL and bones servers).** Both servers bind to `127.0.0.1` by default. Override the address with `--bind <addr>` or the `NETRUST_BIND` environment variable (the flag wins), e.g. `cargo run --bin netrust-graphql -- --bind 0.0.0.0:4000`. If `NETRUST_TOKEN` is set, all GraphQL mutations and the bones server's `POST /api/v1/bones` and `POST /api/v1/reset` require the header `Authorization: Bearer <token>`. Set a token before exposing either server beyond localhost.

### 7. Networked Bones Server
```bash
cargo run --bin netrust-bones-server   # default 127.0.0.1:7777
```
Stores and serves bones files and gravestone records over HTTP; see [docs/agent-and-mcp-integration.md](docs/agent-and-mcp-integration.md).

---

## License & Credits

NetRust is distributed under the **NetHack General Public License (NGPL)**; see [LICENSE](LICENSE). It is derived from **NetHack** by the NetHack DevTeam, and NetRust is not affiliated with or endorsed by the NetHack DevTeam.
