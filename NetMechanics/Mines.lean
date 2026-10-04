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
  · rename_i h; omega
  · split
    · rename_i h h'; omega
    · omega

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
  · rename_i h; omega
  · split
    · rename_i h h'; omega
    · omega

/-- Single luck timeout step toward `baseLuck` (timeout.c:595-620, attrib.c:423).
    No stone: both directions. Blessed: only luck below base recovers.
    Uncursed: frozen. Cursed: only luck above base decays. -/
def luckCanDecay : LuckstoneCarried → Bool
  | LuckstoneCarried.None => true
  | LuckstoneCarried.Cursed => true
  | _ => false

def luckCanRecover : LuckstoneCarried → Bool
  | LuckstoneCarried.None => true
  | LuckstoneCarried.Blessed => true
  | _ => false

def stepLuckDecay (rawLuck baseLuck : Int) (stone : LuckstoneCarried) : Int :=
  if rawLuck > baseLuck ∧ luckCanDecay stone = true then rawLuck - 1
  else if rawLuck < baseLuck ∧ luckCanRecover stone = true then rawLuck + 1
  else rawLuck

/-- THEOREM: A non-cursed luckstone preserves luck above base -/
theorem luckstone_preserves_positive_luck (l base : Int) (stone : LuckstoneCarried)
    (h_pos : l > base)
    (h_stone : stone = LuckstoneCarried.Blessed ∨ stone = LuckstoneCarried.Uncursed) :
    stepLuckDecay l base stone = l := by
  have h1 : ¬ (l > base ∧ luckCanDecay stone = true) := by
    rcases h_stone with h | h <;> subst h <;> simp [luckCanDecay]
  have h2 : ¬ (l < base ∧ luckCanRecover stone = true) := by
    intro ⟨hl, _⟩; omega
  simp only [stepLuckDecay, if_neg h1, if_neg h2]

/-- THEOREM: A blessed luckstone lets luck below base recover by one
    (renamed from `luckstone_heals_negative_luck`, which wrongly included Uncursed). -/
theorem blessed_luckstone_heals_negative_luck (l base : Int)
    (h_neg : l < base) :
    stepLuckDecay l base LuckstoneCarried.Blessed = l + 1 := by
  have h1 : ¬ (l > base ∧ luckCanDecay LuckstoneCarried.Blessed = true) := by
    intro ⟨hl, _⟩; omega
  have h2 : (l < base ∧ luckCanRecover LuckstoneCarried.Blessed = true) :=
    ⟨h_neg, by decide⟩
  simp only [stepLuckDecay, if_neg h1, if_pos h2]

/-- THEOREM: An uncursed luckstone freezes luck entirely (C: neither direction times out) -/
theorem uncursed_luckstone_freezes_luck (l base : Int) :
    stepLuckDecay l base LuckstoneCarried.Uncursed = l := by
  simp [stepLuckDecay, luckCanDecay, luckCanRecover]

/-- THEOREM: Luck decay respects canonical luck bounds [-10, 10] -/
theorem step_luck_bounds_preserved (l base : Int) (stone : LuckstoneCarried)
    (h_low : -10 ≤ l) (h_high : l ≤ 10) (hb_low : -10 ≤ base) (hb_high : base ≤ 10) :
    -10 ≤ stepLuckDecay l base stone ∧ stepLuckDecay l base stone ≤ 10 := by
  unfold stepLuckDecay
  split
  · rename_i h; omega
  · split
    · rename_i h h'; omega
    · omega

/-- Luck timeout period in turns (timeout.c:595-620): 300 with the Amulet or an angry god. -/
def luckDecayPeriod (hasAmulet godAngry : Bool) : Nat :=
  if hasAmulet || godAngry then 300 else 600

end NetMechanics
