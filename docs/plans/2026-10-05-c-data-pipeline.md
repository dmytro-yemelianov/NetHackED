# Plan: NetHack 5.0 C → data → Rust pipeline

Goal: every static fact NetHack keeps in C tables (objects, monsters, generation
weights) reaches the Rust engine as **data**, extracted mechanically and verifiably.
C code is never compiled or executed anywhere; it is only text-expanded at dev time.
No legacy preservation: hand-written catalogs are deleted, goldens re-baselined once.

## 1. Extraction (dev-time, `scripts/extract/`)

- **Preprocessor, not regex.** C tables are macro towers (`WEAPON → OBJECT → BITS`,
  `MON → A/ATTK/SIZ`). A driver `.c` per table defines the header's own mode
  (`OBJECTS_INIT`, `MON` override) and runs `cc -E -P` — a pure text transform with
  exact C macro semantics. Symbolic names (`P_DAGGER`, `IRON`, `CLR_GRAY`, `MR_FIRE`)
  survive because their headers are not included. Replaces the hand-written
  mini-preprocessor of the former `gen-bestiary.py`.
- **Names and ids from C itself:** object `sn` / monster `PM_*` (the C enum names)
  become the stable ids; `OBJ(name,desc)` gives name + unidentified appearance.
- **Fail loud:** every symbolic value goes through an exhaustive mapping; an unknown
  symbol aborts the run with `file:line`. No defaults, no guessing.
- **Invariants checked by the extractor** (taken from C's own init checks):
  exact counts asserted (463 objects, 383 monsters), unique ids,
  per-class probabilities summing to 1000 (`o_init.c`), class markers in order.
- Sources: `objects.h`, `monsters.h`, `mkobj.c` class-probability tables
  (`mkobjprobs`, `boxiprobs`, …), `makemon.c` constants used by `rndmonst`.

## 2. Preservation (committed, reviewable)

- One canonical file per table in `data/nethack-5.0/`: `objects.toml`,
  `monsters.toml`, `gen.toml`, in the **existing rule-pack schema** (`ItemDef`,
  `MonsterDef`) extended with the C fields we now carry (prob, material, color,
  appearance, oc1/oc2, skill, gen flags/frequency, resistances conveyed, …).
- Each record keeps provenance: `src = "include/objects.h:212"`. The file header
  records the C tree hash.
- `extract --check` regenerates into memory and diffs — run in the gate and CI.
- The vanilla ruleset **is** this pack. Editing mechanics data = editing TOML (or
  a pack overlay), never Rust.

## 3. Usage in Rust

- **Single source of truth:** `Ruleset::vanilla()` deserializes the embedded
  `data/nethack-5.0/*.toml` (`include_str!`, parsed once, `OnceLock`). Delete
  `ITEM_CATALOG`, the static `BESTIARY`, and the archetype ↔ def duplication.
- **Ids:** `build.rs` in `nethacked-data` reads the TOML and emits
  `ItemKindId` / `MonsterSpeciesId` (C names → CamelCase, e.g. `WAN_WISHING →
  WanWishing`, `PM_GRID_BUG → GridBug`) plus `ALL` arrays. Code refers to a kind only
  where it has kind-specific *behaviour* (wish wand, cockatrice touch); every
  property comes from the def.
- **Behaviour vs data split:** effects stay Rust (`match` on id or on data tags such
  as `AdType`), with a default arm for kinds not yet modelled. Parity tables track
  which behaviours exist; data completeness is 100% by construction.
- **Generation from data:** port `rndmonst`/`mkobj` class+prob selection (C
  `makemon.c`, `mkobj.c`, cited by line) to read weights from the ruleset, so new
  species/items appear in play with no further code.
- **i18n:** `monsters.uk.tsv` / `objects.uk.tsv` keyed by C id, not English name.

## 4. Steps (each one PR, gate green, inline — no agent fleets)

1. Extractor + `objects.toml` + `monsters.toml` with `--check`; asserts pass.
2. Ruleset loads from TOML; `build.rs` ids; delete static catalogs; migrate
   call sites (~220 item + monster refs, mostly tests → by-id lookups).
   Re-baseline golden fingerprints once, explicitly in the PR.
3. Generation (`rndmonst`, `mkobj` probs) from data; property tests that observed
   frequencies match C weights.
4. i18n by id; parity tables + roadmap updated (objects 463/463 as data).

## Risks

- `cc -E` must exist on dev machines/CI (it does on macOS/Linux runners); the
  committed TOML means builds never need it.
- Old saves/packs break — accepted (no backward compatibility).
- Behaviour gaps become visible (items exist that do little) — tracked, not hidden.

## Status (2026-10-06)

- Steps 1–3 done (PRs #15, #16): objects, monsters, artifacts and the `mkobj.c`
  class tables are extracted; the engine tables are generated; monsters and
  objects are generated from the C weights.
- Ids ended up as table-index newtypes named exactly as the C enums
  (`MonsterSpeciesId::ALIGNED_CLERIC`), not CamelCase enums: open for packs, grep-able
  against C. They serialize as the C name.
- Ukrainian names stay keyed by English name: those names are now generated
  from C, and the i18n coverage tests fail on any untranslated one.
- Behaviour coverage is generated: `scripts/behaviour-coverage.py` →
  [behaviour-coverage.md](../parity/behaviour-coverage.md), checked in CI.

Next: behaviour handlers by tag (AD_* effects, then wand/potion/scroll effects),
picked from the ❌ rows of the coverage report; `mksobj` init; `dat/*.lua` levels.
