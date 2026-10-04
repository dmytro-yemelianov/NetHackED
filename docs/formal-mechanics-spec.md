# NetHack & NetRust Formal Mechanics Specification

This document provides the formal mathematical specification for core NetHack mechanics, cross-referencing the verified **Lean 4 formalization** in [NetMechanics/](../NetMechanics/) with the legacy C implementation in `NetHack-5.0.0/`.

---

## Known divergences from NetHack C

The Lean models and the Rust engine are simplified abstractions of NetHack mechanics and are **not** a faithful transcription of NetHack 5.0. The formulas in this document describe the models, and in several places they differ from the C source. Known divergences:

* **To-hit formula**: the model uses `10 + AC + bonus` with a hit when `d20 <= threshold`. NetHack C (`find_roll_to_hit` in `uhitm.c`) computes `tmp = 1 + abon() + find_mac(mdef) + u.uhitinc + maybe_polyd(...level...) + Luck + ...` (plus rings, weapon hit bonus, skill, role and encumbrance adjustments) and hits when `tmp > rnd(20)`.
* **AC damage reduction**: negative-AC damage absorption is applied to monsters in the model; in C it is applied to damage dealt to the hero (`mhitu.c`), not to monster defenders.
* **Floor-trap set**: the set of traps that fly-over and trigger rules cover is a simplified subset of C's trap types.
* **Luckstone decay**: the model treats uncursed luckstones like blessed ones (positive luck never decays and negative luck recovers). In C, an uncursed luckstone prevents both good and bad luck from timing out; only blessed stones let bad luck time out and only cursed stones let good luck time out.
* **Hunger thresholds**: comparison boundaries (`>` versus `>=`) at the hunger status thresholds differ from C.
* **Encumbrance boundaries**: the weight-to-tier boundaries differ from C's `calc_burden`/`weight_cap` arithmetic.
* **Enchantment cap and recharge explosion**: enchant caps and the recharge-explosion outcome are modelled deterministically; C uses random rolls (`rn2`) for these.
* **Shop charisma table**: the charisma-based price adjustment table is simplified relative to `get_cost` in `shk.c`.

These divergences are planned to be corrected in a later fidelity pass. Until then, do not treat the formulas in this document as authoritative descriptions of NetHack C behavior.

---

## 1. BUC (Blessing / Uncursed / Cursed) Mechanics

### Mathematical Model
In standard NetHack, an item's status is an element of the discrete three-element set:
$$\text{BUC} \in \{ \text{Blessed}, \text{Uncursed}, \text{Cursed} \}$$

In NetHack 5.0.0 C (`NetHack-5.0.0/include/obj.h`), this was represented using two independent bitfields:
```c
Bitfield(cursed, 1);
Bitfield(blessed, 1);
```
Where uncursed is implied when `!cursed && !blessed`. This allows the contradictory state `cursed && blessed`.

### Lean 4 Formalization
Defined in [NetMechanics/BUC.lean](../NetMechanics/BUC.lean):
```lean
inductive BUC where
  | Blessed
  | Uncursed
  | Cursed
deriving Repr, DecidableEq
```

### Transition Functions
1. **Water Dipping (`dipWater`)**:
   $$\text{dipWater}(w, b) = \begin{cases} \text{Blessed} & \text{if } w = \text{Holy} \\ \text{Cursed} & \text{if } w = \text{Unholy} \\ \text{Uncursed} & \text{if } w = \text{Plain} \end{cases}$$

2. **Uncursing (`uncurse`)**:
   $$\text{uncurse}(b) = \begin{cases} \text{Uncursed} & \text{if } b = \text{Cursed} \\ b & \text{otherwise} \end{cases}$$

### Machine-Checked Proofs
* `buc_not_blessed_and_cursed`: Mutual exclusion invariant:
  $$\forall b \in \text{BUC},\; \neg (\text{isBlessed}(b) \land \text{isCursed}(b))$$
* `dip_holy_idempotent`: Idempotency of Holy water dipping:
  $$\text{dipWater}(\text{Holy}, \text{dipWater}(\text{Holy}, b)) = \text{dipWater}(\text{Holy}, b)$$
* `uncurse_never_cursed`: Guarantee that uncursing never yields a cursed item:
  $$\forall b \in \text{BUC},\; \text{uncurse}(b) \ne \text{Cursed}$$

---

## 2. Inventory, Container Hierarchies & Encumbrance

### Mathematical Model
An item is either an atomic item or a container containing a finite multiset or list of items.
$$\text{Item} = \text{Single}(\text{name}, \text{kind}, w_{\text{base}}, b) \;\mid\; \text{Box}(\text{name}, w_{\text{base}}, b, \text{isBoH}, \text{contents})$$
Where $\text{contents} \in \text{List}(\text{Item})$.

### Effective Weight Recurrence
The effective weight $W(i)$ of an item $i$ is defined recursively:
$$W(\text{Single}(\_, \_, w, \_)) = w$$
$$W(\text{Box}(\_, w_{\text{base}}, b, \text{isBoH}, C)) = w_{\text{base}} + \Omega(b, \text{isBoH}, \sum_{c \in C} W(c))$$

Where the Bag of Holding scaling factor $\Omega$ is defined by NetHack 5.0 exact rounding-up rules (`NetHack-5.0.0/src/mkobj.c`:1951):
$$\Omega(b, \text{true}, W_{\text{inner}}) = \begin{cases} \lfloor (W_{\text{inner}} + 3) / 4 \rfloor & \text{if } b = \text{Blessed} \\ \lfloor (W_{\text{inner}} + 1) / 2 \rfloor & \text{if } b = \text{Uncursed} \\ W_{\text{inner}} \times 2 & \text{if } b = \text{Cursed} \end{cases}$$
$$\Omega(\_, \text{false}, W_{\text{inner}}) = W_{\text{inner}}$$

*Notice on Integer Rounding*: In NetHack C, `(cwt + 3) / 4` ensures that an item weighing 1 unit inside a blessed Bag of Holding still contributes at least 1 unit of effective weight ($4 / 4 = 1$), rather than truncating to 0.

### Insertion Safety & Nesting Explosions
In NetHack (`NetHack-5.0.0/src/pickup.c`:2658 `mbag_explodes`), inserting a Bag of Holding inside another Bag of Holding triggers an immediate magical explosion:
$$\text{canInsertSafe}(i, c) = \text{false} \quad \text{if } i.\text{isBoH} \land c.\text{isBoH}$$

### Carrying Capacity & Encumbrance Tiers
Carrying capacity $C$ for a hero with Strength $S$ and Constitution $K$ (`NetHack-5.0.0/src/attrib.c`):
$$C = 5 \times (S + K) + 50$$

Encumbrance tier mapping $\mathcal{E}(W, C)$:
$$\mathcal{E}(W, C) = \begin{cases}
\text{Unencumbered} & \text{if } W \le C \\
\text{Burdened} & \text{if } C < W \le C + \lfloor C / 2 \rfloor \\
\text{Stressed} & \text{if } C + \lfloor C / 2 \rfloor < W \le 2C \\
\text{Strained} & \text{if } 2C < W \le 2C + \lfloor C / 2 \rfloor \\
\text{Overtaxed} & \text{if } 2C + \lfloor C / 2 \rfloor < W \le 3C \\
\text{Overloaded} & \text{if } W > 3C
\end{cases}$$

### Machine-Checked Proofs in [NetMechanics/Inventory.lean](../NetMechanics/Inventory.lean)
* **Acyclicity by Construction**: Because `Item` is defined inductively, self-containment cycles ($A \in \text{contents}(B) \land B \in \text{contents}(A)$) are impossible.
* `contents_weight_ge_tail`: Monotonicity of weight under addition:
  $$\forall x, xs,\; W(x :: xs) \ge W(xs)$$
* `unencumbered_when_le_cap`: Soundness of unencumbered boundary:
  $$\forall W, C > 0,\; W \le C \implies \mathcal{E}(W, C) = \text{Unencumbered}$$

---

## 3. Turn Execution & Speed System

### Mathematical Model
NetHack executes turns via discrete speed rations (`NetHack-5.0.0/src/allmain.c`: `moveloop_core`).
* Standard action cost: $\Delta E = \text{NORMAL\_SPEED} = 12$.
* Actor movement rations:
  * Slow / Burdened Hero: $E_{\text{tick}} = 6$ or $9$.
  * Normal Hero: $E_{\text{tick}} = 12$.
  * Fast Hero: $E_{\text{tick}} = 18$.
  * Very Fast Hero: $E_{\text{tick}} = 24$.

### Scheduler State Machine
$$\text{SchedulerState} = \langle \text{turn}, E_{\text{hero}}, V_{\text{hero}}, E_{\text{monster}}, V_{\text{monster}} \rangle$$

Transition Step:
$$\text{step}(s) = \begin{cases}
\langle s \text{ with } E_{\text{hero}} \leftarrow E_{\text{hero}} - 12, \text{HeroAct} \rangle & \text{if } E_{\text{hero}} \ge 12 \\
\langle s \text{ with } E_{\text{monster}} \leftarrow E_{\text{monster}} - 12, \text{MonsterAct} \rangle & \text{if } E_{\text{hero}} < 12 \land E_{\text{monster}} \ge 12 \\
\langle \text{tickTurn}(s), \text{TurnTick} \rangle & \text{otherwise}
\end{cases}$$

Where:
$$\text{tickTurn}(s) = \langle s.\text{turn} + 1, E_{\text{hero}} + V_{\text{hero}}, V_{\text{hero}}, E_{\text{monster}} + V_{\text{monster}}, V_{\text{monster}} \rangle$$

### Machine-Checked Proofs in [NetMechanics/Energy.lean](../NetMechanics/Energy.lean)
* `scheduler_progress`: **The Progress Theorem**:
  Every step either consumes hero energy, consumes monster energy, or strictly advances the global turn counter:
  $$\forall s,\; \text{let } (s', \text{act}) = \text{step}(s) \text{ in } (s'.E_h < s.E_h) \lor (s'.E_m < s.E_m) \lor (s'.T = s.T + 1)$$
  *Corollary*: The game simulation cannot livelock in an infinite loop without state advancement.

---

## 4. Dungeon Grid & Tile Invariants

### Mathematical Model
Grid dimensions: $80 \times 21$ coordinates:
$$\text{Coord} = \{ (x, y) \in \mathbb{N}^2 \mid x < 80 \land y < 21 \}$$

### Door State Machine
Doors follow a strict deterministic finite automaton (DFA):

```
                   openDoor()
         ┌────────────────────────────┐
         │                            │
         v        closeDoor()         │
     [ Closed ] ───────────────> [ Open ]
         │                            │
unlock() │ lock()                     │
         v                            │ breakDoor()
     [ Locked ]                       │
         │                            │
         │ breakDoor()                │
         v                            v
    [ Broken ] <──────────────────────┘
```

### Tile Accessibility
$$\text{isPassable}(t) = \begin{cases}
\text{true} & \text{if } t \in \{ \text{Room}, \text{Corr}, \text{StairsUp}, \text{StairsDown}, \text{Altar}, \text{Pool}(\text{frozen}=\text{true}) \} \\
\text{true} & \text{if } t = \text{Door}(s, \_) \land s \in \{ \text{Open}, \text{Broken} \} \\
\text{false} & \text{otherwise}
\end{cases}$$

### Machine-Checked Proofs in [NetMechanics/Grid.lean](../NetMechanics/Grid.lean)
* `secret_door_impassable`: Secret doors are strictly impassable: $\forall l,\; \text{isPassable}(\text{SecretDoor}(l)) = \text{false}$.
* `break_door_idempotent`: $\text{breakDoor}(\text{breakDoor}(t)) = \text{breakDoor}(t)$.
* `reveal_secret_door_locked`: Revealing a locked secret door transitions cleanly to $\text{Door}(\text{Locked}, \text{false})$.

---

## 5. Combat & Damage Algebra

### Mathematical Model
NetHack uses the Advanced Dungeons & Dragons descending Armor Class (AC) system (`NetHack-5.0.0/src/uhitm.c`:376).
* Target to-hit threshold:
  $$\Theta = 10 + \text{AC}_{\text{target}} + \text{Bonus}_{\text{attacker}}$$
* Roll on 20-sided die: $D_{20} \in [1, 20]$.
* Hit condition:
  $$\text{Hit}(D_{20}, \Theta) \iff D_{20} \le \Theta$$
  *Notice*: Because better defensive armor lowers AC (e.g., $10 \to 0 \to -5$), it lowers $\Theta$, making $D_{20} \le \Theta$ harder to satisfy.

### Damage Resolution & Negative AC Absorption
Given base damage roll $R$, weapon enchantment $S$, and stat bonus $B$:
$$D_{\text{raw}} = \max(0, R + S + B)$$

In NetHack (`NetHack-5.0.0/src/mhitu.c`:1208), negative AC provides damage absorption:
$$D_{\text{final}} = \begin{cases}
\max(1, D_{\text{raw}} - (-\text{AC}_{\text{target}})) & \text{if } \text{AC}_{\text{target}} < 0 \land D_{\text{raw}} > 0 \\
D_{\text{raw}} & \text{otherwise}
\end{cases}$$

HP state transition:
$$HP_{\text{after}} = \max(0, HP_{\text{before}} - D_{\text{final}})$$
$$\text{isDead} = (HP_{\text{after}} = 0) \lor (D_{\text{final}} \ge HP_{\text{before}})$$

### Machine-Checked Proofs in [NetMechanics/Combat.lean](../NetMechanics/Combat.lean)
* `apply_damage_monotone_hp`: Damage application is strictly monotonic:
  $$\forall c, D,\; HP(\text{applyDamage}(c, D)) \le HP(c)$$
* `lethal_damage_kills`: Lethal damage sets $HP = 0$ and `isDead = true`:
  $$\forall c, D \ge HP(c),\; HP(\text{applyDamage}(c, D)) = 0 \land \text{isDead}(\text{applyDamage}(c, D)) = \text{true}$$
* `apply_damage_preserves_max_hp`: $HP_{\max}$ is invariant under damage application.
* `miss_leaves_defender_unchanged`: A missed attack roll guarantees zero damage and identity preservation.

---

## 6. Wand Beam Raycasting & Specular Wall Reflection

### Mathematical Model
Wand beams in NetHack (`NetHack-5.0.0/src/zap.c`) travel with discrete velocity $(dx, dy)$ on the $80 \times 21$ grid until depleted or reflected by obstacles:
$$\vec{v} = (dx, dy) \in \{-1, 0, 1\}^2 \setminus \{(0, 0)\}$$

### Discrete Specular Reflection Operator
Given surface orientation $S \in \{ \text{Horizontal}, \text{Vertical}, \text{Corner} \}$:
$$R(\vec{v}, \text{Horizontal}) = (dx, -dy)$$
$$R(\vec{v}, \text{Vertical}) = (-dx, dy)$$
$$R(\vec{v}, \text{Corner}) = (-dx, -dy)$$

### Machine-Checked Proofs in [NetMechanics/Raycast.lean](../NetMechanics/Raycast.lean)
* `reflect_involution`: Reflection is an involution:
  $$\forall \vec{v}, S,\; R(R(\vec{v}, S), S) = \vec{v}$$
* `reflect_preserves_speed_sq`: Speed squared / kinetic magnitude is invariant under reflection:
  $$\forall \vec{v}, S,\; \|\vec{v}'\|^2 = \|\vec{v}\|^2$$
* `step_decreases_energy`: Strict attenuation: Every advance or reflection step strictly decreases beam energy:
  $$\forall \text{ray}, W,\; \text{energy}(\text{stepRay}(\text{ray}, W)) < \text{energy}(\text{ray})$$
* `beam_terminates_after_energy_steps`: Guaranteed loop termination: Any beam with initial energy $E$ terminates in at most $E$ steps regardless of obstacle geometry, proving freedom from infinite reflection loops.

---

## 7. Floor Engravings & Elbereth Ward Repulsion

### Mathematical Model
Floors may hold engravings with medium $M \in \{ \text{Burned}, \text{Carved}(d), \text{Marked}(d), \text{Dust}(d) \}$ where $d \in \mathbb{N}$ denotes remaining durability (`NetHack-5.0.0/src/engrave.c`).

### Smudge Operator
Physical trampling or monster steps apply the smudge transition:
$$\text{smudge}(e) = \begin{cases}
e & \text{if } M = \text{Burned} \\
\text{Some}(e[d \mapsto d - 1]) & \text{if } d > 0 \\
\text{None} & \text{if } d = 0
\end{cases}$$

### Elbereth Ward Predicate
$$\text{isElberethWardActive}(e, \text{blind}, \text{covetous}) = \begin{cases}
\text{true} & \text{if } e = \text{Some}(\text{"Elbereth"}) \land \neg \text{blind} \land \neg \text{covetous} \\
\text{false} & \text{otherwise}
\end{cases}$$

### Machine-Checked Proofs in [NetMechanics/Engraving.lean](../NetMechanics/Engraving.lean)
* `burned_engraving_permanent`: Burned engravings are strictly immune to smudge degradation ($\text{smudge}(e_{\text{burned}}) = \text{Some}(e_{\text{burned}})$).
* `blind_monster_ignores_elbereth`: Blind monsters cannot perceive the ward runes.
* `covetous_monster_ignores_elbereth`: Covetous quest bosses (e.g., Wizard of Yendor, Riders) disregard Elbereth.
* `arbitrary_text_not_warding`: Text other than "Elbereth" produces no ward repulsion.
* `dust_zero_durability_erased`: Dust engravings with zero durability are completely wiped upon smudging.

---

## 8. Epistemic Item Identification Lattice

### Mathematical Model
NetHack items possess an epistemic knowledge level $K \in \{ \text{Unidentified}, \text{TypeIdentified}, \text{BucKnown}, \text{FullyIdentified} \}$ (`NetHack-5.0.0/src/read.c`).
* Knowledge is equipped with a total preorder ranking $\text{rank}(K) \in \{0, 1, 2, 3\}$.
* Partial order: $k_1 \le k_2 \iff \text{rank}(k_1) \le \text{rank}(k_2)$.

### Machine-Checked Proofs in [NetMechanics/Identification.lean](../NetMechanics/Identification.lean)
* `knowledge_le_refl`: Knowledge ordering is reflexive ($\forall k,\; k \le k$).
* `knowledge_le_trans`: Knowledge ordering is transitive ($\forall k_1, k_2, k_3,\; k_1 \le k_2 \land k_2 \le k_3 \implies k_1 \le k_3$).
* `knowledge_le_antisymm`: Knowledge ordering is antisymmetric ($k_1 \le k_2 \land k_2 \le k_1 \implies k_1 = k_2$).
* `learn_type_monotone`: Learning the item type never degrades player knowledge ($\forall k,\; k \le \text{learnType}(k)$).
* `learn_buc_monotone`: Altar testing never degrades player knowledge ($\forall k,\; k \le \text{learnBUC}(k)$).
* `identify_fully_monotone`: Full identification maximizes player knowledge ($\forall k,\; k \le \text{identifyFully}(k)$).
* `identify_fully_idempotent`: Full identification is idempotent ($\text{identifyFully}(\text{identifyFully}(k)) = \text{identifyFully}(k)$).

---

## 9. Polymorph & Shape-Shifting HP Buffers

### Mathematical Model
Entities maintain base attributes alongside an optional shape-shifted polymorph form with distinct HP pools (`NetHack-5.0.0/src/polyself.c`).
$$\text{Entity} = \langle \text{Base}, \text{PolyForm}? \rangle$$

### Damage Resolution & Reversion Rule
$$D(e, \text{dmg}) = \begin{cases}
\langle \text{Base}[\text{hp} \mapsto \text{hp} - \text{dmg}], \text{None} \rangle & \text{if } \text{PolyForm} = \text{None} \\
\langle \text{Base}, \text{Some}(\text{Poly}[\text{hp} \mapsto \text{hp} - \text{dmg}]) \rangle & \text{if } \text{dmg} < \text{Poly.hp} \\
\langle \text{Base}[\text{hp} \mapsto \text{hp} - (\text{dmg} - \text{Poly.hp})], \text{None} \rangle & \text{if } \text{dmg} \ge \text{Poly.hp}
\end{cases}$$

### Machine-Checked Proofs in [NetMechanics/Polymorph.lean](../NetMechanics/Polymorph.lean)
* `poly_fatal_damage_reverts`: Lethal damage to the polymorph form strictly forces form reversion to base ($\forall e, \text{dmg} \ge \text{Poly.hp},\; \text{isPolymorphed} = \text{false}$).
* `poly_exact_depletion_preserves_base_hp`: When damage exactly matches polymorph HP, zero excess penetrates, preserving base HP completely.
* `poly_non_fatal_damage_preserves_poly`: Non-fatal damage maintains shape-shifted state.
* `poly_damage_preserves_base_max_hp`: Base maximum HP is invariant under polymorph damage resolution.

---

## 10. Discrete Dijkstra Scent Gradient Pathfinding

### Mathematical Model
Monsters without line-of-sight track player scent through integer metric gradient fields generated by breadth-first / Dijkstra diffusion (`NetHack-5.0.0/src/monmove.c`).
* Metric state: $\text{distToTarget} \in \mathbb{N}$.
* Steepest descent step:
  $$\text{step}(s, n) = \begin{cases}
  s & \text{if } s = 0 \\
  n & \text{if } n.\text{dist} < s.\text{dist} \\
  s & \text{otherwise}
  \end{cases}$$

### Machine-Checked Proofs in [NetMechanics/Pathfinding.lean](../NetMechanics/Pathfinding.lean)
* `target_is_fixed_point`: Once distance is 0, entity has arrived at target and remains stationary.
* `descent_step_decreases_distance`: Stepping to an admissible neighbor strictly decreases distance to target:
  $$\forall s, n,\; n.\text{dist} < s.\text{dist} \implies \text{step}(s, n).\text{dist} < s.\text{dist}$$
* `pathfinding_step_bounded`: Any step along an admissible gradient decreases distance by at least 1, proving convergence in at most $D$ steps.


