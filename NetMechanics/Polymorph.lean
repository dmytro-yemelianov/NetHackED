/-
  NetHack Mechanics Formalized in Lean 4: Polymorph & Shape-Shifting Invariants
  Formalizing polymorph HP buffer pools and the revert-to-base-on-death invariant (polyself.c).
-/

namespace NetMechanics

/-- Form representation with health attributes -/
structure FormStats where
  hp : Nat
  maxHp : Nat
  name : String
deriving Repr, DecidableEq

/-- Entity polymorph state -/
structure PolyEntity where
  baseForm : FormStats
  polyForm : Option FormStats
deriving Repr, DecidableEq

def isPolymorphed (e : PolyEntity) : Bool :=
  e.polyForm.isSome

/--
  Damage resolution with Polymorph HP buffer:
  If polymorphed, damage is absorbed by the polymorph form first.
  Lethal damage to polymorph form breaks the transformation, reverting
  to the base form, with only excess damage penetrating to the base HP pool.
-/
def applyPolyDamage (e : PolyEntity) (damage : Nat) : PolyEntity × Bool :=
  match e.polyForm with
  | none =>
    let newBaseHp := e.baseForm.hp - damage
    let isDead := newBaseHp == 0
    ({ e with baseForm := { e.baseForm with hp := newBaseHp } }, isDead)
  | some poly =>
    if damage < poly.hp then
      -- Poly form absorbs all damage
      let newPoly := { poly with hp := poly.hp - damage }
      ({ e with polyForm := some newPoly }, false)
    else
      -- Lethal to poly form: revert to base, excess penetrates
      let excessDamage := damage - poly.hp
      let newBaseHp := e.baseForm.hp - excessDamage
      let isDead := newBaseHp == 0
      ({ e with baseForm := { e.baseForm with hp := newBaseHp }, polyForm := none }, isDead)

/--
  Theorem: Fatal polymorph damage triggers form reversion.
  Taking damage >= poly HP strictly breaks the polymorph form.
-/
theorem poly_fatal_damage_reverts (e : PolyEntity) (poly : FormStats)
  (he : e.polyForm = some poly) (damage : Nat) (hdam : damage ≥ poly.hp) :
  (applyPolyDamage e damage).1.polyForm = none := by
  simp [applyPolyDamage, he]
  have hnot : ¬ (damage < poly.hp) := Nat.not_lt.mpr hdam
  simp [hnot]

/--
  Theorem: Exact polymorph depletion leaves base HP completely untouched.
  If damage == poly.hp, excess damage is 0, so base HP is completely preserved.
-/
theorem poly_exact_depletion_preserves_base_hp (e : PolyEntity) (poly : FormStats)
  (he : e.polyForm = some poly) :
  (applyPolyDamage e poly.hp).1.baseForm.hp = e.baseForm.hp := by
  simp [applyPolyDamage, he]

/--
  Theorem: Non-fatal polymorph damage preserves base form and stays polymorphed.
-/
theorem poly_non_fatal_damage_preserves_poly (e : PolyEntity) (poly : FormStats)
  (he : e.polyForm = some poly) (damage : Nat) (hlt : damage < poly.hp) :
  (applyPolyDamage e damage).1.polyForm.isSome = true := by
  simp [applyPolyDamage, he, hlt]

/--
  Theorem: Base max HP is invariant under polymorph damage resolution.
-/
theorem poly_damage_preserves_base_max_hp (e : PolyEntity) (damage : Nat) :
  (applyPolyDamage e damage).1.baseForm.maxHp = e.baseForm.maxHp := by
  cases he : e.polyForm with
  | none =>
    simp [applyPolyDamage, he]
  | some poly =>
    simp [applyPolyDamage, he]
    split <;> rfl

end NetMechanics
