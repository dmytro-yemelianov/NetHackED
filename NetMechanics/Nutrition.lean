/-
  NetHack Mechanics Formalized in Lean 4: Nutrition & Hunger Clock
  Formalizes the hunger state transitions, consumption monotonic improvements,
  and starvation bounds.
-/

namespace NetMechanics

/-- Canonical NetHack hunger states -/
inductive HungerState where
  | Satiated
  | Normal
  | Hungry
  | Weak
  | Fainting
  | Starved
deriving Repr, DecidableEq

/--
  Nutrition points to hunger state classification (C `eat.c:3362` `newuhs`,
  starvation `eat.c:3437`). Nutrition is signed; `con` is `ACURR(A_CON)`.
-/
def hungerOfNutrition (n : Int) (con : Int) : HungerState :=
  if n > 1000 then HungerState.Satiated
  else if n > 150 then HungerState.Normal
  else if n > 50 then HungerState.Hungry
  else if n > 0 then HungerState.Weak
  else if n < -(100 + 10 * con) then HungerState.Starved
  else HungerState.Fainting

/-- Direct arithmetic tier of nutrition points: higher is more nourished -/
def hungerTierOfNutrition (n : Int) (con : Int) : Nat :=
  if n > 1000 then 5
  else if n > 150 then 4
  else if n > 50 then 3
  else if n > 0 then 2
  else if n < -(100 + 10 * con) then 0
  else 1

/--
  Theorem: Consuming food (adding nutrition k >= 0) monotonically preserves
  or improves the hunger state tier.
-/
theorem eating_improves_or_preserves_hunger (n k con : Int) (hk : 0 ≤ k) :
  hungerTierOfNutrition n con <= hungerTierOfNutrition (n + k) con := by
  unfold hungerTierOfNutrition
  repeat' split
  all_goals omega

/-- Theorem: Zero nutrition maps directly to Fainting state (Con >= 0) -/
theorem zero_nutrition_fainting (con : Int) (hcon : 0 ≤ con) :
  hungerOfNutrition 0 con = HungerState.Fainting := by
  unfold hungerOfNutrition
  repeat' split
  all_goals first | rfl | omega

/-- Theorem: For Con >= 0 the hero starves exactly below `-(100 + 10 * Con)` -/
theorem starved_iff (n con : Int) (hcon : 0 ≤ con) :
  hungerOfNutrition n con = HungerState.Starved ↔ n < -(100 + 10 * con) := by
  unfold hungerOfNutrition
  repeat' split
  all_goals simp
  all_goals omega

/-- Single-turn passive metabolic consumption (`u.uhunger--`; goes negative) -/
def metabolicTick (n : Int) : Int :=
  n - 1

/-- Theorem: Passive metabolic tick strictly decreases nutrition -/
theorem metabolic_tick_decreases (n : Int) :
  metabolicTick n < n := by
  dsimp [metabolicTick]
  omega

/-- Theorem: Repeated metabolic ticks keep decrementing past zero (no floor) -/
theorem metabolic_tick_zero_goes_negative :
  metabolicTick 0 = -1 := by
  dsimp [metabolicTick]

end NetMechanics

namespace NetMechanics

structure Corpse where
  species : String
  race : String
  nutrition : Nat
  age : Nat
  rotThreshold : Nat

structure Hero where
  race : String
  nutrition : Nat
  poisoned : Bool
  cannibalism_violation : Bool
  fire_resistance : Bool
  telepathy : Bool

inductive Intrinsic where
  | FireResistance
  | Telepathy
  deriving Repr, BEq

def isTainted (c : Corpse) : Bool :=
  c.age > c.rotThreshold

def grantsIntrinsic (p : Nat) : Bool :=
  p > 0

def eatCorpse (h : Hero) (c : Corpse) (p : Nat) (intr : Intrinsic) : Hero :=
  let tainted := isTainted c
  let cannibal := c.race == h.race
  let gets_intrinsic := grantsIntrinsic p
  { h with
    nutrition := if tainted then h.nutrition else h.nutrition + c.nutrition,
    poisoned := if tainted then true else h.poisoned,
    cannibalism_violation := if cannibal then true else h.cannibalism_violation,
    fire_resistance := if gets_intrinsic && intr == Intrinsic.FireResistance then true else h.fire_resistance,
    telepathy := if gets_intrinsic && intr == Intrinsic.Telepathy then true else h.telepathy
  }

theorem fresh_corpse_provides_nutrition (h : Hero) (c : Corpse) (p : Nat) (intr : Intrinsic) (h_fresh : c.age ≤ c.rotThreshold) :
    (eatCorpse h c p intr).nutrition = h.nutrition + c.nutrition := by
  dsimp [eatCorpse, isTainted]
  have h_tainted : (c.age > c.rotThreshold) = false := by
    simp
    omega
  simp [h_tainted]

theorem tainted_corpse_causes_poisoning (h : Hero) (c : Corpse) (p : Nat) (intr : Intrinsic) (h_tainted : c.age > c.rotThreshold) :
    (eatCorpse h c p intr).poisoned = true := by
  dsimp [eatCorpse, isTainted]
  simp [h_tainted]

theorem cannibalism_detects_same_race (h : Hero) (c : Corpse) (p : Nat) (intr : Intrinsic) (h_race : c.race = h.race) :
    (eatCorpse h c p intr).cannibalism_violation = true := by
  dsimp [eatCorpse]
  have h_eq : (c.race == h.race) = true := by
    simp [h_race]
  simp [h_eq]

end NetMechanics
