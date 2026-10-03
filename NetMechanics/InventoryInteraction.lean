import NetMechanics.Basic
import NetMechanics.BUC

namespace NetMechanics

/-!
# Inventory Interactions: Dipping, Dilution, Magic Lamp, and Price Identification

Formalizes inventory management mechanics from NetHack (potion.c, lamp.c, shk.c):
1. Potion dilution state transitions towards water.
2. Magic lamp rubbing transitions: Djinni wishing exhaustion into an ordinary oil lamp.
3. Shopkeeper price identification invariants: buy/sell pricing monotonicity with respect to charisma,
   and the buy-sell spread inequality (arbitrage prevention).
-/

-- ============================================================================
-- Section 1: Potion Dilution
-- ============================================================================

/-- Potion tier representation: Tier 0 = basic potion, Tier (n+1) = concentrated (e.g. Extra Healing),
    Water = fully diluted base liquid. -/
inductive DilutionState where
  | Potion (tier : Nat)
  | Water
  deriving DecidableEq, Repr

/-- Dipping a potion into plain water dilutes it by one tier, eventually becoming Water. -/
def dilutePotion (p : DilutionState) : DilutionState :=
  match p with
  | DilutionState.Potion 0 => DilutionState.Water
  | DilutionState.Potion (n + 1) => DilutionState.Potion n
  | DilutionState.Water => DilutionState.Water

/-- Theorem: Water dilution is idempotent (dipping water into water remains water). -/
theorem dilute_water_idempotent :
  dilutePotion DilutionState.Water = DilutionState.Water := rfl

/-- Iterated dilution function. -/
def diluteN : Nat → DilutionState → DilutionState
  | 0, p => p
  | n + 1, p => diluteN n (dilutePotion p)

/-- Theorem: Iterating dilution on Water remains Water. -/
theorem diluteN_water (k : Nat) : diluteN k DilutionState.Water = DilutionState.Water := by
  induction k with
  | zero => rfl
  | succ n ih =>
      show diluteN n (dilutePotion DilutionState.Water) = DilutionState.Water
      rw [dilute_water_idempotent]
      exact ih

/-- Lemma: Diluting Tier n after one step gives Tier (n-1) or Water. -/
theorem diluteN_step (k : Nat) (p : DilutionState) :
  diluteN (k + 1) p = diluteN k (dilutePotion p) := rfl

/-- Theorem: Diluting a potion tier n exactly (n + 1) times produces Water. -/
theorem dilute_potion_reaches_water (n : Nat) :
  diluteN (n + 1) (DilutionState.Potion n) = DilutionState.Water := by
  induction n with
  | zero =>
      show diluteN 0 (dilutePotion (DilutionState.Potion 0)) = DilutionState.Water
      rfl
  | succ k ih =>
      show diluteN (k + 1 + 1) (DilutionState.Potion (k + 1)) = DilutionState.Water
      rw [diluteN_step]
      show diluteN (k + 1) (DilutionState.Potion k) = DilutionState.Water
      exact ih

/-- Theorem: Repeated dilution of any potion tier eventually yields Water. -/
theorem dilute_eventually_water (p : DilutionState) : ∃ k : Nat, diluteN k p = DilutionState.Water := by
  match p with
  | DilutionState.Water => exact ⟨0, rfl⟩
  | DilutionState.Potion n => exact ⟨n + 1, dilute_potion_reaches_water n⟩

-- ============================================================================
-- Section 2: Magic Lamp Rubbing & Djinni State Machine
-- ============================================================================

/-- Lamp classification: Magic lamp (with or without Djinni) or ordinary Oil lamp. -/
inductive LampType where
  | MagicLamp (hasDjinni : Bool)
  | OilLamp (oilTurns : Nat)
  deriving DecidableEq, Repr

/-- Possible outcomes when rubbing a lamp. -/
inductive RubResult where
  | WishGranted
  | HostileDjinni
  | PeacefulDjinni
  | Smoke
  | Nothing
  deriving DecidableEq, Repr

/-- Rubbing a lamp produces an outcome and transitions the lamp state.
    A magic lamp with a Djinni that grants a wish or releases a Djinni permanently
    transforms into an ordinary OilLamp (canonical NetHack lamp.c). -/
def rubLamp (lamp : LampType) (buc : BUC) : LampType × RubResult :=
  match lamp with
  | LampType.MagicLamp true =>
      match buc with
      | BUC.Blessed => (LampType.OilLamp 1500, RubResult.WishGranted)
      | BUC.Uncursed => (LampType.OilLamp 1000, RubResult.PeacefulDjinni)
      | BUC.Cursed => (LampType.OilLamp 500, RubResult.HostileDjinni)
  | LampType.MagicLamp false => (LampType.OilLamp 1000, RubResult.Smoke)
  | LampType.OilLamp turns =>
      if turns > 0 then (LampType.OilLamp turns, RubResult.Smoke)
      else (LampType.OilLamp 0, RubResult.Nothing)

/-- Theorem: Rubbing a Magic Lamp with a Djinni always consumes the Djinni and transforms into an OilLamp. -/
theorem rub_magic_lamp_exhausts_djinni (buc : BUC) :
  ∃ turns r, rubLamp (LampType.MagicLamp true) buc = (LampType.OilLamp turns, r) := by
  cases buc
  · exact ⟨1500, RubResult.WishGranted, rfl⟩
  · exact ⟨1000, RubResult.PeacefulDjinni, rfl⟩
  · exact ⟨500, RubResult.HostileDjinni, rfl⟩

/-- Theorem: Ordinary oil lamps cannot grant wishes. -/
theorem rub_oil_lamp_never_wishes (turns : Nat) (buc : BUC) :
  (rubLamp (LampType.OilLamp turns) buc).2 ≠ RubResult.WishGranted := by
  intro h
  unfold rubLamp at h
  by_cases ht : turns > 0
  · simp [ht] at h
  · simp [ht] at h

-- ============================================================================
-- Section 3: Shopkeeper Price Identification Invariants
-- ============================================================================

/-- Charisma discount / markup factor:
    In NetHack (shk.c), charisma determines price modifiers.
    Buy multiplier in basis 12:
    Cha <= 5: 24 (200%)
    Cha 6..14: 16 (133%)
    Cha 15..17: 12 (100%)
    Cha >= 18: 9 (75%)
-/
def buyFactor (cha : Nat) : Nat :=
  if cha ≤ 5 then 24
  else if cha ≤ 14 then 16
  else if cha ≤ 17 then 12
  else 9

/-- Sell factor (fraction shopkeeper pays the player) in basis 12:
    Cha <= 5: 3 (25%)
    Cha 6..10: 4 (33%)
    Cha 11..17: 6 (50%)
    Cha >= 18: 8 (66%)
-/
def sellFactor (cha : Nat) : Nat :=
  if cha ≤ 5 then 3
  else if cha ≤ 10 then 4
  else if cha ≤ 17 then 6
  else 8

/-- Price calculation: buy price = (base * buyFactor) / 12, with cursed surcharge (+33%) -/
def calculateBuyPrice (base : Nat) (cha : Nat) (buc : BUC) : Nat :=
  let raw := (base * buyFactor cha) / 12
  match buc with
  | BUC.Cursed => (raw * 4) / 3
  | _ => raw

/-- Sell price = (base * sellFactor) / 12, with cursed penalty (-33%) -/
def calculateSellPrice (base : Nat) (cha : Nat) (buc : BUC) : Nat :=
  let raw := (base * sellFactor cha) / 12
  match buc with
  | BUC.Cursed => (raw * 2) / 3
  | _ => raw

/-- Lemma: sellFactor is always strictly less than buyFactor for all charisma values. -/
theorem sellFactor_lt_buyFactor (cha : Nat) : sellFactor cha < buyFactor cha := by
  unfold sellFactor buyFactor
  by_cases h5 : cha ≤ 5
  · simp [h5]
  · by_cases h10 : cha ≤ 10
    · have h14 : cha ≤ 14 := by omega
      simp [h5, h10, h14]
    · by_cases h14 : cha ≤ 14
      · have h17 : cha ≤ 17 := by omega
        simp [h5, h10, h14, h17]
      · by_cases h17 : cha ≤ 17
        · simp [h5, h10, h14, h17]
        · simp [h5, h10, h14, h17]

/-- Lemma: sellFactor <= buyFactor. -/
theorem sellFactor_le_buyFactor (cha : Nat) : sellFactor cha ≤ buyFactor cha :=
  Nat.le_of_lt (sellFactor_lt_buyFactor cha)

/-- Theorem: Shopkeeper buy price is always greater than or equal to sell price (no arbitrage). -/
theorem sell_le_buy_price (base : Nat) (cha : Nat) (buc : BUC) :
  calculateSellPrice base cha buc ≤ calculateBuyPrice base cha buc := by
  unfold calculateSellPrice calculateBuyPrice
  have hfac : sellFactor cha ≤ buyFactor cha := sellFactor_le_buyFactor cha
  have hmul : base * sellFactor cha ≤ base * buyFactor cha := Nat.mul_le_mul_left base hfac
  have hdiv : (base * sellFactor cha) / 12 ≤ (base * buyFactor cha) / 12 := Nat.div_le_div_right hmul
  cases buc
  · -- Blessed
    exact hdiv
  · -- Uncursed
    exact hdiv
  · -- Cursed
    have hmul24 : (base * sellFactor cha) / 12 * 2 ≤ (base * buyFactor cha) / 12 * 4 := by omega
    exact Nat.div_le_div_right hmul24

end NetMechanics
