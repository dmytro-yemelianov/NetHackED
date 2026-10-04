/-
  NetHack Mechanics Formalized in Lean 4: Peacefulness & peace_minded
  Formalizing monster peacefulness determination from NetHack 5.0 (makemon.c:2268-2308).
-/

namespace NetMechanics

/--
  Signum of an integer alignment.
  Lawful > 0, Neutral = 0, Chaotic < 0.
-/
def alignSign (a : Int) : Int :=
  if a > 0 then 1 else if a < 0 then -1 else 0

/--
  NetHack 5.0 C `peace_minded(ptr)` (`makemon.c:2268-2308`):
  1. `alwaysPeaceful` -> true
  2. `alwaysHostile` -> false
  3. `sgn(mal) != sgn(ual)` -> false (cross-aligned is hostile)
  4. Co-aligned: tested via `coalignedRoll < peacefulOutcomes`
     where `a = 16 + max (-15) record`, `b = 2 + |mal|`,
     and `peacefulOutcomes = (a - 1) * (b - 1)`.
-/
def peaceMinded (alwaysPeaceful alwaysHostile : Bool) (monAlign heroAlign : Int)
    (heroRecord : Int) (coalignedRoll : Nat) : Bool :=
  if alwaysPeaceful then
    true
  else if alwaysHostile then
    false
  else if alignSign monAlign ≠ alignSign heroAlign then
    false
  else
    let a := (16 + max (-15) heroRecord).toNat
    let b := (2 + monAlign.natAbs)
    let peacefulOutcomes := (a - 1) * (b - 1)
    coalignedRoll < peacefulOutcomes

/-- Always-peaceful archetypes (shopkeepers, priests, watchmen, quest leaders) always spawn peaceful. -/
theorem peace_minded_always_peaceful (h : Bool) (m u r : Int) (roll : Nat) :
    peaceMinded true h m u r roll = true := rfl

/-- Always-hostile archetypes (quest nemesis) never spawn peaceful unless marked always-peaceful. -/
theorem peace_minded_always_hostile (m u r : Int) (roll : Nat) :
    peaceMinded false true m u r roll = false := rfl

/-- Cross-aligned monsters never spawn peaceful. -/
theorem peace_minded_cross_aligned_hostile (m u r : Int) (roll : Nat)
    (h : alignSign m ≠ alignSign u) :
    peaceMinded false false m u r roll = false := by
  unfold peaceMinded
  simp [h]

/-- Co-aligned monster peacefulness is strictly determined by roll threshold. -/
theorem peace_minded_coaligned_threshold (m u r : Int) (roll : Nat)
    (h : alignSign m = alignSign u) :
    let a := (16 + max (-15) r).toNat
    let b := (2 + m.natAbs)
    let peacefulOutcomes := (a - 1) * (b - 1)
    peaceMinded false false m u r roll = (roll < peacefulOutcomes) := by
  unfold peaceMinded
  simp [h]

end NetMechanics
