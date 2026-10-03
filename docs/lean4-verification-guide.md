# Lean 4 Verification Reference & Rust Bridge

This guide details the formal verification architecture of **NetMechanics** in Lean 4 and specifies the automated bridge to property-based testing in **NetRust**.

---

## 1. Project Organization & Verified Modules

The Lean 4 verification project is defined in [lakefile.toml](lakefile.toml) and root module [NetMechanics.lean](NetMechanics.lean). All modules compile with zero errors and zero `sorry` placeholders.

| Module | Source File | Key Inductive Types | Key Verified Theorems |
| :--- | :--- | :--- | :--- |
| **`NetMechanics.BUC`** | [NetMechanics/BUC.lean](NetMechanics/BUC.lean) | `BUC`, `WaterType`, `BUCKnowledge` | `buc_not_blessed_and_cursed`<br>`dip_holy_idempotent`<br>`dip_plain_always_uncursed`<br>`uncurse_never_cursed` |
| **`NetMechanics.Inventory`** | [NetMechanics/Inventory.lean](NetMechanics/Inventory.lean) | `Item`, `ItemKind`, `EncumbranceTier` | `item_weight_ge_zero`<br>`contents_weight_ge_tail`<br>`unencumbered_when_le_cap`<br>`boh_cannot_contain_boh` |
| **`NetMechanics.Energy`** | [NetMechanics/Energy.lean](NetMechanics/Energy.lean) | `SchedulerState`, `StepAction` | `turn_strictly_increases`<br>`hero_act_reduces_energy`<br>`scheduler_progress` (Progress Theorem) |
| **`NetMechanics.Grid`** | [NetMechanics/Grid.lean](NetMechanics/Grid.lean) | `Tile`, `DoorState`, `Coord`, `Alignment` | `open_door_is_passable`<br>`secret_door_impassable`<br>`break_door_idempotent`<br>`reveal_secret_door_locked` |
| **`NetMechanics.Combat`** | [NetMechanics/Combat.lean](NetMechanics/Combat.lean) | `Combatant`, `AttackResult` | `apply_damage_monotone_hp`<br>`apply_damage_preserves_max_hp`<br>`lethal_damage_kills`<br>`miss_leaves_defender_unchanged` |
| **`NetMechanics.AST`** | [NetMechanics/AST.lean](NetMechanics/AST.lean) | `ActionAST`, `EffectAST`, `WorldState` | `wait_consumes_normal_speed`<br>`inflict_damage_preserves_well_formed`<br>`heal_damage_preserves_well_formed` |
| **`NetMechanics.FOV`** | [NetMechanics/FOV.lean](NetMechanics/FOV.lean) | `HasLOS` | `los_refl`<br>`open_door_transparent`<br>`closed_door_opaque`<br>`secret_door_opaque` |
| **`NetMechanics.Raycast`** | [NetMechanics/Raycast.lean](NetMechanics/Raycast.lean) | `SurfaceOrientation`, `Velocity`, `BeamRay` | `reflect_involution`<br>`reflect_preserves_speed_sq`<br>`step_decreases_energy`<br>`beam_terminates_after_energy_steps` |
| **`NetMechanics.Engraving`** | [NetMechanics/Engraving.lean](NetMechanics/Engraving.lean) | `EngravingMedium`, `Engraving` | `burned_engraving_permanent`<br>`blind_monster_ignores_elbereth`<br>`covetous_monster_ignores_elbereth`<br>`arbitrary_text_not_warding`<br>`dust_zero_durability_erased` |
| **`NetMechanics.Identification`** | [NetMechanics/Identification.lean](NetMechanics/Identification.lean) | `KnowledgeLevel` | `knowledge_le_refl`<br>`knowledge_le_trans`<br>`knowledge_le_antisymm`<br>`learn_type_monotone`<br>`learn_buc_monotone`<br>`identify_fully_monotone`<br>`identify_fully_idempotent` |
| **`NetMechanics.Polymorph`** | [NetMechanics/Polymorph.lean](NetMechanics/Polymorph.lean) | `FormStats`, `PolyEntity` | `poly_fatal_damage_reverts`<br>`poly_exact_depletion_preserves_base_hp`<br>`poly_non_fatal_damage_preserves_poly`<br>`poly_damage_preserves_base_max_hp` |
| **`NetMechanics.Pathfinding`** | [NetMechanics/Pathfinding.lean](NetMechanics/Pathfinding.lean) | `MetricState` | `target_is_fixed_point`<br>`descent_step_decreases_distance`<br>`pathfinding_step_bounded` |



---

## 2. Compilation & Verification Workflow

The Lean 4 formalization is fully integrated with Lake:

```bash
# Build all Lean 4 libraries and verify all mathematical proofs
lake build

# Execute the demonstration test harness
./.lake/build/bin/netmechanics
```

### Verification Kernel Confidence
Unlike unit tests that sample specific inputs, Lean 4 proofs are verified by the **Lean 4 trusted kernel**. When `lake build` exits with code 0 without `sorry`, the theorem holds for **all possible inputs** in the domain.

---

## 3. The Lean 4 to Rust Verification Bridge

To ensure the Rust implementation faithfully reflects the Lean 4 mathematical specifications, every Lean 4 theorem maps to a **Rust Property-Based Test** (`proptest`).

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

### Direct Theorem-to-Property Mapping Matrix

| Lean 4 Theorem | Mathematical Property | Synthesized Rust Proptest |
| :--- | :--- | :--- |
| `buc_not_blessed_and_cursed` | $\neg (\text{isBlessed}(b) \land \text{isCursed}(b))$ | Assert that for any generated `Buc`, exactly one variant query is true. |
| `dip_holy_idempotent` | $\text{dip}(H, \text{dip}(H, b)) = \text{dip}(H, b)$ | `assert_eq!(dip_water(Holy, dip_water(Holy, b)), dip_water(Holy, b))` for all `b: Buc`. |
| `contents_weight_ge_tail` | $W(x :: xs) \ge W(xs)$ | Assert that prepending any random `Item` to a `Vec<Item>` never decreases total weight. |
| `unencumbered_when_le_cap` | $W \le C \implies \mathcal{E}(W, C) = \text{Unencumbered}$ | Generate $W \le C$ with $C > 0$, assert result is `EncumbranceTier::Unencumbered`. |
| `scheduler_progress` | $\Delta E < 0 \lor \Delta \text{turn} > 0$ | Assert that stepping `SchedulerState` either reduces actor action points or increments `turn`. |
| `apply_damage_monotone_hp` | $HP_{\text{after}} \le HP_{\text{before}}$ | Assert that `c.apply_damage(d)` never increases `c.hp` for any $d \in \mathbb{N}$. |
| `break_door_idempotent` | $\text{break}(\text{break}(t)) = \text{break}(t)$ | Assert that calling `door.break_door()` twice is identical to calling it once. |
| `wait_consumes_normal_speed` | $E' = E - 12$ | Assert that executing `ActionAst::Wait` decrements energy by `NORMAL_SPEED` (12). |

---

## 4. Extending the Formalization: Developer Guide

When formalizing a new NetHack mechanic in Lean 4:

1. **Locate the C Origin**:
   * Identify the corresponding C routines in [NetHack-5.0.0/src/](NetHack-5.0.0/src/).
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
   * Add the module import to [NetMechanics.lean](NetMechanics.lean).
   * Verify compilation with `lake build`.
6. **Implement Rust Mirror**:
   * Mirror the data types and functions in [crates/netrust-core/src/](crates/netrust-core/src/).
   * Add property-based tests in `netrust-core` validating the exact same theorem.
