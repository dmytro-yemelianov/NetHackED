# Master Architectural Roadmap to Complete NetHack Parity

## 1. Executive Summary & Parity Status

NetRust has migrated and formally verified the core foundation of NetHack:
- **Foundational Core**: Turn-based energy loop, coordinate geometry, dungeon branch topology (Main, Mines, Sokoban, Gehennom, Astral, Quest).
- **Core Systems**: 20-slot inventory, AC/to-hit combat mathematics, prayer & piety, altar sacrifice, shopkeeper economics & anger mechanics, stealth & telepathy, monster AI behaviors, NetHack 3.6+ Sokoban level generation, and Class Quest trials.
- **Formal Verification**: Lean 4 models of selected mechanics are machine-checked (simplified models; not formally linked to the Rust code) ([NetMechanics.lean](../NetMechanics.lean)) with zero `sorry`s.

To achieve **100% canonical feature parity** with NetHack (3.6 / 3.7 / 5.0 architecture), the remaining missing systems are organized into **4 Milestones** comprising **9 distinct subsystems**.

```
+-----------------------------------------------------------------------------------+
|                        NETRUST CANONICAL PARITY ROADMAP                           |
+-----------------------------------------------------------------------------------+
| Milestone A: Magic & Transformation                                              |
|   1. Polymorph, Lycanthropy & Polypiling                                         |
|   2. Scroll of Genocide & Magic Marker Inscription                               |
+-----------------------------------------------------------------------------------+
| Milestone B: Combat Mechanics & Status Systems                                    |
|   3. Status Afflictions & Biological Timers (Petrification, Stun, Hallucination)   |
|   4. Ranged Combat, Quivers (`#quiver`) & Steeds / Riding (`#ride`)               |
|   5. Weapon Skills & Proficiency Mastery (`#enhance`)                            |
+-----------------------------------------------------------------------------------+
| Milestone C: Hazard Physics, Secrets & Special Branches                          |
|   6. Canonical Dungeon Trap Physics, Searching (`s`) & Disarming                 |
|   7. Special Boss Branches (Wizard's Tower, Vlad's Tower, Fort Ludios)            |
+-----------------------------------------------------------------------------------+
| Milestone D: Metabolism & Voluntary Conducts                                     |
|   8. Corpse Nutrition, Intrinsic Absorption & Cannibalism                         |
|   9. Formally Tracked Conducts (Pacifist, Vegan, Atheist, Illiterate)             |
+-----------------------------------------------------------------------------------+
```

---

## 2. Milestone A: Iconic Magic & Transformation

### 2.1 Polymorph, Lycanthropy & Polypiling
- **Scope & Mechanics**:
  - **Hero Polymorph**: Wand/spell/trap/potion of polymorph transforms hero into a monster form for $N$ turns (or until HP $\le 0$). Hero inherits monster attacks, size, flying/swimming, but cannot wear armor unsuitable for form. HP drops to 0 revert hero back to original form with overflow damage.
  - **Monster Polymorph**: Beams/traps mutate monster into a random monster of equivalent or varied difficulty tier. Uniques (e.g. Vlad, Medusa, Rodney) cannot be polymorphed.
  - **Polypiling**: Items grouped in a stack hit by a polymorph beam have a chance ($20\%$) to shudder and transform into other items of the same general class (e.g. wands to wands, potions to potions), or degenerate into rocks/golems (system shock).
  - **Were-Creature Lycanthropy**: Infection from werejackal, wererat, or werewolf bite. Periodically forces transformation under full moon or low HP; curable via sprig of wolfsbane or holy water.
- **Crate Allocation**:
  - `netrust-types`: `PolymorphForm`, `LycanthropyState`, `PolypileResult`.
  - `netrust-core`: Polymorph reversion logic, shape-fit equipment validation, system-shock RNG checks.
  - `netrust-sim`: Beam and trap interaction, equipment auto-unequip on shape shift, monster stat swap.
- **Lean 4 Verification Target** (`NetMechanics/Polymorph.lean`):
  - `theorem poly_reversion_preserves_base_stats`: Zero HP in poly form restores base hero HP without instant death unless overflow exceeds base HP.
  - `theorem polypile_preserves_or_reduces_count`: Stack polypiling preserves total item count or strictly reduces it (system shock).
  - `theorem unique_entities_poly_invariant`: Uniques and artifact items are invariant under polymorph beams.

### 2.2 Scroll of Genocide & Magic Marker Inscription
- **Scope & Mechanics**:
  - **Genocide Scroll**: Blessed genocide eliminates an entire monster class (`@`, `d`, `L`, etc.) or targeted species from the current game and all future spawns. Cursed genocide summons 4-6 of the chosen species adjacent to the hero. Self-genocide triggers instant defeat (or polymorph if wearing amulet of life saving).
  - **Magic Marker**: Carries limited ink charges ($1 \dots 100$). Allows hero to write known scrolls and spellbooks with ink consumption scaling with item value.
- **Crate Allocation**:
  - `netrust-types`: `GenocideScope { Class(char), Species(MonsterId) }`, `InkCharges(u8)`.
  - `netrust-core`: Spawn filtration predicate: `is_genocided(monster_id) -> bool`. Ink calculation equations.
  - `netrust-sim`: `Action::Read` handling genocide wipes, `Action::Write` invocation.
- **Lean 4 Verification Target** (`NetMechanics/Genocide.lean`):
  - `theorem genocided_species_cannot_spawn`: $\forall m, \text{is\_genocided}(m) \implies \text{can\_spawn}(m) = \text{false}$.
  - `theorem genocide_conduct_monotonic`: Reading scroll of genocide permanently disables the `genocideless` conduct flag.

---

## 3. Milestone B: Combat Mechanics & Status Systems

### 3.1 Status Afflictions & Biological Timers
- **Scope & Mechanics**:
  - **Petrification Countdown**: Cockatrice/Medusa gaze/hiss or handling cockatrice corpse barehanded starts a 5-turn stone timer. Cured by lizard corpse or acidic potion; failure results in turn-to-stone death.
  - **Confusion & Stun**: Hero movements deviate randomly; spellcasting fails; reading scrolls produces corrupted effects.
  - **Hallucination**: Monster glyphs and names scramble randomly each turn; visual distortion in TUI/WASM.
  - **Sliming**: Green slime attacks turn hero into green slime over 10 turns unless burned by fire.
- **Crate Allocation**:
  - `netrust-types`: `StatusAffliction { Petrification(u8), Sliming(u8), Confused(u8), Stunned(u8), Hallucinating(u8) }`.
  - `netrust-core`: Affliction tick resolution, movement vector scrambling, cure conditions.
  - `netrust-sim`: Per-turn decay in simulation loop; terminal death triggers upon timer expiry.
- **Lean 4 Verification Target** (`NetMechanics/StatusAffliction.lean`):
  - `theorem petrification_timer_decrements_strictly` (with `petrification_reaches_zero_is_fatal`): Petrification timer strictly decrements per turn; non-zero timer reaching 0 triggers stone death unless cured.
  - `theorem lizard_cure_restores_unpetrified`: Consumption of lizard corpse transitions state from `Petrification(n)` to clean.

### 3.2 Ranged Combat, Quivers & Steeds
- **Scope & Mechanics**:
  - **Quiver & Throwing**: Slot for active ammunition (`#quiver`). Bows launch arrows, crossbows launch bolts, slings launch rocks/gems with appropriate range, damage multipliers, and breakage probabilities.
  - **Steeds & Riding (`#ride`)**: Saddle item applied to tame large quadruped (horse, warhorse, dragon). Mounting enables shared movement speed, shared encumbrance, lance jousting bonus damage, and riding skill checks.
- **Crate Allocation**:
  - `netrust-types`: `QuiverSlot`, `SaddleEquipment`, `MountState`.
  - `netrust-sim`: Projectile trajectory collision detection with floor drop on miss/break; mounted movement delegation.
- **Lean 4 Verification Target** (`NetMechanics/Ranged.lean`):
  - `theorem projectile_stops_at_obstacle`: Projectiles stop at the first non-passable tile or target entity.
  - `theorem mount_speed_cost_monotonic` (closest existing; the `min(hero_cost, mount_cost)` dominance statement below is planned): When mounted, hero action point cost derives from `min(hero_cost, mount_cost)`.

### 3.3 Weapon Skills & Proficiency Mastery (`#enhance`)
- **Scope & Mechanics**:
  - Skill levels: `Unskilled`, `Basic`, `Skilled`, `Expert` (plus `Master`/`Grand Master` for Bare-Hands / Martial Arts).
  - Practical usage of weapons of a given skill class accrues experience; skill slots gained at hero level-up are allocated via `#enhance`.
  - Skill level provides additive bonuses to to-hit (+0 to +3) and damage (+0 to +3).
- **Crate Allocation**:
  - `netrust-types`: `SkillClass`, `ProficiencyLevel`, `SkillTree`.
  - `netrust-core`: Skill progression formula, hit/damage modifier lookup.
- **Lean 4 Verification Target** (`NetMechanics/Skills.lean`):
  - `theorem skill_to_hit_monotonic` and `skill_damage_monotonic`: Higher skill levels strictly monotonically increase or preserve to-hit and damage bonuses.

---

## 4. Milestone C: Hazard Physics, Secrets & Special Branches

### 4.1 Canonical Dungeon Trap Physics, Searching & Disarming
- **Scope & Mechanics**:
  - **12 Trap Types**: Arrow trap, Dart trap, Rock fall, Pit / Spiked Pit, Teleport trap, Fire trap, Level-teleporter, Polymorph trap, Anti-magic trap, Sleeping gas, Rust trap, Web.
  - **Detection & Disarming**: Passive searching based on Wisdom/Perception; active searching (`s` action, consumes turn); `#untrap` / disarming with lockpicks or bare hands.
- **Crate Allocation**:
  - `netrust-types`: `TrapType`, `TrapState { Hidden, Revealed, Disarmed }`.
  - `netrust-dungeon`: Procedural trap distribution per branch depth.
  - `netrust-sim`: Movement trigger handlers, trap discovery actions.
- **Lean 4 Verification Target** (`NetMechanics/Traps.lean`):
  - `theorem non_flying_triggers_floor_trap` and `triggering_reveals_hidden_trap`: Stepping on revealed or hidden trap triggers specific effect, revealing hidden traps.
  - `theorem flying_avoids_floor_traps`: Entities with intrinsic or equipment `Flying` bypass pits, bear traps, and spiked pits.

### 4.2 Special Boss Branches
- **Scope & Mechanics**:
  - **The Wizard's Tower (Gehennom)**: 3-level enclosed tower in Gehennom surrounded by moat and false towers. Houses the Wizard of Yendor guarding the Book of the Dead. Includes Rodney's resurrection harass timer once disturbed.
  - **Vlad's Tower**: 3-level vertical tower in Gehennom housing Vlad the Impaler guarding the Candelabrum of Invocation.
  - **Fort Ludios**: Portal level rich in gold, soldiers, and Croesus guarding treasure vaults.
  - **Rogue Level**: Retro ASCII homage floor with classic Rogue generation, no corridors, and strict grid movement.
- **Crate Allocation**:
  - `netrust-types`: `BranchId { WizardsTower, VladsTower, FortLudios, RogueLevel }`.
  - `netrust-dungeon`: Special pre-fabricated map builders and moat generators.
  - `netrust-sim`: Rodney resurrection scheduler and invocation item assembly.
- **Lean 4 Verification Target** (`NetMechanics/InvocationBranches.lean`):
  - `theorem invocation_trio_completeness`: Ascending Astral requires exactly Book of the Dead (Wizard), Candelabrum (Vlad), and Bell of Opening (Quest).

---

## 5. Milestone D: Metabolism & Voluntary Conducts

### 5.1 Corpse Nutrition, Intrinsic Absorption & Cannibalism
- **Scope & Mechanics**:
  - **Nutritional Tiers & Rotting**: Corpses decay into tainted meat after $N$ turns unless preserved by tinning kit or ice box. Eating tainted corpse causes food poisoning.
  - **Intrinsic Absorption**: Consuming specific corpses has a discrete probability of granting permanent intrinsics (e.g. fire resistance from red dragons, poison resistance from snakes, telepathy from floating eyes, giant strength boost).
  - **Cannibalism & Taboos**: Eating human meat as a human incurs immediate luck penalty, alignment drop, and induces telepathy loss / aggravate monster.
- **Crate Allocation**:
  - `netrust-types`: `CorpseAge`, `NutritionValue`, `DietaryTaboo`.
  - `netrust-core`: Intrinsic roll determination, cannibalism violation check.
  - `netrust-sim`: `Action::Eat` state transitions and decay ticks.
- **Lean 4 Verification Target** (`NetMechanics/Nutrition.lean`):
  - `theorem fresh_corpse_provides_nutrition`: Ingesting fresh corpse increases hero nutrition by `corpse.nutrition`.
  - `theorem cannibalism_detects_same_race`: Consuming corpse where `corpse.race == hero.race` unconditionally triggers cannibalism flag.

### 5.2 Formally Tracked Voluntary Conducts
- **Scope & Mechanics**:
  - Canonical NetHack tracks voluntary challenges:
    1. **Pacifist**: Never directly kill any creature (pet kills permitted).
    2. **Vegan / Vegetarian**: Never consume animal products / meat.
    3. **Atheist**: Never pray, sacrifice at altars, or consult deities.
    4. **Illiterate**: Never read any scroll, book, engrave, or T-shirt.
    5. **Genocideless**: Never read or cast genocide.
    6. **Polypileless / Polyselfless**: Never polypile items or polymorph hero.
    7. **Wishless**: Never wish from wand of wishing or magic lamp.
- **Crate Allocation**:
  - `netrust-types`: `ConductTracker` struct with boolean bitflags.
  - `netrust-sim`: Invariant violation monitors attached to action handlers.
  - `netrust-agent`: Heuristic policy filters ensuring AI agent respects active conduct constraints.
- **Lean 4 Verification Target** (`NetMechanics/Conducts.lean`):
  - `theorem conduct_violations_are_irreversible`: Once any conduct flag transitions to false, no subsequent game action can restore it.

---

## 6. Execution Order & Timeline

| Phase | Milestone | Subsystems Included | Primary Deliverables | Target Verification |
|---|---|---|---|---|
| **Phase 1** | **Milestone A** | Polymorph, Lycanthropy, Polypiling, Genocide, Magic Marker | `NetMechanics/Polymorph.lean`, `netrust-core/src/polymorph.rs`, polypiling simulation, genocide item drop filters | 0 `sorry`s, 10+ property tests |
| **Phase 2** | **Milestone B** | Afflictions (Petrification/Stun/Slime), Ranged/Quiver, Steeds, Weapon Skills | `NetMechanics/Affliction.lean`, `NetMechanics/Skills.lean`, projectile animations in WASM, status HUD | 0 `sorry`s, 15+ property tests |
| **Phase 3** | **Milestone C** | 12 Traps + Searching/Untrapping, Wizard's Tower, Vlad's Tower, Fort Ludios | `NetMechanics/Traps.lean`, map generation for Gehennom towers, Rodney resurrection daemon | 0 `sorry`s, dungeon tests |
| **Phase 4** | **Milestone D** | Corpse Nutrition Intrinsics, Cannibalism, Conduct Tracker & Pacifist AI | `NetMechanics/Conducts.lean`, `NetMechanics/Nutrition.lean`, end-game high score conduct audit | 0 `sorry`s, complete parity |

---

## 7. Verification Standards

Every single phase adheres to NetRust's strict mathematical and software engineering gates:
1. **Formal Specification**: Lean 4 theorem suite in `NetMechanics/` with zero `sorry`s (`lake build` must exit 0).
2. **Deterministic Simulation**: All state transitions purely deterministic given `RngSeed`.
3. **Property Testing**: `proptest` validation ensuring invariant preservation across $10,000$ randomized test iterations.
4. **WASM & TUI Parity**: Web interface ([web/index.html](../web/index.html)) and terminal interface ([crates/netrust-tui](../crates/netrust-tui)) reflect all new actions and status indicators.
5. **No Regressions**: Full workspace test suite (`cargo test --workspace`) and demo binary (`netrust-agent demo`) pass cleanly at every commit.
