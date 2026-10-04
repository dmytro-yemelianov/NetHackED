/-
  NetHack Mechanics Formalized in Lean 4: Combat & Damage Resolution
  Formalizing C to-hit (uhitm.c:365, mhitu.c:709), hero AC damage reduction
  (mhitu.c:1208), minimum damage (uhitm.c:1505), and HP clamping.
-/

namespace NetMechanics

/-- Combatant state -/
structure Combatant where
  hp : Nat
  maxHp : Nat
  ac : Int
  level : Nat
  toHitBonus : Int
  damageBonus : Int
  isDead : Bool
deriving Repr, DecidableEq

/--
  Hero Luck contribution to melee to-hit (C `uhitm.c:365`, `find_roll_to_hit`):
    `sgn(Luck) * ((|Luck| + 2) / 3)`, with Luck clamped to the C range `-13..13`.
-/
def luckToHitBonus (luck : Int) : Int :=
  let l := max (-13) (min 13 luck)
  if 0 < l then (l + 2) / 3 else if l < 0 then -((-l + 2) / 3) else 0

/--
  Hero melee to-hit value `tmp` (C `uhitm.c:365`, `find_roll_to_hit`), minimal terms:
    `tmp = 1 + find_mac(mdef) + u.ulevel + luck bonus + weapon spe + weapon_hit_bonus`
  `abon()`, rings, monster-state, encumbrance and trap terms are not modelled
  (taken as 0). Higher target AC makes the target easier to hit.
-/
def toHitValue (level luck enchant skillHit targetAC : Int) : Int :=
  1 + targetAC + level + luckToHitBonus luck + enchant + skillHit

/-- The `rnd(20)` draw, clamped into `1..20` (out-of-range rolls never panic). -/
def clampD20 (d20 : Nat) : Nat := max 1 (min 20 d20)

/-- Hit test (C `uhitm.c:780-782`): `mhit = tmp > rnd(20)`. -/
def attackHits (d20 : Nat) (toHit : Int) : Bool :=
  decide (((clampD20 d20 : Nat) : Int) < toHit)

/--
  `AC_VALUE(ac) = ac >= 0 ? ac : -rnd(-ac)` (C `hack.h:1538`); the `rnd(-ac)`
  draw `acRoll` is clamped into `1..-ac`.
-/
def acValue (ac : Int) (acRoll : Nat) : Int :=
  if 0 ≤ ac then ac else -((max 1 (min acRoll (-ac).toNat) : Nat) : Int)

/-- Monster-vs-hero to-hit (C `mhitu.c:709`): `max 1 (AC_VALUE(u.uac) + 10 + m_lev)`. -/
def monsterToHitValue (mLevel heroAC : Int) (acRoll : Nat) : Int :=
  max 1 (acValue heroAC acRoll + 10 + mLevel)

/--
  Damage of a landed hit before hero-AC reduction (C `uhitm.c:1505`):
  `base + spe + bonus`, raised to 1 if lower.
-/
def meleeDamage (baseRoll : Nat) (enchant bonus : Int) : Nat :=
  max 1 ((baseRoll : Int) + enchant + bonus).toNat

/--
  Negative hero AC damage reduction (C `mhitu.c:1208-1211`):
    `if (dmg && u.uac < 0) { dmg -= rnd(-u.uac); if (dmg < 1) dmg = 1; }`
  The `rnd(-u.uac)` draw `absorbRoll` is clamped into `1..-heroAC`.
-/
def heroDamageAfterAC (damage : Nat) (heroAC : Int) (absorbRoll : Nat) : Nat :=
  if damage = 0 ∨ 0 ≤ heroAC then damage
  else max 1 (damage - max 1 (min absorbRoll (-heroAC).toNat))

/--
  Full damage of a landed hit. `heroAbsorbRoll = some r` only when the defender
  is the hero (`mhitu.c:1208`); monster defenders take no AC reduction (C `hmon`).
-/
def calculateDamage (baseRoll : Nat) (enchant bonus defenderAC : Int)
    (heroAbsorbRoll : Option Nat) : Nat :=
  match heroAbsorbRoll with
  | none => meleeDamage baseRoll enchant bonus
  | some r => heroDamageAfterAC (meleeDamage baseRoll enchant bonus) defenderAC r

/-- Luck bonus is monotone in Luck. -/
theorem luck_bonus_monotone (a b : Int) (h : a ≤ b) :
    luckToHitBonus a ≤ luckToHitBonus b := by
  unfold luckToHitBonus
  simp only []
  split <;> split <;> (try split) <;> (try split) <;> omega

/-- Luck bonus lies in `-5..5`. -/
theorem luck_bonus_bounded (l : Int) :
    -5 ≤ luckToHitBonus l ∧ luckToHitBonus l ≤ 5 := by
  unfold luckToHitBonus
  simp only []
  split <;> (try split) <;> omega

/-- To-hit value is monotone in target AC (higher AC is easier to hit). -/
theorem to_hit_monotone_target_ac (level luck ench skill ac1 ac2 : Int) (h : ac1 ≤ ac2) :
    toHitValue level luck ench skill ac1 ≤ toHitValue level luck ench skill ac2 := by
  unfold toHitValue; omega

/-- To-hit value is monotone in Luck. -/
theorem to_hit_monotone_luck (level l1 l2 ench skill ac : Int) (h : l1 ≤ l2) :
    toHitValue level l1 ench skill ac ≤ toHitValue level l2 ench skill ac := by
  unfold toHitValue
  have := luck_bonus_monotone l1 l2 h
  omega

/-- To-hit value is monotone in hero level. -/
theorem to_hit_monotone_level (lv1 lv2 luck ench skill ac : Int) (h : lv1 ≤ lv2) :
    toHitValue lv1 luck ench skill ac ≤ toHitValue lv2 luck ench skill ac := by
  unfold toHitValue; omega

/-- The hit test is exactly `rnd(20) < tmp` on the clamped roll. -/
theorem attack_hits_iff (d20 : Nat) (toHit : Int) :
    attackHits d20 toHit = true ↔ ((clampD20 d20 : Nat) : Int) < toHit := by
  simp [attackHits]

/-- A larger `tmp` never turns a hit into a miss. -/
theorem attack_hits_monotone (d20 : Nat) (t1 t2 : Int) (h : t1 ≤ t2)
    (hit : attackHits d20 t1 = true) : attackHits d20 t2 = true := by
  rw [attack_hits_iff] at *; omega

/-- `tmp > 20` hits on every roll. -/
theorem attack_always_hits_above_20 (d20 : Nat) (toHit : Int) (h : 20 < toHit) :
    attackHits d20 toHit = true := by
  rw [attack_hits_iff]; unfold clampD20; omega

/-- `tmp ≤ 1` misses on every roll. -/
theorem attack_never_hits_le_1 (d20 : Nat) (toHit : Int) (h : toHit ≤ 1) :
    attackHits d20 toHit = false := by
  cases hc : attackHits d20 toHit
  · rfl
  · rw [attack_hits_iff] at hc; unfold clampD20 at hc; omega

/-- Hitting is monotone in target AC. -/
theorem hit_monotone_target_ac (d20 : Nat) (level luck ench skill ac1 ac2 : Int)
    (h : ac1 ≤ ac2) (hit : attackHits d20 (toHitValue level luck ench skill ac1) = true) :
    attackHits d20 (toHitValue level luck ench skill ac2) = true :=
  attack_hits_monotone d20 _ _ (to_hit_monotone_target_ac level luck ench skill ac1 ac2 h) hit

/-- Hitting is monotone in Luck. -/
theorem hit_monotone_luck (d20 : Nat) (level l1 l2 ench skill ac : Int)
    (h : l1 ≤ l2) (hit : attackHits d20 (toHitValue level l1 ench skill ac) = true) :
    attackHits d20 (toHitValue level l2 ench skill ac) = true :=
  attack_hits_monotone d20 _ _ (to_hit_monotone_luck level l1 l2 ench skill ac h) hit

/-- Monster to-hit value is at least 1. -/
theorem monster_to_hit_pos (mLevel heroAC : Int) (acRoll : Nat) :
    1 ≤ monsterToHitValue mLevel heroAC acRoll := by
  unfold monsterToHitValue; omega

/-- A landed hit always deals at least 1 damage before AC reduction. -/
theorem melee_damage_pos (baseRoll : Nat) (enchant bonus : Int) :
    1 ≤ meleeDamage baseRoll enchant bonus := by
  unfold meleeDamage; omega

/-- Hero AC absorption never increases damage. -/
theorem hero_absorb_le (damage : Nat) (heroAC : Int) (absorbRoll : Nat) :
    heroDamageAfterAC damage heroAC absorbRoll ≤ damage := by
  unfold heroDamageAfterAC
  split <;> omega

/-- Hero AC absorption never reduces positive damage below 1. -/
theorem hero_absorb_pos (damage : Nat) (heroAC : Int) (absorbRoll : Nat) (h : 1 ≤ damage) :
    1 ≤ heroDamageAfterAC damage heroAC absorbRoll := by
  unfold heroDamageAfterAC
  split <;> omega

/-- Non-negative hero AC gives no damage reduction. -/
theorem hero_absorb_nonneg_ac (damage : Nat) (heroAC : Int) (absorbRoll : Nat)
    (h : 0 ≤ heroAC) : heroDamageAfterAC damage heroAC absorbRoll = damage := by
  unfold heroDamageAfterAC
  simp [h]

/-- Full damage of a landed hit is at least 1. -/
theorem calculate_damage_pos (baseRoll : Nat) (enchant bonus ac : Int) (r : Option Nat) :
    1 ≤ calculateDamage baseRoll enchant bonus ac r := by
  unfold calculateDamage
  cases r with
  | none => exact melee_damage_pos baseRoll enchant bonus
  | some r => exact hero_absorb_pos _ ac r (melee_damage_pos baseRoll enchant bonus)

/-- AC absorption never increases damage over the unreduced hit. -/
theorem calculate_damage_le_melee (baseRoll : Nat) (enchant bonus ac : Int) (r : Option Nat) :
    calculateDamage baseRoll enchant bonus ac r ≤ meleeDamage baseRoll enchant bonus := by
  unfold calculateDamage
  cases r with
  | none => exact Nat.le_refl _
  | some r => exact hero_absorb_le _ ac r

/-- Apply damage to a combatant, updating HP and death status -/
def applyDamage (c : Combatant) (dmg : Nat) : Combatant :=
  if dmg ≥ c.hp then
    { c with hp := 0, isDead := true }
  else
    { c with hp := c.hp - dmg }

/--
  Theorem: Damage application never increases HP.
-/
theorem apply_damage_monotone_hp (c : Combatant) (dmg : Nat) :
  (applyDamage c dmg).hp ≤ c.hp := by
  unfold applyDamage
  split
  · next h => exact Nat.zero_le c.hp
  · next h => exact Nat.sub_le c.hp dmg

/--
  Theorem: Damage preserves max HP.
-/
theorem apply_damage_preserves_max_hp (c : Combatant) (dmg : Nat) :
  (applyDamage c dmg).maxHp = c.maxHp := by
  unfold applyDamage
  split <;> rfl

/--
  Theorem: Zero damage leaves HP invariant.
-/
theorem apply_zero_damage_hp (c : Combatant) (hpos : c.hp > 0) :
  (applyDamage c 0).hp = c.hp := by
  unfold applyDamage
  have hnot : ¬(0 ≥ c.hp) := Nat.not_le_of_gt hpos
  simp [hnot]

/--
  Theorem: Lethal damage marks target as dead and sets HP to 0.
-/
theorem lethal_damage_kills (c : Combatant) (dmg : Nat) (h : dmg ≥ c.hp) :
  let afterDmg := applyDamage c dmg
  afterDmg.isDead = true ∧ afterDmg.hp = 0 := by
  unfold applyDamage
  simp [h]

/-- Full melee attack resolution step -/
structure AttackResult where
  hit : Bool
  damageDealt : Nat
  defenderAfter : Combatant
deriving Repr, DecidableEq

/--
  Melee attack from explicit rolls: `toHit` is the C `tmp`, `d20` the `rnd(20)` draw,
  `heroAbsorbRoll = some (rnd(-u.uac))` only when the defender is the hero.
-/
def resolveMeleeAttack
    (toHit : Int)
    (defender : Combatant)
    (d20 baseRoll : Nat)
    (enchant bonus : Int)
    (heroAbsorbRoll : Option Nat) : AttackResult :=
  if attackHits d20 toHit then
    let dmg := calculateDamage baseRoll enchant bonus defender.ac heroAbsorbRoll
    let def' := applyDamage defender dmg
    { hit := true, damageDealt := dmg, defenderAfter := def' }
  else
    { hit := false, damageDealt := 0, defenderAfter := defender }

/--
  Theorem: A missed attack deals 0 damage and leaves defender unchanged.
-/
theorem miss_leaves_defender_unchanged
    (toHit : Int) (defnd : Combatant) (d20 baseRoll : Nat) (ench bonus : Int)
    (r : Option Nat) (hmiss : attackHits d20 toHit = false) :
  (resolveMeleeAttack toHit defnd d20 baseRoll ench bonus r).defenderAfter = defnd ∧
  (resolveMeleeAttack toHit defnd d20 baseRoll ench bonus r).damageDealt = 0 := by
  unfold resolveMeleeAttack
  simp [hmiss]

/--
  Theorem: A landed attack deals at least 1 damage.
-/
theorem hit_deals_positive_damage
    (toHit : Int) (defnd : Combatant) (d20 baseRoll : Nat) (ench bonus : Int)
    (r : Option Nat) (hhit : (resolveMeleeAttack toHit defnd d20 baseRoll ench bonus r).hit = true) :
    1 ≤ (resolveMeleeAttack toHit defnd d20 baseRoll ench bonus r).damageDealt := by
  unfold resolveMeleeAttack at *
  cases h : attackHits d20 toHit
  · simp [h] at hhit
  · simp only [if_true]
    exact calculate_damage_pos baseRoll ench bonus defnd.ac r

/-! ## Monster attacks with dice (C `mattacku`/`hitmu`, `mattackm`/`mdamagem`) -/

/--
  One die draw `rnd(die)`, clamped into `1..die` (out-of-range rolls never panic).
  This is the generalised base damage roll: a die of any size `die ≥ 1`.
-/
def dieRoll (die roll : Nat) : Nat := max 1 (min die roll)

/-- A die roll lies in `1..die` for any die size `die ≥ 1`. -/
theorem die_roll_bounds (die roll : Nat) (h : 1 ≤ die) :
    1 ≤ dieRoll die roll ∧ dieRoll die roll ≤ die := by
  unfold dieRoll; omega

/-- Hit damage from a base roll of any die size is at least 1 (C `uhitm.c:1505`). -/
theorem melee_damage_die_pos (die roll : Nat) (enchant bonus : Int) :
    1 ≤ meleeDamage (dieRoll die roll) enchant bonus :=
  melee_damage_pos _ enchant bonus

/-- Sum of `n` clamped `rnd(d)` draws; a missing draw counts as 1. -/
def diceSum (d : Nat) : Nat → List Nat → Nat
  | 0, _ => 0
  | n + 1, [] => dieRoll d 1 + diceSum d n []
  | n + 1, r :: rs => dieRoll d r + diceSum d n rs

/--
  C `d(n, d)` (`rnd.c`), used by `hitmu` (`mhitu.c:1187`) and `mdamagem`
  (`mhitm.c:1025`): `n` draws of `rnd(d)`; `0` when `n = 0` or `d = 0`.
-/
def diceDamage (n d : Nat) (rolls : List Nat) : Nat :=
  if d = 0 then 0 else diceSum d n rolls

theorem dice_sum_ge (d : Nat) (h : 1 ≤ d) : ∀ (n : Nat) (rolls : List Nat),
    n ≤ diceSum d n rolls
  | 0, _ => by simp [diceSum]
  | n + 1, [] => by
      have := dice_sum_ge d h n []
      have := (die_roll_bounds d 1 h).1
      simp only [diceSum]; omega
  | n + 1, r :: rs => by
      have := dice_sum_ge d h n rs
      have := (die_roll_bounds d r h).1
      simp only [diceSum]; omega

theorem dice_sum_le (d : Nat) (h : 1 ≤ d) : ∀ (n : Nat) (rolls : List Nat),
    diceSum d n rolls ≤ n * d
  | 0, _ => by simp [diceSum]
  | n + 1, [] => by
      have := dice_sum_le d h n []
      have := (die_roll_bounds d 1 h).2
      simp only [diceSum, Nat.succ_mul]; omega
  | n + 1, r :: rs => by
      have := dice_sum_le d h n rs
      have := (die_roll_bounds d r h).2
      simp only [diceSum, Nat.succ_mul]; omega

/-- `d(n, d)` lies in `n..n*d` for `d ≥ 1`. -/
theorem dice_damage_bounds (n d : Nat) (rolls : List Nat) (h : 1 ≤ d) :
    n ≤ diceDamage n d rolls ∧ diceDamage n d rolls ≤ n * d := by
  have hd : d ≠ 0 := by omega
  unfold diceDamage
  simp only [hd, if_false]
  exact ⟨dice_sum_ge d h n rolls, dice_sum_le d h n rolls⟩

/-- Bestiary damage types (C `AD_*`) relevant to resistance. -/
inductive AttackDamage where
  | phys
  | fire
  | cold
  | drainStr
  | other
deriving Repr, DecidableEq

/--
  Resistance zeroes the damage only for AD_FIRE / AD_COLD
  (`mhitm_ad_fire` `uhitm.c:2521`, `mhitm_ad_cold` `uhitm.c:2626`).
-/
def resisted (ad : AttackDamage) (fireRes coldRes : Bool) : Bool :=
  match ad with
  | .fire => fireRes
  | .cold => coldRes
  | _ => false

/--
  Damage of one landed monster attack (C `hitmu` `mhitu.c:1187-1211`):
  `d(n, d)`, `0` if resisted, then for a hero defender (`hero = some (u.uac, rnd(-u.uac))`)
  the negative-AC absorption. Monster defenders (`none`) take no absorption.
-/
def monsterHitDamage (n d : Nat) (rolls : List Nat) (isResisted : Bool)
    (hero : Option (Int × Nat)) : Nat :=
  let dmg := if isResisted then 0 else diceDamage n d rolls
  match hero with
  | none => dmg
  | some (ac, r) => heroDamageAfterAC dmg ac r

/-- A resisted attack deals 0 damage. -/
theorem resisted_hit_zero (n d : Nat) (rolls : List Nat) (hero : Option (Int × Nat)) :
    monsterHitDamage n d rolls true hero = 0 := by
  unfold monsterHitDamage
  cases hero with
  | none => rfl
  | some p => simp [heroDamageAfterAC]

/-- A resisted attack leaves the defender's HP unchanged. -/
theorem resisted_hit_hp_unchanged (c : Combatant) (n d : Nat) (rolls : List Nat)
    (hero : Option (Int × Nat)) :
    (applyDamage c (monsterHitDamage n d rolls true hero)).hp = c.hp := by
  rw [resisted_hit_zero]
  unfold applyDamage
  split
  · next h => show 0 = c.hp; omega
  · rfl

/-- An unresisted landed attack with at least one die (`n ≥ 1`, `d ≥ 1`) deals ≥ 1. -/
theorem unresisted_hit_pos (n d : Nat) (rolls : List Nat) (hero : Option (Int × Nat))
    (hn : 1 ≤ n) (hd : 1 ≤ d) :
    1 ≤ monsterHitDamage n d rolls false hero := by
  have hb := (dice_damage_bounds n d rolls hd).1
  unfold monsterHitDamage
  cases hero with
  | none => simp; omega
  | some p =>
      obtain ⟨ac, r⟩ := p
      simp only [Bool.false_eq_true, if_false]
      exact hero_absorb_pos _ ac r (by omega)

/-- Monster damage to a monster never exceeds `n * d` (no hero absorption). -/
theorem monster_vs_monster_damage_le (n d : Nat) (rolls : List Nat) (b : Bool) (hd : 1 ≤ d) :
    monsterHitDamage n d rolls b none ≤ n * d := by
  have := (dice_damage_bounds n d rolls hd).2
  unfold monsterHitDamage
  cases b <;> simp <;> omega

/--
  Monster-vs-monster to-hit (C `mattackm`, `mhitm.c:321`/`:441`):
  `(tmp, die) = (find_mac(mdef) + m_lev, 20 + i)` — no `+10`.
-/
def mhitmToHit (attackerLevel defenderAC : Int) (i : Nat) : Int × Nat :=
  (defenderAC + attackerLevel, 20 + i)

/-- Per-attack hit test `tmp > rnd(die)` with the roll clamped into `1..die`. -/
def monsterAttackHits (tmp : Int) (die roll : Nat) : Bool :=
  decide (((dieRoll die roll : Nat) : Int) < tmp)

/-- mhitm to-hit carries no `+10` term and uses the `rnd(20 + i)` die. -/
theorem mhitm_no_plus_ten (lvl ac : Int) (i : Nat) :
    mhitmToHit lvl ac i = (ac + lvl, 20 + i) := rfl

/-- `tmp ≤ 1` never hits on any die (e.g. a level-2 pet vs AC -1). -/
theorem monster_attack_never_hits_le_1 (tmp : Int) (die roll : Nat) (h : tmp ≤ 1) :
    monsterAttackHits tmp die roll = false := by
  unfold monsterAttackHits dieRoll
  simp only [decide_eq_false_iff_not]
  omega

/-! ## Hero weapon damage (C `dmgval`, `weapon.c:216-293`, `uhitm.c:847`) -/

/--
  Weapon damage die (C `weapon.c:216-293`, `uhitm.c:847`):
  - Bare hands (`none`): `rnd(4)` with martial arts, `rnd(2)` without.
  - Weapon (`some (small, large)`): `rnd(large)` vs large targets, `rnd(small)` otherwise.
-/
def weaponDamageDie (weapon : Option (Nat × Nat)) (targetLarge martialArts : Bool) : Nat :=
  match weapon with
  | some (s, l) => if targetLarge then l else s
  | none => if martialArts then 4 else 2

/--
  Base weapon damage (C `dmgval`, `weapon.c:216-293` and `uhitm.c:847`).
  Draws `rnd(die)` with `die = weaponDamageDie weapon targetLarge martialArts`.
  `0` when `die = 0`.
-/
def dmgval (weapon : Option (Nat × Nat)) (targetLarge martialArts : Bool) (roll : Nat) : Nat :=
  let die := weaponDamageDie weapon targetLarge martialArts
  if die = 0 then 0 else dieRoll die roll

/-- For any weapon with positive die, `dmgval` is bounded by `1..die`. -/
theorem dmgval_bounds (weapon : Option (Nat × Nat)) (targetLarge martialArts : Bool) (roll : Nat)
    (h : 1 ≤ weaponDamageDie weapon targetLarge martialArts) :
    1 ≤ dmgval weapon targetLarge martialArts roll ∧
    dmgval weapon targetLarge martialArts roll ≤ weaponDamageDie weapon targetLarge martialArts := by
  have hd : weaponDamageDie weapon targetLarge martialArts ≠ 0 := by omega
  unfold dmgval
  simp only [hd, if_false]
  exact die_roll_bounds (weaponDamageDie weapon targetLarge martialArts) roll h

/-- For any weapon with positive die, hit damage with dmgval is at least 1 (C `uhitm.c:1505`). -/
theorem melee_damage_dmgval_pos (weapon : Option (Nat × Nat)) (targetLarge martialArts : Bool)
    (roll : Nat) (enchant bonus : Int)
    (h : 1 ≤ weaponDamageDie weapon targetLarge martialArts) :
    1 ≤ meleeDamage (dmgval weapon targetLarge martialArts roll) enchant bonus := by
  have hb := (dmgval_bounds weapon targetLarge martialArts roll h).1
  exact melee_damage_pos _ enchant bonus

/-! ## Armor Class and ARM_BONUS (C `ARM_BONUS`, `hack.h:1526-1528`, `find_ac`, `do_wear.c:2473-2507`) -/

/--
  NetHack 5.0 C `ARM_BONUS(obj)` (`include/hack.h:1526-1528`):
  `a_ac + spe - min(erosion, a_ac)` (erosion cannot reduce `a_ac` below 0).
-/
def armBonus (a_ac : Int) (spe : Int) (erosion : Nat) : Int :=
  let ero := min (erosion : Int) (max 0 a_ac)
  a_ac + spe - ero

/-- For non-negative `a_ac`, `armBonus` is bounded below by `spe` and above by `a_ac + spe`. -/
theorem arm_bonus_bounds (a_ac spe : Int) (erosion : Nat) (ha : 0 ≤ a_ac) :
    spe ≤ armBonus a_ac spe erosion ∧ armBonus a_ac spe erosion ≤ a_ac + spe := by
  have he : 0 ≤ (erosion : Int) := by omega
  unfold armBonus
  simp only []
  omega

/-- Total AC bonus of a list of worn armor items `(a_ac, spe, erosion)`. -/
def armorListBonus (worn : List (Int × Int × Nat)) : Int :=
  (worn.map (fun p => armBonus p.1 p.2.1 p.2.2)).sum

/--
  NetHack 5.0 C `find_ac(void)` (`src/do_wear.c:2473-2507`):
  Hero AC computed from `baseAc` (10 for human form), worn armor pieces, and divine `protection`.
  Clamped to `[-99, 99]` (C `AC_MAX`, `include/you.h:472`).
-/
def findAc (baseAc : Int) (worn : List (Int × Int × Nat)) (protection : Int) : Int :=
  let uncurbed := baseAc - armorListBonus worn - protection
  if uncurbed < -99 then -99 else if uncurbed > 99 then 99 else uncurbed

/-- Adding an armor piece with non-negative bonus never increases AC (monotone in worn armor). -/
theorem find_ac_monotonic_armor_piece (baseAc : Int) (worn : List (Int × Int × Nat)) (protection : Int)
    (p : Int × Int × Nat) (hp : 0 ≤ armBonus p.1 p.2.1 p.2.2) :
    findAc baseAc (p :: worn) protection ≤ findAc baseAc worn protection := by
  unfold findAc armorListBonus
  simp only [List.map_cons, List.sum_cons]
  repeat (split <;> try omega)

/-- Divine protection never increases AC (monotone in protection). -/
theorem find_ac_monotonic_protection (baseAc : Int) (worn : List (Int × Int × Nat)) (prot1 prot2 : Int)
    (h : prot1 ≤ prot2) :
    findAc baseAc worn prot2 ≤ findAc baseAc worn prot1 := by
  unfold findAc
  dsimp only []
  repeat (split <;> try omega)

/-- Every worn piece with `a_ac ≥ 0` and `spe ≥ 0` contributes a non-negative total bonus. -/
theorem armor_list_bonus_nonneg (worn : List (Int × Int × Nat))
    (h : ∀ p ∈ worn, 0 ≤ p.1 ∧ 0 ≤ p.2.1) :
    0 ≤ armorListBonus worn := by
  induction worn with
  | nil => simp [armorListBonus]
  | cons p rest ih =>
    have hp := h p (List.mem_cons_self ..)
    have hrest : ∀ q ∈ rest, 0 ≤ q.1 ∧ 0 ≤ q.2.1 :=
      fun q hq => h q (List.mem_cons_of_mem _ hq)
    have hb := (arm_bonus_bounds p.1 p.2.1 p.2.2 hp.1).1
    have ihr := ih hrest
    unfold armorListBonus at ihr ⊢
    simp only [List.map_cons, List.sum_cons]
    omega

/--
  Human-form hero AC (`baseAc = 10`, `do_wear.c:2475`) never exceeds 10 when every worn
  piece has `a_ac ≥ 0` and `spe ≥ 0` and divine protection is non-negative
  (`do_wear.c:2478-2501`, `hack.h:1526-1528`).
-/
theorem find_ac_le_base_nonneg_armor (worn : List (Int × Int × Nat)) (protection : Int)
    (h : ∀ p ∈ worn, 0 ≤ p.1 ∧ 0 ≤ p.2.1) (hprot : 0 ≤ protection) :
    findAc 10 worn protection ≤ 10 := by
  have hb := armor_list_bonus_nonneg worn h
  unfold findAc
  dsimp only []
  repeat (split <;> try omega)

/-- `findAc` is always clamped within `[-99, 99]` (C `AC_MAX`). -/
theorem find_ac_bounds (baseAc : Int) (worn : List (Int × Int × Nat)) (protection : Int) :
    -99 ≤ findAc baseAc worn protection ∧ findAc baseAc worn protection ≤ 99 := by
  unfold findAc
  dsimp only []
  constructor <;> (split <;> (try split) <;> omega)

end NetMechanics

