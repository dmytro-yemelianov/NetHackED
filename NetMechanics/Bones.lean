/-
  NetHack Mechanics Formalized in Lean 4: Bones Files & Graveyard Persistence
  Formalizing item corruption upon hero death, ghost reincarnation bounds, and bones validity.
-/

import NetMechanics.BUC

namespace NetMechanics

/--
  BUC after death (bones.c:290-291, :173-189): `if (rn2(5)) curse(otmp)`; the Amulet and
  invocation items are always cursed (`alwaysCursed`); quest artifacts are not. `r` is the `rn2(5)` draw.
-/
def corruptBucOnDeath (b : BUC) (alwaysCursed : Bool) (r : Nat) : BUC :=
  if alwaysCursed || r ≠ 0 then BUC.Cursed else b

/-- Theorem: Corruption is idempotent for a fixed roll. -/
theorem corrupt_buc_idempotent (b : BUC) (q : Bool) (r : Nat) :
    corruptBucOnDeath (corruptBucOnDeath b q r) q r = corruptBucOnDeath b q r := by
  unfold corruptBucOnDeath
  split <;> simp_all

/-- Theorem: A nonzero roll curses the item (renamed from `corrupt_buc_always_cursed`). -/
theorem corrupt_buc_cursed_when_roll_nonzero (b : BUC) (q : Bool) (r : Nat) (h : r ≠ 0) :
    corruptBucOnDeath b q r = BUC.Cursed := by
  simp [corruptBucOnDeath, h]

/-- Theorem: A zero roll on a item not flagged always-cursed keeps the original BUC. -/
theorem corrupt_buc_keeps_original_on_zero_roll (b : BUC) :
    corruptBucOnDeath b false 0 = b := by
  simp [corruptBucOnDeath]

/-- Theorem: Items flagged always-cursed are always cursed. -/
theorem corrupt_buc_flagged_always_cursed (b : BUC) (r : Nat) :
    corruptBucOnDeath b true r = BUC.Cursed := by
  simp [corruptBucOnDeath]

/--
  Bones file record specification.
-/
structure BonesRecord where
  depth : Nat
  formerMaxHp : Nat
  ghostHp : Nat
  ghostAc : Int
deriving Repr, DecidableEq

/--
  NetHack ghost creation invariant:
  Ghost HP is initialized to the former adventurer's maximum HP clamped to at least 1.
-/
def createGhostHp (formerMaxHp : Nat) : Nat :=
  max 1 formerMaxHp

/--
  Theorem: Ghost HP never exceeds former adventurer's maximum HP (if formerMaxHp ≥ 1).
-/
theorem ghost_hp_bounded (formerMaxHp : Nat) (h : formerMaxHp ≥ 1) :
    createGhostHp formerMaxHp = formerMaxHp := by
  unfold createGhostHp
  exact Nat.max_eq_right h

/--
  Theorem: Ghost is never spawned dead (HP ≥ 1).
-/
theorem ghost_hp_strictly_positive (formerMaxHp : Nat) :
    createGhostHp formerMaxHp ≥ 1 := by
  unfold createGhostHp
  exact Nat.le_max_left 1 formerMaxHp

/--
  Bones generation validity:
  Bones can only occur on valid dungeon levels (depth ≥ 1).
-/
def isValidBonesLevel (depth : Nat) : Bool :=
  depth ≥ 1

theorem valid_bones_level_ge_one (depth : Nat) (h : isValidBonesLevel depth = true) :
    depth ≥ 1 := by
  unfold isValidBonesLevel at h
  exact of_decide_eq_true h

end NetMechanics
