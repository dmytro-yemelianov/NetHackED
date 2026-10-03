/-
  NetMechanics.Religion: Formalized Pantheons, Divine Favor, Altar Sacrifices and Holy Water.
  Models piety calculus, prayer cooldowns, altar conversions, and holy water consecration.
-/

import NetMechanics.Grid
import NetMechanics.BUC

namespace NetMechanics

/-- A deity in the pantheon -/
structure Deity where
  name : String
  align : Alignment
deriving Repr, DecidableEq

/-- 3-deity Pantheon for a role (Lawful, Neutral, Chaotic) -/
structure Pantheon where
  lawful : Deity
  neutral : Deity
  chaotic : Deity
deriving Repr, DecidableEq

/-- Canonical Valkyrie Pantheon (Tyr, Odin, Loki) -/
def valkyriePantheon : Pantheon := {
  lawful := ⟨"Tyr", Alignment.Lawful⟩,
  neutral := ⟨"Odin", Alignment.Neutral⟩,
  chaotic := ⟨"Loki", Alignment.Chaotic⟩,
}

/-- Canonical Wizard Pantheon (Ptah, Thoth, Anhur) -/
def wizardPantheon : Pantheon := {
  lawful := ⟨"Ptah", Alignment.Lawful⟩,
  neutral := ⟨"Thoth", Alignment.Neutral⟩,
  chaotic := ⟨"Anhur", Alignment.Chaotic⟩,
}

/-- Divine state of a character -/
structure DivineState where
  favor : Int            -- Piety in [-20, 20]
  prayerTimeout : Nat    -- Turns until safe to pray again
  giftCount : Nat        -- Artifacts gifted by deity
deriving Repr, DecidableEq

/-- Clamp favor within bounds [-20, 20] -/
def clampFavor (f : Int) : Int :=
  if f > 20 then 20 else if f < -20 then -20 else f

/-- Theorem: clampFavor is bounded in [-20, 20] -/
theorem clamp_favor_bounded (f : Int) : -20 ≤ clampFavor f ∧ clampFavor f ≤ 20 := by
  dsimp [clampFavor]
  split
  · omega
  · split <;> omega

/-- Tick prayer cooldown timer down by 1 -/
def tickPrayerTimeout (timeout : Nat) : Nat :=
  timeout.pred

/-- Theorem: prayer cooldown strictly decreases if positive -/
theorem prayer_cooldown_strictly_decreases (timeout : Nat) (h : timeout > 0) :
  tickPrayerTimeout timeout < timeout := by
  dsimp [tickPrayerTimeout]
  omega

/-- Sacrificing a corpse on an altar -/
inductive SacrificeResult where
  | AltarConverted (newAlign : Alignment)
  | FavorIncreased (newFavor : Int)
  | DivineGift (artifactName : String)
deriving Repr, DecidableEq

/-- Resolve sacrifice of a corpse -/
def resolveSacrifice (state : DivineState) (heroAlign altarAlign : Alignment) (corpseNutrition : Nat) :
    DivineState × SacrificeResult :=
  if heroAlign ≠ altarAlign then
    -- Cross-aligned altar converts to hero's alignment
    ({ state with favor := clampFavor (state.favor + 2) }, SacrificeResult.AltarConverted heroAlign)
  else
    let favorGain : Int := if corpseNutrition >= 200 then 3 else 1
    let newFavor := clampFavor (state.favor + favorGain)
    if newFavor ≥ 15 ∧ state.giftCount == 0 then
      ({ state with favor := newFavor, giftCount := 1 }, SacrificeResult.DivineGift "Excalibur")
    else
      ({ state with favor := newFavor }, SacrificeResult.FavorIncreased newFavor)

/-- Simplification lemma for co-aligned sacrifice favor -/
theorem resolveSacrifice_coaligned_favor (state : DivineState) (align : Alignment) (nutr : Nat) :
    (resolveSacrifice state align align nutr).1.favor =
      clampFavor (state.favor + if nutr >= 200 then 3 else 1) := by
  dsimp [resolveSacrifice]
  split
  · contradiction
  · split
    · split <;> rfl
    · split <;> rfl

/-- Theorem: Co-aligned sacrifice strictly increases divine favor within normal bounds -/
theorem sacrifice_coaligned_increases_favor (state : DivineState) (align : Alignment) (nutr : Nat)
    (h_low : -20 ≤ state.favor) (h_high : state.favor ≤ 17) :
    state.favor < (resolveSacrifice state align align nutr).1.favor := by
  rw [resolveSacrifice_coaligned_favor]
  dsimp [clampFavor]
  by_cases hn : nutr ≥ 200
  · rw [if_pos hn]
    have h2 : ¬(state.favor + 3 > 20) := by omega
    have h3 : ¬(state.favor + 3 < -20) := by omega
    rw [if_neg h2, if_neg h3]
    omega
  · rw [if_neg hn]
    have h2 : ¬(state.favor + 1 > 20) := by omega
    have h3 : ¬(state.favor + 1 < -20) := by omega
    rw [if_neg h2, if_neg h3]
    omega

/-- Consecrating water into Holy Water on co-aligned altar with positive favor -/
def consecrateWater (waterBuc : BUC) (isCoaligned : Bool) (favor : Int) : BUC :=
  if isCoaligned ∧ favor > 5 then
    BUC.Blessed
  else
    waterBuc

/-- Theorem: Consecrating water with positive favor on co-aligned altar yields Blessed (Holy) water -/
theorem consecrate_water_yields_blessed (waterBuc : BUC) (favor : Int) (h : favor > 5) :
  consecrateWater waterBuc true favor = BUC.Blessed := by
  dsimp [consecrateWater]
  rw [if_pos ⟨rfl, h⟩]

end NetMechanics
