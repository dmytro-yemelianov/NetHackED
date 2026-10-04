# Frontend Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One shared action/character parser for every frontend, plus the TUI, WASM/web, Python RL and i18n fixes from the repo review.

**Architecture:** A new `netrust_agent::commands` module owns the action vocabulary and character parsing; MCP, JSON-RPC (lib + bin), GraphQL and WASM delegate to it and return errors for unknown input. The TUI gets a pure, testable key handler; WASM gets a chunked tournament API and a panic hook; Python gets a real `gymnasium.Env`; i18n gets full name coverage enforced by tests.

**Tech Stack:** Rust 1.88.0 (pinned), crossterm 0.28, wasm-bindgen 0.2, pyo3 0.23 (abi3-py310), Python 3.13 via `uv` venv, gymnasium, numpy.

**Spec:** `docs/superpowers/specs/2026-10-04-frontend-fixes-design.md`

## Global Constraints

- Branch: `feat/frontend-fixes` (stacked on `chore/repo-hygiene`). Never commit to `main`.
- Every Rust change keeps green: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --exclude netrust-py --locked`, `cargo build -p netrust-wasm --target wasm32-unknown-unknown --locked`, `bash scripts/check-doc-links.sh`.
- TDD: write the failing test first, see it fail, then implement.
- New dependencies allowed ONLY: `console_error_panic_hook = "0.1"` (netrust-wasm), `netrust-data` as dev-dependency of `netrust-i18n`.
- `ZAP_ENERGY = 6`; `MAX_WISH_LEN = 128`; inventory page size 20.
- Every action name accepted before this change by MCP `STEP_ACTIONS`, JSON-RPC `netrust.step`, `bin/jsonrpc.rs`, GraphQL `stepAction`, and WASM `step` must still parse.
- Python: use a venv at `/private/tmp/claude-501/-Users-dmytro-github-NetRust/a8f949ef-7f59-4320-b57a-60104d9bff6a/scratchpad/venv` created with `uv venv --python 3.13` and `uv pip install gymnasium numpy maturin`; build bindings with `maturin develop -m crates/netrust-py/Cargo.toml` inside the venv; run tests with `python -m unittest discover python/tests`.
- Commit messages end with:
  ```
  Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_019cXhwXQqEEChd362EQ6RKp
  ```

## Rulings recorded at planning time

- `quiver` is NOT in the shared vocabulary: `ActionAst::Quiver(SlotId)` takes an item id, not an index, and no string parser accepts it today. (Spec table listed it; corrected here.)
- WASM-only actions `dip` (`"idx:holy|unholy|plain"` text) and `donate` (amount via `index`) are added to the shared vocabulary so WASM keeps working.
- WASM `step` keeps returning `String`; on parse error it returns `{"error":"<message>"}` JSON, which `web/index.html` checks before parsing an observation.

## Review Focus

1. Mixed-case action names and directions (`"Move_North"`, `"NE"`) — case-insensitive acceptance; vi-keys only lowercase single letters. Test in Task 1.
2. `index` given as a JSON string (`"3"`) vs number in MCP/GraphQL args — both accepted where the old parser accepted numbers only? Old parsers used `as_u64`; keep numbers only and return an error for strings. Test in Task 2.
3. TUI quit prompt answered with Ukrainian `н` (n) / `т`→? — prompt accepts `y`/`Y` and the Ukrainian layout key on the `y` position (`н`) after `map_ukrainian_key`. Test in Task 4.
4. Chunked tournament with `step(0)` or after completion — `step(0)` makes no progress and returns current done-state; calling after done is a no-op returning `true`. Test in Task 3.
5. Wish text exactly `MAX_WISH_LEN` chars accepted, `MAX_WISH_LEN + 1` rejected; multibyte chars counted as chars. Test in Task 1.

---

### Task 1: Shared command module

**Files:**
- Create: `crates/netrust-agent/src/commands.rs`
- Modify: `crates/netrust-agent/src/lib.rs` (`pub mod commands;` + re-export `parse_action`, `parse_character`, `ActionArgs`, `ZAP_ENERGY`)
- Modify: `crates/netrust-agent/src/rpc.rs` (move `parse_direction` into `commands`, keep `pub use crate::commands::parse_direction;` in rpc)
- Test: unit tests in `commands.rs`

**Interfaces:**
- Produces:
  - `pub const ZAP_ENERGY: u32 = 6; pub const MAX_WISH_LEN: usize = 128;`
  - `pub struct ActionArgs { pub index: Option<usize>, pub direction: Option<String>, pub target: Option<(usize, usize)>, pub text: Option<String>, pub player: Option<Coord> }` (derive `Debug, Default, Clone`)
  - `pub fn parse_direction(s: &str) -> Option<Direction>` (8 compass names + vi-keys, case-insensitive for names, `k/j/l/h/u/y/n/b` lowercase)
  - `pub fn parse_action(name: &str, args: &ActionArgs) -> Result<ActionAst, String>`
  - `pub fn parse_character(name: Option<&str>, role: Option<&str>, race: Option<&str>, gender: Option<&str>, alignment: Option<&str>) -> Result<CharacterConfig, String>`

- [ ] **Step 1: Write failing tests** (in `commands.rs` `#[cfg(test)] mod tests`, with the module's functions declared as `todo!()`-free stubs is NOT allowed — create the file with only the tests and `use super::*;`, so it fails to compile)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use netrust_sim::{ActionAst, Coord, Direction};

    fn a() -> ActionArgs { ActionArgs::default() }
    fn with_dir(d: &str) -> ActionArgs { ActionArgs { direction: Some(d.into()), ..a() } }
    fn with_idx(i: usize) -> ActionArgs { ActionArgs { index: Some(i), ..a() } }
    fn at(x: usize, y: usize) -> ActionArgs { ActionArgs { player: Coord::new(x, y), ..a() } }

    #[test]
    fn moves_in_all_spellings() {
        assert_eq!(parse_action("move_north", &a()).unwrap(), ActionAst::Move(Direction::North));
        assert_eq!(parse_action("Move_NorthEast", &a()).unwrap(), ActionAst::Move(Direction::NorthEast));
        assert_eq!(parse_action("move", &with_dir("sw")).unwrap_err().contains("direction"), true);
        assert_eq!(parse_action("move", &with_dir("southwest")).unwrap(), ActionAst::Move(Direction::SouthWest));
        assert_eq!(parse_action("north", &a()).unwrap(), ActionAst::Move(Direction::North));
        assert_eq!(parse_action("k", &a()).unwrap(), ActionAst::Move(Direction::North));
        assert!(parse_action("move", &a()).is_err());
    }

    #[test]
    fn indexed_actions_default_to_zero() {
        assert_eq!(parse_action("drop", &with_idx(3)).unwrap(), ActionAst::Drop(3));
        assert_eq!(parse_action("quaff", &a()).unwrap(), ActionAst::Quaff(0));
        for (n, v) in [("wield", ActionAst::Wield(2)), ("read", ActionAst::Read(2)), ("eat", ActionAst::Eat(2)),
                       ("sacrifice", ActionAst::Sacrifice(2)), ("apply", ActionAst::Apply(2)), ("light", ActionAst::Apply(2)),
                       ("price_check", ActionAst::PriceCheck(2)), ("appraise", ActionAst::PriceCheck(2)), ("rub", ActionAst::Rub(2)),
                       ("donate", ActionAst::Donate(2))] {
            assert_eq!(parse_action(n, &with_idx(2)).unwrap(), v, "{n}");
        }
    }

    #[test]
    fn directional_actions() {
        assert_eq!(parse_action("cast", &with_dir("west")).unwrap(), ActionAst::Cast { spell_index: 0, dir: Direction::West });
        assert_eq!(parse_action("cast", &a()).unwrap(), ActionAst::Cast { spell_index: 0, dir: Direction::East });
        assert_eq!(parse_action("zap", &with_dir("n")).unwrap(), ActionAst::ZapWand { dir: Direction::SouthEast, energy: ZAP_ENERGY });
        assert_eq!(parse_action("fire", &with_dir("north")).unwrap(), ActionAst::Fire(Direction::North));
        assert!(parse_action("fire", &a()).is_err());
        assert!(parse_action("cast", &with_dir("up")).is_err());
    }

    #[test]
    fn targeted_actions() {
        let p = at(10, 10);
        assert_eq!(parse_action("kick_east", &p).unwrap(), ActionAst::Kick(Coord::new(11, 10).unwrap()));
        assert_eq!(parse_action("kick", &ActionArgs { direction: Some("north".into()), ..p.clone() }).unwrap(), ActionAst::Kick(Coord::new(10, 9).unwrap()));
        assert_eq!(parse_action("open_door", &ActionArgs { target: Some((3, 4)), ..a() }).unwrap(), ActionAst::OpenDoor(Coord::new(3, 4).unwrap()));
        assert_eq!(parse_action("close_door", &ActionArgs { direction: Some("w".into()), ..p.clone() }).unwrap_err().contains("direction"), true);
        assert_eq!(parse_action("untrap", &ActionArgs { direction: Some("west".into()), ..p.clone() }).unwrap(), ActionAst::Untrap(Coord::new(9, 10).unwrap()));
        assert!(parse_action("kick_east", &at(79, 0)).is_err());
        assert!(parse_action("open_door", &ActionArgs { target: Some((500, 4)), ..a() }).is_err());
        assert!(parse_action("kick_east", &a()).is_err(), "needs player position");
    }

    #[test]
    fn text_actions() {
        assert_eq!(parse_action("wish", &ActionArgs { text: Some("long sword".into()), ..a() }).unwrap(), ActionAst::Wish("long sword".into()));
        assert!(parse_action("wish", &a()).is_err());
        let ok = "ж".repeat(MAX_WISH_LEN);
        assert!(parse_action("wish", &ActionArgs { text: Some(ok), ..a() }).is_ok());
        let long = "ж".repeat(MAX_WISH_LEN + 1);
        assert!(parse_action("wish", &ActionArgs { text: Some(long), ..a() }).is_err());
        assert!(matches!(parse_action("engrave", &a()).unwrap(), ActionAst::Engrave { ref text, .. } if text == "Elbereth"));
        assert!(matches!(parse_action("dip", &ActionArgs { text: Some("2:unholy".into()), ..a() }).unwrap(),
            ActionAst::Dip { item_index: 2, into_water: netrust_types::WaterType::Unholy }));
    }

    #[test]
    fn simple_and_unknown() {
        for (n, v) in [("pickup", ActionAst::PickUp), ("search", ActionAst::Search), ("pay", ActionAst::Pay),
                       ("pray", ActionAst::Pray), ("ascend", ActionAst::Ascend), ("descend", ActionAst::Descend), ("wait", ActionAst::Wait)] {
            assert_eq!(parse_action(n, &a()).unwrap(), v);
        }
        assert!(parse_action("dance", &a()).is_err());
        assert!(parse_action("", &a()).is_err());
    }

    #[test]
    fn every_legacy_name_still_parses() {
        // Names accepted by MCP STEP_ACTIONS, JSON-RPC netrust.step, bin/jsonrpc.rs, GraphQL stepAction, WASM step
        let p = at(10, 10);
        let legacy = [
            "move_north","move_east","move_south","move_west","move_northeast","move_northwest","move_southeast","move_southwest",
            "wait","pickup","pay","pray","sacrifice","eat","cast","ascend","descend","kick_north","kick_east","kick_south","kick_west",
            "north","east","south","west","k","l","j","h",
            "drop","wield","quaff","read","rub","price_check","appraise","engrave","apply","light","donate",
        ];
        for n in legacy {
            assert!(parse_action(n, &p).is_ok(), "legacy name {n} rejected");
        }
        assert!(parse_action("move", &ActionArgs { direction: Some("north".into()), ..p.clone() }).is_ok());
        assert!(parse_action("open_door", &ActionArgs { target: Some((10, 9)), ..p.clone() }).is_ok());
        assert!(parse_action("zap", &p).is_ok());
        assert!(parse_action("wish", &ActionArgs { text: Some("x".into()), ..p }).is_ok());
    }

    #[test]
    fn character_parsing() {
        let c = parse_character(None, None, None, None, None).unwrap();
        assert_eq!(c.name, "Hero");
        let c = parse_character(Some("Ann"), Some("WIZARD"), Some("elf"), Some("male"), Some("chaotic")).unwrap();
        assert_eq!(c.role, netrust_data::RoleId::Wizard);
        assert_eq!(c.race, netrust_data::RaceId::Elf);
        assert_eq!(c.gender, netrust_data::Gender::Male);
        assert_eq!(c.alignment, netrust_types::Alignment::Chaotic);
        assert!(parse_character(None, Some("samurai"), None, None, None).unwrap_err().contains("samurai"));
        assert!(parse_character(None, None, Some("hobbit"), None, None).is_err());
        assert!(parse_character(None, None, None, Some("other"), None).is_err());
        assert!(parse_character(None, None, None, None, Some("evil")).is_err());
    }
}
```
Note: `zap` with `"n"` maps to SouthEast — vi-key `n` = southeast (NetHack). The `"sw"` short form is NOT a valid direction (only full names and vi-keys), hence the `is_err` expectation via `.unwrap_err()`. `close_door` with `"w"` (not a vi-key) errors. Confirm `RoleId`, `RaceId`, `Gender` re-export paths in `netrust_data` and adjust `use` lines only.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p netrust-agent --lib commands`
Expected: compile errors (functions missing).

- [ ] **Step 3: Implement** `commands.rs`

Requirements (write the code to satisfy the tests exactly):
- `parse_direction`: move the body from `rpc.rs`; names compared lowercase; vi-keys exact lowercase.
- `parse_action`: `let n = name.trim().to_lowercase();` then:
  - `move_<dir>` → `Move`; `move` → requires `args.direction` → `Move`; bare direction names/vi-keys → `Move`.
  - `kick_<dir>` → target `= args.player.ok_or("requires player position")?.step(dir).ok_or("target off the map")?`; `kick`/`open_door`/`close_door`/`untrap` → `args.target` (validated with `Coord::new`, else Err "off the map") or `args.direction` + `args.player` stepping; neither → Err.
  - indexed: `drop wield quaff read eat sacrifice rub apply|light price_check|appraise` → `args.index.unwrap_or(0)`; `donate` → `Donate(args.index.unwrap_or(0) as u32)`.
  - `cast` → `Cast { spell_index: index.unwrap_or(0), dir: compass_or(args, Direction::East)? }`; `zap` → `ZapWand { dir: compass_or(args, East)?, energy: ZAP_ENERGY }`; `fire` → requires direction.
  - `compass_or`: if `args.direction` is `Some(s)` it must parse via `parse_direction` (which only yields compass dirs) else Err mentioning "direction"; `None` → default.
  - `wish` → requires `text`, `text.chars().count() <= MAX_WISH_LEN`.
  - `engrave` → `Engrave { text: text.unwrap_or("Elbereth"), medium: EngravingMedium::Dust(1) }`.
  - `dip` → text `"idx[:water]"` where water ∈ `holy|blessed` (Holy), `unholy|cursed` (Unholy), `plain|uncursed` (Plain), default Holy; missing text → index 0 holy; unparseable index → Err.
  - simple: `pickup search pay pray ascend descend wait`.
  - anything else → `Err(format!("Unknown action '{name}'"))`.
- `parse_character`: case-insensitive match against `ROLES[i].name` / `RACES[i].name`; gender `male|female`; alignment `lawful|neutral|chaotic`; `None` → `CharacterConfig::default()` field values; unknown → `Err(format!("unknown role '{r}'; expected one of: …"))` listing names (same pattern for race, gender, alignment).

- [ ] **Step 4: Run tests** — `cargo test -p netrust-agent --lib commands` → PASS; then the full Global Constraints command set.

- [ ] **Step 5: Commit** — `feat(agent): shared command module for action and character parsing`

---

### Task 2: Route MCP, JSON-RPC (lib + bin), GraphQL and Python zap through `commands`

**Files:**
- Modify: `crates/netrust-agent/src/mcp.rs` (`parse_step_action` → build `ActionArgs` from `args` {index: u64 number only, direction: string, target: x/y numbers, text}, player = current player coord; call `parse_action`; character tool → `parse_character`, Err → `-32602`)
- Modify: `crates/netrust-agent/src/jsonrpc.rs` (`netrust.step`: `action` default `"wait"` kept; args from params; Err → `-32602`)
- Modify: `crates/netrust-agent/src/bin/jsonrpc.rs` (use `parse_action` for its action names; `get_state` stays a special command; Err → existing `{"error": ...}` shape)
- Modify: `crates/netrust-agent/src/graphql.rs` (`step_action` builds `ActionArgs` from `direction`, `target_x/target_y`, `index`, new optional `text: Option<String>` argument; Err → `Err(async_graphql::Error)`; `reset_with_character` → `parse_character`, Err → GraphQL error)
- Modify: `crates/netrust-py/src/lib.rs` (`ZAP_WAND` energy → `netrust_agent::commands::ZAP_ENERGY`)
- Test: `crates/netrust-agent/tests/protocol_tests.rs` (append), `crates/netrust-agent/src/graphql.rs` tests (append)

**Interfaces:**
- Consumes: Task 1 API.
- Produces: no new public API; `STEP_ACTIONS` remains and must be a subset of what `parse_action` accepts.

- [ ] **Step 1: Write failing tests**

Append to `protocol_tests.rs`:
```rust
#[test]
fn mcp_index_must_be_a_number() {
    let mut s = AgentSession::new(42);
    let r = mcp(&mut s, json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"netrust_step","arguments":{"action":"eat","index":"3"}}})).unwrap();
    assert_eq!(r["error"]["code"], INVALID_PARAMS);
}

#[test]
fn mcp_cast_accepts_diagonal_and_rejects_garbage() {
    let mut s = AgentSession::new(42);
    let ok = mcp(&mut s, json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"netrust_step","arguments":{"action":"cast","direction":"northwest"}}})).unwrap();
    assert!(ok.get("result").is_some());
    let bad = mcp(&mut s, json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"netrust_step","arguments":{"action":"cast","direction":"sideways"}}})).unwrap();
    assert_eq!(bad["error"]["code"], INVALID_PARAMS);
}

#[test]
fn mcp_reset_with_unknown_role_is_invalid_params() {
    let mut s = AgentSession::new(42);
    let r = mcp(&mut s, json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"netrust_reset_with_character","arguments":{"role":"samurai"}}})).unwrap();
    assert_eq!(r["error"]["code"], INVALID_PARAMS);
}

#[test]
fn jsonrpc_accepts_move_prefixed_names() {
    let mut s = AgentSession::new(42);
    let r = handle_jsonrpc_request(&mut s, r#"{"jsonrpc":"2.0","id":1,"method":"netrust.step","params":{"action":"move_north"}}"#).unwrap();
    assert!(r.get("result").is_some());
}
```
Append to `graphql.rs` tests:
```rust
    #[tokio::test]
    async fn test_graphql_unknown_action_is_error() {
        let schema = create_schema(AppState { session: Arc::new(Mutex::new(AgentSession::new(42))) });
        let res = schema.execute(r#"mutation { stepAction(action: "dance") { success } }"#).await;
        assert!(!res.errors.is_empty());
        let res = schema.execute(r#"mutation { resetWithCharacter(role: "samurai") { hp } }"#).await;
        assert!(!res.errors.is_empty());
    }
```

- [ ] **Step 2: Run** `cargo test -p netrust-agent` → the new tests FAIL.
- [ ] **Step 3: Implement** the routing described in **Files**. Delete the now-unused local parsers and role/race matches. MCP `STEP_ACTIONS` stays; add a unit test in `mcp.rs` or `protocol_tests.rs` asserting every `STEP_ACTIONS` entry parses with `parse_action` given a player position (the existing `mcp_step_enum_matches_accepted_actions` already covers end-to-end; keep it).
- [ ] **Step 4: Run** the Global Constraints command set → green.
- [ ] **Step 5: Commit** — `refactor(agent): MCP, JSON-RPC and GraphQL use the shared command parser`

---

### Task 3: WASM — shared parser, panic hook, chunked tournament; web page updates

**Files:**
- Modify: `crates/netrust-wasm/Cargo.toml` (`console_error_panic_hook = "0.1"`)
- Modify: `crates/netrust-wasm/src/lib.rs`
- Modify: `crates/netrust-agent/src/arena.rs` only if needed to expose per-seed evaluation for chunking (prefer reusing existing `run_single_game`/summary helpers; add a small `pub fn summarize(results: &[RunResult]) -> BenchmarkReport`-style helper if the one-shot suite has no reusable aggregator)
- Modify: `web/index.html`
- Test: native unit tests in `crates/netrust-wasm/src/lib.rs` (`#[cfg(test)]`, run by `cargo test -p netrust-wasm` on the host)

**Interfaces:**
- Consumes: `netrust_agent::commands::{parse_action, parse_character, ActionArgs}`.
- Produces:
  - `fn action_args_for(action: &str, arg: Option<String>, player: Option<Coord>) -> ActionArgs` (private, host-testable): index-taking actions parse `arg` as `usize` into `index` (non-numeric → leaves `index: None` and returns via `parse_action` an Err only if required — for these actions an unparseable arg must produce Err: store it in `text` and let a pre-check `arg.parse::<usize>()` fail → Err "index must be a number"); `move/cast/zap/fire/kick/open_door/close_door/untrap` put `arg` in `direction`; `wish/engrave/dip` put `arg` in `text`.
  - `step(&mut self, action_str: &str, arg: Option<String>) -> String`: on `Err(e)` returns `serde_json::json!({"error": e}).to_string()` without stepping.
  - `#[wasm_bindgen(start)] pub fn wasm_start() { console_error_panic_hook::set_once(); }` (cfg target wasm32 only for the hook call).
  - `#[wasm_bindgen] pub struct TournamentRun` with `new(num_seeds: u32, max_turns: u32) -> TournamentRun`, `step(&mut self, k: u32) -> bool`, `progress(&self) -> u32` (seeds completed), `total(&self) -> u32`, `report_json(&self) -> String`.
  - `run_tournament_benchmark(num_seeds, max_turns) -> String` becomes `let mut t = TournamentRun::new(..); while !t.step(u32::MAX) {}; t.report_json()`.

- [ ] **Step 1: Write failing tests**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_rejects_unknown_action_without_advancing() {
        let mut s = WasmGameSession::new(7);
        let turn = s.turn();
        let out = s.step("dance", None);
        assert!(out.contains("\"error\""));
        assert_eq!(s.turn(), turn);
    }

    #[test]
    fn cast_and_zap_take_direction() {
        let args = action_args_for("cast", Some("west".into()), None);
        assert_eq!(args.direction.as_deref(), Some("west"));
        let args = action_args_for("drop", Some("2".into()), None);
        assert_eq!(args.index, Some(2));
    }

    #[test]
    fn non_numeric_index_is_error() {
        let mut s = WasmGameSession::new(7);
        assert!(s.step("drop", Some("abc".into())).contains("\"error\""));
    }

    #[test]
    fn chunked_tournament_matches_one_shot() {
        let one = run_tournament_benchmark(3, 30);
        let mut t = TournamentRun::new(3, 30);
        assert!(!t.step(0));
        assert_eq!(t.progress(), 0);
        while !t.step(1) {}
        assert_eq!(t.progress(), 3);
        assert!(t.step(1), "after completion step is a no-op returning true");
        assert_eq!(t.report_json(), one);
    }
}
```
(Use the actual getter/constructor names on `WasmGameSession`; adjust only identifiers.)

- [ ] **Step 2: Run** `cargo test -p netrust-wasm` → FAIL.
- [ ] **Step 3: Implement** per **Interfaces**; delete the old match and the local role/race parsing (use `parse_character`; on Err in `new_with_character`, fall back is NOT allowed — return a `Result<WasmGameSession, JsValue>` from the constructor; update `web/index.html` call site to catch and show the error).
- [ ] **Step 4: web/index.html**
  - Every `session.step(...)` result: `const res = JSON.parse(resStr); if (res.error) { showLog(res.error); return; }` (use the page's existing log/message function).
  - `x` (cast) and zap key handlers pass the last movement direction name (track `lastDir` in the movement handler, default `"east"`).
  - Tournament button: use `TournamentRun` with `while (!run.step(1)) { updateProgress(run.progress(), run.total()); await new Promise(r => setTimeout(r)); }` then render `run.report_json()`.
  - Add `function escapeHtml(s) { return String(s).replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c])); }` and wrap every interpolated game-derived string in `innerHTML` templates (inventory row name/class/buc, `p.name`, `m.policy`, `pol`, `e.message`), or switch to `textContent`.
  - Verify: `grep -n 'innerHTML' web/index.html` and confirm each remaining interpolation uses `escapeHtml`.
- [ ] **Step 5: Run** Global Constraints set (wasm build included) → green. Manually: `wasm-pack build crates/netrust-wasm --target web --out-dir ../../web/pkg` succeeds.
- [ ] **Step 6: Commit** — `fix(wasm): shared parser with errors, panic hook, chunked tournament; escape game strings in web UI`

---

### Task 4: TUI — pure key handler, NetHack-style quit, help coverage, ґ

**Files:**
- Create: `crates/netrust-tui/src/keys.rs` (pure key handling)
- Modify: `crates/netrust-tui/src/main.rs` (main loop calls `keys::handle_key`; quit prompt; help text)
- Test: unit tests in `keys.rs`

**Interfaces:**
- Produces (in `keys.rs`):
  ```rust
  pub enum KeyOutcome { Act(ActionAst), Quit, ConfirmQuit, OpenInventory(InventoryPurpose), OpenHelp, ToggleLanguage, Redraw, Nothing }
  pub enum InventoryPurpose { Drop, Wield, Quaff, Read, Eat, Sacrifice, Apply, Rub, PriceCheck, Quiver }
  pub struct KeyContext { pub player: Coord, pub last_dir: Direction }
  pub fn handle_key(key: KeyEvent, ctx: &KeyContext) -> KeyOutcome;
  pub fn confirm_quit_answer(key: KeyEvent) -> bool; // true only for 'y'/'Y' (after map_ukrainian_key)
  pub const HELP_KEYS: &[(char, &str)]; // single-source list used to render the help screen
  pub fn map_ukrainian_key(c: char) -> char; // moved from main.rs, adds 'ґ'->'\\', 'Ґ'->'|'
  ```
- Behaviour: `q` → `OpenInventory(Quaff)`; `Esc` → `ConfirmQuit`; `Ctrl-C` (`KeyModifiers::CONTROL` + `'c'`) → `ConfirmQuit`; every key that `main.rs` handles today keeps its effect (move keys → `Act(Move)`, `,` → `Act(PickUp)`, `x` → `Act(Cast{spell_index:0, dir: last_dir})`, `z` → zap with `last_dir` and `ZAP_ENERGY`, `\` → `ToggleLanguage`, `?` → `OpenHelp`, `d`/`w`/`E`/`C`/… → as today).

- [ ] **Step 1: Write failing tests** in `keys.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn k(c: char) -> KeyEvent { KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE) }
    fn ctx() -> KeyContext { KeyContext { player: Coord::new(10, 10).unwrap(), last_dir: Direction::East } }

    #[test] fn q_quaffs_not_quits() { assert!(matches!(handle_key(k('q'), &ctx()), KeyOutcome::OpenInventory(InventoryPurpose::Quaff))); }
    #[test] fn esc_and_ctrl_c_ask_to_quit() {
        assert!(matches!(handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), &ctx()), KeyOutcome::ConfirmQuit));
        assert!(matches!(handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL), &ctx()), KeyOutcome::ConfirmQuit));
        assert!(!matches!(handle_key(k('c'), &ctx()), KeyOutcome::ConfirmQuit), "plain c is close-door");
    }
    #[test] fn quit_confirmation_accepts_y_and_ukrainian_n_key() {
        assert!(confirm_quit_answer(k('y')));
        assert!(confirm_quit_answer(k('Y')));
        assert!(confirm_quit_answer(k('н')), "UA layout key on the y position");
        assert!(!confirm_quit_answer(k('n')));
        assert!(!confirm_quit_answer(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    }
    #[test] fn every_help_key_is_handled() {
        for (c, _) in HELP_KEYS {
            assert!(!matches!(handle_key(k(*c), &ctx()), KeyOutcome::Nothing), "help lists '{c}' but it does nothing");
        }
        for c in ['d', 'w', 'x', 'E', 'C', '\\', '?'] {
            assert!(HELP_KEYS.iter().any(|(h, _)| *h == c), "handled key '{c}' missing from help");
        }
    }
    #[test] fn cast_uses_last_direction() {
        let c = KeyContext { last_dir: Direction::NorthWest, ..ctx() };
        assert!(matches!(handle_key(k('x'), &c), KeyOutcome::Act(ActionAst::Cast { dir: Direction::NorthWest, .. })));
    }
    #[test] fn ukrainian_ghe_maps_to_backslash() { assert_eq!(map_ukrainian_key('ґ'), '\\'); }
}
```
(`н` on the UA layout sits on the QWERTY `y` key; `map_ukrainian_key('н')` must already map to `'y'` — verify in the existing table and keep it.)

- [ ] **Step 2: Run** `cargo test -p netrust-tui` → FAIL (module missing).
- [ ] **Step 3: Implement** `keys.rs` by moving the per-key logic out of `main.rs`'s big `match code` (main.rs ~886-1100) into `handle_key`, returning outcomes instead of mutating state; `main.rs` interprets outcomes (stepping actions, opening modals, the quit prompt drawn on the message line, toggling locale). Help screen renders from `HELP_KEYS` (localized descriptions may stay in main.rs keyed by char until Task 7). Behaviour of every existing key must be preserved except `q` (now quaff) and Esc/Ctrl-C (now confirm).
- [ ] **Step 4: Run** Global Constraints set → green. Manual check: `cargo run --bin netrust -- --help` not required yet.
- [ ] **Step 5: Commit** — `refactor(tui): pure key handler; q quaffs, Esc/Ctrl-C confirm quit; help matches keys`

---

### Task 5: TUI — rendering, resize, seed flag, panic safety, inventory paging

**Files:**
- Modify: `crates/netrust-tui/src/main.rs`
- Create (optional): `crates/netrust-tui/src/pager.rs` for the inventory pager
- Test: unit tests for `parse_seed_args` and the pager

**Interfaces:**
- Produces:
  - `fn parse_seed_args(args: &[String]) -> Result<Option<u64>, String>` — `--seed N` → `Some(N)`; no flag → `None`; `--seed` without/with invalid value → `Err`.
  - `pub struct Pager { pub page: usize }` with `pub fn page_items<T>(&self, items: &[T]) -> &[T]` (20 per page), `pub fn next(&mut self, len: usize)`, `pub fn prev(&mut self)`, `pub fn select(&self, letter: char, len: usize) -> Option<usize>` (a–t → absolute index on current page; None if beyond visible items).
- Behaviour: seed = `--seed` or `SystemTime::now()` nanos as u64; shown in the status line; `Event::Resize(..)` → redraw; `render` uses `queue!` for every draw command and calls `stdout.flush()` once at the end; panic hook installed at start of `main` that runs `disable_raw_mode`, `execute!(stdout(), Show, LeaveAlternateScreen)` then the previous hook; `TerminalGuard::new` disables raw mode if the alternate-screen command fails.

- [ ] **Step 1: Write failing tests**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    fn s(v: &[&str]) -> Vec<String> { v.iter().map(|x| x.to_string()).collect() }

    #[test] fn seed_flag() {
        assert_eq!(parse_seed_args(&s(&["netrust"])), Ok(None));
        assert_eq!(parse_seed_args(&s(&["netrust", "--seed", "99"])), Ok(Some(99)));
        assert!(parse_seed_args(&s(&["netrust", "--seed"])).is_err());
        assert!(parse_seed_args(&s(&["netrust", "--seed", "x"])).is_err());
    }
    #[test] fn pager_pages_and_selects_visible_only() {
        let items: Vec<u32> = (0..45).collect();
        let mut p = Pager { page: 0 };
        assert_eq!(p.page_items(&items).len(), 20);
        assert_eq!(p.select('t', items.len()), Some(19));
        p.next(items.len()); p.next(items.len());
        assert_eq!(p.page_items(&items), &items[40..45]);
        assert_eq!(p.select('a', items.len()), Some(40));
        assert_eq!(p.select('f', items.len()), None);
        p.next(items.len());
        assert_eq!(p.page, 2, "no page past the end");
        p.prev(); p.prev(); p.prev();
        assert_eq!(p.page, 0);
    }
}
```
- [ ] **Step 2: Run** → FAIL. **Step 3: Implement.** The inventory modal shows `page_items`, a "(page X/Y, >/< to turn)" footer, and resolves selection via `Pager::select`.
- [ ] **Step 4: Run** Global Constraints set → green. Manual checks (record in report): resize redraws; `cargo run --bin netrust -- --seed 5` shows seed 5; a forced panic (temporary local edit, not committed) leaves the terminal usable.
- [ ] **Step 5: Commit** — `fix(tui): batched rendering, resize redraw, --seed, panic-safe terminal, paged inventory`

---

### Task 6: i18n — full name coverage and name translation inside messages

**Files:**
- Modify: `crates/netrust-i18n/Cargo.toml` (`[dev-dependencies] netrust-data = { path = "../netrust-data" }`)
- Modify: `crates/netrust-i18n/src/lib.rs`
- Modify callers that pre-translate names (grep `t_monster(` / `t_item(` in netrust-tui, netrust-sim) to pass raw names instead
- Test: `crates/netrust-i18n/tests/coverage.rs` (new)

**Interfaces:**
- Produces: `pub const UK_IDENTICAL_NAMES: &[&str]` (proper nouns that stay identical in Ukrainian, lowercase), `pub const UK_IDENTICAL_KEYS: &[&str]` (t() keys allowed to be identical), `pub const ALL_KEYS: &[&str]` (every key `t()` handles).
- Behaviour: every `Messages::*` function that takes a monster or item name translates it with `t_monster`/`t_item` when `locale == Locale::Uk`.

- [ ] **Step 1: Write failing tests** — `crates/netrust-i18n/tests/coverage.rs`:
```rust
use netrust_i18n::{t, t_item, t_monster, Messages, ALL_KEYS, UK_IDENTICAL_KEYS, UK_IDENTICAL_NAMES};
use netrust_types::Locale;

#[test]
fn every_bestiary_name_is_translated() {
    let missing: Vec<_> = netrust_data::BESTIARY.iter().map(|m| m.name)
        .filter(|n| !UK_IDENTICAL_NAMES.contains(&n.to_lowercase().as_str()))
        .filter(|n| t_monster(n, Locale::Uk) == *n).collect();
    assert!(missing.is_empty(), "untranslated monsters: {missing:?}");
}

#[test]
fn every_catalog_item_is_translated() {
    let missing: Vec<_> = netrust_data::ITEM_CATALOG.iter().map(|i| i.name)
        .filter(|n| !UK_IDENTICAL_NAMES.contains(&n.to_lowercase().as_str()))
        .filter(|n| t_item(n, Locale::Uk) == *n).collect();
    assert!(missing.is_empty(), "untranslated items: {missing:?}");
}

#[test]
fn every_key_differs_between_locales() {
    let same: Vec<_> = ALL_KEYS.iter().filter(|k| !UK_IDENTICAL_KEYS.contains(k))
        .filter(|k| t(k, Locale::En) == t(k, Locale::Uk)).collect();
    assert!(same.is_empty(), "keys not translated: {same:?}");
}

#[test]
fn messages_translate_embedded_names() {
    let s = Messages::attack_hit("jackal", Locale::Uk);
    assert!(!s.contains("jackal"), "{s}");
    assert!(s.contains(&t_monster("jackal", Locale::Uk)));
    let w = Messages::wish_granted("long sword", Locale::Uk);
    assert!(!w.contains("long sword"), "{w}");
}

#[test]
fn corpse_suffix_is_case_insensitive() {
    assert_ne!(t_item("Jackal Corpse", Locale::Uk), "Jackal Corpse");
}
```
(Use each function's real signature — check `attack_hit` / `wish_granted` parameter order and adapt the call, not the assertion. `t()`'s key parameter is `&'static str`; `ALL_KEYS` is `&[&'static str]`.)

- [ ] **Step 2: Run** `cargo test -p netrust-i18n --test coverage` → FAIL listing missing names.
- [ ] **Step 3: Implement** translations for every listed name (standard Ukrainian NetHack-community terms where they exist; otherwise accurate literal translations), `ALL_KEYS`, allow-lists (keep them minimal: true proper nouns like "Excalibur", "Mjollnir", "Magicbane", "Medusa", "Croesus", "Vlad the Impaler"? — transliterate where Ukrainian uses Cyrillic, e.g. "Медуза"; only names identical in Ukrainian text go in the list), internal translation in name-taking `Messages` functions, and update callers to stop double-translating.
- [ ] **Step 4: Run** Global Constraints set → green.
- [ ] **Step 5: Commit** — `feat(i18n): translate all monster and item names; messages localize embedded names; coverage tests`

---

### Task 7: TUI — move inline locale strings into i18n

**Files:**
- Modify: `crates/netrust-tui/src/main.rs`, `crates/netrust-tui/src/keys.rs` (help descriptions)
- Modify: `crates/netrust-i18n/src/lib.rs` (new keys/messages; add them to `ALL_KEYS`)
- Test: `crates/netrust-tui` test asserting no `Locale::Uk` branching remains is not practical — instead a grep-based check in the report plus the i18n coverage test covering the new keys.

- [ ] **Step 1:** Grep `Locale::Uk` in `crates/netrust-tui/src` and list every occurrence (≈39). For each, add a `t()` key or `Messages` fn in netrust-i18n with EN and UK text identical to the current strings, and add new keys to `ALL_KEYS` (the coverage test from Task 6 now fails until both locales differ — that's the RED step).
- [ ] **Step 2: Run** `cargo test -p netrust-i18n` → FAIL if any new key is missing a translation; fix in i18n.
- [ ] **Step 3:** Replace each inline branch in the TUI with the i18n call. Remaining `Locale::Uk` uses in the TUI must be limited to the language toggle itself (`grep -n "Locale::Uk" crates/netrust-tui/src` shows ≤ 2 hits).
- [ ] **Step 4: Run** Global Constraints set → green; launch `cargo run --bin netrust -- --seed 1`, toggle language with `\`, confirm help/status/modals switch (manual, note in report).
- [ ] **Step 5: Commit** — `refactor(tui): all UI text via netrust-i18n`

---

### Task 8: Python RL — real gymnasium env, REINFORCE script fixes, binding fixes

**Files:**
- Modify: `crates/netrust-py/Cargo.toml` (remove unused `rand`)
- Modify: `crates/netrust-py/src/lib.rs` (clear `explored_tiles` on depth change)
- Modify: `python/netrust_gym/env.py`, `python/netrust_gym/__init__.py` (exports)
- Rename: `python/train_ppo.py` → `python/train_reinforce.py` (`git mv`), update references in README.md, python/README.md, docs
- Modify: `python/tests/test_rl.py` (+ new tests)

**Interfaces:**
- Produces: `class NetRustGymEnv(gymnasium.Env)` with `metadata = {"render_modes": ["ansi"]}`; `__init__(self, seed=None, max_steps=…, conduct_masking=…, render_mode=None)`; `observation_space` = `gymnasium.spaces.Dict` of numeric `Box`es only (glyph map, hp, max_hp, depth, gold, nutrition, pw, inventory_count, turn — use the fields the env already produces); string fields (`ascii_map`, messages) move to `info`; `reset(seed=None, options=None)` calls `super().reset(seed=seed)`, uses `seed` if given else `int(self.np_random.integers(0, 2**31 - 1))`, returns `(obs, info)` with `info["seed"]`; `action_masks()` unchanged; helper `sample_masked_action(mask, rng) -> int` in `env.py`.

- [ ] **Step 1: Set up venv** (Global Constraints) and confirm existing tests run: `maturin develop -m crates/netrust-py/Cargo.toml && python -m unittest discover python/tests` (fix the test loader path if `maturin develop` installs the module into the venv — then the dylib copy hack becomes unnecessary; keep it harmless).
- [ ] **Step 2: Write failing tests** (append to `python/tests/test_rl.py`):
```python
    def test_env_checker_passes(self):
        from gymnasium.utils.env_checker import check_env
        check_env(NetRustGymEnv(max_steps=20), skip_render_check=True)

    def test_reset_without_seed_varies(self):
        env = NetRustGymEnv(max_steps=5)
        seeds = {env.reset()[1]["seed"] for _ in range(5)}
        self.assertGreater(len(seeds), 1)

    def test_reset_with_seed_is_deterministic(self):
        env = NetRustGymEnv(max_steps=5)
        a, _ = env.reset(seed=123)
        b, _ = env.reset(seed=123)
        self.assertTrue((a["map_glyphs"] == b["map_glyphs"]).all())

    def test_masked_sampling_respects_mask(self):
        import numpy as np
        from netrust_gym.env import sample_masked_action
        rng = np.random.default_rng(0)
        mask = [False] * 26
        mask[3] = mask[23] = True
        for _ in range(200):
            self.assertIn(sample_masked_action(mask, rng), (3, 23))

    def test_exploration_reward_resets_per_level(self):
        from netrust_py import NetRustEnv  # adjust to the actual PyO3 class name
        # Descend once and assert that stepping onto a coordinate explored on level 1 still yields exploration reward on level 2.
        # Implement via the binding's debug/introspection API if available; otherwise test that `explored_count()` (add a #[getter] if missing) drops to the new level's count after a depth change.
```
For the last test: add a `#[getter] fn explored_count(&self) -> usize` to the PyO3 class if none exists, and assert it resets (≤ visible tiles of the new level) right after a depth change caused by `DESCEND` on the stairs (move the player onto stairs via existing env helpers, or use a seed+action sequence that reaches stairs; if not feasible in a test, assert the Rust-side unit test instead: add a `#[cfg(test)]` test in `netrust-py/src/lib.rs` calling the internal `record_exploration` + depth-change path directly).
- [ ] **Step 3: Run** → FAIL. **Step 4: Implement** env changes, `sample_masked_action`, `train_reinforce.py` (docstring REINFORCE; prayer shaping via `ACTION_NAMES.index("PRAY")`; remove the constant gold bonus; sample with `sample_masked_action(env.action_masks(), rng)`), PyO3 fixes.
- [ ] **Step 5: Run** Python tests → PASS; `python python/train_reinforce.py --episodes 2` (or its equivalent quick flag; add `--episodes` if missing) runs without error; Global Constraints Rust set → green; `bash scripts/check-doc-links.sh` passes after the rename.
- [ ] **Step 6: Commit** — `fix(python): real gymnasium.Env, seeded resets, masked REINFORCE trainer, per-level exploration`

---

### Task 9: Final verification

- [ ] Run the full Global Constraints command set; `wasm-pack build crates/netrust-wasm --target web --out-dir ../../web/pkg`; Python tests in the venv.
- [ ] `grep -rn "Direction::None" crates/netrust-wasm/src crates/netrust-agent/src/graphql.rs` → no parse-fallback uses remain.
- [ ] `grep -n "Locale::Uk" crates/netrust-tui/src` → ≤ 2 hits (language toggle).
- [ ] Commit any fixups.
