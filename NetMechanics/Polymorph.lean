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
  Damage resolution (C `hack.c:4256` `losehp`, `polyself.c:1367` `rehumanize`):
  if polymorphed, `u.mh -= n`; when it drops below 1 the hero rehumanizes with
  NO carry-over to `u.uhp` (excess damage is discarded), unless `Unchanging`,
  in which case the hero dies. Not polymorphed: `u.uhp -= n`, death at < 1.
  Returns the updated entity and whether the hero died.
-/
def applyPolyDamage (e : PolyEntity) (damage : Nat) (unchanging : Bool) : PolyEntity × Bool :=
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
    else if unchanging then
      -- Unchanging: cannot rehumanize, dies in creature form
      ({ e with polyForm := some { poly with hp := 0 } }, true)
    else
      -- rehumanize: revert to base, excess discarded, base HP untouched
      ({ e with polyForm := none }, e.baseForm.hp == 0)

/--
  Theorem: Fatal polymorph damage triggers form reversion (absent Unchanging).
-/
theorem poly_fatal_damage_reverts (e : PolyEntity) (poly : FormStats)
  (he : e.polyForm = some poly) (damage : Nat) (hdam : damage ≥ poly.hp) :
  (applyPolyDamage e damage false).1.polyForm = none := by
  have hnot : ¬ (damage < poly.hp) := Nat.not_lt.mpr hdam
  simp [applyPolyDamage, he, hnot]

/--
  Theorem: Reverting never touches base HP (any damage >= poly.hp, so overkill
  is discarded; generalises exact depletion).
-/
theorem poly_exact_depletion_preserves_base_hp (e : PolyEntity) (poly : FormStats)
  (he : e.polyForm = some poly) (damage : Nat) (hdam : damage ≥ poly.hp) :
  (applyPolyDamage e damage false).1.baseForm.hp = e.baseForm.hp := by
  have hnot : ¬ (damage < poly.hp) := Nat.not_lt.mpr hdam
  simp [applyPolyDamage, he, hnot]

/--
  Theorem: Non-fatal polymorph damage preserves base form and stays polymorphed.
-/
theorem poly_non_fatal_damage_preserves_poly (e : PolyEntity) (poly : FormStats)
  (he : e.polyForm = some poly) (damage : Nat) (hlt : damage < poly.hp) (u : Bool) :
  (applyPolyDamage e damage u).1.polyForm.isSome = true := by
  simp [applyPolyDamage, he, hlt]

/--
  Theorem: Base max HP is invariant under polymorph damage resolution.
-/
theorem poly_damage_preserves_base_max_hp (e : PolyEntity) (damage : Nat) (u : Bool) :
  (applyPolyDamage e damage u).1.baseForm.maxHp = e.baseForm.maxHp := by
  cases he : e.polyForm with
  | none =>
    simp [applyPolyDamage, he]
  | some poly =>
    unfold applyPolyDamage
    rw [he]
    dsimp only
    split
    · rfl
    · split <;> rfl

/--
  Theorem: Rehumanizing (damage >= poly.hp, not Unchanging) leaves the base HP
  untouched and kills only if base HP was already 0.
-/
theorem poly_reversion_preserves_base_hp (e : PolyEntity) (poly : FormStats) (damage : Nat)
  (he : e.polyForm = some poly) (hdam : damage ≥ poly.hp) :
  let res := applyPolyDamage e damage false
  res.1.polyForm = none ∧ res.1.baseForm = e.baseForm ∧ res.2 = (e.baseForm.hp == 0) := by
  have hnot : ¬ (damage < poly.hp) := Nat.not_lt.mpr hdam
  simp [applyPolyDamage, he, hnot]

theorem poly_reversion_preserves_base_stats (e : PolyEntity) (poly : FormStats) (damage : Nat)
  (he : e.polyForm = some poly)
  (hdam : damage ≥ poly.hp)
  (hbase : e.baseForm.hp ≠ 0) :
  let res := applyPolyDamage e damage false
  res.1.polyForm = none ∧
  res.1.baseForm.maxHp = e.baseForm.maxHp ∧
  res.1.baseForm.name = e.baseForm.name ∧
  res.2 = false := by
  have hnot : ¬ (damage < poly.hp) := Nat.not_lt.mpr hdam
  simp [applyPolyDamage, he, hnot, hbase]

/--
  Theorem: With Unchanging, fatal polyform damage kills (C `rehumanize`).
-/
theorem poly_unchanging_fatal_dies (e : PolyEntity) (poly : FormStats) (damage : Nat)
  (he : e.polyForm = some poly) (hdam : damage ≥ poly.hp) :
  (applyPolyDamage e damage true).2 = true := by
  have hnot : ¬ (damage < poly.hp) := Nat.not_lt.mpr hdam
  simp [applyPolyDamage, he, hnot]

-- 2. Unique Entities
structure Monster where
  name : String
  isUnique : Bool
deriving Repr, DecidableEq

def tryPolymorph (m : Monster) (targetSpecies : String) : Monster :=
  if m.isUnique then m
  else { m with name := targetSpecies }

theorem unique_entities_poly_invariant (m : Monster) (target : String)
  (h : m.isUnique = true) : tryPolymorph m target = m := by
  simp [tryPolymorph, h]

-- 3. Polypiling
structure ItemStack where
  category : String
  count : Nat
deriving Repr, DecidableEq

def polypile (stack : ItemStack) (shockFactor : Nat) : ItemStack :=
  if shockFactor == 0 then
    { stack with count := 0 }
  else if shockFactor == 1 then
    { stack with count := stack.count / 2 }
  else
    stack

theorem polypile_preserves_or_reduces_count (stack : ItemStack) (shock : Nat) :
  (polypile stack shock).count ≤ stack.count := by
  unfold polypile
  split
  · exact Nat.zero_le _
  · split
    · exact Nat.div_le_self _ _
    · exact Nat.le_refl _

-- 4. Lycanthropy
inductive LycanthropyState
  | clean
  | infected (species : String)
deriving Repr, DecidableEq

inductive CureItem
  | wolfsbane
  | holyWater
  | regularFood
deriving Repr, DecidableEq

def consumeItem (state : LycanthropyState) (item : CureItem) : LycanthropyState :=
  match state with
  | .clean => .clean
  | .infected s =>
    match item with
    | .wolfsbane => .clean
    | .holyWater => .clean
    | .regularFood => .infected s

theorem lycanthropy_cure_restores_clean_state (state : LycanthropyState) (item : CureItem) (s : String)
  (h_inf : state = .infected s)
  (h_cure : item = .wolfsbane ∨ item = .holyWater) :
  consumeItem state item = .clean := by
  cases h_cure with
  | inl h1 => simp [consumeItem, h_inf, h1]
  | inr h2 => simp [consumeItem, h_inf, h2]


end NetMechanics
