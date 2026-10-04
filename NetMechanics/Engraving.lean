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
  NetHack 5.0 C `onscary` (`monmove.c:240-303`):
  Humans, minotaurs, shopkeepers/priests/guards, and riders are exempt from Elbereth.
-/
def onscaryExempt (isHuman isMinotaur isShopkeeper isRider : Bool) : Bool :=
  isHuman || isMinotaur || isShopkeeper || isRider

theorem human_onscary_exempt (m shk r : Bool) :
  onscaryExempt true m shk r = true := rfl

theorem shopkeeper_onscary_exempt (h m r : Bool) :
  onscaryExempt h m true r = true := by
  cases h <;> cases m <;> rfl

/--
  Elbereth ward active predicate:
  An engraving wards against a monster if:
  1. The text is exactly "Elbereth"
  2. The monster is NOT blind (can see the runes)
  3. The monster is NOT covetous (covetous bosses like Rodney and Riders ignore Elbereth)
  4. The monster is NOT peaceful (peacefuls don't fear Elbereth)
  5. The monster is NOT exempt (humans, minotaurs, shopkeepers, riders)
-/
def isElberethWardActive (eng : Option Engraving) (monsterBlind : Bool) (monsterCovetous : Bool)
    (monsterPeaceful : Bool) (monsterExempt : Bool) : Bool :=
  match eng with
  | none => false
  | some e =>
    if monsterBlind || monsterCovetous || monsterPeaceful || monsterExempt then
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
theorem blind_monster_ignores_elbereth (eng : Option Engraving) (cov pfc ex : Bool) :
  isElberethWardActive eng true cov pfc ex = false := by
  cases eng <;> rfl

/--
  Theorem: Covetous monsters ignore Elbereth.
  Special quest bosses and high-ranking demons disregard the ward.
-/
theorem covetous_monster_ignores_elbereth (eng : Option Engraving) (blind pfc ex : Bool) :
  isElberethWardActive eng blind true pfc ex = false := by
  cases eng with
  | none => rfl
  | some e => cases blind <;> rfl

/--
  Theorem: Peaceful monsters ignore Elbereth (monmove.c:300).
-/
theorem peaceful_monster_ignores_elbereth (eng : Option Engraving) (blind cov ex : Bool) :
  isElberethWardActive eng blind cov true ex = false := by
  cases eng with
  | none => rfl
  | some e => cases blind <;> cases cov <;> rfl

/--
  Theorem: Exempt monsters (humans, minotaurs, shopkeepers, riders) ignore Elbereth (monmove.c:260-302).
-/
theorem exempt_monster_ignores_elbereth (eng : Option Engraving) (blind cov pfc : Bool) :
  isElberethWardActive eng blind cov pfc true = false := by
  cases eng with
  | none => rfl
  | some e => cases blind <;> cases cov <;> cases pfc <;> rfl

/--
  Theorem: Non-Elbereth text never grants ward protection.
-/
theorem arbitrary_text_not_warding (txt : String) (med : EngravingMedium)
  (h : (txt == "Elbereth") = false) (blind cov pfc ex : Bool) :
  isElberethWardActive (some { text := txt, medium := med }) blind cov pfc ex = false := by
  cases blind <;> cases cov <;> cases pfc <;> cases ex
  · simp [isElberethWardActive, h]
  all_goals rfl

/--
  Theorem: Zero-durability dust engravings are completely erased by smudge.
-/
theorem dust_zero_durability_erased (txt : String) :
  smudge { text := txt, medium := EngravingMedium.Dust 0 } = none := by
  rfl

end NetMechanics
