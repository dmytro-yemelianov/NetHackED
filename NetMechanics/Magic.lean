/-
  NetHack Mechanics Formalized in Lean 4: Magic Spells & Mana (Power)
  Formalizes spell power consumption, non-negative mana bounds,
  and spellbook retention decay.
-/

namespace NetMechanics

/-- Canonical NetHack spell schools -/
inductive SpellSchool where
  | Attack
  | Healing
  | Divination
  | Enchantment
  | Escape
deriving Repr, DecidableEq

/-- Discrete spells available to casters -/
inductive SpellKind where
  | ForceBolt
  | MagicMissile
  | CureLightWounds
  | ExtraHealing
deriving Repr, DecidableEq

/-- Power cost (mana) to cast a spell -/
def manaCost : SpellKind -> Nat
  | SpellKind.ForceBolt        => 5
  | SpellKind.MagicMissile     => 10
  | SpellKind.CureLightWounds  => 5
  | SpellKind.ExtraHealing     => 15

/-- Predicate for whether caster possesses sufficient mana -/
def canCast (currentPw : Nat) (spell : SpellKind) : Prop :=
  currentPw >= manaCost spell

instance (currentPw : Nat) (spell : SpellKind) : Decidable (canCast currentPw spell) :=
  inferInstanceAs (Decidable (currentPw >= manaCost spell))

/-- Execute mana expenditure for casting -/
def castSpell (currentPw : Nat) (spell : SpellKind) : Option Nat :=
  if canCast currentPw spell then
    some (currentPw - manaCost spell)
  else
    none

/-- Theorem: Casting a spell never results in negative mana -/
theorem cast_preserves_non_negative_mana (currentPw : Nat) (spell : SpellKind)
  (nextPw : Nat) (h_cast : castSpell currentPw spell = some nextPw) :
  nextPw <= currentPw := by
  unfold castSpell canCast at h_cast
  split at h_cast
  · cases h_cast
    omega
  · contradiction

/-- Theorem: Insufficient mana strictly forbids casting -/
theorem insufficient_mana_fails (currentPw : Nat) (spell : SpellKind)
  (h_lt : currentPw < manaCost spell) :
  castSpell currentPw spell = none := by
  unfold castSpell canCast
  have h_not : ¬(currentPw >= manaCost spell) := by omega
  rw [if_neg h_not]

/-- Spell retention in memory (NetHack canonical memory is 20,000 turns) -/
def initialSpellRetention : Nat := 20000

/-- Turn-based memory decay -/
def decayRetention (retention : Nat) : Nat :=
  retention.sub 1

/-- Theorem: Spell retention monotonically decreases towards 0 -/
theorem retention_decay_monotonic (r : Nat) :
  decayRetention r <= r := by
  dsimp [decayRetention]
  omega

end NetMechanics
