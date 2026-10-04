# Lean 4 Verification Reference & Rust Bridge

This guide details the formal verification architecture of **NetMechanics** in Lean 4 and specifies the automated bridge to property-based testing in **NetRust**.

---

## 1. Project Organization & Verified Modules

The Lean 4 verification project is defined in [lakefile.toml](../lakefile.toml) and root module [NetMechanics.lean](../NetMechanics.lean). All modules compile with zero errors and zero `sorry` placeholders, and the project uses no axioms beyond Lean's standard ones. The table below lists a representative subset (17 of the 41 modules under `NetMechanics/`); the other modules cover areas such as Nutrition, Religion, Traps, Pet, Quest, Sokoban, Skills and Tournament. The Lean models are deliberately simplified abstractions of NetHack mechanics, not a faithful transcription of NetHack 5.0 (see Section 4, Known limitations).

| Module | Source File | Key Inductive Types | Key Verified Theorems |
| :--- | :--- | :--- | :--- |
| **`NetMechanics.BUC`** | [NetMechanics/BUC.lean](../NetMechanics/BUC.lean) | `BUC`, `WaterType`, `BUCKnowledge` | `buc_not_blessed_and_cursed`<br>`dip_holy_idempotent`<br>`dip_plain_always_uncursed`<br>`uncurse_never_cursed` |
| **`NetMechanics.Inventory`** | [NetMechanics/Inventory.lean](../NetMechanics/Inventory.lean) | `Item`, `ItemKind`, `BagCheckKind`, `BagCheckItem`, `EncumbranceTier` | `item_weight_ge_zero`<br>`contents_weight_ge_tail`<br>`unencumbered_when_le_cap`<br>`encumbrance_tier_rank`<br>`weight_cap_le_1000`<br>`weight_cap_pos`<br>`depth_zero_boh_explodes`<br>`cancellation_wand_explodes`<br>`boh_cannot_contain_boh` |
| **`NetMechanics.Energy`** | [NetMechanics/Energy.lean](../NetMechanics/Energy.lean) | `SchedulerState`, `StepAction` | `turn_strictly_increases`<br>`hero_act_reduces_energy`<br>`scheduler_progress` (Progress Theorem) |
| **`NetMechanics.Grid`** | [NetMechanics/Grid.lean](../NetMechanics/Grid.lean) | `Tile`, `DoorState`, `Coord`, `Alignment` | `open_door_is_passable`<br>`secret_door_impassable`<br>`break_door_idempotent`<br>`reveal_secret_door_locked` |
| **`NetMechanics.Combat`** | [NetMechanics/Combat.lean](../NetMechanics/Combat.lean) | `Combatant`, `AttackResult` | `apply_damage_monotone_hp`<br>`apply_damage_preserves_max_hp`<br>`lethal_damage_kills`<br>`miss_leaves_defender_unchanged`<br>`hit_monotone_target_ac`<br>`hit_monotone_luck`<br>`hit_deals_positive_damage`<br>`hero_absorb_le`<br>`arm_bonus_bounds`<br>`find_ac_monotonic_armor_piece`<br>`find_ac_monotonic_protection`<br>`find_ac_bounds`<br>`armor_list_bonus_nonneg`<br>`find_ac_le_base_nonneg_armor` |
| **`NetMechanics.AST`** | [NetMechanics/AST.lean](../NetMechanics/AST.lean) | `ActionAST`, `EffectAST`, `WorldState` | `wait_consumes_normal_speed`<br>`inflict_damage_preserves_well_formed`<br>`heal_damage_preserves_well_formed` |
| **`NetMechanics.Mines`** | [NetMechanics/Mines.lean](../NetMechanics/Mines.lean) | `LuckstoneCarried` | `luckstone_preserves_positive_luck`<br>`blessed_luckstone_heals_negative_luck`<br>`uncursed_luckstone_freezes_luck`<br>`step_luck_bounds_preserved` |
| **`NetMechanics.Bones`** | [NetMechanics/Bones.lean](../NetMechanics/Bones.lean) | `BonesRecord` | `corrupt_buc_idempotent`<br>`corrupt_buc_cursed_when_roll_nonzero`<br>`corrupt_buc_keeps_original_on_zero_roll`<br>`corrupt_buc_flagged_always_cursed` |
| **`NetMechanics.Enchantment`** | [NetMechanics/Enchantment.lean](../NetMechanics/Enchantment.lean) | `EnchantOutcome`, `ItemErosion`, `AlchemyPotion` | `enchant_weapon_safe_le_limit`<br>`enchant_weapon_increases_le_limit`<br>`enchant_weapon_evaporates_only_beyond_limit`<br>`enchant_weapon_cursed_decrements`<br>`enchant_armor_safe_le_limit`<br>`enchant_armor_evaporates_only_beyond_limit`<br>`enchant_armor_nondecreasing`<br>`enchant_armor_increases_le_two`<br>`proofed_impermeable`<br>`erosion_monotonic` |
| **`NetMechanics.FOV`** | [NetMechanics/FOV.lean](../NetMechanics/FOV.lean) | `HasLOS`, `isAdjacent` | `los_refl`<br>`hasLOS_symm`<br>`hasLOS_opaque_local`<br>`open_door_transparent`<br>`closed_door_opaque`<br>`secret_door_opaque` |
| **`NetMechanics.Raycast`** | [NetMechanics/Raycast.lean](../NetMechanics/Raycast.lean) | `SurfaceOrientation`, `Velocity`, `BeamRay` | `reflect_involution`<br>`reflect_preserves_speed_sq`<br>`step_decreases_energy`<br>`beam_terminates_after_energy_steps`<br>`beam_terminates_within`<br>`beam_live_within_energy` |
| **`NetMechanics.Engraving`** | [NetMechanics/Engraving.lean](../NetMechanics/Engraving.lean) | `EngravingMedium`, `Engraving` | `burned_engraving_permanent`<br>`blind_monster_ignores_elbereth`<br>`covetous_monster_ignores_elbereth`<br>`arbitrary_text_not_warding`<br>`dust_zero_durability_erased`<br>`peaceful_monster_ignores_elbereth`<br>`exempt_monster_ignores_elbereth`<br>`human_onscary_exempt`<br>`shopkeeper_onscary_exempt` |
| **`NetMechanics.Peace`** | [NetMechanics/Peace.lean](../NetMechanics/Peace.lean) | `alignSign`, `peaceMinded` | `peace_minded_always_peaceful`<br>`peace_minded_always_hostile`<br>`peace_minded_cross_aligned_hostile`<br>`peace_minded_coaligned_threshold` |
| **`NetMechanics.Identification`** | [NetMechanics/Identification.lean](../NetMechanics/Identification.lean) | `KnowledgeLevel` | `knowledge_le_refl`<br>`knowledge_le_trans`<br>`knowledge_le_antisymm`<br>`learn_type_monotone`<br>`learn_buc_monotone`<br>`identify_fully_monotone`<br>`identify_fully_idempotent` |
| **`NetMechanics.Polymorph`** | [NetMechanics/Polymorph.lean](../NetMechanics/Polymorph.lean) | `FormStats`, `PolyEntity` | `poly_fatal_damage_reverts`<br>`poly_exact_depletion_preserves_base_hp`<br>`poly_non_fatal_damage_preserves_poly`<br>`poly_damage_preserves_base_max_hp`<br>`poly_reversion_preserves_base_hp`<br>`poly_reversion_preserves_base_stats`<br>`poly_unchanging_fatal_dies` |
| **`NetMechanics.Pathfinding`** | [NetMechanics/Pathfinding.lean](../NetMechanics/Pathfinding.lean) | `MetricState` | `target_is_fixed_point`<br>`descent_step_decreases_distance`<br>`pathfinding_step_bounded`<br>`pathfinding_converges_within` |
| **`NetMechanics.MysteriousForce`** | [NetMechanics/MysteriousForce.lean](../NetMechanics/MysteriousForce.lean) | `MysteriousForceOutcome` | `mysterious_force_push_bounded`<br>`mysterious_force_disabled_bottom`<br>`mf_diff_le` |



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

The Rust engine is an independent, hand-written implementation. **Nothing machine-links the Lean models to the Rust code**: there is no extraction, translation, or proof that the Rust functions satisfy the Lean theorems. Instead, many Lean theorems are mirrored by hand as Rust property-based tests (`proptest`), so that the Rust code is at least checked against the same stated properties on randomized inputs. `crates/netrust-core/tests/proptest_mechanics.rs` currently holds 105 such `prop_` tests; they cover a selection of the Lean theorems, not all 300. A proptest passing is evidence, not proof, that Rust agrees with the Lean property.

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
| `encumbrance_tier_rank`, `weight_cap_le_1000`, `weight_cap_pos` | Tier rank $=\min(\lfloor 2(W-C)/C \rfloor + 1, 5)$ for $W > C$ (`hack.c:4372`); $1 \le C \le 1000$ (`hack.c:4295`). | `prop_encumbrance_matches_c_reference`, `prop_weight_cap_matches_c_reference`: `encumbrance_tier`/`weight_cap` equal references from `hack.c`. |
| `eating_improves_or_preserves_hunger`, `zero_nutrition_fainting`, `starved_iff`, `metabolic_tick_decreases`, `metabolic_tick_zero_goes_negative` | Signed nutrition: Satiated $h>1000$, Normal $>150$, Hungry $>50$, Weak $>0$, Starved $h < -(100+10\,Con)$ (`eat.c:3362`, `:3437`); adding food never lowers the tier; ticks decrement past 0. (`metabolic_tick_zero_fixed_point` was removed: nutrition is no longer floored at 0.) | `prop_hunger_matches_c_reference`, `prop_eating_improves_hunger`: `hunger_of_nutrition(n, con)` equals a reference from `eat.c:3362`. |
| `scheduler_progress` | $\Delta E < 0 \lor \Delta \text{turn} > 0$ | Assert that stepping `SchedulerState` either reduces actor action points or increments `turn`. |
| `apply_damage_monotone_hp` | $HP_{\text{after}} \le HP_{\text{before}}$ | Assert that `c.apply_damage(d)` never increases `c.hp` for any $d \in \mathbb{N}$. |
| `hit_monotone_target_ac`, `hit_monotone_luck` | $\text{tmp} = 1 + AC + lvl + luck(L) + spe + skill$; hit $\iff d_{20} < \text{tmp}$ | `prop_to_hit_matches_c_reference`: `to_hit_value`/`attack_hits` equal a reference written from `uhitm.c:365`/`:780` over random inputs and rolls. |
| `hero_absorb_le`, `calculate_damage_pos` | $D' = \max(1, D - \text{rnd}(-AC_{hero}))$ if $AC_{hero} < 0$; $D \ge 1$ on hit | `prop_hero_ac_absorb_matches_c`: `melee_damage`/`hero_damage_after_ac`/`calculate_damage` equal a reference from `uhitm.c:1505`/`mhitu.c:1208`. |
| `monster_to_hit_pos` | $\text{tmp} = \max(1, AC\_VALUE(AC_{hero}) + 10 + m_{lev})$ | `prop_monster_to_hit_matches_c_reference` (reference from `mhitu.c:709`, `hack.h:1538`). |
| `die_roll_bounds`, `dice_damage_bounds`, `unresisted_hit_pos`, `monster_vs_monster_damage_le` | $d(n, d) = \sum_{k=1}^{n} \text{rnd}(d) \in [n, n d]$, $d(0, x) = 0$ (`mhitu.c:1187`, `mhitm.c:1025`); unresisted landed hit $\ge 1$ | `prop_monster_attack_damage_matches_c_dice`: `monster_attack_damage` equals a reference from C `d()` over random $n$, $d$ and rolls. |
| `resisted_hit_zero`, `resisted_hit_hp_unchanged` | AD_FIRE / AD_COLD resisted $\Rightarrow D = 0$ and HP unchanged (`uhitm.c:2521`, `:2626`); else $D = d(n,d)$ then hero AC absorption | `prop_monster_hit_damage_matches_c`: `resisted`/`monster_hit_damage` equal a reference from `hitmu` (`mhitu.c:1187-1211`). |
| `mhitm_no_plus_ten`, `monster_attack_never_hits_le_1` | Monster vs monster $\text{tmp} = AC_{def} + m_{lev}$, hit $\iff \text{tmp} > \text{rnd}(20 + i)$ (`mhitm.c:321`, `:441`) | `prop_mhitm_to_hit_matches_c`: `mhitm_to_hit`/`monster_attack_hits` equal the C reference; `prop_zap_hit_matches_c` covers the breath ray's `zap_hit` (`zap.c:4705`; no Lean theorem). |
| `dmgval_bounds`, `melee_damage_dmgval_pos` | Hero base weapon damage (`weapon.c:216-293`, `uhitm.c:847`): $R \in [1, d]$, $1 \le D_{\text{raw}}$ | `prop_dmgval_matches_c_rule`: `dmgval`/`weapon_damage_die` matches C rule across small/large targets, martial arts and weapon dice. |
| `arm_bonus_bounds`, `find_ac_monotonic_armor_piece`, `find_ac_monotonic_protection`, `find_ac_bounds` | Armor Class and ARM_BONUS (`hack.h:1526-1528`, `do_wear.c:2473-2507`): $spe \le \text{armBonus} \le a\_ac + spe$; adding armor or protection never increases AC; $\text{findAc} \in [-99, 99]$ | `prop_find_ac_matches_c_reference`: `arm_bonus` and `find_ac` match C reference across base AC, worn armor, erosion and divine protection. |
| `armor_list_bonus_nonneg`, `find_ac_le_base_nonneg_armor` | Human-form hero AC never exceeds 10 when every worn piece has $a\_ac \ge 0$, $spe \ge 0$ and protection $\ge 0$ (`do_wear.c:2475-2501`) | `prop_find_ac_matches_c_reference` (C-valid $a\_ac \in [0, 9]$, erosion $\le$ `MAX_ERODE`); no dedicated $\le 10$ proptest yet. |
| `break_door_idempotent` | $\text{break}(\text{break}(t)) = \text{break}(t)$ | Assert that calling `door.break_door()` twice is identical to calling it once. |
| `luckstone_preserves_positive_luck`, `blessed_luckstone_heals_negative_luck`, `uncursed_luckstone_freezes_luck` | Luck timeout toward base (`timeout.c:595-620`): no stone both ways; blessed only bad luck recovers; uncursed frozen; cursed only good luck decays. Period 300 with Amulet or angry god, else 600. (`blessed_luckstone_heals_negative_luck` was `luckstone_heals_negative_luck`, which wrongly covered uncursed.) | `prop_luck_timeout_matches_c_reference`: `step_luck_decay`/`luck_decay_period` equal a reference from `timeout.c:595-620` / `attrib.c:423` over random luck, base, stone BUC. |
| `corrupt_buc_idempotent`, `corrupt_buc_cursed_when_roll_nonzero`, `corrupt_buc_keeps_original_on_zero_roll`, `corrupt_buc_flagged_always_cursed` | Bones cursing (`bones.c:290-291`): cursed iff always-cursed item (Amulet of Yendor, invocation items) or $rn2(5) \ne 0$, else original BUC. (`corrupt_buc_cursed_when_roll_nonzero` was `corrupt_buc_always_cursed`.) | `prop_corrupt_buc_matches_c_reference` over random BUC, always-cursed flag and roll. |
| `enchant_weapon_safe_le_limit`, `enchant_weapon_increases_le_limit`, `enchant_weapon_evaporates_only_beyond_limit`, `enchant_weapon_cursed_decrements`, `enchant_armor_safe_le_limit`, `enchant_armor_evaporates_only_beyond_limit`, `enchant_armor_nondecreasing`, `enchant_armor_increases_le_two` | Weapon (`read.c:1667`, `wield.c:999`): amount cursed $-1$, $spe \ge 9$: $[rn2(spe)=0]$, blessed $rnd(3 - spe/3)$, else $1$; evaporates iff $rn2(3) \ne 0$ and $spe > 5$ (non-cursed) or $spe < -5$ (cursed). Armor (`read.c:1115`, `:1179`): $s = \pm spe$; evaporates iff $s > 3$ ($>5$ elven/special) and $rn2(s) \ne 0$; gain $rnd((4-s)/2 + [special] + [\neg magic] + [blessed])$ capped at 11, or $[rn2(spe)=0]$ when the die is $\le 0$. Safe and increasing for all rolls at or below the limit (armor strictly only at $spe \le 2$, otherwise non-decreasing). (Replaces `enchant_below_cap_safe` / `enchant_below_cap_increases`, which used a deterministic $+7$ cap.) | `prop_enchant_armor_matches_c_reference`, `prop_enchant_weapon_matches_c_reference` (references from `read.c`/`wield.c` over random `spe`, BUC, rolls); `prop_enchant_safe_at_or_below_limit`, `prop_enchant_weapon_increases_below_limit`. |
| `recharge_safe_first`, `recharge_explodes_iff`, `recharge_explodes_at_cap`, `recharge_wishing_explodes`, `recharge_cursed_strips`, `recharge_cursed_keeps_blessed`, `recharge_blessed_amount` | Wand recharge (`read.c:737-794`): with $n$ prior recharges (capped at 7) and any BUC the wand explodes iff $n > 0 \land (\text{wishing} \lor n^3 > rn2(343))$; the first recharge never explodes, $n \ge 7$ always does, a wishing wand explodes on any re-recharge. Otherwise a cursed scroll strips the wand's charges to 0 (`stripspe`, `read.c:652`), except that a blessed wand keeps them (`recharge_cursed_keeps_blessed`; `rechargeWand` takes a `wandBlessed` flag); blessed/uncursed set $spe = \max(spe+1, n)$ with $n = rn1(5, lim-4)$ (uncursed: $rnd(n)$). (Replace `recharge_safe_below_cap` / `recharge_explodes_at_cap`, which used a deterministic cap of 3.) | `prop_recharge_theorems`: `recharge_wand` equals a reference from `read.c:737-794` over random charges, recharge count, wishing, wand BUC, scroll BUC and rolls. |
| `sell_le_buy_price`, `buy_price_antitone_cha` | Buy (`shk.c:2877` `get_cost`): base (0 → 5) × unID 4/3 × dunce/tourist 4/3 × CHA ($>18$: 1/2, 18: 2/3, 16–17: 3/4, 11–15: 1, 8–10: 4/3, 6–7: 3/2, $\le 5$: 2), rounded $((x\cdot m\cdot 10/d)+5)/10$, $\ge 1$; artifact ×4; angry $+\lceil p/3 \rceil$. Sell (`shk.c:3148` `set_cost`): 1/2 or 1/3 (dunce/tourist), lowball 3/4, no CHA/BUC. Sell $\le$ buy for all inputs; buy antitone in CHA. | `prop_buy_price_matches_c_reference`, `prop_sell_price_matches_c_reference` (references transcribed from `shk.c`), `prop_price_identification_theorems`. |
| `priest_protection_bounded`, `priest_protection_monotonic`, `priest_protection_insufficient`, `priest_protection_first_gain`, `priest_protection_soft_cap_step` | Priest donation (`priest.c:637-699`): suggested $= \max(L_{peak},1)\cdot \text{rn1}(101, 150+40c)$, quan $=\max(1, g/3s)$; protection only for $2sq \le \text{offer} < 3sq$, $\lfloor \text{offer}/2s \rfloor$ steps: first $2..4$, $+1$ below 9, then $+1$ iff $\text{rn2}(p)=0$, cap 20. | `prop_priest_protection_theorems` (step vs reference transcribed from `priest.c:694-698`). |
| `depth_zero_boh_explodes`, `cancellation_wand_explodes`, `boh_cannot_contain_boh` | C `mbag_explodes` (`pickup.c:2488`): depth 0 draw is $\text{rn2}(1) = 0 \le 0$, so a BoH or charged wand of cancellation always explodes; a BoH cannot be safely inserted into a BoH | `prop_mbag_explodes_matches_c_reference` (reference from `pickup.c:2488-2507`, random item trees and rolls, same draw count), `prop_depth_zero_boh_always_explodes` |
| `poly_fatal_damage_reverts`, `poly_exact_depletion_preserves_base_hp`, `poly_reversion_preserves_base_hp`, `poly_unchanging_fatal_dies` | C `hack.c:4256` `losehp` / `polyself.c:1367` `rehumanize`: polyform HP $< 1$ reverts with excess discarded (base HP untouched) unless Unchanging (death); negative damage $= 0$ | `prop_poly_damage_matches_c_reference` (Rust vs a reference written from the C rule, random base/poly HP, damage, `unchanging`), `prop_poly_fatal_damage_reverts` |
| `mysterious_force_push_bounded`, `mysterious_force_disabled_bottom`, `mf_diff_le` | Mysterious Force (`do.c:1541-1573`): fires iff $\text{rn2}(4+mf)=0$ and $dunlev < bottom-3$; push $=\text{rnd}(\text{rn2}(3+align))$ so $\le 3/2/1$ for lawful/neutral/chaotic, strictly downward. | `prop_mysterious_force_matches_c_reference` (reference from `do.c:1541-1573`; also asserts the push caps and the bottom-4 gate). |
| `peace_minded_always_peaceful`, `peace_minded_always_hostile`, `peace_minded_cross_aligned_hostile`, `peace_minded_coaligned_threshold` | Peacefulness (`makemon.c:2268-2308`): `always_peaceful` $\implies$ true, `always_hostile` $\implies$ false, cross-aligned $\implies$ false, co-aligned $\iff r < (A-1)(B-1)$ with $A = 16 + \max(-15, \text{record})$, $B = 2 + |\text{record}|$. | `prop_peace_minded_matches_c_reference`: `peace_minded` matches C reference across all alignments, records, and rolls. |
| `peaceful_monster_ignores_elbereth`, `exempt_monster_ignores_elbereth`, `human_onscary_exempt`, `shopkeeper_onscary_exempt` | Elbereth ward and exemptions (`monmove.c:240-303`): ward active only when adjacent, non-blind, non-covetous, non-peaceful, and non-exempt (`@`, minotaurs, shopkeepers, priests, watchmen, riders). | `prop_onscary_exempt_and_elbereth`: `onscary_exempt` and `is_elbereth_ward_active` match C exemption rules. |
| `hasLOS_symm`, `hasLOS_opaque_local` | `HasLOS.Adjacent` needs Chebyshev distance 1; $\text{LOS}(a,b) \implies \text{LOS}(b,a)$; with every cell opaque, $\text{LOS}(a,b) \implies a = b \lor \text{cheb}(a,b) = 1$ | (no proptest yet) Intended: `fov` visibility symmetric on random open maps. |
| `beam_terminates_within`, `beam_live_within_energy` | For any wall sequence, `runRay` with fuel $> E$ returns `Terminated`; with fuel $\le E$ the beam is still live (exactly $E$ non-terminal steps). | (no proptest yet) Intended: a bouncing beam of range $E$ stops after exactly $E$ cells/bounces. |
| `pathfinding_converges_within` | With an admissible gradient oracle, $k \ge D \implies \text{descend}^k(s).\text{dist} = 0$ | (no proptest yet) |
| `wait_consumes_normal_speed` | $E' = E - 12$ | (no proptest yet) Intended: executing `ActionAst::Wait` decrements energy by `NORMAL_SPEED` (12). |

---

## 4. Known limitations

* `HasLOS` (`NetMechanics/FOV.lean`) is connectivity through transparent cells (king-move links, Chebyshev distance 1), not NetHack's straight-line `clear_path` (`vision.c`); it is not a faithful line-of-sight model.
* `pathfinding_converges_within` assumes an admissible-gradient oracle; it does not prove that the real grid's BFS distance map always offers a strictly closer passable neighbour.
* About 90 of the 298 theorems are one-line proofs (`rfl`, `simp`, `decide`); many state definitional facts rather than deep invariants.
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
