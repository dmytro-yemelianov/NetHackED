# NetHackED Rule Packs Guide

Rule packs allow modders, tournament organizers, and researchers to modify game data—such as monster stats and attacks, item attributes and costs, and role starting inventories—without recompiling the engine or breaking replay determinism.

---

## Overview

A **Rule Pack** is a self-contained bundle that describes modifications relative to the canonical **vanilla ruleset**.
Packs can:
- **Patch** existing entities (e.g. increase monster HP, change item cost or damage dice).
- **Add** new monsters or items (with unique names and valid attributes).
- **Remove** non-engine-required entities from generation and catalogs.
- **Provide localized names** for new entities in supported languages (English and Ukrainian).

Packs **cannot**:
- Introduce arbitrary code execution. NetHackED rule packs are pure data (declarative TOML or JSON).
- Break replay determinism. Given the same ruleset and seed, simulation execution is 100% reproducible bit-for-bit across platforms.
- Add new roles or attack types in P1 (roles may only be patched; new roles and mechanics knobs arrive in P2).

---

## Directory Layout

A rule pack source directory has the following structure:

```
my-pack/
├── pack.toml          # Required: pack metadata (id, name, version, base = "vanilla", description)
├── monsters.toml      # Optional: monster patches, new monsters, monster removals
├── items.toml         # Optional: item patches, new items, item removals
├── roles.toml         # Optional: role starting inventory patches
├── mechanics.toml     # Optional: reserved for P2 mechanics knobs (must be empty or omitted in P1)
└── i18n/              # Optional: localized entity name translations
    └── uk.toml        # Key-value mappings: "english name" = "українська назва"
```

### 1. `pack.toml`
Defines the pack identifier, human-readable name, semantic version, and base:
```toml
id = "hard-mode"
name = "Hard Mode"
version = "0.1.0"
base = "vanilla"
description = "A harder NetHackED challenge pack with buffed jackals and a new dire jackal"
```
> [!NOTE]
> In Phase 1, `base` must be `"vanilla"`. Deriving from other packs is reserved for future extensions.

### 2. `monsters.toml`
Contains a list of `[[monster]]` tables.

#### Patching an existing monster:
Specify the target monster's `name` and only the fields you wish to override. Unspecified fields retain their vanilla values.
```toml
[[monster]]
name = "jackal"
attacks = [{ at = "Bite", ad = "Phys", n = 1, d = 4 }]
```

#### Adding a new monster:
Set `new = true` and provide all required monster fields:
```toml
[[monster]]
name = "dire jackal"
new = true
glyph = "d"
base_hp = 12
max_hp = 12
ac = 6
level = 2
speed = 14
alignment = "Neutral"
intrinsics = {}
attacks = [{ at = "Bite", ad = "Phys", n = 2, d = 4 }]
size = "Medium"
peaceful_by_default = false
always_hostile = true
maligntyp = 0
msound = "Other"
is_human = false
is_unique = false
mindless = false
ai_behavior = "MeleeHunter"
abilities = []
```

#### Removing a monster:
Set `remove = true`:
```toml
[[monster]]
name = "newt"
remove = true
```
> [!WARNING]
> Engine-required monsters (such as quest leaders, quest guardians, quest nemeses, and `shopkeeper`) cannot be removed. Attempting to remove an engine-required monster fails validation.

### 3. `items.toml`
Contains a list of `[[item]]` tables for patching, adding (`new = true`), or removing (`remove = true`) items.
```toml
[[item]]
name = "leather armor"
cost = 2
```

### 4. `roles.toml`
Contains a list of `[[role]]` tables. Roles cannot be added or removed in P1, only patched:
```toml
[[role]]
name = "Valkyrie"
starting_items = [
    { item = "long sword", spe = 1 },
    { item = "potion of healing" },
    { item = "food ration" },
]
```

### 5. `i18n/<lang>.toml`
Flat key-value mappings of English names to translations:
```toml
"dire jackal" = "лютий шакал"
```

---

## Validation Rules

When a pack is validated (`nethacked-pack validate`) or built (`nethacked-pack build`), the engine verifies:
1. **Unique Names**: Monster and item names must be unique within their catalogs.
2. **Target Existence**: Patches and removals must reference existing entities.
3. **Engine-Required Invariants**: Engine-required monsters (`shopkeeper`, `priest`, quest leaders/guardians/nemeses) and engine-required items (`amulet of yendor`, quest artifacts) cannot be removed.
4. **Valid References**: Starting items in `roles.toml` and quest monsters must exist in the resolved ruleset.
5. **Numerical Ranges**:
   - `level`: 0 to 49
   - `speed`: 0 to 60
   - `ac`: -20 to 20
   - `base_hp` / `max_hp`: 1 to 10,000 (with `base_hp <= max_hp`)
   - `attacks`: `n` and `d` in 1..=255 (special passive/gaze/magic attacks may be 0d0)
   - `weight`: 0 to 10,000
   - `cost`: 0 to 1,000,000
   - `glyph`: single printable ASCII character
6. **Unknown Fields**: Unknown fields in TOML files are rejected immediately with file and field diagnostics.
7. **i18n Coverage**: Added entities without corresponding translations in present i18n files trigger warnings.

---

## Pack Distribution: `.nhpack` Files

While pack authors edit human-readable TOML directories, packs can be compiled into `.nhpack` files:
```bash
cargo run -p nethacked-pack -- build packs/examples/hard-mode -o hard-mode.nhpack
```

An `.nhpack` file is a deterministic, canonical JSON file containing:
- `format`: Pack format version (`1`).
- `manifest`: Pack metadata (`id`, `name`, `version`, `base`, `description`).
- `ruleset`: Complete resolved ruleset data.
- `hash`: Cryptographic SHA-256 digest (`sha256:<hex>`) of the canonical ruleset JSON.

If an `.nhpack` file is tampered with or corrupted, `load_nhpack` detects the hash mismatch and refuses to load it.

---

## `nethacked-pack` CLI Reference

The workspace includes a command-line tool `nethacked-pack`:

### `new`
Scaffold a new rule pack directory with commented template files:
```bash
cargo run -p nethacked-pack -- new packs/my-pack --id my-pack --name "My Pack"
```

### `validate`
Check a pack directory or `.nhpack` file for syntax errors, missing fields, range violations, or broken references:
```bash
cargo run -p nethacked-pack -- validate packs/examples/hard-mode
```

### `build`
Compile and validate a pack directory into a distribution `.nhpack` file and print its SHA-256 hash:
```bash
cargo run -p nethacked-pack -- build packs/examples/hard-mode -o hard-mode.nhpack
```

### `diff`
Display deterministic, field-by-field differences between two rulesets (supports `vanilla`, directory, or `.nhpack`):
```bash
cargo run -p nethacked-pack -- diff vanilla hard-mode.nhpack
```

### `export-vanilla`
Export the built-in vanilla ruleset as a clean directory of TOML files:
```bash
cargo run -p nethacked-pack -- export-vanilla packs/vanilla-exported
```

### `schema`
Generate JSON Schema files (`pack.schema.json`, `monsters.schema.json`, `items.schema.json`, `roles.schema.json`) for IDE auto-completion and linting:
```bash
cargo run -p nethacked-pack -- schema -o schemas/
```

### `simulate`
Run autonomous agent simulations across roles and seeds to stress-test game balance and check for panics or crashes:
```bash
cargo run -p nethacked-pack -- simulate hard-mode.nhpack --seeds 5 --turns 500
```

### `install`, `list`, `info`
Install packs into a local packs directory (`$NETHACKED_PACKS_DIR`, else `~/.nethacked/packs`) so tools can refer to them by id:
```bash
cargo run -p nethacked-pack -- install packs/examples/hard-mode   # validates, builds, copies <id>-<version>.nhpack
cargo run -p nethacked-pack -- list                               # id, version, hash, path
cargo run -p nethacked-pack -- info hard-mode                     # manifest, hash, counts, diff size vs vanilla
```
`install` refuses to overwrite an installed pack that has the same id and version but a different hash, unless you pass `--force`. Pack `id` and `version` must be safe slugs (`[A-Za-z0-9][A-Za-z0-9._-]*` and `[A-Za-z0-9][A-Za-z0-9.+-]*`), because they become file names.

---

## Rule Packs in the Browser

The developer web client includes a pack manager at [`packs.html`](https://dmytro-yemelianov.github.io/NetHackED/packs.html). It runs the same Rust pack library compiled to WebAssembly, so a pack it builds is byte-identical to the CLI's. It can:
- load vanilla, the bundled example packs, or your own uploads (`.nhpack`, pack `.toml` files, or a pack folder);
- show monsters, items and roles, validation diagnostics, and a field-level diff against vanilla;
- edit patches in a form (typed from the pack JSON schemas) or as raw TOML, rebuilding live, then save the pack or download the `.nhpack`.

![Pack manager: Hard Mode vs vanilla](images/pack-manager-diff.png)

To play on a pack in the browser, use **Play with this pack** in the manager, or the clean terminal's URL option: `https://nethacked.yemelianov.dev/?pack=hard-mode`.

---

## Using Rule Packs in the TUI

To launch the NetHackED Terminal User Interface with a rule pack:

```bash
# Using a compiled .nhpack file
cargo run --bin nethacked -- --pack hard-mode.nhpack

# Using a pack directory directly
cargo run --bin nethacked -- --pack packs/examples/hard-mode

# Using an installed pack by id (highest installed version wins)
cargo run --bin nethacked -- --pack hard-mode
```

When a custom pack is active, its name is displayed in the bottom status line:
```
Hero:Neu Dlvl:1  $:0  HP:12(12) Pw:1(1) AC:10  T:1    Wield:none [Hard Mode]
```

---

## Save File Compatibility & Determinism

Every NetHackED save file records a `ruleset_ref` containing:
- `id`: The pack identifier.
- `version`: The pack version string.
- `hash`: The cryptographic SHA-256 ruleset hash (or `"vanilla"` for vanilla).

When restoring a saved game, the engine checks that the active ruleset matches the save file's `ruleset_ref`. If there is a mismatch, the engine refuses to load the save with a `RulesetMismatch` error, preventing corrupted game state or replay divergence.
