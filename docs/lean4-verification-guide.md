# Lean 4 Verification Reference & Rust Bridge

This guide details the formal verification architecture of **NetMechanics** in Lean 4 and specifies the automated bridge to property-based testing in **NetRust**.

---

## 1. Project Organization & Verified Modules

The Lean 4 verification project is defined in [lakefile.toml](../lakefile.toml) and root module [NetMechanics.lean](../NetMechanics.lean). All modules compile with zero errors and zero `sorry` placeholders, and the project uses no axioms beyond Lean's standard ones. The table below lists a representative subset (12 of the 39 modules under `NetMechanics/`); the other modules cover areas such as Nutrition, Religion, Traps, Pet, Quest, Sokoban, Skills and Tournament. The Lean models are deliberately simplified abstractions of NetHack mechanics, not a faithful transcription of NetHack 5.0 (see Section 4, Known limitations).

| Module | Source File | Key Inductive Types | Key Verified Theorems |
| :--- | :--- | :--- | :--- |
| **`NetMechanics.BUC`** | [NetMechanics/BUC.lean](../NetMechanics/BUC.lean) | `BUC`, `WaterType`, `BUCKnowledge` | `buc_not_blessed_and_cursed`<br>`dip_holy_idempotent`<br>`dip_plain_always_uncursed`<br>`uncurse_never_cursed` |
| **`NetMechanics.Inventory`** | [NetMechanics/Inventory.lean](../NetMechanics/Inventory.lean) | `Item`, `ItemKind`, `EncumbranceTier` | `item_weight_ge_zero`<br>`contents_weight_ge_tail`<br>`unencumbered_when_le_cap`<br>`boh_cannot_contain_boh` |
| **`NetMechanics.Energy`** | [NetMechanics/Energy.lean](../NetMechanics/Energy.lean) | `SchedulerState`, `StepAction` | `turn_strictly_increases`<br>`hero_act_reduces_energy`<br>`scheduler_progress` (Progress Theorem) |
| **`NetMechanics.Grid`** | [NetMechanics/Grid.lean](../NetMechanics/Grid.lean) | `Tile`, `DoorState`, `Coord`, `Alignment` | `open_door_is_passable`<br>`secret_door_impassable`<br>`break_door_idempotent`<br>`reveal_secret_door_locked` |
| **`NetMechanics.Combat`** | [NetMechanics/Combat.lean](../NetMechanics/Combat.lean) | `Combatant`, `AttackResult` | `apply_damage_monotone_hp`<br>`apply_damage_preserves_max_hp`<br>`lethal_damage_kills`<br>`miss_leaves_defender_unchanged`<br>`hit_monotone_target_ac`<br>`hit_monotone_luck`<br>`hit_deals_positive_damage`<br>`hero_absorb_le` |
| **`NetMechanics.AST`** | [NetMechanics/AST.lean](../NetMechanics/AST.lean) | `ActionAST`, `EffectAST`, `WorldState` | `wait_consumes_normal_speed`<br>`inflict_damage_preserves_well_formed`<br>`heal_damage_preserves_well_formed` |
| **`NetMechanics.Mines`** | [NetMechanics/Mines.lean](../NetMechanics/Mines.lean) | `LuckstoneCarried` | `luckstone_preserves_positive_luck`<br>`blessed_luckstone_heals_negative_luck`<br>`uncursed_luckstone_freezes_luck`<br>`step_luck_bounds_preserved` |
| **`NetMechanics.Bones`** | [NetMechanics/Bones.lean](../NetMechanics/Bones.lean) | `BonesRecord` | `corrupt_buc_idempotent`<br>`corrupt_buc_cursed_when_roll_nonzero`<br>`corrupt_buc_keeps_original_on_zero_roll`<br>`corrupt_buc_quest_always_cursed` |
| **`NetMechanics.FOV`** | [NetMechanics/FOV.lean](../NetMechanics/FOV.lean) | `HasLOS` | `los_refl`<br>`open_door_transparent`<br>`closed_door_opaque`<br>`secret_door_opaque` |
| **`NetMechanics.Raycast`** | [NetMechanics/Raycast.lean](../NetMechanics/Raycast.lean) | `SurfaceOrientation`, `Velocity`, `BeamRay` | `reflect_involution`<br>`reflect_preserves_speed_sq`<br>`step_decreases_energy`<br>`beam_terminates_after_energy_steps` |
| **`NetMechanics.Engraving`** | [NetMechanics/Engraving.lean](../NetMechanics/Engraving.lean) | `EngravingMedium`, `Engraving` | `burned_engraving_permanent`<br>`blind_monster_ignores_elbereth`<br>`covetous_monster_ignores_elbereth`<br>`arbitrary_text_not_warding`<br>`dust_zero_durability_erased` |
| **`NetMechanics.Identification`** | [NetMechanics/Identification.lean](../NetMechanics/Identification.lean) | `KnowledgeLevel` | `knowledge_le_refl`<br>`knowledge_le_trans`<br>`knowledge_le_antisymm`<br>`learn_type_monotone`<br>`learn_buc_monotone`<br>`identify_fully_monotone`<br>`identify_fully_idempotent` |
| **`NetMechanics.Polymorph`** | [NetMechanics/Polymorph.lean](../NetMechanics/Polymorph.lean) | `FormStats`, `PolyEntity` | `poly_fatal_damage_reverts`<br>`poly_exact_depletion_preserves_base_hp`<br>`poly_non_fatal_damage_preserves_poly`<br>`poly_damage_preserves_base_max_hp` |
| **`NetMechanics.Pathfinding`** | [NetMechanics/Pathfinding.lean](../NetMechanics/Pathfinding.lean) | `MetricState` | `target_is_fixed_point`<br>`descent_step_decreases_distance`<br>`pathfinding_step_bounded` |



---

## 2. Compilation & Verification Workflow

The Lean 4 formalization is fully integrated with Lake:

```bash
# Build all Lean 4 libraries and verify all mathematical proofs
lake build

# Run the demonstration executable (`Main.lean`)
./.lake/build/bin/netmechanics
```

### Verification Kernel Confidence
Unlike unit tests that sample specific inputs, Lean 4 proofs are verified by the **Lean 4 trusted kernel**. When `lake build` exits with code 0 without `sorry`, the theorem holds for **all possible inputs** in the domain.

---

## 3. The Lean 4 to Rust Verification Bridge

The Rust engine is an independent, hand-written implementation. **Nothing machine-links the Lean models to the Rust code**: there is no extraction, translation, or proof that the Rust functions satisfy the Lean theorems. Instead, many Lean theorems are mirrored by hand as Rust property-based tests (`proptest`), so that the Rust code is at least checked against the same stated properties on randomized inputs. `crates/netrust-core/tests/proptest_mechanics.rs` currently holds 88 such `prop_` tests; they cover a selection of the Lean theorems, not all 223. A proptest passing is evidence, not proof, that Rust agrees with the Lean property.

```
    [Lean 4 Formal Specification]               [Rust Implementation]
      NetMechanics/BUC.lean                     crates/netrust-core/src/buc.rs
                 │                                           │
                 v                                           v
       Theorem dip_holy_idempotent                 fn dip_water(...)
    ∀ b, dip(Holy, dip(Holy, b)) = dip(Holy, b)               │
                 │                                           │
                 └───────────────────┬───────────────────────┘
                                     │
                                     v
                       [Rust Property-Based Test]
                      crates/netrust-core/tests/
                     proptest! {
                       fn prop_dip_holy_idempotent(b: Buc) {
                         assert_eq!(dip(Holy, dip(Holy, b)), dip(Holy, b));
                       }
                     }
```

### Theorem-to-Property Mapping Matrix

Rows marked "(no proptest yet)" have no corresponding proptest at present.

| Lean 4 Theorem | Mathematical Property | Synthesized Rust Proptest |
| :--- | :--- | :--- |
| `buc_not_blessed_and_cursed` | $\neg (\text{isBlessed}(b) \land \text{isCursed}(b))$ | (no proptest yet) Intended: for any generated `Buc`, exactly one variant query is true. |
| `dip_holy_idempotent` | $\text{dip}(H, \text{dip}(H, b)) = \text{dip}(H, b)$ | `assert_eq!(dip_water(Holy, dip_water(Holy, b)), dip_water(Holy, b))` for all `b: Buc`. |
| `contents_weight_ge_tail` | $W(x :: xs) \ge W(xs)$ | (no proptest yet) Intended: prepending any random `Item` to a `Vec<Item>` never decreases total weight. |
| `unencumbered_when_le_cap` | $W \le C \implies \mathcal{E}(W, C) = \text{Unencumbered}$ | Generate $W \le C$ with $C > 0$, assert result is `EncumbranceTier::Unencumbered`. |
| `scheduler_progress` | $\Delta E < 0 \lor \Delta \text{turn} > 0$ | Assert that stepping `SchedulerState` either reduces actor action points or increments `turn`. |
| `apply_damage_monotone_hp` | $HP_{\text{after}} \le HP_{\text{before}}$ | Assert that `c.apply_damage(d)` never increases `c.hp` for any $d \in \mathbb{N}$. |
| `hit_monotone_target_ac`, `hit_monotone_luck` | $\text{tmp} = 1 + AC + lvl + luck(L) + spe + skill$; hit $\iff d_{20} < \text{tmp}$ | `prop_to_hit_matches_c_reference`: `to_hit_value`/`attack_hits` equal a reference written from `uhitm.c:365`/`:780` over random inputs and rolls. |
| `hero_absorb_le`, `calculate_damage_pos` | $D' = \max(1, D - \text{rnd}(-AC_{hero}))$ if $AC_{hero} < 0$; $D \ge 1$ on hit | `prop_hero_ac_absorb_matches_c`: `melee_damage`/`hero_damage_after_ac`/`calculate_damage` equal a reference from `uhitm.c:1505`/`mhitu.c:1208`. |
| `monster_to_hit_pos` | $\text{tmp} = \max(1, AC\_VALUE(AC_{hero}) + 10 + m_{lev})$ | `prop_monster_to_hit_matches_c_reference` (reference from `mhitu.c:709`, `hack.h:1538`). |
| `break_door_idempotent` | $\text{break}(\text{break}(t)) = \text{break}(t)$ | Assert that calling `door.break_door()` twice is identical to calling it once. |
| `luckstone_preserves_positive_luck`, `blessed_luckstone_heals_negative_luck`, `uncursed_luckstone_freezes_luck` | Luck timeout toward base (`timeout.c:595-620`): no stone both ways; blessed only bad luck recovers; uncursed frozen; cursed only good luck decays. Period 300 with Amulet or angry god, else 600. (`blessed_luckstone_heals_negative_luck` was `luckstone_heals_negative_luck`, which wrongly covered uncursed.) | `prop_luck_timeout_matches_c_reference`: `step_luck_decay`/`luck_decay_period` equal a reference from `timeout.c:595-620` / `attrib.c:423` over random luck, base, stone BUC. |
| `corrupt_buc_idempotent`, `corrupt_buc_cursed_when_roll_nonzero`, `corrupt_buc_keeps_original_on_zero_roll`, `corrupt_buc_quest_always_cursed` | Bones cursing (`bones.c:290-291`): cursed iff quest item or $rn2(5) \ne 0$, else original BUC. (`corrupt_buc_cursed_when_roll_nonzero` was `corrupt_buc_always_cursed`.) | `prop_corrupt_buc_matches_c_reference` over random BUC, quest flag and roll. |
| `wait_consumes_normal_speed` | $E' = E - 12$ | (no proptest yet) Intended: executing `ActionAst::Wait` decrements energy by `NORMAL_SPEED` (12). |

---

## 4. Known limitations

* `HasLOS.Adjacent` in `NetMechanics/FOV.lean` accepts any two distinct cells, so line of sight holds between every pair of cells; the FOV theorems are therefore much weaker than they appear.
* `beam_terminates_after_energy_steps` (`Raycast.lean`) and `pathfinding_step_bounded` (`Pathfinding.lean`) prove only one-step facts, not the termination or convergence their names suggest.
* About 90 of the 223 theorems are one-line proofs (`rfl`, `simp`, `decide`); many state definitional facts rather than deep invariants.
* The models are simplified and are not NetHack 5.0 itself; see `docs/formal-mechanics-spec.md` for known divergences.
* There is no machine-checked link between Lean and Rust (see Section 3).

---

## 5. Extending the Formalization: Developer Guide

When formalizing a new NetHack mechanic in Lean 4:

1. **Locate the C Origin**:
   * Identify the corresponding C routines in `NetHack-5.0.0/src/`.
   * Isolate the core mathematical logic from UI printing (`pline`) and window clipping.
2. **Define Inductive Types**:
   * Define algebraic data types in `NetMechanics/<Subsystem>.lean`.
   * Ensure that mutually exclusive states are represented as sum types rather than bitmasks.
3. **Specify the Pure Transition Function**:
   * Write the state transformation function without IO or ambient state.
4. **State and Prove Invariant Theorems**:
   * Formulate invariants (safety, idempotency, monotonicity, conservation).
   * Complete the proof using Lean tactics (`rfl`, `simp`, `split`, `decide`, `cases`).
5. **Update Root Library**:
   * Add the module import to [NetMechanics.lean](../NetMechanics.lean).
   * Verify compilation with `lake build`.
6. **Implement Rust Mirror**:
   * Mirror the data types and functions in [crates/netrust-core/src/](../crates/netrust-core/src/).
   * Add property-based tests in `netrust-core` validating the exact same theorem.
