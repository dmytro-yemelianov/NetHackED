# D1 research: C reference vs NetRust Rust/Lean/proptests

Notation: rnd(n)=1..n, rn2(n)=0..n-1, rn1(x,y)=y..y+x-1. C = NetHack-5.0.0/src. Rust core = crates/netrust-core/src. Sim = crates/netrust-sim/src. PT = crates/netrust-core/tests/proptest_mechanics.rs.

---------------------------------------------------------------------
## 1. Hero melee to-hit

(a) C: uhitm.c:365 `find_roll_to_hit`:
```c
tmp = 1 + abon() + find_mac(mtmp) + u.uhitinc
      + (sgn(Luck) * ((abs(Luck) + 2) / 3))
      + maybe_polyd(youmonst.data->mlevel, u.ulevel);
if (mtmp->mstun) tmp += 2; if (mtmp->mflee) tmp += 2;
if (mtmp->msleeping) tmp += 2; if (!mtmp->mcanmove) tmp += 4;
/* Monk: uarm -> -spelarmr; no wep & no shield -> +(ulevel/3)+2 */
/* elf vs orc +1 */
if ((tmp2 = near_capacity()) != 0) tmp -= (tmp2 * 2) - 1;
if (u.utrap) tmp -= 3;
if (aatyp == AT_WEAP || aatyp == AT_CLAW) {
    if (weapon) tmp += hitval(weapon, mtmp);   /* spe + oc_hitbon + vs-type bonuses */
    tmp += weapon_hit_bonus(weapon);           /* skill */
}
```
Hit test: uhitm.c:780-782 `dieroll = rnd(20); mhit = (tmp > dieroll || u.uswallow);` i.e. hit iff d20 < tmp (d20 <= tmp-1).
- abon() weapon.c:950: Str <6:-2, <8:-1, <17:0, <18/50:+1, <18/100:+2, else +3; +1 if ulevel<3; Dex <4:-3, <6:-2, <8:-1, <14:0, else +(dex-14).
- Luck term: sgn(L)*((|L|+2)/3) -> L in {-13..13} gives -5..+5. Luck = u.uluck + u.moreluck (you.h:464).
- hitval weapon.c:149: +spe (weapons/weptools), +oc_hitbon, +2 blessed vs undead/demon, misc.
- weapon_hit_bonus weapon.c:1545: weapon skill Restricted/Unskilled -4, Basic 0, Skilled +2, Expert +3. Bare-handed (weapon.c:1601): ((max(P,1)-1)+2)*(martial?2:1)/2 -> unskilled/basic +1, skilled/expert +2 (martial arts +3..+7). Riding penalty if mounted.
- find_mac (worn.c:717) = monster AC incl. worn armor.
Minimal model given NetRust state (level, luck, enchant, skill, target AC): `tmp = 1 + abon + targetAC + level + luckBonus(luck) + enchant + skillBonus (+ encumbrance penalty if tracked)`; hit iff `rnd(20) < tmp`. abon can be 0 (or +1 for level<3) if Str/Dex not tracked.
Monster->hero (mhitu.c:709): `tmp = AC_VALUE(u.uac) + 10 + m_lev` (+4 if hero helpless, -2 hero invisible & !perceives, -2 mtrapped, min 1); AC_VALUE(ac) = ac>=0 ? ac : -rnd(-ac) (hack.h:1538); hit iff tmp > rnd(20).

(b) Rust: combat.rs:30 `to_hit_threshold(attacker_bonus, target_ac) -> i32 { 10 + target_ac + attacker_bonus }`, combat.rs:34 `attack_lands(d20, thr) = d20 <= thr`, combat.rs:60 `resolve_melee_attack(attacker_bonus, attacker_dmg_bonus, defender, d20_roll, dmg_roll, weapon_enchant)`. Sim passes attacker_bonus = level + weapon_ench + skill (sim/combat.rs:115).
Diffs: base 10 vs C 1 (Rust ~+9 too accurate; with `<=` vs `<` effective offset is +10); no luck term; no abon; same formula used for monster->hero (C uses 10+AC_VALUE+m_lev with randomized negative AC); bare-handed uses weapon table (-4 unskilled) instead of +1. Skill table (skills.rs:3) matches C for weapons.
Suggested signature: `to_hit_threshold(level, luck, enchant, skill_bonus, target_ac, abon) -> i32` returning C `tmp`, `attack_lands(d20, tmp) = (d20 as i32) < tmp`; separate `monster_to_hit(m_lev, hero_ac, ac_roll)`.

(c) Lean Combat.lean:25 `toHitThreshold` (= 10+AC+bonus; docstring already cites the C formula 1+abon+...), :29 `attackLands` (≤). Theorems: `miss_leaves_defender_unchanged` (:117) stays true (stated over attackLands). No theorem fixes the constant, so nothing becomes false; would add e.g. `to_hit_monotone_luck/level` and `hit_iff_roll_lt_tmp`. Effort S.
(d) PT: no direct to-hit proptest (arb_combatant :54 only). `prop_damage_monotone_hp` unaffected.
(e) Sim: sim/combat.rs:115-117 (computes to_hit_bonus, d20 = random_range(1..=20), calls resolve_melee_attack); callers of resolve_combat: actions/movement.rs:78, actions/doors.rs:122 (hero), monsters.rs:143 (pet vs mon), monsters.rs:470 (mon vs hero). Needs player_luck (world.rs:48) and an AC roll for monster->hero.

---------------------------------------------------------------------
## 2. Negative-AC damage reduction

(a) C applies only to damage TO THE HERO:
- mhitu.c:1208-1211 (hitmu):
```c
if (mhm.damage && u.uac < 0) {
    mhm.damage -= rnd(-u.uac);
    if (mhm.damage < 1) mhm.damage = 1;
}
```
- also mhitu.c:1551-1554 (passive/engulf-ish physical) and uhitm.c:4084-4087 (clone split), same rule (tmp<1 -> 1, one path uses `tmp < 0`).
- Hero->monster (uhitm.c hmon): NO AC damage reduction; monster AC only affects to-hit. Min damage: uhitm.c:1505 `if (hmd->dmg < 1) hmd->dmg = 1;` after adding dmgbonus — "don't let penalty turn a hit into a miss".
(b) Rust combat.rs:38 `calculate_damage(roll, enchant, bonus, defender_ac) -> u32`: subtracts the full |AC| (deterministic, not rnd(-AC)), applied for every defender incl. monsters; returns 0 when roll+ench+bonus <= 0 (C gives 1). Fix: `calculate_damage(roll, enchant, bonus, ac_reduction_roll: Option<u32>)` where caller passes `Some(rnd(-uac))` only when defender is hero; clamp to >=1 on hit always.
(c) Lean Combat.lean:37 `calculateDamage` (full absorb, 0 if total<=0). No theorem about calculateDamage itself in Lean (proptest refers to a "negative_ac_absorbs_damage" theorem that does not exist in Combat.lean). New model: `calculateDamage roll ench bonus (absorb : Nat)` with `1 ≤ absorb ≤ -AC`; theorems `damage_pos_on_hit : result ≥ 1`, `absorb_le_raw`. Effort S.
(d) PT: `prop_negative_ac_absorbs_damage` (:187) — "dmg_neg_ac <= dmg_no_ac" still OK; "dmg_no_ac > 0 -> >=1" becomes unconditional. `test_combat_hit_and_damage` unit test in combat.rs uses threshold 21.
(e) sim/combat.rs:36-44 builds def_combat.ac = ac - armor_ench (also used for hero->mon damage); sim/combat.rs:117 must pass an rnd(-AC) roll only when defender_id == player_id.

---------------------------------------------------------------------
## 3. Floor-triggered traps vs Flying/Levitation

(a) trap.c:1061 `floor_trigger`: ARROW, DART, ROCKTRAP, SQKY_BOARD, BEAR_TRAP, LANDMINE, ROLLING_BOULDER_TRAP, SLP_GAS_TRAP, RUST_TRAP, FIRE_TRAP, PIT, SPIKED_PIT, HOLE, TRAPDOOR -> TRUE. Not floor: TELEP_TRAP, LEVEL_TELEP, MAGIC_PORTAL, WEB, STATUE_TRAP, MAGIC_TRAP, ANTI_MAGIC, POLY_TRAP, VIBRATING_SQUARE.
trap.c:1086 `check_in_air`: HURTLING || Levitation || (Flying && !(TOOKPLUNGE|VIASITTING)).
dotrap trap.c:2996-3046:
```c
if (Sokoban && (is_pit(ttype) || is_hole(ttype))) { /* air currents: can't avoid */ }
else if (!forcetrap) {
    if (floor_trigger(ttype) && check_in_air(&youmonst, trflags)) return; /* avoided */
    if (already_seen && !Fumbling && !undestroyable_trap(ttype) && ttype != ANTI_MAGIC
        && !forcebungle && !plunged && !conj_pit && !adj_pit
        && (!rn2(5) || (is_pit(ttype) && is_clinger(youmonst.data))))
        return; /* "You escape a <trap>." 1/5 for seen traps */
}
```
So: web is NOT avoided by flying (Rust/Lean treat it as floor trap). Seen-trap escape chance 1/5 (rn2(5)==0) is not modelled anywhere.
(b) Rust traps.rs:3 `is_floor_trap` = {Pit, SpikedPit, Web}; traps.rs:10 `can_trigger_trap(trap, is_flying)`; traps.rs:20 `trigger_trap`. TrapType (netrust-types lib ~:189) has Arrow, Dart, RockFall, Pit, SpikedPit, Teleport, Fire, LevelTeleport, Polymorph, AntiMagic, SleepingGas, Rust, Web. Correct set: {Arrow, Dart, RockFall, Pit, SpikedPit, Fire, SleepingGas, Rust} (Web/Teleport/LevelTeleport/Polymorph/AntiMagic false). Add `escape_roll: Option<u32>` (rn2(5)) for revealed traps.
(c) Lean Traps.lean:23 `isFloorTrap` (Pit/SpikedPit/Web), :42 `attemptTrigger`. Theorems `flying_avoids_floor_traps` (:56), `non_flying_triggers_floor_trap` (:61), `triggering_reveals_hidden_trap` (:66), `disarm_trap_neutralizes` (:71) all remain true after changing the isFloorTrap table (proofs are generic). If seen-trap escape is added, `non_flying_triggers_floor_trap` must be restated with hypothesis "hidden or escape roll ≠ 0". Effort S.
(d) PT: `prop_flying_bypasses_floor_traps` (:1676), `prop_disarmed_trap_never_triggers` (:1686), `prop_trigger_trap_reveals_hidden` (:1692), `prop_disarm_trap_transitions_to_disarmed` (:1700); arb_trap_type (:1634).
(e) actions/movement.rs:205-214: is_flying = intrinsics.levitation only (no Flying intrinsic); would pass escape roll.

---------------------------------------------------------------------
## 4. Luck timeout with luckstone

(a) timeout.c:595-620 (nh_timeout):
```c
int baseluck = (flags.moonphase == FULL_MOON) ? 1 : 0;
if (flags.friday13) baseluck -= 1;  if (quest killed?..) baseluck -= 4 (Inhell/quest-related);
if (Archeologist && fedora) baseluck += 1;
if (u.uluck != baseluck && moves % ((u.uhave.amulet || u.ugangr) ? 300 : 600) == 0) {
    int time_luck = stone_luck(FALSE);
    boolean nostone = !carrying(LUCKSTONE) && !stone_luck(TRUE);
    if (u.uluck > baseluck && (nostone || time_luck < 0)) u.uluck--;
    else if (u.uluck < baseluck && (nostone || time_luck > 0)) u.uluck++;
}
```
attrib.c:423 `stone_luck(include_uncursed)`: sum over luck items: cursed -quan, blessed (or uncursed if include_uncursed) +quan; return sgn.
Resulting rules (single stone): none -> both decay toward base; BLESSED (time_luck=+1) -> good luck kept, bad luck recovers; UNCURSED (time_luck=0, not nostone) -> NEITHER changes (both frozen); CURSED (time_luck=-1) -> good luck decays, bad luck kept. Interval 600, or 300 with Amulet or god angry. Decay toward baseluck (not 0). u.uluck clamped [-10,10]; Luck adds moreluck ±3 with stone (attrib.c:441).
(b) Rust mines.rs:60 `step_luck_decay(raw_luck, stone: LuckstoneStatus) -> i32`: treats Uncursed like Blessed (bad luck recovers) — WRONG; target 0 not baseluck; no moreluck. Sim fires every 600 turns (actions/mod.rs:140-146), never 300. Suggested `step_luck_decay(luck, base, stone)`; `luck_decay_interval(has_amulet, god_angry) -> u32`.
(c) Lean Mines.lean:105 `stepLuckDecay`; `luckstone_preserves_positive_luck` (:124) stays true (uncursed also preserves); `luckstone_heals_negative_luck` (:139) becomes FALSE for Uncursed (restate: Blessed only; add `uncursed_luckstone_freezes_luck : stepLuckDecay l Uncursed = l`); `step_luck_bounds_preserved` (:158) stays true. Effort S.
(d) PT `prop_luckstone_theorems` (:1004) asserts uncursed heals bad luck -> must change. sim tests simulation_tests.rs:2175-2192.
(e) world.rs:306 `tick_luck_decay` (line 324 call), actions/mod.rs:141-146 (interval).

---------------------------------------------------------------------
## 5. Hunger state thresholds

(a) eat.c:3362 newuhs:
```c
newhs = (h > 1000) ? SATIATED : (h > 150) ? NOT_HUNGRY : (h > 50) ? HUNGRY : (h > 0) ? WEAK : FAINTING;
```
FAINTING/FAINTED (eat.c:3410-3433): when newhs==FAINTING, uhunger_div_by_10 = sgn(h)*((|h|+5)/10); if already fainted -> FAINTED; if (u.uhs <= WEAK || rn2(20 - div10) >= 19) faint for 10-div10 turns (FAINTED). STARVED (eat.c:3437): else-if `u.uhunger < -(100 + 10*ACURR(A_CON))` -> die. Note uhunger goes negative (needs i32).
Boundaries: Satiated h>1000; NotHungry 151..1000; Hungry 51..150; Weak 1..50; Fainting <=0; Starved < -(100+10*Con).
(b) Rust nutrition.rs:19 `hunger_of_nutrition(n: u32)`: `n >= 150` Normal, `n >= 50` Hungry — off by one at 150 and 50 (C: 150 is Hungry, 50 is Weak). u32 can't go negative, Starved unreachable, no Con. Suggested `hunger_of_nutrition(n: i32, con: u8)`; metabolic_tick (nutrition.rs:46) must allow negatives.
(c) Lean Nutrition.lean:20 `hungerOfNutrition`, :28 `hungerTierOfNutrition` (Nat), `eating_improves_or_preserves_hunger` (:39) stays true (monotone) but proof must be redone for `>` thresholds and Int domain (omega handles; M if moving to Int with Starved tier), `zero_nutrition_fainting` (:102) stays true, `metabolic_tick_zero_fixed_point` (:117) becomes false/irrelevant if hunger goes negative (restate `metabolicTick n = n - 1` on Int). Effort S-M.
(d) PT `prop_eating_improves_hunger` (:427).
(e) world.rs:302 (`hunger_of_nutrition(self.player_nutrition)`), turns.rs:48 (metabolic_tick); world.rs player_nutrition type u32.

---------------------------------------------------------------------
## 6. Encumbrance

(a) hack.c:4295 weight_cap:
```c
carrcap = 25 * (ACURRSTR + ACURR(A_CON)) + 50;   /* weight.h: WT_WEIGHTCAP_STRCON=25, SPARE=50 */
/* polyd adjustments (nymph -> MAX; cwt==0 -> *msize/MZ_HUMAN; else *cwt/WT_HUMAN) */
if (Levitation || Is_airlevel || (usteed && strongmonst(steed))) carrcap = MAX_CARR_CAP; /* 1000 */
else { if (carrcap > 1000) carrcap = 1000;
       if (!Flying) { -100 per wounded leg (WT_WOUNDEDLEG_REDUCT) } }
return max(carrcap, 1);
```
ACURRSTR (attrib.c:1245): 3..18 as-is; 18/01..18/31->19, 18/32..18/81->20, 18/82..18/100 & 19..21 ->21 (=19+str/50); 22..25.
hack.c:4351 inv_weight = sum(owt) (+ gold (quan+50)/100) - wc. hack.c:4372 calc_capacity:
```c
wt = inv_weight() + xtra;  if (wt <= 0) return UNENCUMBERED;
if (wc <= 1) return OVERLOADED;
cap = (wt * 2 / wc) + 1;  return min(cap, OVERLOADED /*5*/);
```
i.e. excess e = total-wc: Burdened 0<e and 2e<wc; Stressed wc<=2e<2wc; Strained 2wc<=2e<3wc; Overtaxed 3wc<=2e<4wc; Overloaded 2e>=4wc. In total weight terms: Burdened wc<W<1.5wc, Stressed 1.5wc<=W<2wc, Strained 2wc<=W<2.5wc, Overtaxed 2.5wc<=W<3wc, Overloaded W>=3wc.
(b) Rust inventory.rs:107 `calculate_encumbrance(weight, capacity)` uses `<=` at each boundary: W=1.5wc -> Burdened (C Stressed), W=2wc -> Stressed (C Strained), W=2.5wc -> Strained, W=3wc -> Overtaxed (C Overloaded); integer: C uses floor(2e/wc), Rust uses cap/2 floor — differ on odd wc. No weight_cap function exists. Suggest `weight_cap(str, con, levitating, ...) -> u32` and `calculate_encumbrance(weight, wc) { if weight<=wc Unencumbered else tier = min((weight-wc)*2/wc + 1, 5) }`. Note arena (netrust-arena/src/lib.rs:149 `calculate_total_weight`) uses BoH (cwt+1)/2 for all BUC; C: blessed (cwt+3)/4, uncursed (cwt+1)/2, cursed cwt*2 (not asked, flag).
(c) Lean Inventory.lean:120 `calculateEncumbrance`, `unencumbered_when_le_cap` (:139) stays true; restate as closed form `tierRank (calculateEncumbrance w c) = min ((w-c)*2/c + 1) 5` for w>c. Monotonicity is only a proptest. Effort S.
(d) PT `prop_unencumbered_when_le_cap` (:115), `prop_encumbrance_monotonic` (:121) — both still hold.
(e) No sim caller of calculate_encumbrance (sim never applies encumbrance). To-hit (mech 1) and speed would need it.

---------------------------------------------------------------------
## 7. Enchant armor / enchant weapon

(a) Armor read.c:1115 seffect_enchant_armor (no armor -> strange feeling; confused -> erodeproof toggle):
```c
special_armor = is_elven_armor(otmp) || (Wizard && otmp->otyp == CORNUTHAUM);
s = scursed ? -otmp->spe : otmp->spe;
if (s > (special_armor ? 5 : 3) && rn2(s)) { /* evaporate */ useup(otmp); return; }   /* read.c:1179 */
s = (4 - s) / 2;                     /* 2 for -1..0, 1 for +1..+2, 0 for +3..+4, C int division truncates toward 0 */
if (special_armor) ++s; if (!objects[otyp].oc_magic) ++s; if (sblessed) ++s;
if (s <= 0) { s = 0; if (otmp->spe > 0 && !rn2(otmp->spe)) s = 1; } else s = rnd(s);
if (s > 11) s = 11;  if (scursed) s = -s;
/* dragon scales -> DSM (blessed +1) */ ... otmp->spe += s; cap_spe();
/* curse/bless/uncurse armor to match scroll */
if (otmp->spe > (special ? 5 : 3) && (special || !rn2(7))) "suddenly vibrates" (warning)
```
Evaporation prob for spe=s>limit: (s-1)/s. Cursed scroll on negative armor uses -spe.
NOTE 5.0 differs from 3.6 (3.6: s = cursed?-1 : spe>=9 ? !rn2(spe) : blessed ? rnd(3-spe/3) : 1).
Weapon read.c:1667:
```c
s = scursed ? -1 : !uwep ? 1 : (uwep->spe >= 9) ? (rn2(uwep->spe) == 0)
    : sblessed ? rnd(3 - uwep->spe / 3) : 1;
```
wield.c:918 chwepon, evaporation wield.c:999-1000:
```c
if (((uwep->spe > 5 && amount >= 0) || (uwep->spe < -5 && amount < 0)) && rn2(3)) evaporate;
```
(2/3 chance when spe>5 before reading, i.e. +6 or more; also spe<-5 with cursed). Then spe += amount; warning vibrate if spe>5 && (elven || artifact || !rn2(7)).
(b) Rust enchantment.rs:17 `enchant_item(cur_ench, is_blessed, is_cursed) -> EnchantResult`, SAFE_ENCHANT_CAP=7: deterministic evaporation at >=7, blessed +2 fixed, cursed -1. Diffs: weapon limit is >5 with 2/3 odds; armor limit >3 (>5 special) with (s-1)/s odds; armor amount formula above; blessed weapon rnd(3-spe/3) (1..3 at spe<3, 1..2 at 3..5, 1 at 6..8). Proposed: `enchant_weapon(spe, buc, evap_roll_rn2_3, amount_roll) ` and `enchant_armor(spe, buc, special, magical, evap_roll_rn2_s, amount_roll)`.
(c) Lean Enchantment.lean:10 safeEnchantCap=7, :19 `enchantItem`. `enchant_below_cap_safe` (:30) FALSE under C for cap 7 (weapon +6 evaporates w.p. 2/3; armor +4 evaporates w.p. 3/4) -> restate with per-kind safe limit (weapon spe ≤ 5, armor spe ≤ 3 / 5 special) and as ∀rolls. `enchant_below_cap_increases` (:37): true for weapons (s≥1 when spe<9 non-cursed), FALSE for armor (s=0 possible for spe≥3 non-special magic uncursed; also at spe 2? s=(4-2)/2=1 → rnd(1)=1 ok) -> restate armor as `≥` or restrict to spe ≤ 2. Effort M (two models, roll parameters).
(d) PT `prop_enchant_theorems` (:580), `prop_enchant_at_or_above_cap` (:587).
(e) actions/items.rs:329 (enchant weapon) and :374 (enchant armor; picks first armor item; C uses some_armor random-ish slot order).

---------------------------------------------------------------------
## 8. Wand recharge explosion

(a) read.c:729 recharge, wands at read.c:737-794:
```c
int lim = (otyp == WAN_WISHING) ? 1 : (oc_dir != NODIR) ? 8 : 15;
if (obj->spe == -1) obj->spe = 0;               /* undo cancellation */
n = (int) obj->recharged;
if (n > 0 && (otyp == WAN_WISHING || (n * n * n > rn2(7 * 7 * 7)))) {
    wand_explode(obj, rnd(lim)); return;
}
obj->recharged = n + 1;
if (is_cursed) stripspe(obj);
else { n = (lim == 1) ? 1 : rn1(5, lim + 1 - 5);   /* directional 4..8, nodir 11..15 */
       if (!is_blessed) n = rnd(n);
       if (obj->spe < n) obj->spe = n; else obj->spe++;
       if (WAN_WISHING && spe > 3) explode; }
```
Explode prob per attempt with n prior recharges = n^3/343 (n=1..7: 0.29%, 2.33%, 7.87%, 18.66%, 36.44%, 62.97%, 100%). First recharge never explodes. recharged is capped at 7 (n>=7 always explodes). Wishing explodes on any re-recharge.
(b) Rust artifacts_wands.rs:70 `recharge_wand(w: WandCharges, added_charges) -> RechargeResult`: explodes iff recharges >= 3 (deterministic), adds charges (C sets spe = max(spe+1, n)). Suggested `recharge_wand(w, buc, lim, explode_roll: u32 /*rn2(343)*/, charge_roll)`.
(c) Lean ArtifactsWands.lean:88 `rechargeWand`, `recharge_safe_below_cap` (:98: recharges<3 -> success) FALSE under C (n=1,2 can explode) -> restate `recharge_safe_first : recharges = 0 -> success` and `recharge_explodes_iff : n>0 ∧ n^3 > roll`; `recharge_explodes_at_cap` (:106: ≥3 explodes) FALSE -> restate as `recharges ≥ 7 -> explodes for all roll < 343`. Effort S.
(d) PT `prop_recharge_theorems` (:745); `prop_wand_charge_depletes` (:731) unaffected.
(e) actions/items.rs:415-424 (recharges stored in item.erosion field(!), added=5 fixed; explosion damage fixed 20 vs C wand_explode d(rnd(lim)... )).

---------------------------------------------------------------------
## 9. Shop prices

(a) Buy (shk.c:2877 get_cost): base = getprice(obj,FALSE) (shk.c:4319: oc_cost; +10*spe for weapon/armor spe>0; food *uhs when Hungry+; cancelled wand / uncursed water 0; artifacts arti_cost); if 0 -> 5.
mult/div accumulate:
- unidentified (not dknown or not name_known) & not glass: if (o_id % 4 == 0) *4/3 (shk.c:2864); glass gems priced as a real gem.
- dunce cap: *4/3; else tourist (ulevel < 15) or visible shirt (uarmu && !uarm && !uarmc): *4/3.
- CHA: >18: /2; ==18: *2/3; 16-17: *3/4; 11-15: x1; 8-10: *4/3; 6-7: *3/2; <=5: *2.
- tmp = ((tmp*mult*10/div) + 5)/10 (rounded); min 1; artifact *4; angry shk (surcharge) tmp += (tmp+2)/3.
BUC does not affect price.
Sell (shk.c:3148 set_cost): base = getprice(obj, TRUE) * units; divisor = 3 if dunce cap or tourist(<15)/visible shirt, else 2; unidentified non-gem: if (tmp > 1 && shkp->m_id % 4 == 0) *3/4 (per-shopkeeper, deterministic in 5.0, no rn2); unid gems -> ((otyp-FIRST_REAL_GEM) % (6 - m_id%3) + 3)*quan; same rounding; min 1. CHA does NOT affect sell price. Credit instead of gold: offer*9/10 (shk.c sellobj ~4049).
(b) Rust inventory_interaction.rs:58 `buy_factor(cha)` (<=5:24, <=14:16, <=17:12, else 9 of 12), :71 `sell_factor(cha)` (3/4/6/8 of 12), :84 `calculate_buy_price(base, cha, buc)` (cursed *4/3), :94 `calculate_sell_price(base, cha, buc)` (cursed *2/3). Diffs: wrong CHA table (6-7 should be 3/2, 11-15 x1, 16-17 3/4, 18 2/3, 19+ 1/2), sell depends on CHA and BUC (C: neither), no rounding, no dunce/tourist, no unid surcharges, no 5-zm floor. Proposed: `buy_price(base, cha, dunce_or_tourist, unid_surcharge: bool, angry) ` and `sell_price(base, dunce_or_tourist, shk_lowball: bool)`.
(c) Lean InventoryInteraction.lean:138 buyFactor, :150 sellFactor, :157/:164 prices; `sellFactor_lt_buyFactor` (:171) FALSE (C sell 1/2 = buy 1/2 at CHA≥19 with no surcharges); `sellFactor_le_buyFactor` (:186) and `sell_le_buy_price` (:190) still TRUE but must be reproved with rounding ((x*10/d)+5)/10 and 3-way shapes; reverse_price_id unaffected structurally. Effort M (rounding lemmas).
(d) PT `prop_price_identification_theorems` (:898) — sell-factor monotonicity becomes trivial/constant; buy monotone in CHA still true.
(e) actions/economy.rs:78-80 (hardcoded cha=12; would pass hero CHA, dunce/tourist flag, o_id, shk m_id).

---------------------------------------------------------------------
## 10. Bag of Holding explosion

(a) pickup.c:2488:
```c
mbag_explodes(obj, depthin) {
  if ((otyp == WAN_CANCELLATION || otyp == BAG_OF_TRICKS) && obj->spe <= 0) return FALSE;
  if ((Is_mbag(obj) /*BoH or BoTricks*/ || otyp == WAN_CANCELLATION)
      && (rn2(1 << (depthin > 7 ? 7 : depthin)) <= depthin)) return TRUE;
  else if (Has_contents(obj))
      for (otmp in obj->cobj) if (mbag_explodes(otmp, depthin + 1)) return TRUE;
  return FALSE;
}
```
Called with depth 0 when putting obj into a BoH (pickup.c:2658, 3776). Odds by depth: d0 1/1, d1 2/2, d2 3/4, d3 4/8, d4 5/16, d5 6/32, d6 7/64, d7 8/128, d8 9/128... So inserting a BoH, a charged bag of tricks, or a charged wand of cancellation always explodes; a non-empty sack containing one at depth1 always explodes; deeper nesting probabilistic. Explosion (do_boh_explosion) scatters contents, each item destroyed with is_boh_item_gone 1/13.
(b) Rust inventory.rs:67 `can_insert_safe(item, container)` / :89 `can_insert_safe_flags(item_is_boh, container_is_container, container_is_boh)`: only direct BoH-in-BoH; ignores cancellation wands, bag of tricks, nested contents. Proposed `mbag_explodes(item_tree, depth, rolls: &mut impl FnMut(u32)->u32) -> bool` or deterministic `explosion_odds(depth) -> (num, den)`.
(c) Lean Inventory.lean:81 `canInsertSafe`, `boh_cannot_contain_boh` (:96) stays TRUE (depth 0 always). New theorems: `charged_cancel_explodes`, `depth_odds` (num=d+1, den=2^min(d,7)). Recursive Item type already exists (Item.Box contents) -> structural recursion fine. Effort S-M.
(d) PT `prop_boh_cannot_contain_boh` (:224).
(e) actions/inventory.rs:154-161 (can_insert_safe_flags; would need recursive scan of arena container + rng).

---------------------------------------------------------------------
## 11. Polymorph overkill / rehumanize

(a) hack.c:4256 losehp:
```c
if (Upolyd) { u.mh -= n; if (u.mhmax < u.mh) u.mhmax = u.mh;
              if (u.mh < 1) rehumanize(); ... return; }   /* NO carry-over to u.uhp */
u.uhp -= n; if (u.uhp < 1) { "You die..."; done(DIED); }
```
polyself.c:1367 rehumanize: if Unchanging && u.mh<1 -> die ("killed while stuck in creature form"); else polyman() back to normal form with u.uhp unchanged; only dies if u.uhp<1 already. Same in mhitu.c (mdamageu). So excess damage is discarded; reverting never kills (absent Unchanging).
(b) Rust polymorph.rs:152 `apply_poly_damage(hero: &mut Hero, damage: i32) -> PolyDamageResult` subtracts excess from base_hp and can return Dead; PolyDamageResult::Reverted{excess_damage} (:148); also `PolyEntity::apply_damage` (:39, excess at :71-72). Fix: Reverted with base_hp untouched; add `unchanging: bool` -> Dead.
(c) Lean Polymorph.lean:30 `applyPolyDamage` (excess penetrates). Theorems: `poly_fatal_damage_reverts` (:52) TRUE; `poly_exact_depletion_preserves_base_hp` (:63) TRUE (generalises to all damage ≥ poly.hp); `poly_non_fatal_damage_preserves_poly` (:71) TRUE; `poly_damage_preserves_base_max_hp` (:79) TRUE; `poly_reversion_preserves_base_stats` (:89) TRUE but hypothesis `hsurvives` becomes unnecessary. Add `poly_reversion_preserves_base_hp`. Effort S (proofs simplify).
(d) PT `prop_poly_damage_preserves_base_max_hp` (:344), `prop_poly_fatal_damage_reverts` (:371), `prop_poly_damage_absorption_and_reversion` (:1330 — asserts base_hp - excess and Dead; must change).
(e) sim/combat.rs:158-184 (match on Reverted/Dead); netrust-agent/src/bin/demo.rs:478,486. No roll needed.

---------------------------------------------------------------------
## 12. Bones cursing

(a) bones.c:259 drop_upon_death, bones.c:290-291:
```c
if (rn2(5)) curse(otmp);   /* 4/5 of items become cursed (blessed -> cursed too); 1/5 keep BUC */
... else if (!rn2(8)) give_to_nearby_mon(otmp) else place_object
```
resetobjs (bones.c:51) additionally always curses converted quest items: Amulet -> fake Amulet cursed (:173), Candelabrum -> wax candles cursed (:183), Bell -> bell cursed (:186), Book of the Dead -> blank paper cursed (:189).
(b) Rust bones.rs:8 `corrupt_buc_on_death(_buc) -> Buc` always Cursed. Proposed `corrupt_buc_on_death(buc, roll_rn2_5: u32) -> Buc { if roll != 0 {Cursed} else {buc} }`.
(c) Lean Bones.lean:14 `corruptBucOnDeath`; `corrupt_buc_idempotent` (:20) TRUE only if restated (cursed stays cursed regardless of roll: `corruptBucOnDeath Cursed r = Cursed`); `corrupt_buc_always_cursed` (:27) FALSE -> restate `r ≠ 0 -> Cursed` and `r = 0 -> b`. Effort S.
(d) PT `prop_corrupt_buc_theorems` (:836).
(e) sim/bones.rs:29 (per-item; add rng roll; also 1/8 give to nearby monster not modelled).

---------------------------------------------------------------------
## 13. Mysterious Force

(a) do.c:1540-1572 (goto_level):
```c
if (Inhell && up && u.uhave.amulet && !newdungeon && !portal
    && (dunlev(&u.uz) < dunlevs_in_dungeon(&u.uz) - 3)) {
  if (!rn2(4 + svc.context.mysteryforce)) {          /* 1/(4+mf) */
     int odds = 3 + u.ualign.type;   /* lawful 4, neutral 3, chaotic 2 */
     diff = rn2(odds);               /* L 0..3, N 0..2, C 0..1 */
     if (diff) { assign_rnd_level(newlevel, &u.uz, diff); diff = newlevel->dlevel - u.uz.dlevel;
                 if (was_in_W_tower && !On_W_tower_level(newlevel)) diff = 0; }
     if (diff == 0) assign_level(newlevel, &u.uz);   /* stay on current level, random teleport */
     "A mysterious force momentarily surrounds you...";
     svc.context.mysteryforce += rn2(diff + 2);
     if (same level) { safe_teleds(); return; }
  } }
```
dungeon.c assign_rnd_level: dest = src + rnd(range), clamped to [1, dunlevs_in_dungeon]. So actual push = rnd(rn2(odds)) levels down (L: up to 3, N: up to 2, C: up to 1), from the CURRENT level (not from the target). Not active on the bottom 4 levels of Gehennom (dunlev >= dunlevs-3, i.e. VotD/Sanctum vicinity) — this is the "bottom limit". Chance decays via context.mysteryforce (5.0 change).
(b) Rust gehennom.rs:111 `calculate_mysterious_force(current_depth, roll) -> Option<usize>`: triggers iff roll%3==0 (1/3, C is 1/4 initially), pushback 1..3 uniform regardless of alignment; no "stay & teleport" outcome; no decay counter; clamp in sim stairs.rs:19 `clamp_mysterious_force(pushed, sanctum_open)` (min SANCTUM_DEPTH-1), no "dunlev < bottom-3" gate. Proposed `mysterious_force(depth, bottom, alignment, mf_counter, trigger_roll /*rn2(4+mf)*/, diff_roll /*rn2(3+align)*/, dist_roll /*rnd(diff)*/) -> (Option<usize>, new_mf)`.
(c) Lean: NO model (no Gehennom.lean definition; PT comment cites nonexistent theorem `mysterious_force_bounds`). New: `mf_push_le : result ≤ depth + (2+align)` and `mf_bounded_by_bottom`. Effort S.
(d) PT `prop_mysterious_force_bounds` (:1243).
(e) actions/stairs.rs:807-815 (roll = rng.random::<u32>(); need alignment, mf counter in world, bottom depth).

---------------------------------------------------------------------
## 14. Priest protection purchase

(a) priest.c:629-715 (5.0, randomized):
```c
suggested = (u.ulevelpeak ? u.ulevelpeak : 1) * rn1(101, 150 + cheapskate*40); /* L*(150..250 + 40c) */
quan = max(1, money_cnt(invent) / (suggested * 3));
offer == 0: "regret", adjalign(-1) if coaligned, cheapskate++
offer <  suggested*quan:   "Cheapskate" (if gold > 2*offer, cheapskate++) else thanks
offer <  2*suggested*quan: clairvoyance rn1(500*offer/sugg, 500*offer/sugg) turns
offer <  3*suggested*quan: protection:
    for (; offer >= 2*suggested; offer -= 2*suggested) {
        if (!u.ublessed) u.ublessed = rn1(3, 2);              /* first purchase 2..4 */
        else if (u.ublessed < 20 && (u.ublessed < 9 || !rn2(u.ublessed))) u.ublessed++;
    }
offer >= 3*suggested*quan: alignment/cleansing only (no protection)
```
(3.6 was 400*ulevel / 600*ulevel with +1 per purchase and <20 && (<9 || !rn2(ublessed)) — 5.0 differs.)
(b) Rust mines.rs:14 `protection_donation_cost(level) = 400*level`, :21 `apply_priest_donation(cur, amount, level)`: +1 per donation, hard cap 9 (MAX_DIVINE_PROTECTION :11). Diffs: cost randomized (150..250 per level, ulevelpeak), first gain 2..4, multiple increments per donation, soft cap 9 with 1/ublessed chance up to hard 20, upper donation band gives nothing. Proposed `priest_protection(cur, offer, suggested, quan, first_roll /*rn1(3,2)*/, soft_rolls: &[u32])`, `suggested_donation(level_peak, roll_rn1_101, cheapskate)`.
(c) Lean Mines.lean:31 maxDivineProtection=9, :34 cost, :38 applyPriestDonation. `priest_protection_bounded` (:47, ≤9) FALSE (can reach 20; first gain 2..4) -> restate ≤20; `priest_protection_monotonic` (:56) TRUE; `priest_protection_insufficient` (:64) TRUE if restated with `offer < 2*suggested*quan`. Effort S-M (loop -> recursion over offer).
(d) PT `prop_priest_protection_theorems` (:974); unit tests mines.rs test_priest_donation_protection_cap.
(e) actions/religion.rs:274-293 (default donation = cost; needs rng rolls, player gold, level peak, cheapskate counter).

---------------------------------------------------------------------
## 15. Quest leaders / nemeses (role.c urole table)

C (role.c line of PM_<ROLE>; leader +2, nemesis +4, artifact +9; home/goal strings are the 2 lines above):
| Role | Leader | Nemesis | Artifact | home / goal (role.c) |
|---|---|---|---|---|
| Arc (:45) | Lord Carnarvon | Minion of Huhetotl | Orb of Detection | the College of Archeology / the Tomb of the Toltec Kings |
| Bar (:86) | Pelias | Thoth Amon | Heart of Ahriman | the Camp of the Duali Tribe / the Duali Oasis |
| Hea (:168) | Hippocrates | Cyclops | Staff of Aesculapius | the Temple of Epidaurus / the Temple of Coeus |
| Kni (:208) | King Arthur | Ixoth | Magic Mirror of Merlin | Camelot Castle / the Isle of Glass |
| Mon (:248) | Grand Master | Master Kaen | Eyes of the Overworld | the Monastery of Chan-Sune / the Monastery of the Earth-Lord |
| Rog (:332) | Master of Thieves | Master Assassin | Master Key of Thievery | the Thieves' Guild Hall / the Assassins' Guild Hall |
| Tou (:467) | Twoflower | Master of Thieves | Platinum Yendorian Express Card | Ankh-Morpork / the Thieves' Guild Hall |
| Val (:507) | Norn | Lord Surtur | Orb of Fate | the Shrine of Destiny / the cave of Surtur |
| Wiz (:547) | Neferet the Green | Dark One | Eye of the Aethiopica | the Lonely Tower / the Tower of Darkness |
Rust quest.rs:149 `get_role_quest_config`: WRONG — Rogue leader/nemesis swapped (Rust leader "Master Assassin", nemesis "Master of Thieves"), Tourist nemesis "Master Kaen" (C: Master of Thieves). Others match (modulo "The Norn"/"The Dark One" article). home_desc/goal_desc strings are invented (none match C). Unknown role defaults to Archaeologist silently.
(c) Lean Quest.lean has no per-role names (only eligibility/progress theorems) -> no impact. Effort S (data fix).
(d) PT `prop_leader_qualification_theorems` (:1262), `prop_nemesis_combat_and_artifact_theorems` (:1288) don't check names.
(e) Callers using names: sim/combat.rs:223, actions/stairs.rs:458, 518, 703, 923, actions/inventory.rs:47; netrust-data monsters.rs:587/743 define both monsters (check nemesis spawn uses cfg.nemesis_name).

---------------------------------------------------------------------
## 16. Lean vacuity / weak theorems

FOV.lean:23-27 `HasLOS`: constructor `Adjacent (c1 c2) (h : c1 ≠ c2)` has no adjacency hypothesis, so `HasLOS t a b` holds for ALL a b (SameCell or Adjacent) — relation is trivially total; `Step` is redundant. Correct options:
 (i) adjacency: `Adjacent (h : chebyshev c1 c2 = 1)`; with Step this is "connected through transparent cells" (still not NetHack LOS, which is straight-line: vision.c clear_path / Bresenham).
 (ii) Faithful: `def line : Coord → Coord → List Coord` (Bresenham, interior points) and `def HasLOS t a b := ∀ c ∈ line a b, t c = true`. Theorems: `los_refl` (trivial), `los_adjacent : chebyshev a b ≤ 1 → HasLOS t a b` (line has empty interior; S), `los_opaque_blocks : c ∈ line a b → t c = false → ¬HasLOS t a b` (S), `los_symm` (requires a symmetric line definition, e.g. line b a = (line a b).reverse; proving for Bresenham with tie-breaking is M-L; NetHack's vision isn't perfectly symmetric anyway, so state symmetry only for a symmetric line variant), `los_monotone_transparency` (S). Option (ii) needs Fin arithmetic on COLNO/ROWNO: M overall.
Raycast.lean:121 `beam_terminates_after_energy_steps` only says stepRay with energy 0 terminates (one step). Proper multi-step:
```lean
def runRay : Nat → BeamRay → (Nat → Option SurfaceOrientation) → StepResult  -- fuel-indexed
theorem beam_terminates (ray) (walls : Nat → Option SurfaceOrientation) :
  ∃ k ≤ ray.energy + 1, iterateRay k ray walls = .Terminated
-- or: the number of non-terminal steps is exactly ray.energy
```
Proof: induction on energy using step_decreases_energy (exact decrement) — S. (C beams: range rn1(7,7), bounces in buzz() decrement range; worth matching if modelled.)
Pathfinding.lean:60 `pathfinding_step_bounded` is a single-step lemma (docstring claims D-step convergence). Proper:
```lean
def descend (oracle : MetricState → Option MetricState) : Nat → MetricState → MetricState
theorem pathfinding_converges (oracle) (hgrad : ∀ s, s.dist > 0 → ∃ n, oracle s = some n ∧ n.dist < s.dist)
  (s) : (descend oracle s.dist s).dist = 0
```
Proof by strong induction on dist using descent_step_decreases_distance — S. Grounding it in the actual grid (existence of a strictly-closer passable neighbour under BFS distance) is the hard part — L.
