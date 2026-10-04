/-
  NetHack Mechanics Formalized in Lean 4: Item Enchantment, Erosion & Alchemy
  Formalizes enchant armor / enchant weapon (read.c:1115, read.c:1667, wield.c:999),
  erosion monotonicity, proofing invariants, and deterministic alchemy transformations.
-/
import NetMechanics.BUC

namespace NetMechanics

/-- C integer division (truncates toward zero) by a positive literal. -/
def cDiv (x : Int) (n : Nat) : Int :=
  if 0 ≤ x then x / n else -((-x) / n)

/-- Clamp a roll into C `rn2(n)` range `0..n-1`. -/
def clampRn2 (n : Int) (roll : Nat) : Int := min (roll : Int) (n - 1)

/-- Clamp a roll into C `rnd(n)` range `1..n`. -/
def clampRnd (n : Int) (roll : Nat) : Int := max 1 (min (roll : Int) n)

/-- Weapon safe limit (wield.c:999): a non-cursed scroll can evaporate only above +5. -/
def weaponSafeLimit : Int := 5

/-- Armor safe limit (read.c:1179): +3, or +5 for elven armor / Wizard's cornuthaum. -/
def armorSafeLimit (special : Bool) : Int := if special then 5 else 3

/-- Outcome of reading an enchant scroll: new enchantment, or the item evaporates. -/
inductive EnchantOutcome where
  | changed (spe : Int)
  | evaporated
deriving Repr, DecidableEq

/-- Enchant armor's working value before the evaporation check: `scursed ? -spe : spe`. -/
def armorS (spe : Int) (buc : BUC) : Int :=
  if buc = BUC.Cursed then -spe else spe

/-- Enchant armor gain die (read.c:1180-1183): `(4 - s) / 2`, +1 special, +1 non-magic, +1 blessed. -/
def armorGainDie (spe : Int) (buc : BUC) (special magical : Bool) : Int :=
  cDiv (4 - armorS spe buc) 2 + (if special then 1 else 0) + (if magical then 0 else 1)
    + (if buc = BUC.Blessed then 1 else 0)

/-- Scroll of enchant armor (read.c:1115 `seffect_enchant_armor`). Rolls: `evapRoll` is
    `rn2(s)`, `gainRoll` is `rnd(d)` or `rn2(spe)`; both clamped into range. -/
def enchantArmor (spe : Int) (buc : BUC) (special magical : Bool) (evapRoll gainRoll : Nat) :
    EnchantOutcome :=
  let s := armorS spe buc
  if s > armorSafeLimit special ∧ clampRn2 s evapRoll ≠ 0 then
    EnchantOutcome.evaporated
  else
    let d := armorGainDie spe buc special magical
    let g : Int :=
      if d ≤ 0 then (if 0 < spe ∧ clampRn2 spe gainRoll = 0 then 1 else 0)
      else clampRnd d gainRoll
    let g := min g 11
    EnchantOutcome.changed (spe + (if buc = BUC.Cursed then -g else g))

/-- Enchant weapon amount (read.c:1667):
    `scursed ? -1 : spe >= 9 ? (rn2(spe) == 0) : sblessed ? rnd(3 - spe/3) : 1`. -/
def weaponAmount (spe : Int) (buc : BUC) (gainRoll : Nat) : Int :=
  if buc = BUC.Cursed then -1
  else if 9 ≤ spe then (if clampRn2 spe gainRoll = 0 then 1 else 0)
  else if buc = BUC.Blessed then clampRnd (3 - cDiv spe 3) gainRoll
  else 1

/-- Scroll of enchant weapon on the wielded weapon (wield.c:999-1000 `chwepon`):
    evaporates on `rn2(3) ≠ 0` when `spe > 5 ∧ amount ≥ 0` or `spe < -5 ∧ amount < 0`. -/
def enchantWeapon (spe : Int) (buc : BUC) (evapRoll gainRoll : Nat) : EnchantOutcome :=
  let amount := weaponAmount spe buc gainRoll
  if ((spe > 5 ∧ amount ≥ 0) ∨ (spe < -5 ∧ amount < 0)) ∧ clampRn2 3 evapRoll ≠ 0 then
    EnchantOutcome.evaporated
  else
    EnchantOutcome.changed (spe + amount)

/-- THEOREM: a non-cursed enchant weapon scroll never evaporates a weapon at or below +5,
    for every roll. -/
theorem enchant_weapon_safe_le_limit (spe : Int) (buc : BUC) (e g : Nat)
    (h_buc : buc ≠ BUC.Cursed) (h_lim : spe ≤ weaponSafeLimit) :
    enchantWeapon spe buc e g ≠ EnchantOutcome.evaporated := by
  have ha : weaponAmount spe buc g ≥ 0 := by
    unfold weaponAmount clampRnd clampRn2
    simp only [h_buc, if_false]
    split <;> (try split) <;> omega
  have h5 : spe ≤ 5 := h_lim
  unfold enchantWeapon
  simp only
  rw [if_neg (by omega)]
  simp

/-- THEOREM: a non-cursed enchant weapon scroll strictly raises a weapon at or below +5. -/
theorem enchant_weapon_increases_le_limit (spe : Int) (buc : BUC) (e g : Nat)
    (h_buc : buc ≠ BUC.Cursed) (h_lim : spe ≤ weaponSafeLimit) :
    enchantWeapon spe buc e g = EnchantOutcome.changed (spe + weaponAmount spe buc g) ∧
      0 < weaponAmount spe buc g := by
  have h5 : spe ≤ 5 := h_lim
  have ha : weaponAmount spe buc g > 0 := by
    unfold weaponAmount clampRnd
    simp only [h_buc, if_false]
    rw [if_neg (by omega)]
    split <;> omega
  refine ⟨?_, ha⟩
  unfold enchantWeapon
  simp only
  rw [if_neg (by omega)]

/-- THEOREM: enchant weapon evaporates only above the limit (+5 non-cursed, −5 cursed). -/
theorem enchant_weapon_evaporates_only_beyond_limit (spe : Int) (buc : BUC) (e g : Nat)
    (h : enchantWeapon spe buc e g = EnchantOutcome.evaporated) :
    (buc ≠ BUC.Cursed ∧ spe > weaponSafeLimit) ∨ (buc = BUC.Cursed ∧ spe < -weaponSafeLimit) := by
  unfold enchantWeapon at h
  simp only at h
  split at h
  · rename_i hc
    unfold weaponSafeLimit
    by_cases hb : buc = BUC.Cursed
    · have : weaponAmount spe buc g = -1 := by simp [weaponAmount, hb]
      right; exact ⟨hb, by omega⟩
    · have : weaponAmount spe buc g ≥ 0 := by
        unfold weaponAmount clampRnd clampRn2
        simp only [hb, if_false]
        split <;> (try split) <;> omega
      left; exact ⟨hb, by omega⟩
  · cases h

/-- THEOREM: a cursed enchant weapon scroll lowers a weapon at or above −5 by exactly 1. -/
theorem enchant_weapon_cursed_decrements (spe : Int) (e g : Nat) (h_lim : -weaponSafeLimit ≤ spe) :
    enchantWeapon spe BUC.Cursed e g = EnchantOutcome.changed (spe - 1) := by
  unfold enchantWeapon weaponAmount weaponSafeLimit at *
  simp only [if_true]
  rw [if_neg (by omega)]
  congr 1

/-- THEOREM: a non-cursed enchant armor scroll never evaporates armor at or below its limit
    (+3, +5 special), for every roll. -/
theorem enchant_armor_safe_le_limit (spe : Int) (buc : BUC) (special magical : Bool) (e g : Nat)
    (h_buc : buc ≠ BUC.Cursed) (h_lim : spe ≤ armorSafeLimit special) :
    enchantArmor spe buc special magical e g ≠ EnchantOutcome.evaporated := by
  unfold enchantArmor
  have hs : armorS spe buc = spe := by simp [armorS, h_buc]
  simp only [hs]
  rw [if_neg (by omega)]
  simp

/-- THEOREM: enchant armor evaporates only when `s = scursed ? -spe : spe` exceeds the limit. -/
theorem enchant_armor_evaporates_only_beyond_limit (spe : Int) (buc : BUC)
    (special magical : Bool) (e g : Nat)
    (h : enchantArmor spe buc special magical e g = EnchantOutcome.evaporated) :
    armorS spe buc > armorSafeLimit special := by
  unfold enchantArmor at h
  simp only at h
  split at h
  · rename_i hc; exact hc.1
  · cases h

/-- THEOREM: a non-cursed enchant armor scroll never lowers enchantment (the gain may be 0,
    e.g. magic armor at +3). -/
theorem enchant_armor_nondecreasing (spe v : Int) (buc : BUC) (special magical : Bool) (e g : Nat)
    (h_buc : buc ≠ BUC.Cursed)
    (h : enchantArmor spe buc special magical e g = EnchantOutcome.changed v) :
    spe ≤ v := by
  unfold enchantArmor at h
  simp only [h_buc, if_false] at h
  split at h
  · cases h
  · injection h with h
    unfold clampRnd at h
    split at h <;> (try split at h) <;> omega

/-- THEOREM: a non-cursed enchant armor scroll strictly raises armor at or below +2. -/
theorem enchant_armor_increases_le_two (spe : Int) (buc : BUC) (special magical : Bool) (e g : Nat)
    (h_buc : buc ≠ BUC.Cursed) (h_lim : spe ≤ 2) :
    ∃ v, enchantArmor spe buc special magical e g = EnchantOutcome.changed v ∧ spe < v := by
  have hd : 1 ≤ armorGainDie spe buc special magical := by
    unfold armorGainDie armorS cDiv
    simp only [h_buc, if_false]
    rw [if_pos (by omega)]
    split <;> split <;> split <;> omega
  unfold enchantArmor
  have hs : armorS spe buc = spe := by simp [armorS, h_buc]
  simp only [hs, h_buc, if_false]
  rw [if_neg (by unfold armorSafeLimit; split <;> omega)]
  refine ⟨_, rfl, ?_⟩
  rw [if_neg (by omega)]
  unfold clampRnd
  omega

/-- Discrete item erosion levels (0 = None, 1 = Light, 2 = Moderate, 3 = Heavy, 4 = Destroyed) -/
def maxErosion : Nat := 4

structure ItemErosion where
  level   : Nat
  proofed : Bool
deriving Repr, DecidableEq

/-- Corrosive exposure (acid, water, rust, fire) -/
def applyErosion (item : ItemErosion) : ItemErosion :=
  if item.proofed then
    item
  else if item.level < maxErosion then
    ⟨item.level + 1, false⟩
  else
    item

/-- THEOREM: Proofed items are completely immune to erosion exposure -/
theorem proofed_impermeable (item : ItemErosion) (h_proof : item.proofed = true) :
    applyErosion item = item := by
  dsimp [applyErosion]
  rw [if_pos h_proof]

/-- THEOREM: Erosion is monotonic under corrosive exposure -/
theorem erosion_monotonic (item : ItemErosion) :
    (applyErosion item).level ≥ item.level := by
  dsimp [applyErosion]
  split
  · omega
  · split
    · dsimp
      omega
    · omega

/-- Canonical NetHack Alchemy Potions -/
inductive AlchemyPotion where
  | Water
  | Healing
  | ExtraHealing
  | Speed
  | GainEnergy
  | Sickness
deriving Repr, DecidableEq

/-- Deterministic Alchemy Dipping Matrix -/
def mixAlchemy (potA potB : AlchemyPotion) : Option AlchemyPotion :=
  match potA, potB with
  | AlchemyPotion.Healing, AlchemyPotion.GainEnergy => some AlchemyPotion.ExtraHealing
  | AlchemyPotion.GainEnergy, AlchemyPotion.Healing => some AlchemyPotion.ExtraHealing
  | AlchemyPotion.Healing, AlchemyPotion.Speed      => some AlchemyPotion.ExtraHealing
  | AlchemyPotion.Speed, AlchemyPotion.Healing      => some AlchemyPotion.ExtraHealing
  | AlchemyPotion.Water, _                          => some AlchemyPotion.Water
  | _, _                                            => none

/-- THEOREM: Potion alchemy mixing is commutative for complementary reagents -/
theorem alchemy_healing_energy_commutative :
    mixAlchemy AlchemyPotion.Healing AlchemyPotion.GainEnergy =
    mixAlchemy AlchemyPotion.GainEnergy AlchemyPotion.Healing := by
  rfl

end NetMechanics
