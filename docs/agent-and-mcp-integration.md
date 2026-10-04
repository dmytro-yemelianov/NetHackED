# Agent, LLM & Model Context Protocol (MCP) Integration Guide

NetRust provides first-class support for autonomous agents, Reinforcement Learning (RL) environments, and Large Language Models (LLMs) through multiple programmatic interfaces without requiring a graphical or terminal display.

---

## 1. Architectural Overview

NetRust decouples the game engine into pure state transitions and presentation adapters:

```
                          ┌───────────────────────────┐
                          │    SimulationWorld        │
                          │ (Deterministic, Seeded)   │
                          └─────────────┬─────────────┘
                                        │
                         ActionAst In   │   GameEvent Out
                                        │
           ┌────────────────────────────┼───────────────────────────┐
           │                            │                           │
           ▼                            ▼                           ▼
  [crates/netrust-agent]       [crates/netrust-agent]     [crates/netrust-tui]
     (MCP Server)                (JSON line stream)        (Terminal UI)
   stdio JSON-RPC 2.0             stdin/stdout lines        crossterm/curses
   LLMs, Subagents, IDEs        RL agents, bash pipes       Human players
```

---

## 2. Model Context Protocol (MCP) Server (`netrust-mcp`)

The `netrust-mcp` binary implements the open [Model Context Protocol (MCP)](https://modelcontextprotocol.io/) over standard input/output (stdio JSON-RPC 2.0).

### Launching the MCP Server
```bash
cargo run --bin netrust-mcp
```

### Supported MCP Tools

| Tool Name | Arguments | Description |
| :--- | :--- | :--- |
| `netrust_get_observation` | `{}` | Returns structured JSON observation: player stats, visible monsters, turn, and ASCII viewport with FOV fog-of-war. |
| `netrust_step` | `{"action": "move_east"}` | Steps the simulation with an action and returns the new observation. Actions: `move_<dir>` (8 directions: `north`, `northeast`, `east`, `southeast`, `south`, `southwest`, `west`, `northwest`), `wait`, `pickup`, `pay`, `pray`, `sacrifice`, `eat`, `cast`, `ascend`, `descend`, and `kick_<north\|east\|south\|west>`. Optional arguments: `index` (inventory/spell slot for `sacrifice`, `eat`, `cast`; default 0) and `direction` (for `cast`; default east). |
| `netrust_inspect_tile` | `{"x": 10, "y": 14}` | Inspects tile sum-type and any occupant actor at coordinate $(x, y)$. |
| `netrust_render_map` | `{}` | Returns the complete 80×21 ASCII dungeon level with FOV visibility shading. |
| `netrust_reset_game` | `{"seed": 42}` | Resets the dungeon level with a deterministic PRNG seed. |
| `netrust_get_roles` | `{}` | Returns available classic NetHack roles (Valkyrie, Wizard, Barbarian, etc.), races, starting stats, and inventories. |
| `netrust_reset_with_character` | `{"seed": 42, "role": "valkyrie", "race": "human"}` | Resets the game session with customizable role, race, gender, and alignment. |

### Protocol Behavior

* Requests are JSON-RPC 2.0. Errors use the standard codes: `-32700` (parse error), `-32600` (invalid request), `-32601` (method not found), `-32602` (invalid params).
* Notifications (requests without an `id`) get no reply.
* The server supports the MCP `ping` method (returns an empty result).
* Unknown actions or bad arguments to a tool are reported as tool errors rather than crashing the server.

### Configuration for Claude Desktop / Cursor / Antigravity

Add the server to your MCP configuration file:

```json
{
  "mcpServers": {
    "netrust": {
      "command": "cargo",
      "args": ["run", "--manifest-path", "/absolute/path/to/NetRust/Cargo.toml", "--bin", "netrust-mcp"]
    }
  }
}
```

---

## 3. Streaming JSON Line-Protocol (`netrust-jsonrpc`)

For high-throughput RL environments, automated benchmarks (e.g. NetHack Learning Environment / NLE), or lightweight agent subprocesses:

### Launching the Stream
```bash
cargo run --bin netrust-jsonrpc
```

### Protocol Format
This binary speaks a simple line-delimited JSON protocol (it is not JSON-RPC 2.0); invalid input yields a line of the form `{"error": "..."}`. Supported actions: `move_<dir>` (8 directions), `wait`, `pickup`, `pay`, `pray`, `sacrifice` (optional `index`), and `get_state`.

Upon launch, `netrust-jsonrpc` immediately writes the initial `GameObservation` JSON object to `stdout`. It then processes JSON lines from `stdin`:

#### Input Action Examples:
```json
{"action": "move_east"}
{"action": "move_north"}
{"action": "wait"}
{"action": "get_state"}
```

#### Output Structured Observation:
```json
{
  "turn": 2,
  "player_coord": {"x": 70, "y": 14},
  "player_hp": 20,
  "player_max_hp": 20,
  "player_ac": 8,
  "visible_actors": [
    {
      "name": "Hero",
      "coord": {"x": 70, "y": 14},
      "hp": 20,
      "max_hp": 20,
      "is_player": true
    }
  ],
  "ascii_map": "...",
  "last_events": [
    {"TurnAdvanced": {"turn": 2}}
  ],
  "is_game_over": false
}
```

---

## 4. Programmatic Rust API ([crates/netrust-agent](../crates/netrust-agent))

You can also embed the agent session directly in Rust binaries:

```rust
use netrust_agent::AgentSession;
use netrust_sim::ActionAst;

let mut session = AgentSession::new(12345);
let obs = session.get_observation();
println!("Turn: {}, Player HP: {}", obs.turn, obs.player_hp);

// Execute an action
let next_obs = session.step(ActionAst::Wait);
println!("New Turn: {}", next_obs.turn);
```

---

## 5. Networked GraphQL API & GraphiQL Explorer (`netrust-graphql`)

For web dashboards, remote agent swarms, and visual browser inspection, `netrust-graphql` provides an HTTP server with interactive GraphiQL playground on port `4000` (override with the `PORT` environment variable).

### Launching the Server
```bash
cargo run --bin netrust-graphql
```
* **Server URL**: `http://localhost:4000/graphql`
* **GraphiQL Explorer**: Open `http://localhost:4000/graphql` in any web browser.

### Network binding & authentication

By default the GraphQL server binds to `127.0.0.1` only. To listen elsewhere, pass `--bind <addr>` or set `NETRUST_BIND` (the command-line flag wins):
```bash
cargo run --bin netrust-graphql -- --bind 0.0.0.0:4000
NETRUST_BIND=0.0.0.0:4000 cargo run --bin netrust-graphql
```
If `NETRUST_TOKEN` is set (non-empty), every GraphQL **mutation** requires the header `Authorization: Bearer <token>`; queries remain open. Set a token before binding to a non-loopback address.
```bash
NETRUST_TOKEN=s3cret cargo run --bin netrust-graphql
curl -H 'Authorization: Bearer s3cret' -H 'Content-Type: application/json' \
  -d '{"query":"mutation { resetGame(seed: 1) }"}' http://127.0.0.1:4000/graphql
```

### Example GraphQL Queries

#### Query Player State & Bestiary
```graphql
query GetGameInfo {
  playerState {
    x
    y
    hp
    maxHp
    ac
  }
  bestiary {
    name
    glyph
    baseHp
    ac
    speed
    level
  }
}
```

#### Mutation: Step Player Action
```graphql
mutation TakeTurn {
  stepAction(action: "move", direction: "east") {
    success
    hp
    turn
    events
    asciiMap
  }
}
```

---

## 6. Networked Bones Server (`netrust-bones-server`)

A standalone HTTP daemon that stores and serves bones files and gravestone records (`/api/v1/bones`, `/api/v1/graves`, `/api/v1/stats`, `/api/v1/reset`).
```bash
cargo run --bin netrust-bones-server
```
It binds to `127.0.0.1:7777` by default (`NETRUST_BONES_HOST` / `NETRUST_BONES_PORT` adjust the default). Override the full address with `--bind <addr>` or `NETRUST_BIND`. If `NETRUST_TOKEN` is set, `POST /api/v1/bones` and `POST /api/v1/reset` require `Authorization: Bearer <token>`; read-only `GET` endpoints stay open.

---

## 7. Runnable Agent Examples

NetRust provides turnkey example clients demonstrating how AI agents connect and explore:

### Python MCP Client (`examples/mcp_agent_client.py`)
Spawns `netrust-mcp` as a subprocess, initializes the protocol, discovers tools, generates a custom Wizard character, and executes movement/wait actions:
```bash
python3 examples/mcp_agent_client.py
```

### Rust Autonomous Agent (`crates/netrust-agent/examples/autonomous_bot.rs`)
An autonomous bot loop inspecting tiles, detecting monsters in FOV, and executing actions:
```bash
cargo run --example autonomous_bot -p netrust-agent
```


