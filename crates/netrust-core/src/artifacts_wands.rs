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

/// Recharges a wand using a Scroll of Charging.
///
/// If recharges >= 3, the wand explodes!
/// Matches Lean theorem `recharge_safe_below_cap` and `recharge_explodes_at_cap`.
pub fn recharge_wand(w: WandCharges, added_charges: u32) -> RechargeResult {
    if w.recharges >= 3 {
        RechargeResult::Exploded
    } else {
        RechargeResult::Success(WandCharges {
            charges: w.charges + added_charges,
            recharges: w.recharges + 1,
        })
    }
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
    fn test_wand_recharge_and_explosion() {
        let mut w = WandCharges::new(0);
        for _ in 0..3 {
            let res = recharge_wand(w, 4);
            match res {
                RechargeResult::Success(next) => w = next,
                RechargeResult::Exploded => panic!("Should not explode yet"),
            }
        }
        assert_eq!(w.recharges, 3);
        assert_eq!(w.charges, 12);

        // 4th recharge exceeds limit -> explosion!
        assert_eq!(recharge_wand(w, 4), RechargeResult::Exploded);
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
