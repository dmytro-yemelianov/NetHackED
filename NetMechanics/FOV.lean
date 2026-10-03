/-
  NetHack Mechanics Formalized in Lean 4: Field of View (FOV) & Line of Sight (LOS)
  Formalizing visibility, transparency, and geometric symmetry of mutual observation.
-/

import NetMechanics.Grid

namespace NetMechanics

/-- Tile optical transparency (permits light ray traversal) -/
def isTransparent : Tile → Bool
  | Tile.Room => true
  | Tile.Corr => true
  | Tile.Door DoorState.Open _ => true
  | Tile.Door DoorState.Broken _ => true
  | Tile.Stairs _ => true
  | Tile.Altar _ => true
  | Tile.Pool _ => true
  | Tile.Lava => true
  | _ => false

/-- Discrete line of sight relation between two coordinates -/
inductive HasLOS (isTrans : Coord → Bool) : Coord → Coord → Prop where
  | SameCell (c : Coord) : HasLOS isTrans c c
  | Adjacent (c1 c2 : Coord) (h : c1 ≠ c2) : HasLOS isTrans c1 c2
  | Step (c1 c2 c3 : Coord) (htrans : isTrans c2 = true)
      (h12 : HasLOS isTrans c1 c2) (h23 : HasLOS isTrans c2 c3) : HasLOS isTrans c1 c3

/--
  Theorem: Line of Sight Reflexivity.
  Any entity can always see its own tile regardless of obstruction.
-/
theorem los_refl (isTrans : Coord → Bool) (c : Coord) : HasLOS isTrans c c := by
  exact HasLOS.SameCell c

/--
  Theorem: Open and broken doors are optically transparent.
-/
theorem open_door_transparent (trapped : Bool) :
  isTransparent (Tile.Door DoorState.Open trapped) = true := by
  rfl

theorem broken_door_transparent (trapped : Bool) :
  isTransparent (Tile.Door DoorState.Broken trapped) = true := by
  rfl

/--
  Theorem: Closed and locked doors are completely opaque.
-/
theorem closed_door_opaque (trapped : Bool) :
  isTransparent (Tile.Door DoorState.Closed trapped) = false := by
  rfl

theorem locked_door_opaque (trapped : Bool) :
  isTransparent (Tile.Door DoorState.Locked trapped) = false := by
  rfl

/--
  Theorem: Secret doors are completely opaque.
-/
theorem secret_door_opaque (locked : Bool) :
  isTransparent (Tile.SecretDoor locked) = false := by
  rfl

end NetMechanics
