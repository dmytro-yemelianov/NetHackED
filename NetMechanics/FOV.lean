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

/-- Chebyshev (king-move) distance between two coordinates. -/
def coordChebyshev (a b : Coord) : Nat :=
  max (Int.natAbs ((a.x.val : Int) - (b.x.val : Int)))
      (Int.natAbs ((a.y.val : Int) - (b.y.val : Int)))

/-- Two cells are adjacent iff their Chebyshev distance is exactly 1
    (one of the 8 king-move neighbours). -/
def isAdjacent (c1 c2 : Coord) : Bool := coordChebyshev c1 c2 == 1

/--
  Discrete line of sight relation between two coordinates.
  This is a "connected through transparent cells" relation: a cell sees itself,
  its 8 neighbours, and anything reachable by chaining such links through
  transparent intermediate cells. It is not NetHack's straight-line
  `clear_path` (vision.c); it is only a sound over-approximation shape.
-/
inductive HasLOS (isTrans : Coord → Bool) : Coord → Coord → Prop where
  | SameCell (c : Coord) : HasLOS isTrans c c
  | Adjacent (c1 c2 : Coord) (h : isAdjacent c1 c2 = true) : HasLOS isTrans c1 c2
  | Step (c1 c2 c3 : Coord) (htrans : isTrans c2 = true)
      (h12 : HasLOS isTrans c1 c2) (h23 : HasLOS isTrans c2 c3) : HasLOS isTrans c1 c3

/--
  Theorem: Line of Sight Reflexivity.
  Any entity can always see its own tile regardless of obstruction.
-/
theorem los_refl (isTrans : Coord → Bool) (c : Coord) : HasLOS isTrans c c := by
  exact HasLOS.SameCell c

theorem coordChebyshev_comm (a b : Coord) : coordChebyshev a b = coordChebyshev b a := by
  unfold coordChebyshev
  rw [← Int.neg_sub (a.x.val : Int), Int.natAbs_neg,
      ← Int.neg_sub (a.y.val : Int), Int.natAbs_neg]

theorem isAdjacent_symm (a b : Coord) : isAdjacent a b = isAdjacent b a := by
  simp [isAdjacent, coordChebyshev_comm a b]

/-- A cell is never adjacent to itself (distance 0, not 1). -/
theorem not_isAdjacent_self (c : Coord) : isAdjacent c c = false := by
  simp [isAdjacent, coordChebyshev]

/--
  Theorem: Line of Sight Symmetry.
  If `a` sees `b` then `b` sees `a`: adjacency is symmetric and a `Step`
  chain reverses through the same transparent middle cells.
-/
theorem hasLOS_symm (isTrans : Coord → Bool) {a b : Coord}
    (h : HasLOS isTrans a b) : HasLOS isTrans b a := by
  induction h with
  | SameCell c => exact HasLOS.SameCell c
  | Adjacent c1 c2 hadj =>
    exact HasLOS.Adjacent c2 c1 (by rw [isAdjacent_symm]; exact hadj)
  | Step c1 c2 c3 htrans _ _ ih12 ih23 =>
    exact HasLOS.Step c3 c2 c1 htrans ih23 ih12

/--
  Theorem: Non-vacuity. With every cell opaque, a cell sees only itself and
  its 8 neighbours (the relation is no longer total).
-/
theorem hasLOS_opaque_local (isTrans : Coord → Bool)
    (hopaque : ∀ c, isTrans c = false) {a b : Coord}
    (h : HasLOS isTrans a b) : a = b ∨ isAdjacent a b = true := by
  induction h with
  | SameCell c => exact Or.inl rfl
  | Adjacent c1 c2 hadj => exact Or.inr hadj
  | Step c1 c2 c3 htrans _ _ _ _ =>
    rw [hopaque c2] at htrans
    exact absurd htrans (by decide)

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
