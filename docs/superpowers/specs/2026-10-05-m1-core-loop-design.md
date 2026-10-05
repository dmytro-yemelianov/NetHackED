# M1 "Playable core loop" design

*Status: approved for implementation on branch `feat/m1-core-loop`. Plan: [2026-10-05-m1-core-loop.md](../plans/2026-10-05-m1-core-loop.md).*

## 1. Intent

Roadmap milestone M1 ([roadmap.md](../../roadmap.md), "M1 — Playable core loop") makes NetHackED read and control like NetHack 5.0 in both player frontends:

- the terminal UI (`crates/nethacked-tui`, `src/main.rs` + `src/keys.rs`);
- the browser terminal (`web/play/play.js` driving `crates/nethacked-wasm`, map from `nethacked_agent::render_ascii_map`).

The seven scope bullets become seven work items:

| Key | Bullet | Main C source |
|---|---|---|
| `doors-diagonal` | doors blocking diagonal moves | hack.c:991 `test_move`, :1141, :1209, :4063 `doorless_door`; mon.c:2250 `mfndpos` |
| `messages` | message history `^P`, every message shown | cmd.c:164 `doprev_message`; win/tty/topl.c:20, :251 `update_topl`, :205 `more` |
| `map-memory` | explored tiles stay drawn | display.c:917 `newsym`, :448 `_map_location`; rm.h `struct rm.glyph` |
| `glyphs` | monsters and objects drawn with their C glyphs | include/defsym.h MONSYM / OBJCLASS / PCHAR tables; drawing.c |
| `item-prompts` | `What do you want to eat? [a-c or ?*]` in the terminal | invent.c:1752 `getobj`, :1627 `compactify` |
| `zap-choice` | `z` chooses the wand | zap.c:2641 `dozap`, :2632 `zap_ok` |
| `look` | `;` `:` `/` and "You see here" | pager.c:1673 `do_look`, invent.c:4150 `look_here`, pickup.c:430 `check_here` |

Every claim below was checked against the NetHack 5.0 tree at `/Users/dmytro/github/NetHack` (`grep -n` on the cited line). Implementers re-verify each citation they copy into a doc comment.

## 2. Cross-cutting architecture decisions

The investigators' designs overlapped and in places contradicted each other. These decisions are binding for all items.

### D1. One display pipeline, in the engine crate

`nethacked-sim` gets a new module `map_view.rs` that owns everything that decides what a map cell shows:

- `SimulationWorld::update_map_memory(&mut self)` (writes the per-level memory);
- `SimulationWorld::map_view(&self) -> MapView` (one `compute_perception` call; live cells in sight, remembered cells otherwise);
- `SimulationWorld::glyph_at(&self, view: &MapView, c: Coord) -> MapGlyph { ch, kind, remembered }`, the **only** function that chooses a map character.

`glyph_at` lives in `sim`, not in `agent`, because the `look` item (also in `sim`) must describe exactly the character that is drawn (`do_screen_description` starts from the displayed glyph, pager.c:1247), and `agent` depends on `sim`, not the reverse. `nethacked_agent::render_ascii_map` becomes a thin loop over `glyph_at`. The TUI builds its cells from the same `glyph_at` and adds colour by `GlyphKind`. A frontend parity test asserts that the TUI cell characters equal `render_ascii_map` character for character.

Sequence: `map-memory` creates the module with today's characters; `glyphs` changes only the character choices inside it (monster class letters, C terrain symbols) and adds the per-cell kind string for the browser; `look` reads it.

### D2. Map memory is semantic, stored on the level

`DungeonLevel` gains `#[serde(default, with = "coord_map")] memory: HashMap<Coord, RememberedCell>`, where `RememberedCell { terrain: Tile, trap: Option<TrapType>, object: Option<RememberedObject { class, name }> }` (types crate). The glyph is derived when drawing, so the `glyphs` item never touches the save format. Memory travels with `StoredLevel.level`, like C saves `levl[][].glyph` with the level (save.c:461 `savelev`, restore.c:1074 `getlev`).

### D3. One message pipeline, frontend state

`nethacked-sim` gets `messages.rs`:

- `turn_messages(world: &SimulationWorld, events: &[GameEvent]) -> Vec<String>`, the single event-to-text policy for both frontends (takes the world, not just the locale, so `look` can add "You see here" without a signature change);
- `MessageWindow`, a port of the tty top line (`update_topl` packing, `--More--`, `^P` ring of 20 lines, `msg_window:single`).

The window is frontend state (TUI loop and `WasmGameSession`), never inside `SimulationWorld`: no save field, no RNG, no event.

### D4. "You see here" is derived after the step, not emitted by the engine

The `look` design proposed new `LogMessage`/`LookWindow` events inside `handle_move`. The hero starts next to floor items, so that would change `EXPECTED_LOGS` in `golden_determinism.rs`, which the project rules forbid for display work. Instead:

- `movement.rs` is **not** changed by `look`;
- `turn_messages` finds the hero's `ActorMoved { to }` in the step's events and, when objects lie at `to`, replaces the engine's engraving line for `to` with the full `look_here(Step)` report (feature line, engraving, "You see here X." or "There are several objects here."), inserted where C prints it: before trap messages, after them for pits (hack.c:3381 vs :3400);
- `turn_window(world, events) -> Option<Vec<String>>` returns the "Things that are here:" pile window for 2..4 objects;
- `:` calls `world.look_here(LookCtx::Command)` directly (free action, no events).

Divergence (documented): agent observations, MCP and the event log do not contain "You see here"; only the two player frontends show it.

### D5. One getobj implementation, shared by item commands and `z`

`crates/nethacked-agent/src/item_prompt.rs` (from `item-prompts`) is the only port of `getobj` classification, letters, `compactify` and key resolution. It defines `ItemVerb::Zap` from the start. `zap-choice` adds no second prompt module; it consumes `ItemPrompt::build(world, ItemVerb::Zap)`, the generic i18n texts and the generic wasm methods `item_prompt_json(action)` / `item_prompt_pick(action, key)`.

Letters are global and positional: `inv_letter(i)` = `a..z`, `A..Z`, then `#` (invent.c:694 letter space). Persistent letters stay out of scope.

### D6. Ownership of shared hot spots

| Shared spot | Rule |
|---|---|
| `crates/nethacked-tui/src/main.rs` event loop | `messages` replaces the `message: String` with `MessageWindow`; later items only call `win.post(..)` (history) or `win.show(..)` (prompt, no history). |
| TUI help grid | `messages` adds `pub const HELP_ROWS: usize = 14` in `keys.rs`, a `HELP_CTRL_KEYS` list and a capacity test (`HELP_KEYS.len() + HELP_CTRL_KEYS.len() <= HELP_ROWS * 3`). `item-prompts` adds `a` (41 of 42). `look` adds `:` `;` `/` and raises `HELP_ROWS` to 15. |
| i18n generic prompt texts | Owned by `item-prompts`: `tui.never_mind`, `tui.no_such_object`, `verb.*` (including `verb.zap`), `Messages::item_prompt`, `nothing_to`, `silly_thing`. |
| `play.js` message state | `messages` replaces `messages`/`message` with `session.msg*` calls and `say()` / `prompt()` helpers; later items use those helpers. |
| Door glyph arms | `doors-diagonal` adds `DoorState::NoDoor` arms; `map-memory` moves the terrain table into `map_view.rs`; `glyphs` changes its characters. |
| `docs/parity/README.md` summary block and the manual `Counts:` line in `flow-ui.md` | Every task re-runs `python3 scripts/parity-summary.py` and fixes the `Counts:` line by hand after editing rows. |

### D7. Determinism

No item may add an RNG draw, reorder one, or add or remove a `GameEvent` on paths that the golden tests exercise. `doors-diagonal` changes behaviour only next to door tiles, and the level generator places none (`grep Tile::Door crates/nethacked-dungeon/src` finds only `quest.rs`). If any golden fingerprint changes, the implementer stops and reports the diff instead of updating the constants.

## 3. Per-item behaviour

### 3.1 `doors-diagonal`

C rule (hack.c:1138-1151 and 1206-1213, `doorless_door` hack.c:4063-4073): a diagonal step is refused when the destination is a door that is not doorless (`D_NODOOR` or `D_BROKEN`), checked first ("You can't move diagonally into an intact doorway."), or when the hero stands on such a door ("... out of an intact doorway."). The refusal takes no time (hack.c:2843 `context.move = 0`). Attacks are resolved before `test_move` (hack.c:2794-2810), so fighting diagonally across a doorway works. Pet and peaceful swaps go through `test_move` and are refused. A closed door bumped diagonally is still auto-opened first (hack.c:1074-1100). Monsters follow `mfndpos` (mon.c:2250-2257): no diagonal step into or out of a door unless broken or doorless; monster melee is not restricted.

Rust:
- `DoorState::NoDoor` appended last (empty doorway, C `D_NODOOR`); passable and transparent like `Broken`; `Tile::has_intact_door()`.
- Pure rule `nethacked_core::grid::diagonal_door_block(from, to, dx, dy) -> Option<DiagonalDoorBlock>`.
- Hero: new guarded arm in `handle_move` before the passable-move arm; same check in the tame swap branch and in `bump_peaceful` after its `rn2(7)` draw, so RNG order is unchanged.
- Monsters: edge-aware Dijkstra (`compute_with_edges`, `steepest_descent_by`, `steepest_ascent_by`) in `core/pathfinding.rs`, used by `sim/monsters.rs`; existing APIs kept for the Lean-modelled tests.
- Message printed always (C prints it only with `mention_walls`): documented divergence, consistent with the existing unconditional `bump_wall`.

### 3.2 `messages`

C: the tty top line packs messages of one command with two spaces while `n0 + strlen(toplines) + 3 < min(CO-8, TBUFSZ)` and the new text does not start with "You die" (topl.c:262-269); otherwise `--More--` (wintty.c:182) waits for a key, Esc skips the rest (topl.c:233-236). The next command clears the line (wintty.c:4101). `^P` (cmd.c:1811) in the default `msg_window:single` (options.c:7202) re-shows the newest line, then older ones, wrapping (topl.c:102-119); 20 lines kept (options.c:7198).

Rust: `sim/messages.rs` with `turn_messages`, `pack_lines` (char counts), `MessageWindow { post_turn, post, show, append_page, more, dismiss, skip_rest, clear_line, recall_prev, line, history }`. Event policy: `LogMessage` text as is; `AttackLanded`/`AttackMissed` skipped when the next event is a `LogMessage` (combat pairs them) and otherwise rendered with `Messages::tui_hit` / `tui.miss`; `DoorToggled` Open/Closed → "The door opens." / "The door closes." (lock.c:906, :1040); `LevelChanged` and the rest print nothing. TUI: `Ctrl-P` → `KeyOutcome::PrevMessage` (today it falls through to `p` = pay); `consume_more_key` runs before any command. Browser: `WasmGameSession` holds a `MessageWindow` fed in `step()`; `play.js` draws `msgLine()`, handles `--More--` through `msgMore/msgDismiss/msgSkipRest`, and `Ctrl+P` with `preventDefault` before the existing ctrl/meta early return.

### 3.3 `map-memory`

C: the map shows what is in sight with priority monster > object > seen trap > background (display.c:31-44, `_map_location` display.c:448-462), and out of sight shows `levl[x][y].glyph`, the hero's memory (display.c:1094-1096). Objects, traps and terrain are remembered; monsters are not (display.c:26-29). Memory is stale until the square is seen again (display.c:409-438 `unmap_object`). Pools and lava hide objects and traps (`covers_objects`/`covers_traps`, include/display.h:218-222). The hero's own square is felt even when not seen (display.c:1004-1007). Remembered unlit room floor is drawn as `S_darkroom` (display.c:240-250, include/defsym.h:113, `dark_room` default on).

Rust: types `RememberedObject`, `RememberedCell`, `MapLayer`, `RememberedCell::top_layer()` (object > trap > terrain, water/lava suppression); `DungeonLevel.memory`; `sim/map_view.rs` (`snapshot_cell`, `update_map_memory`, `map_view`, `MapView::{cell, in_sight, detected}`, `top_floor_object`, `terrain_char`, `MapGlyph`, `GlyphKind`, `glyph_at`). `update_map_memory` runs at the end of `new_with_character_and_ruleset` and at the end of `step_player_action` (after the last `recompute_hero_ac`, before `record_events`), which covers stair arrival. Web: `render_ascii_map` loops over `glyph_at` (and now draws floor objects and revealed traps, which it never did). TUI: `map_cells(world) -> Vec<Vec<(char, Color)>>` replaces the private `compute_fov(.., 8)` loop, so the TUI uses the same `compute_perception` as the web (darkness, blindness, lamps, telepathy); remembered room floor is DarkGrey.

### 3.4 `glyphs`

C: monster class letters come from `MONSYM` (include/defsym.h:295-366, e.g. ghost `' '` at :358), object classes from `OBJCLASS` (include/defsym.h:466-486), terrain from `PCHAR`/`PCHAR2` (include/defsym.h:90-154), traps `^` except web `"` (include/defsym.h:157-180). Hero is `@`, or the polymorph form's class letter (include/display.h:654-655).

Rust (inside `glyph_at`):
- monster: `world.ruleset.monster_class_of(&actor.name)` (handles "hostile " and "ghost of " prefixes); `'?'` fallback guarded by a test that every spawnable name resolves;
- object: top of pile `ItemClass::symbol()`; boulder first;
- terrain to the C default symset where Rust differs today: open door `-` in a vertical wall (`S_vodoor`) and `|` in a horizontal wall (`S_hodoor`); broken door and `NoDoor` `.` (`S_ndoor`); frozen pool `.` (`S_ice`); lava `}` (`S_lava`); unfilled `Tile::Pit` `^` (`S_pit`); secret door drawn as the wall it hides in (`-` or `|`); trap `Web` `"`;
- `render_glyph_kinds` / wasm `renderKinds()`: an 80x21 string of kind codes (`h` hero, `m` monster, `o`/`O` object, `t`/`T` trap, `.`/`:` terrain, ` ` blank; upper case and `:` = remembered, out of sight). `play.js` colours cells by kind instead of guessing from the character (letters-are-monsters fails for `& ; : ' ~` and collides with `$ + / =` items), and dims remembered cells (the `dark_room` look).

### 3.5 `item-prompts`

C `getobj` (invent.c:1752): classify each carried object with the command's callback (`GETOBJ_SUGGEST/DOWNPLAY/EXCLUDE_SELECTABLE/EXCLUDE`, include/hack.h:515-538). No suggested items and no `GETOBJ_PROMPT` → "You don't have anything to eat." (invent.c:1912-1914). Otherwise "What do you want to eat? [c or ?*]" with letters `compactify`d only when more than five (invent.c:1908, :1627), `[*]` when empty, `- ` prefix when bare hands are allowed. Quit chars → "Never mind."; unknown letter → "You don't have that object." and re-prompt (invent.c:2059); excluded object → "That is a silly thing to eat." (invent.c:2072, :2094); `?` lists suggested (or downplayed) items, `*` everything. Callbacks: `any_obj_ok` (drop), `wield_ok` (wield.c:336), `eat_ok` (eat.c:3522), `drink_ok` (potion.c:505, word "drink"), `read_ok` (read.c:316, `GETOBJ_PROMPT`), `apply_ok` (apply.c:4152, word "use or apply"), `rub_ok` (apply.c:1773), `offer_ok` (eat.c:3544), `zap_ok` (zap.c:2632), `ready_ok` (wield.c:299).

Rust: `agent/src/item_prompt.rs` (`ItemVerb`, `Fit`, `fit`, `inv_letter`, `letter_index`, `compactify`, `ItemPrompt`, `Pick`); `ActionAst::Unwield` (appended last) with `handle_unwield` (wield.c:169-182); TUI `KeyOutcome::ItemPrompt(ItemVerb)` for `d w e q r S a Q`, `#r` for rub, global letters in the inventory modal; wasm `item_prompt_json` / `item_prompt_pick`, `letter` in `get_inventory_json`; `play.js` delegates classification and key resolution to wasm and re-prompts on an unknown letter.

Key decision: TUI `R` stays ride (C `R` is remove-accessory, `M-r` rub, `M-R` ride); rub is `#r` in the TUI and `R` in the browser. Same prompt and result, different key, documented.

### 3.6 `zap-choice`

C `dozap` (zap.c:2641): `getobj("zap", zap_ok)` over wands; `zappable` spends the charge; `getdir` only when `oc_dir != NODIR` (zap.c:2658); a cancelled or invalid direction prints "What a strange direction!" for non-quit keys (cmd.c:4103-4118) and then "The wand glows and fades." with the charge and the turn spent (zap.c:2667-2669).

Rust: `ActionAst::ZapWand` gains `#[serde(default)] wand: Option<usize>` (carried index; `None` = first wand, the legacy agent behaviour) and `#[serde(default)] dir_cancelled: bool`. `handle_zap_wand` resolves the chosen wand; out-of-range → "You don't have that object.", non-wand → "That is a silly thing to zap.", both free; NODIR wands never trace a beam and ignore `dir`; `dir_cancelled` spends the charge and the turn and logs "The <wand> glows and fades."; wand of wishing zapped through `z` keeps its charge and points at the wish action (no wish text prompt exists in either frontend). `SimulationWorld::wand_is_nodir`, `wand_needs_direction`. Frontends: item prompt (`ItemVerb::Zap`) then direction prompt only when needed. Agent API: `zap` takes optional `index`; MCP `STEP_ACTIONS` gains `zap`.

### 3.7 `look`

C: `:` `dolook` → `look_here(0)` (invent.c:4371, :4150): trap line, dungeon feature (`dfeature_at` invent.c:4083), blind wording, "You see no objects here.", "You see here X.", "There are several objects here." when `obj_cnt >= pile_limit` (5), else the "Things that are here:" window. Stepping onto objects runs `check_here` (pickup.c:430) from `spoteffects` (hack.c:3381 before traps, :3400 after pits). `;` = `do_look(1)` (pager.c:2329): `getpos` (getpos.c:771: `hjklyubn` 1, `HJKLYUBN` 8, `@` self, `m`/`M` monsters, `.` `,` `;` `:` pick, Esc cancel), then `do_screen_description` (pager.c:1247): `<sym>        <alternatives>[ (<detail>)]`, "can be many things" when more than four alternatives.

Rust: `sim/look.rs` (`LookCtx`, `LookReport`, `look_here`, `farlook`, `getpos_monsters`, `GetPosKey`, `GetPosResult`, `getpos_step`), `nethacked-data/src/symbols.rs` (MONSYM/OBJCLASS/PCHAR explanations transcribed from include/defsym.h), `nethacked_i18n::an` (objnam.c:2138 `just_an`, :2174 `an`). Step report through `turn_messages`/`turn_window` (D4). TUI keys `:` `;` `/` with a cursor loop; wasm `look_here_json`, `farlook_start_json`, `farlook_key_json(x, y, key)` (stateless; cursor in JS); `play.js` `getpos` mode with the cursor drawn as an accent cell. `/` behaves like `;` (single shot); C's `/` menu stays a gap.

## 4. Frontend parity

Both frontends call the same Rust for every decision; JavaScript and the TUI only map keys and draw strings.

| Concern | Shared Rust | TUI | Browser |
|---|---|---|---|
| Map characters | `SimulationWorld::glyph_at` | `map_cells` (colour by kind) | `render_ascii_map` + `renderKinds` |
| Messages | `turn_messages`, `turn_window`, `MessageWindow` | owns a `MessageWindow` | `WasmGameSession.messages` via `msg*` |
| Item prompts | `ItemPrompt`, `Pick` | `run_item_prompt` | `item_prompt_json`, `item_prompt_pick` |
| Look | `look_here`, `farlook`, `getpos_step` | `farlook_loop` | `look_here_json`, `farlook_*_json` |
| Diagonal doors | `handle_move`, `step_monsters` | none | none |

Parity tests: `map_cells_match_ascii_render` (TUI vs agent renderer), wasm tests asserting the JSON equals the engine call for prompts and look, and one shared `turn_messages` used by both loops. `play.js` has no test harness; every task that edits it runs `node --check web/play/play.js` and records a manual browser check (wasm-pack build, `web/play/index.html?seed=1`) in its commit body when wasm-pack is available.

## 5. Non-goals

- Vision rewrite (objects-dungeon.md "Vision / line of sight", radius-8 raycast vs vision.c): not one of the seven bullets. **The roadmap exit criterion names this row, so M1 cannot be closed by these seven items alone** (see 7).
- C's `/` menu (pager.c:1700-1800: map, carried, by symbol, nearby monsters/objects/traps/engravings) and `&` whatdoes.
- Remembered unseen-monster marker `I` (display.c:377-386), engraving glyphs (`S_engroom`, include/defsym.h:114), boulder symbol option, per-monster colours, hallucination.
- Persistent inventory letters (`#adjust`), stack quantities and counts at prompts, floor-food prompts (eat.c:3584 `floorfood`), worn-item states.
- Message options: `msg_window` full/combination/reversed, `MSGTYPE`, `^P` at `--More--`, saved history.
- Self-zap (`.` at the direction prompt), zap up/down, cursed-wand backfire, wresting.
- Monsters opening doors, boulder pushes through door diagonals (hack.c:434), travel and running.
- Agent observations gaining "You see here" (D4).

## 6. Risks

| Risk | Mitigation |
|---|---|
| Golden fingerprints move | D4 and D7; golden suite in every task's gate; stop and report on any change. |
| Arena slowdown from `update_map_memory` on every step (bots clone the world each step) | Single pass over `arena.items` per update; time `cargo test -p nethacked-agent --test golden_determinism` before and after and record both in the commit body. |
| Save format | Only additive: `DungeonLevel.memory` with `serde(default)` and a test that deletes the key from saved JSON; `DoorState::NoDoor`, `ActionAst::Unwield` appended last; `ZapWand` fields with `serde(default)`. |
| TUI behaviour shifts (dark rooms now dark, telepathy shows monsters, line clears on next key, Esc at `--More--` skips, invented stair and door texts disappear) | Intended; listed in commit bodies and `formal-mechanics-spec.md` "Known divergences". |
| Web shows things it never showed (floor objects, traps, remembered map) | Intended; check `docs/agent-and-mcp-integration.md` examples of `ascii_map`. |
| Symbol collisions (`+` spellbook vs closed door, `.` ice vs floor) | Browser colours by kind string, not by character; farlook explains the ambiguity like C. |
| `play.js` untested | Logic stays in Rust behind wasm methods covered by `cargo test`; `node --check` plus manual check. |
| Merge conflicts in `main.rs`, `play.js`, i18n `ALL_KEYS` | Strict order (plan), additive edits, D6 ownership. |

## 7. Exit criteria

Rows to flip, with the item that flips them. A row becomes ✅ only with a test that pins the C behaviour; evidence cites `file:line`.

| File | Row | Now | Target | Item |
|---|---|---|---|---|
| hero.md | Diagonal movement through doorways forbidden | ❌ | ✅ | doors-diagonal |
| hero.md | Peaceful swap / displacing pets | ✅ | ✅ (note: refused diagonally at doors) | doors-diagonal |
| flow-ui.md | `^P` prevmsg | ❌ | ✅ | messages |
| flow-ui.md | new row: `--More--` paging, every message of a turn shown (topl.c:205, :251) | — | ✅ | messages |
| flow-ui.md | Map memory (remembered tiles out of sight) | ❌ | ✅ | map-memory |
| objects-dungeon.md | Map memory → split: (a) remembered terrain, objects, known traps | ❌ | ✅ | map-memory |
| objects-dungeon.md | new row (b): remembered unseen-monster marker `I` (display.c:377) | — | ❌ | map-memory |
| flow-ui.md | Monster glyphs | 🟡 | ✅ | glyphs |
| flow-ui.md | new row: Object and terrain glyphs (include/defsym.h) | — | ✅ | glyphs |
| flow-ui.md | `a` apply, `d` drop, `e` eat, `r` read, `w` wield, `#rub` | 🟡 | ✅ | item-prompts |
| flow-ui.md | new row: eat/offer from the floor (eat.c:3584 floorfood) | — | ❌ | item-prompts |
| flow-ui.md | `q` quaff | ✅ | ✅ (evidence updated) | item-prompts |
| flow-ui.md | `#offer`, `Q` quiver, `i` inventory, Fixed inventory letters | 🟡 | 🟡 (evidence and reasons updated) | item-prompts |
| flow-ui.md | `z` zap | 🟡 | ✅ | zap-choice |
| objects-dungeon.md | Wand selection when zapping | 🟡 | ✅ | zap-choice |
| flow-ui.md | `;` glance / farlook | ❌ | ✅ | look |
| flow-ui.md | `:` look here | ❌ | ✅ | look |
| flow-ui.md | "You see here X" on stepping onto items | ❌ | ✅ (note: frontends only) | look |
| flow-ui.md | `&` whatdoes / `/` whatis → split: `/` whatis | ❌ | 🟡 (single shot, no menu) | look |
| flow-ui.md | split: `&` whatdoes | — | ❌ | look |
| flow-ui.md | Farlook / `;` / `/` / `:` | ❌ | 🟡 (because of `/`) | look |
| objects-dungeon.md | Vision / line of sight | 🟡 | 🟡 (not in these items) | — |

After all seven items, the roadmap's M1 exit criterion still has two open rows: "Vision / line of sight" and the `/` whatis menu. The roadmap M1 entry stays "in progress" and these become follow-up items M1.8 (vision.c lit rooms and `clear_path`) and M1.9 (`/` menu), each with its own spec. The final task updates the roadmap M1 section to say exactly that.
