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

/-- Nutrition points to hunger state classification -/
def hungerOfNutrition (n : Nat) : HungerState :=
  if n > 1000 then HungerState.Satiated
  else if n >= 150 then HungerState.Normal
  else if n >= 50 then HungerState.Hungry
  else if n >= 1 then HungerState.Weak
  else HungerState.Fainting

/-- Direct arithmetic tier of nutrition points: higher is more nourished -/
def hungerTierOfNutrition (n : Nat) : Nat :=
  if n > 1000 then 5
  else if n >= 150 then 4
  else if n >= 50 then 3
  else if n >= 1 then 2
  else 1

/--
  Theorem: Consuming food (adding nutrition k >= 0) monotonically preserves
  or improves the hunger state tier.
-/
theorem eating_improves_or_preserves_hunger (n k : Nat) :
  hungerTierOfNutrition n <= hungerTierOfNutrition (n + k) := by
  unfold hungerTierOfNutrition
  by_cases h1 : n > 1000
  · have h1k : n + k > 1000 := by omega
    rw [if_pos h1, if_pos h1k]
    omega
  · rw [if_neg h1]
    by_cases h2 : n >= 150
    · have h2k : n + k >= 150 := by omega
      by_cases h1k : n + k > 1000
      · rw [if_pos h2, if_pos h1k]
        omega
      · rw [if_pos h2, if_neg h1k, if_pos h2k]
        omega
    · rw [if_neg h2]
      by_cases h3 : n >= 50
      · have h3k : n + k >= 50 := by omega
        by_cases h1k : n + k > 1000
        · rw [if_pos h3, if_pos h1k]
          omega
        · rw [if_neg h1k]
          by_cases h2k : n + k >= 150
          · rw [if_pos h3, if_pos h2k]
            omega
          · rw [if_pos h3, if_neg h2k, if_pos h3k]
            omega
      · rw [if_neg h3]
        by_cases h4 : n >= 1
        · have h4k : n + k >= 1 := by omega
          by_cases h1k : n + k > 1000
          · rw [if_pos h4, if_pos h1k]
            omega
          · rw [if_neg h1k]
            by_cases h2k : n + k >= 150
            · rw [if_pos h4, if_pos h2k]
              omega
            · rw [if_neg h2k]
              by_cases h3k : n + k >= 50
              · rw [if_pos h4, if_pos h3k]
                omega
              · rw [if_pos h4, if_neg h3k, if_pos h4k]
                omega
        · rw [if_neg h4]
          by_cases h1k : n + k > 1000
          · rw [if_pos h1k]
            omega
          · rw [if_neg h1k]
            by_cases h2k : n + k >= 150
            · rw [if_pos h2k]
              omega
            · rw [if_neg h2k]
              by_cases h3k : n + k >= 50
              · rw [if_pos h3k]
                omega
              · rw [if_neg h3k]
                by_cases h4k : n + k >= 1
                · rw [if_pos h4k]
                  omega
                · rw [if_neg h4k]
                  omega

/-- Theorem: Zero nutrition maps directly to Fainting state -/
theorem zero_nutrition_fainting :
  hungerOfNutrition 0 = HungerState.Fainting := by
  dsimp [hungerOfNutrition]

/-- Single-turn passive metabolic consumption -/
def metabolicTick (n : Nat) : Nat :=
  n.sub 1

/-- Theorem: Passive metabolic tick decreases nutrition monotonically towards zero -/
theorem metabolic_tick_decreases (n : Nat) :
  metabolicTick n <= n := by
  dsimp [metabolicTick]
  omega

/-- Theorem: Nutrition is bounded below by zero under repeated metabolic ticks -/
theorem metabolic_tick_zero_fixed_point :
  metabolicTick 0 = 0 := by
  dsimp [metabolicTick]

end NetMechanics
