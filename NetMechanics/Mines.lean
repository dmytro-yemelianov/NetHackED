/-
  NetMechanics.Mines: Formalized Gnomish Mines, Minetown Temple, and Luckstone Mechanics.
  Models branch structure, Minetown priest protection donations, item uncursing,
  and Mines' End Luckstone positive luck preservation theorems.
-/

import NetMechanics.Basic
import NetMechanics.Grid
import NetMechanics.BUC

namespace NetMechanics

/-- Sub-level classification of the Gnomish Mines -/
inductive MinesSubLevel where
  | Cavern (subDepth : Nat)
  | Minetown
  | MinesEnd
deriving Repr, DecidableEq

/-- Discrete depth of a Mines level within the branch [1, 5] -/
def minesSubDepth : MinesSubLevel → Nat
  | MinesSubLevel.Cavern d => d
  | MinesSubLevel.Minetown => 3
  | MinesSubLevel.MinesEnd => 5

/-- Theorem: All predefined Mines levels lie within depth [1, 5] -/
theorem minetown_depth_valid : minesSubDepth MinesSubLevel.Minetown = 3 := rfl
theorem mines_end_depth_valid : minesSubDepth MinesSubLevel.MinesEnd = 5 := rfl

/-- Maximum divine AC protection purchasable from a temple priest -/
def maxDivineProtection : Nat := 9

/-- Cost per character level for divine protection donation attempt -/
def protectionDonationCost (heroLevel : Nat) : Nat :=
  400 * heroLevel

/-- Calculate resulting divine protection after a donation -/
def applyPriestDonation (currentProt : Nat) (donationAmount : Nat) (heroLevel : Nat) : Nat :=
  if currentProt ≥ maxDivineProtection then
    currentProt
  else if donationAmount ≥ protectionDonationCost heroLevel then
    currentProt + 1
  else
    currentProt

/-- THEOREM: Divine protection is strictly capped at maxDivineProtection (9) when starting within bounds -/
theorem priest_protection_bounded (cur : Nat) (amount : Nat) (lvl : Nat)
    (h_cur : cur ≤ maxDivineProtection) :
    applyPriestDonation cur amount lvl ≤ maxDivineProtection := by
  dsimp [applyPriestDonation, maxDivineProtection]
  split
  · exact h_cur
  · split <;> omega

/-- THEOREM: Divine protection is monotonic (never decreases from a donation) -/
theorem priest_protection_monotonic (cur : Nat) (amount : Nat) (lvl : Nat) :
    cur ≤ applyPriestDonation cur amount lvl := by
  dsimp [applyPriestDonation, maxDivineProtection]
  split
  · omega
  · split <;> omega

/-- THEOREM: Insufficient gold donation guarantees protection is unchanged -/
theorem priest_protection_insufficient (cur : Nat) (amount : Nat) (lvl : Nat)
    (h_cur : cur < maxDivineProtection)
    (h_insuf : amount < protectionDonationCost lvl) :
    applyPriestDonation cur amount lvl = cur := by
  dsimp [applyPriestDonation]
  split
  · omega
  · split
    · omega
    · rfl

/-- Priest uncurses a BUC item when purified -/
def priestUncurse (itemBuc : BUC) : BUC :=
  if itemBuc == BUC.Cursed then BUC.Uncursed else itemBuc

/-- THEOREM: A priest-purified item is never cursed -/
theorem priest_uncurse_never_cursed (b : BUC) :
    priestUncurse b ≠ BUC.Cursed := by
  dsimp [priestUncurse]
  cases b <;> decide

/-- Luckstone status carried by the player -/
inductive LuckstoneCarried where
  | None
  | Blessed
  | Uncursed
  | Cursed
deriving Repr, DecidableEq

/-- Clamp raw luck to canonical NetHack limits [-10, 10] -/
def clampLuck (luck : Int) : Int :=
  if luck > 10 then 10 else if luck < -10 then -10 else luck

/-- THEOREM: Clamped luck is within [-10, 10] -/
theorem clamp_luck_bounded (l : Int) : -10 ≤ clampLuck l ∧ clampLuck l ≤ 10 := by
  dsimp [clampLuck]
  split
  · omega
  · split <;> omega

/-- Single luck decay step (normally fires every 600 turns) -/
def stepLuckDecay (rawLuck : Int) (stone : LuckstoneCarried) : Int :=
  match stone with
  | LuckstoneCarried.Blessed | LuckstoneCarried.Uncursed =>
    -- Non-cursed luckstone: positive luck NEVER decays; negative luck recovers!
    if rawLuck > 0 then rawLuck
    else if rawLuck < 0 then rawLuck + 1
    else 0
  | LuckstoneCarried.Cursed =>
    -- Cursed luckstone: positive luck decays; negative luck NEVER recovers!
    if rawLuck > 0 then rawLuck - 1
    else if rawLuck < 0 then rawLuck
    else 0
  | LuckstoneCarried.None =>
    -- No luckstone: natural decay toward 0 from both directions
    if rawLuck > 0 then rawLuck - 1
    else if rawLuck < 0 then rawLuck + 1
    else 0

/-- THEOREM: Carrying a non-cursed luckstone strictly preserves positive luck -/
theorem luckstone_preserves_positive_luck (l : Int) (stone : LuckstoneCarried)
    (h_pos : l > 0)
    (h_stone : stone = LuckstoneCarried.Blessed ∨ stone = LuckstoneCarried.Uncursed) :
    stepLuckDecay l stone = l := by
  cases h_stone with
  | inl h =>
    rw [h]
    dsimp [stepLuckDecay]
    rw [if_pos h_pos]
  | inr h =>
    rw [h]
    dsimp [stepLuckDecay]
    rw [if_pos h_pos]

/-- THEOREM: Carrying a non-cursed luckstone strictly improves negative luck toward 0 -/
theorem luckstone_heals_negative_luck (l : Int) (stone : LuckstoneCarried)
    (h_neg : l < 0)
    (h_stone : stone = LuckstoneCarried.Blessed ∨ stone = LuckstoneCarried.Uncursed) :
    stepLuckDecay l stone = l + 1 := by
  cases h_stone with
  | inl h =>
    rw [h]
    dsimp [stepLuckDecay]
    have h_not_pos : ¬(l > 0) := by omega
    rw [if_neg h_not_pos]
    rw [if_pos h_neg]
  | inr h =>
    rw [h]
    dsimp [stepLuckDecay]
    have h_not_pos : ¬(l > 0) := by omega
    rw [if_neg h_not_pos]
    rw [if_pos h_neg]

/-- THEOREM: Luck decay respects canonical luck bounds [-10, 10] -/
theorem step_luck_bounds_preserved (l : Int) (stone : LuckstoneCarried)
    (h_low : -10 ≤ l) (h_high : l ≤ 10) :
    -10 ≤ stepLuckDecay l stone ∧ stepLuckDecay l stone ≤ 10 := by
  dsimp [stepLuckDecay]
  cases stone
  · split <;> split <;> omega
  · split <;> split <;> omega
  · split <;> split <;> omega
  · split <;> split <;> omega

end NetMechanics
