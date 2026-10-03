/-
  NetHack Mechanics Formalized in Lean 4: Bones Files & Graveyard Persistence
  Formalizing item corruption upon hero death, ghost reincarnation bounds, and bones validity.
-/

import NetMechanics.BUC

namespace NetMechanics

/--
  When an adventurer dies, their equipment corrupts into cursed items
  when saved to a bones graveyard file (bones.c:52).
-/
def corruptBucOnDeath (_b : BUC) : BUC :=
  BUC.Cursed

/--
  Theorem: Item corruption upon death is idempotent.
-/
theorem corrupt_buc_idempotent (b : BUC) :
    corruptBucOnDeath (corruptBucOnDeath b) = corruptBucOnDeath b := by
  rfl

/--
  Theorem: Every corrupted item is strictly cursed.
-/
theorem corrupt_buc_always_cursed (b : BUC) :
    corruptBucOnDeath b = BUC.Cursed := by
  rfl

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
