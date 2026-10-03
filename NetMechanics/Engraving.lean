/-
  NetHack Mechanics Formalized in Lean 4: Engravings & Elbereth Wards
  Formalizing engraving mediums, smudge degradation, and ward repulsion predicates
  from NetHack (engrave.c, monmove.c).
-/

namespace NetMechanics

/-- Medium used to create an engraving on the floor -/
inductive EngravingMedium where
  | Burned                     -- Wand of fire, lightning, magic missile (permanent)
  | Carved (durability : Nat)  -- Hard rock with edged blade/athame
  | Marked (durability : Nat)  -- Chalk or blood
  | Dust (durability : Nat)    -- Finger in dust (rapidly smudged)
deriving Repr, DecidableEq

/-- Floor engraving record -/
structure Engraving where
  text : String
  medium : EngravingMedium
deriving Repr, DecidableEq

/--
  Smudge transition:
  When an entity attacks or steps on an engraving, non-permanent mediums degrade.
-/
def smudge (e : Engraving) : Option Engraving :=
  match e.medium with
  | EngravingMedium.Burned =>
    some e
  | EngravingMedium.Carved d =>
    match d with
    | 0 => none
    | Nat.succ d' => some { e with medium := EngravingMedium.Carved d' }
  | EngravingMedium.Marked d =>
    match d with
    | 0 => none
    | Nat.succ d' => some { e with medium := EngravingMedium.Marked d' }
  | EngravingMedium.Dust d =>
    match d with
    | 0 => none
    | Nat.succ d' => some { e with medium := EngravingMedium.Dust d' }

/--
  Elbereth ward active predicate:
  An engraving wards against a monster if:
  1. The text is exactly "Elbereth"
  2. The monster is NOT blind (can see the runes)
  3. The monster is NOT covetous (covetous bosses like Rodney and Riders ignore Elbereth)
-/
def isElberethWardActive (eng : Option Engraving) (monsterBlind : Bool) (monsterCovetous : Bool) : Bool :=
  match eng with
  | none => false
  | some e =>
    if monsterBlind || monsterCovetous then
      false
    else
      e.text == "Elbereth"

/--
  Theorem: Permanence of Burned Engravings.
  Burned runes resist trampling and physical smudging without degrading.
-/
theorem burned_engraving_permanent (txt : String) :
  let e : Engraving := { text := txt, medium := EngravingMedium.Burned }
  smudge e = some e := by
  rfl

/--
  Theorem: Blind monsters are never repelled by Elbereth.
  Monsters unable to see the floor do not perceive the ward.
-/
theorem blind_monster_ignores_elbereth (eng : Option Engraving) (cov : Bool) :
  isElberethWardActive eng true cov = false := by
  cases eng <;> rfl

/--
  Theorem: Covetous monsters ignore Elbereth.
  Special quest bosses and high-ranking demons disregard the ward.
-/
theorem covetous_monster_ignores_elbereth (eng : Option Engraving) (blind : Bool) :
  isElberethWardActive eng blind true = false := by
  cases eng with
  | none => rfl
  | some e =>
    cases blind <;> rfl

/--
  Theorem: Non-Elbereth text never grants ward protection.
-/
theorem arbitrary_text_not_warding (txt : String) (med : EngravingMedium)
  (h : (txt == "Elbereth") = false) (blind cov : Bool) :
  isElberethWardActive (some { text := txt, medium := med }) blind cov = false := by
  cases blind
  · cases cov
    · simp [isElberethWardActive, h]
    · rfl
  · rfl

/--
  Theorem: Zero-durability dust engravings are completely erased by smudge.
-/
theorem dust_zero_durability_erased (txt : String) :
  smudge { text := txt, medium := EngravingMedium.Dust 0 } = none := by
  rfl

end NetMechanics
