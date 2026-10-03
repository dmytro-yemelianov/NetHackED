/-
  NetHack Mechanics Formalized in Lean 4: Combat & Damage Resolution
  Formalizing to-hit calculation, AC reduction, and HP clamping.
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
  NetHack's to-hit threshold formula (uhitm.c:376):
    threshold = 1 + abon() + find_mac(target) + uhitinc
  Lower AC is better for defense (AD&D 1e convention), reducing the threshold.
  A d20 roll succeeds if (roll : Int) ≤ threshold.
-/
def toHitThreshold (attackerBonus : Int) (targetAC : Int) : Int :=
  10 + targetAC + attackerBonus

/-- Check if an attack roll lands given d20 roll (1..20) -/
def attackLands (d20Roll : Nat) (threshold : Int) : Bool :=
  (d20Roll : Int) ≤ threshold

/--
  Calculate damage given roll, enchant, attacker stat bonus, and defender AC.
  In NetHack (mhitu.c:1208), negative AC absorbs damage:
    damage -= rnd(-ac), clamped to at least 1 if hit lands.
-/
def calculateDamage (roll : Nat) (enchant : Int) (bonus : Int) (defenderAC : Int := 0) : Nat :=
  let total := (roll : Int) + enchant + bonus
  if total > 0 then
    let raw := total.toNat
    if defenderAC < 0 then
      let absorb := (-defenderAC).toNat
      max 1 (raw - absorb)
    else
      raw
  else
    0

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

def resolveMeleeAttack
    (attackerBonus : Int)
    (attackerDmgBonus : Int)
    (defender : Combatant)
    (d20Roll : Nat)
    (dmgRoll : Nat)
    (weaponEnchant : Int := 0) : AttackResult :=
  let thresh := toHitThreshold attackerBonus defender.ac
  if attackLands d20Roll thresh then
    let dmg := calculateDamage dmgRoll weaponEnchant attackerDmgBonus defender.ac
    let def' := applyDamage defender dmg
    { hit := true, damageDealt := dmg, defenderAfter := def' }
  else
    { hit := false, damageDealt := 0, defenderAfter := defender }

/--
  Theorem: A missed attack deals 0 damage and leaves defender unchanged.
-/
theorem miss_leaves_defender_unchanged
    (attBonus attDmgBonus : Int) (defnd : Combatant)
    (d20Roll dmgRoll : Nat) (ench : Int)
    (hmiss : attackLands d20Roll (toHitThreshold attBonus defnd.ac) = false) :
  (resolveMeleeAttack attBonus attDmgBonus defnd d20Roll dmgRoll ench).defenderAfter = defnd ∧
  (resolveMeleeAttack attBonus attDmgBonus defnd d20Roll dmgRoll ench).damageDealt = 0 := by
  unfold resolveMeleeAttack
  simp [hmiss]

end NetMechanics
