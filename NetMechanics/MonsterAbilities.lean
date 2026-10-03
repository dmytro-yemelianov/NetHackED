/-
  NetHack Mechanics Formalized in Lean 4: Monster Abilities & Breath Weapons
  Formalizing dragon breath attacks, gaze attacks, reflection, resistances, and monster spellcasting.
-/

namespace NetMechanics

inductive BreathType where
  | Fire
  | Cold
  | Shock
  | Sleep
  | Poison
  | Disintegration
deriving Repr, DecidableEq

structure Resistances where
  fire : Bool
  cold : Bool
  shock : Bool
  poison : Bool
  disintegration : Bool
deriving Repr, DecidableEq

def defaultResistances : Resistances :=
  ⟨false, false, false, false, false⟩

def hasResistance (r : Resistances) : BreathType → Bool
  | BreathType.Fire => r.fire
  | BreathType.Cold => r.cold
  | BreathType.Shock => r.shock
  | BreathType.Poison => r.poison
  | BreathType.Disintegration => r.disintegration
  | BreathType.Sleep => false

/-- Breath attack resolution:
    - If target has reflection, ray bounces (0 damage to target, reflected = true)
    - If target has resistance to breath element, damage is reduced to 0 (absorbed)
    - Otherwise full damage is dealt -/
def resolveBreathDamage (rawDamage : Nat) (b : BreathType) (res : Resistances) (hasReflection : Bool) : Nat × Bool :=
  if hasReflection then
    (0, true)
  else if hasResistance res b then
    (0, false)
  else
    (rawDamage, false)

/--
  Theorem: Having resistance or reflection never increases damage taken compared to raw damage.
-/
theorem breath_damage_le_raw (raw : Nat) (b : BreathType) (res : Resistances) (hasReflect : Bool) :
    (resolveBreathDamage raw b res hasReflect).1 ≤ raw := by
  unfold resolveBreathDamage
  split
  · exact Nat.zero_le raw
  · split
    · exact Nat.zero_le raw
    · exact Nat.le_refl raw

inductive GazeType where
  | Paralysis
  | Petrification
  | Confusion
deriving Repr, DecidableEq

inductive GazeEffect where
  | ReflectedToAttacker
  | BlindImmune
  | Afflicted (g : GazeType)
deriving Repr, DecidableEq

/-- Gaze attack resolution:
    - If defender has reflection (or mirror), gaze is reflected back onto attacker
    - If defender is blind, gaze has no effect (immune)
    - Otherwise defender is afflicted by the gaze -/
def resolveGaze (g : GazeType) (hasReflection : Bool) (isBlind : Bool) : GazeEffect :=
  if hasReflection then
    GazeEffect.ReflectedToAttacker
  else if isBlind then
    GazeEffect.BlindImmune
  else
    GazeEffect.Afflicted g

/--
  Theorem: A defender with reflection always reflects gaze back to attacker regardless of blindness.
-/
theorem gaze_reflection_immune (g : GazeType) (isBlind : Bool) :
    resolveGaze g true isBlind = GazeEffect.ReflectedToAttacker := by
  rfl

/--
  Theorem: A blind defender without reflection is immune to gaze attacks.
-/
theorem gaze_blindness_immune (g : GazeType) :
    resolveGaze g false true = GazeEffect.BlindImmune := by
  rfl

inductive MonsterSpell where
  | SummonMonsters
  | CurseItems
  | RaiseDead
  | CauseWounds
deriving Repr, DecidableEq

/-- Monster spell summon capacity bound:
    Summoning cannot exceed level capacity limit maxCapacity -/
def calculateSummonCount (currentMonsters : Nat) (maxCapacity : Nat) (desiredSummons : Nat) : Nat :=
  if currentMonsters ≥ maxCapacity then
    0
  else
    min desiredSummons (maxCapacity - currentMonsters)

/--
  Theorem: Summoning never causes total monster count to exceed maxCapacity.
-/
theorem summon_count_bounded (cur : Nat) (cap : Nat) (desired : Nat) :
    cur + calculateSummonCount cur cap desired ≤ max cap cur := by
  unfold calculateSummonCount
  split
  · next _ =>
    rw [Nat.add_zero]
    exact Nat.le_max_right cap cur
  · next h =>
    have hlt : cur < cap := Nat.lt_of_not_ge h
    have h_min : min desired (cap - cur) ≤ cap - cur := Nat.min_le_right desired (cap - cur)
    have h_sum : cur + min desired (cap - cur) ≤ cur + (cap - cur) := Nat.add_le_add_left h_min cur
    have h_sub : cur + (cap - cur) = cap := Nat.add_sub_of_le (Nat.le_of_lt hlt)
    rw [h_sub] at h_sum
    exact Nat.le_trans h_sum (Nat.le_max_left cap cur)

end NetMechanics
