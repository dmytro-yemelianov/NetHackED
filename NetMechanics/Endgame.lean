/-
  NetMechanics.Endgame: Formalized Castle Drawbridge and Astral Plane Ascension.
  Models the Castle drawbridge mechanics (raising, lowering, crushing),
  the five elemental and astral planes, and the High Altar ascension theorem.
-/

import NetMechanics.Grid

namespace NetMechanics

/-- State of the Castle drawbridge -/
inductive DrawbridgeState where
  | Open       -- Lowered over moat, walkable
  | Closed     -- Raised portcullis, impassable wall
  | Destroyed  -- Collapsed into moat
deriving Repr, DecidableEq

/-- Accessibility of the drawbridge tile based on state -/
def isDrawbridgePassable : DrawbridgeState → Bool
  | DrawbridgeState.Open => true
  | DrawbridgeState.Closed => false
  | DrawbridgeState.Destroyed => false

/-- Result of operating a drawbridge -/
inductive DrawbridgeTransition where
  | Lowered
  | Raised (crushedDamage : Nat)
  | DestroyedAndFellInMoat
deriving Repr, DecidableEq

/-- Operating a drawbridge with an occupant -/
def toggleDrawbridge (current : DrawbridgeState) (hasOccupant : Bool) : DrawbridgeState × DrawbridgeTransition :=
  match current with
  | DrawbridgeState.Closed =>
    (DrawbridgeState.Open, DrawbridgeTransition.Lowered)
  | DrawbridgeState.Open =>
    let damage := if hasOccupant then 9999 else 0
    (DrawbridgeState.Closed, DrawbridgeTransition.Raised damage)
  | DrawbridgeState.Destroyed =>
    (DrawbridgeState.Destroyed, DrawbridgeTransition.DestroyedAndFellInMoat)

/-- Theorem: Lowered drawbridge is always passable -/
theorem open_drawbridge_is_passable :
  isDrawbridgePassable DrawbridgeState.Open = true := by
  rfl

/-- Theorem: Raised drawbridge is impassable -/
theorem closed_drawbridge_is_impassable :
  isDrawbridgePassable DrawbridgeState.Closed = false := by
  rfl

/-- Theorem: Raising a drawbridge with an occupant delivers fatal crushing damage -/
theorem raise_crushes_occupant_fatal (hasOccupant : Bool) (h : hasOccupant = true) :
  (toggleDrawbridge DrawbridgeState.Open hasOccupant).2 = DrawbridgeTransition.Raised 9999 := by
  dsimp [toggleDrawbridge]
  rw [h]
  rfl

/-- The Planes leading to the Astral Plane -/
inductive EndgamePlane where
  | Earth
  | Air
  | Fire
  | Water
  | Astral
deriving Repr, DecidableEq

/-- Progression through the endgame planes -/
def nextPlane : EndgamePlane → Option EndgamePlane
  | EndgamePlane.Earth => some EndgamePlane.Air
  | EndgamePlane.Air => some EndgamePlane.Fire
  | EndgamePlane.Fire => some EndgamePlane.Water
  | EndgamePlane.Water => some EndgamePlane.Astral
  | EndgamePlane.Astral => none

/-- Outcome of offering the Amulet of Yendor on a High Altar -/
inductive AscensionOutcome where
  | Ascended (godAlign : Alignment)
  | Rejected (reason : String)
deriving Repr, DecidableEq

/-- Offering the Amulet on a High Altar -/
def offerAmuletOnHighAltar (hasRealAmulet : Bool) (heroAlign : Alignment) (altarAlign : Alignment) : AscensionOutcome :=
  if ¬hasRealAmulet then
    AscensionOutcome.Rejected "An imitation Amulet cannot grant immortality!"
  else if heroAlign = altarAlign then
    AscensionOutcome.Ascended altarAlign
  else
    AscensionOutcome.Rejected "The deity furiously rejects your cross-aligned offering!"

/-- Theorem: Ascension is impossible without the genuine Amulet of Yendor -/
theorem ascension_requires_real_amulet (heroAlign altarAlign : Alignment) (godAlign : Alignment) :
  offerAmuletOnHighAltar false heroAlign altarAlign = AscensionOutcome.Ascended godAlign → False := by
  intro h
  dsimp [offerAmuletOnHighAltar] at h
  contradiction

/-- Theorem: Ascension succeeds if and only if the altar alignment matches the hero's alignment -/
theorem ascension_iff_aligned (heroAlign altarAlign : Alignment) :
  (offerAmuletOnHighAltar true heroAlign altarAlign = AscensionOutcome.Ascended altarAlign) ↔
  (heroAlign = altarAlign) := by
  dsimp [offerAmuletOnHighAltar]
  by_cases h : heroAlign = altarAlign
  · rw [if_pos h]
    constructor
    · intro _
      exact h
    · intro _
      rfl
  · rw [if_neg h]
    constructor
    · intro hcontra
      contradiction
    · intro heq
      contradiction

/-- Theorem: Cross-aligned offering is strictly rejected -/
theorem cross_aligned_offering_rejected (heroAlign altarAlign : Alignment) (h : heroAlign ≠ altarAlign) :
  ∃ reason, offerAmuletOnHighAltar true heroAlign altarAlign = AscensionOutcome.Rejected reason := by
  dsimp [offerAmuletOnHighAltar]
  rw [if_neg h]
  refine ⟨"The deity furiously rejects your cross-aligned offering!", rfl⟩

end NetMechanics
