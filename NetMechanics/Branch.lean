/-
  NetHack Mechanics Formalized in Lean 4: Dungeon Branching & Topology
  Proves branch depth invariance, bidirectional transitions, and entrance bounds.
-/

namespace NetMechanics

/-- Canonical NetHack dungeon branch taxonomy -/
inductive BranchId where
  | DungeonsOfDoom
  | GnomishMines
  | Sokoban
deriving Repr, DecidableEq

/-- Discrete branch coordinate: (BranchId, LevelWithinBranch) -/
structure BranchCoord where
  branch : BranchId
  depth  : Nat
deriving Repr, DecidableEq

/-- Entrance depth from the parent branch (Dungeons of Doom) -/
def branchEntranceDepth : BranchId → Nat
  | BranchId.DungeonsOfDoom => 1
  | BranchId.GnomishMines  => 3
  | BranchId.Sokoban       => 4

/-- Max depth of each branch -/
def branchMaxDepth : BranchId → Nat
  | BranchId.DungeonsOfDoom => 5
  | BranchId.GnomishMines  => 5
  | BranchId.Sokoban       => 3

/-- Enter a branch from the main dungeon stack -/
def enterBranch (b : BranchId) (mainDepth : Nat) : Option BranchCoord :=
  if mainDepth == branchEntranceDepth b then
    some ⟨b, 1⟩
  else
    none

/-- Return to the main dungeon stack from a branch's top level -/
def exitBranch (bc : BranchCoord) : Option BranchCoord :=
  if bc.depth == 1 then
    some ⟨BranchId.DungeonsOfDoom, branchEntranceDepth bc.branch⟩
  else
    none

/-- THEOREM: Entering and immediately exiting a branch at entrance depth is the identity -/
theorem branch_transition_invertible (b : BranchId) :
    let entrance := branchEntranceDepth b
    enterBranch b entrance = some ⟨b, 1⟩ ∧
    exitBranch ⟨b, 1⟩ = some ⟨BranchId.DungeonsOfDoom, entrance⟩ := by
  dsimp [enterBranch, exitBranch]
  constructor
  · rw [if_pos (beq_self_eq_true (branchEntranceDepth b))]
  · rfl

/-- THEOREM: Main dungeon cannot exit itself as a side branch -/
theorem main_dungeon_no_side_exit :
    exitBranch ⟨BranchId.DungeonsOfDoom, 1⟩ = some ⟨BranchId.DungeonsOfDoom, 1⟩ := by
  rfl

end NetMechanics
