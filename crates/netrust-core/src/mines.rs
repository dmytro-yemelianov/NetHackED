//! Gnomish Mines, Minetown Temple Priest, and Mines' End Luckstone Mechanics.
//!
//! Formalized and verified in `NetMechanics.Mines`.
//! Models branch mechanics, divine AC protection donations, item uncursing,
//! and Luckstone positive luck preservation.

use netrust_types::Buc;
use serde::{Deserialize, Serialize};

/// Maximum divine AC protection purchasable from a temple priest (+9 AC bonus).
pub const MAX_DIVINE_PROTECTION: u32 = 9;

/// Minimum gold donation per character level to attempt divine AC protection purchase.
pub fn protection_donation_cost(hero_level: u32) -> u32 {
    400 * hero_level.max(1)
}

/// Calculate resulting divine AC protection after a gold donation to a temple priest.
///
/// Proven in the Lean 4 model to be monotonic and bounded by `MAX_DIVINE_PROTECTION`.
pub fn apply_priest_donation(current_prot: u32, donation_amount: u32, hero_level: u32) -> u32 {
    if current_prot >= MAX_DIVINE_PROTECTION {
        current_prot
    } else if donation_amount >= protection_donation_cost(hero_level) {
        current_prot + 1
    } else {
        current_prot
    }
}

/// Priest purifies a BUC item when uncursing service is triggered.
///
/// Proven in the Lean 4 model (`priest_uncurse_never_cursed`) that the resulting item is never Cursed.
pub fn priest_uncurse(item_buc: Buc) -> Buc {
    if item_buc == Buc::Cursed {
        Buc::Uncursed
    } else {
        item_buc
    }
}

/// Clamp raw luck to canonical NetHack limits [-10, 10].
pub fn clamp_luck(luck: i32) -> i32 {
    luck.clamp(-10, 10)
}

/// Status of a carried luckstone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LuckstoneStatus {
    None,
    Blessed,
    Uncursed,
    Cursed,
}

/// Single luck decay step (normally fires every 600 turns in NetHack).
///
/// Proven in the Lean 4 model (`luckstone_preserves_positive_luck`):
/// Carrying an uncursed or blessed luckstone guarantees positive luck NEVER decays downward.
pub fn step_luck_decay(raw_luck: i32, stone: LuckstoneStatus) -> i32 {
    match stone {
        LuckstoneStatus::Blessed | LuckstoneStatus::Uncursed => {
            // Non-cursed luckstone: positive luck NEVER decays; negative luck recovers toward 0!
            if raw_luck > 0 {
                raw_luck
            } else if raw_luck < 0 {
                raw_luck + 1
            } else {
                0
            }
        }
        LuckstoneStatus::Cursed => {
            // Cursed luckstone: positive luck decays; negative luck NEVER recovers!
            if raw_luck > 0 {
                raw_luck - 1
            } else if raw_luck < 0 {
                raw_luck
            } else {
                0
            }
        }
        LuckstoneStatus::None => {
            // No luckstone: natural decay toward 0 from both directions
            if raw_luck > 0 {
                raw_luck - 1
            } else if raw_luck < 0 {
                raw_luck + 1
            } else {
                0
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priest_donation_protection_cap() {
        let mut prot = 0;
        let level = 1;
        let cost = protection_donation_cost(level);

        for _ in 0..15 {
            prot = apply_priest_donation(prot, cost, level);
        }
        assert_eq!(prot, MAX_DIVINE_PROTECTION);
    }

    #[test]
    fn test_insufficient_donation_no_protection() {
        let prot = apply_priest_donation(3, 100, 5);
        assert_eq!(prot, 3);
    }

    #[test]
    fn test_priest_uncurse() {
        assert_eq!(priest_uncurse(Buc::Cursed), Buc::Uncursed);
        assert_eq!(priest_uncurse(Buc::Uncursed), Buc::Uncursed);
        assert_eq!(priest_uncurse(Buc::Blessed), Buc::Blessed);
    }

    #[test]
    fn test_luckstone_preservation_and_recovery() {
        // Blessed / Uncursed preserves good luck
        assert_eq!(step_luck_decay(5, LuckstoneStatus::Blessed), 5);
        assert_eq!(step_luck_decay(5, LuckstoneStatus::Uncursed), 5);

        // Blessed / Uncursed recovers bad luck
        assert_eq!(step_luck_decay(-4, LuckstoneStatus::Blessed), -3);

        // Without luckstone, good luck decays
        assert_eq!(step_luck_decay(5, LuckstoneStatus::None), 4);

        // Cursed luckstone traps bad luck and drains good luck
        assert_eq!(step_luck_decay(5, LuckstoneStatus::Cursed), 4);
        assert_eq!(step_luck_decay(-4, LuckstoneStatus::Cursed), -4);
    }
}
