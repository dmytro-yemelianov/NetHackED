//! Gnomish Mines, Minetown Temple Priest, and Mines' End Luckstone Mechanics.
//!
//! Formalized and verified in `NetMechanics.Mines`.
//! Models branch mechanics, divine AC protection donations, item uncursing,
//! and Luckstone positive luck preservation.

use netrust_types::Buc;

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

/// Luck timeout period in turns (C `timeout.c:595-620`, `nh_timeout`):
/// `moves % ((u.uhave.amulet || u.ugangr) ? 300 : 600) == 0`.
pub fn luck_decay_period(has_amulet: bool, god_angry: bool) -> u64 {
    if has_amulet || god_angry {
        300
    } else {
        600
    }
}

/// Single luck timeout step toward `base_luck` (C `timeout.c:595-620`, `attrib.c:423`
/// `stone_luck`). `stone` is the BUC of the carried luckstone, if any.
///
/// - No stone: luck moves one step toward `base_luck` from either side.
/// - Blessed (`time_luck > 0`): only luck below base recovers.
/// - Uncursed (`time_luck == 0`, stone present): luck is frozen.
/// - Cursed (`time_luck < 0`): only luck above base decays.
///
/// The caller is responsible for the period gate ([`luck_decay_period`]).
pub fn step_luck_decay(luck: i32, base_luck: i32, stone: Option<Buc>) -> i32 {
    let can_decay = matches!(stone, None | Some(Buc::Cursed));
    let can_recover = matches!(stone, None | Some(Buc::Blessed));
    if luck > base_luck && can_decay {
        luck - 1
    } else if luck < base_luck && can_recover {
        luck + 1
    } else {
        luck
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
    fn test_luck_timeout_table() {
        use Buc::*;
        // (luck, base, stone, expected)
        let table = [
            (5, 0, None, 4),
            (-4, 0, None, -3),
            (0, 0, None, 0),
            (5, 0, Some(Blessed), 5),
            (-4, 0, Some(Blessed), -3),
            (5, 0, Some(Uncursed), 5),
            (-4, 0, Some(Uncursed), -4),
            (5, 0, Some(Cursed), 4),
            (-4, 0, Some(Cursed), -4),
            (1, 1, None, 1),
            (3, 1, None, 2),
            (-1, 1, None, 0),
        ];
        for (l, b, st, want) in table {
            assert_eq!(step_luck_decay(l, b, st), want, "{l} {b} {st:?}");
        }
    }

    #[test]
    fn test_luck_decay_period() {
        assert_eq!(luck_decay_period(false, false), 600);
        assert_eq!(luck_decay_period(true, false), 300);
        assert_eq!(luck_decay_period(false, true), 300);
        assert_eq!(luck_decay_period(true, true), 300);
    }
}
