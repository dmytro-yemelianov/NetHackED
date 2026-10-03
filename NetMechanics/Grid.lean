/-
  NetHack Mechanics Formalized in Lean 4: Dungeon Grid & Tile State Invariants
  Eliminating the struct rm 5-bit flags overloading bug from C NetHack (rm.h).
-/

namespace NetMechanics

/-- Standard NetHack dungeon dimensions -/
def COLNO : Nat := 80
def ROWNO : Nat := 21

/-- Coordinate bounded by level dimensions -/
structure Coord where
  x : Fin COLNO
  y : Fin ROWNO
deriving Repr, DecidableEq

/-- State of a door -/
inductive DoorState where
  | Open
  | Closed
  | Locked
  | Broken
deriving Repr, DecidableEq

/-- Alignment for altars and characters -/
inductive Alignment where
  | Lawful
  | Neutral
  | Chaotic
  | Unaligned
deriving Repr, DecidableEq

/--
  In C NetHack (rm.h):
    struct rm {
      Bitfield(flags, 5); // Overloaded across 11 tile types!
    }
  In Lean 4, we use a disjoint algebraic data type where each variant
  carries only its exact relevant fields with zero bitfield collisions.
-/
inductive Tile where
  | Stone
  | Wall (horizontal : Bool)
  | Corr
  | Room
  | Door (state : DoorState) (trapped : Bool)
  | SecretDoor (locked : Bool)
  | Stairs (up : Bool)
  | Altar (align : Alignment)
  | Pool (frozen : Bool)
  | Lava
deriving Repr, DecidableEq

/-- Base walking accessibility without special intrinsic powers -/
def isPassable : Tile → Bool
  | Tile.Corr => true
  | Tile.Room => true
  | Tile.Door DoorState.Open _ => true
  | Tile.Door DoorState.Broken _ => true
  | Tile.Stairs _ => true
  | Tile.Altar _ => true
  | Tile.Pool true => true -- Ice is passable
  | _ => false

/-- Open a door -/
def openDoor : Tile → Tile
  | Tile.Door DoorState.Closed trapped => Tile.Door DoorState.Open trapped
  | other => other

/-- Close a door -/
def closeDoor : Tile → Tile
  | Tile.Door DoorState.Open trapped => Tile.Door DoorState.Closed trapped
  | other => other

/-- Unlock a door -/
def unlockDoor : Tile → Tile
  | Tile.Door DoorState.Locked trapped => Tile.Door DoorState.Closed trapped
  | other => other

/-- Break a door (e.g. kicking or force) -/
def breakDoor : Tile → Tile
  | Tile.Door _ _ => Tile.Door DoorState.Broken false
  | other => other

/-- Reveal a secret door (detect.c cvt_sdoor_to_door) -/
def revealSecretDoor : Tile → Tile
  | Tile.SecretDoor locked =>
    Tile.Door (if locked then DoorState.Locked else DoorState.Closed) false
  | other => other

/--
  Theorem: Open and broken doors are passable.
-/
theorem open_door_is_passable (trapped : Bool) :
  isPassable (Tile.Door DoorState.Open trapped) = true := by
  rfl

theorem broken_door_is_passable (trapped : Bool) :
  isPassable (Tile.Door DoorState.Broken trapped) = true := by
  rfl

/--
  Theorem: Closed and locked doors are impassable.
-/
theorem closed_door_impassable (trapped : Bool) :
  isPassable (Tile.Door DoorState.Closed trapped) = false := by
  rfl

theorem locked_door_impassable (trapped : Bool) :
  isPassable (Tile.Door DoorState.Locked trapped) = false := by
  rfl

/--
  Theorem: Secret doors are always impassable until discovered.
-/
theorem secret_door_impassable (locked : Bool) :
  isPassable (Tile.SecretDoor locked) = false := by
  rfl

/--
  Theorem: Breaking a door is idempotent.
-/
theorem break_door_idempotent (t : Tile) :
  breakDoor (breakDoor t) = breakDoor t := by
  cases t <;> rfl

/--
  Theorem: Secret door reveal preserves lock status faithfully.
-/
theorem reveal_secret_door_locked :
  revealSecretDoor (Tile.SecretDoor true) = Tile.Door DoorState.Locked false := by
  rfl

theorem reveal_secret_door_unlocked :
  revealSecretDoor (Tile.SecretDoor false) = Tile.Door DoorState.Closed false := by
  rfl

end NetMechanics
