# Hardening, Sim Integrity & Level Reachability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove remotely triggerable crashes and server exposure, fix simulation-state corruption bugs, and make every special level's required locations reachable.

**Architecture:** Validation moves to the edges (validated `Coord` deserialization, bounds-checked session API, shared JSON-RPC/stdio helpers in `nethacked-agent`). Level persistence keeps entity IDs consistent by storing `(old_id, record)` pairs and remapping on restore. Dungeon generators gain a shared BFS reachability helper used both for check-and-revert hazard placement and for seed-sweep tests.

**Tech Stack:** Rust 2021 workspace; serde/serde_json; slotmap; rand 0.9 + rand_chacha 0.9 (`serde` feature); axum 0.7 + tokio; async-graphql 7.0.2.

**Spec:** `docs/superpowers/specs/2026-10-04-hardening-integrity-reachability-design.md`

## Global Constraints

- Branch: `fix/review-hardening-integrity` (never commit to `main`).
- TDD: every fix lands with a test that fails before the fix.
- Must pass: `cargo build --workspace --all-targets` and `cargo test --workspace --exclude nethacked-py`.
- No new crate dependencies except: `rand_chacha` feature `serde` (nethacked-sim), `serde_json` dev-dependency (nethacked-types).
- Match surrounding code style (no `cargo fmt` sweep — that is a later cycle).
- Commit messages end with:
  ```
  Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_019cXhwXQqEEChd362EQ6RKp
  ```
- Bones validation limits: `hero_name` 1..=32 chars, `killer` ≤ 64 chars, `items.len()` ≤ 64, `depth` 1..=60; ≤ 16 bones per depth (409 beyond); ≤ 1000 graves (oldest evicted).
- Bones client limits: response body ≤ 4 MiB, status/header line ≤ 8 KiB.
- GraphQL: `limit_depth(16)`, `limit_complexity(2000)`, body ≤ 64 KiB (413).
- Default bind `127.0.0.1`; override `--bind <addr>` then `NETHACKED_BIND`; token env `NETHACKED_TOKEN`.
- Initial wand charges: wishing 1; non-directional (secret door detection) 13; other wands 6.

## Review Focus

1. Multibyte names anywhere they are rendered/truncated (headstone, grave lookup) — must never panic. Test: 40-char Cyrillic name in Task 6.
2. A request with a JSON `id` of `0`, a string, or `null` — `id` must be echoed exactly and `"id": null` is still a request (not a notification) in JSON-RPC 2.0 only when the key is present. Test in Task 4.
3. Level round-trip when the hero carries a container with contents and the floor has a nested container (sack in sack) — no orphans either way. Test in Task 14.
4. Wishing with BUC/enchant prefixes and articles ("a blessed +2 long sword", "the Amulet of Yendor") — exact match after normalization. Test in Task 11.
5. Mysterious Force at depth 4 and 5 with every roll — never lands on the Sanctum without the invocation. Test in Task 16 loops rolls directly.

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/nethacked-types/src/lib.rs` | `Coord` validated deserialization |
| `crates/nethacked-dungeon/src/level.rs` | `coord_map` serde module for `HashMap<Coord, V>` fields |
| `crates/nethacked-dungeon/src/reach.rs` (new) | `reachable_from`, `reachable_from_with`, `find_free_floor` |
| `crates/nethacked-dungeon/src/{sokoban,mines,quest,gehennom,generator}.rs` | layout fixes |
| `crates/nethacked-dungeon/tests/reachability.rs` (new) | 200-seed layout sweeps |
| `crates/nethacked-agent/src/rpc.rs` (new) | JSON-RPC 2.0 request parsing/response builders, direction parsing |
| `crates/nethacked-agent/src/stdio.rs` (new) | UTF-8-tolerant line server loop |
| `crates/nethacked-agent/src/netconfig.rs` (new, non-wasm) | bind address / token / bearer check |
| `crates/nethacked-agent/src/{mcp,jsonrpc,session,graphql}.rs` | protocol hardening |
| `crates/nethacked-agent/src/bones/{server,client,headstone}.rs` | bones hardening |
| `crates/nethacked-agent/tests/{protocol_tests,bones_hardening_test,graphql_http_test}.rs` (new) | agent tests |
| `crates/nethacked-sim/src/world.rs` | rng serde, `damage_player`, `StoredLevel` shape |
| `crates/nethacked-sim/src/actions/stairs.rs` | pack/unpack remap, spawn placement, mysterious-force clamp |
| `crates/nethacked-sim/src/actions/{items,movement,mod}.rs`, `combat.rs` | genocide, wands/wishes, traps, melee, timers |
| `crates/nethacked-data/src/{items,monsters}.rs` | wand charges, `monster_class_of` |
| `crates/nethacked-sim/tests/{save_load_tests,integrity_tests,level_persistence_tests,reachability_tests}.rs` (new) | sim tests |

---

### Task 1: Validated `Coord` deserialization

**Files:**
- Modify: `crates/nethacked-types/src/lib.rs:46-51` (Coord struct)
- Modify: `crates/nethacked-types/Cargo.toml` (dev-dep `serde_json = "1.0"`)
- Test: `crates/nethacked-types/src/lib.rs` (`mod tests` at ~line 715)

**Interfaces:**
- Produces: `Coord` deserialization fails for `x >= COLNO || y >= ROWNO`; serialized shape unchanged `{"x":..,"y":..}`.

- [ ] **Step 1: Add dev-dependency**

In `crates/nethacked-types/Cargo.toml` under `[dev-dependencies]` add:
```toml
serde_json = "1.0"
```

- [ ] **Step 2: Write the failing test** (append inside existing `mod tests` in `lib.rs`)

```rust
    #[test]
    fn coord_deserialize_rejects_out_of_bounds() {
        let ok: Coord = serde_json::from_str(r#"{"x":79,"y":20}"#).unwrap();
        assert_eq!(ok, Coord::new(79, 20).unwrap());
        assert!(serde_json::from_str::<Coord>(r#"{"x":80,"y":0}"#).is_err());
        assert!(serde_json::from_str::<Coord>(r#"{"x":0,"y":21}"#).is_err());
        assert!(serde_json::from_str::<Coord>(r#"{"x":1000,"y":5}"#).is_err());
        let json = serde_json::to_string(&Coord::new(3, 4).unwrap()).unwrap();
        assert_eq!(json, r#"{"x":3,"y":4}"#);
    }
```

- [ ] **Step 3: Run to verify it fails**

Run: `cargo test -p nethacked-types coord_deserialize_rejects_out_of_bounds`
Expected: FAIL (out-of-bounds JSON deserializes successfully).

- [ ] **Step 4: Implement**

Replace the `Coord` definition with:
```rust
/// Bounded coordinate on the $80 \times 21$ dungeon grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "RawCoord")]
pub struct Coord {
    pub x: usize,
    pub y: usize,
}

/// Unvalidated wire form of `Coord`; deserialization goes through `Coord::new`.
#[derive(Deserialize)]
struct RawCoord {
    x: usize,
    y: usize,
}

impl TryFrom<RawCoord> for Coord {
    type Error = String;

    fn try_from(raw: RawCoord) -> Result<Self, Self::Error> {
        Coord::new(raw.x, raw.y).ok_or_else(|| {
            format!("coordinate ({}, {}) out of bounds {}x{}", raw.x, raw.y, COLNO, ROWNO)
        })
    }
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p nethacked-types && cargo test --workspace --exclude nethacked-py 2>&1 | grep -E "test result|FAILED|panicked"`
Expected: all PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/nethacked-types
git commit -m "fix(types): reject out-of-bounds Coord on deserialization"
```

---

### Task 2: Save/load fidelity (RNG state + coord-keyed maps)

**Files:**
- Modify: `crates/nethacked-dungeon/src/level.rs:11-22`
- Modify: `crates/nethacked-sim/Cargo.toml` (`rand_chacha = { version = "0.9", features = ["serde"] }`)
- Modify: `crates/nethacked-sim/src/world.rs:62-63`
- Test: `crates/nethacked-sim/tests/save_load_tests.rs` (new)

**Interfaces:**
- Produces: `SimulationWorld` JSON round-trip preserves RNG stream, traps, engravings. `DungeonLevel.engravings`/`traps` stay `HashMap<Coord, _>` in memory; on the wire they are sorted arrays of `[coord, value]`.

- [ ] **Step 1: Write the failing test**

Create `crates/nethacked-sim/tests/save_load_tests.rs`:
```rust
//! Save/load fidelity: full world JSON round-trip including RNG stream and coord-keyed maps.

use nethacked_core::engraving::{Engraving, EngravingMedium};
use nethacked_sim::{Coord, SimulationWorld};
use nethacked_types::{TrapRecord, TrapState, TrapType};
use rand::RngCore;

#[test]
fn world_roundtrip_preserves_rng_traps_and_engravings() {
    let mut sim = SimulationWorld::new_with_seed(4242);
    let c1 = Coord::new(10, 5).unwrap();
    let c2 = Coord::new(11, 5).unwrap();
    sim.level.set_engraving(c1, Engraving::new("Elbereth", EngravingMedium::Dust(5)));
    sim.level.traps.insert(c2, TrapRecord { id: 1, trap_type: TrapType::Arrow, state: TrapState::Hidden, coord: c2 });
    // Advance RNG away from its seed position.
    for _ in 0..7 {
        sim.rng.next_u32();
    }

    let json = serde_json::to_string(&sim).expect("serialize");
    let mut restored: SimulationWorld = serde_json::from_str(&json).expect("deserialize");

    // Compare as Value so HashMap key order in objects does not matter.
    let v1: serde_json::Value = serde_json::from_str(&json).unwrap();
    let v2: serde_json::Value = serde_json::to_value(&restored).unwrap();
    assert_eq!(v1, v2);

    assert_eq!(restored.level.get_engraving(c1).map(|e| e.text.clone()), Some("Elbereth".to_string()));
    assert_eq!(restored.level.traps.get(&c2).map(|t| t.trap_type), Some(TrapType::Arrow));
    assert_eq!(sim.rng.next_u64(), restored.rng.next_u64());
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p nethacked-sim --test save_load_tests`
Expected: FAIL — serialization error "key must be a string" (or RNG mismatch).

- [ ] **Step 3: Implement coord_map**

In `crates/nethacked-dungeon/src/level.rs`, change the two fields to:
```rust
    #[serde(with = "coord_map")]
    pub engravings: HashMap<Coord, Engraving>,
    #[serde(default, with = "coord_map")]
    pub traps: HashMap<Coord, nethacked_types::TrapRecord>,
```
and append to the file:
```rust
/// Serializes `HashMap<Coord, V>` as a coordinate-sorted sequence of `(Coord, V)` pairs:
/// valid JSON (no struct keys) and deterministic ordering.
mod coord_map {
    use std::collections::HashMap;
    use nethacked_types::Coord;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S, V>(map: &HashMap<Coord, V>, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        V: Serialize,
    {
        let mut entries: Vec<(&Coord, &V)> = map.iter().collect();
        entries.sort_by_key(|(c, _)| **c);
        s.collect_seq(entries)
    }

    pub fn deserialize<'de, D, V>(d: D) -> Result<HashMap<Coord, V>, D::Error>
    where
        D: Deserializer<'de>,
        V: Deserialize<'de>,
    {
        let entries: Vec<(Coord, V)> = Vec::deserialize(d)?;
        Ok(entries.into_iter().collect())
    }
}
```

- [ ] **Step 4: Implement RNG serde**

`crates/nethacked-sim/Cargo.toml`: `rand_chacha = { version = "0.9", features = ["serde"] }`.
`crates/nethacked-sim/src/world.rs`: replace `#[serde(skip, default = "default_rng")]` with `#[serde(default = "default_rng")]`.

- [ ] **Step 5: Run tests**

Run: `cargo test -p nethacked-sim --test save_load_tests && cargo test --workspace --exclude nethacked-py 2>&1 | grep -E "test result|FAILED|panicked"`
Expected: PASS. If `rand_core` serde feature is needed for the build, enable it the same way and note it.

- [ ] **Step 6: Commit**

```bash
git add crates/nethacked-dungeon/src/level.rs crates/nethacked-sim/Cargo.toml crates/nethacked-sim/src/world.rs crates/nethacked-sim/tests/save_load_tests.rs Cargo.lock
git commit -m "fix(sim): make world save/load round-trip RNG state, traps and engravings"
```

---

### Task 3: Agent protocol foundations (rpc helpers, stdio loop, bounds-checked inspect_tile)

**Files:**
- Create: `crates/nethacked-agent/src/rpc.rs`, `crates/nethacked-agent/src/stdio.rs`
- Modify: `crates/nethacked-agent/src/lib.rs` (add `pub mod rpc; pub mod stdio;`)
- Modify: `crates/nethacked-agent/src/session.rs:76-94` (`inspect_tile`)
- Modify callers: `crates/nethacked-agent/src/graphql.rs:121-131`, `crates/nethacked-agent/examples/autonomous_bot.rs:68-72` (mcp/jsonrpc callers are rewritten in Tasks 4–5; in this task change them to `session.inspect_tile(x, y)` and serialize `Ok`/`Err` text so the crate compiles)
- Test: unit tests inside `rpc.rs`, `stdio.rs`; `crates/nethacked-agent/tests/protocol_tests.rs` (new)

**Interfaces:**
- Produces:
  - `pub fn AgentSession::inspect_tile(&self, x: usize, y: usize) -> Result<TileInspection, String>`
  - `rpc::RpcRequest { pub id: Option<Value>, pub method: String, pub params: Value }`
  - `rpc::parse_request(line: Result<&str, ()>) -> Result<RpcRequest, Value>` — `Err` carries a ready error response (`-32700` / `-32600`, id echoed when available else `null`)
  - `rpc::error_response(id: Value, code: i64, message: impl Into<String>) -> Value`
  - `rpc::result_response(id: Value, result: Value) -> Value`
  - `rpc::parse_direction(s: &str) -> Option<Direction>` (north/south/east/west/northeast/northwest/southeast/southwest and vi-keys k/j/l/h/u/y/n/b)
  - constants `PARSE_ERROR=-32700, INVALID_REQUEST=-32600, METHOD_NOT_FOUND=-32601, INVALID_PARAMS=-32602`
  - `stdio::serve_lines<R: BufRead, W: Write>(reader: R, writer: W, handler: impl FnMut(Result<&str, ()>) -> Option<String>) -> io::Result<()>`

- [ ] **Step 1: Write failing tests**

Create `crates/nethacked-agent/tests/protocol_tests.rs` (all new helpers are tested through the public API here):
```rust
//! Protocol-level hardening tests for session, rpc helpers and stdio loop.

use nethacked_agent::rpc::{parse_direction, parse_request, INVALID_REQUEST, PARSE_ERROR};
use nethacked_agent::stdio::serve_lines;
use nethacked_agent::AgentSession;
use nethacked_sim::Direction;
use std::io::Cursor;

#[test]
fn inspect_tile_out_of_bounds_is_error_not_panic() {
    let session = AgentSession::new(42);
    assert!(session.inspect_tile(1000, 0).is_err());
    assert!(session.inspect_tile(0, 21).is_err());
    assert!(session.inspect_tile(5, 5).is_ok());
}

#[test]
fn parse_request_classifies_errors() {
    let e = parse_request(Ok("{not json")).unwrap_err();
    assert_eq!(e["error"]["code"], PARSE_ERROR);
    assert!(e["id"].is_null());

    let e = parse_request(Err(())).unwrap_err();
    assert_eq!(e["error"]["code"], PARSE_ERROR);

    let e = parse_request(Ok(r#"{"jsonrpc":"2.0","id":7}"#)).unwrap_err();
    assert_eq!(e["error"]["code"], INVALID_REQUEST);
    assert_eq!(e["id"], 7);

    let e = parse_request(Ok(r#"[1,2]"#)).unwrap_err();
    assert_eq!(e["error"]["code"], INVALID_REQUEST);

    let r = parse_request(Ok(r#"{"jsonrpc":"2.0","id":0,"method":"ping"}"#)).unwrap();
    assert_eq!(r.id, Some(serde_json::json!(0)));
    assert_eq!(r.method, "ping");

    let r = parse_request(Ok(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)).unwrap();
    assert!(r.id.is_none());
}

#[test]
fn parse_direction_covers_eight_ways() {
    assert_eq!(parse_direction("northeast"), Some(Direction::NorthEast));
    assert_eq!(parse_direction("b"), Some(Direction::SouthWest));
    assert_eq!(parse_direction("up"), None);
}

#[test]
fn serve_lines_survives_invalid_utf8() {
    let mut input: Vec<u8> = vec![0xff, 0xfe, b'\n'];
    input.extend_from_slice(b"hello\n\n");
    let mut out = Vec::new();
    serve_lines(Cursor::new(input), &mut out, |line| match line {
        Ok(s) => Some(format!("ok:{}", s)),
        Err(()) => Some("bad".to_string()),
    })
    .unwrap();
    assert_eq!(String::from_utf8(out).unwrap(), "bad\nok:hello\n");
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p nethacked-agent --test protocol_tests`
Expected: FAIL to compile (modules/functions missing).

- [ ] **Step 3: Implement `rpc.rs`**

```rust
//! Shared JSON-RPC 2.0 request parsing and response builders for stdio servers.

use nethacked_sim::Direction;
use serde_json::{json, Value};

pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;

/// A structurally valid JSON-RPC request. `id == None` means notification.
#[derive(Debug, Clone)]
pub struct RpcRequest {
    pub id: Option<Value>,
    pub method: String,
    pub params: Value,
}

pub fn error_response(id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message.into() } })
}

pub fn result_response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

/// Parse one line. `Err(())` input means the line was not valid UTF-8.
/// On failure returns a ready-to-send error response.
pub fn parse_request(line: Result<&str, ()>) -> Result<RpcRequest, Value> {
    let text = line.map_err(|_| error_response(Value::Null, PARSE_ERROR, "Parse error: invalid UTF-8"))?;
    let req: Value = serde_json::from_str(text)
        .map_err(|e| error_response(Value::Null, PARSE_ERROR, format!("Parse error: {}", e)))?;
    let Some(obj) = req.as_object() else {
        return Err(error_response(Value::Null, INVALID_REQUEST, "Invalid Request: expected object"));
    };
    let id = obj.get("id").cloned();
    let Some(method) = obj.get("method").and_then(|m| m.as_str()) else {
        return Err(error_response(id.unwrap_or(Value::Null), INVALID_REQUEST, "Invalid Request: missing method"));
    };
    Ok(RpcRequest {
        id,
        method: method.to_string(),
        params: obj.get("params").cloned().unwrap_or_else(|| json!({})),
    })
}

/// Parse a compass direction name or vi-key.
pub fn parse_direction(s: &str) -> Option<Direction> {
    match s.to_lowercase().as_str() {
        "north" | "k" => Some(Direction::North),
        "south" | "j" => Some(Direction::South),
        "east" | "l" => Some(Direction::East),
        "west" | "h" => Some(Direction::West),
        "northeast" | "u" => Some(Direction::NorthEast),
        "northwest" | "y" => Some(Direction::NorthWest),
        "southeast" | "n" => Some(Direction::SouthEast),
        "southwest" | "b" => Some(Direction::SouthWest),
        _ => None,
    }
}
```

- [ ] **Step 4: Implement `stdio.rs`**

```rust
//! Line-oriented stdio server loop tolerant of invalid UTF-8.

use std::io::{self, BufRead, Write};

/// Reads `\n`-terminated lines, skips blank ones, and writes each handler reply on its own line.
/// Lines that are not valid UTF-8 are passed to the handler as `Err(())` instead of aborting.
pub fn serve_lines<R: BufRead, W: Write>(
    mut reader: R,
    mut writer: W,
    mut handler: impl FnMut(Result<&str, ()>) -> Option<String>,
) -> io::Result<()> {
    let mut buf = Vec::new();
    loop {
        buf.clear();
        if reader.read_until(b'\n', &mut buf)? == 0 {
            return Ok(());
        }
        let line = std::str::from_utf8(&buf).map(|s| s.trim()).map_err(|_| ());
        if matches!(line, Ok("")) {
            continue;
        }
        if let Some(out) = handler(line) {
            writeln!(writer, "{}", out)?;
            writer.flush()?;
        }
    }
}
```

Add to `lib.rs`: `pub mod rpc;` and `pub mod stdio;`.

- [ ] **Step 5: Implement bounds-checked `inspect_tile`**

In `session.rs` replace `inspect_tile` with:
```rust
    pub fn inspect_tile(&self, x: usize, y: usize) -> Result<TileInspection, String> {
        let coord = Coord::new(x, y).ok_or_else(|| format!("Coordinate ({}, {}) is outside the dungeon map", x, y))?;
        let tile = self.world.level.get_tile(coord).clone();
        let occupant = self.world.actor_at(coord).and_then(|id| {
            self.world.arena.actors.get(id).map(|a| ActorObservation {
                name: a.name.clone(),
                coord: a.coord,
                hp: a.hp,
                max_hp: a.max_hp,
                is_player: id == self.world.player_id,
            })
        });

        Ok(TileInspection {
            coord,
            is_passable: tile.is_passable(),
            is_transparent: tile.is_transparent(),
            tile,
            occupant,
        })
    }
```
Update callers:
- `graphql.rs` `inspect_tile` resolver body:
  ```rust
        match session.inspect_tile(x, y) {
            Ok(info) => serde_json::to_string_pretty(&info).unwrap_or_default(),
            Err(e) => e,
        }
  ```
- `examples/autonomous_bot.rs`: `if let Ok(inspection) = session.inspect_tile(target_x, target_y) { println!(...) }` (drop the `Coord::new` wrapper).
- `mcp.rs` line ~170 and `jsonrpc.rs` line ~42 (temporary until Tasks 4–5): `let insp = session.inspect_tile(x, y);` then `serde_json::to_string_pretty(&insp.map_err(|e| e))`… simplest compile-only form: `match session.inspect_tile(x, y) { Ok(i) => serde_json::to_string_pretty(&i).unwrap_or_default(), Err(e) => e }` (mcp) and `match session.inspect_tile(x, y) { Ok(i) => json!(i), Err(e) => json!({ "error": e }) }` (jsonrpc).

- [ ] **Step 6: Run tests**

Run: `cargo test -p nethacked-agent && cargo build --workspace --all-targets`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/nethacked-agent
git commit -m "feat(agent): shared JSON-RPC helpers, UTF-8 tolerant stdio loop, bounds-checked inspect_tile"
```

---

### Task 4: MCP server compliance and robustness

**Files:**
- Modify: `crates/nethacked-agent/src/mcp.rs` (whole file restructure)
- Modify: `crates/nethacked-agent/src/bin/mcp.rs`
- Test: `crates/nethacked-agent/tests/protocol_tests.rs` (append)

**Interfaces:**
- Consumes: `rpc::*`, `stdio::serve_lines`, `AgentSession::inspect_tile(x, y)` from Task 3.
- Produces: `pub fn handle_mcp_request(session: &mut AgentSession, line: &str) -> Option<Value>` (signature unchanged), `pub fn handle_mcp_line(session: &mut AgentSession, line: Result<&str, ()>) -> Option<Value>`, `pub fn run_mcp_server(seed: u64) -> io::Result<()>`.

Behavior table (all responses include `"jsonrpc":"2.0"`):

| Input | Response |
|---|---|
| invalid JSON / invalid UTF-8 | `-32700`, id null |
| non-object, missing/non-string method | `-32600` |
| no `id` key (notification), any method | none |
| `ping` | `result: {}` |
| unknown method | `-32601` |
| `tools/call` without `params.name` string | `-32602` |
| unknown tool | `-32602` |
| `nethacked_step` missing/unknown `action` | `-32602` |
| `nethacked_step` kick off-map | `-32602` |
| `nethacked_inspect_tile` missing x/y or out of bounds | `-32602` |

- [ ] **Step 1: Write failing tests** (append to `tests/protocol_tests.rs`)

```rust
use nethacked_agent::mcp::{handle_mcp_line, handle_mcp_request};
use nethacked_agent::rpc::{INVALID_PARAMS, METHOD_NOT_FOUND};
use serde_json::json;

fn mcp(session: &mut AgentSession, v: serde_json::Value) -> Option<serde_json::Value> {
    handle_mcp_request(session, &v.to_string())
}

#[test]
fn mcp_malformed_input_gets_errors() {
    let mut s = AgentSession::new(42);
    let r = handle_mcp_request(&mut s, "{oops").unwrap();
    assert_eq!(r["error"]["code"], PARSE_ERROR);
    let r = handle_mcp_line(&mut s, Err(())).unwrap();
    assert_eq!(r["error"]["code"], PARSE_ERROR);
    let r = mcp(&mut s, json!({"jsonrpc":"2.0","id":"a"})).unwrap();
    assert_eq!(r["error"]["code"], INVALID_REQUEST);
    assert_eq!(r["id"], "a");
}

#[test]
fn mcp_notifications_never_answered() {
    let mut s = AgentSession::new(42);
    assert!(mcp(&mut s, json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{}})).is_none());
    assert!(mcp(&mut s, json!({"jsonrpc":"2.0","method":"no/such"})).is_none());
}

#[test]
fn mcp_ping_and_unknown_method() {
    let mut s = AgentSession::new(42);
    let r = mcp(&mut s, json!({"jsonrpc":"2.0","id":0,"method":"ping"})).unwrap();
    assert_eq!(r["id"], 0);
    assert_eq!(r["result"], json!({}));
    let r = mcp(&mut s, json!({"jsonrpc":"2.0","id":1,"method":"no/such"})).unwrap();
    assert_eq!(r["error"]["code"], METHOD_NOT_FOUND);
}

#[test]
fn mcp_invalid_params_paths() {
    let mut s = AgentSession::new(42);
    let call = |name: &str, args: serde_json::Value| json!({"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":name,"arguments":args}});
    let r = mcp(&mut s, json!({"jsonrpc":"2.0","id":9,"method":"tools/call"})).unwrap();
    assert_eq!(r["error"]["code"], INVALID_PARAMS);
    let r = mcp(&mut s, call("nope", json!({}))).unwrap();
    assert_eq!(r["error"]["code"], INVALID_PARAMS);
    let r = mcp(&mut s, call("nethacked_step", json!({"action":"dance"}))).unwrap();
    assert_eq!(r["error"]["code"], INVALID_PARAMS);
    let r = mcp(&mut s, call("nethacked_step", json!({}))).unwrap();
    assert_eq!(r["error"]["code"], INVALID_PARAMS);
    let r = mcp(&mut s, call("nethacked_inspect_tile", json!({"x":1000,"y":0}))).unwrap();
    assert_eq!(r["error"]["code"], INVALID_PARAMS);
    let r = mcp(&mut s, call("nethacked_inspect_tile", json!({"x":5}))).unwrap();
    assert_eq!(r["error"]["code"], INVALID_PARAMS);
}

#[test]
fn mcp_kick_off_map_is_invalid_params() {
    let mut s = AgentSession::new(42);
    let pid = s.world.player_id;
    s.world.arena.actors.get_mut(pid).unwrap().coord = nethacked_sim::Coord::new(79, 20).unwrap();
    let r = mcp(&mut s, json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"nethacked_step","arguments":{"action":"kick_east"}}})).unwrap();
    assert_eq!(r["error"]["code"], INVALID_PARAMS);
}

#[test]
fn mcp_step_enum_matches_accepted_actions() {
    let mut s = AgentSession::new(42);
    let r = mcp(&mut s, json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})).unwrap();
    let tools = r["result"]["tools"].as_array().unwrap();
    let step = tools.iter().find(|t| t["name"] == "nethacked_step").unwrap();
    let actions: Vec<String> = step["inputSchema"]["properties"]["action"]["enum"]
        .as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    for a in ["descend", "ascend", "eat", "cast", "kick_east"] {
        assert!(actions.contains(&a.to_string()), "enum missing {}", a);
    }
    for a in &actions {
        let mut fresh = AgentSession::new(42);
        let r = mcp(&mut fresh, json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"nethacked_step","arguments":{"action":a}}})).unwrap();
        assert!(r.get("result").is_some(), "enum action {} rejected: {}", a, r);
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p nethacked-agent --test protocol_tests mcp_`
Expected: FAIL (compile error for `handle_mcp_line`, then assertion failures).

- [ ] **Step 3: Implement**

Restructure `mcp.rs`:
```rust
use crate::rpc::{self, error_response, parse_direction, result_response, RpcRequest, INVALID_PARAMS, METHOD_NOT_FOUND};

/// Every action string `nethacked_step` accepts; also used as the schema enum.
pub const STEP_ACTIONS: &[&str] = &[
    "move_north", "move_east", "move_south", "move_west",
    "move_northeast", "move_northwest", "move_southeast", "move_southwest",
    "wait", "pickup", "pay", "pray", "sacrifice", "eat", "cast", "ascend", "descend",
    "kick_north", "kick_east", "kick_south", "kick_west",
];

pub fn handle_mcp_request(session: &mut AgentSession, line: &str) -> Option<Value> {
    handle_mcp_line(session, Ok(line))
}

pub fn handle_mcp_line(session: &mut AgentSession, line: Result<&str, ()>) -> Option<Value> {
    let req = match rpc::parse_request(line) {
        Ok(r) => r,
        Err(err) => return Some(err),
    };
    let RpcRequest { id, method, params } = req;
    let reply = dispatch(session, &method, &params);
    // Notifications (no id) never get a response.
    let id = id?;
    Some(match reply {
        Ok(result) => result_response(id, result),
        Err((code, msg)) => error_response(id, code, msg),
    })
}

fn dispatch(session: &mut AgentSession, method: &str, params: &Value) -> Result<Value, (i64, String)> {
    match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "nethacked-mcp", "version": "0.1.0" }
        })),
        "notifications/initialized" => Ok(Value::Null),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(tools_list()),
        "tools/call" => {
            let tool_name = params.get("name").and_then(|n| n.as_str())
                .ok_or((INVALID_PARAMS, "tools/call requires params.name".to_string()))?;
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            let text = call_tool(session, tool_name, &args).map_err(|m| (INVALID_PARAMS, m))?;
            Ok(json!({ "content": [ { "type": "text", "text": text } ] }))
        }
        _ => Err((METHOD_NOT_FOUND, format!("Method '{}' not found", method))),
    }
}
```
- `tools_list()` returns the existing `{"tools":[...]}` value, with the `nethacked_step` description changed to `"Execute a game action and return the next observation. 'sacrifice'/'eat'/'cast' take an optional 'index'; 'cast' takes an optional 'direction' (8 compass names)."`, its `action` enum built from `STEP_ACTIONS` (`json!(STEP_ACTIONS)`), plus `"index": {"type":"integer"}` and `"direction": {"type":"string"}` properties.
- `call_tool(session, name, args) -> Result<String, String>` holds the existing per-tool bodies with these changes:
  - unknown tool → `Err(format!("Tool '{}' not found", name))`
  - `nethacked_step`: `let act_str = args.get("action").and_then(|a| a.as_str()).ok_or("missing 'action'")?;` then `let action = parse_step_action(act_str, args, p_coord)?;`
  - `nethacked_inspect_tile`: `let x = args.get("x").and_then(|v| v.as_u64()).ok_or("missing integer 'x'")? as usize;` (same for y) then `let insp = session.inspect_tile(x, y)?;`
- `fn parse_step_action(act: &str, args: &Value, p: Coord) -> Result<ActionAst, String>`:
  - `move_<dir>` → `parse_direction(dir)` → `ActionAst::Move`
  - `cast`: direction via `args["direction"]` with `parse_direction`, default `Direction::East` when absent; an unparseable present direction → `Err`
  - `kick_<dir>`: `let d = parse_direction(dir).ok_or(...)?; let target = p.step(d).ok_or("kick target is off the map")?; ActionAst::Kick(target)`
  - others as before; anything not in `STEP_ACTIONS` → `Err(format!("Unknown action '{}'", act))`
- `run_mcp_server`:
  ```rust
  pub fn run_mcp_server(seed: u64) -> std::io::Result<()> {
      let mut session = AgentSession::new(seed);
      let stdin = std::io::stdin();
      crate::stdio::serve_lines(stdin.lock(), std::io::stdout(), |line| {
          handle_mcp_line(&mut session, line).map(|v| v.to_string())
      })
  }
  ```
- `bin/mcp.rs` body becomes:
  ```rust
  fn main() -> std::io::Result<()> {
      nethacked_agent::run_mcp_server(42)
  }
  ```
  (keep the module doc comment; drop now-unused imports).

- [ ] **Step 4: Run tests**

Run: `cargo test -p nethacked-agent`
Expected: PASS, including existing `test_mcp_initialize_and_tools_list` and `test_mcp_tool_call_observation`.

- [ ] **Step 5: Commit**

```bash
git add crates/nethacked-agent
git commit -m "fix(agent): JSON-RPC 2.0/MCP compliant error handling, ping, notifications and action enum"
```

---

### Task 5: JSON-RPC server robustness

**Files:**
- Modify: `crates/nethacked-agent/src/jsonrpc.rs`
- Modify: `crates/nethacked-agent/src/bin/jsonrpc.rs` (use `serve_lines`; protocol unchanged)
- Test: `crates/nethacked-agent/tests/protocol_tests.rs` (append)

**Interfaces:**
- Consumes: `rpc::*`, `stdio::serve_lines`.
- Produces: `handle_jsonrpc_request(&mut AgentSession, &str) -> Option<Value>` (unchanged signature), `handle_jsonrpc_line(&mut AgentSession, Result<&str, ()>) -> Option<Value>`.

- [ ] **Step 1: Write failing tests**

```rust
use nethacked_agent::jsonrpc::{handle_jsonrpc_line, handle_jsonrpc_request};

#[test]
fn jsonrpc_robustness() {
    let mut s = AgentSession::new(42);
    let r = handle_jsonrpc_request(&mut s, "nope").unwrap();
    assert_eq!(r["error"]["code"], PARSE_ERROR);
    let r = handle_jsonrpc_line(&mut s, Err(())).unwrap();
    assert_eq!(r["error"]["code"], PARSE_ERROR);
    assert!(handle_jsonrpc_request(&mut s, r#"{"jsonrpc":"2.0","method":"nethacked.step","params":{"action":"wait"}}"#).is_none());
    let r = handle_jsonrpc_request(&mut s, r#"{"jsonrpc":"2.0","id":1,"method":"nethacked.inspectTile","params":{"x":1000,"y":0}}"#).unwrap();
    assert_eq!(r["error"]["code"], INVALID_PARAMS);
    let r = handle_jsonrpc_request(&mut s, r#"{"jsonrpc":"2.0","id":2,"method":"nethacked.step","params":{"action":"dance"}}"#).unwrap();
    assert_eq!(r["error"]["code"], INVALID_PARAMS);
    let r = handle_jsonrpc_request(&mut s, r#"{"jsonrpc":"2.0","id":3,"method":"nethacked.step","params":{"action":"northeast"}}"#).unwrap();
    assert!(r.get("result").is_some());
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p nethacked-agent --test protocol_tests jsonrpc_robustness`
Expected: FAIL.

- [ ] **Step 3: Implement**

Rewrite `jsonrpc.rs` following the Task 4 shape: `handle_jsonrpc_request` → `handle_jsonrpc_line(session, Ok(line))`; `handle_jsonrpc_line` parses via `rpc::parse_request`, calls `fn dispatch(session, method, params) -> Result<Value, (i64, String)>`, returns `None` when `id` is absent. Dispatch:
- `nethacked.getObservation` → `json!(session.get_observation())`
- `nethacked.renderAscii` → `json!({ "ascii": render_ascii_map(&session.world) })`
- `nethacked.step` → action = `params.action` (default `"wait"` when absent, as before); `"pickup"|"pay"|"pray"|"descend"|"ascend"|"wait"` map as before; otherwise `parse_direction(action)` → `Move`; else `Err((INVALID_PARAMS, "Unknown action ..."))`
- `nethacked.inspectTile` → require integer x,y (`INVALID_PARAMS` if missing), `session.inspect_tile(x, y).map(|i| json!(i)).map_err(|e| (INVALID_PARAMS, e))`
- else `METHOD_NOT_FOUND`.

`run_jsonrpc_server` uses `serve_lines` exactly like `run_mcp_server`.

`bin/jsonrpc.rs`: keep its line protocol; replace the `for line_res in stdin.lock().lines()` loop with `serve_lines(stdin.lock(), stdout, |line| { ... })` where `Err(())` yields `Some(json!({"error":"Invalid UTF-8"}).to_string())` and the existing per-command logic returns `Some(string)`. Print the initial observation before entering the loop as today.

- [ ] **Step 4: Run tests**

Run: `cargo test -p nethacked-agent && cargo build -p nethacked-agent --bins`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/nethacked-agent
git commit -m "fix(agent): JSON-RPC server error codes, notifications, UTF-8 tolerance and bounds checks"
```

---

### Task 6: Bones server hardening + shared netconfig

**Files:**
- Create: `crates/nethacked-agent/src/netconfig.rs` (non-wasm)
- Modify: `crates/nethacked-agent/src/lib.rs` (`#[cfg(not(target_arch = "wasm32"))] pub mod netconfig;`)
- Modify: `crates/nethacked-agent/src/bones/headstone.rs:12-23`
- Modify: `crates/nethacked-agent/src/bones/server.rs`
- Modify: `crates/nethacked-agent/src/bones/mod.rs` (export `create_bones_router_with_token`)
- Modify: `crates/nethacked-agent/src/bin/bones_server.rs`
- Test: `crates/nethacked-agent/tests/bones_hardening_test.rs` (new), headstone unit test

**Interfaces:**
- Produces:
  - `netconfig::resolve_bind_addr(cli_args: &[String], env_bind: Option<String>, default_addr: &str) -> String` — `--bind <addr>` wins, then `env_bind` (non-empty), else `default_addr`
  - `netconfig::token_from_env() -> Option<String>` — `NETHACKED_TOKEN` if non-empty
  - `netconfig::bearer_ok(headers: &axum::http::HeaderMap, token: Option<&str>) -> bool` — `true` when `token` is `None`
  - `bones::create_bones_router_with_token(state: SharedGraveyard, token: Option<String>) -> Router`; `create_bones_router(state)` = `create_bones_router_with_token(state, netconfig::token_from_env())`
  - constants in `server.rs`: `MAX_NAME_CHARS=32, MAX_KILLER_CHARS=64, MAX_ITEMS=64, MAX_DEPTH=60, MAX_BONES_PER_DEPTH=16, MAX_GRAVES=1000`

- [ ] **Step 1: Write failing tests**

Headstone unit test (append to `headstone.rs` tests):
```rust
    #[test]
    fn test_headstone_multibyte_name_does_not_panic() {
        let name = "Святослав Хоробрий Великий Київський";
        let stone = render_headstone(name, 1, 1, "гоблін", "2026-10-04", "");
        let line = stone.lines().find(|l| l.contains("Святослав")).unwrap();
        assert_eq!(line.chars().count(), 31);
    }
```

Create `crates/nethacked-agent/tests/bones_hardening_test.rs`:
```rust
//! Hardening tests for the bones HTTP server: validation, poisoning resistance, caps, auth.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use nethacked_agent::bones::{create_bones_router_with_token, GraveyardState};
use nethacked_agent::netconfig::resolve_bind_addr;
use serde_json::json;

async fn spawn(token: Option<&str>) -> (String, Arc<Mutex<GraveyardState>>) {
    let state = Arc::new(Mutex::new(GraveyardState::default()));
    let app = create_bones_router_with_token(state.clone(), token.map(|t| t.to_string()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (addr, state)
}

fn raw(addr: &str, method: &str, path: &str, extra_headers: &str, body: &str) -> (u16, String) {
    let mut s = TcpStream::connect(addr).unwrap();
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{extra_headers}\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    let code = out.split_whitespace().nth(1).unwrap().parse().unwrap();
    let body = out.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
    (code, body)
}

fn bones(name: &str, depth: u32) -> String {
    json!({"depth":depth,"hero_name":name,"hero_level":3,"max_hp":20,"ac":5,
           "death_coord":{"x":10,"y":5},"items":[],"killer":"jackal"}).to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn multibyte_name_does_not_brick_server() {
    let (addr, _) = spawn(None).await;
    let a = addr.clone();
    let (code, _) = tokio::task::spawn_blocking(move || raw(&a, "POST", "/api/v1/bones", "", &bones("Святослав Хоробрий", 3))).await.unwrap();
    assert_eq!(code, 201);
    let a = addr.clone();
    let (code, _) = tokio::task::spawn_blocking(move || raw(&a, "GET", "/api/v1/stats", "", "")).await.unwrap();
    assert_eq!(code, 200);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn validation_rejects_bad_payloads() {
    let (addr, _) = spawn(None).await;
    for body in [
        bones(&"x".repeat(33), 3),
        bones("", 3),
        bones("Ok", 0),
        bones("Ok", 61),
        json!({"depth":3,"hero_name":"Ok","hero_level":1,"max_hp":1,"ac":0,"death_coord":{"x":10,"y":5},"items":[],"killer":"k".repeat(65)}).to_string(),
    ] {
        let a = addr.clone();
        let (code, _) = tokio::task::spawn_blocking(move || raw(&a, "POST", "/api/v1/bones", "", &body)).await.unwrap();
        assert_eq!(code, 422);
    }
    // Out-of-range coordinate fails JSON extraction (4xx), never panics.
    let a = addr.clone();
    let bad = bones("Ok", 3).replace(r#""x":10"#, r#""x":1000"#);
    let (code, _) = tokio::task::spawn_blocking(move || raw(&a, "POST", "/api/v1/bones", "", &bad)).await.unwrap();
    assert!((400..500).contains(&code));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn per_depth_cap_and_poison_recovery() {
    let (addr, state) = spawn(None).await;
    for i in 0..16 {
        let a = addr.clone();
        let (code, _) = tokio::task::spawn_blocking(move || raw(&a, "POST", "/api/v1/bones", "", &bones(&format!("H{}", i), 4))).await.unwrap();
        assert_eq!(code, 201);
    }
    let a = addr.clone();
    let (code, _) = tokio::task::spawn_blocking(move || raw(&a, "POST", "/api/v1/bones", "", &bones("Overflow", 4))).await.unwrap();
    assert_eq!(code, 409);

    // Poison the mutex from another thread; server must keep serving.
    let st = state.clone();
    let _ = std::thread::spawn(move || {
        let _g = st.lock().unwrap();
        panic!("poison");
    })
    .join();
    let a = addr.clone();
    let (code, _) = tokio::task::spawn_blocking(move || raw(&a, "GET", "/api/v1/stats", "", "")).await.unwrap();
    assert_eq!(code, 200);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn token_protects_mutating_routes() {
    let (addr, _) = spawn(Some("s3cret")).await;
    let a = addr.clone();
    let (code, _) = tokio::task::spawn_blocking(move || raw(&a, "POST", "/api/v1/reset", "", "")).await.unwrap();
    assert_eq!(code, 401);
    let a = addr.clone();
    let (code, _) = tokio::task::spawn_blocking(move || raw(&a, "POST", "/api/v1/bones", "", &bones("Ok", 3))).await.unwrap();
    assert_eq!(code, 401);
    let a = addr.clone();
    let (code, _) = tokio::task::spawn_blocking(move || raw(&a, "POST", "/api/v1/bones", "Authorization: Bearer s3cret\r\n", &bones("Ok", 3))).await.unwrap();
    assert_eq!(code, 201);
    let a = addr.clone();
    let (code, _) = tokio::task::spawn_blocking(move || raw(&a, "GET", "/api/v1/stats", "", "")).await.unwrap();
    assert_eq!(code, 200);
}

#[test]
fn bind_addr_resolution() {
    let args = vec!["bin".to_string(), "--bind".to_string(), "0.0.0.0:9".to_string()];
    assert_eq!(resolve_bind_addr(&args, Some("1.2.3.4:5".into()), "127.0.0.1:7777"), "0.0.0.0:9");
    assert_eq!(resolve_bind_addr(&[], Some("1.2.3.4:5".into()), "127.0.0.1:7777"), "1.2.3.4:5");
    assert_eq!(resolve_bind_addr(&[], Some("".into()), "127.0.0.1:7777"), "127.0.0.1:7777");
    assert_eq!(resolve_bind_addr(&[], None, "127.0.0.1:7777"), "127.0.0.1:7777");
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p nethacked-agent --test bones_hardening_test; cargo test -p nethacked-agent headstone`
Expected: FAIL (missing items; headstone panics on byte slicing).

- [ ] **Step 3: Implement headstone fix**

```rust
    let width = 31;
    let center = |s: &str| -> String {
        let max_chars = width - 4;
        let trimmed: String = s.chars().take(max_chars).collect();
        let len = trimmed.chars().count();
        let pad_total = (width - 2).saturating_sub(len);
        let pad_left = pad_total / 2;
        let pad_right = pad_total - pad_left;
        format!("|{}{}{}|", " ".repeat(pad_left), trimmed, " ".repeat(pad_right))
    };
```

- [ ] **Step 4: Implement netconfig**

```rust
//! Shared network server configuration: bind address resolution and bearer-token auth.

use axum::http::{header::AUTHORIZATION, HeaderMap};

/// `--bind <addr>` on the command line wins, then a non-empty env value, else the default.
pub fn resolve_bind_addr(cli_args: &[String], env_bind: Option<String>, default_addr: &str) -> String {
    if let Some(pos) = cli_args.iter().position(|a| a == "--bind") {
        if let Some(addr) = cli_args.get(pos + 1) {
            return addr.clone();
        }
    }
    match env_bind {
        Some(addr) if !addr.trim().is_empty() => addr,
        _ => default_addr.to_string(),
    }
}

/// The shared secret from `NETHACKED_TOKEN`, if set and non-empty.
pub fn token_from_env() -> Option<String> {
    std::env::var("NETHACKED_TOKEN").ok().filter(|t| !t.is_empty())
}

/// True when no token is configured, or the request carries `Authorization: Bearer <token>`.
pub fn bearer_ok(headers: &HeaderMap, token: Option<&str>) -> bool {
    let Some(expected) = token else { return true };
    headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|got| got == expected)
        .unwrap_or(false)
}
```

- [ ] **Step 5: Implement server hardening**

In `server.rs`:
- Add router state struct and lock helper:
  ```rust
  #[derive(Clone)]
  struct BonesApp {
      graveyard: SharedGraveyard,
      token: Option<Arc<str>>,
  }

  fn lock_graveyard(state: &SharedGraveyard) -> std::sync::MutexGuard<'_, GraveyardState> {
      state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
  }
  ```
- `create_bones_router_with_token(state, token)` builds the same routes with `.with_state(BonesApp { graveyard: state, token: token.map(Arc::from) })`; `create_bones_router(state)` calls it with `crate::netconfig::token_from_env()`.
- Every handler takes `State(app): State<BonesApp>` and uses `lock_graveyard(&app.graveyard)` instead of `state.lock().unwrap()`.
- `store_bones(State(app), headers: HeaderMap, Json(bones))`:
  1. `if !bearer_ok(&headers, app.token.as_deref()) { return (StatusCode::UNAUTHORIZED, Json(json!({"error":"missing or invalid bearer token"}))).into_response(); }`
  2. `if let Err(msg) = validate_bones(&bones) { return (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({"error": msg}))).into_response(); }`
  3. Render headstone and build `GraveRecord` (unchanged content) **before** locking.
  4. Lock; if `pool.len() >= MAX_BONES_PER_DEPTH` → `409` with `{"error":"bones pool for this depth is full"}`; else push bones, push grave, `if graves.len() > MAX_GRAVES { graves.remove(0); }`, `total_deaths += 1`, return `(StatusCode::CREATED, Json(grave)).into_response()`.
- `fn validate_bones(b: &BonesData) -> Result<(), String>` enforcing the Global Constraints limits (`chars().count()` for strings; name must be non-empty).
- `reset_graveyard(State(app), headers)` → `401` unless `bearer_ok`.
- Handlers return `axum::response::Response` (`.into_response()`) where branches differ in type.
- `run_bones_server` unchanged except using `create_bones_router`.

`bin/bones_server.rs`:
```rust
    let host = env::var("NETHACKED_BONES_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = env::var("NETHACKED_BONES_PORT").unwrap_or_else(|_| "7777".to_string());
    let args: Vec<String> = env::args().collect();
    let addr = nethacked_agent::netconfig::resolve_bind_addr(&args, env::var("NETHACKED_BIND").ok(), &format!("{}:{}", host, port));
```

Export from `bones/mod.rs`: `pub use server::{create_bones_router, create_bones_router_with_token, run_bones_server, GraveyardState, SharedGraveyard};`

- [ ] **Step 6: Run tests**

Run: `cargo test -p nethacked-agent`
Expected: PASS including the existing `bones_server_test.rs` (no token set in env).

- [ ] **Step 7: Commit**

```bash
git add crates/nethacked-agent
git commit -m "fix(bones): char-safe headstones, validation, caps, poison-tolerant locking, localhost bind and token auth"
```

---

### Task 7: Bones client hardening

**Files:**
- Modify: `crates/nethacked-agent/src/bones/client.rs`
- Test: unit tests in `client.rs`

**Interfaces:**
- Produces: `BonesClient` gains `tls_requested: bool` (pub field); `pub fn encode_path_segment(s: &str) -> String`; constants `MAX_BODY_BYTES = 4 * 1024 * 1024`, `MAX_LINE_BYTES = 8 * 1024`; `fn read_limited_line<R: BufRead>(r: &mut R, max: usize) -> Result<String, String>`.

- [ ] **Step 1: Write failing tests** (new `#[cfg(test)] mod tests` in `client.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn encodes_path_segments() {
        assert_eq!(encode_path_segment("Sir Lancelot"), "Sir%20Lancelot");
        assert_eq!(encode_path_segment("a/b\r\n"), "a%2Fb%0D%0A");
        assert_eq!(encode_path_segment("Тарас"), "%D0%A2%D0%B0%D1%80%D0%B0%D1%81");
        assert_eq!(encode_path_segment("ok-_.~9"), "ok-_.~9");
    }

    #[test]
    fn https_is_refused_not_downgraded() {
        let c = BonesClient::new("https://example.com");
        assert!(c.tls_requested);
        let err = c.fetch_stats().unwrap_err();
        assert!(err.contains("https"));
    }

    #[test]
    fn limited_line_rejects_overlong() {
        let mut r = Cursor::new(vec![b'a'; MAX_LINE_BYTES + 10]);
        assert!(read_limited_line(&mut r, MAX_LINE_BYTES).is_err());
        let mut r = Cursor::new(b"HTTP/1.1 200 OK\r\n".to_vec());
        assert_eq!(read_limited_line(&mut r, MAX_LINE_BYTES).unwrap(), "HTTP/1.1 200 OK\r\n");
    }

    #[test]
    fn oversized_content_length_rejected() {
        let resp = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", MAX_BODY_BYTES + 1);
        let err = parse_response(&mut Cursor::new(resp.into_bytes())).unwrap_err();
        assert!(err.contains("too large"));
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p nethacked-agent --lib bones::client`
Expected: FAIL to compile.

- [ ] **Step 3: Implement**

- `new`: `let tls_requested = base_url.starts_with("https://");` stored in the struct.
- `send_request` begins with `if self.tls_requested { return Err("https not supported by BonesClient; use http://".into()); }` and, after writing the request, calls `parse_response(&mut BufReader::new(stream))`.
- `fetch_grave` uses `format!("/api/v1/graves/{}", encode_path_segment(hero_name))`.
- New functions:
  ```rust
  pub const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;
  pub const MAX_LINE_BYTES: usize = 8 * 1024;

  /// Percent-encodes everything except RFC 3986 unreserved characters.
  pub fn encode_path_segment(s: &str) -> String {
      let mut out = String::with_capacity(s.len());
      for b in s.bytes() {
          if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
              out.push(b as char);
          } else {
              out.push_str(&format!("%{:02X}", b));
          }
      }
      out
  }

  fn read_limited_line<R: BufRead>(r: &mut R, max: usize) -> Result<String, String> {
      let mut buf = Vec::new();
      r.by_ref().take(max as u64 + 1).read_until(b'\n', &mut buf).map_err(|e| e.to_string())?;
      if buf.len() > max {
          return Err("response line too long".into());
      }
      Ok(String::from_utf8_lossy(&buf).into_owned())
  }

  fn parse_response<R: BufRead>(reader: &mut R) -> Result<(u16, String), String> {
      let status_line = read_limited_line(reader, MAX_LINE_BYTES)?;
      let status_code: u16 = status_line.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(500);
      let mut content_len: Option<usize> = None;
      loop {
          let header_line = read_limited_line(reader, MAX_LINE_BYTES)?;
          if header_line.trim().is_empty() {
              break;
          }
          if header_line.to_ascii_lowercase().starts_with("content-length:") {
              content_len = header_line.split(':').nth(1).and_then(|v| v.trim().parse().ok());
          }
      }
      let mut body_buf = Vec::new();
      match content_len {
          Some(len) if len > MAX_BODY_BYTES => return Err(format!("response body too large: {} bytes", len)),
          Some(len) => {
              body_buf.resize(len, 0);
              reader.read_exact(&mut body_buf).map_err(|e| e.to_string())?;
          }
          None => {
              reader.by_ref().take(MAX_BODY_BYTES as u64 + 1).read_to_end(&mut body_buf).map_err(|e| e.to_string())?;
              if body_buf.len() > MAX_BODY_BYTES {
                  return Err("response body too large".into());
              }
          }
      }
      Ok((status_code, String::from_utf8_lossy(&body_buf).into_owned()))
  }
  ```

- [ ] **Step 4: Run tests**

Run: `cargo test -p nethacked-agent`
Expected: PASS (including `bones_server_test.rs` network flow).

- [ ] **Step 5: Commit**

```bash
git add crates/nethacked-agent/src/bones/client.rs
git commit -m "fix(bones): client percent-encodes paths, refuses https downgrade, caps response size"
```

---

### Task 8: GraphQL limits, body cap, localhost bind, mutation auth

**Files:**
- Modify: `crates/nethacked-agent/src/graphql.rs`
- Modify: `crates/nethacked-agent/src/bin/graphql.rs`
- Test: `crates/nethacked-agent/tests/graphql_http_test.rs` (new)

**Interfaces:**
- Consumes: `netconfig::{bearer_ok, resolve_bind_addr, token_from_env}`.
- Produces:
  - `pub struct Authorized(pub bool);` (request data)
  - mutations return `async_graphql::Result<_>` and call `require_mutation_auth(ctx)?` first; `ctx.data_opt::<Authorized>()` absent ⇒ allowed (in-process `schema.execute`)
  - `pub const MAX_BODY_BYTES: usize = 64 * 1024;`
  - `pub fn create_router(schema: NetHackEDSchema, token: Option<String>) -> axum::Router` serving `GET /graphql` (GraphiQL) and `POST /graphql`

- [ ] **Step 1: Write failing tests**

Create `crates/nethacked-agent/tests/graphql_http_test.rs`:
```rust
//! HTTP-level tests for the GraphQL server: body cap, depth/complexity limits, mutation auth.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use nethacked_agent::graphql::{create_router, create_schema, AppState};
use nethacked_agent::AgentSession;
use serde_json::json;

async fn spawn(token: Option<&str>) -> String {
    let schema = create_schema(AppState { session: Arc::new(Mutex::new(AgentSession::new(42))) });
    let app = create_router(schema, token.map(String::from));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    addr
}

fn post(addr: &str, extra_headers: &str, body: &str) -> (u16, String) {
    let mut s = TcpStream::connect(addr).unwrap();
    let req = format!(
        "POST /graphql HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{extra_headers}\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).unwrap();
    let mut out = String::new();
    let _ = s.read_to_string(&mut out);
    let code = out.split_whitespace().nth(1).unwrap().parse().unwrap();
    (code, out.split("\r\n\r\n").nth(1).unwrap_or("").to_string())
}

async fn run(addr: &str, headers: &'static str, body: String) -> (u16, String) {
    let a = addr.to_string();
    tokio::task::spawn_blocking(move || post(&a, headers, &body)).await.unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn query_works_and_oversize_body_rejected() {
    let addr = spawn(None).await;
    let (code, body) = run(&addr, "", json!({"query":"{ playerState { hp } }"}).to_string()).await;
    assert_eq!(code, 200);
    assert!(body.contains("\"hp\":18"));
    let huge = json!({"query": format!("{{ playerState {{ hp }} }} #{}", "x".repeat(70 * 1024))}).to_string();
    let (code, _) = run(&addr, "", huge).await;
    assert_eq!(code, 413);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn alias_amplification_is_limited() {
    let addr = spawn(None).await;
    // 1500 aliases x 2 fields = complexity 3000 > 2000, body ~36 KiB < 64 KiB.
    let q: String = (0..1500).map(|i| format!("a{}: playerState {{ hp }} ", i)).collect();
    let (code, body) = run(&addr, "", json!({"query": format!("{{ {} }}", q)}).to_string()).await;
    assert_eq!(code, 200);
    assert!(body.contains("errors"), "complexity limit not enforced");
    assert!(!body.contains("\"a1499\""), "complexity limit not enforced");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mutations_require_token_when_configured() {
    let addr = spawn(Some("tok")).await;
    let m = json!({"query":"mutation { resetGame(seed: 1) }"}).to_string();
    let (_, body) = run(&addr, "", m.clone()).await;
    assert!(body.contains("errors"), "unauthenticated mutation succeeded: {}", body);
    let (_, body) = run(&addr, "Authorization: Bearer tok\r\n", m).await;
    assert!(body.contains("\"resetGame\":true"), "{}", body);
    let (code, body) = run(&addr, "", json!({"query":"{ playerState { hp } }"}).to_string()).await;
    assert_eq!(code, 200);
    assert!(body.contains("hp"));
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p nethacked-agent --test graphql_http_test`
Expected: FAIL to compile (`create_router` missing).

- [ ] **Step 3: Implement schema changes**

In `graphql.rs`:
```rust
pub const MAX_BODY_BYTES: usize = 64 * 1024;

/// Per-request authorization decision inserted by the HTTP handler.
pub struct Authorized(pub bool);

fn require_mutation_auth(ctx: &Context<'_>) -> async_graphql::Result<()> {
    match ctx.data_opt::<Authorized>() {
        Some(Authorized(false)) => Err("unauthorized: missing or invalid bearer token".into()),
        _ => Ok(()),
    }
}
```
- Each `MutationRoot` method returns `async_graphql::Result<T>` (`StepResultGql` / `bool`), starts with `require_mutation_auth(ctx)?;` and wraps its return in `Ok(...)`.
- `create_schema`:
  ```rust
  Schema::build(QueryRoot, MutationRoot, EmptySubscription)
      .data(state)
      .limit_depth(16)
      .limit_complexity(2000)
      .finish()
  ```
- Router (same file, already non-wasm module):
  ```rust
  use axum::{body::Body, extract::State, http::{HeaderMap, StatusCode}, response::{Html, IntoResponse, Response}, routing::get, Json, Router};

  #[derive(Clone)]
  struct GqlApp {
      schema: NetHackEDSchema,
      token: Option<Arc<str>>,
  }

  pub fn create_router(schema: NetHackEDSchema, token: Option<String>) -> Router {
      Router::new()
          .route("/graphql", get(graphiql).post(graphql_post))
          .with_state(GqlApp { schema, token: token.map(Arc::from) })
  }

  async fn graphiql() -> impl IntoResponse {
      Html(async_graphql::http::GraphiQLSource::build().endpoint("/graphql").finish())
  }

  async fn graphql_post(State(app): State<GqlApp>, headers: HeaderMap, body: Body) -> Response {
      let bytes = match axum::body::to_bytes(body, MAX_BODY_BYTES).await {
          Ok(b) => b,
          Err(_) => return (StatusCode::PAYLOAD_TOO_LARGE, "request body too large").into_response(),
      };
      let request: async_graphql::Request = match serde_json::from_slice(&bytes) {
          Ok(r) => r,
          Err(e) => return (StatusCode::BAD_REQUEST, format!("invalid GraphQL request: {}", e)).into_response(),
      };
      let authorized = crate::netconfig::bearer_ok(&headers, app.token.as_deref());
      let response = app.schema.execute(request.data(Authorized(authorized))).await;
      Json(response).into_response()
  }
  ```
- Existing unit tests in `graphql.rs` still call `schema.execute(...)` directly and must keep passing (no `Authorized` data ⇒ allowed).

- [ ] **Step 4: Update binary**

`bin/graphql.rs`:
```rust
use nethacked_agent::graphql::{create_router, create_schema, AppState};
use nethacked_agent::netconfig::{resolve_bind_addr, token_from_env};
use nethacked_agent::AgentSession;
use std::sync::{Arc, Mutex};

#[tokio::main]
async fn main() {
    let port = std::env::var("PORT").unwrap_or_else(|_| "4000".to_string());
    let args: Vec<String> = std::env::args().collect();
    let addr = resolve_bind_addr(&args, std::env::var("NETHACKED_BIND").ok(), &format!("127.0.0.1:{}", port));

    let state = AppState { session: Arc::new(Mutex::new(AgentSession::new(42))) };
    let app = create_router(create_schema(state), token_from_env());

    println!("🗡️ NetHackED GraphQL Server listening on http://{}", addr);
    println!("📊 GraphiQL Interactive Explorer: http://{}/graphql", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind GraphQL address");
    axum::serve(listener, app).await.expect("GraphQL server error");
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p nethacked-agent && cargo build -p nethacked-agent --bins`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/nethacked-agent
git commit -m "fix(graphql): depth/complexity limits, 64KiB body cap, localhost bind and token-guarded mutations"
```

---

### Task 9: Player damage helper; trap damage cannot wrap

**Files:**
- Modify: `crates/nethacked-sim/src/world.rs` (add method in `impl SimulationWorld`)
- Modify: `crates/nethacked-sim/src/actions/movement.rs:163-167`
- Test: `crates/nethacked-sim/tests/integrity_tests.rs` (new)

**Interfaces:**
- Produces: `pub fn damage_player(&mut self, amount: u32, cause: &str) -> Vec<GameEvent>` on `SimulationWorld`.

- [ ] **Step 1: Write failing test**

Create `crates/nethacked-sim/tests/integrity_tests.rs`:
```rust
//! Simulation integrity regressions: damage, genocide, wands/wishes, melee variance, timers.

use nethacked_arena::ItemLocation;
use nethacked_data::{create_item_record, create_monster_record, ItemKindId, MonsterSpeciesId};
use nethacked_sim::{ActionAst, Buc, Coord, Direction, SimulationWorld, Tile};
use nethacked_types::{TrapRecord, TrapState, TrapType};

fn open_east(sim: &mut SimulationWorld) -> Coord {
    let p = sim.arena.actors.get(sim.player_id).unwrap().coord;
    let east = Coord::new(p.x + 1, p.y).unwrap();
    sim.level.set_tile(east, Tile::Room);
    if let Some(id) = sim.actor_at(east) {
        sim.arena.actors.remove(id);
    }
    east
}

#[test]
fn arrow_trap_at_one_hp_kills_without_wrapping() {
    let mut sim = SimulationWorld::new_with_seed(5);
    let east = open_east(&mut sim);
    sim.level.traps.insert(east, TrapRecord { id: 1, trap_type: TrapType::Arrow, state: TrapState::Hidden, coord: east });
    sim.arena.actors.get_mut(sim.player_id).unwrap().hp = 1;
    sim.step_player_action(ActionAst::Move(Direction::East));
    let p = sim.arena.actors.get(sim.player_id).unwrap();
    assert_eq!(p.hp, 0);
    assert!(p.is_dead);
}

#[test]
fn damage_player_saturates_and_marks_death() {
    let mut sim = SimulationWorld::new_with_seed(5);
    sim.arena.actors.get_mut(sim.player_id).unwrap().hp = 3;
    let ev = sim.damage_player(2, "test");
    assert!(ev.is_empty());
    assert!(!sim.arena.actors.get(sim.player_id).unwrap().is_dead);
    assert_eq!(sim.arena.actors.get(sim.player_id).unwrap().hp, 1);
    let ev = sim.damage_player(50, "an arrow trap");
    let p = sim.arena.actors.get(sim.player_id).unwrap();
    assert_eq!(p.hp, 0);
    assert!(p.is_dead);
    assert!(!ev.is_empty());
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p nethacked-sim --test integrity_tests`
Expected: FAIL (compile error `damage_player`; trap test panics with "attempt to subtract with overflow").

- [ ] **Step 3: Implement**

In `world.rs` `impl SimulationWorld`:
```rust
    /// Apply damage to the hero without underflow; marks death at 0 HP.
    pub fn damage_player(&mut self, amount: u32, cause: &str) -> Vec<GameEvent> {
        let mut events = Vec::new();
        if let Some(p) = self.arena.actors.get_mut(self.player_id) {
            p.hp = p.hp.saturating_sub(amount);
            if p.hp == 0 && !p.is_dead {
                p.is_dead = true;
                events.push(GameEvent::LogMessage { text: format!("You die... killed by {}.", cause) });
            }
        }
        events
    }
```
(import `crate::events::GameEvent` in `world.rs` if not already imported).

In `movement.rs` replace the Arrow/Dart arm body's damage block with:
```rust
                                        events.extend(self.damage_player(2, &format!("{:?} trap", triggered_type).to_lowercase()));
```
If the borrow checker rejects this because `trap` (a `get_mut` borrow of `self.level.traps`) is still live, first copy `triggered_type` out and end the `if let Some(trap)` borrow before matching (restructure: `let triggered = self.level.traps.get_mut(&target_coord).and_then(|trap| nethacked_core::traps::trigger_trap(trap, is_flying));` then `if let Some(triggered_type) = triggered { match ... }`), computing `is_flying` before.

- [ ] **Step 4: Run tests**

Run: `cargo test -p nethacked-sim`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/nethacked-sim
git commit -m "fix(sim): saturating player damage helper; traps can no longer wrap HP"
```

---

### Task 10: Genocide targets only matching monsters

**Files:**
- Modify: `crates/nethacked-data/src/monsters.rs` (add `monster_class_of`), `crates/nethacked-data/src/lib.rs` (export)
- Modify: `crates/nethacked-sim/src/actions/items.rs:331-362`
- Modify: `crates/nethacked-sim/tests/simulation_tests.rs:2209-2239` (test used the wrong species name)
- Test: `crates/nethacked-sim/tests/integrity_tests.rs` (append)

**Interfaces:**
- Produces:
  - `nethacked_data::monster_class_of(name: &str) -> Option<char>` — bestiary glyph by case-insensitive name
  - `SimulationWorld::remove_actor_dropping_items(&mut self, id: ActorId)` (pub(crate)) — carried items move to `Floor(actor.coord)`, then actor removed
  - species genocide is stored lowercase in `genocide_registry.genocided_species`

- [ ] **Step 1: Write failing tests**

Append to `integrity_tests.rs`:
```rust
fn genocide_scroll(sim: &mut SimulationWorld, buc: Buc) -> usize {
    let mut scroll = create_item_record(ItemKindId::ScrollOfIdentify, ItemLocation::CarriedBy(sim.player_id), buc);
    scroll.name = "scroll of genocide".into();
    let id = sim.arena.spawn_item(scroll);
    sim.arena.items_carried_by(sim.player_id).iter().position(|&i| i == id).unwrap()
}

#[test]
fn blessed_genocide_spares_non_lich_actors_and_drops_items() {
    let mut sim = SimulationWorld::new_with_seed(77);
    let p = sim.arena.actors.get(sim.player_id).unwrap().coord;
    let c1 = Coord::new(p.x + 2, p.y).unwrap();
    let c2 = Coord::new(p.x + 3, p.y).unwrap();
    let lich = sim.arena.spawn_actor(create_monster_record(MonsterSpeciesId::Lich, c1));
    let dog = sim.arena.spawn_actor(create_monster_record(MonsterSpeciesId::LittleDog, c2));
    let loot = sim.arena.spawn_item(create_item_record(ItemKindId::LongSword, ItemLocation::CarriedBy(lich), Buc::Uncursed));
    let before = sim.arena.actors.len();

    let idx = genocide_scroll(&mut sim, Buc::Blessed);
    sim.step_player_action(ActionAst::Read(idx));

    assert!(!sim.arena.actors.contains_key(lich));
    assert!(sim.arena.actors.contains_key(dog));
    assert_eq!(sim.arena.actors.len(), before - 1);
    assert_eq!(sim.arena.items.get(loot).unwrap().location, ItemLocation::Floor(c1));
}

#[test]
fn uncursed_genocide_hits_bestiary_goblins() {
    let mut sim = SimulationWorld::new_with_seed(78);
    let p = sim.arena.actors.get(sim.player_id).unwrap().coord;
    let gob = sim.arena.spawn_actor(create_monster_record(MonsterSpeciesId::Goblin, Coord::new(p.x + 2, p.y).unwrap()));
    let idx = genocide_scroll(&mut sim, Buc::Uncursed);
    sim.step_player_action(ActionAst::Read(idx));
    assert!(!sim.arena.actors.contains_key(gob));
    assert!(nethacked_core::genocide::is_genocided(&sim.genocide_registry, "goblin", 'o'));
}

#[test]
fn monster_class_lookup() {
    assert_eq!(nethacked_data::monster_class_of("master lich"), Some('L'));
    assert_eq!(nethacked_data::monster_class_of("GOBLIN"), Some('o'));
    assert_eq!(nethacked_data::monster_class_of("no such thing"), None);
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p nethacked-sim --test integrity_tests genocide`
Expected: FAIL.

- [ ] **Step 3: Implement**

`nethacked-data/src/monsters.rs`:
```rust
/// NetHack monster class letter (bestiary glyph) for a species name, case-insensitive.
pub fn monster_class_of(name: &str) -> Option<char> {
    BESTIARY.iter().find(|m| m.name.eq_ignore_ascii_case(name)).map(|m| m.glyph)
}
```
Export in `lib.rs` alongside the other `monsters::` re-exports.

`nethacked-sim` (in `items.rs` or `world.rs`):
```rust
    /// Remove an actor, leaving anything it carried on the floor where it stood.
    pub(crate) fn remove_actor_dropping_items(&mut self, id: ActorId) {
        let Some(coord) = self.arena.actors.get(id).map(|a| a.coord) else { return };
        for item_id in self.arena.items_carried_by(id) {
            if let Some(item) = self.arena.items.get_mut(item_id) {
                item.location = ItemLocation::Floor(coord);
            }
        }
        self.arena.actors.remove(id);
    }

    fn actor_is_genocided(&self, name: &str) -> bool {
        let lower = name.to_lowercase();
        let class = nethacked_data::monster_class_of(name);
        self.genocide_registry.genocided_species.contains(&lower)
            || class.map(|c| self.genocide_registry.genocided_classes.contains(&c)).unwrap_or(false)
    }
```
Uncursed arm: `GenocideTarget::Species("goblin".to_string())`; Blessed arm keeps `GenocideTarget::Class('L')`. In both arms collect `to_remove` with `!actor.is_player && self.actor_is_genocided(&actor.name)` and call `self.remove_actor_dropping_items(aid)` for each.

Update the old test at `simulation_tests.rs:2210`: actor name `"goblin"`; final assertion `assert!(is_genocided(&sim.genocide_registry, "goblin", 'o'));`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p nethacked-sim && cargo test -p nethacked-data && cargo test -p nethacked-core`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/nethacked-data crates/nethacked-sim
git commit -m "fix(sim): genocide removes only matching species/class and drops their inventory"
```

---

### Task 11: Wands require a wand, wishes require charges, exact wish matching, initial charges

**Files:**
- Modify: `crates/nethacked-data/src/items.rs:925-944` (`create_item_record`)
- Modify: `crates/nethacked-sim/src/actions/items.rs` (`handle_zap_wand` start, `handle_wish`, recharge `as i8`)
- Test: `crates/nethacked-sim/tests/integrity_tests.rs` (append); data unit test

**Interfaces:**
- Produces:
  - `nethacked_data::initial_wand_charges(id: ItemKindId) -> i8` (pub): `WandOfWishing => 1`, `WandOfSecretDoorDetection => 13`, other `Wand`-class kinds `=> 6`, non-wands `=> 0`; used by `create_item_record` for `enchantment`
  - `pub fn normalize_wish_name(query: &str) -> String` in `nethacked-sim/src/actions/items.rs` (re-exported as `nethacked_sim::normalize_wish_name`): lowercase, trims, drops leading `a`/`an`/`the`
  - unwishable kinds: `AmuletOfYendor` (→ imitation), `BellOfOpening`, `CandelabrumOfInvocation`, `BookOfTheDead`, `OrbOfFate`, `HeartOfAhriman`, `MagicMirrorOfMerlin`, `EyesOfTheOverworld`, `MasterKeyOfThievery`, `TsurugiOfMuramasa`, `PlatinumYendorianExpressCard`, `StaffOfAesculapius`, `OrbOfDetection` (→ nothing)

- [ ] **Step 1: Write failing tests**

Append to `integrity_tests.rs`:
```rust
fn has_item(sim: &SimulationWorld, name: &str) -> bool {
    sim.arena.items.values().any(|it| it.name == name)
}

#[test]
fn zap_without_wand_is_free_noop() {
    let mut sim = SimulationWorld::new_with_seed(90);
    for id in sim.arena.items_carried_by(sim.player_id) {
        if sim.arena.items.get(id).unwrap().class == nethacked_types::ItemClass::Wand {
            sim.arena.items.remove(id);
        }
    }
    let energy = sim.scheduler.hero_energy;
    let ev = sim.step_player_action(ActionAst::ZapWand { dir: Direction::East, energy: 6 });
    assert!(ev.iter().any(|e| format!("{:?}", e).contains("no wand")));
    assert!(!ev.iter().any(|e| matches!(e, nethacked_sim::GameEvent::BeamPropagated { .. })));
    assert_eq!(sim.scheduler.hero_energy, energy);
}

#[test]
fn wish_requires_charged_wand_of_wishing() {
    let mut sim = SimulationWorld::new_with_seed(91);
    sim.step_player_action(ActionAst::Wish("long sword".into()));
    assert_eq!(sim.arena.items.values().filter(|it| it.name == "long sword").count(), 0);

    sim.arena.spawn_item(create_item_record(ItemKindId::WandOfWishing, ItemLocation::CarriedBy(sim.player_id), Buc::Uncursed));
    sim.step_player_action(ActionAst::Wish("a blessed +2 long sword".into()));
    let sword = sim.arena.items.values().find(|it| it.name == "long sword").expect("wish granted");
    assert_eq!(sword.buc, Buc::Blessed);
    assert_eq!(sword.enchantment, 2);
    // Wand had 1 charge: the next wish must not create anything.
    let dagger_count_before = sim.arena.items.values().filter(|it| it.name == "dagger").count();
    sim.step_player_action(ActionAst::Wish("dagger".into()));
    assert_eq!(sim.arena.items.values().filter(|it| it.name == "dagger").count(), dagger_count_before);
}

#[test]
fn wishing_for_the_amulet_gives_imitation() {
    let mut sim = SimulationWorld::new_with_seed(92);
    sim.arena.spawn_item(create_item_record(ItemKindId::WandOfWishing, ItemLocation::CarriedBy(sim.player_id), Buc::Uncursed));
    sim.step_player_action(ActionAst::Wish("the Amulet of Yendor".into()));
    assert!(!has_item(&sim, "Amulet of Yendor"));
    assert!(has_item(&sim, "cheap plastic imitation of the Amulet of Yendor"));
}

#[test]
fn wish_substring_does_not_match() {
    let mut sim = SimulationWorld::new_with_seed(93);
    sim.arena.spawn_item(create_item_record(ItemKindId::WandOfWishing, ItemLocation::CarriedBy(sim.player_id), Buc::Uncursed));
    let before = sim.arena.items.len();
    sim.step_player_action(ActionAst::Wish("sword".into()));
    assert_eq!(sim.arena.items.len(), before);
}

#[test]
fn catalog_wands_start_charged() {
    let w = create_item_record(ItemKindId::WandOfStriking, ItemLocation::Limbo, Buc::Uncursed);
    assert_eq!(w.enchantment, 6);
    let w = create_item_record(ItemKindId::WandOfWishing, ItemLocation::Limbo, Buc::Uncursed);
    assert_eq!(w.enchantment, 1);
    let s = create_item_record(ItemKindId::LongSword, ItemLocation::Limbo, Buc::Uncursed);
    assert_eq!(s.enchantment, 0);
}

#[test]
fn normalize_wish_strips_articles() {
    assert_eq!(nethacked_sim::normalize_wish_name("  The Amulet of Yendor "), "amulet of yendor");
    assert_eq!(nethacked_sim::normalize_wish_name("an elven mithril-coat"), "elven mithril-coat");
}
```
Note: `parse_wish` (core) already strips BUC and `+N`; the sim normalizes the remaining name.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p nethacked-sim --test integrity_tests -- wish zap catalog normalize`
Expected: FAIL.

- [ ] **Step 3: Implement data side**

```rust
/// Initial charges for a freshly generated wand (stored in `enchantment`).
pub fn initial_wand_charges(id: ItemKindId) -> i8 {
    let arch = get_item_archetype(id);
    if arch.class != nethacked_types::ItemClass::Wand {
        return 0;
    }
    match id {
        ItemKindId::WandOfWishing => 1,
        ItemKindId::WandOfSecretDoorDetection => 13,
        _ => 6,
    }
}
```
In `create_item_record`: `enchantment: initial_wand_charges(id),`. Export `initial_wand_charges` from `lib.rs`.

- [ ] **Step 4: Implement sim side**

`handle_zap_wand`: right after computing `wand_id`:
```rust
        let Some(wid) = wand_id else {
            events.push(GameEvent::LogMessage { text: "You have no wand to zap.".into() });
            return events;
        };
```
then use `wid` directly (remove the `"wand of striking"` fallbacks; the `else` of `get_mut(wid)` can no longer happen—use `let Some(wand_item) = self.arena.items.get_mut(wid) else { return events; };`).

Recharge site (`w_mut.enchantment = new_w.charges as i8;`) and zap/wish sites: use `new_charges.charges.min(i8::MAX as u32) as i8`.

`handle_wish`:
```rust
        let Some(wid) = wow_id else {
            events.push(GameEvent::LogMessage { text: "You have no means of wishing.".into() });
            return events;
        };
        // existing charge consumption on wid (unchanged), returning early when empty
```
Matching:
```rust
pub fn normalize_wish_name(query: &str) -> String {
    let lower = query.trim().to_lowercase();
    for article in ["a ", "an ", "the "] {
        if let Some(rest) = lower.strip_prefix(article) {
            return rest.trim().to_string();
        }
    }
    lower
}

const UNWISHABLE: &[ItemKindId] = &[
    ItemKindId::BellOfOpening, ItemKindId::CandelabrumOfInvocation, ItemKindId::BookOfTheDead,
    ItemKindId::OrbOfFate, ItemKindId::HeartOfAhriman, ItemKindId::MagicMirrorOfMerlin,
    ItemKindId::EyesOfTheOverworld, ItemKindId::MasterKeyOfThievery, ItemKindId::TsurugiOfMuramasa,
    ItemKindId::PlatinumYendorianExpressCard, ItemKindId::StaffOfAesculapius, ItemKindId::OrbOfDetection,
];
```
In `handle_wish` after `parse_wish`:
```rust
            let wanted = normalize_wish_name(&item_query);
            let matched_arch = nethacked_data::ITEM_CATALOG.iter().find(|arch| arch.name.to_lowercase() == wanted);
            match matched_arch {
                Some(arch) if arch.id == ItemKindId::AmuletOfYendor => {
                    let mut fake = create_item_record(arch.id, ItemLocation::Floor(player.coord), buc);
                    fake.name = "cheap plastic imitation of the Amulet of Yendor".into();
                    self.arena.spawn_item(fake);
                    events.push(GameEvent::LogMessage { text: nethacked_i18n::Messages::wish_granted("cheap plastic imitation of the Amulet of Yendor", self.locale) });
                }
                Some(arch) if UNWISHABLE.contains(&arch.id) => {
                    events.push(GameEvent::LogMessage { text: format!("You feel a vague sense of loss. The {} cannot be wished for.", arch.name) });
                }
                Some(arch) => { /* existing spawn + message */ }
                None => { /* existing "received nothing" message */ }
            }
```
Re-export `normalize_wish_name` from `nethacked-sim/src/lib.rs` (`pub use actions::items::normalize_wish_name;` — make `items` module `pub` access path valid; it is `pub mod items` already).

- [ ] **Step 5: Run full sim and workspace tests**

Run: `cargo test --workspace --exclude nethacked-py 2>&1 | grep -E "test result|FAILED|panicked"`
Expected: PASS. Existing tests that zapped without a wand or wished without a wand must be fixed by giving the hero the wand in setup (not by weakening assertions). Record each such adjustment in the commit message body.

- [ ] **Step 6: Commit**

```bash
git add crates/nethacked-data crates/nethacked-sim
git commit -m "fix(sim): zapping and wishing require the item, exact wish matching, wands spawn charged"
```

---

### Task 12: Melee uses the world RNG; damage-bonus argument fixed

**Files:**
- Modify: `crates/nethacked-sim/src/combat.rs:86-90`
- Test: `crates/nethacked-sim/tests/integrity_tests.rs` (append)

**Interfaces:**
- Consumes: `SimulationWorld.rng` (`ChaCha8Rng`), `rand::Rng::random_range`.

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn melee_outcomes_vary_with_seed() {
    use std::collections::BTreeSet;
    let mut outcomes = BTreeSet::new();
    for seed in 0..40u64 {
        let mut sim = SimulationWorld::new_with_seed(seed);
        let east = open_east(&mut sim);
        let mut mon = create_monster_record(MonsterSpeciesId::Goblin, east);
        mon.hp = 1000;
        mon.max_hp = 1000;
        mon.ac = 4;
        let mid = sim.arena.spawn_actor(mon);
        sim.step_player_action(ActionAst::MeleeAttack(east));
        let hp = sim.arena.actors.get(mid).map(|m| m.hp).unwrap_or(0);
        outcomes.insert(1000 - hp);
    }
    assert!(outcomes.len() >= 3, "melee is not random: {:?}", outcomes);
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p nethacked-sim --test integrity_tests melee_outcomes_vary_with_seed`
Expected: FAIL (single outcome; monster counter-attacks do not change the goblin's HP).

- [ ] **Step 3: Implement**

```rust
        let to_hit_bonus = attacker.level as i32 + weapon_ench + skill_hit_bonus;
        let d20 = self.rng.random_range(1..=20u32);
        let dmg_roll = (self.rng.random_range(1..=6i32) + skill_dmg_bonus).max(1) as u32;

        let result = resolve_melee_attack(to_hit_bonus, skill_dmg_bonus, def_combat, d20, dmg_roll, weapon_ench);
```
Change the import to `use rand::{Rng, RngCore};`.
Note: `skill_dmg_bonus` is now both added into `dmg_roll` and passed as the damage bonus — avoid double counting: pass `0` as `attacker_dmg_bonus` since `dmg_roll` already includes it:
```rust
        let result = resolve_melee_attack(to_hit_bonus, 0, def_combat, d20, dmg_roll, weapon_ench);
```

- [ ] **Step 4: Run all tests; fix fallout via setup**

Run: `cargo test --workspace --exclude nethacked-py 2>&1 | grep -E "test result|FAILED|panicked"`
Expected: new test PASS. For each pre-existing test that now fails because it relied on a guaranteed hit or exact damage: make the outcome deterministic through setup (e.g. set defender `ac` to 20 so `10 + 20 + bonus >= 20` guarantees a hit; set defender `hp` to 1 for guaranteed kills; or loop the attack until the defender dies with a bound like 50 iterations). Do not delete or loosen the behavior being asserted. List adjusted tests in the commit body.

- [ ] **Step 5: Commit**

```bash
git add crates/nethacked-sim
git commit -m "fix(sim): melee to-hit and damage rolls come from the world RNG"
```

---

### Task 13: Prayer timeout and luck decay only advance with game time

**Files:**
- Modify: `crates/nethacked-sim/src/actions/mod.rs:20-130`
- Test: `crates/nethacked-sim/tests/integrity_tests.rs` (append)

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn bumping_a_wall_does_not_tick_prayer_timeout() {
    let mut sim = SimulationWorld::new_with_seed(12);
    let p = sim.arena.actors.get(sim.player_id).unwrap().coord;
    let west = Coord::new(p.x - 1, p.y).unwrap();
    sim.level.set_tile(west, Tile::Wall { horizontal: false });
    sim.divine_state.prayer_timeout = 100;
    let turn = sim.scheduler.turn;
    for _ in 0..10 {
        sim.step_player_action(ActionAst::Move(Direction::West));
    }
    assert_eq!(sim.scheduler.turn, turn, "wall bump should take no time");
    assert_eq!(sim.divine_state.prayer_timeout, 100);
}
```
(If `p.x == 0` for the seed, pick another seed where the player is not on column 0; the test should assert `p.x > 0` first.)

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p nethacked-sim --test integrity_tests bumping_a_wall`
Expected: FAIL (timeout decremented to 90).

- [ ] **Step 3: Implement**

At the top of `step_player_action` (after the dead check):
```rust
        let energy_before = self.scheduler.hero_energy;
        let turn_before = self.scheduler.turn;
```
Replace the tail with:
```rust
        let spent_time = self.scheduler.hero_energy != energy_before;

        // Process monster actions and turn scheduler ticks
        let sim_events = self.process_turn_ticks();
        events.extend(sim_events);

        if spent_time {
            self.divine_state.prayer_timeout = nethacked_core::religion::tick_prayer_timeout(self.divine_state.prayer_timeout);
        }

        if self.scheduler.turn != turn_before && self.scheduler.turn > 0 && self.scheduler.turn % 600 == 0 {
            self.tick_luck_decay();
        }
```

- [ ] **Step 4: Run tests**

Run: `cargo test --workspace --exclude nethacked-py 2>&1 | grep -E "test result|FAILED|panicked"`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/nethacked-sim
git commit -m "fix(sim): prayer timeout and luck decay tick only when time passes"
```

---

### Task 14: Level persistence with ID remapping (containers, monster inventories, unpaid ledger, steed, quiver)

**Files:**
- Modify: `crates/nethacked-sim/src/world.rs:19-24` (`StoredLevel`)
- Modify: `crates/nethacked-sim/src/actions/stairs.rs:16-73` (`pack_current_level`, restore branch of `unpack_or_generate_level`)
- Modify: arrival code in `stairs.rs` where the hero's coord is set after a level change (descend, ascend, branch transitions, mysterious force) — move the steed with the hero
- Test: `crates/nethacked-sim/tests/level_persistence_tests.rs` (new)

**Interfaces:**
- Produces:
  ```rust
  pub struct StoredLevel {
      pub level: DungeonLevel,
      pub monsters: Vec<(ActorId, ActorRecord)>,
      pub items: Vec<(ItemId, ItemRecord)>,
      pub unpaid_items: Vec<(ItemId, u32)>,
  }
  ```
  `pub(crate) fn place_steed_with_hero(&mut self)` — if mounted and the steed exists, set steed coord to the hero's coord.

- [ ] **Step 1: Write failing tests**

Create `crates/nethacked-sim/tests/level_persistence_tests.rs`:
```rust
//! Level pack/unpack keeps every cross-entity reference valid.

use nethacked_arena::ItemLocation;
use nethacked_data::{create_item_record, create_monster_record, ItemKindId, MonsterSpeciesId};
use nethacked_sim::{ActionAst, Buc, SimulationWorld};
use nethacked_types::MountState;

fn go_down_and_up(sim: &mut SimulationWorld) {
    let down = sim.level.stairs_down;
    sim.arena.actors.get_mut(sim.player_id).unwrap().coord = down;
    sim.step_player_action(ActionAst::Descend);
    assert_eq!(sim.depth, 2);
    sim.step_player_action(ActionAst::Ascend);
    assert_eq!(sim.depth, 1);
}

fn assert_no_dangling_refs(sim: &SimulationWorld) {
    for (id, it) in sim.arena.items.iter() {
        match it.location {
            ItemLocation::InContainer(c) => assert!(sim.arena.items.contains_key(c), "item {:?} in missing container", id),
            ItemLocation::CarriedBy(a) => assert!(sim.arena.actors.contains_key(a), "item {:?} carried by missing actor", id),
            _ => {}
        }
    }
    for (iid, _) in &sim.unpaid_items {
        assert!(sim.arena.items.contains_key(*iid), "unpaid ledger references missing item");
    }
}

fn find_named(sim: &SimulationWorld, name: &str) -> Vec<nethacked_arena::ItemId> {
    sim.arena.items.iter().filter(|(_, it)| it.name == name).map(|(id, _)| id).collect()
}

#[test]
fn nested_floor_containers_survive_round_trip() {
    let mut sim = SimulationWorld::new_with_seed(321);
    let spot = sim.level.stairs_down;
    let outer = sim.arena.spawn_item(create_item_record(ItemKindId::Sack, ItemLocation::Floor(spot), Buc::Uncursed));
    let mut inner_rec = create_item_record(ItemKindId::Sack, ItemLocation::InContainer(outer), Buc::Uncursed);
    inner_rec.name = "inner sack".into();
    let inner = sim.arena.spawn_item(inner_rec);
    let mut gem = create_item_record(ItemKindId::Dagger, ItemLocation::InContainer(inner), Buc::Uncursed);
    gem.name = "precious dagger".into();
    sim.arena.spawn_item(gem);

    go_down_and_up(&mut sim);

    assert_no_dangling_refs(&sim);
    let inner_ids = find_named(&sim, "inner sack");
    assert_eq!(inner_ids.len(), 1);
    let dagger_ids = find_named(&sim, "precious dagger");
    assert_eq!(dagger_ids.len(), 1);
    assert_eq!(sim.arena.items.get(dagger_ids[0]).unwrap().location, ItemLocation::InContainer(inner_ids[0]));
    match sim.arena.items.get(inner_ids[0]).unwrap().location {
        ItemLocation::InContainer(o) => assert!(matches!(sim.arena.items.get(o).unwrap().location, ItemLocation::Floor(_))),
        ref other => panic!("inner sack not in outer sack: {:?}", other),
    }
}

#[test]
fn hero_container_contents_untouched() {
    let mut sim = SimulationWorld::new_with_seed(322);
    let bag = sim.arena.spawn_item(create_item_record(ItemKindId::Sack, ItemLocation::CarriedBy(sim.player_id), Buc::Uncursed));
    let inside = sim.arena.spawn_item(create_item_record(ItemKindId::Dagger, ItemLocation::InContainer(bag), Buc::Uncursed));
    go_down_and_up(&mut sim);
    assert_eq!(sim.arena.items.get(inside).unwrap().location, ItemLocation::InContainer(bag));
    assert_no_dangling_refs(&sim);
}

#[test]
fn monster_inventory_survives_round_trip() {
    let mut sim = SimulationWorld::new_with_seed(323);
    let room = sim.level.rooms[0].center();
    let mut gob = create_monster_record(MonsterSpeciesId::Goblin, room);
    gob.name = "hoarder goblin".into();
    gob.speed = 0;
    let gid = sim.arena.spawn_actor(gob);
    let mut loot = create_item_record(ItemKindId::LongSword, ItemLocation::CarriedBy(gid), Buc::Uncursed);
    loot.name = "goblin loot".into();
    sim.arena.spawn_item(loot);

    go_down_and_up(&mut sim);

    assert_no_dangling_refs(&sim);
    let loot_id = find_named(&sim, "goblin loot")[0];
    let ItemLocation::CarriedBy(owner) = sim.arena.items.get(loot_id).unwrap().location else { panic!("loot not carried") };
    assert_eq!(sim.arena.actors.get(owner).unwrap().name, "hoarder goblin");
}

#[test]
fn unpaid_ledger_follows_items() {
    let mut sim = SimulationWorld::new_with_seed(324);
    if sim.unpaid_items.len() < 2 {
        return; // seed without a shop; covered by other seeds in CI sweep below
    }
    let (carried_id, _) = sim.unpaid_items[0];
    sim.arena.items.get_mut(carried_id).unwrap().location = ItemLocation::CarriedBy(sim.player_id);
    let floor_count = sim.unpaid_items.len() - 1;

    let down = sim.level.stairs_down;
    sim.arena.actors.get_mut(sim.player_id).unwrap().coord = down;
    sim.step_player_action(ActionAst::Descend);
    assert!(sim.is_unpaid(carried_id), "carried unpaid item lost its debt");
    sim.step_player_action(ActionAst::Ascend);

    assert!(sim.is_unpaid(carried_id));
    assert_eq!(sim.unpaid_items.len(), floor_count + 1);
    assert_no_dangling_refs(&sim);
}

#[test]
fn unpaid_ledger_follows_items_over_seeds() {
    let mut checked = 0;
    for seed in 0..30u64 {
        let mut sim = SimulationWorld::new_with_seed(seed);
        let before = sim.unpaid_items.len();
        if before == 0 {
            continue;
        }
        go_down_and_up(&mut sim);
        assert_eq!(sim.unpaid_items.len(), before, "seed {}", seed);
        assert_no_dangling_refs(&sim);
        checked += 1;
    }
    assert!(checked > 0, "no seed produced a shop");
}

#[test]
fn steed_travels_with_hero() {
    let mut sim = SimulationWorld::new_with_seed(325);
    let p = sim.arena.actors.get(sim.player_id).unwrap().coord;
    let mut pony = create_monster_record(MonsterSpeciesId::Dog, p);
    pony.is_tame = true;
    let steed = sim.arena.spawn_actor(pony);
    sim.hero.mount = Some(MountState { steed_id: steed, saddle_equipped: true });

    let down = sim.level.stairs_down;
    sim.arena.actors.get_mut(sim.player_id).unwrap().coord = down;
    sim.step_player_action(ActionAst::Descend);

    assert!(sim.arena.actors.contains_key(steed));
    assert_eq!(sim.arena.actors.get(steed).unwrap().coord, sim.arena.actors.get(sim.player_id).unwrap().coord);
}

#[test]
fn quiver_cleared_when_item_left_behind() {
    let mut sim = SimulationWorld::new_with_seed(326);
    let spot = sim.level.stairs_down;
    let arrow = sim.arena.spawn_item(create_item_record(ItemKindId::Dagger, ItemLocation::Floor(spot), Buc::Uncursed));
    sim.hero.quivered_item = Some(arrow);
    sim.arena.actors.get_mut(sim.player_id).unwrap().coord = spot;
    sim.step_player_action(ActionAst::Descend);
    assert_eq!(sim.hero.quivered_item, None);
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p nethacked-sim --test level_persistence_tests`
Expected: assertion FAILs in the nested-container, monster-inventory, unpaid-ledger, steed and quiver tests (the tests do not touch `StoredLevel` directly, so they compile).

- [ ] **Step 3: Implement pack**

```rust
    pub(crate) fn pack_current_level(&mut self) {
        let from_key = (self.current_branch, self.depth);
        let steed_id = self.hero.mount.as_ref().map(|m| m.steed_id);

        // Quiver must refer to something the hero carries.
        if let Some(q) = self.hero.quivered_item {
            let carried = self.arena.items.get(q).map(|it| it.location == ItemLocation::CarriedBy(self.player_id)).unwrap_or(false);
            if !carried {
                self.hero.quivered_item = None;
            }
        }

        let monster_ids: Vec<ActorId> = self.arena.actors.iter()
            .filter(|(id, _)| *id != self.player_id && Some(*id) != steed_id)
            .map(|(id, _)| id)
            .collect();

        // Items belonging to the level: floor items, items carried by packed monsters, and
        // everything transitively inside those.
        let mut item_ids: Vec<ItemId> = self.arena.items.iter()
            .filter(|(_, it)| match it.location {
                ItemLocation::Floor(_) => true,
                ItemLocation::CarriedBy(a) => monster_ids.contains(&a),
                _ => false,
            })
            .map(|(id, _)| id)
            .collect();
        let mut i = 0;
        while i < item_ids.len() {
            let children = self.arena.items_in_container(item_ids[i]);
            item_ids.extend(children);
            i += 1;
        }

        let mut monsters = Vec::with_capacity(monster_ids.len());
        for mid in monster_ids {
            if let Some(actor) = self.arena.destroy_actor(mid) {
                monsters.push((mid, actor));
            }
        }
        let mut items = Vec::with_capacity(item_ids.len());
        for iid in &item_ids {
            if let Some(item) = self.arena.destroy_item(*iid) {
                items.push((*iid, item));
            }
        }

        let (level_unpaid, hero_unpaid): (Vec<_>, Vec<_>) = std::mem::take(&mut self.unpaid_items)
            .into_iter()
            .partition(|(iid, _)| item_ids.contains(iid));
        self.unpaid_items = hero_unpaid;

        let stored_current = StoredLevel {
            level: self.level.clone(),
            monsters,
            items,
            unpaid_items: level_unpaid,
        };

        if let Some(pos) = self.stored_levels.iter().position(|(k, _)| *k == from_key) {
            self.stored_levels[pos] = (from_key, stored_current);
        } else {
            self.stored_levels.push((from_key, stored_current));
        }
    }
```

- [ ] **Step 4: Implement unpack (restore branch)**

```rust
        if let Some(pos) = self.stored_levels.iter().position(|(k, _)| *k == target_key) {
            let (_, stored) = self.stored_levels.remove(pos);
            self.level = stored.level;

            let mut actor_map = std::collections::HashMap::new();
            for (old, m) in stored.monsters {
                actor_map.insert(old, self.arena.spawn_actor(m));
            }
            let mut item_map = std::collections::HashMap::new();
            let mut spawned = Vec::with_capacity(stored.items.len());
            for (old, it) in stored.items {
                let new = self.arena.spawn_item(it);
                item_map.insert(old, new);
                spawned.push(new);
            }
            let fallback = ItemLocation::Floor(self.level.stairs_up);
            for new in spawned {
                if let Some(it) = self.arena.items.get_mut(new) {
                    it.location = match it.location.clone() {
                        ItemLocation::InContainer(old) => item_map.get(&old).map(|n| ItemLocation::InContainer(*n)).unwrap_or(fallback.clone()),
                        ItemLocation::CarriedBy(old) => actor_map.get(&old).map(|n| ItemLocation::CarriedBy(*n)).unwrap_or(fallback.clone()),
                        other => other,
                    };
                }
            }
            for (old, cost) in stored.unpaid_items {
                if let Some(new) = item_map.get(&old) {
                    self.unpaid_items.push((*new, cost));
                }
            }
        } else {
```
(`StoredLevel` field rename: `floor_items` → `items`, `monsters` element type changes. Grep the workspace for `floor_items` and `StoredLevel {` and update any other constructors/readers, e.g. in `bones.rs`, wasm or py crates.)

- [ ] **Step 5: Move steed on arrival**

Add to `stairs.rs`:
```rust
    /// A mounted hero's steed arrives on the same square as the hero.
    pub(crate) fn place_steed_with_hero(&mut self) {
        let Some(steed_id) = self.hero.mount.as_ref().map(|m| m.steed_id) else { return };
        let Some(hero_coord) = self.arena.actors.get(self.player_id).map(|p| p.coord) else { return };
        if let Some(steed) = self.arena.actors.get_mut(steed_id) {
            steed.coord = hero_coord;
        } else {
            self.hero.mount = None;
        }
    }
```
Call `self.place_steed_with_hero();` immediately after every place in `stairs.rs` that sets the player's `coord` following `unpack_or_generate_level` (search for `p.coord = new_coord` and similar assignments after level changes).

- [ ] **Step 6: Run tests**

Run: `cargo test --workspace --exclude nethacked-py 2>&1 | grep -E "test result|FAILED|panicked"`
Expected: PASS, including existing `test_multi_floor_persistence_with_items_and_amulet`.

- [ ] **Step 7: Commit**

```bash
git add crates/nethacked-sim
git commit -m "fix(sim): keep container, inventory, ledger, steed and quiver references valid across level changes"
```

---

### Task 15: Dungeon reachability helpers, layout fixes and seed sweeps

**Files:**
- Create: `crates/nethacked-dungeon/src/reach.rs`
- Modify: `crates/nethacked-dungeon/src/lib.rs` (`pub mod reach; pub use reach::{find_free_floor, reachable_from, reachable_from_with};`)
- Modify: `crates/nethacked-dungeon/src/generator.rs:125-149` (`validate_stair_connectivity` on top of `reachable_from`)
- Modify: `crates/nethacked-dungeon/src/sokoban.rs:37-40`
- Modify: `crates/nethacked-dungeon/src/mines.rs` (Minetown doors/corridors; watchman coord)
- Modify: `crates/nethacked-dungeon/src/quest.rs:143-150`
- Modify: `crates/nethacked-dungeon/src/gehennom.rs` (lava check-and-revert; sanctum up stairs)
- Test: `crates/nethacked-dungeon/tests/reachability.rs` (new)

**Interfaces:**
- Produces:
  - `pub fn reachable_from(level: &DungeonLevel, start: Coord) -> HashSet<Coord>` (8-connected, `Tile::is_passable`)
  - `pub fn reachable_from_with(level: &DungeonLevel, start: Coord, passable: impl Fn(&Tile) -> bool) -> HashSet<Coord>`
  - `pub fn find_free_floor(level: &DungeonLevel, rect: &Rect, avoid: &[Coord]) -> Vec<Coord>` — interior `Tile::Room` tiles, row-major (y outer, x inner), excluding `level.stairs_up`, `level.stairs_down`, and `avoid`

- [ ] **Step 1: Write failing tests**

Create `crates/nethacked-dungeon/tests/reachability.rs`:
```rust
//! Seed sweeps: every generated level's required locations are reachable from the arrival point.

use nethacked_dungeon::*;
use nethacked_types::{Coord, Tile};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

const SEEDS: u64 = 200;

fn rng(seed: u64) -> ChaCha8Rng {
    ChaCha8Rng::seed_from_u64(seed)
}

fn assert_reach(level: &DungeonLevel, targets: &[Coord], ctx: &str) {
    let reach = reachable_from(level, level.stairs_up);
    for t in targets {
        assert!(reach.contains(t), "{}: {:?} unreachable from {:?}", ctx, t, level.stairs_up);
    }
}

#[test]
fn regular_dungeon_levels_connected() {
    for s in 0..SEEDS {
        let l = generate_dungeon_level(&mut rng(s));
        assert_reach(&l, &[l.stairs_down], &format!("dungeon seed {}", s));
    }
}

#[test]
fn sokoban_prize_reachable_once_pits_are_filled() {
    let (l, boulders) = generate_sokoban_level(1);
    let reach = reachable_from_with(&l, l.stairs_up, |t| t.is_passable() || matches!(t, Tile::Pit { .. }));
    assert!(reach.contains(&l.stairs_down), "prize room sealed off");
    assert!(reach.contains(&Coord::new(64, 10).unwrap()));
    let pits = l.tiles.iter().flatten().filter(|t| matches!(t, Tile::Pit { filled: false })).count();
    assert!(boulders.len() >= pits);
}

#[test]
fn minetown_shops_temple_reachable() {
    for s in 0..SEEDS {
        let lay = generate_minetown_level(&mut rng(s));
        let mut targets = vec![lay.level.stairs_down, lay.priest_coord, lay.altar_coord];
        targets.extend(lay.shopkeeper_coords.iter().copied());
        targets.extend(lay.watchmen_coords.iter().copied());
        assert_reach(&lay.level, &targets, &format!("minetown seed {}", s));
    }
}

#[test]
fn mines_levels_connected() {
    for s in 0..SEEDS {
        for d in [1usize, 2, 4] {
            let l = generate_mines_cavern_level(&mut rng(s), d);
            assert_reach(&l, &[l.stairs_down], &format!("mines d{} seed {}", d, s));
        }
        let (l, luck) = generate_mines_end_level(&mut rng(s));
        assert_reach(&l, &[luck], &format!("mines end seed {}", s));
    }
}

#[test]
fn quest_levels_connected() {
    for s in 0..SEEDS {
        let l = generate_quest_locate_level(&mut rng(s), 2);
        assert_reach(&l, &[l.stairs_down], &format!("quest locate seed {}", s));
        let home = generate_quest_home_level(&mut rng(s), "valkyrie");
        assert_reach(&home.level, &[home.leader_coord], &format!("quest home seed {}", s));
    }
}

#[test]
fn gehennom_levels_connected() {
    for s in 0..SEEDS {
        let (l, _) = generate_gehennom_maze_level(&mut rng(s), 3, false);
        let mut targets = vec![l.stairs_down];
        targets.extend(l.rooms.iter().map(|r| r.center()));
        assert_reach(&l, &targets, &format!("gehennom seed {}", s));

        let (l, vs) = generate_gehennom_maze_level(&mut rng(s), 5, true);
        let mut targets = vec![vs.unwrap()];
        targets.extend(l.rooms.iter().map(|r| r.center()));
        assert_reach(&l, &targets, &format!("gehennom vs seed {}", s));

        let v = generate_valley_of_the_dead(&mut rng(s));
        assert_reach(&v, &[v.stairs_down], &format!("valley seed {}", s));
    }
}

#[test]
fn sanctum_has_up_stairs_and_reachable_altar() {
    let (l, spawn) = generate_moloch_sanctum_level(&mut rng(1));
    assert!(matches!(l.get_tile(spawn), Tile::Stairs { up: true }));
    assert_reach(&l, &[l.stairs_down], "sanctum");
}

#[test]
fn find_free_floor_skips_stairs_and_avoid() {
    let l = generate_dungeon_level(&mut rng(3));
    let room = l.rooms[0];
    let avoid = [room.center()];
    let free = find_free_floor(&l, &room.rect, &avoid);
    assert!(!free.is_empty());
    for c in &free {
        assert_eq!(*l.get_tile(*c), Tile::Room);
        assert!(*c != l.stairs_up && *c != l.stairs_down && *c != room.center());
        assert!(room.contains_inner(*c));
    }
}
```
(`rand_chacha = "0.9"` is already a dev-dependency of nethacked-dungeon.)

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p nethacked-dungeon --test reachability`
Expected: compile failure (helpers missing), then failures for sokoban, minetown, quest locate (some seeds), gehennom (lava on corridor, some seeds), sanctum.

- [ ] **Step 3: Implement `reach.rs`**

```rust
//! Reachability analysis and free-floor placement helpers shared by generators and the sim.

use std::collections::{HashSet, VecDeque};
use nethacked_types::{Coord, Tile};

use crate::level::DungeonLevel;
use crate::room::Rect;

/// All coordinates reachable from `start` by 8-way moves over passable tiles.
pub fn reachable_from(level: &DungeonLevel, start: Coord) -> HashSet<Coord> {
    reachable_from_with(level, start, |t| t.is_passable())
}

/// Like [`reachable_from`] but with a caller-supplied passability rule.
pub fn reachable_from_with(level: &DungeonLevel, start: Coord, passable: impl Fn(&Tile) -> bool) -> HashSet<Coord> {
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    visited.insert(start);
    queue.push_back(start);
    while let Some(current) = queue.pop_front() {
        for n in current.neighbors() {
            if !visited.contains(&n) && passable(level.get_tile(n)) {
                visited.insert(n);
                queue.push_back(n);
            }
        }
    }
    visited
}

/// Interior floor tiles of `rect` in row-major order, excluding stairs and `avoid`.
pub fn find_free_floor(level: &DungeonLevel, rect: &Rect, avoid: &[Coord]) -> Vec<Coord> {
    let mut out = Vec::new();
    for y in (rect.y1 + 1)..rect.y2 {
        for x in (rect.x1 + 1)..rect.x2 {
            let c = Coord::new_unchecked(x, y);
            if *level.get_tile(c) == Tile::Room && c != level.stairs_up && c != level.stairs_down && !avoid.contains(&c) {
                out.push(c);
            }
        }
    }
    out
}
```
`validate_stair_connectivity(level)` becomes `level.stairs_up == level.stairs_down || reachable_from(level, level.stairs_up).contains(&level.stairs_down)`; remove now-unused imports.

- [ ] **Step 4: Layout fixes**

- Sokoban (`sokoban.rs`): replace the second doorway with the shared wall tile: `level.set_tile(Coord::new_unchecked(60, 10), Tile::Room);` (keep `(11, 10)`).
- Minetown (`mines.rs`): plaza is `Rect::new(28, 7, 24, 8)` (walls x=28, x=52, y=7, y=15). Replace the shop/temple doorway + corridor block with:
  ```rust
  // General Store door (east wall) -> corridor -> plaza west wall
  level.set_tile(Coord::new_unchecked(22, 8), Tile::Room);
  // Delicatessen door (east wall) -> corridor -> plaza west wall
  level.set_tile(Coord::new_unchecked(22, 13), Tile::Room);
  // Temple door (west wall) -> corridor -> plaza east wall
  level.set_tile(Coord::new_unchecked(56, 9), Tile::Room);

  carve_h_corr(&mut level, 22, 28, 8);
  carve_h_corr(&mut level, 22, 28, 13);
  carve_h_corr(&mut level, 52, 56, 9);
  ```
  (corridor carving overwrites the wall tiles at both ends, forming the doorways). Remove the old `(56,7)`, `(22,6)`, `(22,15)` doorways and the y=6/15/7 corridors. Change the third watchman coord from `(54, 7)` to `(54, 9)`.
- Quest locate (`quest.rs`): pit placement with check-and-revert:
  ```rust
    for _ in 0..5 {
        let px = rng.random_range(10..COLNO - 10);
        let py = rng.random_range(3..ROWNO - 3);
        let c = Coord::new_unchecked(px, py);
        if *level.get_tile(c) == Tile::Corr {
            level.set_tile(c, Tile::Pit { filled: false });
            if !crate::reach::reachable_from(&level, level.stairs_up).contains(&level.stairs_down) {
                level.set_tile(c, Tile::Corr);
            }
        }
    }
  ```
  (RNG draws are unchanged, so generation stays deterministic per seed.)
- Gehennom maze (`gehennom.rs`): move the lava loop **after** stairs/vibrating-square placement and make it check-and-revert:
  ```rust
    let mut keep: Vec<Coord> = rooms.iter().map(|r| r.center()).collect();
    keep.push(level.stairs_down);
    for r in &rooms {
        let c = Coord::new_unchecked(r.x1 + 1, r.y1 + 1);
        if c == level.stairs_up || c == level.stairs_down || Some(c) == vibrating_square || *level.get_tile(c) != Tile::Room {
            continue;
        }
        level.set_tile(c, Tile::Lava);
        let reach = crate::reach::reachable_from(&level, level.stairs_up);
        if !keep.iter().all(|k| reach.contains(k)) {
            level.set_tile(c, Tile::Room);
        }
    }
  ```
  (`level.rooms = rooms;` stays after this loop; borrow `rooms` immutably here.)
- Sanctum (`generate_moloch_sanctum_level`): after computing `entrance_spawn`: `level.set_tile(entrance_spawn, Tile::Stairs { up: true });`.

If any sweep still fails for some seed after these fixes, diagnose that seed (print the level) and fix the generator with the same check-and-revert pattern; do not reduce `SEEDS`.

- [ ] **Step 5: Run tests**

Run: `cargo test -p nethacked-dungeon && cargo test --workspace --exclude nethacked-py 2>&1 | grep -E "test result|FAILED|panicked"`
Expected: PASS (existing `test_gehennom_maze_with_vibrating_square` still asserts the VS tile is `Room`).

- [ ] **Step 6: Commit**

```bash
git add crates/nethacked-dungeon
git commit -m "fix(dungeon): reachability helpers; Sokoban, Minetown, Quest, Gehennom and Sanctum layouts connected"
```

---

### Task 16: Sim placement — monsters avoid stairs/occupied tiles, invocation items on free floor, Mysterious Force clamp

**Files:**
- Modify: `crates/nethacked-sim/src/actions/stairs.rs` (spawn loops in `unpack_or_generate_level`, Gehennom depth-5 item placement, mysterious force)
- Test: `crates/nethacked-sim/tests/reachability_tests.rs` (new)

**Interfaces:**
- Consumes: `nethacked_dungeon::{find_free_floor, reachable_from}` (Task 15).
- Produces:
  - `pub(crate) fn spawn_monster_near(&mut self, species: MonsterSpeciesId, preferred: Coord) -> Option<ActorId>`
  - `pub const SANCTUM_DEPTH: usize = 6;` in `stairs.rs` (re-export not required)
  - `pub fn clamp_mysterious_force(pushed: usize, sanctum_open: bool) -> usize` (pub, in `stairs.rs`, re-exported from `nethacked_sim` for tests): returns `pushed` if `sanctum_open`, else `pushed.min(SANCTUM_DEPTH - 1)`

- [ ] **Step 1: Write failing tests**

Create `crates/nethacked-sim/tests/reachability_tests.rs`:
```rust
//! Spawned key items and monsters on generated levels are reachable and never block stairs.

use nethacked_arena::ItemLocation;
use nethacked_dungeon::reachable_from;
use nethacked_sim::{clamp_mysterious_force, SimulationWorld};
use nethacked_types::BranchId;

fn floor_coords_named(sim: &SimulationWorld, name: &str) -> Vec<nethacked_sim::Coord> {
    sim.arena.items.values().filter(|it| it.name == name).filter_map(|it| match it.location {
        ItemLocation::Floor(c) => Some(c),
        _ => None,
    }).collect()
}

#[test]
fn invocation_items_reachable_on_every_seed() {
    for seed in 0..100u64 {
        let mut sim = SimulationWorld::new_with_seed(seed);
        sim.current_branch = BranchId::Gehennom;
        sim.depth = 5;
        sim.unpack_or_generate_level(BranchId::Gehennom, 5);
        let reach = reachable_from(&sim.level, sim.level.stairs_up);
        let candles = floor_coords_named(&sim, "wax candle");
        assert_eq!(candles.len(), 7, "seed {}", seed);
        for name in ["wax candle", "Candelabrum of Invocation", "Book of the Dead"] {
            for c in floor_coords_named(&sim, name) {
                assert!(reach.contains(&c), "seed {}: {} at {:?} unreachable", seed, name, c);
            }
        }
        assert!(reach.contains(&sim.vibrating_square.unwrap()), "seed {}", seed);
    }
}

#[test]
fn spawned_monsters_never_on_stairs_or_stacked() {
    let cases = [(BranchId::GnomishMines, 1usize), (BranchId::GnomishMines, 5), (BranchId::Gehennom, 1), (BranchId::Gehennom, 3), (BranchId::Gehennom, 5), (BranchId::Quest, 2)];
    for seed in 0..50u64 {
        for (branch, depth) in cases {
            let mut sim = SimulationWorld::new_with_seed(seed);
            let pre: Vec<_> = sim.arena.actors.keys().collect();
            sim.current_branch = branch;
            sim.depth = depth;
            sim.unpack_or_generate_level(branch, depth);
            let mut seen = std::collections::HashSet::new();
            for (id, a) in sim.arena.actors.iter() {
                if pre.contains(&id) {
                    continue;
                }
                assert!(a.coord != sim.level.stairs_up && a.coord != sim.level.stairs_down, "{:?} d{} seed {}: {} on stairs", branch, depth, seed, a.name);
                assert!(seen.insert(a.coord), "{:?} d{} seed {}: stacked at {:?}", branch, depth, seed, a.coord);
            }
        }
    }
}

#[test]
fn mysterious_force_never_reaches_sanctum_without_invocation() {
    for depth in 1..=5usize {
        for roll in 0..300u32 {
            if let Some(pushed) = nethacked_core::calculate_mysterious_force(depth, roll) {
                assert!(clamp_mysterious_force(pushed, false) <= 5);
                assert_eq!(clamp_mysterious_force(pushed, true), pushed);
            }
        }
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p nethacked-sim --test reachability_tests`
Expected: compile failure (`clamp_mysterious_force`), then candle/book reachability and stairs-occupancy failures.

- [ ] **Step 3: Implement `spawn_monster_near`**

```rust
    /// Spawn a monster at or near `preferred` on a passable, unoccupied, non-stairs tile
    /// (searching outward up to radius 3 in row-major ring order). Returns `None` if no spot.
    pub(crate) fn spawn_monster_near(&mut self, species: MonsterSpeciesId, preferred: Coord) -> Option<ActorId> {
        for radius in 0..=3isize {
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    if dx.abs().max(dy.abs()) != radius {
                        continue;
                    }
                    let (x, y) = (preferred.x as isize + dx, preferred.y as isize + dy);
                    if x < 0 || y < 0 {
                        continue;
                    }
                    let Some(c) = Coord::new(x as usize, y as usize) else { continue };
                    if self.level.is_passable(c)
                        && c != self.level.stairs_up
                        && c != self.level.stairs_down
                        && self.actor_at(c).is_none()
                    {
                        return Some(self.arena.spawn_actor(create_monster_record(species, c)));
                    }
                }
            }
        }
        None
    }
```
Replace every generated-level spawn of the form
```rust
let mon = create_monster_record(species, room.center());
self.arena.spawn_actor(mon);
```
inside `unpack_or_generate_level` with `self.spawn_monster_near(species, center);`. Because `self.level.rooms.iter()` borrows `self`, first collect centers: `let centers: Vec<Coord> = self.level.rooms.iter().map(|r| r.center()).collect();` and loop over `centers.iter().enumerate()`. Apply the same to priest/watchmen/shopkeeper/quest leader/guardian/nemesis spawns that use fixed coordinates (preserve `is_tame` tweaks by setting them on the returned id via `self.arena.actors.get_mut(id)`).

- [ ] **Step 4: Invocation item placement**

Replace the Gehennom depth-5 Candelabrum/Book/candle placement with: free floor of `rooms[1]` first, then free floor of every other room in index order (skipping duplicates), never on the vibrating square:
```rust
                    let vs_avoid: Vec<Coord> = self.vibrating_square.into_iter().collect();
                    let rects: Vec<nethacked_dungeon::Rect> = self.level.rooms.iter().map(|r| r.rect).collect();
                    let mut order: Vec<Coord> = nethacked_dungeon::find_free_floor(&self.level, &rects[1], &vs_avoid);
                    for (i, rect) in rects.iter().enumerate() {
                        if i == 1 {
                            continue;
                        }
                        for c in nethacked_dungeon::find_free_floor(&self.level, rect, &vs_avoid) {
                            if !order.contains(&c) {
                                order.push(c);
                            }
                        }
                    }
                    let mut spots = order.into_iter();
                    if let Some(c) = spots.next() {
                        self.arena.spawn_item(create_item_record(ItemKindId::CandelabrumOfInvocation, ItemLocation::Floor(c), Buc::Uncursed));
                    }
                    if let Some(c) = spots.next() {
                        self.arena.spawn_item(create_item_record(ItemKindId::BookOfTheDead, ItemLocation::Floor(c), Buc::Blessed));
                    }
                    for c in spots.take(7) {
                        self.arena.spawn_item(create_item_record(ItemKindId::WaxCandle, ItemLocation::Floor(c), Buc::Uncursed));
                    }
```

- [ ] **Step 5: Mysterious Force clamp**

```rust
pub const SANCTUM_DEPTH: usize = 6;

/// Without the completed invocation, the push can never land on Moloch's Sanctum.
pub fn clamp_mysterious_force(pushed: usize, sanctum_open: bool) -> usize {
    if sanctum_open { pushed } else { pushed.min(SANCTUM_DEPTH - 1) }
}
```
In the ascend handler:
```rust
                        if let Some(pushed_depth) = nethacked_core::calculate_mysterious_force(self.depth, roll) {
                            let pushed_depth = clamp_mysterious_force(pushed_depth, nethacked_core::is_sanctum_accessible(self.ritual_progress));
                            if pushed_depth != self.depth {
                                /* existing push body */
                            }
                        }
```
(If clamped to the current depth, fall through to the normal ascend.) Use `SANCTUM_DEPTH` in the `(BranchId::Gehennom, 6)` match arm (`(BranchId::Gehennom, SANCTUM_DEPTH)`). Re-export `clamp_mysterious_force` from `nethacked-sim/src/lib.rs` (`pub use actions::stairs::{clamp_mysterious_force, SANCTUM_DEPTH};`).

- [ ] **Step 6: Run tests**

Run: `cargo test --workspace --exclude nethacked-py 2>&1 | grep -E "test result|FAILED|panicked"`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/nethacked-sim
git commit -m "fix(sim): safe monster placement, reachable invocation items, Mysterious Force cannot skip the invocation"
```

---

### Task 17: Final verification

- [ ] **Step 1: Full build and tests**

Run:
```bash
cargo build --workspace --all-targets 2>&1 | tail -3
cargo test --workspace --exclude nethacked-py 2>&1 | grep -E "test result|FAILED|panicked"
```
Expected: build OK; every `test result: ok`; zero `FAILED`.

- [ ] **Step 2: No new clippy warnings in touched crates**

Run: `cargo clippy --workspace --all-targets 2>&1 | grep -c "^warning"` and compare with the baseline count recorded before Task 1 (`git stash`-free method: run on `main` via `git worktree add /tmp/nr-base main` if needed). New warnings introduced by this branch must be fixed.

- [ ] **Step 3: Spot-run the binaries**

Run:
```bash
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"ping"}' '{bad' '{"jsonrpc":"2.0","method":"notifications/initialized"}' | cargo run -q -p nethacked-agent --bin nethacked-mcp
```
Expected: two lines — `{"id":1,"jsonrpc":"2.0","result":{}}` (key order may vary) and a `-32700` error.

- [ ] **Step 4: Commit any fixups**

```bash
git status --short
git commit -am "chore: verification fixups" # only if there are changes
```
