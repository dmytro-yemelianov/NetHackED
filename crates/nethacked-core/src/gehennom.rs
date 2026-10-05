//! Gehennom & Invocation Ritual Mechanics in Safe Rust.
//!
//! Modeled in Lean 4 (`NetMechanics.Gehennom`).

use crate::grid::Alignment;
use serde::{Deserialize, Serialize};

/// Number of candles required on the Candelabrum of Invocation.
pub const REQUIRED_CANDLES: u32 = 7;

/// State of the Candelabrum of Invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CandelabrumState {
    pub candle_count: u32,
    pub is_lit: bool,
}

impl CandelabrumState {
    pub fn empty() -> Self {
        Self {
            candle_count: 0,
            is_lit: false,
        }
    }

    pub fn full_and_lit() -> Self {
        Self {
            candle_count: REQUIRED_CANDLES,
            is_lit: true,
        }
    }

    pub fn attach_candle(&mut self) -> bool {
        if self.candle_count < REQUIRED_CANDLES {
            self.candle_count += 1;
            true
        } else {
            false
        }
    }

    pub fn light(&mut self) -> bool {
        if self.candle_count == REQUIRED_CANDLES {
            self.is_lit = true;
            true
        } else {
            false
        }
    }
}

/// Check if the Candelabrum is ready to perform the Invocation Ritual.
pub fn is_candelabrum_ready(c: &CandelabrumState) -> bool {
    c.candle_count == REQUIRED_CANDLES && c.is_lit
}

/// The three liturgical steps of the Invocation Ritual.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InvocationStep {
    RingBell,
    LightCandelabrum,
    ReadBook,
}

/// Sequence progression of the Invocation Ritual.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RitualProgress {
    #[default]
    Uninitiated,
    BellResounding,
    CandlesBurning,
    SanctumOpened,
}

/// Execute one step of the Invocation Ritual.
///
/// Matches the operational semantics verified in `NetMechanics.Gehennom.stepRitual`.
pub fn step_ritual(
    curr: RitualProgress,
    step: InvocationStep,
    on_vibrating_square: bool,
    candelabrum: &CandelabrumState,
) -> RitualProgress {
    match (curr, step) {
        (RitualProgress::Uninitiated, InvocationStep::RingBell) => RitualProgress::BellResounding,
        (RitualProgress::BellResounding, InvocationStep::LightCandelabrum) => {
            if is_candelabrum_ready(candelabrum) {
                RitualProgress::CandlesBurning
            } else {
                curr
            }
        }
        (RitualProgress::CandlesBurning, InvocationStep::ReadBook) => {
            if on_vibrating_square {
                RitualProgress::SanctumOpened
            } else {
                curr
            }
        }
        (RitualProgress::SanctumOpened, _) => RitualProgress::SanctumOpened,
        _ => curr,
    }
}

/// Returns true if the subterranean passage to Moloch's Sanctum is open.
pub fn is_sanctum_accessible(r: RitualProgress) -> bool {
    r == RitualProgress::SanctumOpened
}

/// Outcome of the Mysterious Force check (NetHack `goto_level`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MysteriousForceOutcome {
    /// The force did not trigger (or is disabled on this level).
    NoEffect,
    /// The force triggered but pushed 0 levels: hero is teleported on the same
    /// level (`do.c:1555-1556`, `1565-1568`).
    SameLevelTeleport,
    /// The force triggered and pushes the hero down to this depth.
    PushDown(usize),
}

/// The Mysterious Force: when ascending Gehennom carrying the Amulet of Yendor.
///
/// C reference: `do.c:1541-1573` (`goto_level`). Active only while
/// `dunlev < dunlevs_in_dungeon - 3` (`do.c:1542`); NetHackED's Gehennom uses
/// `depth == dunlev` and `bottom_depth == dunlevs_in_dungeon` (Sanctum, 6), so it
/// is disabled for `depth + 3 >= bottom_depth` (the bottom 4 levels).
/// Triggers iff `rn2(4 + mf) == 0` (`do.c:1543`); `odds = 3 + ualign.type`
/// (lawful 4, neutral 3, chaotic 2; unaligned <= 1 gives 0, `do.c:1544-1545`),
/// `diff = rn2(odds)`, push = `rnd(diff)` levels from the current level, clamped
/// to the dungeon bottom (`assign_rnd_level`, `do.c:1548`). A push of 0 is a
/// same-level teleport (`do.c:1555`).
///
/// Rolls are raw `u32` draws reduced modulo the C range (any value is valid):
/// `trigger_roll` -> `rn2(4+mf)`, `dist_roll_a` -> `rn2(odds)`,
/// `dist_roll_b` -> `rnd(diff)` (`dist_roll_b % diff + 1`).
pub fn mysterious_force(
    depth: usize,
    bottom_depth: usize,
    mf: u32,
    alignment: Alignment,
    trigger_roll: u32,
    dist_roll_a: u32,
    dist_roll_b: u32,
) -> MysteriousForceOutcome {
    if depth.saturating_add(3) >= bottom_depth {
        return MysteriousForceOutcome::NoEffect;
    }
    if trigger_roll % mf.saturating_add(4) != 0 {
        return MysteriousForceOutcome::NoEffect;
    }
    let odds: u32 = match alignment {
        Alignment::Lawful => 4,
        Alignment::Neutral => 3,
        Alignment::Chaotic => 2,
        Alignment::Unaligned => 0,
    };
    let diff = if odds <= 1 { 0 } else { dist_roll_a % odds };
    if diff == 0 {
        return MysteriousForceOutcome::SameLevelTeleport;
    }
    let push = ((dist_roll_b % diff) + 1) as usize;
    let push = push.min(bottom_depth - depth);
    if push == 0 {
        MysteriousForceOutcome::SameLevelTeleport
    } else {
        MysteriousForceOutcome::PushDown(depth + push)
    }
}

/// Amount added to the force's decay counter when it triggers:
/// `mysteryforce += rn2(diff + 2)` where `diff` is the actual levels descended
/// (`do.c:1563`). `roll` is a raw draw reduced modulo `diff + 2`.
pub fn mysterious_force_counter_increment(
    depth: usize,
    outcome: MysteriousForceOutcome,
    roll: u32,
) -> u32 {
    match outcome {
        MysteriousForceOutcome::NoEffect => 0,
        MysteriousForceOutcome::SameLevelTeleport => roll % 2,
        MysteriousForceOutcome::PushDown(to) => {
            roll % (to.saturating_sub(depth) as u32).saturating_add(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference written directly from C `do.c:1541-1573`.
    fn ref_mf(
        depth: usize,
        bottom: usize,
        mf: u32,
        align: Alignment,
        t: u32,
        a: u32,
        b: u32,
    ) -> MysteriousForceOutcome {
        use MysteriousForceOutcome::*;
        if depth + 3 >= bottom || t % (4 + mf) != 0 {
            return NoEffect;
        }
        let ty: i64 = match align {
            Alignment::Lawful => 1,
            Alignment::Neutral => 0,
            Alignment::Chaotic => -1,
            Alignment::Unaligned => -128,
        };
        let odds = 3 + ty;
        let mut diff = if odds <= 1 { 0 } else { (a as i64) % odds };
        if diff != 0 {
            let dest = (depth as i64 + (b as i64) % diff + 1).min(bottom as i64);
            diff = dest - depth as i64;
        }
        if diff == 0 {
            SameLevelTeleport
        } else {
            PushDown(depth + diff as usize)
        }
    }

    #[test]
    fn mf_trigger_odds_bucket() {
        for mf in 0..6u32 {
            let n = 4 + mf;
            let hits = (0..n * 100)
                .filter(|&t| {
                    mysterious_force(1, 10, mf, Alignment::Lawful, t, 1, 0)
                        != MysteriousForceOutcome::NoEffect
                })
                .count();
            assert_eq!(hits, 100, "exactly 1/{n} of rolls trigger");
        }
    }

    #[test]
    fn mf_push_max_by_alignment() {
        for (al, max) in [
            (Alignment::Lawful, 3usize),
            (Alignment::Neutral, 2),
            (Alignment::Chaotic, 1),
        ] {
            let mut best = 0;
            for a in 0..24 {
                for b in 0..24 {
                    if let MysteriousForceOutcome::PushDown(d) =
                        mysterious_force(1, 20, 0, al, 0, a, b)
                    {
                        best = best.max(d - 1);
                    }
                }
            }
            assert_eq!(best, max);
        }
    }

    #[test]
    fn mf_disabled_in_bottom_four_levels() {
        for depth in 3..=8 {
            for t in 0..8 {
                assert_eq!(
                    mysterious_force(depth, 6, 0, Alignment::Lawful, t * 4, 3, 2),
                    MysteriousForceOutcome::NoEffect
                );
            }
        }
        assert_ne!(
            mysterious_force(2, 6, 0, Alignment::Lawful, 0, 3, 2),
            MysteriousForceOutcome::NoEffect
        );
    }

    #[test]
    fn mf_zero_distance_is_same_level_teleport() {
        assert_eq!(
            mysterious_force(1, 6, 0, Alignment::Lawful, 0, 0, 5),
            MysteriousForceOutcome::SameLevelTeleport
        );
        assert_eq!(
            mysterious_force(1, 6, 0, Alignment::Unaligned, 0, 3, 5),
            MysteriousForceOutcome::SameLevelTeleport
        );
    }

    #[test]
    fn mf_matches_reference_exhaustive_small() {
        let aligns = [
            Alignment::Lawful,
            Alignment::Neutral,
            Alignment::Chaotic,
            Alignment::Unaligned,
        ];
        for depth in 0..12 {
            for bottom in 1..12 {
                for mf in 0..4 {
                    for &al in &aligns {
                        for t in 0..8 {
                            for a in 0..4 {
                                for b in 0..4 {
                                    assert_eq!(
                                        mysterious_force(depth, bottom, mf, al, t, a, b),
                                        ref_mf(depth, bottom, mf, al, t, a, b)
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn mf_counter_increment_range() {
        use MysteriousForceOutcome::*;
        assert_eq!(mysterious_force_counter_increment(1, NoEffect, 7), 0);
        for r in 0..20 {
            assert!(mysterious_force_counter_increment(1, PushDown(4), r) < 5);
            assert!(mysterious_force_counter_increment(1, SameLevelTeleport, r) < 2);
        }
    }

    #[test]
    fn test_candelabrum_lifecycle() {
        let mut cand = CandelabrumState::empty();
        assert!(!is_candelabrum_ready(&cand));

        for _ in 0..6 {
            assert!(cand.attach_candle());
        }
        assert_eq!(cand.candle_count, 6);
        assert!(!cand.light()); // Needs 7

        assert!(cand.attach_candle());
        assert_eq!(cand.candle_count, 7);
        assert!(!cand.attach_candle()); // Capped at 7

        assert!(cand.light());
        assert!(is_candelabrum_ready(&cand));
    }

    #[test]
    fn test_ritual_full_sequence_on_vibrating_square() {
        let cand = CandelabrumState::full_and_lit();

        let s0 = RitualProgress::Uninitiated;
        let s1 = step_ritual(s0, InvocationStep::RingBell, true, &cand);
        assert_eq!(s1, RitualProgress::BellResounding);

        let s2 = step_ritual(s1, InvocationStep::LightCandelabrum, true, &cand);
        assert_eq!(s2, RitualProgress::CandlesBurning);

        let s3 = step_ritual(s2, InvocationStep::ReadBook, true, &cand);
        assert_eq!(s3, RitualProgress::SanctumOpened);
        assert!(is_sanctum_accessible(s3));
    }

    #[test]
    fn test_reading_book_off_vibrating_square_fails() {
        let cand = CandelabrumState::full_and_lit();
        let s2 = RitualProgress::CandlesBurning;

        let s3 = step_ritual(s2, InvocationStep::ReadBook, false, &cand);
        assert_eq!(s3, RitualProgress::CandlesBurning);
        assert!(!is_sanctum_accessible(s3));
    }
}
