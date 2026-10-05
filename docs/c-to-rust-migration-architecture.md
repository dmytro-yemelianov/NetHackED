# NetHack C to Rust (NetHackED) Migration Architecture

This document specifies the technical architecture for migrating the 250,000-line C codebase in `NetHack-5.0.0/` to safe, idiomatic, deterministic Rust (**NetHackED**).

---

## 1. Architectural Paradigm Shift

| Domain Area | NetHack 5.0.0 (C99 Legacy) | NetHackED (Idiomatic Rust) | Rationale & Advantage |
| :--- | :--- | :--- | :--- |
| **Global State** | Ambient globals: `ga`..`gz`, `svb`..`svy`, `u`, `level` in `NetHack-5.0.0/include/decl.h`. | Explicit `World` struct passed as `&mut World` through pipeline stages. | Re-entrant, thread-safe, allows multiple games in one process, headless test runs. |
| **Entity Storage** | Intrusive linked lists (`nmon`, `nobj`), raw pointers, `union vptrs`. | Generational arena storage using typed handles (`slotmap::SlotMap<ItemId, Item>`). | Eliminates dangling pointers, use-after-free, pointer aliasing, and borrow checker conflicts. |
| **Tile Bitfields** | Overloaded 5-bit flags in `struct rm` (`NetHack-5.0.0/include/rm.h`). | Disjoint algebraic enums (`Tile`, `DoorState`). | Prevents semantic collisions between secret doors, walls, and altars by construction. |
| **Object Data** | Monolithic `struct obj` with overloaded signed integer `spe`. | Typed enums (`ItemKind`, `ItemData`) with specific payloads. | Makes illegal states unrepresentable (e.g. wand charges cannot be stored on armor). |
| **Game Output** | Direct side-effecting `pline()` calls embedded deep in mechanics. | Structured event stream (`Vec<GameEvent>`) emitted to presentation subscriber. | UI-agnostic core; permits TUI, Web, headless AI training, and unit test assertions without mocking. |
| **RNG & Replay** | Global `rnd()` / `rn2()` using system C PRNG with platform variance. | Single seeded `rand_chacha::ChaCha8Rng` (rand 0.9) owned by the world. | Reproducible runs from a seed; the world JSON round-trip preserves RNG state. Cross-platform replay is not yet tested. |
| **Level Scripts** | Embedded Lua 5.x C API hooks in `NetHack-5.0.0/src/nhlua.c`. | Not implemented: no Lua or scripting engine (`mlua`/`rhai` are not dependencies). Special levels are built by hand-written Rust generators in `nethacked-dungeon`. | No embedded interpreter; level layouts are plain Rust code. |

---

## 2. NetHackED Crate Workspace Graph

```
                                  [nethacked-tui]
                           (Ratatui / Crossterm Frontend)
                                        │
                                        v
                                 [nethacked-sim]
                         (Headless Simulation & Replay)
                                        │
                                        v
                               [nethacked-dungeon]
                     (Grid, BSP Rooms, Mazes, Level Scripts)
                                        │
                                        v
                                [nethacked-arena]
                     (Generational SlotMaps & Entity IDs)
                                        │
                                        v
                                [nethacked-core]
                 (Pure Mechanics: BUC, Inventory, Energy, Combat)
                                        │
                                        v
                                [nethacked-types]
                       (Shared Coordinates, Enums, Primitives)
```

### Crate Responsibilities

1. **`nethacked-types`**:
   * Fundamental coordinate types `Coord`, `Direction`, `Alignment`.
   * Error types, dice roll primitives, and bitmask wrappers.
   * Zero external dependencies.

2. **`nethacked-core`** (Implemented in [crates/nethacked-core/](../crates/nethacked-core/)):
   * The pure mathematical core verified in Lean 4.
   * `buc.rs`: BUC state transitions and water dipping.
   * `inventory.rs`: Recursive weight calculations, Bag of Holding scaling, encumbrance tiers.
   * `energy.rs`: Deterministic turn scheduler and speed points (`NORMAL_SPEED = 12`).
   * `grid.rs`: Door state machines, accessibility checks, secret door revelation.
   * `combat.rs`: Melee attack resolution, descending AC target numbers, lethal damage clamping.
   * `ast.rs`: Deep embedding of actions, atomic effects, and small-step operational semantics.

3. **`nethacked-arena`**:
   * Generational indices (`ItemId`, `ActorId`, `LevelId`) via `slotmap`.
   * Solves the container-in-inventory ownership graph without `Rc<RefCell<T>>` or `unsafe`.

4. **`nethacked-dungeon`**:
   * Classical procedural map generation (BSP rooms, random walks, maze generation).
   * Field of View (FOV) shadowcasting and lighting octants.
   * Special levels (Sokoban, Mines, Minetown, Quest, Gehennom, Sanctum) are built by hand-written Rust generators; there is no `.des`/Lua script parser and no `rhai`/`mlua` engine (planned, not implemented).

5. **`nethacked-sim`**:
   * The headless simulation harness.
   * Seeded deterministic simulation (single ChaCha8 RNG); world state serializes to JSON including RNG state. A keystroke replay logger and cross-platform replay testing are not yet implemented.
   * Property-based verification harness running `proptest` suites against Lean 4 reference models.

6. **`nethacked-tui`**:
   * Decoupled terminal interface built with `ratatui` and `crossterm`.
   * Listens to `GameEvent` channels and formats messages, inventory menus, status bars, and map glyphs.

---

## 3. Entity Graph: From Raw Pointers to Generational Handles

### The C Problem (`NetHack-5.0.0/include/obj.h`)
```c
struct obj {
    struct obj *nobj;      /* Next in linked list */
    union vptrs v;         /* Overloaded: v_nexthere, v_ocontainer, v_ocarry */
    struct obj *cobj;      /* First item in container */
    ...
};
```
In C, when an item is moved from a monster's inventory to the ground, or placed inside a nested chest, three separate raw pointers must be updated in lockstep. Dangling pointers and double-frees were frequent sources of bugs.

### The Rust Solution (`nethacked-arena`)
```rust
use slotmap::{new_key_type, SlotMap};

new_key_type! {
    pub struct ItemId;
    pub struct ActorId;
    pub struct LevelId;
}

pub struct ItemEntity {
    pub name: String,
    pub kind: ItemKind,
    pub location: ItemLocation,
    pub buc: Buc,
}

pub enum ItemLocation {
    Floor(Coord),
    InContainer(ItemId),
    CarriedBy(ActorId),
    Limbo,
}

pub struct World {
    pub items: SlotMap<ItemId, ItemEntity>,
    pub actors: SlotMap<ActorId, ActorEntity>,
}
```

* **Safety Guarantee**: Looking up an invalid or deleted item handle returns `None` rather than triggering undefined behavior or memory corruption.
* **Borrow Checker Harmony**: The `World` struct owns the arena. Functions take `&World` or `&mut World` and pass lightweight 64-bit copyable keys (`ItemId`), eliminating borrow checker conflicts.

---

## 4. Decoupling Engine Logic from Presentation

### The C Problem (`NetHack-5.0.0/src/uhitm.c`)
```c
if (hmon(mon, weapon, aatyp)) {
    pline("You hit %s!", mon_nam(mon));
    show_glyph(mon->mx, mon->my, ...);
}
```
Mechanics, English grammar inflection, and window redrawing are permanently entwined.

### The Rust Solution (Event-Driven Architecture)
```rust
pub enum GameEvent {
    MeleeHit {
        attacker: ActorId,
        target: ActorId,
        damage: u32,
        is_lethal: bool,
    },
    DoorOpened {
        coord: Coord,
    },
    ItemPickedUp {
        actor: ActorId,
        item: ItemId,
    },
}

impl World {
    pub fn step(&mut self, action: ActionAst) -> (StepResult, Vec<GameEvent>) {
        let mut events = Vec::new();
        // Pure mechanics update...
        events.push(GameEvent::MeleeHit { ... });
        (StepResult::Ok, events)
    }
}
```
* The TUI / Frontend subscribes to `Vec<GameEvent>` and formats messages, plays audio cues, and updates the display accordingly.
* Headless test runners can assert on emitted events with zero terminal or display dependencies.
