/-
  NetHack Mechanics Formalized in Lean 4: Autonomous Agent Tournament & Tactical Heuristics
  Formalizing BUC pet testing priority, Elbereth ward defense, and tournament scoring invariants.
-/

import NetMechanics.Grid

namespace NetMechanics

/-- Tactical actions evaluated by autonomous agents -/
inductive TacticalAction where
  | EngraveElbereth
  | SwapWithPet
  | WaitPetTest
  | AttackHostile
  | Descend
  | Explore
  deriving Repr, DecidableEq

/-- Agent health and spatial situation -/
structure TacticalContext where
  hp : Nat
  maxHp : Nat
  hasHostileAdj : Bool
  hasPetAdj : Bool
  hasUncheckedFloorItem : Bool
  onStairsDown : Bool
  deriving Repr, DecidableEq

/-- Critical health threshold (35% max HP) -/
def isHpCritical (ctx : TacticalContext) : Bool :=
  ctx.hp * 100 < ctx.maxHp * 35

/--
  Tactical Decision Function:
  1. If HP is critical and hostiles are adjacent, engrave Elbereth ward to survive.
  2. If adjacent to pet and need to advance, swap displacement without violence.
  3. If unchecked item on floor and pet is adjacent, wait for pet BUC test.
  4. If hostile adjacent, attack.
  5. If on stairs down, descend.
  6. Otherwise explore.
-/
def decideTacticalAction (ctx : TacticalContext) : TacticalAction :=
  if isHpCritical ctx && ctx.hasHostileAdj then
    TacticalAction.EngraveElbereth
  else if ctx.hasPetAdj && ctx.hasHostileAdj then
    TacticalAction.SwapWithPet
  else if ctx.hasUncheckedFloorItem && ctx.hasPetAdj then
    TacticalAction.WaitPetTest
  else if ctx.hasHostileAdj then
    TacticalAction.AttackHostile
  else if ctx.onStairsDown then
    TacticalAction.Descend
  else
    TacticalAction.Explore

/-- Tournament benchmark scoring formula -/
def calculateTournamentScore (turns : Nat) (depth : Nat) (kills : Nat) (gold : Nat) : Nat :=
  turns * 2 + depth * 100 + kills * 50 + gold

-- =========================================================================
-- THEOREMS: Tactical Decision & Scoring Invariants
-- =========================================================================

/-- Theorem: Critical health with adjacent hostiles strictly triggers Elbereth engraving. -/
theorem critical_hp_elbereth_priority (ctx : TacticalContext)
    (hcrit : isHpCritical ctx = true) (hadj : ctx.hasHostileAdj = true) :
    decideTacticalAction ctx = TacticalAction.EngraveElbereth := by
  unfold decideTacticalAction
  simp [hcrit, hadj]

/-- Theorem: Unchecked floor item with pet present triggers pet-testing wait. -/
theorem pet_test_priority (ctx : TacticalContext)
    (hnot_crit : isHpCritical ctx = false ∨ ctx.hasHostileAdj = false)
    (hno_hostile : ctx.hasHostileAdj = false)
    (hitem : ctx.hasUncheckedFloorItem = true)
    (hpet : ctx.hasPetAdj = true) :
    decideTacticalAction ctx = TacticalAction.WaitPetTest := by
  unfold decideTacticalAction
  cases hnot_crit with
  | inl hnc =>
    simp [hnc, hno_hostile, hitem, hpet]
  | inr hnh =>
    simp [hnh, hitem, hpet]

/-- Theorem: Tournament score is monotonic with respect to depth advancement. -/
theorem tournament_score_depth_monotonic (turns d1 d2 kills gold : Nat) (h : d1 ≤ d2) :
    calculateTournamentScore turns d1 kills gold ≤ calculateTournamentScore turns d2 kills gold := by
  unfold calculateTournamentScore
  have hd : d1 * 100 ≤ d2 * 100 := Nat.mul_le_mul_right 100 h
  omega

/-- Theorem: Tournament score is monotonic with respect to monster kills. -/
theorem tournament_score_kills_monotonic (turns depth k1 k2 gold : Nat) (h : k1 ≤ k2) :
    calculateTournamentScore turns depth k1 gold ≤ calculateTournamentScore turns depth k2 gold := by
  unfold calculateTournamentScore
  have hk : k1 * 50 ≤ k2 * 50 := Nat.mul_le_mul_right 50 h
  omega

end NetMechanics
