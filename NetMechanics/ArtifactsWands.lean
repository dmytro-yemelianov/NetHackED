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

/-- Recharging a wand: fails/explodes if over-recharged (> 3 times) -/
def rechargeWand (w : WandCharges) (addedCharges : Nat) : RechargeResult :=
  if w.recharges ≥ 3 then
    RechargeResult.Exploded
  else
    RechargeResult.Success {
      charges := w.charges + addedCharges,
      recharges := w.recharges + 1
    }

/-- Theorem: Recharging under limit increases charges without explosion -/
theorem recharge_safe_below_cap (w : WandCharges) (add : Nat) (h : w.recharges < 3) :
  ∃ w', rechargeWand w add = RechargeResult.Success w' ∧ w'.charges = w.charges + add := by
  dsimp [rechargeWand]
  have h_not : ¬(w.recharges ≥ 3) := by omega
  rw [if_neg h_not]
  refine ⟨{ charges := w.charges + add, recharges := w.recharges + 1 }, ⟨rfl, rfl⟩⟩

/-- Theorem: Over-recharging (> 3) triggers explosion -/
theorem recharge_explodes_at_cap (w : WandCharges) (add : Nat) (h : w.recharges ≥ 3) :
  rechargeWand w add = RechargeResult.Exploded := by
  dsimp [rechargeWand]
  rw [if_pos h]

end NetMechanics
