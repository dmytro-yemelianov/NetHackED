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
     (MCP Server)                (JSON-RPC Stream)        (Terminal UI)
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
| `netrust_step` | `{"action": "move_east"}` | Steps the simulation with an action (`move_north`, `move_east`, `move_south`, `move_west`, `move_northeast`, `move_northwest`, `move_southeast`, `move_southwest`, `wait`, `pickup`, `pay`, `pray`, `sacrifice`, `kick_east`, `kick_west`, `kick_north`, `kick_south`), returns new observation. |
| `netrust_inspect_tile` | `{"x": 10, "y": 14}` | Inspects tile sum-type and any occupant actor at coordinate $(x, y)$. |
| `netrust_render_map` | `{}` | Returns the complete 80×21 ASCII dungeon level with FOV visibility shading. |
| `netrust_reset_game` | `{"seed": 42}` | Resets the dungeon level with a deterministic PRNG seed. |
| `netrust_get_roles` | `{}` | Returns available classic NetHack roles (Valkyrie, Wizard, Barbarian, etc.), races, starting stats, and inventories. |
| `netrust_reset_with_character` | `{"seed": 42, "role": "valkyrie", "race": "human"}` | Resets the game session with customizable role, race, gender, and alignment. |

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

## 4. Programmatic Rust API ([crates/netrust-agent](crates/netrust-agent))

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

For web dashboards, remote agent swarms, and visual browser inspection, `netrust-graphql` provides an HTTP server with interactive GraphiQL playground on port `4000`.

### Launching the Server
```bash
cargo run --bin netrust-graphql
```
* **Server URL**: `http://localhost:4000/graphql`
* **GraphiQL Explorer**: Open `http://localhost:4000/graphql` in any web browser.

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

## 6. Runnable Agent Examples

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


