/-
  NetHack Mechanics Formalized in Lean 4: The Mysterious Force
  Model of `do.c:1541-1573` (`goto_level`): the force that pushes a hero who
  ascends Gehennom with the Amulet of Yendor back down.
  Rolls are raw draws reduced modulo the C range, mirroring the Rust
  `mysterious_force` in `crates/nethacked-core/src/gehennom.rs`.
-/

import NetMechanics.Grid

namespace NetMechanics

/-- Result of the Mysterious Force check. -/
inductive MysteriousForceOutcome where
  | NoEffect
  | SameLevelTeleport
  | PushDown (depth : Nat)
  deriving Repr, DecidableEq

/-- `odds = 3 + u.ualign.type` (`do.c:1544`): lawful 4, neutral 3, chaotic 2;
    unaligned (`A_NONE`) is `<= 1`, modelled as 0. -/
def mfOdds : Alignment → Nat
  | .Lawful => 4
  | .Neutral => 3
  | .Chaotic => 2
  | .Unaligned => 0

/-- Maximum push distance per alignment (`rn2(odds)` is at most `odds - 1`). -/
def mfMaxPush : Alignment → Nat
  | .Lawful => 3
  | .Neutral => 2
  | .Chaotic => 1
  | .Unaligned => 0

/-- Mysterious Force (`do.c:1541-1573`). Active only while `dunlev < bottom - 3`;
    triggers iff `rn2(4 + mf) = 0`; push is `rnd(rn2(odds))` clamped to the bottom. -/
def mysteriousForce (depth bottom mf : Nat) (al : Alignment) (t a b : Nat) :
    MysteriousForceOutcome :=
  if depth + 3 ≥ bottom then .NoEffect
  else if t % (mf + 4) ≠ 0 then .NoEffect
  else
    let diff := if mfOdds al ≤ 1 then 0 else a % mfOdds al
    if diff = 0 then .SameLevelTeleport
    else
      let push := min (b % diff + 1) (bottom - depth)
      if push = 0 then .SameLevelTeleport else .PushDown (depth + push)

/-- The force never fires in the bottom four levels of the dungeon. -/
theorem mysterious_force_disabled_bottom (depth bottom mf : Nat) (al : Alignment)
    (t a b : Nat) (h : depth + 3 ≥ bottom) :
    mysteriousForce depth bottom mf al t a b = .NoEffect := by
  simp [mysteriousForce, h]

/-- The drawn `diff` never exceeds the alignment's maximum push. -/
theorem mf_diff_le (al : Alignment) (a : Nat) :
    (if mfOdds al ≤ 1 then 0 else a % mfOdds al) ≤ mfMaxPush al := by
  cases al <;> simp [mfOdds, mfMaxPush] <;> omega

/-- A push is strictly downward, at most 3 levels for lawful / 2 for neutral /
    1 for chaotic, and only happens outside the bottom four levels. -/
theorem mysterious_force_push_bounded (depth bottom mf : Nat) (al : Alignment)
    (t a b d : Nat)
    (h : mysteriousForce depth bottom mf al t a b = .PushDown d) :
    depth < d ∧ d - depth ≤ mfMaxPush al ∧ depth + 3 < bottom := by
  unfold mysteriousForce at h
  by_cases h1 : depth + 3 ≥ bottom
  · simp [h1] at h
  · by_cases h2 : t % (mf + 4) ≠ 0
    · simp [h1, h2] at h
    · have hlt : depth + 3 < bottom := by omega
      have hd := mf_diff_le al a
      generalize (if mfOdds al ≤ 1 then 0 else a % mfOdds al) = diff at h hd
      simp only [h1, h2, if_false] at h
      by_cases h3 : diff = 0
      · simp [h3] at h
      · have hb := Nat.mod_lt b (Nat.pos_of_ne_zero h3)
        simp only [h3, if_false] at h
        by_cases h4 : min (b % diff + 1) (bottom - depth) = 0
        · simp [h4] at h
        · simp only [h4, if_false] at h
          injection h with h
          refine ⟨?_, ?_, hlt⟩ <;> omega
