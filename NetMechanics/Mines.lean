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

/-- Hard cap on priest-bought divine protection `u.ublessed` (priest.c:696). -/
def maxDivineProtection : Nat := 20

/-- Below this value every purchase is a guaranteed +1 (priest.c:697). -/
def protectionSoftCap : Nat := 9

/-- Suggested donation (priest.c:637-638): `max(ulevelpeak,1) * rn1(101, 150 + 40*cheapskate)`;
    `rn2_101` is clamped to `0..100`. -/
def protectionDonationCost (levelPeak cheapskate rn2_101 : Nat) : Nat :=
  max levelPeak 1 * (150 + cheapskate * 40 + min rn2_101 100)

/-- One protection-loop iteration (priest.c:694-698). `roll` is `rn2(3)` when `cur = 0`
    (first purchase 2..4), else `rn2(cur)` (only 0 succeeds from 9 to 19). -/
def protectionStep (cur roll : Nat) : Nat :=
  if cur = 0 then 2 + min roll 2
  else if cur < maxDivineProtection ∧ (cur < protectionSoftCap ∨ roll = 0) then cur + 1
  else cur

/-- Protection after a donation of `offer` (priest.c:681-699): only the band
    `2*suggested*quan ≤ offer < 3*suggested*quan` buys protection, with
    `offer / (2*suggested)` loop iterations drawing from `rolls`
    (missing rolls default to 0). -/
def applyPriestDonation (cur offer suggested quan : Nat) (rolls : List Nat) : Nat :=
  if 2 * suggested * quan ≤ offer ∧ offer < 3 * suggested * quan then
    (List.range (offer / (2 * suggested))).foldl
      (fun p i => protectionStep p (rolls.getD i 0)) cur
  else cur

theorem protectionStep_le (cur roll : Nat) (h : cur ≤ maxDivineProtection) :
    protectionStep cur roll ≤ maxDivineProtection := by
  unfold protectionStep maxDivineProtection protectionSoftCap at *
  repeat' (first | split | omega)

theorem protectionStep_ge (cur roll : Nat) : cur ≤ protectionStep cur roll := by
  unfold protectionStep
  repeat' (first | split | omega)

/-- THEOREM: First purchase grants 2..4 points (priest.c:695). -/
theorem priest_protection_first_gain (roll : Nat) :
    2 ≤ protectionStep 0 roll ∧ protectionStep 0 roll ≤ 4 := by
  unfold protectionStep
  simp only [↓reduceIte]
  omega

/-- THEOREM: Below the soft cap each purchase is a guaranteed +1 (priest.c:697). -/
theorem priest_protection_soft_cap_step (cur roll : Nat) (h0 : 0 < cur)
    (h : cur < protectionSoftCap) : protectionStep cur roll = cur + 1 := by
  unfold protectionStep maxDivineProtection protectionSoftCap at *
  repeat' (first | split | omega)

theorem foldl_step_le (xs : List Nat) (rolls : List Nat) (cur : Nat)
    (h : cur ≤ maxDivineProtection) :
    xs.foldl (fun p i => protectionStep p (rolls.getD i 0)) cur ≤ maxDivineProtection := by
  induction xs generalizing cur with
  | nil => exact h
  | cons x xs ih => exact ih _ (protectionStep_le _ _ h)

theorem foldl_step_ge (xs : List Nat) (rolls : List Nat) (cur : Nat) :
    cur ≤ xs.foldl (fun p i => protectionStep p (rolls.getD i 0)) cur := by
  induction xs generalizing cur with
  | nil => exact Nat.le_refl _
  | cons x xs ih => exact Nat.le_trans (protectionStep_ge _ _) (ih _)

/-- THEOREM: Divine protection never exceeds the hard cap 20 when starting within bounds,
    for every offer and every roll sequence. -/
theorem priest_protection_bounded (cur offer suggested quan : Nat) (rolls : List Nat)
    (h_cur : cur ≤ maxDivineProtection) :
    applyPriestDonation cur offer suggested quan rolls ≤ maxDivineProtection := by
  unfold applyPriestDonation
  split
  · exact foldl_step_le _ _ _ h_cur
  · exact h_cur

/-- THEOREM: Divine protection is monotonic (never decreases from a donation) -/
theorem priest_protection_monotonic (cur offer suggested quan : Nat) (rolls : List Nat) :
    cur ≤ applyPriestDonation cur offer suggested quan rolls := by
  unfold applyPriestDonation
  split
  · exact foldl_step_ge _ _ _
  · exact Nat.le_refl _

/-- THEOREM: An offer below the protection band (`< 2*suggested*quan`) leaves protection
    unchanged (priest.c:654-680). -/
theorem priest_protection_insufficient (cur offer suggested quan : Nat) (rolls : List Nat)
    (h_insuf : offer < 2 * suggested * quan) :
    applyPriestDonation cur offer suggested quan rolls = cur := by
  unfold applyPriestDonation
  split
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
