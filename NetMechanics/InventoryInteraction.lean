import NetMechanics.Basic
import NetMechanics.BUC

namespace NetMechanics

/-!
# Inventory Interactions: Dipping, Dilution, Magic Lamp, and Price Identification

Formalizes inventory management mechanics from NetHack (potion.c, lamp.c, shk.c):
1. Potion dilution state transitions towards water.
2. Magic lamp rubbing transitions: Djinni wishing exhaustion into an ordinary oil lamp.
3. Shopkeeper price identification invariants (shk.c `get_cost` / `set_cost`): the buy price is
   antitone in charisma, the sell price ignores charisma and BUC, and the buy-sell spread
   inequality (arbitrage prevention) holds with C rounding.
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

/-- Charisma buy adjustment `(multiplier, divisor)` (shk.c:2953-2964, `get_cost`):
    `>18`: 1/2; `18`: 2/3; `16-17`: 3/4; `11-15`: 1/1; `8-10`: 4/3; `6-7`: 3/2; `≤5`: 2/1. -/
def buyFactor (cha : Nat) : Nat × Nat :=
  if cha > 18 then (1, 2)
  else if cha = 18 then (2, 3)
  else if cha ≥ 16 then (3, 4)
  else if cha ≤ 5 then (2, 1)
  else if cha ≤ 7 then (3, 2)
  else if cha ≤ 10 then (4, 3)
  else (1, 1)

/-- Sell adjustment `(multiplier, divisor)` (shk.c:3148-3175, `set_cost`): 1/2, or 1/3 with
    the dunce/tourist surcharge; a lowballing shopkeeper takes a further 3/4.
    Charisma and BUC never enter the sell price. -/
def sellFactor (dunce lowball : Bool) : Nat × Nat :=
  let d := if dunce then 3 else 2
  if lowball then (3, d * 4) else (1, d)

/-- C rounding `((tmp * mult * 10 / div) + 5) / 10` (shk.c:2966-2974). -/
def roundMulDiv (tmp mult div : Nat) : Nat :=
  if div > 1 then (tmp * mult * 10 / div + 5) / 10 else tmp * mult

/-- Buy price (shk.c:2877-2988, `get_cost`): base 0 is priced 5; unidentified (`o_id % 4 = 0`)
    and dunce/tourist each ×4/3, then charisma; rounded, floored at 1; artifact ×4;
    angry shopkeeper `+ (p + 2) / 3`. -/
def calculateBuyPrice (base cha : Nat) (dunce unid artifact angry : Bool) : Nat :=
  let tmp := if base = 0 then 5 else base
  let m := (if unid then 4 else 1) * (if dunce then 4 else 1) * (buyFactor cha).1
  let d := (if unid then 3 else 1) * (if dunce then 3 else 1) * (buyFactor cha).2
  let p := max 1 (roundMulDiv tmp m d)
  let p := if artifact then p * 4 else p
  if angry then p + (p + 2) / 3 else p

/-- Sell price (shk.c:3148-3192, `set_cost`); the lowball applies only when `base > 1`;
    a zero base stays 0, otherwise floored at 1. -/
def calculateSellPrice (base : Nat) (dunce lowball : Bool) : Nat :=
  if base = 0 then 0
  else
    let f := sellFactor dunce (lowball && decide (base > 1))
    max 1 (roundMulDiv base f.1 f.2)

/-- `roundMulDiv` agrees with the C formula for every positive divisor. -/
theorem roundMulDiv_eq (tmp mult div : Nat) (hd : 0 < div) :
    roundMulDiv tmp mult div = (tmp * mult * 10 / div + 5) / 10 := by
  unfold roundMulDiv
  split
  · rfl
  · have h1 : div = 1 := by omega
    subst h1
    omega

/-- Rounded scaling is monotone in the ratio `m / d`. -/
theorem round_mono (x m1 d1 m2 d2 : Nat) (hd1 : 0 < d1) (hd2 : 0 < d2)
    (h : m1 * d2 ≤ m2 * d1) :
    (x * m1 * 10 / d1 + 5) / 10 ≤ (x * m2 * 10 / d2 + 5) / 10 := by
  apply Nat.div_le_div_right
  apply Nat.add_le_add_right
  apply (Nat.le_div_iff_mul_le hd2).mpr
  have hq : x * m1 * 10 / d1 * d1 ≤ x * m1 * 10 := Nat.div_mul_le_self _ _
  have hx : x * m1 * 10 * d2 ≤ x * m2 * 10 * d1 := by
    have := Nat.mul_le_mul_left (x * 10) h
    calc x * m1 * 10 * d2 = x * 10 * (m1 * d2) := by
            simp only [Nat.mul_comm, Nat.mul_left_comm, Nat.mul_assoc]
      _ ≤ x * 10 * (m2 * d1) := this
      _ = x * m2 * 10 * d1 := by
            simp only [Nat.mul_comm, Nat.mul_left_comm, Nat.mul_assoc]
  have h2 : x * m1 * 10 / d1 * d2 * d1 ≤ x * m2 * 10 * d1 := by
    calc x * m1 * 10 / d1 * d2 * d1 = x * m1 * 10 / d1 * d1 * d2 := by
            simp only [Nat.mul_comm, Nat.mul_left_comm, Nat.mul_assoc]
      _ ≤ x * m1 * 10 * d2 := Nat.mul_le_mul_right d2 hq
      _ ≤ x * m2 * 10 * d1 := hx
  exact Nat.le_of_mul_le_mul_right h2 hd1

/-- Every buy adjustment is at least 1/2: `2·m ≥ d` and `d > 0`. -/
theorem buy_ratio_ge_half (cha : Nat) (dunce unid : Bool) :
    0 < (if unid then 3 else 1) * (if dunce then 3 else 1) * (buyFactor cha).2 ∧
    1 * ((if unid then 3 else 1) * (if dunce then 3 else 1) * (buyFactor cha).2) ≤
      ((if unid then 4 else 1) * (if dunce then 4 else 1) * (buyFactor cha).1) * 2 := by
  unfold buyFactor
  cases unid <;> cases dunce <;> simp only [Bool.false_eq_true, ↓reduceIte] <;>
    repeat' (first | split | omega)

/-- Every sell adjustment is at most 1/2: `2·m ≤ d` and `d > 0`. -/
theorem sell_ratio_le_half (dunce lowball : Bool) :
    0 < (sellFactor dunce lowball).2 ∧
    (sellFactor dunce lowball).1 * 2 ≤ 1 * (sellFactor dunce lowball).2 := by
  unfold sellFactor
  cases dunce <;> cases lowball <;> decide

/-- Theorem: the shopkeeper never offers more than he charges (no arbitrage), for every
    charisma and every combination of surcharges (shk.c:2877 vs shk.c:3148). -/
theorem sell_le_buy_price (base cha : Nat) (dunce unid artifact angry lowball : Bool) :
    calculateSellPrice base dunce lowball ≤
      calculateBuyPrice base cha dunce unid artifact angry := by
  unfold calculateSellPrice
  split
  · exact Nat.zero_le _
  · rename_i hb
    -- half := round(base / 2) bounds sell from above and buy from below.
    have hsell := sell_ratio_le_half dunce (lowball && decide (base > 1))
    have hbuy := buy_ratio_ge_half cha dunce unid
    have hs : roundMulDiv base (sellFactor dunce (lowball && decide (base > 1))).1
        (sellFactor dunce (lowball && decide (base > 1))).2 ≤ (base * 1 * 10 / 2 + 5) / 10 := by
      rw [roundMulDiv_eq _ _ _ hsell.1]
      exact round_mono base _ _ 1 2 hsell.1 (by decide) hsell.2
    have hhalf_pos : 1 ≤ (base * 1 * 10 / 2 + 5) / 10 := by omega
    unfold calculateBuyPrice
    simp only [hb, ↓reduceIte]
    have hbu : (base * 1 * 10 / 2 + 5) / 10 ≤
        roundMulDiv base ((if unid then 4 else 1) * (if dunce then 4 else 1) * (buyFactor cha).1)
          ((if unid then 3 else 1) * (if dunce then 3 else 1) * (buyFactor cha).2) := by
      rw [roundMulDiv_eq _ _ _ hbuy.1]
      exact round_mono base 1 2 _ _ (by decide) hbuy.1 hbuy.2
    have hfin : ∀ p : Nat, (base * 1 * 10 / 2 + 5) / 10 ≤ p →
        max 1 (roundMulDiv base (sellFactor dunce (lowball && decide (base > 1))).1
          (sellFactor dunce (lowball && decide (base > 1))).2) ≤
        (if angry then (if artifact then p * 4 else p) + ((if artifact then p * 4 else p) + 2) / 3
          else (if artifact then p * 4 else p)) := by
      intro p hp
      cases artifact <;> cases angry <;> simp only [Bool.false_eq_true, ↓reduceIte] <;> omega
    exact hfin _ (Nat.le_trans hbu (Nat.le_max_right _ _))

/-- Lemma: a higher charisma never raises the charisma ratio (shk.c:2953-2964). -/
theorem buyFactor_antitone (cha : Nat) :
    (buyFactor (cha + 1)).1 * (buyFactor cha).2 ≤ (buyFactor cha).1 * (buyFactor (cha + 1)).2 := by
  unfold buyFactor
  repeat' (first | split | omega)

theorem buyFactor_div_pos (cha : Nat) : 0 < (buyFactor cha).2 := by
  unfold buyFactor
  repeat' (first | split | omega)

/-- Theorem: a higher charisma never raises the buy price, whatever the surcharges. -/
theorem buy_price_antitone_cha (base cha : Nat) (dunce unid artifact angry : Bool) :
    calculateBuyPrice base (cha + 1) dunce unid artifact angry ≤
      calculateBuyPrice base cha dunce unid artifact angry := by
  unfold calculateBuyPrice
  simp only
  generalize hu : (if unid then 4 else 1) * (if dunce then 4 else 1) = mu
  generalize hv : (if unid then 3 else 1) * (if dunce then 3 else 1) = dv
  have hdv : 0 < dv := by subst hv; cases unid <;> cases dunce <;> decide
  have hf := buyFactor_antitone cha
  have hp1 := buyFactor_div_pos cha
  have hp2 := buyFactor_div_pos (cha + 1)
  have hmono : roundMulDiv (if base = 0 then 5 else base) (mu * (buyFactor (cha + 1)).1)
      (dv * (buyFactor (cha + 1)).2) ≤
      roundMulDiv (if base = 0 then 5 else base) (mu * (buyFactor cha).1)
      (dv * (buyFactor cha).2) := by
    rw [roundMulDiv_eq _ _ _ (Nat.mul_pos hdv hp2), roundMulDiv_eq _ _ _ (Nat.mul_pos hdv hp1)]
    apply round_mono _ _ _ _ _ (Nat.mul_pos hdv hp2) (Nat.mul_pos hdv hp1)
    have := Nat.mul_le_mul_left (mu * dv) hf
    calc mu * (buyFactor (cha + 1)).1 * (dv * (buyFactor cha).2)
        = mu * dv * ((buyFactor (cha + 1)).1 * (buyFactor cha).2) := by
          simp only [Nat.mul_left_comm, Nat.mul_assoc]
      _ ≤ mu * dv * ((buyFactor cha).1 * (buyFactor (cha + 1)).2) := this
      _ = mu * (buyFactor cha).1 * (dv * (buyFactor (cha + 1)).2) := by
          simp only [Nat.mul_left_comm, Nat.mul_assoc]
  generalize roundMulDiv (if base = 0 then 5 else base) (mu * (buyFactor (cha + 1)).1)
      (dv * (buyFactor (cha + 1)).2) = x at hmono ⊢
  generalize roundMulDiv (if base = 0 then 5 else base) (mu * (buyFactor cha).1)
      (dv * (buyFactor cha).2) = y at hmono ⊢
  cases artifact <;> cases angry <;> simp only [Bool.false_eq_true, ↓reduceIte] <;> omega

end NetMechanics
