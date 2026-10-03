import NetMechanics.BUC
import NetMechanics.Basic

namespace NetMechanics

inductive GenocideScope where
  | species (name : String)
  | cls (glyph : Char)

structure GenocideMonster where
  name : String
  glyph : Char

structure ConductTracker where
  genocideless : Bool

structure GenocideState where
  genocidedSpecies : List String
  genocidedClasses : List Char
  conduct : ConductTracker

def can_spawn (state : GenocideState) (m : GenocideMonster) : Bool :=
  if state.genocidedSpecies.contains m.name then false
  else if state.genocidedClasses.contains m.glyph then false
  else true

inductive GenocideScrollEffect where
  | wipe_class_or_species (scope : GenocideScope)
  | wipe_species (name : String)
  | summon (name : String)

def apply_genocide (state : GenocideState) (effect : GenocideScrollEffect) : GenocideState :=
  match effect with
  | GenocideScrollEffect.wipe_class_or_species (GenocideScope.species name) =>
    { state with
      genocidedSpecies := name :: state.genocidedSpecies,
      conduct := { genocideless := false } }
  | GenocideScrollEffect.wipe_class_or_species (GenocideScope.cls glyph) =>
    { state with
      genocidedClasses := glyph :: state.genocidedClasses,
      conduct := { genocideless := false } }
  | GenocideScrollEffect.wipe_species name =>
    { state with
      genocidedSpecies := name :: state.genocidedSpecies,
      conduct := { genocideless := false } }
  | GenocideScrollEffect.summon _ => state

theorem genocided_species_cannot_spawn (state : GenocideState) (m : GenocideMonster)
    (h : state.genocidedSpecies.contains m.name = true) : can_spawn state m = false := by
  dsimp [can_spawn]
  rw [h]
  rfl

theorem genocided_class_cannot_spawn (state : GenocideState) (m : GenocideMonster)
    (h_species : state.genocidedSpecies.contains m.name = false)
    (h_class : state.genocidedClasses.contains m.glyph = true) : can_spawn state m = false := by
  dsimp [can_spawn]
  rw [h_species, h_class]
  rfl

theorem genocide_conduct_monotonic (state : GenocideState) (effect : GenocideScrollEffect)
    (h_not_summon : ∀ name, effect ≠ GenocideScrollEffect.summon name) :
    (apply_genocide state effect).conduct.genocideless = false := by
  cases effect
  · rename_i scope
    cases scope <;> rfl
  · rfl
  · rename_i name
    have contra := h_not_summon name
    contradiction

structure MagicMarker where
  ink : Nat

inductive MarkerResult where
  | success (new_ink : Nat)
  | failure (original_ink : Nat)
  deriving Repr, DecidableEq

def write_with_marker (marker : MagicMarker) (cost : Nat) : MarkerResult :=
  if marker.ink ≥ cost then
    MarkerResult.success (marker.ink - cost)
  else
    MarkerResult.failure marker.ink

theorem marker_ink_depletion_bounds (i c : Nat) (hc : c > 0) (hi : i ≥ c) :
    write_with_marker { ink := i } c = MarkerResult.success (i - c) ∧ (i - c) < i := by
  dsimp [write_with_marker]
  rw [if_pos hi]
  refine ⟨rfl, ?_⟩
  omega

theorem marker_insufficient_ink_fails (i c : Nat) (h : i < c) :
    write_with_marker { ink := i } c = MarkerResult.failure i := by
  dsimp [write_with_marker]
  have hnot : ¬(i ≥ c) := by omega
  rw [if_neg hnot]

end NetMechanics
