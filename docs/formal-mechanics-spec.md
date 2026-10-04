# NetHack & NetRust Formal Mechanics Specification

This document provides the formal mathematical specification for core NetHack mechanics, cross-referencing the verified **Lean 4 formalization** in [NetMechanics/](../NetMechanics/) with the legacy C implementation in `NetHack-5.0.0/`.

---

## Known divergences from NetHack C

The Lean models and the Rust engine are simplified abstractions of NetHack mechanics and are **not** a faithful transcription of NetHack 5.0. The formulas in this document describe the models, and in several places they differ from the C source. Known divergences:

* **Monster attacks (simplified C `mattacku`/`mattackm`)**: monsters resolve their C `mattk[]` hand-to-hand slots (claw, bite, kick, touch, weapon) in order with `tmp > rnd(20 + i)` and `d(n, d)` damage (`mhitu.c:768-912`, `mhitm.c:375-441`), but: (a) only AD_FIRE/AD_COLD damage is zeroed by resistance; every other AD type deals its dice as physical damage without its side effect (poisoning `rn2(8)`, level drain, slow, stun, stoning, AD_SAMU theft, paralysis), and magic cancellation (`mhitm_mgc_atk_negated`'s `rn2(10)`), item destruction (`rn2(20)`), knockback and the undead midnight double damage are not modelled (their draws are skipped); (b) monsters wield no weapons, so AT_WEAP adds no `hitval`/`dmgval` and never throws at range; (c) the to-hit omits the helpless/confused `+4`, invisible/blind and trapped `-2`, and the elf-vs-orc `+1`; (d) AT_MAGC is not cast in melee (`castmu`); only the summon/curse spellcaster abilities act, on their own cooldowns, for the lich, Dark One, Thoth Amon and Wizard of Yendor; (e) an actor whose name resolves to no bestiary archetype attacks once with a `d(1, 6)` claw; (f) breath (AT_BREA, `breamm` `mthrowu.c:1093`) fires at a lined-up hero 2..7 tiles away on `rn2(3)`, hits per `zap_hit` and deals `d(n, 6)` (resisted by fire/cold resistance; a fire breath burns away sliming even when resisted, `zap.c:4432`), and after breathing at the hero `!rn2(3)` sets the `mspec_used` cooldown to `8 + rn2(18)` turns (`mthrowu.c:1131-1132`, decremented once per turn as in `mon_regen` `monmove.c:311`), but the sleep-breath `rnd(20)` cooldown extra, beam range `rn1(7,7)`, bounces and the reflected ray's return path are not modelled (a reflected breath hurts no one; every breather resists its own element); (g) Medusa's gaze keeps its 30-damage approximation instead of stoning, and the floating eye's passive paralysis (`passive`, `uhitm.c:5865`) is not modelled.
* **Bare-handed damage**: bare-handed base damage draws $R \in [1, 2]$ (or $R \in [1, 4]$ for Monk martial arts) per C `uhitm.c:847`, but the skill damage bonus uses the standard table (Unskilled $-2$) rather than C's $0/{+1}/{+1}/{+2}$, and martial-arts damage doubling is not modelled.
* **Hero weapon damage (simplified C `dmgval`, `weapon.c:216-356`)**: the hero's base weapon damage is only `rnd(oc_wsdam)` / `rnd(oc_wldam)` (drawn only on a hit, as in `hmon_hitmon` after `tmp > rnd(20)`). Omitted terms: (a) the per-weapon extras (`weapon.c:228-295`; e.g. mace/war hammer/flail `+1` vs small, broadsword/morning star `+rnd(4)` vs small, Tsurugi/two-handed sword/dwarvish mattock `+d(2,6)` vs large, halberd `+rnd(6)` vs large); (b) C adds `spe` inside `dmgval` and clamps the result to $\ge 0$ (`weapon.c:297-301`), while NetRust adds the enchantment with the skill bonus in `melee_damage` and clamps once to $\ge 1$ (`uhitm.c:1505`); (c) the erosion reduction (`weapon.c:344-353`); (d) the blessed-vs-undead/demon `rnd(4)`, axe-vs-wooden `rnd(4)`, silver `rnd(20)` and light-hating `rnd(8)` bonuses (`weapon.c:322-341`), thick-skinned and shade zeroing, and the heavy iron ball weight bonus. C's `martial_bonus()` (`skills.h:81`) covers Samurai and Monk; only Monk gets the `rnd(4)` bare-handed die because NetRust has no Samurai role.
* **`abon()` omitted**: attributes are not tracked, so the to-hit `abon()` term is 0; C's $+1$ below experience level 3 and the Str/Dex to-hit bonuses are absent.
* **Temple priest donations**: the NetRust priest additionally uncurses carried items for an offer of at least $200 \times$ level and grants divine favor $+2$ for any donation that is not refused or a cheapskate offer; neither exists in C `priest.c`. The clairvoyance band (`priest.c:671-680`) and the selfless band's alignment gain / cleansing (`priest.c:706-719`) are not applied (message only).
* **Mysterious Force depth mapping**: C gates the force on `dunlev < dunlevs_in_dungeon - 3` of the real Gehennom (about 20+ levels); NetRust's Gehennom has 6 levels, so the force is only active at depths 1 and 2 (`do.c:1541-1573`).
* **Mysterious Force RNG and teleport**: the sim draws four random values unconditionally on every Amulet ascent attempt in Gehennom (trigger, push, and counter draws are not consumed lazily as in C), and the same-level outcome teleports the hero to a uniformly random passable, unoccupied tile instead of C `safe_teleds` (`do.c:1555`).
* **Bag of Holding explosion scatter**: surviving contents are dropped at the hero's square; C `scatter()` flies them outward with damage (`pickup.c:2517-2532`).
* **Starting inventories**: NetRust role inventories still differ from C `u_init.c` (e.g. the NetRust Valkyrie starts with a long sword +0 and a healing potion; C gives a +1 spear, a +0 dagger, a +3 small shield and a food ration, `u_init.c:160-165`). Starting enchantment (`trspe`, applied by `ini_inv`, `u_init.c:1233-1234`) is copied only for the weapon/armor role items NetRust shares with C: Knight long sword +1 (`u_init.c:91`), Rogue short sword +0, dagger +0 and leather armor +1 (`u_init.c:134-136`), Wizard cloak of magic resistance +0 (`u_init.c:169`); every other starting item keeps its catalog enchantment.
* **Attributes**: there is no Charisma, Constitution or Unchanging tracking; constants are used (`DEFAULT_PLAYER_CON` for the starvation threshold, a fixed Charisma for shop prices, Unchanging is a parameter of the core polymorph function, not a tracked hero property).
* **Worn armor and hero AC**: hero AC follows C `find_ac` (`do_wear.c:2473-2507`) with base AC 10 (`do_wear.c:2475`) and `a_ac` from the C armor table (`objects.h:445-725`). Because the Valkyrie lacks the C small shield, she starts at AC 10; the Rogue's +1 leather armor gives AC 7 and the Wizard's +0 cloak AC 9. Carried armor counts as worn, at most one per slot (the first carried piece wins; extra pieces in the same slot do not stack); explicit equipment slots and wearing/taking off armor are deferred to D3. `find_ac` terms that are not modelled: ring of protection `spe` (`do_wear.c:2492-2495`), amulet of guarding $-2$ (`do_wear.c:2496-2497`) and spell protection `u.uspellprot` (`do_wear.c:2502`); divine protection `u.ublessed` is always subtracted, without C's `HProtection & INTRINSIC` gate (`do_wear.c:2500-2501`). Hero AC is recomputed after every player action and again after the monster turns of that step.
* **Fainting**: while Fainting, NetRust drains 1 HP every 10 turns and the hero dies at 0 HP (with a death message); this is an approximation, C instead makes the hero faint and lose turns (`eat.c` `newuhs`/`done_in_by` starvation only below the Starved threshold).
* **Shop, priest, luck and headgear simplifications**: the shop price is computed at payment time rather than stored when the item is billed; the priest uses the hero's current level as the peak level and a single global cheapskate counter; a carried dunce cap counts as worn; luck ignores every source other than the luckstone (base luck is 0).
* **Bestiary data**: archetype stats (level, speed, AC, alignment, class glyph, size, uniqueness via G_UNIQ) follow C `monsters.h` (pinned by `crates/netrust-data/tests/bestiary_c_table.rs`) and flow into spawned actors. The archetype `attacks` (`mattk[]`) lists drive monster melee and breath (see *Monster attacks*). HP is a fixed `base_hp` rather than C's rolled `d(lvl, 8)`. Intrinsics cover only what `Intrinsics` can represent (no stone resistance, no per-monster MR percentage; shopkeeper keeps `magic_resistance`, silver dragon keeps `reflection`). The per-monster MR percentages (e.g. guardians 10-30%) and `M2_MAGIC` (guide, apprentice) are not modelled. Names "The Norn"/"The Dark One" remain approximations; the invented floating-eye active gaze and Surtur/Huhetotl breath were removed, and spell summoning/cursing is an approximation of AT_MAGC. Pets promote at levels 4/7 instead of C's 4/6 (`makemon.c:2121`).
* **Ghost class letter**: the ghost's class letter is `' '` per C `S_GHOST` (`defsym.h`); display paths use their own glyphs, so only class genocide and `monster_class_of` see it.
* **Peacefulness, Elbereth and attacking peacefuls**: monsters created through the sim's generation path (`spawn_monster_near` and the starting-level goblins) get C `peace_minded` (`makemon.c:1299`, `:2268-2308`): `M2_PEACEFUL`/`M2_HOSTILE`, `MS_LEADER`/`MS_GUARDIAN`/`MS_NEMESIS`, the hero race's love/hate masks (`role.c`), the alignment sign, the Amulet rule and the co-aligned `rn2(16 + max(record, -15)) && rn2(2 + |mal|)` (second draw only after a non-zero first). Not ported: (a) the Erinys rule (`u.ualign.abuse`, no Erinys species) and the minion rule (no `M2_MINION` species); (b) the post-creation adjustments of `makemon` (`makemon.c:1333-1340`: orcs hostile to elves, which the elf hate mask already gives for every BESTIARY orc; co-aligned unicorns, none exist); (c) the djinni from a lamp and the cursed-genocide goblins are created hostile without `makemon`'s `peace_minded` draw (C draws for a co-aligned hero); (d) the hero's starting alignment record is 25, not C `urole.initrecord` (`attrib.c:1094`), and temple priests keep the archetype alignment instead of the altar's (`EPRI shralign`). Peaceful monsters do not attack or approach: stationary ones stay put, the others take a random step (not C `m_move`). Pets skip every peaceful target; C skips one only when the pet is below 25% HP or the target is a leader or guardian (`dogmove.c:1119-1128`). Attacking (melee, thrown, force bolt/magic missile, wand) runs C `setmangry` (`mon.c:4265-4318`): the Elbereth hypocrisy penalty `-5`/`-rnd(5)` with the engraving erased, `adjalign(-1)` (temple priest: `-5` co-aligned, `+2` otherwise), "<Mon> gets angry!" and the quest guardians turning hostile when the hero attacks their leader; not ported: `growl()` (every peaceful the sim makes is humanoid), `peacefuls_respond` (the watch's arrest, other peacefuls fleeing), `u.ualign.abuse`, `ghod_hitsu`/`hot_pursuit` for angered priests/shopkeepers, and the penalties for killing a peaceful (`xkilled`). Walking into a peaceful follows C `is_safemon` (`uhitm.c:462-509`, `hack.c:2141-2176`): `!rn2(7)` or a tended shop stops the hero ("You stop. <Mon> is in the way!"), a temple priest, shopkeeper or the quest leader (`mundisplaceable`) or a trap on the hero's square refuses to swap, anyone else swaps places; not ported: Punished, long worms, the `dopay()` bump on a blocking shopkeeper, the speed-0 "doesn't seem to move" case (no speed-0 species), monster traps after the swap. A confused, stunned or hallucinating hero who walks into a peaceful attacks it, as in C (no `is_safemon`, no confirmation); "can spot" is approximated by "the hero is not blind", and a blind hero also attacks, whereas C prints "Wait! There's something there you can't see!" and angers the monster without an attack (`uhitm.c:230-251`). Pets keep the sim's unconditional swap (C applies the same `rn2(7)` stop and `monflee` to pets). Shoplifting angers the shopkeeper (`is_peaceful = false`) without C's `rob_shop`/`hot_pursuit` effects. Elbereth (`onscary`, `monmove.c:240-302`) ignores `@`-class, unique, shopkeeper, blind and peaceful monsters; not ported: the Gehennom/endgame suppression (`:302`), Angels and lawful minions (`:251`; none exist), minotaurs, Riders and vault guards (no such species; the core predicate keeps the flags), the scare-monster scroll and displaced image. A scared adjacent monster steps away for that turn instead of C `monflee(rnd(rn2(7) ? 10 : 100))` (`monmove.c:560-564`).
* **Name-based item kind detection**: some sim code (e.g. weapon skill selection in `netrust-sim/src/combat.rs`) infers an item's kind from substrings of its name rather than from its object class/type.
* **Item catalog simplifications**: catalog cost/weight/AC/`oc_magic`/wand direction/nutrition follow `objects.h`, but (a) dice are a single `(n, sides)` so the mace/Mjollnir `+1` small-target bonus and the Tsurugi's `+2d6` large-target bonus are omitted; (b) corpse weight and nutrition come from the monster in C, the catalog keeps weight 50 and nutrition 0 (the sim eats a corpse for a flat 400); (c) `PotionOfHolyWater` is not a C object (it is blessed `potion of water`; the catalog models it with water's cost 100); (d) wand charges are fixed (13 NODIR, 6 directional, 1 wishing) instead of C `rn1(5,11)` / `rn1(5,4)` (`mkobj.c:1115-1124`); (e) beam wands deal fixed damage (striking 12, cold 18, death 100) instead of `d(2,12)` / `d(6,6)` / instant death with resistance checks, and wands of digging and teleportation deal no damage but do not yet dig through or teleport the target; (f) armour name-based magic detection (`armor_is_magical`) covers items outside the catalog, so it is not driven by `oc_magic`.

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

Monster attacks use the archetype's C attack list. For the hand-to-hand slot $i$ (0-based C slot index) the hit test is $\text{tmp} > \text{rnd}(20 + i)$ (`mhitu.c`:806, `mhitm.c`:441), with $\text{tmp}$ computed once per round against the hero and $\text{tmp} = \text{find\_mac}(\text{def}) + m_{\text{lev}}$ (no $+10$, no floor; `mhitm.c`:321) against another monster. A landed attack deals
$$D = \begin{cases} 0 & \text{if the defender resists AD\_FIRE / AD\_COLD} \\ d(n, d) = \textstyle\sum_{k=1}^{n} \text{rnd}(d) & \text{otherwise} \end{cases}$$
($d(0, x) = 0$), then the hero's negative-AC absorption below (`mhitu.c`:1187-1211). A breath ray hits the hero iff `zap_hit` (`zap.c`:4705): $c = \text{rn2}(20)$; $c = 0 \Rightarrow \text{rnd}(10) < AC$, else $3 - c < \text{AC\_VALUE}(AC)$.

### Damage Resolution & Negative AC Absorption
Hero base weapon damage draws $R = \text{dmgval}(W, \text{large}, \text{martial}, \text{roll})$ (`NetHack-5.0.0/src/weapon.c`:216, `uhitm.c`:847): for bare hands, $R \in [1, 2]$ ($[1, 4]$ with Monk martial arts); for a wielded weapon with small die $d_s$ and large die $d_l$ from the catalog, $R \in [1, d_l]$ against large targets ($\ge \text{MZ\_LARGE}$) and $R \in [1, d_s]$ otherwise; non-weapon objects deal $\text{rnd}(2)$ (`uhitm.c`:895).

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

### Armor Class & ARM_BONUS Calculation
Hero AC is determined by `find_ac(void)` (`NetHack-5.0.0/src/do_wear.c`:2473-2507). In human form, base AC is 10:
$$\text{AC}_{\text{uncurbed}} = \text{baseAC} - \sum_{i \in \text{worn}} \text{ARM\_BONUS}(i) - \text{protection}$$
$$\text{AC}_{\text{hero}} = \min(99, \max(-99, \text{AC}_{\text{uncurbed}}))$$
with $\text{ARM\_BONUS}(i)$ (`NetHack-5.0.0/include/hack.h`:1526-1528) for an armor item with base AC bonus $a_{\text{ac}}$, enchantment $\text{spe}$, and erosion $e$:
$$\text{ARM\_BONUS}(i) = a_{\text{ac}} + \text{spe} - \min(e, \max(0, a_{\text{ac}}))$$
Erosion degrades only the intrinsic base bonus of the piece ($a_{\text{ac}}$ down to 0) and never erodes magical enchantment ($\text{spe}$).
Worn armor items are partitioned into slots ($\text{Suit}, \text{Cloak}, \text{Helmet}, \text{Shield}, \text{Gloves}, \text{Boots}, \text{Shirt}$), with at most one item contributing per slot.

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
* `die_roll_bounds` / `melee_damage_die_pos`: a damage die of any size $d \ge 1$ rolls in $[1, d]$, and a landed hit from it deals at least 1.
* `dmgval_bounds` / `melee_damage_dmgval_pos`: for any weapon with positive die, base weapon damage $R \in [1, d]$ and a landed hit deals at least 1.
* `dice_damage_bounds`: $n \le d(n, d) \le n \cdot d$ for $d \ge 1$.
* `resisted_hit_zero` / `resisted_hit_hp_unchanged`: a resisted monster attack deals 0 and leaves the defender's HP unchanged.
* `unresisted_hit_pos` / `monster_vs_monster_damage_le`: an unresisted landed attack with $n, d \ge 1$ deals at least 1; against a monster it deals at most $n \cdot d$.
* `mhitm_no_plus_ten` / `monster_attack_never_hits_le_1`: monster-vs-monster $\text{tmp} = AC + m_{lev}$ on the $\text{rnd}(20+i)$ die; $\text{tmp} \le 1$ never hits.
* `arm_bonus_bounds`: for non-negative $a_{\text{ac}}$, $\text{spe} \le \text{armBonus}(a_{\text{ac}}, \text{spe}, e) \le a_{\text{ac}} + \text{spe}$.
* `find_ac_monotonic_armor_piece`: adding an armor piece with non-negative bonus never increases AC ($\forall p,\; 0 \le \text{armBonus}(p) \implies \text{findAc}(\text{baseAc}, p :: \text{worn}, \text{prot}) \le \text{findAc}(\text{baseAc}, \text{worn}, \text{prot})$).
* `find_ac_monotonic_protection`: divine protection monotonically decreases or preserves AC ($\text{prot}_1 \le \text{prot}_2 \implies \text{findAc}(\text{worn}, \text{prot}_2) \le \text{findAc}(\text{worn}, \text{prot}_1)$).
* `find_ac_bounds`: hero AC is unconditionally clamped within $[-99, 99]$ (`AC_MAX`).
* `armor_list_bonus_nonneg` / `find_ac_le_base_nonneg_armor`: when every worn piece has $a_{\text{ac}} \ge 0$ and $\text{spe} \ge 0$ and $\text{prot} \ge 0$, the total armor bonus is non-negative and $\text{findAc}(10, \text{worn}, \text{prot}) \le 10$.


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

## 7. Floor Engravings, Elbereth Ward Repulsion & Monster Peacefulness

### Mathematical Model of Floor Engravings
Floors may hold engravings with medium $M \in \{ \text{Burned}, \text{Carved}(d), \text{Marked}(d), \text{Dust}(d) \}$ where $d \in \mathbb{N}$ denotes remaining durability (`NetHack-5.0.0/src/engrave.c`).

### Smudge Operator
Physical trampling or monster steps apply the smudge transition:
$$\text{smudge}(e) = \begin{cases}
e & \text{if } M = \text{Burned} \\
\text{Some}(e[d \mapsto d - 1]) & \text{if } d > 0 \\
\text{None} & \text{if } d = 0
\end{cases}$$

### Elbereth Ward Predicate & Monster Exemptions
The ward under the hero scares an adjacent monster only if it is inscribed with "Elbereth", the monster can see (not blind), is not unique, is not peaceful, and is not otherwise exempt (C `onscary`, `NetHack-5.0.0/src/monmove.c`:240-302). Adjacency is checked by the sim, not by the predicate:
$$\text{isElberethWardActive}(e, \text{blind}, \text{unique}, \text{peaceful}, \text{exempt}) = \begin{cases}
\text{true} & \text{if } e = \text{Some}(\text{"Elbereth"}) \land \neg \text{blind} \land \neg \text{unique} \land \neg \text{peaceful} \land \neg \text{exempt} \\
\text{false} & \text{otherwise}
\end{cases}$$

Here `unique` is C `unique_corpstat` (`monmove.c:260`, which also covers the Wizard of Yendor). Exempt monsters (`onscary_exempt`):
* `@`-class monsters (`mlet == S_HUMAN`, `monmove.c:260`): humans, shopkeepers, temple priests, watchmen
* Minotaurs (`monmove.c:301`)
* Shopkeepers (anywhere) and vault guards (`isshk || isgd`, `monmove.c:299`)
* The Riders (`monmove.c:251-252`)

### Machine-Checked Proofs in [NetMechanics/Engraving.lean](../NetMechanics/Engraving.lean)
* `burned_engraving_permanent`: Burned engravings are strictly immune to smudge degradation ($\text{smudge}(e_{\text{burned}}) = \text{Some}(e_{\text{burned}})$).
* `blind_monster_ignores_elbereth`: Blind monsters cannot perceive the ward runes.
* `unique_monster_ignores_elbereth`: Unique monsters (`unique_corpstat`) disregard Elbereth.
* `arbitrary_text_not_warding`: Text other than "Elbereth" produces no ward repulsion.
* `dust_zero_durability_erased`: Dust engravings with zero durability are completely wiped upon smudging.
* `peaceful_monster_ignores_elbereth`: Peaceful monsters ignore Elbereth.
* `exempt_monster_ignores_elbereth`: Exempt monsters ignore Elbereth.
* `human_onscary_exempt`: Monsters of the `@` class are always exempt.
* `shopkeeper_onscary_exempt`: Shopkeepers (and vault guards) are always exempt.

### Monster Peacefulness (`peace_minded`)
Monster creation in NetHack 5.0 C sets `mpeaceful = peace_minded(ptr)` (`NetHack-5.0.0/src/makemon.c`:1299, :2268-2308). With $\text{mal}$ the monster's `maligntyp`, $\text{ual}$ the hero's alignment type and $\text{record}$ the hero's alignment record, the steps are, in order:
1. `M2_PEACEFUL` $\implies$ peaceful; 2. `M2_HOSTILE` $\implies$ hostile;
3. `MS_LEADER` or `MS_GUARDIAN` $\implies$ peaceful; `MS_NEMESIS` $\implies$ hostile;
4. `race_peaceful` (hero race love mask) $\implies$ peaceful; `race_hostile` (hate mask) $\implies$ hostile;
5. $\text{sgn}(\text{mal}) \ne \text{sgn}(\text{ual}) \implies$ hostile;
6. $\text{mal} < 0$ and the hero carries the Amulet $\implies$ hostile;
7. a minion is peaceful iff $\text{record} \ge 0$;
8. otherwise two draws $r_1 = \text{rn2}(A)$ and, only if $r_1 \ne 0$, $r_2 = \text{rn2}(B)$ with
   $$A = 16 + \max(-15, \text{record}) \ge 1, \qquad B = 2 + |\text{mal}| \ge 2,$$
   and the monster is peaceful iff $r_1 \ne 0 \land r_2 \ne 0$. Of the $A \cdot B$ equally likely $(r_1, r_2)$ outcomes exactly $(A-1)(B-1)$ are peaceful; with $\text{record} \le -15$, $A = 1$ and the monster is always hostile.

The Rust core splits this into `peace_decision` (steps 1-7, or the roll arguments $A$, $B$) and `peace_minded`, which draws $r_1$ and $r_2$ through a C `rn2` callback.

### Machine-Checked Proofs in [NetMechanics/Peace.lean](../NetMechanics/Peace.lean)
* `peace_minded_always_peaceful`, `peace_minded_always_hostile`, `peace_minded_nemesis_hostile`: the flag and `msound` steps, for any rolls.
* `peace_minded_cross_aligned_hostile`: with no flag, `msound` or race rule, a cross-aligned monster is hostile.
* `peace_minded_amulet_chaotic_hostile`: a chaotic monster is hostile to a hero carrying the Amulet.
* `peace_decision_roll_args`, `peace_minded_roll_args_valid`: the roll arguments are $A$ and $B$ above, with $A \ge 1$ and $B \ge 2$.
* `peace_minded_coaligned_iff`: in the co-aligned case the monster is peaceful iff $r_1 \ne 0 \land r_2 \ne 0$.
* `peace_minded_first_draw_zero_hostile`: $r_1 = 0$ is hostile whatever $r_2$ (the second draw is never taken).
* `peace_minded_low_record_always_hostile`: with $\text{record} \le -15$ every $r_1 < A$ gives hostile.

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


