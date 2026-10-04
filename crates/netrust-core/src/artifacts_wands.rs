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

/// Recharges a wand with a Scroll of Charging.
///
/// C `read.c:729` `recharge`, wands at `read.c:737-794`. With `n` prior
/// recharges (capped at 7) the wand explodes iff
/// `n > 0 && (wishing || n*n*n > rn2(343))` (all BUC; the first recharge never
/// explodes). Otherwise the counter increments and:
/// - cursed: `stripspe` (charges become 0 if positive);
/// - else `n = (lim == 1) ? 1 : rn1(5, lim - 4)`, and uncursed takes `rnd(n)`;
///   `spe = max(spe + 1, n)`; a wishing wand left above 3 charges explodes
///   (`read.c:785`).
///
/// Rolls (all clamped, never panic):
/// - `roll_343`: `rn2(343)`, range `0..343`. C short-circuits, so the caller
///   should draw it only when `recharges > 0 && !is_wishing` (see
///   [`recharge_needs_explosion_roll`]); otherwise it is ignored.
/// - `rn5`: `rn2(5)` of `rn1(5, lim-4)`, range `0..=4` (ignored if `lim == 1`).
/// - `rnd_roll`: the uncursed `rnd(n)` draw, range `1..=n` (n = `lim-4+rn5`).
/// - `lim`: 1 wishing, 8 directional, 15 non-directional (clamped to 5..=15
///   unless 1).
pub fn recharge_wand(
    state: WandCharges,
    buc: Buc,
    lim: u32,
    is_wishing: bool,
    roll_343: u32,
    rn5: u32,
    rnd_roll: u32,
) -> RechargeResult {
    let n = state.recharges.min(7);
    if n > 0 && (is_wishing || n * n * n > roll_343.min(342)) {
        return RechargeResult::Exploded;
    }
    let recharges = state.recharges + 1;
    if buc == Buc::Cursed {
        return RechargeResult::Success(WandCharges {
            charges: 0,
            recharges,
        });
    }
    let amount = if lim <= 1 {
        1
    } else {
        let top = lim.clamp(5, 15) - 4 + rn5.min(4);
        if buc == Buc::Blessed {
            top
        } else {
            rnd_roll.clamp(1, top)
        }
    };
    let charges = (state.charges + 1).max(amount);
    if is_wishing && charges > 3 {
        return RechargeResult::Exploded;
    }
    RechargeResult::Success(WandCharges { charges, recharges })
}

/// Whether C draws `rn2(343)` for this recharge (`read.c:741`: only when
/// `n > 0` and the wand is not wishing).
pub fn recharge_needs_explosion_roll(recharges: u32, is_wishing: bool) -> bool {
    recharges > 0 && !is_wishing
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

    fn rc(
        w: WandCharges,
        buc: Buc,
        wishing: bool,
        r343: u32,
        rn5: u32,
        rnd: u32,
    ) -> RechargeResult {
        recharge_wand(w, buc, if wishing { 1 } else { 8 }, wishing, r343, rn5, rnd)
    }

    #[test]
    fn test_recharge_first_never_explodes() {
        for roll in [0, 1, 342] {
            assert!(matches!(
                rc(WandCharges::new(0), Buc::Blessed, false, roll, 0, 1),
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
            for buc in [Buc::Cursed, Buc::Uncursed, Buc::Blessed] {
                assert_eq!(rc(w, buc, false, roll, 0, 1), RechargeResult::Exploded);
            }
        }
    }

    #[test]
    fn test_recharge_n1_explodes_only_on_roll_0() {
        let w = WandCharges {
            charges: 2,
            recharges: 1,
        };
        assert_eq!(
            rc(w, Buc::Blessed, false, 0, 0, 1),
            RechargeResult::Exploded
        );
        assert!(matches!(
            rc(w, Buc::Blessed, false, 1, 0, 1),
            RechargeResult::Success(_)
        ));
    }

    #[test]
    fn test_recharge_wishing() {
        let w = WandCharges {
            charges: 0,
            recharges: 1,
        };
        assert_eq!(
            rc(w, Buc::Blessed, true, 342, 0, 1),
            RechargeResult::Exploded
        );
        assert_eq!(
            rc(WandCharges::new(0), Buc::Blessed, true, 0, 0, 1),
            RechargeResult::Success(WandCharges {
                charges: 1,
                recharges: 1
            })
        );
        // spe 3 -> max(4, 1) = 4 > 3 explodes.
        assert_eq!(
            rc(WandCharges::new(3), Buc::Blessed, true, 0, 0, 1),
            RechargeResult::Exploded
        );
    }

    #[test]
    fn test_recharge_cursed_strips_charges() {
        let w = WandCharges {
            charges: 6,
            recharges: 0,
        };
        assert_eq!(
            rc(w, Buc::Cursed, false, 0, 4, 8),
            RechargeResult::Success(WandCharges {
                charges: 0,
                recharges: 1
            })
        );
    }

    #[test]
    fn test_recharge_blessed_and_uncursed_ranges() {
        let w = WandCharges {
            charges: 0,
            recharges: 0,
        };
        // blessed directional: rn1(5,4) = 4 + rn5
        for rn5 in 0..=4 {
            assert_eq!(
                rc(w, Buc::Blessed, false, 0, rn5, 1),
                RechargeResult::Success(WandCharges {
                    charges: 4 + rn5,
                    recharges: 1
                })
            );
        }
        // uncursed: rnd(4+rn5), clamped to the range
        assert_eq!(
            rc(w, Buc::Uncursed, false, 0, 4, 99),
            RechargeResult::Success(WandCharges {
                charges: 8,
                recharges: 1
            })
        );
        assert_eq!(
            rc(w, Buc::Uncursed, false, 0, 4, 2),
            RechargeResult::Success(WandCharges {
                charges: 2,
                recharges: 1
            })
        );
        // max rule: spe 6, blessed roll 4 -> 7
        assert_eq!(
            rc(
                WandCharges {
                    charges: 6,
                    recharges: 0
                },
                Buc::Blessed,
                false,
                0,
                0,
                1
            ),
            RechargeResult::Success(WandCharges {
                charges: 7,
                recharges: 1
            })
        );
    }

    #[test]
    fn test_explosion_roll_needed_only_when_c_draws_it() {
        assert!(!recharge_needs_explosion_roll(0, false));
        assert!(!recharge_needs_explosion_roll(3, true));
        assert!(recharge_needs_explosion_roll(1, false));
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
