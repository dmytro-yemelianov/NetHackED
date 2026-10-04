/-
  NetHack Mechanics Formalized in Lean 4: Peacefulness & peace_minded
  Formalizing monster peacefulness determination from NetHack 5.0 C
  `peace_minded` (makemon.c:2268-2308).
-/

namespace NetMechanics

/--
  Signum of an integer alignment (C `sgn`).
  Lawful > 0, Neutral = 0, Chaotic < 0.
-/
def alignSign (a : Int) : Int :=
  if a > 0 then 1 else if a < 0 then -1 else 0

/-- Inputs of C `peace_minded` for one species and the current hero. -/
structure PeaceInput where
  /-- `M2_PEACEFUL` (makemon.c:2272) -/
  alwaysPeaceful : Bool
  /-- `M2_HOSTILE` (makemon.c:2274) -/
  alwaysHostile : Bool
  /-- `msound == MS_LEADER || msound == MS_GUARDIAN` (makemon.c:2276) -/
  leaderOrGuardian : Bool
  /-- `msound == MS_NEMESIS` (makemon.c:2278) -/
  nemesis : Bool
  /-- `race_peaceful(ptr)` (makemon.c:2283) -/
  racePeaceful : Bool
  /-- `race_hostile(ptr)` (makemon.c:2285) -/
  raceHostile : Bool
  /-- `ptr->maligntyp` -/
  monAlign : Int
  /-- `u.ualign.type` -/
  heroAlign : Int
  /-- `u.ualign.record` -/
  heroRecord : Int
  /-- `u.uhave.amulet` (makemon.c:2294) -/
  hasAmulet : Bool
  /-- `is_minion(ptr)` (makemon.c:2298) -/
  isMinion : Bool

/-- The decision C `peace_minded` reaches before (or instead of) drawing. -/
inductive PeaceDecision where
  | peaceful
  | hostile
  /-- the co-aligned last case: `!!rn2(a) && !!rn2(b)` (makemon.c:2305-2307) -/
  | coalignedRoll (a b : Nat)
deriving Repr, DecidableEq

/-- The deterministic steps of C `peace_minded`, in C order (Erinys not modelled). -/
def peaceDecision (p : PeaceInput) : PeaceDecision :=
  if p.alwaysPeaceful then .peaceful
  else if p.alwaysHostile then .hostile
  else if p.leaderOrGuardian then .peaceful
  else if p.nemesis then .hostile
  else if p.racePeaceful then .peaceful
  else if p.raceHostile then .hostile
  else if alignSign p.monAlign ≠ alignSign p.heroAlign then .hostile
  else if p.monAlign < 0 ∧ p.hasAmulet = true then .hostile
  else if p.isMinion then (if p.heroRecord ≥ 0 then .peaceful else .hostile)
  else .coalignedRoll (16 + max (-15) p.heroRecord).toNat (2 + p.monAlign.natAbs)

/--
  C `peace_minded(ptr)` with its two draws `r1 = rn2(a)` and `r2 = rn2(b)`.
  `r2` only matters when `r1 ≠ 0` (C's `&&` short-circuit: the second draw is
  never taken after a zero first draw).
-/
def peaceMinded (p : PeaceInput) (r1 r2 : Nat) : Bool :=
  match peaceDecision p with
  | .peaceful => true
  | .hostile => false
  | .coalignedRoll _ _ => r1 != 0 && r2 != 0

/-- `M2_PEACEFUL` species (shopkeepers, priests, watchmen, quest leaders and guardians)
    are always peaceful, whatever the rolls. -/
theorem peace_minded_always_peaceful (p : PeaceInput) (r1 r2 : Nat)
    (h : p.alwaysPeaceful = true) :
    peaceMinded p r1 r2 = true := by
  simp [peaceMinded, peaceDecision, h]

/-- `M2_HOSTILE` species (nemeses, most low monsters) are never peaceful unless
    also `M2_PEACEFUL`. -/
theorem peace_minded_always_hostile (p : PeaceInput) (r1 r2 : Nat)
    (hp : p.alwaysPeaceful = false) (hh : p.alwaysHostile = true) :
    peaceMinded p r1 r2 = false := by
  simp [peaceMinded, peaceDecision, hp, hh]

/-- `MS_NEMESIS` without either flag is hostile. -/
theorem peace_minded_nemesis_hostile (p : PeaceInput) (r1 r2 : Nat)
    (hp : p.alwaysPeaceful = false) (hh : p.alwaysHostile = false)
    (hl : p.leaderOrGuardian = false) (hn : p.nemesis = true) :
    peaceMinded p r1 r2 = false := by
  simp [peaceMinded, peaceDecision, hp, hh, hl, hn]

/-- Cross-aligned monsters (no flag, msound or race rule applies) are hostile. -/
theorem peace_minded_cross_aligned_hostile (p : PeaceInput) (r1 r2 : Nat)
    (hp : p.alwaysPeaceful = false) (hh : p.alwaysHostile = false)
    (hl : p.leaderOrGuardian = false) (hn : p.nemesis = false)
    (hrp : p.racePeaceful = false) (hrh : p.raceHostile = false)
    (h : alignSign p.monAlign ≠ alignSign p.heroAlign) :
    peaceMinded p r1 r2 = false := by
  simp [peaceMinded, peaceDecision, hp, hh, hl, hn, hrp, hrh, h]

/-- A chaotic monster is hostile to a hero carrying the Amulet (makemon.c:2294). -/
theorem peace_minded_amulet_chaotic_hostile (p : PeaceInput) (r1 r2 : Nat)
    (hp : p.alwaysPeaceful = false) (hh : p.alwaysHostile = false)
    (hl : p.leaderOrGuardian = false) (hn : p.nemesis = false)
    (hrp : p.racePeaceful = false) (hrh : p.raceHostile = false)
    (hm : p.monAlign < 0) (ha : p.hasAmulet = true) :
    peaceMinded p r1 r2 = false := by
  unfold peaceMinded peaceDecision
  by_cases hs : alignSign p.monAlign = alignSign p.heroAlign <;> simp [hp, hh, hl, hn, hrp, hrh, hs, hm, ha]

/-- The co-aligned roll's `rn2` arguments are C's `16 + max(record, -15)` and `2 + |mal|`. -/
theorem peace_decision_roll_args (p : PeaceInput) (a b : Nat)
    (h : peaceDecision p = .coalignedRoll a b) :
    a = (16 + max (-15) p.heroRecord).toNat ∧ b = 2 + p.monAlign.natAbs := by
  unfold peaceDecision at h
  split at h
  · cases h
  split at h
  · cases h
  split at h
  · cases h
  split at h
  · cases h
  split at h
  · cases h
  split at h
  · cases h
  split at h
  · cases h
  split at h
  · cases h
  split at h
  · split at h <;> cases h
  · injection h with ha hb
    exact ⟨ha.symm, hb.symm⟩

/-- The co-aligned roll always has valid `rn2` arguments: `a ≥ 1`, `b ≥ 2`. -/
theorem peace_minded_roll_args_valid (p : PeaceInput) (a b : Nat)
    (h : peaceDecision p = .coalignedRoll a b) :
    1 ≤ a ∧ 2 ≤ b := by
  obtain ⟨ha, hb⟩ := peace_decision_roll_args p a b h
  subst ha; subst hb
  constructor <;> omega

/-- Co-aligned: peaceful iff both C draws are non-zero. -/
theorem peace_minded_coaligned_iff (p : PeaceInput) (a b r1 r2 : Nat)
    (h : peaceDecision p = .coalignedRoll a b) :
    peaceMinded p r1 r2 = true ↔ r1 ≠ 0 ∧ r2 ≠ 0 := by
  simp [peaceMinded, h]

/-- A zero first draw is hostile whatever the (undrawn) second value. -/
theorem peace_minded_first_draw_zero_hostile (p : PeaceInput) (a b r2 : Nat)
    (h : peaceDecision p = .coalignedRoll a b) :
    peaceMinded p 0 r2 = false := by
  simp [peaceMinded, h]

/-- With `record ≤ -15` the first draw is `rn2(1) = 0`: co-aligned monsters are
    always hostile. -/
theorem peace_minded_low_record_always_hostile (p : PeaceInput) (a b r1 r2 : Nat)
    (h : peaceDecision p = .coalignedRoll a b) (hr : p.heroRecord ≤ -15)
    (hr1 : r1 < a) :
    peaceMinded p r1 r2 = false := by
  have ha : a = 1 := by
    obtain ⟨ha, _⟩ := peace_decision_roll_args p a b h
    subst ha
    omega
  have : r1 = 0 := by omega
  subst this
  exact peace_minded_first_draw_zero_hostile p a b r2 h

end NetMechanics
