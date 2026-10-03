namespace NetMechanics

structure VoluntaryConductTracker where
  pacifist : Bool
  vegan : Bool
  vegetarian : Bool
  atheist : Bool
  illiterate : Bool
  genocideless : Bool
  polypileless : Bool
  wishless : Bool

inductive GameAction where
  | DirectKill
  | EatMeat
  | EatAnimalProduct
  | PrayOrSacrifice
  | ReadScrollOrSpellbook
  | ReadCastGenocide
  | Polypile
  | Wish
  | Move

def applyAction (tracker : VoluntaryConductTracker) (action : GameAction) : VoluntaryConductTracker :=
  match action with
  | GameAction.DirectKill => { tracker with pacifist := false }
  | GameAction.EatMeat => { tracker with vegetarian := false, vegan := false }
  | GameAction.EatAnimalProduct => { tracker with vegan := false }
  | GameAction.PrayOrSacrifice => { tracker with atheist := false }
  | GameAction.ReadScrollOrSpellbook => { tracker with illiterate := false }
  | GameAction.ReadCastGenocide => { tracker with genocideless := false }
  | GameAction.Polypile => { tracker with polypileless := false }
  | GameAction.Wish => { tracker with wishless := false }
  | GameAction.Move => tracker

theorem kill_invalidates_pacifist (t : VoluntaryConductTracker) :
  (applyAction t GameAction.DirectKill).pacifist = false := by rfl

theorem reading_invalidates_illiterate (t : VoluntaryConductTracker) :
  (applyAction t GameAction.ReadScrollOrSpellbook).illiterate = false := by rfl

theorem meat_invalidates_vegan_and_vegetarian (t : VoluntaryConductTracker) :
  (applyAction t GameAction.EatMeat).vegan = false ∧ (applyAction t GameAction.EatMeat).vegetarian = false := by
  exact ⟨rfl, rfl⟩

theorem conduct_violations_are_irreversible (t : VoluntaryConductTracker) (a : GameAction) :
  (t.pacifist = false → (applyAction t a).pacifist = false) ∧
  (t.vegan = false → (applyAction t a).vegan = false) ∧
  (t.vegetarian = false → (applyAction t a).vegetarian = false) ∧
  (t.atheist = false → (applyAction t a).atheist = false) ∧
  (t.illiterate = false → (applyAction t a).illiterate = false) ∧
  (t.genocideless = false → (applyAction t a).genocideless = false) ∧
  (t.polypileless = false → (applyAction t a).polypileless = false) ∧
  (t.wishless = false → (applyAction t a).wishless = false) := by
  cases a <;> simp [applyAction] <;> simp

end NetMechanics
