/-
  NetHack Mechanics Formalized in Lean 4: BUC System (Blessed / Uncursed / Cursed)
  Eliminating C bitfield representation bugs by construction.
-/

namespace NetMechanics

/--
  In C NetHack (obj.h):
    Bitfield(cursed, 1);
    Bitfield(blessed, 1);
  where "uncursed when neither cursed nor blessed".
  This allows the illegal state (cursed = 1 ∧ blessed = 1).
  In Lean 4, we define BUC as a disjoint inductive sum type,
  guaranteeing mutual exclusion by construction.
-/
inductive BUC where
  | Blessed
  | Uncursed
  | Cursed
deriving Repr, DecidableEq

/-- Check if an item is blessed -/
def BUC.isBlessed : BUC → Bool
  | Blessed => true
  | _ => false

/-- Check if an item is uncursed -/
def BUC.isUncursed : BUC → Bool
  | Uncursed => true
  | _ => false

/-- Check if an item is cursed -/
def BUC.isCursed : BUC → Bool
  | Cursed => true
  | _ => false

/--
  Theorem: Mutual Exclusion.
  An item cannot be simultaneously blessed and cursed.
-/
theorem buc_not_blessed_and_cursed (b : BUC) :
  ¬(b.isBlessed = true ∧ b.isCursed = true) := by
  cases b <;> decide

/--
  Theorem: Completeness.
  Every item is exactly one of Blessed, Uncursed, or Cursed.
-/
theorem buc_trichotomy (b : BUC) :
  (b = BUC.Blessed) ∨ (b = BUC.Uncursed) ∨ (b = BUC.Cursed) := by
  cases b
  · exact Or.inl rfl
  · exact Or.inr (Or.inl rfl)
  · exact Or.inr (Or.inr rfl)

/-- Water types used for dipping -/
inductive WaterType where
  | Holy
  | Plain
  | Unholy
deriving Repr, DecidableEq

/-- Dipping an item into water of various types -/
def dipWater (water : WaterType) (item : BUC) : BUC :=
  match water with
  | WaterType.Holy => BUC.Blessed
  | WaterType.Plain =>
    match item with
    | BUC.Blessed => BUC.Uncursed
    | BUC.Cursed => BUC.Uncursed
    | BUC.Uncursed => BUC.Uncursed
  | WaterType.Unholy => BUC.Cursed

/--
  Theorem: Dipping into holy water always produces a blessed item,
  regardless of original status.
-/
theorem dip_holy_always_blessed (b : BUC) :
  dipWater WaterType.Holy b = BUC.Blessed := by
  rfl

/--
  Theorem: Holy water dipping is idempotent.
-/
theorem dip_holy_idempotent (b : BUC) :
  dipWater WaterType.Holy (dipWater WaterType.Holy b) = dipWater WaterType.Holy b := by
  rfl

/--
  Theorem: Plain water always neutralizes both blessed and cursed items.
-/
theorem dip_plain_always_uncursed (b : BUC) :
  dipWater WaterType.Plain b = BUC.Uncursed := by
  cases b <;> rfl

/--
  Uncursing action (e.g. reading a non-cursed scroll of remove curse).
  Uncurses cursed items, leaves others unchanged.
-/
def uncurse (b : BUC) : BUC :=
  match b with
  | BUC.Cursed => BUC.Uncursed
  | other => other

/--
  Theorem: Uncursing is idempotent.
-/
theorem uncurse_idempotent (b : BUC) :
  uncurse (uncurse b) = uncurse b := by
  cases b <;> rfl

/--
  Theorem: Uncursing never results in a cursed item.
-/
theorem uncurse_never_cursed (b : BUC) :
  uncurse b ≠ BUC.Cursed := by
  cases b <;> decide

/--
  Epistemic state of the player regarding BUC.
-/
inductive BUCKnowledge where
  | Unknown
  | Known (status : BUC)
deriving Repr, DecidableEq

/-- Monotonic information refinement (knowledge order) -/
def BUCKnowledge.le : BUCKnowledge → BUCKnowledge → Prop
  | Unknown, _ => True
  | Known s1, Known s2 => s1 = s2
  | Known _, Unknown => False

/--
  Theorem: Knowledge order is reflexive.
-/
theorem knowledge_refl (k : BUCKnowledge) : BUCKnowledge.le k k := by
  cases k with
  | Unknown => trivial
  | Known s => rfl

end NetMechanics
