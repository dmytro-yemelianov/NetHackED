# Frontend Fixes (TUI, WASM/web, Python RL, i18n) — Design

Date: 2026-10-04
Status: Design approved in conversation; written spec awaiting review
Source: full-repo review of 2026-10-04, group F
Branch: `feat/frontend-fixes` (stacked on `chore/repo-hygiene`)

## Goal

Fix the frontend findings from the repo review and remove the duplicated action/character
parsing that has drifted between frontends.

## Non-goals

- NetHack fidelity of mechanics (cycle D).
- TUI field-of-view vs `compute_perception` unification.
- Multi-session GraphQL.
- Changing the PyO3 integer action space (indices 0–25 stay as they are).

## Constraints

- TDD: each fix lands with a test that fails before the fix (UI-only rendering changes are
  verified by a pure-function test where possible, otherwise by a documented manual check).
- CI stays green: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`,
  `cargo test --workspace --exclude nethacked-py --locked`, wasm32 build, doc-link check. Toolchain pinned 1.88.0.
- Public action names accepted today by MCP (`STEP_ACTIONS`), JSON-RPC, GraphQL and WASM keep working.
- New dependencies allowed: `console_error_panic_hook` (nethacked-wasm only); `nethacked-data` as a
  dev-dependency of `nethacked-i18n`. Nothing else.

---

## 1. Shared command module — `crates/nethacked-agent/src/commands.rs`

### API

```rust
pub const ZAP_ENERGY: u32 = 6;
pub const MAX_WISH_LEN: usize = 128;

#[derive(Debug, Default, Clone)]
pub struct ActionArgs {
    pub index: Option<usize>,
    pub direction: Option<String>,
    pub target: Option<(usize, usize)>,
    pub text: Option<String>,
    /// Player position, needed for direction-relative targets (kick_<dir>, open/close with a direction).
    pub player: Option<Coord>,
}

pub fn parse_action(name: &str, args: &ActionArgs) -> Result<ActionAst, String>;
pub fn parse_character(
    name: Option<&str>, role: Option<&str>, race: Option<&str>,
    gender: Option<&str>, alignment: Option<&str>,
) -> Result<CharacterConfig, String>;
```

`parse_direction` moves from `rpc.rs` into `commands.rs` (re-exported from `rpc` for compatibility).

### Vocabulary (case-insensitive)

| Name(s) | Required args | Result |
|---|---|---|
| `move_<dir>` / `move` | `move` needs `direction` | `Move(dir)` |
| bare `<dir>` (`north`, `k`, …) | — | `Move(dir)` (JSON-RPC compatibility) |
| `kick_<dir>` / `kick` | `kick`: `direction`+`player`, or `target` | `Kick(coord)`; off-map → Err |
| `open_door`, `close_door` | `target`, or `direction`+`player` | `OpenDoor`/`CloseDoor(coord)` |
| `pickup` | — | `PickUp` |
| `drop`, `wield`, `quaff`, `read`, `eat`, `sacrifice`, `quiver`, `apply`/`light`, `price_check`/`appraise`, `rub` | `index` (default 0) | corresponding variant |
| `cast` | `index` (default 0), `direction` (default East) | `Cast { spell_index, dir }` |
| `zap` | `direction` (default East) | `ZapWand { dir, energy: ZAP_ENERGY }` |
| `fire` | `direction` (required) | `Fire(dir)` |
| `wish` | `text` (required, ≤ `MAX_WISH_LEN` chars) | `Wish(text)` |
| `engrave` | `text` (default "Elbereth") | `Engrave { text, medium: Dust(1) }` |
| `search`, `pay`, `pray`, `ascend`, `descend`, `wait` | — | corresponding variant |
| `untrap` | `target`, or `direction`+`player` | `Untrap(coord)` |

Directions: the 8 compass names and vi-keys; `up`/`down`/`none` are rejected for actions
that need a compass direction. Any unknown name, bad direction, missing required argument,
non-integer index or off-map coordinate → `Err(message)`.

The exact set must be a superset of every name accepted today by MCP `STEP_ACTIONS`,
JSON-RPC `nethacked.step`, `bin/jsonrpc.rs`, GraphQL `stepAction`, and WASM `step`
(audit each before deleting its parser; any name not in the table above is added).

### Character parsing

Uses `nethacked_data::roles::{ROLES, RACES}` names (case-insensitive). `None` → current
defaults (name "Hero", Valkyrie, Human, Female, role's default alignment if the existing
code uses it, else Neutral). Unknown value → `Err("unknown role 'x'; expected one of …")`.

### Consumers

- `mcp.rs`: `parse_step_action` → `commands::parse_action`; `STEP_ACTIONS` stays as the
  advertised schema enum and must be a subset of what `parse_action` accepts (test).
  `nethacked_reset_with_character` → `parse_character` (Err → `-32602`).
- `jsonrpc.rs` (`nethacked.step`) and `bin/jsonrpc.rs`: use `parse_action`; errors as today's shapes.
- `graphql.rs`: `stepAction` and `resetWithCharacter` use the shared parsers; errors become
  GraphQL errors (resolver returns `Err`), no silent `Wait`.
- `nethacked-wasm` `step(action, arg)`: maps `arg` into `ActionArgs` (index or direction or text
  by action kind) and calls `parse_action`; returns `Result<String, JsValue>`-style error string
  to JS. Character creation uses `parse_character`.
- `nethacked-py`: `ZAP_WAND` uses `ZAP_ENERGY`.

---

## 2. TUI — `crates/nethacked-tui`

- Key handling is extracted into a pure function, e.g.
  `fn handle_key(mode: &mut UiMode, key: KeyEvent, ctx: &KeyContext) -> KeyOutcome`
  (`KeyOutcome::{Act(ActionAst), Quit, OpenModal(..), Redraw, None}`) so it can be unit-tested.
- `q` = quaff (opens the potion selection like other index actions). `Esc` and `Ctrl-C`
  (KeyModifiers::CONTROL + 'c') open a "Really quit? [yn]" prompt; `y` quits, anything else
  cancels. The dead-player screen keeps its current any-key/quit behaviour.
- Help screen lists exactly the handled keys (test: every key in the help list maps to a
  non-`None` outcome; spot-check `d`, `w`, `x`, `E`, `C`, `\`).
- `Event::Resize` triggers a redraw.
- Rendering uses `queue!` throughout and a single `stdout.flush()` per frame.
- `--seed <u64>` CLI flag (`std::env::args`, no new deps); without it the seed is derived from
  `SystemTime::now()`; the seed is shown in the status/help line.
- Panic hook: restores the terminal (show cursor, leave alternate screen, disable raw mode)
  before delegating to the default hook. `TerminalGuard::new` disables raw mode if entering
  the alternate screen fails.
- Inventory modal pages 20 items at a time (`>`/`<` change page); letters a–t select only
  visible items on the current page.
- `map_ukrainian_key` adds `ґ`/`Ґ` → `\`/`|` (the key position of `\` on the UA layout).
- Inline `if locale == Locale::Uk { … } else { … }` strings move into `nethacked_i18n::Messages`
  / `t()` keys (role names, modal titles, help text, death text, language toggle, event text).

## 3. WASM / web

- `step` uses `commands::parse_action` (see §1). `cast`/`zap`/`fire` receive a direction;
  `web/index.html` passes the last movement direction for `x` (cast) and zap keys, mirroring
  the TUI's `last_dir`.
- `console_error_panic_hook::set_once()` in a `#[wasm_bindgen(start)]` function.
- Tournament runs in chunks: `TournamentRun::new(num_seeds, max_turns)` exported class with
  `step(k: usize) -> bool` (runs up to k seeds, returns done) and `progress()`, `report_json()`.
  `run_tournament_benchmark` stays as a convenience wrapper (runs to completion). The page
  uses the chunked API with `await new Promise(r => setTimeout(r))` between chunks and
  shows progress; the result equals the one-shot wrapper's for the same inputs (test).
- `web/index.html`: game-derived strings (item name/class/buc, policy names, error messages,
  player names) are inserted via `textContent` or an `escapeHtml` helper; no `innerHTML`
  interpolation of such strings remains.

## 4. Python RL

- `NetHackEDGymEnv(gymnasium.Env)`: `metadata`, `action_space = Discrete(26)`, an
  `observation_space` that `gymnasium.utils.env_checker.check_env` accepts (numeric `Box`es
  in a `Dict`; non-numeric fields move to `info`). `reset(seed=None, options=None)` calls
  `super().reset(seed=seed)` and, when `seed` is None, draws the engine seed from
  `self.np_random`; `render_mode="ansi"` supported.
- `train_ppo.py` → `train_reinforce.py` (README/docs references updated); docstring says
  REINFORCE; prayer shaping uses `ACTION_NAMES.index("PRAY")`; constant gold bonus removed;
  actions sampled only among `action_masks()`-allowed indices.
- `nethacked-py`: `explored_tiles` cleared when depth changes; `ZAP_WAND` uses `ZAP_ENERGY`;
  unused `rand` dependency removed.
- Tests (`python/tests/`): `check_env` passes; two `reset()` calls without seed give
  different initial observations with high probability (compare seeds via `info`); a
  masked-sampling helper never returns a disallowed action. Python tests run locally via
  the documented maturin flow; CI stays Rust-only in this cycle.

## 5. i18n

- `t_monster` / `t_item` cover every `BESTIARY` name and every `ITEM_CATALOG` name in
  Ukrainian (case-insensitive lookup; existing rules like "ghost of " and corpse suffix kept,
  corpse suffix made case-insensitive).
- Messages that embed a monster/item name translate it internally: callers pass raw English
  names; functions call `t_monster`/`t_item` for `Locale::Uk`. Applies to every
  `Messages::*` function taking a monster or item name.
- Tests (in `nethacked-i18n`, with `nethacked-data` as dev-dependency): every bestiary/catalog
  name has a Ukrainian translation different from the English (allow-list for proper nouns
  that stay identical, e.g. "Excalibur", "Mjollnir", "Medusa"); every `t()` key returns
  different EN/UK text except an explicit allow-list (e.g. "Elbereth"); a sample of
  name-taking `Messages` in UK contains the translated name, not the English one.

## Testing strategy

- `commands.rs`: table-driven unit tests for every vocabulary row, every error class, and a
  compatibility test asserting each name from the old parsers (listed in the test) parses.
- Frontend integration: MCP/JSON-RPC/GraphQL tests for an unknown action → error; WASM
  `step` covered via native (non-wasm) unit tests of the arg-mapping function.
- TUI: unit tests of `handle_key` and the inventory pager; manual check of resize and panic
  restore documented in the PR.
- Python: unittest additions listed above.
- i18n: coverage tests above.

## Risks

- GraphQL/WASM clients relying on unknown actions silently waiting will now get errors (intended).
- TUI `q` changes from quit to quaff (matches NetHack and the existing help text).
- i18n translation quality for ~100 new names: machine-assisted; reviewed for obvious errors only.
