# NetHack & NetRust Formal Mechanics Specification

This document provides the formal mathematical specification for core NetHack mechanics, cross-referencing the verified **Lean 4 formalization** in [NetMechanics/](../NetMechanics/) with the legacy C implementation in `NetHack-5.0.0/`.

---

## Known divergences from NetHack C

The Lean models and the Rust engine are simplified abstractions of NetHack mechanics and are **not** a faithful transcription of NetHack 5.0. The formulas in this document describe the models, and in several places they differ from the C source. Known divergences:

* **Monster-vs-monster to-hit**: pet and other monster-vs-monster attacks use the monster-vs-hero formula (`mhitu.c`: $\text{AC\_VALUE}(\text{AC}) + 10 + m_{\text{lev}}$). C `mhitm.c` uses $\text{find\_mac}(mdef) + m_{\text{lev}}$ (no $+10$) against $\text{rnd}(20 + i)$.
* **Bare-handed damage**: bare-handed attacks use the weapon skill damage table (Unskilled $-2$). In C, the bare-handed/martial-arts damage bonus is $0/{+1}/{+1}/{+2}$ for Unskilled/Basic/Skilled/Expert.
* **`abon()` omitted**: attributes are not tracked, so the to-hit `abon()` term is 0; C's $+1$ below experience level 3 and the Str/Dex to-hit bonuses are absent.
* **Temple priest donations**: the NetRust priest additionally uncurses carried items for an offer of at least $200 \times$ level and grants divine favor $+2$ for any donation that is not refused or a cheapskate offer; neither exists in C `priest.c`. The clairvoyance band (`priest.c:671-680`) and the selfless band's alignment gain / cleansing (`priest.c:706-719`) are not applied (message only).
* **Mysterious Force depth mapping**: C gates the force on `dunlev < dunlevs_in_dungeon - 3` of the real Gehennom (about 20+ levels); NetRust's Gehennom has 6 levels, so the force is only active at depths 1 and 2 (`do.c:1541-1573`).
* **Mysterious Force RNG and teleport**: the sim draws four random values unconditionally on every Amulet ascent attempt in Gehennom (trigger, push, and counter draws are not consumed lazily as in C), and the same-level outcome teleports the hero to a uniformly random passable, unoccupied tile instead of C `safe_teleds` (`do.c:1555`).
* **Bag of Holding explosion scatter**: surviving contents are dropped at the hero's square; C `scatter()` flies them outward with damage (`pickup.c:2517-2532`).
* **Starting inventories**: NetRust role inventories differ from C `u_init.c` (e.g. the NetRust Valkyrie starts with a long sword +0, leather armor and a healing potion; C gives a +1 spear, a +0 dagger, a +3 small shield and a food ration).
* **Attributes**: there is no Charisma, Constitution or Unchanging tracking; constants are used (`DEFAULT_PLAYER_CON` for the starvation threshold, a fixed Charisma for shop prices, Unchanging is a parameter of the core polymorph function, not a tracked hero property).
* **Name-based item kind detection**: some sim code (e.g. weapon skill selection in `netrust-sim/src/combat.rs`) infers an item's kind from substrings of its name rather than from its object class/type.

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
When an object $o$ is put into a Bag of Holding, C `mbag_explodes(o, d)` (`NetHack-5.0.0/src/pickup.c`:2488-2507, called with $d = 0$ at :2658) decides whether the bag explodes. Empty (spe $\le 0$) wands of cancellation and bags of tricks never explode. Otherwise a Bag of Holding, bag of tricks or wand of cancellation explodes if $\text{rn2}(2^{\min(d,7)}) \le d$ (odds $1/1, 2/2, 3/4, 4/8, \ldots$); failing that, each content item is checked recursively at depth $d+1$ and the first explosion wins. Draws happen only where C draws. Hence a BoH, charged bag of tricks or charged wand of cancellation always explodes at depth 0, and a container holding one at depth 1 always explodes.

### Carrying Capacity & Encumbrance Tiers
Carrying capacity $C$ for a hero with (reduced) Strength $S$ and Constitution $K$ (`hack.c:4295`, `weight_cap`):
$$C = \max\bigl(1,\; \min(1000,\, 25(S + K) + 50) - 100 \cdot \text{woundedLegs}\bigr), \qquad C = 1000 \text{ if levitating}$$

Encumbrance tier $\mathcal{E}(W, C)$ (`hack.c:4372`, `calc_capacity`): Unencumbered if $W \le C$; Overloaded if $C \le 1$; otherwise tier rank
$$\min\bigl(\lfloor 2(W - C) / C \rfloor + 1,\; 5\bigr)$$
(1 Burdened, 2 Stressed, 3 Strained, 4 Overtaxed, 5 Overloaded). Hunger (`eat.c:3362`): Satiated $h > 1000$, Normal $h > 150$, Hungry $h > 50$, Weak $h > 0$, Fainting above $-(100 + 10\,\text{Con})$, else Starved; nutrition is signed.

### Machine-Checked Proofs in [NetMechanics/Inventory.lean](../NetMechanics/Inventory.lean)
* **Acyclicity by Construction**: Because `Item` is defined inductively, self-containment cycles ($A \in \text{contents}(B) \land B \in \text{contents}(A)$) are impossible.
* `contents_weight_ge_tail`: Monotonicity of weight under addition:
  $$\forall x, xs,\; W(x :: xs) \ge W(xs)$$
* `unencumbered_when_le_cap`: Soundness of unencumbered boundary:
  $$\forall W, C > 0,\; W \le C \implies \mathcal{E}(W, C) = \text{Unencumbered}$$
* `encumbrance_tier_rank`: for $C \ge 2$, $W > C$: $\text{rank}(\mathcal{E}(W, C)) = \min(\lfloor 2(W-C)/C \rfloor + 1, 5)$.
* `weight_cap_le_1000`, `weight_cap_pos`: $1 \le C \le 1000$.

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
NetHack uses the descending Armor Class (AC) system. Hero melee to-hit (`NetHack-5.0.0/src/uhitm.c`:365, `find_roll_to_hit`), minimal terms tracked by NetRust:
$$\text{tmp} = 1 + \text{AC}_{\text{target}} + \text{level} + \text{sgn}(L)\left\lfloor\frac{|L| + 2}{3}\right\rfloor + \text{spe} + \text{skill}$$
with Luck $L$ clamped to $[-13, 13]$ (bonus in $[-5, 5]$). `abon()`, rings of increase accuracy, monster-state bonuses, encumbrance and trap penalties are not modelled. Bare-handed skill uses the C table (`weapon.c`:1601: Unskilled/Basic $+1$, Skilled/Expert $+2$).
* Roll on 20-sided die: $D_{20} \in [1, 20]$ (out-of-range rolls are clamped).
* Hit condition (`uhitm.c`:780): $\text{Hit} \iff D_{20} < \text{tmp}$. Higher target AC makes the target easier to hit.

Monster attacking the hero (`mhitu.c`:709): $\text{tmp} = \max(1, \text{AC\_VALUE}(\text{AC}_{\text{hero}}) + 10 + m_{\text{lev}})$ with $\text{AC\_VALUE}(a) = a$ for $a \ge 0$ and $-\text{rnd}(-a)$ otherwise (`hack.h`:1538).

### Damage Resolution & Negative AC Absorption
Given base damage roll $R$, weapon enchantment $S$, and skill bonus $B$, a landed hit deals at least 1 (`uhitm.c`:1505):
$$D_{\text{raw}} = \max(1, R + S + B)$$

Negative AC reduces damage **only when the hero is the defender** (`NetHack-5.0.0/src/mhitu.c`:1208), using an explicit roll $r = \text{rnd}(-\text{AC}_{\text{hero}})$:
$$D_{\text{final}} = \begin{cases}
\max(1, D_{\text{raw}} - r) & \text{if the hero is hit and } \text{AC}_{\text{hero}} < 0 \\
D_{\text{raw}} & \text{otherwise (including every monster defender)}
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
* `hit_monotone_target_ac` / `hit_monotone_luck`: raising target AC or Luck never turns a hit into a miss.
* `attack_always_hits_above_20` / `attack_never_hits_le_1`: $\text{tmp} > 20$ always hits; $\text{tmp} \le 1$ never hits.
* `melee_damage_pos`, `calculate_damage_pos`, `hit_deals_positive_damage`: a landed hit deals at least 1 damage.
* `hero_absorb_le` / `hero_absorb_pos` / `hero_absorb_nonneg_ac`: hero AC absorption never increases damage, never drops positive damage below 1, and is a no-op for $\text{AC} \ge 0$.
* `monster_to_hit_pos`: the monster-vs-hero to-hit value is at least 1.

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
* `beam_terminates_after_energy_steps`: One-step helper: a beam with energy 0 terminates on its next step.
* `beam_terminates_within`: Guaranteed loop termination: for any wall sequence, the fuel-bounded runner `runRay` with fuel greater than the initial energy $E$ returns `Terminated` ($E$ advance/reflect steps, then the terminating call), proving freedom from infinite reflection loops. `beam_live_within_energy` shows the bound is tight.

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
\langle \text{Base}, \text{None} \rangle & \text{if } \text{dmg} \ge \text{Poly.hp} \land \lnot\text{Unchanging}
\end{cases}$$

### Machine-Checked Proofs in [NetMechanics/Polymorph.lean](../NetMechanics/Polymorph.lean)
* `poly_fatal_damage_reverts`: Lethal damage to the polymorph form strictly forces form reversion to base ($\forall e, \text{dmg} \ge \text{Poly.hp},\; \text{isPolymorphed} = \text{false}$).
* `poly_exact_depletion_preserves_base_hp`: Reverting (damage $\ge$ Poly.hp) discards the excess: base HP is untouched (C `hack.c:4256` `losehp`, `polyself.c:1367` `rehumanize`).
* `poly_reversion_preserves_base_hp`: Rehumanizing leaves the whole base form unchanged; death only if base HP was already 0.
* `poly_unchanging_fatal_dies`: With Unchanging, fatal polyform damage kills instead of reverting.
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
* `pathfinding_step_bounded`: One-step helper: any step along an admissible gradient decreases distance by at least 1.
* `pathfinding_converges_within`: If the neighbour oracle always offers a strictly closer state away from the target, iterating the descent step $k \ge D$ times from distance $D$ reaches distance 0.


