# NetRust: NetHack 5.0 Formally Verified in Lean 4 & Implemented in Safe Rust

This repository houses the formal specification, verification, and migration of **NetHack 5.0.0** to safe, deterministic Rust (**NetRust**), backed by machine-checked proofs in **Lean 4**. It features rich interactive frontends: a retro **WebAssembly (WASM)** browser terminal, a human **terminal TUI**, an open **Model Context Protocol (MCP)** server, a high-throughput **JSON-RPC** stream, and a networked **GraphQL** API for autonomous AI agents and reinforcement learning.

---

## Documentation Suite & Knowledge Base

The repository includes a comprehensive, formal architectural reference suite in [docs/](docs/):

1. **[docs/ontology-and-taxonomy.md](docs/ontology-and-taxonomy.md)**:
   * **Domain Ontology**: Spatial, Dynamic, and Abstract entity graphs.
   * **Taxonomies**: Complete 17-class item classification, 4-tier epistemic/identification states, intrinsic/extrinsic sources, combat modalities, damage types, dungeon branches, and turn action costs.

2. **[docs/formal-mechanics-spec.md](docs/formal-mechanics-spec.md)**:
   * **Mathematical Specifications**: Exact formulas and state machine recurrences for BUC transitions, Acyclic container hierarchies, Bag of Holding weight scaling, Encumbrance tiers, Turn speed scheduler, Grid/Door DFAs, and AD&D descending AC combat formulas.
   * **Legacy C Cross-References**: Direct mapping of formulas to legacy routines in [NetHack-5.0.0/src/](NetHack-5.0.0/src/).

3. **[docs/c-to-rust-migration-architecture.md](docs/c-to-rust-migration-architecture.md)**:
   * **Architectural Blueprint**: Eliminating global ambient state (`ga`..`gz`, `sv*`, `u`), transitioning from intrusive pointers (`union vptrs`) to generational handle arenas (`slotmap`), event-driven presentation decoupling, and bit-exact deterministic PRNG replay.
   * **Crate Graph**: Multi-crate workspace organization (`netrust-types`, `netrust-core`, `netrust-arena`, `netrust-data`, `netrust-dungeon`, `netrust-sim`, `netrust-agent`, `netrust-tui`, `netrust-wasm`).

4. **[docs/lean4-verification-guide.md](docs/lean4-verification-guide.md)**:
   * **Verification Reference**: Formal catalog of machine-checked theorems in Lean 4.
   * **Property Bridge**: Direct mapping of Lean 4 theorems to Rust property-based tests (`proptest`).

5. **[docs/agent-and-mcp-integration.md](docs/agent-and-mcp-integration.md)**:
   * **AI Agent & LLM Guide**: Connecting autonomous agents and subagents via the Model Context Protocol (MCP), JSON-RPC streaming, and GraphQL.
   * **Tool Catalog**: `netrust_get_observation`, `netrust_step`, `netrust_inspect_tile`, `netrust_render_map`, `netrust_reset_game`, `netrust_get_roles`, `netrust_reset_with_character`.

---

## Repository Structure

```
NetRust/
├── docs/                                  # Master specifications & ontologies
│   ├── ontology-and-taxonomy.md           # Domain ontology and taxonomy
│   ├── formal-mechanics-spec.md           # Mathematical rule specifications
│   ├── c-to-rust-migration-architecture.md# Rust crate architecture & C migration
│   ├── lean4-verification-guide.md        # Lean 4 proof catalog & proptest bridge
│   └── agent-and-mcp-integration.md      # LLM, Subagent, and MCP server guide
├── NetMechanics/                          # Formal Lean 4 specifications
│   ├── BUC.lean                           # BUC algebra and water dipping
│   ├── Inventory.lean                     # Acyclic container tree and encumbrance
│   ├── Energy.lean                        # Speed points and Progress theorem
│   ├── Grid.lean                          # Tile sum types and door state machine
│   ├── Combat.lean                        # Melee attack rolls, AC, and HP bounds
│   ├── AST.lean                           # Deep embedding, small-step semantics
│   ├── FOV.lean                           # Field of view & line of sight symmetry
│   ├── Raycast.lean                       # Wand beam reflection & loop termination
│   ├── Engraving.lean                     # Elbereth wards & smudge degradation
│   ├── Identification.lean                # Epistemic knowledge lattice & monotonicity
│   ├── Polymorph.lean                     # Shape-shifting HP buffers & revert on death
│   └── Pathfinding.lean                   # Dijkstra scent gradient & convergence
├── NetMechanics.lean                      # Lean 4 root module
├── Main.lean                              # Executable Lean 4 verification harness
├── lakefile.toml                          # Lake build definition
├── crates/
│   ├── netrust-types/                     # Foundational coordinates, tiles, and ontology enums
│   ├── netrust-arena/                     # Generational handles (SlotMap) for actors/items
│   ├── netrust-data/                      # Declarative bestiary, item catalog, roles & races
│   ├── netrust-core/                      # Core mechanics mirror of Lean 4 theorems + proptests
│   ├── netrust-dungeon/                   # Grid level, raycasting FOV, procedural generator, shops & temples
│   ├── netrust-sim/                       # Deterministic simulation world, Dijkstra AI, containers, altars
│   ├── netrust-agent/                     # MCP server, JSON streaming, and GraphQL HTTP server
│   ├── netrust-tui/                       # Human playable terminal UI with classic NetHack ASCII
│   └── netrust-wasm/                      # WebAssembly bindings and browser simulation runner
├── web/                                   # Retro CRT web terminal and WASM deployment
│   ├── index.html                         # In-browser playable client + agent telemetry inspector
│   └── pkg/                               # wasm-pack generated bundle (184 KB WASM)
├── examples/                              # Autonomous AI agent integration examples
│   └── mcp_agent_client.py                # Python stdio MCP client
├── Cargo.toml                             # Rust workspace manifest
└── NetHack-5.0.0/                         # Legacy C source distribution (reference)
```

---

## Verification & Execution

### 1. Build and Verify Lean 4 Proofs
```bash
lake build
./.lake/build/bin/netmechanics
```
* **Status**: 100% verified by Lean 4 kernel (**32 build jobs, 0 `sorry`s, 0 warnings**).

### 2. Run Rust Unit & Property-Based Tests
```bash
cargo test --workspace
```
* **Status**: 100% passing (**89 tests** covering unit suites, GraphQL integration, and thousands of randomized `proptest` executions, zero failures).

---

## Frontends & Interaction Options

### 1. Play in Web Browser (WebAssembly Terminal)
No backend server required. NetRust compiles to a standalone 184 KB WebAssembly module:
```bash
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
* **Endpoint**: `http://localhost:4000/graphql`
* **Interactive GraphiQL Explorer**: Open `http://localhost:4000/graphql` to execute GraphQL queries (`playerState`, `observationJson`, `asciiMap`, `inspectTile`, `bestiary`, `itemCatalog`, `roles`, `races`) and mutations (`stepAction`, `resetGame`, `resetWithCharacter`).