/-
  NetHack Mechanics Formalized in Lean 4: Pet & Companion Dynamics
  Formalizes companion displacement invariant, distance conservation,
  and non-violent displacement mechanics.
-/

namespace NetMechanics

/-- 2D Grid Coordinate -/
structure PetCoord where
  x : Int
  y : Int
deriving Repr, DecidableEq

/-- Chebyshev distance between two coordinates -/
def chebyshev (c1 c2 : PetCoord) : Nat :=
  let dx := (c1.x - c2.x).natAbs
  let dy := (c1.y - c2.y).natAbs
  max dx dy

/-- Whether two coordinates are adjacent (king's move) -/
def areAdjacent (c1 c2 : PetCoord) : Bool :=
  c1 ≠ c2 ∧ chebyshev c1 c2 == 1

/-- Displacement swap: hero moves to pet's position, pet moves to hero's old position -/
def swapPositions (heroPos petPos : PetCoord) : (PetCoord × PetCoord) :=
  (petPos, heroPos)

/-- THEOREM: Displacement swap is an involution (swapping twice restores original positions) -/
theorem swap_involution (h p : PetCoord) :
    let (h1, p1) := swapPositions h p
    swapPositions h1 p1 = (h, p) := by
  dsimp [swapPositions]

/-- THEOREM: Swapping positions strictly preserves Chebyshev distance between the two entities -/
theorem swap_preserves_distance (h p : PetCoord) :
    let (h1, p1) := swapPositions h p
    chebyshev h1 p1 = chebyshev h p := by
  dsimp [swapPositions, chebyshev]
  have hx : (p.x - h.x).natAbs = (h.x - p.x).natAbs := by
    rw [← Int.natAbs_neg]
    congr 1
    omega
  have hy : (p.y - h.y).natAbs = (h.y - p.y).natAbs := by
    rw [← Int.natAbs_neg]
    congr 1
    omega
  rw [hx, hy]

/-- Pet tameness and loyalty state -/
structure PetState where
  tameness : Nat
  isTame   : Bool
deriving Repr, DecidableEq

/-- Feeding treats increases or reinforces tameness -/
def feedPet (state : PetState) (nutrition : Nat) : PetState :=
  if state.isTame then
    ⟨state.tameness + nutrition / 10 + 1, true⟩
  else
    if nutrition ≥ 100 then
      ⟨5, true⟩
    else
      state

/-- THEOREM: Feeding an already tame pet strictly increases its tameness -/
theorem feed_increases_tameness (state : PetState) (nut : Nat) (ht : state.isTame = true) :
    (feedPet state nut).tameness > state.tameness := by
  unfold feedPet
  rw [if_pos ht]
  dsimp
  omega

/-- THEOREM: Feeding an already tame pet preserves its tame status -/
theorem feed_preserves_tame (state : PetState) (nut : Nat) (ht : state.isTame = true) :
    (feedPet state nut).isTame = true := by
  unfold feedPet
  rw [if_pos ht]

/-- Interaction outcome: Hostile attack vs Tame displacement -/
inductive HeroInteraction where
  | MeleeAttack (targetId : Nat)
  | DisplacePet (petId : Nat) (newHeroPos : PetCoord) (newPetPos : PetCoord)
deriving Repr, DecidableEq

/-- Determine whether stepping into an occupied tile triggers combat or displacement -/
def interactWithOccupant (heroPos : PetCoord) (occupantPos : PetCoord) (occupantId : Nat) (isTame : Bool) : HeroInteraction :=
  if isTame then
    let (newH, newP) := swapPositions heroPos occupantPos
    HeroInteraction.DisplacePet occupantId newH newP
  else
    HeroInteraction.MeleeAttack occupantId

/-- THEOREM: Stepping onto a tame creature strictly displaces and never triggers a melee attack -/
theorem displacement_non_violent (h p : PetCoord) (id : Nat) :
    interactWithOccupant h p id true = HeroInteraction.DisplacePet id p h := by
  dsimp [interactWithOccupant, swapPositions]

end NetMechanics
