# Hardening, Sim Integrity & Level Reachability — Design

Date: 2026-10-04
Status: Design approved in conversation; written spec awaiting review
Source: full-repo review of 2026-10-04 (groups A, B, C)

## Goal

Fix the critical/high findings from the repo review that are (A) remotely triggerable
crashes and server exposure, (B) simulation-integrity bugs that corrupt or lose game state,
and (C) special levels whose required locations are unreachable in real play.

## Non-goals

- NetHack fidelity of formulas and data tables, Lean spec/proof changes (group D).
- LICENSE, README/doc truthing, CI changes, `cargo fmt` sweep (group E).
- TUI / WASM / Python RL / i18n fixes (group F).
- Pets following the hero across levels (except the steed).
- Per-client GraphQL sessions.
- Changing `GET /api/v1/bones/:depth` pop semantics.
- Backward compatibility with previously serialized `SimulationWorld` saves (they cannot
  round-trip today).

## Constraints

- Work on branch `fix/review-hardening-integrity`, never `main`.
- TDD: every fix lands with a test that fails before the fix.
- `cargo test --workspace --exclude netrust-py` stays green; `cargo build --workspace --all-targets` builds.
- Match surrounding code style; no new crates except where stated.

---

## A. Hardening

### A1. Validated `Coord` deserialization (`netrust-types`)

`Coord` gains `#[serde(try_from = "RawCoord")]` where `RawCoord { x: usize, y: usize }`
and `TryFrom` delegates to `Coord::new` (which bounds-checks against `COLNO`/`ROWNO`).
Serialization is unchanged (`{x, y}`). Any untrusted JSON (bones, saves, protocol input)
can no longer produce an out-of-range `Coord`.

### A2. `AgentSession::inspect_tile` bounds

`inspect_tile` takes raw `(x, y)` (or a `Coord` built via `Coord::new`) and returns
`Result<TileInspection, String>`; callers in MCP, JSON-RPC and GraphQL map the error to their
protocol error. No caller uses `Coord::new_unchecked` on external input.

### A3. Bones server (`netrust-agent/src/bones/`)

- `render_headstone` truncates and centers by `chars()` count, never byte slicing.
- Handlers compute everything that can fail (validation, headstone rendering) **before** taking
  the lock; locking goes through a helper that recovers a poisoned mutex
  (`lock().unwrap_or_else(PoisonError::into_inner)`).
- Validation → `422 Unprocessable Entity` with a JSON error body:
  `hero_name` 1..=32 chars, `killer` ≤ 64 chars, `items.len()` ≤ 64, `depth` 1..=60.
  (`death_coord` is covered by A1 — malformed coords fail JSON extraction.)
- Caps: at most 16 bones per depth (new submissions beyond that are rejected with 409);
  at most 1000 graves (oldest evicted).
- Bind: default `127.0.0.1:<port>`; overridden by `--bind <addr>` or `NETRUST_BIND`.
- Auth: if env `NETRUST_TOKEN` is set and non-empty, `POST /api/v1/bones` and
  `POST /api/v1/reset` require `Authorization: Bearer <token>`, else `401`.
  Read-only routes stay open.

### A4. Bones client (`netrust-agent/src/bones/client.rs`)

- Path segments (e.g. hero name) are percent-encoded (RFC 3986 unreserved set kept).
- A base URL with `https://` returns `Err("https not supported")` instead of silently using
  plaintext.
- Response `Content-Length` capped at 4 MiB; status/header lines capped at 8 KiB; exceeding
  either returns `Err`.

### A5. MCP server (`netrust-agent/src/mcp.rs`, `bin/mcp.rs`)

- Request handling returns a proper JSON-RPC 2.0 response for every request that has an `id`:
  - unparseable JSON → `-32700` with `id: null`
  - not an object / missing or non-string `method` → `-32600`
  - unknown method → `-32601`
  - missing/invalid params, unknown tool, unknown action, out-of-range coords → `-32602`
- Messages without `id` (notifications) never get a response.
- `ping` returns `{}`.
- `netrust_step` schema enum lists exactly the actions the handler accepts (including
  `descend`, `ascend`, `eat`, `cast`); description no longer mentions nonexistent actions.
  Unknown actions are `-32602`, never silently `Wait`.
- Kick targets use checked stepping (`Coord::step`-style, returning `None` off-map → `-32602`).
- The stdin loop reads raw bytes per line; invalid UTF-8 yields `-32700` and the loop continues.
- `bin/mcp.rs` calls the library's `run_mcp_server` instead of duplicating it.

### A6. JSON-RPC server (`netrust-agent/src/jsonrpc.rs`, `bin/jsonrpc.rs`)

Same robustness rules as A5 (error codes, notifications, invalid UTF-8, bounds-checked
`inspect_tile`). Protocol surface otherwise unchanged.

### A7. GraphQL server (`netrust-agent/src/graphql.rs`, `bin/graphql.rs`)

- Default bind `127.0.0.1`, same `--bind`/`NETRUST_BIND` override as A3.
- Schema built with `limit_depth(16)` and `limit_complexity(2000)`.
- POST handler reads the body via `axum::body::to_bytes(body, 64 * 1024)`; oversize → 413.
- If `NETRUST_TOKEN` is set, a request whose operation is a mutation and lacks the bearer token
  is rejected (resolver-level guard reading an `Authorized(bool)` value inserted into request
  data).

---

## B. Simulation integrity

### B1. Level persistence with ID remapping (`netrust-sim/src/actions/stairs.rs`, `world.rs`)

`StoredLevel` becomes:

```rust
pub struct StoredLevel {
    pub level: DungeonLevel,
    pub monsters: Vec<(ActorId, ActorRecord)>,
    pub items: Vec<(ItemId, ItemRecord)>,
    pub unpaid_items: Vec<(ItemId, u32)>,
}
```

Pack (leaving a level):
1. Steed (if mounted) is not packed — it travels with the hero.
2. Monsters = all non-player actors except the steed.
3. Items = floor items ∪ items carried by packed monsters ∪ transitive contents of any packed
   item (containers). Player inventory and its contents are never packed.
4. The unpaid ledger is partitioned: entries whose item is being packed go to the
   `StoredLevel`; entries for items the hero carries stay in `world.unpaid_items`.
5. `hero.quivered_item` is cleared if it does not refer to an item carried by the player.

Unpack (restoring):
1. Spawn monsters, building `actor_map: old → new`.
2. Spawn every item (location temporarily as stored), building `item_map: old → new`.
3. Rewrite each restored item's location: `InContainer(old)` → `InContainer(item_map[old])`,
   `CarriedBy(old)` → `CarriedBy(actor_map[old])`. Unmapped references become `Floor` at the
   level's `stairs_up` (defensive; should not happen).
4. Remap restored ledger entries through `item_map` and append to `world.unpaid_items`.

### B2. Player damage helper (`netrust-sim`)

`SimulationWorld::damage_player(amount: u32, cause: &str) -> Vec<GameEvent>` does
`hp = hp.saturating_sub(amount)` and sets `is_dead` when `hp == 0`, emitting a `GameEvent::LogMessage` naming
the cause (deaths elsewhere are reported the same way; there is no dedicated death event). Arrow/Dart traps use it.

### B3. Genocide (`netrust-sim/src/actions/items.rs`, `netrust-data`)

- `netrust_data::monster_class_of(name: &str) -> Option<char>` maps bestiary species names
  (case-insensitive) to their NetHack class letter.
- `is_genocided` comparisons are case-insensitive on species name; class checks use
  `monster_class_of(actor.name)`.
- Blessed (class `'L'`) and uncursed (species `"goblin"`) remove only matching actors.
- Removal goes through a helper that drops the actor's carried items to the floor at its coord.

### B4. Wands & wishes (`netrust-sim/src/actions/items.rs`, `netrust-data/src/items.rs`)

- `ZapWand` with no wand in the pack: message "You have no wand to zap.", no time spent,
  no effect.
- `handle_wish` requires a carried wand of wishing with charges > 0; otherwise
  "You have no means of wishing." and no item.
- Wish lookup: normalize (lowercase, strip leading `a`/`an`/`the`/BUC words) and match catalog
  names **exactly**. Wishing for the Amulet of Yendor yields "cheap plastic imitation of the
  Amulet of Yendor" (not the real one).
- `create_item_record` gives wands initial charges in `enchantment`: wishing 1; non-directional
  wands 13; other wands 6.
- Charge arithmetic uses saturating/clamped conversion (no `as i8` wrap).

### B5. Save/load fidelity

- Enable `rand_chacha` `serde` feature in `netrust-sim`; drop `#[serde(skip)]` on `rng`.
- `DungeonLevel.engravings` / `traps` serialize as a sorted `Vec<(Coord, V)>` via a small
  `coord_map` serde module (deterministic order, valid JSON). In-memory type stays `HashMap`.
- Round-trip test: world with traps and engravings → JSON → world; JSON of both equal and
  the next RNG draw is identical.

### B6. Melee randomness (`netrust-sim/src/combat.rs`)

Hero melee rolls `d20 = rng.random_range(1..=20)` and `dmg = rng.random_range(1..=6) + skill_dmg_bonus`
(min 1) from `world.rng`. Damage bonus argument is the skill damage bonus, not the to-hit bonus.
Tests that depended on the fixed roll are adjusted to be deterministic via seed or setup
(e.g. AC that guarantees a hit), not by weakening assertions.

### B7. Time-consuming actions only

Prayer timeout tick and luck decay in `actions/mod.rs` run only when the action consumed time
(energy cost > 0 / the scheduler advanced).

---

## C. Level reachability

### C1. Shared helpers (`netrust-dungeon`)

- `pub fn reachable_from(level: &DungeonLevel, start: Coord) -> HashSet<Coord>` — 8-connected
  BFS over `is_passable` tiles (same movement model as `validate_stair_connectivity`, which is
  reimplemented on top of it).
- `pub fn find_free_floor(level, room: &Room, avoid: &[Coord]) -> Vec<Coord>` — passable
  `Room` tiles inside the room, row-major order, excluding stairs and `avoid`.

### C2. Seed-sweep tests

For 200 seeds each: regular dungeon, Sokoban (depths 1–4 as generated), Minetown,
Mines' End, Quest home/locate/goal, Gehennom maze (with and without vibrating square),
Moloch's Sanctum. Assert `stairs_down` (where present) and all required coordinates
(key items, shopkeepers, priest, vibrating square, sanctum up-stairs) ∈
`reachable_from(stairs_up)`.

Required-coordinate assertions for item/monster placement live in `netrust-sim` tests (they
depend on spawn code); pure layout assertions live in `netrust-dungeon` tests.

### C3. Fixes

- Sokoban: open the hallway↔prize-room doorway on the shared wall (x = 60, y = 10).
- Minetown: corridors from the shops end at a doorway cut into the plaza wall.
- Quest locate: a corridor tile becomes an unfilled pit only if `stairs_down` stays reachable
  afterwards; otherwise it is reverted.
- Gehennom: lava is never placed on a coordinate used for stairs, the vibrating square, or item
  spawns; Candelabrum, Book of the Dead and the 7 candles are placed with `find_free_floor`
  (spilling to further rooms if one room is too small).
- Moloch's Sanctum: gets an up staircase reachable from the arrival point.
- Mysterious Force: the push-down never lands on the Sanctum unless the invocation is complete
  (clamped to the level above).
- Monster spawning in generated levels never uses a stairs tile or an occupied tile
  (skip or shift to next free floor via `find_free_floor`).

## Testing strategy

- Unit tests next to the code for pure functions (headstone, coord serde, percent-encoding,
  reachability helpers, wish normalization).
- `netrust-agent` tests drive `handle_mcp_request` / JSON-RPC handler with malformed input
  and assert response codes; bones server tested via its router (`tower::ServiceExt::oneshot`
  if available, else the existing in-process server test pattern).
- `netrust-sim/tests/` integration tests for level round-trips (container contents, unpaid
  ledger, steed), genocide, wands/wishes, save/load, melee variance, free-action timers,
  reachability of spawned key items.

## Risks

- Melee randomness changes outcomes of existing scripted tests/demo; mitigated by fixing
  tests via setup, not assertions.
- `Coord` validated deserialization could reject previously accepted (invalid) data — intended.
- GraphQL mutation guard relies on async-graphql 7.0.2 `Guard`/context data APIs.
