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

/-- Recharging a wand with a Scroll of Charging (read.c:737-794).
    `roll343` is `rn2(343)`; `chargeRoll` is the caller's `n` (read.c:760-766).
    Explodes iff `n > 0 ∧ (wishing ∨ n³ > roll343)` with `n` prior recharges
    (capped at 7); a wishing wand left above 3 charges also explodes. -/
def rechargeWand (w : WandCharges) (isWishing : Bool) (roll343 chargeRoll : Nat) :
    RechargeResult :=
  let n := min w.recharges 7
  if n > 0 ∧ (isWishing = true ∨ n * n * n > roll343) then
    RechargeResult.Exploded
  else if isWishing = true ∧ max (w.charges + 1) chargeRoll > 3 then
    RechargeResult.Exploded
  else
    RechargeResult.Success {
      charges := max (w.charges + 1) chargeRoll,
      recharges := w.recharges + 1
    }

/-- Theorem: the first recharge of a non-wishing wand never explodes. -/
theorem recharge_safe_first (w : WandCharges) (roll343 chargeRoll : Nat)
    (h : w.recharges = 0) :
    ∃ w', rechargeWand w false roll343 chargeRoll = RechargeResult.Success w'
      ∧ w'.charges = max (w.charges + 1) chargeRoll := by
  subst_vars
  simp [rechargeWand, h]

/-- Theorem: a non-wishing wand explodes iff `n > 0 ∧ n³ > roll` (n = recharges capped at 7). -/
theorem recharge_explodes_iff (w : WandCharges) (roll343 chargeRoll : Nat) :
    rechargeWand w false roll343 chargeRoll = RechargeResult.Exploded ↔
      (0 < min w.recharges 7 ∧
        min w.recharges 7 * min w.recharges 7 * min w.recharges 7 > roll343) := by
  unfold rechargeWand
  simp only [Bool.false_eq_true, false_or, false_and, if_false]
  split <;> simp_all

/-- Theorem: with 7 or more prior recharges the wand always explodes for every
    `rn2(343)` outcome (n³ = 343 > roll). -/
theorem recharge_explodes_at_cap (w : WandCharges) (chargeRoll roll343 : Nat)
    (h : w.recharges ≥ 7) (hr : roll343 < 343) :
    rechargeWand w false roll343 chargeRoll = RechargeResult.Exploded := by
  rw [recharge_explodes_iff]
  have hm : min w.recharges 7 = 7 := by omega
  rw [hm]
  omega

/-- Theorem: a wishing wand explodes on any re-recharge. -/
theorem recharge_wishing_explodes (w : WandCharges) (roll343 chargeRoll : Nat)
    (h : 0 < w.recharges) :
    rechargeWand w true roll343 chargeRoll = RechargeResult.Exploded := by
  have hm : 0 < min w.recharges 7 := by omega
  simp [rechargeWand, hm]

end NetMechanics
