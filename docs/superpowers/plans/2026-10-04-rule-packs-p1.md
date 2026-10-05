# Rule Packs P1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The simulation reads all game data from an `Arc<Ruleset>` (vanilla behaviour unchanged, proven by a golden determinism test), and a TOML pack directory can be built into a hashed `.nhpack`, validated, diffed, simulated and played in the TUI.

**Architecture:** Owned, serde/schemars-derivable definition types and `Ruleset` live in `nethacked-data` (which gains a `nethacked-core` dependency for the armor table and quest config); `Ruleset::vanilla()` is built once from today's static tables. `SimulationWorld` holds `Arc<Ruleset>` (not serialized) plus a serialized `RulesetRef`. A new `nethacked-pack` crate (lib + `nethacked-pack` binary) parses, merges, validates, hashes and builds packs; the TUI loads `.nhpack` via its lib.

**Tech Stack:** Rust 1.88.0 (pinned), serde/serde_json, new deps: `toml` 0.8, `schemars` 0.8 (feature-gated in data/types), `sha2` 0.10, `clap` 4 (derive) — only in `nethacked-pack`/`nethacked-tui`; proptest 1.5.

**Spec:** `docs/superpowers/specs/2026-10-04-rule-packs-p1-design.md` (umbrella: `docs/superpowers/specs/2026-10-04-rule-packs-design.md`)

## Global Constraints

- Branch `feat/rule-packs-p1`. Never commit to `main`.
- Every task keeps green: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets --locked -- -D warnings`; `cargo test --workspace --exclude nethacked-py --locked`; `cargo build -p nethacked-wasm --target wasm32-unknown-unknown --locked`; `cargo check -p nethacked-py`; `bash scripts/check-doc-links.sh`; `lake build`. When adding deps, update `Cargo.lock` in the same commit (run without `--locked` once, then gates with `--locked`).
- **Vanilla behaviour is bit-for-bit unchanged**: the golden determinism test from Task 1 passes unmodified after every later task. Its expected constants are never edited after Task 1.
- `nethacked-wasm` must not depend on `toml`, `clap`, `schemars` or `nethacked-pack` (schemars only behind a non-default `schema` feature).
- No `HashMap`/`HashSet` iteration may influence game state or output ordering (indexes are lookup-only; outputs use `Vec` order or `BTreeMap`).
- Canonical JSON = `serde_json::to_vec(&serde_json::to_value(x)?)` with serde_json's default `Map` (BTreeMap, sorted keys; the `preserve_order` feature must stay off — a test asserts it). No floats in definition types.
- Lookups keep today's semantics: case-insensitive names; `monster(name)` strips a leading `"hostile "` and maps `"ghost of …"` to `ghost`.
- Borrowing pattern for sim code that mutates the world while reading data: `let rs = Arc::clone(&self.ruleset);` then borrow from `rs`.
- Existing tests may change only by setup (e.g. constructor names); assertions are never weakened. List adjusted tests in commit bodies.
- New serialized fields `#[serde(default)]` with a load test.
- Docs: `docs/formal-mechanics-spec.md` stays truthful; `docs/lean4-verification-guide.md` two-table rule if touched (module table 4 cols, matrix 3 cols).
- Commit trailer (verbatim):
  ```
  Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_019cXhwXQqEEChd362EQ6RKp
  ```

## Review Focus

1. A pack that patches a monster the engine needs (shopkeeper, quest leader) with zero-dice attacks or level 0 — validation must reject invalid ranges with a message naming file, entry and field; the sim must not panic if such a ruleset is loaded programmatically. Test in Task 5 (validation) and Task 3 (`quest_species_by_name` no panic).
2. A save made with pack A loaded with vanilla (or pack B) — must fail with `RulesetMismatch`, never silently play with different data; a pre-P1 save (no `ruleset_ref`) loads as vanilla. Test in Task 3.
3. Two builds of the same pack with keys/entries written in different orders — identical hash; a hand-edited `.nhpack` (one value changed) — rejected on load. Test in Task 5.
4. `export-vanilla` then `build` — yields exactly the vanilla hash (round-trip fidelity of every field, incl. enums and optional fields). Test in Task 6.
5. Pack-added monster/item (no enum id) used in play: spawn by name, attacks resolved, starting item granted, i18n missing → English fallback, no panic in any `get_*`/`expect` path. Test in Task 7.

---

### Task 1: Golden determinism baseline

**Files:**
- Create: `crates/nethacked-agent/tests/golden_determinism.rs`

**Interfaces:** Produces: test `golden_vanilla_runs_are_unchanged` with frozen constants (never edited later).

- [ ] **Step 1: Write the test**

```rust
//! Frozen fingerprints of vanilla play. Recorded on main before the rule-pack
//! refactor (P1); every later change must keep them identical.
use nethacked_agent::arena::run_seed_games;
use nethacked_data::roles::RoleId;

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

const ROLES: [RoleId; 9] = [/* all nine RoleId variants, in declaration order */];

#[test]
fn golden_vanilla_runs_are_unchanged() {
    let mut fingerprints = Vec::new();
    for seed in 1..=4u64 {
        let results = run_seed_games(seed, &ROLES, 400);
        let json = serde_json::to_vec(&results).expect("RunResult serializes");
        fingerprints.push(fnv1a(&json));
    }
    assert_eq!(fingerprints, EXPECTED_RUNS);
}

#[test]
fn golden_vanilla_event_logs_are_unchanged() {
    // For each role: new_with_character(seed 7), apply a fixed scripted action
    // sequence (move in each of 8 directions ×5, search ×10, rest ×20, then
    // repeat) for 300 steps via step_player_action; fingerprint the
    // serde_json of world.event_log and of (hp, max_hp, ac, depth, turn, coord).
    // Compare to EXPECTED_LOGS.
}
```

If `RunResult` doesn't derive `Serialize`, fingerprint `format!("{results:?}")` instead (Debug output of plain structs is stable). Use whatever public action API the sim exposes (`step_player_action(ActionAst::…)`); write the script as a `const` array.

- [ ] **Step 2:** Run with placeholder constants `[0; 4]` / `[0; 9]`, read the actual values from the failure output, paste them in as `EXPECTED_RUNS` / `EXPECTED_LOGS`.
- [ ] **Step 3:** Run twice more → PASS both times (determinism across runs); also `cargo test --release -p nethacked-agent --test golden_determinism` → PASS (no debug/release divergence).
- [ ] **Step 4:** Full gates; commit `test(agent): golden determinism fingerprints for vanilla play`.

### Task 2: Owned definitions and `Ruleset::vanilla()` (`nethacked-data`)

**Files:**
- Modify: `crates/nethacked-data/Cargo.toml` (add `nethacked-core` dep; optional `schemars` behind feature `schema`), `crates/nethacked-types/Cargo.toml` (optional `schemars`, feature `schema`; derive `JsonSchema` via `cfg_attr` on types used in defs: `Attack`, `AttackType`, `DamageType`, `Intrinsics`, `Alignment`, `MonsterAbility`, `ItemClass`, …)
- Create: `crates/nethacked-data/src/ruleset.rs` (types + vanilla + lookups), `crates/nethacked-data/tests/ruleset_vanilla.rs`
- Modify: `crates/nethacked-data/src/lib.rs` (export)

**Interfaces (produces):**

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct MonsterDef {
    #[serde(default)] pub id: Option<MonsterSpeciesId>, // None for pack-added
    pub name: String, pub glyph: char, pub base_hp: u32, pub max_hp: u32, pub ac: i32,
    pub level: u32, pub speed: u32, pub alignment: Alignment, pub intrinsics: Intrinsics,
    pub attacks: Vec<Attack>, pub size: MonsterSize, pub peaceful_by_default: bool,
    pub always_hostile: bool, pub maligntyp: i8, pub msound: MonsterSound,
    pub m2_race: Option<RaceId>, pub is_human: bool, pub is_unique: bool, pub mindless: bool,
    pub ai_behavior: AiBehavior, pub abilities: Vec<MonsterAbility>,
}
pub struct ItemDef {   // same derives
    #[serde(default)] pub id: Option<ItemKindId>,
    pub name: String, pub class: ItemClass, pub weight: u32, pub cost: u32,
    pub damage_small: (u32, u32), pub damage_large: (u32, u32), pub ac_bonus: i32,
    pub is_container: bool, pub is_bag_of_holding: bool, pub oc_magic: bool,
    pub wand_dir: Option<WandDir>, pub nutrition: u32,
    /// Armor slot and C base AC (from `nethacked_core::ac` table); `None` for non-armor.
    pub armor: Option<ArmorDef>,
}
pub struct ArmorDef { pub slot: ArmorSlot, pub base_ac: i32 }
pub struct StartingItem { pub item: String, pub spe: Option<i8> }
pub struct QuestDef { pub leader: String, pub nemesis: String, pub guardian: String,
                      pub artifact: String, pub home_desc: String, pub goal_desc: String }
pub struct RoleDef {
    pub id: RoleId, pub name: String, pub base_hp: u32, pub ac: i32, pub speed: u32,
    pub default_alignment: Alignment, pub starting_items: Vec<StartingItem>,
    pub skills: Vec<(SkillClass, SkillLevel)>, pub pantheon: [String; 3],
    pub quest: Option<QuestDef>, pub initial_alignment_record: i32, // vanilla 25
}
pub struct RaceDef { pub id: RaceId, pub name: String, pub intrinsics: Intrinsics }
pub struct PackManifest { pub id: String, pub name: String, pub version: String,
                          pub base: String, pub description: String }
#[serde(deny_unknown_fields)] pub struct MechanicsSection {} // P2
pub struct RulesetRef { pub id: String, pub version: String, pub hash: String }

pub struct Ruleset {
    pub manifest: PackManifest, pub monsters: Vec<MonsterDef>, pub items: Vec<ItemDef>,
    pub roles: Vec<RoleDef>, pub races: Vec<RaceDef>, pub mechanics: MechanicsSection,
    #[serde(skip)] index: RulesetIndex, // lowercase name -> position; rebuilt by `reindex()`
}
impl Ruleset {
    pub fn vanilla() -> Arc<Ruleset>;                       // OnceLock-cached
    pub fn from_parts(manifest, monsters, items, roles, races, mechanics) -> Ruleset; // builds index
    pub fn reindex(&mut self);                              // after deserialize
    pub fn monster(&self, name: &str) -> Option<&MonsterDef>;   // prefix-stripping semantics
    pub fn monster_by_id(&self, id: MonsterSpeciesId) -> Option<&MonsterDef>;
    pub fn item(&self, name: &str) -> Option<&ItemDef>;
    pub fn item_by_id(&self, id: ItemKindId) -> Option<&ItemDef>;
    pub fn role(&self, id: RoleId) -> Option<&RoleDef>;
    pub fn race(&self, id: RaceId) -> Option<&RaceDef>;
    pub fn monster_class_of(&self, name: &str) -> Option<char>;
    pub fn create_monster_record(&self, name: &str, coord: Coord) -> Option<ActorRecord>;
    pub fn create_item_record(&self, name: &str, loc: ItemLocation, buc: Buc) -> Option<ItemRecord>;
    pub fn spawn_player_character(&self, cfg: &CharacterConfig, coord: Coord, arena: &mut EntityArena)
        -> (ActorId, Vec<ItemId>);
}
```

`create_*_record` must produce records equal to today's `create_monster_record`/`create_item_record`/`spawn_player_character` (incl. `initial_wand_charges` by id; pack-added wands with `id: None` use the `wand_dir` rule without the wishing special case). Existing free functions stay (vanilla helpers used by tests).

- [ ] **Step 1: Failing tests** in `tests/ruleset_vanilla.rs`:
  - for every `BESTIARY` entry: `vanilla().monster(name)` fields equal the static entry; same for `ITEM_CATALOG` (+ `armor` equals `(armor_slot(name), armor_base_ac(name))` for `ItemClass::Armor`), `ROLES` (+ skills = `starting_skills`, spe = `starting_item_spe`, pantheon = `get_pantheon_for_role`, quest = `get_role_quest_config`, `initial_alignment_record == 25`), `RACES`;
  - `create_monster_record`/`create_item_record`/`spawn_player_character` parity with the free functions for every id/role/race (compare records with `==` or serde_json values);
  - lookup parity: `"Hostile Djinni"`, `"ghost of Bob"`, `"JACKAL"`, unknown name → `None`;
  - serde round-trip: `serde_json::from_value::<Ruleset>(to_value(&*vanilla()))` then `reindex()` → equal, lookups work;
  - `serde_json` `preserve_order` off: `to_string(&json!({"b":1,"a":2}))` == `{"a":2,"b":1}`.
- [ ] **Step 2:** Run → FAIL (types missing).
- [ ] **Step 3:** Implement `ruleset.rs`; implement `PartialEq` for `Ruleset` ignoring `index`.
- [ ] **Step 4:** Tests PASS; `cargo build -p nethacked-wasm --target wasm32-unknown-unknown --locked` (schemars must not enter the wasm graph: `cargo tree -p nethacked-wasm -e normal | grep -c schemars` = 0).
- [ ] **Step 5:** Full gates; commit `feat(data): owned definitions and vanilla Ruleset`.

### Task 3: Simulation reads the ruleset; saves record it

**Files:**
- Modify: `crates/nethacked-sim/src/world.rs`, `combat.rs`, `monsters.rs`, `peace.rs`, `bones.rs`, `turns.rs` (if it reads tables), `actions/*.rs` (stairs, items, economy, movement, inventory), `crates/nethacked-sim/tests/*` (setup only)
- Create: `crates/nethacked-sim/tests/ruleset_world_tests.rs`

**Interfaces:**
- Consumes: Task 2 `Ruleset` API.
- Produces:
  ```rust
  // SimulationWorld
  #[serde(skip, default = "Ruleset::vanilla")] pub ruleset: Arc<Ruleset>,
  #[serde(default = "RulesetRef::vanilla")] pub ruleset_ref: RulesetRef,
  pub fn new_with_character_and_ruleset(seed: u64, cfg: CharacterConfig, rs: Arc<Ruleset>, rref: RulesetRef) -> Self;
  pub fn from_save_json(json: &str, rs: Arc<Ruleset>, rref: &RulesetRef) -> Result<Self, LoadError>;
  pub fn to_save_json(&self) -> Result<String, serde_json::Error>;
  pub enum LoadError { Json(String), RulesetMismatch { expected: RulesetRef, found: RulesetRef } }
  pub fn spawn_monster_by_name(&mut self, name: &str, preferred: Coord) -> Option<ActorId>;
  ```
  `RulesetRef::vanilla()` = `{ id: "vanilla", version: <crate version>, hash: "vanilla" }` (the real hash is computed by `nethacked-pack`; vanilla uses the sentinel so `nethacked-data` needs no sha2). `new_with_character`/`new_with_seed` delegate with vanilla. `spawn_monster_near(MonsterSpeciesId, …)` delegates to `spawn_monster_by_name` using the id's vanilla name.

- [ ] **Step 1: Failing tests** (`ruleset_world_tests.rs`):
  - a ruleset clone of vanilla with jackal attacks set to `[Bite Phys 3d6]`: a jackal adjacent to a hero with AC 10 over 200 steps deals at least one hit > 2 (impossible in vanilla 1d2) — proves combat reads `world.ruleset`;
  - same ruleset with `"dire jackal"` added (`id: None`, copy of jackal fields with new name): `spawn_monster_by_name("dire jackal", …)` returns an actor; stepping 50 turns adjacent never panics and the hero takes damage;
  - save/load: world with ruleset ref `{id:"a",…,hash:"h1"}` saved → `from_save_json(json, vanilla, &RulesetRef::vanilla())` → `Err(RulesetMismatch{..})`; same ref → `Ok` and `ruleset` equals supplied; a JSON with the `ruleset_ref` key removed loads as vanilla;
  - a ruleset where the Archeologist quest leader species name doesn't exist: entering the quest home level logs a message and does not panic (`quest_species_by_name` returns `Option`).
- [ ] **Step 2:** Run → FAIL.
- [ ] **Step 3:** Implement: add fields; replace every production read of `BESTIARY`, `ITEM_CATALOG`, `monster_archetype_by_name`, `item_archetype_by_name`, `get_monster_species`, `get_item_archetype`, `create_monster_record`, `create_item_record`, `spawn_player_character`, `get_role`, `get_race`, `starting_skills`, `get_role_quest_config`, `armor_base_ac`, `armor_slot` (for catalog items; keep the core heuristic as fallback for unknown names), `monster_class_of`, `INITIAL_ALIGNMENT_RECORD` (now `role.initial_alignment_record`) in `nethacked-sim/src` with `world.ruleset` calls. Use the `Arc::clone` borrowing pattern. Remove `world.rs`'s `INITIAL_ALIGNMENT_RECORD` const (or keep as vanilla's source in data).
- [ ] **Step 4:** Golden test (Task 1) PASS unchanged; new tests PASS; whole suite PASS.
- [ ] **Step 5:** Full gates; commit `refactor(sim): read game data from the world's Ruleset; saves record it`.

### Task 4: Agent, WASM and TUI read the ruleset; static-table guard

**Files:**
- Modify: `crates/nethacked-agent/src/{graphql.rs,mcp.rs,commands.rs,arena.rs,session.rs}`, `crates/nethacked-agent/src/bin/demo.rs`, `crates/nethacked-wasm/src/lib.rs`, `crates/nethacked-tui/src/main.rs` (role menu from `Ruleset::vanilla().roles` instead of `ROLE_ROWS` names; keep key bindings)
- Create: `scripts/check-no-static-tables.sh`; Modify: `.github/workflows/*.yml` (run it in the doc-links/lint job)

**Interfaces:** Consumes Task 3. Produces `AgentSession::new_with_ruleset(seed, cfg, Arc<Ruleset>, RulesetRef)`.

- [ ] **Step 1: Failing check:** write the script:
  ```bash
  #!/usr/bin/env bash
  # Production code outside nethacked-data must read game data through a Ruleset.
  set -euo pipefail
  pattern='\b(BESTIARY|ITEM_CATALOG|ROLES|RACES|C_ARMOR|get_role_quest_config|monster_archetype_by_name|item_archetype_by_name|get_monster_species|get_item_archetype|spawn_player_character)\b'
  hits=$(grep -rnE "$pattern" crates/*/src --include=*.rs \
    | grep -v '^crates/nethacked-data/' \
    | grep -v '^crates/nethacked-core/src/ac.rs' \
    | grep -v '^crates/nethacked-core/src/quest.rs' \
    | grep -v '// static-ok' || true)
  if [ -n "$hits" ]; then echo "Static game-data access outside a Ruleset:"; echo "$hits"; exit 1; fi
  echo "No static game-data access outside nethacked-data."
  ```
  Run → FAIL listing agent/wasm/tui hits (and any sim leftovers).
- [ ] **Step 2:** Port each hit to `Ruleset::vanilla()` or the session's ruleset; GraphQL/MCP/WASM role/race/bestiary/catalog JSON output must stay byte-identical (add a test per surface comparing the JSON before/after: capture expected JSON from current code first, store as a test fixture string or compare against serializing the static table).
- [ ] **Step 3:** Script PASS; golden PASS; wire into CI.
- [ ] **Step 4:** Full gates; commit `refactor(agent,wasm,tui): read game data through Ruleset; CI guard against static tables`.

### Task 5: `nethacked-pack` library — format, merge, validate, hash

**Files:**
- Modify: root `Cargo.toml` (member), Create: `crates/nethacked-pack/{Cargo.toml,src/lib.rs,src/format.rs,src/merge.rs,src/validate.rs,src/nhpack.rs}`, `crates/nethacked-pack/tests/pack_lib.rs`

**Interfaces (produces):**

```rust
pub struct PackDir { pub manifest: PackToml, pub monsters: Vec<MonsterPatch>, pub items: Vec<ItemPatch>,
                     pub roles: Vec<RolePatch>, pub i18n: BTreeMap<String, BTreeMap<String,String>> }
pub fn read_pack_dir(dir: &Path) -> Result<PackDir, PackError>;           // TOML parse; deny_unknown_fields
pub fn resolve(pack: &PackDir, base: &Ruleset) -> Result<Ruleset, PackError>; // patch/add/remove
pub fn validate(rs: &Ruleset) -> Report;  // Report { errors: Vec<Diagnostic>, warnings: Vec<Diagnostic> }
pub struct Diagnostic { pub file: String, pub entry: String, pub field: Option<String>, pub message: String }
pub fn ruleset_hash(rs: &Ruleset) -> String;                // "sha256:<hex>" of canonical JSON
pub struct NhPack { pub format: u32 /* 1 */, pub manifest: PackManifest, pub ruleset: Ruleset, pub hash: String }
pub fn build(dir: &Path) -> Result<(NhPack, Report), PackError>; // read+resolve+validate; Err if errors
pub fn write_nhpack(p: &NhPack, out: &Path) -> io::Result<()>;    // canonical JSON
pub fn load_nhpack(path: &Path) -> Result<(Arc<Ruleset>, RulesetRef), PackError>; // verifies format+hash, reindexes
pub fn diff(a: &Ruleset, b: &Ruleset) -> Vec<DiffEntry>; // DiffEntry { table, entry, field, before, after } (Value strings), deterministic order
pub enum PackError { Io(String), Toml { file: String, message: String }, Resolve(Diagnostic), Invalid(Report), Hash { expected: String, found: String }, Format(u32) }
```

TOML shapes: `pack.toml` = `id`, `name`, `version`, `base = "vanilla"` (anything else → error "only base = \"vanilla\" is supported in this version"), optional `description`. `monsters.toml` = `[[monster]]` tables of `MonsterPatch` (every `MonsterDef` field as `Option`, plus `name: String`, `new: bool`, `remove: bool`, `deny_unknown_fields`); likewise `[[item]]`, `[[role]]` (roles patch-only, addressed by `name` matching `RoleDef.name`; `new`/`remove` rejected). `mechanics.toml` must be empty or absent (non-empty → error "mechanics knobs arrive in P2"). `i18n/<lang>.toml`: flat `"english name" = "translation"`. TOML errors carry toml's line/column message prefixed with the file name.

Validation rules (each its own test): unique names (monsters, items); patch target exists / `new` name not taken; `remove` of an engine-required monster/item → error (`ENGINE_REQUIRED_MONSTERS`/`ENGINE_REQUIRED_ITEMS` consts in `nethacked-data::ruleset`, compiled by grepping every species/kind the sim references by id or name in Task 3 — include every role's quest leader/guardian/nemesis); starting items exist; quest monsters exist; ranges: level 0..=49, speed 0..=60, AC −20..=20, `base_hp`/`max_hp` 1..=10000 with `base_hp <= max_hp`, attack `n` and `d` 1..=255 (except `AttackType::None` passives may be 0d0), weight 0..=10000, cost 0..=1_000_000, damage dice sides 0..=255 (0 only for non-weapons), `glyph` printable ASCII; warnings: added entries without a translation in each present i18n file.

- [ ] **Step 1: Failing tests** (`tests/pack_lib.rs`, using `tempfile`-free temp dirs under `std::env::temp_dir()/nethacked-pack-test-<test name>`):
  - patch jackal `level = 3` only → resolved jackal level 3, all other fields equal vanilla;
  - add `dire jackal` (all fields) → present, `id: None`; add with missing field → `Resolve` error naming field;
  - remove a non-required monster → absent; remove `shopkeeper` → validation error;
  - unknown field `levle = 3` → `Toml` error containing `monsters.toml` and `levle`;
  - each range rule → error with `file`, `entry`, `field`;
  - hash: same pack with entries/keys reordered → same hash; changing one value → different hash;
  - `load_nhpack` on a file with one value edited → `PackError::Hash`; `format: 2` → `PackError::Format(2)`;
  - `diff(vanilla, resolved)` lists exactly the patched fields; output order deterministic across 10 runs;
  - property test (proptest, 64 cases): random subset of vanilla monsters patched with random in-range `level`/`ac`/`speed` → `validate` has no errors and `diff` reports exactly those changes.
- [ ] **Step 2:** Run → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** PASS; `cargo tree -p nethacked-wasm -e normal | grep -cE 'toml|clap|schemars|nethacked-pack'` = 0.
- [ ] **Step 5:** Full gates; commit `feat(pack): rule pack format, merge, validation and hashed .nhpack`.

### Task 6: `nethacked-pack` CLI

**Files:**
- Create: `crates/nethacked-pack/src/main.rs` (clap derive), `crates/nethacked-pack/src/simulate.rs`, `crates/nethacked-pack/tests/cli.rs`
- Modify: `crates/nethacked-pack/Cargo.toml` (`[[bin]] name = "nethacked-pack"`, deps `clap`, `nethacked-sim`, `nethacked-agent`)

**Interfaces:** Subcommands and exit codes (0 ok, 1 validation/load failure, 2 usage):
- `new <dir> [--id ID] [--name NAME]` — writes `pack.toml` + `monsters.toml`/`items.toml`/`roles.toml` with commented examples (a patch and a `new = true` entry, commented out so the skeleton builds to the vanilla ruleset with the new manifest); refuses a non-empty dir.
- `validate <dir|file.nhpack>` — prints `error:`/`warning:` lines `<file>: <entry>.<field>: <message>`, summary line; exit 1 on errors.
- `build <dir> -o <file.nhpack>` — prints the hash.
- `diff <a> <b>` — `a`/`b` each `vanilla`, a pack dir or a `.nhpack`; one line per change `<table> <entry> <field>: <before> -> <after>`.
- `export-vanilla <dir>` — writes vanilla as a pack dir with every entry fully listed as patches (no-op patches) so `build` reproduces vanilla data; the manifest id is `vanilla-export`.
- `schema -o <dir>` — writes `pack.schema.json`, `monsters.schema.json`, `items.schema.json`, `roles.schema.json` (schemars on the patch types, `schema` feature enabled for data/types).
- `simulate <pack|vanilla> [--seeds N=5] [--turns T=500] [--json]` — for seeds 1..=N × all roles, runs the built-in agent policy (`run_seed_games` equivalent with the ruleset; add `run_seed_games_with_ruleset` in `nethacked-agent::arena`) catching panics (`std::panic::catch_unwind`); prints per role: games, deaths, mean depth, mean turns, panics; and the same for vanilla side by side; exit 1 if any panic.

- [ ] **Step 1: Failing tests** (`tests/cli.rs` via `std::process::Command::new(env!("CARGO_BIN_EXE_nethacked-pack"))`): each subcommand happy path; `validate` on a broken pack exits 1 and prints the field; `new` then `build` succeeds; `export-vanilla` → `build` → resolved ruleset data equals `Ruleset::vanilla()` data (compare monsters/items/roles/races vectors; manifests differ); `diff vanilla vanilla` prints nothing and exits 0; `simulate vanilla --seeds 1 --turns 50` exits 0 with zero panics; `schema` writes 4 valid JSON files.
- [ ] **Step 2:** FAIL → implement → PASS. Golden PASS.
- [ ] **Step 3:** Full gates; commit `feat(pack): nethacked-pack CLI (new, validate, build, diff, export-vanilla, schema, simulate)`.

### Task 7: TUI `--pack`, example pack, docs

**Files:**
- Modify: `crates/nethacked-tui/Cargo.toml` (dep `nethacked-pack`), `crates/nethacked-tui/src/main.rs` (arg `--pack <file.nhpack>`; on error print message and exit 1 before entering raw mode; status line shows pack name when not vanilla; role menu from the pack's roles; i18n fallback to English for pack-added names)
- Create: `packs/examples/hard-mode/{pack.toml,monsters.toml,items.toml,roles.toml,i18n/uk.toml}`, `docs/rule-packs.md`, `crates/nethacked-pack/tests/example_pack.rs`
- Modify: CI workflow (build + validate + `simulate --seeds 2 --turns 200` the example pack), `README.md` (short "Rule packs" section linking the guide), `docs/architecture.md` if present on main (pointer), `docs/formal-mechanics-spec.md` (note: Lean proofs and C citations describe vanilla; packs may change any data value)

Example pack content: jackal attacks `Bite Phys 1d4`; leather armor cost 2; new monster `dire jackal` (glyph `d`, level 2, speed 14, AC 6, `Bite Phys 2d4`, otherwise jackal's fields) with Ukrainian name `лютий шакал`; Valkyrie `starting_items` adds `food ration` (use an existing catalog item name; check `ITEM_CATALOG`).

- [ ] **Step 1: Failing tests** (`example_pack.rs`): example pack builds with zero errors and zero warnings; a world with the built ruleset spawns `dire jackal` by name and its record has level 2 / AC 6; Valkyrie starts with the extra item; TUI arg parsing unit test (factor `parse_args(&[String]) -> Result<TuiArgs, String>` out of `main` if needed): `--pack missing.nhpack` → `Err` containing the path.
- [ ] **Step 2:** FAIL → implement → PASS. Golden PASS.
- [ ] **Step 3:** Write `docs/rule-packs.md`: what a pack is; directory layout; patch/add/remove semantics with examples; field reference (link the generated schema command); validation rules list; CLI reference; determinism and save compatibility (`RulesetMismatch`); limits (no new attack types/roles; mechanics knobs in P2). Doc links PASS.
- [ ] **Step 4:** Full gates; commit `feat(tui): --pack; example hard-mode pack; rule pack author guide`.

---

## Self-review notes

- Spec §1 → Task 2; §2 → Tasks 3–4 (golden: Task 1; save ref: Task 3; guard script: Task 4); §3 → Tasks 5–6 (example pack in CI: Task 7); §4 → Task 7; §5 → Task 7; Testing list → distributed (golden T1, round-trip/parity T2, merge/validation/hash/tamper T5, export round-trip T6, sim-with-pack T3/T7, CLI T6).
- Spec deviation recorded: TOML spans come from `toml`'s own parse/unknown-field errors (line/column); semantic diagnostics name file + entry + field instead of spans.
- Vanilla `RulesetRef.hash` is the sentinel `"vanilla"` (no sha2 in `nethacked-data`); `nethacked-pack` reports the real vanilla hash for display (`build` of an export prints it).
