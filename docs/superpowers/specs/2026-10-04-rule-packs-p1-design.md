# Rule Packs P1 — Ruleset, Pack Format, CLI — Design

Date: 2026-10-04
Status: Approved by delegation (umbrella design approved in conversation)
Umbrella: [2026-10-04-rule-packs-design.md](2026-10-04-rule-packs-design.md)
Branch: `feat/rule-packs-p1` (from `main` after D2)

## Goal

The simulation reads all game data from an `Arc<Ruleset>` instead of global statics; vanilla
behaviour is bit-for-bit unchanged; a pack directory can be built into a `.nhpack`, validated,
diffed and played in the TUI.

## Non-goals (later phases)

Mechanics knobs (P2 — P1 ships an empty `mechanics` section reserved in the format), web
editor (P3), Gym/MCP/GraphQL/benchmark/WASM pack loading (P4), adding roles/races.

## 1. Owned definitions and `Ruleset` (`nethacked-data`)

- New owned types deriving `Serialize, Deserialize, JsonSchema, Clone, PartialEq`:
  `MonsterDef` (all `MonsterArchetype` fields; `String`/`Vec`), `ItemDef` (all `ItemArchetype`
  fields + armor slot/base AC — single armor source, replacing the separate `C_ARMOR` lookup for
  catalog items), `RoleDef` (stats, starting items by item name + spe, skills, pantheon,
  quest config, initial alignment record = today's 25), `RaceDef`.
- `Ruleset { manifest: PackManifest, monsters: Vec<MonsterDef>, items: Vec<ItemDef>,
  roles: Vec<RoleDef>, races: Vec<RaceDef>, mechanics: MechanicsSection /* empty in P1 */ }`
  with name indexes built on load (`HashMap<String, usize>`, lowercase keys) and accessors
  `monster(&str)`, `item(&str)`, `role(RoleId)`, `race(RaceId)` mirroring today's lookup
  semantics (incl. prefix stripping in `monster_archetype_by_name`).
- `Ruleset::vanilla() -> Arc<Ruleset>` built once from the existing static tables, quest
  config, pantheons, starting skills/spe and `C_ARMOR` (cached in a `OnceLock`). Static tables
  remain, private-ish, as vanilla's source; C-table tests keep pinning them; a new test asserts
  `vanilla()` round-trips (serialize → deserialize → equal) and matches the statics entry by entry.

## 2. Engine plumbing (`nethacked-sim`, `nethacked-agent`, `nethacked-wasm`, `nethacked-tui`)

- `SimulationWorld { ruleset: Arc<Ruleset> /* serde(skip), default vanilla */,
  ruleset_ref: RulesetRef /* serde(default) = vanilla */ }`; constructors take an optional
  ruleset (`new_with_character_and_ruleset`); existing constructors use vanilla.
- Every consumer listed in the consumer map (world.rs, combat.rs, monsters.rs, peace.rs,
  actions/*, bones.rs, agent graphql/mcp/arena, wasm, tui role menu) reads through
  `world.ruleset` (or a passed `&Ruleset`). No production code reads `BESTIARY`,
  `ITEM_CATALOG`, `ROLES`, `RACES`, `C_ARMOR`, `get_role_quest_config` directly after P1
  (enforced by a grep test in CI script `scripts/check-no-static-tables.sh`, allow-listing
  `nethacked-data` and vanilla construction).
- `quest_species_by_name` panics → returns `Result`/falls back with a logged message; validation
  guarantees quest monsters exist.
- Determinism: a golden test runs fixed seeds × roles × N turns with the built-in agent before
  and after the refactor and compares the full event logs / final world hash (recorded at the
  start of P1 on `main`).
- Save/load: `RulesetRef { id, version, hash }`; `SimulationWorld::load(json, &Arc<Ruleset>)`
  errors on hash mismatch (`RulesetMismatch { expected, found }`); missing ref ⇒ vanilla.

## 3. Pack format and `nethacked-pack` crate

- New crate `crates/nethacked-pack` (lib + bin `nethacked-pack`), deps: serde, toml, serde_json,
  schemars, sha2, clap (already in workspace? else add, `--locked` updated).
- Pack dir per umbrella design (`pack.toml`, `monsters.toml`, `items.toml`, `roles.toml`,
  `mechanics.toml` reserved, `i18n/<lang>.toml`). Base: `vanilla` only in P1 (pack-on-pack later).
- Merge: patch by name (listed fields only), `new = true` adds, `remove = true` removes; lists
  replace wholesale; unknown fields are errors (`deny_unknown_fields`) with file:line spans
  (toml spans) in messages.
- Validation (semantic): unique names; references (starting items, quest leader/guardian/
  nemesis monsters, shopkeeper/priest/watchman/Medusa/etc. engine-required list) exist; ranges
  (level 0..=49, speed 0..=60, AC −20..=20, dice n,d 1..=255 / 0d0 only for AT_NONE-like,
  weight 0..=10000, cost 0..=1_000_000); i18n coverage → warnings.
- `.nhpack` = canonical JSON `{ format: 1, manifest, ruleset, hash }`; hash = sha256 over the
  canonical ruleset JSON (sorted keys). Loading verifies format and hash.
- CLI: `new <dir>` (skeleton with commented examples), `validate <dir|file>`,
  `build <dir> -o file.nhpack`, `diff <a> <b>` (vanilla allowed as `vanilla`; per-entry field
  diffs), `export-vanilla <dir>` (full vanilla as a pack dir — round-trips to the vanilla hash),
  `schema -o dir` (JSON Schema files), `simulate <pack> --seeds N --turns T` (seed sweep with
  the built-in agent per role: panics, deaths, mean depth/turns, compared to vanilla).
- Example pack in `packs/examples/hard-mode/` (stronger jackals, cheaper armor, a new
  "dire jackal") built and simulated in CI.

## 4. TUI

`nethacked-tui --pack file.nhpack` loads and verifies the pack, shows its name in the status line,
role menu built from the ruleset (removes the duplicated `ROLE_ROWS` names).

## 5. Docs

`docs/rule-packs.md` (author guide: format, patch semantics, validation messages, CLI);
architecture article/README pointers; formal-mechanics spec note that proofs cover vanilla.

## Testing

- Golden determinism test (vanilla before/after refactor).
- Ruleset: vanilla round-trip; lookups equal statics for every name; prefix-stripping parity.
- Pack: merge unit tests (patch/add/remove/unknown field/span error), validation tests per rule,
  hash stability (same input ⇒ same hash; key order irrelevant), export-vanilla → build ⇒ vanilla
  hash, `.nhpack` tamper detection.
- Sim with a pack: patched jackal dice used in combat; added monster spawnable by name and
  resolves attacks; save with pack, load with vanilla ⇒ `RulesetMismatch`.
- CLI integration tests (assert_cmd or std::process) for each subcommand.
- CI: existing gates + `check-no-static-tables.sh` + example pack build/validate/simulate (small N).

## Risks

- Refactor breadth: mitigated by golden determinism test and vanilla default constructors.
- Lifetime churn from `&'static` returns: accessors return `&MonsterDef` tied to the ruleset;
  callers holding `&'static` across world mutation are rewritten to clone small data or re-look-up.
- WASM size from schemars/toml: `nethacked-pack` is not a wasm dependency in P1 (WASM loads
  `.nhpack` JSON via `nethacked-data` only in P4).
