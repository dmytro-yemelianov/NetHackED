/-
  NetHack Mechanics Formalized in Lean 4: Action & Effect AST
  Deep embedding of player/monster actions, atomic state effects,
  operational semantics, and invariant preservation.
-/

import NetMechanics.BUC
import NetMechanics.Inventory
import NetMechanics.Energy
import NetMechanics.Grid
import NetMechanics.Combat

namespace NetMechanics

/-- 8-way directional movement plus vertical navigation -/
inductive Direction where
  | North
  | East
  | South
  | West
  | NorthEast
  | SouthEast
  | SouthWest
  | NorthWest
  | Up
  | Down
deriving Repr, DecidableEq

/--
  Action AST: Syntactic representation of player and monster intentions.
  Deep embedding allows formal reasoning over all possible game actions.
-/
inductive ActionAST where
  | Move (dir : Direction)
  | MeleeAttack (targetCoord : Coord)
  | OpenDoor (targetCoord : Coord)
  | CloseDoor (targetCoord : Coord)
  | Kick (targetCoord : Coord)
  | PickUp
  | Drop (itemIndex : Nat)
  | Wield (itemIndex : Nat)
  | Dip (itemIndex : Nat) (intoWater : WaterType)
  | Wait
  | Pray
  | Pay
  | Sacrifice (itemIndex : Nat)
  | Ascend
  | Descend
  | Eat (itemIndex : Nat)
  | Cast (spellIndex : Nat) (dir : Direction)
  | Donate (amount : Nat)
  | Apply (itemIndex : Nat)
deriving Repr, DecidableEq

/--
  Effect AST: Primitive, atomic state mutations resulting from an action.
  An action evaluates to a sequence of effects.
-/
inductive EffectAST where
  | SpendEnergy (amount : Nat)
  | RestoreEnergy (amount : Nat)
  | InflictDamage (amount : Nat)
  | HealDamage (amount : Nat)
  | MovePlayer (newCoord : Coord)
  | SetTile (coord : Coord) (newTile : Tile)
  | TransformBUC (itemIndex : Nat) (newBUC : BUC)
  | LogMessage (msg : String)
deriving Repr, DecidableEq

/-- Minimal World State for AST evaluation -/
structure WorldState where
  playerCoord : Coord
  playerEnergy : Nat
  playerCombat : Combatant
  turn : Nat
deriving Repr

/--
  Operational semantics: Evaluating atomic effects on the WorldState.
-/
def applyEffect (s : WorldState) : EffectAST → WorldState
  | EffectAST.SpendEnergy amount =>
    { s with playerEnergy := s.playerEnergy - amount }
  | EffectAST.RestoreEnergy amount =>
    { s with playerEnergy := s.playerEnergy + amount }
  | EffectAST.InflictDamage amount =>
    { s with playerCombat := applyDamage s.playerCombat amount }
  | EffectAST.HealDamage amount =>
    let newHp := min s.playerCombat.maxHp (s.playerCombat.hp + amount)
    { s with playerCombat := { s.playerCombat with hp := newHp } }
  | EffectAST.MovePlayer newCoord =>
    { s with playerCoord := newCoord }
  | EffectAST.SetTile _ _ => s
  | EffectAST.TransformBUC _ _ => s
  | EffectAST.LogMessage _ => s

/-- Apply a list of effects sequentially -/
def applyEffects (s : WorldState) (effects : List EffectAST) : WorldState :=
  effects.foldl applyEffect s

/--
  Evaluate an ActionAST into its operational effects and new state.
-/
def evalAction (s : WorldState) : ActionAST → WorldState × List EffectAST
  | ActionAST.Wait =>
    let effects := [EffectAST.SpendEnergy NORMAL_SPEED]
    (applyEffects s effects, effects)
  | ActionAST.Move _ =>
    let effects := [EffectAST.SpendEnergy NORMAL_SPEED]
    (applyEffects s effects, effects)
  | ActionAST.MeleeAttack _ =>
    let effects := [EffectAST.SpendEnergy NORMAL_SPEED]
    (applyEffects s effects, effects)
  | _ =>
    let effects := [EffectAST.SpendEnergy NORMAL_SPEED]
    (applyEffects s effects, effects)

/--
  World Well-Formedness Invariant.
  A state is well-formed if player HP does not exceed max HP.
-/
def WorldWellFormed (s : WorldState) : Prop :=
  s.playerCombat.hp ≤ s.playerCombat.maxHp

/--
  Theorem: Wait action consumes exactly NORMAL_SPEED energy.
-/
theorem wait_consumes_normal_speed (s : WorldState) :
  let (nextS, _) := evalAction s ActionAST.Wait
  nextS.playerEnergy = s.playerEnergy - NORMAL_SPEED := by
  rfl

/--
  Theorem: Inflicting damage preserves the World Well-Formedness invariant.
-/
theorem inflict_damage_preserves_well_formed (s : WorldState) (dmg : Nat) (hwf : WorldWellFormed s) :
  WorldWellFormed (applyEffect s (EffectAST.InflictDamage dmg)) := by
  unfold WorldWellFormed at hwf ⊢
  unfold applyEffect
  dsimp
  have hle := apply_damage_monotone_hp s.playerCombat dmg
  have heq := apply_damage_preserves_max_hp s.playerCombat dmg
  rw [heq]
  exact Nat.le_trans hle hwf

/--
  Theorem: Healing damage preserves the World Well-Formedness invariant.
-/
theorem heal_damage_preserves_well_formed (s : WorldState) (amount : Nat) :
  WorldWellFormed (applyEffect s (EffectAST.HealDamage amount)) := by
  unfold WorldWellFormed applyEffect
  dsimp
  exact Nat.min_le_left s.playerCombat.maxHp (s.playerCombat.hp + amount)

/--
  Theorem: Restoring energy leaves player combat stats invariant.
-/
theorem restore_energy_preserves_combat (s : WorldState) (amt : Nat) :
  (applyEffect s (EffectAST.RestoreEnergy amt)).playerCombat = s.playerCombat := by
  rfl

end NetMechanics
