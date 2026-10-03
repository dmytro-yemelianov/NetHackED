/-
  NetHack Mechanics Formalized in Lean 4: Item Enchantment, Erosion & Alchemy
  Formalizes enchantment safe bounds, erosion monotonicity, proofing invariants,
  and deterministic alchemy transformations.
-/

namespace NetMechanics

/-- Safe enchantment cap in NetHack (+7) -/
def safeEnchantCap : Int := 7

/-- Result of reading an enchantment scroll -/
structure EnchantResult where
  newEnch   : Int
  evaporated : Bool
deriving Repr, DecidableEq

/-- Enchant an item: blessed gives +1 to +3 (abstracted as +1 or +2), cursed gives -1 -/
def enchantItem (curEnch : Int) (isBlessed : Bool) (isCursed : Bool) : EnchantResult :=
  if isCursed then
    ⟨curEnch - 1, false⟩
  else if curEnch ≥ safeEnchantCap then
    -- Exceeding safe cap risks item vaporization
    ⟨curEnch, true⟩
  else
    let delta := if isBlessed then 2 else 1
    ⟨curEnch + delta, false⟩

/-- THEOREM: Enchanting an item below the safe cap with an uncursed or blessed scroll never evaporates -/
theorem enchant_below_cap_safe (curEnch : Int) (blessed : Bool) (h_cap : curEnch < safeEnchantCap) :
    (enchantItem curEnch blessed false).evaporated = false := by
  dsimp [enchantItem]
  have h_not_ge : ¬(curEnch ≥ safeEnchantCap) := by omega
  rw [if_neg h_not_ge]

/-- THEOREM: Enchanting an item below the safe cap strictly increases enchantment value -/
theorem enchant_below_cap_increases (curEnch : Int) (blessed : Bool) (h_cap : curEnch < safeEnchantCap) :
    (enchantItem curEnch blessed false).newEnch > curEnch := by
  dsimp [enchantItem]
  have h_not_ge : ¬(curEnch ≥ safeEnchantCap) := by omega
  rw [if_neg h_not_ge]
  dsimp
  split <;> omega

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
