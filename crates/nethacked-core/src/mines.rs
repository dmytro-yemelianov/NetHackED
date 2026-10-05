//! Gnomish Mines, Minetown Temple Priest, and Mines' End Luckstone Mechanics.
//!
//! Formalized and verified in `NetMechanics.Mines`.
//! Models branch mechanics, divine AC protection donations, item uncursing,
//! and Luckstone positive luck preservation.

use nethacked_types::Buc;

/// Hard cap on priest-bought divine protection `u.ublessed` (C `priest.c:696`).
pub const MAX_DIVINE_PROTECTION: u32 = 20;

/// Below this value every purchase is a guaranteed +1 (C `priest.c:697`); from here on
/// each purchase succeeds only with probability `1 / ublessed`.
pub const PROTECTION_SOFT_CAP: u32 = 9;

/// Base donation the temple priest asks for (C `priest.c:637-638`):
/// `max(ulevelpeak, 1) * rn1(101, 150 + cheapskate * 40)`.
///
/// `rn2_101` is the `rn2(101)` of `rn1(101, ...)`, range `0..=100` (clamped).
/// `cheapskate` is the priest's `cheapskate_count`.
pub fn priest_suggested_donation(level_peak: u32, cheapskate: u32, rn2_101: u32) -> u32 {
    let per_level = 150u32
        .saturating_add(cheapskate.saturating_mul(40))
        .saturating_add(rn2_101.min(100));
    level_peak.max(1).saturating_mul(per_level)
}

/// Donation multiplier (C `priest.c:639-643`): `max(1, gold / (suggested * 3))`, where
/// `gold` is the hero's gold before the offer is handed over.
pub fn priest_donation_quan(gold: u32, suggested: u32) -> u32 {
    (gold / suggested.max(1).saturating_mul(3)).max(1)
}

/// What a donation of `offer` buys (C `priest.c:654-723`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DonationOutcome {
    /// `offer == 0`: "Thou shalt regret thine action!" (cheapskate count +1).
    Refused,
    /// `offer < suggested*quan` while keeping more than twice the offer: "Cheapskate." (+1).
    Cheapskate,
    /// `offer < suggested*quan` otherwise: thanks only.
    Thanks,
    /// `offer < 2*suggested*quan`: clairvoyance.
    Clairvoyance,
    /// `offer < 3*suggested*quan`: protection loop ([`protection_purchase_count`] steps).
    Protection,
    /// `offer >= 3*suggested*quan`: alignment / cleansing only, no protection.
    Selfless,
}

/// Classify a donation (C `priest.c:654-723`). `gold_after` is `money_cnt(invent)` after
/// the offer has been handed over (C `bribe` moves the gold first, `minion.c:385`).
pub fn priest_donation_outcome(
    offer: u32,
    suggested: u32,
    quan: u32,
    gold_after: u32,
) -> DonationOutcome {
    let offer = u64::from(offer);
    let band = u64::from(suggested.max(1)) * u64::from(quan.max(1));
    if offer == 0 {
        DonationOutcome::Refused
    } else if offer < band {
        if u64::from(gold_after) > offer * 2 {
            DonationOutcome::Cheapskate
        } else {
            DonationOutcome::Thanks
        }
    } else if offer < band * 2 {
        DonationOutcome::Clairvoyance
    } else if offer < band * 3 {
        DonationOutcome::Protection
    } else {
        DonationOutcome::Selfless
    }
}

/// Number of protection-loop iterations (C `priest.c:693`):
/// `for (; offer >= 2*suggested; offer -= 2*suggested)`, i.e. `offer / (2*suggested)`.
pub fn protection_purchase_count(offer: u32, suggested: u32) -> u32 {
    offer / suggested.max(1).saturating_mul(2)
}

/// The `rn2` bound C draws for one protection-loop iteration at `current` (C
/// `priest.c:694-698`): `rn2(3)` (of `rn1(3, 2)`) when `current == 0`, `rn2(current)` when
/// `PROTECTION_SOFT_CAP <= current < MAX_DIVINE_PROTECTION`, otherwise no draw.
pub fn protection_step_roll_bound(current: u32) -> Option<u32> {
    if current == 0 {
        Some(3)
    } else if (PROTECTION_SOFT_CAP..MAX_DIVINE_PROTECTION).contains(&current) {
        Some(current)
    } else {
        None
    }
}

/// One protection-loop iteration (C `priest.c:694-698`):
/// first purchase sets `rn1(3, 2)` = 2..4; below 9 always +1; from 9 to 19 +1 iff
/// `!rn2(ublessed)`; 20 is the hard cap.
///
/// `roll` is the draw named by [`protection_step_roll_bound`]: `rn2(3)` (range `0..=2`,
/// clamped) when `current == 0`, else `rn2(current)` (only `roll == 0` succeeds; ignored
/// below 9 and at the cap).
pub fn protection_purchase_step(current: u32, roll: u32) -> u32 {
    if current == 0 {
        2 + roll.min(2)
    } else if current < MAX_DIVINE_PROTECTION && (current < PROTECTION_SOFT_CAP || roll == 0) {
        current + 1
    } else {
        current
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
    fn test_priest_suggested_donation() {
        // priest.c:637: max(ulevelpeak,1) * rn1(101, 150 + cheapskate*40)
        assert_eq!(priest_suggested_donation(0, 0, 0), 150);
        assert_eq!(priest_suggested_donation(1, 0, 100), 250);
        assert_eq!(priest_suggested_donation(10, 2, 50), 10 * (150 + 80 + 50));
        assert_eq!(priest_suggested_donation(1, 0, 999), 250); // roll clamped to 0..=100
                                                               // priest.c:639-642: quan = max(1, gold / (3*suggested))
        assert_eq!(priest_donation_quan(0, 200), 1);
        assert_eq!(priest_donation_quan(599, 200), 1);
        assert_eq!(priest_donation_quan(1800, 200), 3);
    }

    #[test]
    fn test_priest_donation_bands() {
        use DonationOutcome::*;
        // suggested 200, quan 1 (priest.c:654-723)
        assert_eq!(priest_donation_outcome(0, 200, 1, 1000), Refused);
        assert_eq!(priest_donation_outcome(199, 200, 1, 1000), Cheapskate);
        assert_eq!(priest_donation_outcome(199, 200, 1, 398), Thanks);
        assert_eq!(priest_donation_outcome(200, 200, 1, 0), Clairvoyance);
        assert_eq!(priest_donation_outcome(399, 200, 1, 0), Clairvoyance);
        assert_eq!(priest_donation_outcome(400, 200, 1, 0), Protection);
        assert_eq!(priest_donation_outcome(599, 200, 1, 0), Protection);
        assert_eq!(priest_donation_outcome(600, 200, 1, 0), Selfless);
        // quan 3 scales every band
        assert_eq!(priest_donation_outcome(1199, 200, 3, 0), Clairvoyance);
        assert_eq!(priest_donation_outcome(1200, 200, 3, 0), Protection);
        assert_eq!(protection_purchase_count(1200, 200), 3);
        assert_eq!(protection_purchase_count(599, 200), 1);
    }

    #[test]
    fn test_protection_purchase_step() {
        // First purchase: rn1(3,2) = 2..4 (priest.c:694-695)
        assert_eq!(protection_step_roll_bound(0), Some(3));
        assert_eq!(protection_purchase_step(0, 0), 2);
        assert_eq!(protection_purchase_step(0, 2), 4);
        assert_eq!(protection_purchase_step(0, 99), 4); // clamped
                                                        // Below 9: guaranteed +1, no draw
        assert_eq!(protection_step_roll_bound(5), None);
        assert_eq!(protection_purchase_step(5, 7), 6);
        assert_eq!(protection_purchase_step(8, 3), 9);
        // 9..19: +1 iff !rn2(ublessed)
        assert_eq!(protection_step_roll_bound(9), Some(9));
        assert_eq!(protection_purchase_step(9, 0), 10);
        assert_eq!(protection_purchase_step(9, 1), 9);
        assert_eq!(protection_purchase_step(19, 0), 20);
        // Hard cap 20: no draw, no gain
        assert_eq!(protection_step_roll_bound(20), None);
        assert_eq!(protection_purchase_step(20, 0), 20);
        assert_eq!(MAX_DIVINE_PROTECTION, 20);
        assert_eq!(PROTECTION_SOFT_CAP, 9);
    }

    #[test]
    fn test_protection_sequence_bounded() {
        // Always-lucky rolls: 0 -> 4 -> ... -> 20 and never beyond.
        let mut prot = 0;
        let mut seen = Vec::new();
        for _ in 0..40 {
            prot = protection_purchase_step(prot, if prot == 0 { 2 } else { 0 });
            seen.push(prot);
        }
        assert_eq!(seen[0], 4);
        assert_eq!(seen[5], 9);
        assert_eq!(prot, MAX_DIVINE_PROTECTION);
        assert!(seen.windows(2).all(|w| w[0] <= w[1]));
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
