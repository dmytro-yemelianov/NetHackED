# Rule Packs and the Settings Editor — Design

Date: 2026-10-04
Status: Approved in conversation (goals: modding and RL equally; UI: web editor + CLI;
depth: data + typed knobs, no custom code)
Phases: P1 Ruleset + pack format + CLI · P2 typed mechanics knobs · P3 web editor ·
P4 RL / benchmark integration. This document is the umbrella design; each phase gets its own
spec and plan. P1 spec: [2026-10-04-rule-packs-p1-design.md](2026-10-04-rule-packs-p1-design.md).

## Goal

Let people change NetRust's game rules without touching Rust: edit monster, item, role and
mechanics data in a **rule pack**, validate it, preview it, and run it in every frontend (TUI,
web, Python Gym, MCP/GraphQL, benchmark). The same pack is a modding unit ("DLC") and an RL
experiment variable (curricula, ablations, domain randomisation).

## Non-goals

- Custom code in packs (no scripting, WASM plugins or Lua). Packs are data plus typed knobs.
- New behaviour kinds. A pack can add a monster that bites for 3d6 with fire, but not a new
  attack *type* the engine doesn't implement.
- New roles/quests/dungeon branches (need level content; later, maybe never).
- Networked pack registry. Packs are files; sharing is out of band.

## Concepts

| Term | Meaning |
|---|---|
| **Ruleset** | The fully resolved, immutable game data the simulation reads: monsters, items, roles, races, pantheons, quest config, armor table, mechanics parameters. `Arc<Ruleset>`. |
| **Vanilla** | The ruleset built from today's static tables (NetHack 5.0 C values). Always available, id `vanilla`. |
| **Pack** | A directory of TOML files describing changes on top of a base (vanilla or another pack). Human-edited. |
| **`.nrpack`** | The built artefact: canonical JSON of the resolved ruleset + manifest + content hash. What engines load. |
| **Knob** | A typed, range-checked mechanics parameter with the C value as default (P2). |

## Architecture

```
pack dir (TOML) ──netrust-pack build──▶ .nrpack (canonical JSON, sha256)
                                            │
            vanilla (static tables) ─┐      ▼
                                     ├─▶ Ruleset (Arc, immutable) ──▶ SimulationWorld
                         .nrpack ────┘                                 (holds Arc + RulesetRef)
```

- **`netrust-data`** gains owned definition types (`MonsterDef`, `ItemDef`, `RoleDef`, …) and
  `Ruleset`. The static tables stay as the source of vanilla; `Ruleset::vanilla()` converts them
  once (lazily, cached). The C-table tests keep pinning vanilla.
- **Engines read the ruleset, not statics.** `SimulationWorld` holds `Arc<Ruleset>`
  (`#[serde(skip)]`) plus a serialized `RulesetRef { id, version, hash }`. Every lookup that
  today calls `BESTIARY`/`ITEM_CATALOG`/`monster_archetype_by_name`/… goes through the world's
  ruleset. Pure `netrust-core` functions keep taking values, not tables.
- **Saves are tied to their ruleset.** Loading a save checks `RulesetRef` against the supplied
  ruleset (hash match); old saves without a ref load as vanilla.
- **New crate `netrust-pack`** (lib + `netrust-pack` binary): parse, merge, validate, build,
  diff, schema export, simulate. Frontends depend on its lib only to load `.nrpack` files.
- **Determinism:** same `.nrpack` hash + seed + actions ⇒ same game. The hash is over canonical
  JSON (sorted keys, no floats in data; knobs use integers/rationals).

## Pack format

```
my-pack/
  pack.toml        # id, name, version, base = "vanilla" | "<pack id>@<version>", engine = ">=0.x"
  monsters.toml    # [[monster]] entries: patch existing by name, or add with `new = true`
  items.toml
  roles.toml       # patch existing roles (stats, starting items/spe, alignment record)
  mechanics.toml   # P2 knobs: [hunger] [prayer] [luck] [encumbrance] [shop] [spawn] ...
  i18n/uk.toml     # names/messages for added entries (optional; English fallback)
```

- Patch semantics: an entry with an existing `name` changes only the fields it lists; `new = true`
  adds an entry (name must be unique); `remove = true` removes it (validation fails if the engine
  needs it — e.g. quest monsters, shopkeeper). Lists (attacks, starting items) replace wholesale.
- Stable keys are names (the sim already resolves records by name). Renaming = remove + add.

## Validation (three layers)

1. **Schema** — JSON Schema generated with `schemars` from the def types; editors and CI use it.
2. **Semantic** — referential integrity (starting items exist, quest monsters exist), ranges
   (dice ≥ 1, AC in −20..=20, level 0..=50, weights ≥ 0), engine-required entries present, knob
   ranges, unique names, i18n coverage warnings.
3. **Behavioural** — `netrust-pack simulate`: seed sweep (N seeds × T turns per role with the
   built-in agent) reporting panics, early deaths, mean depth/turns vs vanilla. Used by CI for
   shipped packs and by the editor's preview.

## Typed knobs (P2)

`MechanicsParams` grouped by system (hunger thresholds, prayer timeout, luck periods, weight cap
formula coefficients, shop factors, breath cooldown, alignment limits and initial record, quest
gates, mysterious-force odds, spawn rates once spawning exists). Each knob: type, unit, range,
C default with citation, doc string. Pure core functions take the relevant params struct.
**Lean:** proofs stay about vanilla constants where they are; where a theorem is naturally
parametric it is generalised with the range as hypothesis; the knob's validated range is chosen
so documented invariants still hold (or the guide says the theorem covers defaults only).

## Web editor (P3)

In `web/` (vanilla JS like the current UI, no framework): load vanilla or a `.nrpack`, schema-
driven forms per table, inline validation (WASM-exported validator), diff vs vanilla, live
preview (WASM session with the edited ruleset, play or watch the agent), export `.nrpack` and
pack TOML. WASM gains `validate_pack`, `build_pack`, `new_session_with_ruleset`.

## Consumers (P1 TUI, rest P4)

TUI `--pack file.nrpack`; WASM session constructor takes ruleset JSON; Gym
`NetRustEnv(pack="file.nrpack" | None, pack_sampler=...)` for domain randomisation; MCP/GraphQL
`reset` accepts a pack id from a configured pack dir; benchmark `--pack` and report includes the
pack hash.

## Risks

- Large mechanical refactor (static → ruleset) touches most sim modules and ~150 test helper
  calls; done once in P1 with `Ruleset::vanilla()` default so tests change minimally.
- Closed ID enums (`MonsterSpeciesId`, `ItemKindId`) remain for engine-special entities; packs
  address entries by name. Added entries have no enum id (lookups by name already).
- Duplicate sources of truth (armor AC in `ITEM_CATALOG` and `C_ARMOR`; role names in TUI menu,
  quest config, dungeon) are unified in P1 so packs can't make them disagree.
