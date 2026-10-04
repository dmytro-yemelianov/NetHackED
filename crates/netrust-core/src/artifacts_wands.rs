//! Signature Artifacts, Wishing, and Wand Mechanics.
//!
//! Modeled in Lean 4 (`NetMechanics.ArtifactsWands`).
//! Models artifact combat bonuses, wand charge depletion, and recharging explosion limits.

use netrust_types::{ArtifactKind, Buc, RechargeResult, WandCharges};

/// Resolves bonus damage dealt by a signature artifact weapon.
///
/// Matches Lean theorem `resolveArtifactDamage`.
pub fn resolve_artifact_damage(
    art: ArtifactKind,
    base_damage: u32,
    is_demon_or_undead: bool,
) -> u32 {
    match art {
        ArtifactKind::Excalibur => {
            if is_demon_or_undead {
                base_damage + 10
            } else {
                base_damage + 5
            }
        }
        ArtifactKind::Mjollnir => base_damage + 12,
        ArtifactKind::VorpalBlade => base_damage + 6,
        ArtifactKind::Magicbane => base_damage + 4,
        ArtifactKind::EyeOfTheAethiopica => base_damage,
        ArtifactKind::TheTsurugiOfMuramasa => base_damage + 16,
        ArtifactKind::TheStaffOfAesculapius => base_damage + 8,
        ArtifactKind::TheOrbOfFate
        | ArtifactKind::TheHeartOfAhriman
        | ArtifactKind::TheMagicMirrorOfMerlin
        | ArtifactKind::TheEyesOfTheOverworld
        | ArtifactKind::TheMasterKeyOfThievery
        | ArtifactKind::ThePlatinumYendorianExpressCard
        | ArtifactKind::TheOrbOfDetection => base_damage + 5,
    }
}

/// Resolves Vorpal Blade decapitation instakill effect.
///
/// Returns (new_hp, is_dead). Matches Lean theorem `vorpal_decapitation_fatal`.
pub fn apply_vorpal_strike(hp: u32, decapitates: bool) -> (u32, bool) {
    if decapitates {
        (0, true)
    } else {
        (hp, hp == 0)
    }
}

/// Zapping a wand decrements charges by 1 if available.
///
/// Returns `None` if the wand is depleted (`charges == 0`).
/// Matches Lean theorem `wand_charge_depletes`.
pub fn zap_wand(w: WandCharges) -> Option<WandCharges> {
    if w.charges == 0 {
        None
    } else {
        Some(WandCharges {
            charges: w.charges - 1,
            recharges: w.recharges,
        })
    }
}

/// Recharges a wand with a Scroll of Charging (uncursed or blessed).
///
/// C `read.c:729` `recharge`, wands at `read.c:737-794`: with `n` prior
/// recharges the wand explodes iff `n > 0 && (wishing || n*n*n > rn2(7*7*7))`
/// (so the first recharge never explodes; `n >= 7` always does). Otherwise the
/// recharge counter increments and `spe = max(spe + 1, charge_roll)`, where
/// `charge_roll` is the caller's `n` from `read.c:760-766`: `rn1(5, lim-4)`
/// when blessed, `rnd()` of that when uncursed (`lim` 1 wishing / 8 directional
/// / 15 non-directional). A wand of wishing left with more than 3 charges
/// explodes (`read.c:785`).
///
/// - `roll_343`: the `rn2(343)` draw, range `0..343` (clamped).
/// - `charge_roll`: final charge count, range `1..=15` (clamped).
/// - `blessed` is informational here: the BUC formulas are applied by the
///   caller when drawing `charge_roll`.
pub fn recharge_wand(
    state: WandCharges,
    is_wishing: bool,
    _blessed: bool,
    roll_343: u32,
    charge_roll: u32,
) -> RechargeResult {
    let roll = roll_343.min(342);
    let n = state.recharges.min(7);
    if n > 0 && (is_wishing || n * n * n > roll) {
        return RechargeResult::Exploded;
    }
    let charges = (state.charges + 1).max(charge_roll.clamp(1, 15));
    if is_wishing && charges > 3 {
        return RechargeResult::Exploded;
    }
    RechargeResult::Success(WandCharges {
        charges,
        recharges: state.recharges + 1,
    })
}

/// Parses a wishing string into (item_name, enchantment, buc).
///
/// Handles tokens like "blessed", "cursed", "+2", "-1".
pub fn parse_wish(wish_str: &str) -> Option<(String, i8, Buc)> {
    let raw = wish_str.trim().to_lowercase();
    if raw.is_empty() {
        return None;
    }

    let mut buc = Buc::Uncursed;
    let mut enchantment: i8 = 0;
    let mut words = Vec::new();

    for token in raw.split_whitespace() {
        if token == "blessed" {
            buc = Buc::Blessed;
        } else if token == "cursed" {
            buc = Buc::Cursed;
        } else if token == "uncursed" {
            buc = Buc::Uncursed;
        } else if let Some(stripped) = token.strip_prefix('+') {
            if let Ok(val) = stripped.parse::<i8>() {
                enchantment = val;
            } else {
                words.push(token);
            }
        } else if let Some(stripped) = token.strip_prefix('-') {
            if let Ok(val) = stripped.parse::<i8>() {
                enchantment = -val;
            } else {
                words.push(token);
            }
        } else {
            words.push(token);
        }
    }

    if words.is_empty() {
        return None;
    }

    let item_name = words.join(" ");
    Some((item_name, enchantment, buc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_excalibur_damage_bonus() {
        assert_eq!(
            resolve_artifact_damage(ArtifactKind::Excalibur, 8, true),
            18
        );
        assert_eq!(
            resolve_artifact_damage(ArtifactKind::Excalibur, 8, false),
            13
        );
    }

    #[test]
    fn test_vorpal_decapitation() {
        let (hp, dead) = apply_vorpal_strike(50, true);
        assert_eq!(hp, 0);
        assert!(dead);

        let (hp2, dead2) = apply_vorpal_strike(50, false);
        assert_eq!(hp2, 50);
        assert!(!dead2);
    }

    #[test]
    fn test_wand_charge_depletion() {
        let w = WandCharges::new(3);
        let w1 = zap_wand(w).unwrap();
        assert_eq!(w1.charges, 2);

        let empty = WandCharges::new(0);
        assert_eq!(zap_wand(empty), None);
    }

    #[test]
    fn test_recharge_first_never_explodes() {
        let w = WandCharges::new(0);
        for roll in [0, 1, 342] {
            assert!(matches!(
                recharge_wand(w, false, true, roll, 5),
                RechargeResult::Success(_)
            ));
        }
    }

    #[test]
    fn test_recharge_n7_always_explodes() {
        let w = WandCharges {
            charges: 2,
            recharges: 7,
        };
        for roll in [0, 100, 342, 9999] {
            assert_eq!(
                recharge_wand(w, false, false, roll, 5),
                RechargeResult::Exploded
            );
        }
    }

    #[test]
    fn test_recharge_n1_explodes_only_on_roll_0() {
        let w = WandCharges {
            charges: 2,
            recharges: 1,
        };
        assert_eq!(
            recharge_wand(w, false, true, 0, 5),
            RechargeResult::Exploded
        );
        assert!(matches!(
            recharge_wand(w, false, true, 1, 5),
            RechargeResult::Success(_)
        ));
    }

    #[test]
    fn test_recharge_wishing_explodes_from_n1() {
        let w = WandCharges {
            charges: 0,
            recharges: 1,
        };
        assert_eq!(
            recharge_wand(w, true, true, 342, 1),
            RechargeResult::Exploded
        );
        // First recharge of an empty wishing wand: spe = max(0+1, 1) = 1.
        let ok = recharge_wand(WandCharges::new(0), true, true, 342, 1);
        assert_eq!(
            ok,
            RechargeResult::Success(WandCharges {
                charges: 1,
                recharges: 1
            })
        );
    }

    #[test]
    fn test_recharge_sets_max_of_roll_and_spe_plus_one() {
        let w = WandCharges {
            charges: 6,
            recharges: 0,
        };
        assert_eq!(
            recharge_wand(w, false, true, 0, 4),
            RechargeResult::Success(WandCharges {
                charges: 7,
                recharges: 1
            })
        );
        assert_eq!(
            recharge_wand(w, false, true, 0, 8),
            RechargeResult::Success(WandCharges {
                charges: 8,
                recharges: 1
            })
        );
    }

    #[test]
    fn test_parse_wish() {
        let (name, ench, buc) = parse_wish("blessed +2 silver dragon scale mail").unwrap();
        assert_eq!(name, "silver dragon scale mail");
        assert_eq!(ench, 2);
        assert_eq!(buc, Buc::Blessed);

        let (name2, ench2, buc2) = parse_wish("wand of death").unwrap();
        assert_eq!(name2, "wand of death");
        assert_eq!(ench2, 0);
        assert_eq!(buc2, Buc::Uncursed);
    }
}
