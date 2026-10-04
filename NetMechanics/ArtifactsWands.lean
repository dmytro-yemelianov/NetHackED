/-
  NetMechanics.ArtifactsWands: Formalized Signature Artifacts, Wishing, and Wand Mechanics.
  Models artifact combat bonuses (Excalibur, Vorpal Blade, Mjollnir),
  wand charge depletion, and recharging explosion bounds.
-/

import NetMechanics.Combat
import NetMechanics.BUC

namespace NetMechanics

/-- Canonical NetHack signature named artifacts -/
inductive ArtifactKind where
  | Excalibur
  | VorpalBlade
  | Mjollnir
  | Magicbane
  | EyeOfTheAethiopica
deriving Repr, DecidableEq

/-- Combat resolution incorporating artifact effects -/
def resolveArtifactDamage (art : ArtifactKind) (baseDamage : Nat) (isDemonOrUndead : Bool) : Nat :=
  match art with
  | ArtifactKind.Excalibur =>
    if isDemonOrUndead then baseDamage + 10 else baseDamage + 5
  | ArtifactKind.Mjollnir =>
    baseDamage + 12
  | ArtifactKind.VorpalBlade =>
    baseDamage + 6
  | ArtifactKind.Magicbane =>
    baseDamage + 4
  | ArtifactKind.EyeOfTheAethiopica =>
    baseDamage

/-- Vorpal Blade decapitation effect: instant kill -/
def applyVorpalStrike (target : Combatant) (decapitates : Bool) : Combatant :=
  if decapitates then
    { target with hp := 0, isDead := true }
  else
    target

/-- Theorem: Vorpal decapitation is strictly fatal -/
theorem vorpal_decapitation_fatal (target : Combatant) :
  (applyVorpalStrike target true).isDead = true ∧ (applyVorpalStrike target true).hp = 0 := by
  dsimp [applyVorpalStrike]
  exact ⟨rfl, rfl⟩

/-- Wand charge state: (remainingCharges, timesRecharged) -/
structure WandCharges where
  charges : Nat
  recharges : Nat
deriving Repr, DecidableEq

/-- Zapping a wand decrements charges by 1 if available -/
def zapWand (w : WandCharges) : Option WandCharges :=
  if w.charges == 0 then
    none
  else
    some { w with charges := w.charges - 1 }

/-- Theorem: Zapping non-empty wand strictly decrements charge -/
theorem wand_charge_depletes (w : WandCharges) (h : w.charges > 0) :
  ∃ w', zapWand w = some w' ∧ w'.charges = w.charges - 1 := by
  dsimp [zapWand]
  have h_ne : ¬(w.charges == 0) = true := by
    intro h_eq
    have h0 : w.charges = 0 := of_decide_eq_true h_eq
    omega
  rw [if_neg h_ne]
  refine ⟨{ charges := w.charges - 1, recharges := w.recharges }, ⟨rfl, rfl⟩⟩

/-- Theorem: Empty wand cannot zap -/
theorem empty_wand_cannot_zap (w : WandCharges) (h : w.charges = 0) :
  zapWand w = none := by
  dsimp [zapWand]
  have h_eq : (w.charges == 0) = true := by
    rw [h]
    rfl
  rw [if_pos h_eq]

/-- Wand recharging outcome -/
inductive RechargeResult where
  | Success (newCharges : WandCharges)
  | Exploded
deriving Repr, DecidableEq

/-- Scroll BUC for recharging: 0 cursed, 1 uncursed, 2 blessed. -/
inductive ChargeBuc where
  | Cursed
  | Uncursed
  | Blessed
deriving Repr, DecidableEq

/-- Recharging a wand with a Scroll of Charging (read.c:737-794).
    `roll343` is `rn2(343)`, `rn5` the `rn2(5)` of `rn1(5, lim-4)`, `rndRoll`
    the uncursed `rnd(n)` draw (clamped to `1..n`). The explosion check applies
    to every BUC: `n > 0 ∧ (wishing ∨ n³ > roll343)` with `n` prior recharges
    capped at 7. Cursed then strips charges; otherwise `spe = max (spe+1) amt`
    and a wishing wand above 3 charges explodes. -/
def rechargeWand (w : WandCharges) (buc : ChargeBuc) (lim : Nat) (isWishing : Bool)
    (roll343 rn5 rndRoll : Nat) : RechargeResult :=
  let n := min w.recharges 7
  if n > 0 ∧ (isWishing = true ∨ n * n * n > roll343) then
    RechargeResult.Exploded
  else if buc = ChargeBuc.Cursed then
    RechargeResult.Success { charges := 0, recharges := w.recharges + 1 }
  else
    let top := if lim ≤ 1 then 1 else (min (max lim 5) 15) - 4 + min rn5 4
    let amt := if lim ≤ 1 ∨ buc = ChargeBuc.Blessed then top else max 1 (min rndRoll top)
    let spe := max (w.charges + 1) amt
    if isWishing = true ∧ spe > 3 then
      RechargeResult.Exploded
    else
      RechargeResult.Success { charges := spe, recharges := w.recharges + 1 }

/-- Theorem: the first recharge of a non-wishing wand never explodes. -/
theorem recharge_safe_first (w : WandCharges) (buc : ChargeBuc) (lim roll343 rn5 rndRoll : Nat)
    (h : w.recharges = 0) :
    ∃ w', rechargeWand w buc lim false roll343 rn5 rndRoll = RechargeResult.Success w' := by
  unfold rechargeWand
  simp only [h]
  split <;> simp_all
  all_goals split <;> simp_all

/-- Theorem: the explosion check is BUC-independent: a non-wishing wand's recharge
    explodes (for cursed, or for any BUC since non-wishing never explodes later)
    iff `n > 0 ∧ n³ > roll` (n = recharges capped at 7). -/
theorem recharge_explodes_iff (w : WandCharges) (buc : ChargeBuc) (lim roll343 rn5 rndRoll : Nat) :
    rechargeWand w buc lim false roll343 rn5 rndRoll = RechargeResult.Exploded ↔
      (0 < min w.recharges 7 ∧
        min w.recharges 7 * min w.recharges 7 * min w.recharges 7 > roll343) := by
  unfold rechargeWand
  simp only [Bool.false_eq_true, false_or, false_and, if_false]
  split
  · simp_all
  · split <;> simp_all <;> split <;> simp_all

/-- Theorem: with 7 or more prior recharges the wand always explodes for every
    `rn2(343)` outcome (n³ = 343 > roll), whatever the BUC. -/
theorem recharge_explodes_at_cap (w : WandCharges) (buc : ChargeBuc)
    (lim chargeRoll roll343 rndRoll : Nat) (h : w.recharges ≥ 7) (hr : roll343 < 343) :
    rechargeWand w buc lim false roll343 chargeRoll rndRoll = RechargeResult.Exploded := by
  rw [recharge_explodes_iff]
  have hm : min w.recharges 7 = 7 := by omega
  rw [hm]
  omega

/-- Theorem: a wishing wand explodes on any re-recharge (any BUC). -/
theorem recharge_wishing_explodes (w : WandCharges) (buc : ChargeBuc)
    (lim roll343 rn5 rndRoll : Nat) (h : 0 < w.recharges) :
    rechargeWand w buc lim true roll343 rn5 rndRoll = RechargeResult.Exploded := by
  have hm : 0 < min w.recharges 7 := by omega
  simp [rechargeWand, hm]

/-- Theorem: a cursed, surviving recharge leaves the wand with 0 charges. -/
theorem recharge_cursed_strips (w : WandCharges) (lim roll343 rn5 rndRoll : Nat) (isW : Bool)
    (h : rechargeWand w ChargeBuc.Cursed lim isW roll343 rn5 rndRoll ≠ RechargeResult.Exploded) :
    rechargeWand w ChargeBuc.Cursed lim isW roll343 rn5 rndRoll =
      RechargeResult.Success { charges := 0, recharges := w.recharges + 1 } := by
  unfold rechargeWand at h ⊢
  by_cases hc : (0 < min w.recharges 7 ∧ (isW = true ∨ min w.recharges 7 * min w.recharges 7 * min w.recharges 7 > roll343))
  · simp [hc] at h
  · simp [hc]

/-- Theorem: a blessed non-wishing directional recharge (lim 8) that survives yields
    `max (spe+1) (4 + rn5)` charges (`rn1(5,4)`, rn5 ≤ 4). -/
theorem recharge_blessed_amount (w : WandCharges) (roll343 rn5 rndRoll : Nat) (h5 : rn5 ≤ 4)
    (h0 : w.recharges = 0) :
    rechargeWand w ChargeBuc.Blessed 8 false roll343 rn5 rndRoll =
      RechargeResult.Success { charges := max (w.charges + 1) (4 + rn5), recharges := 1 } := by
  unfold rechargeWand
  have : min rn5 4 = rn5 := by omega
  simp [h0, this]

end NetMechanics
