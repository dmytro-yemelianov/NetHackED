//! Gehennom & Invocation Ritual Mechanics in Safe Rust.
//!
//! Formally verified in `NetMechanics.Gehennom`.

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

/// The Mysterious Force: When ascending Gehennom with the real Amulet of Yendor,
/// an eldritch force has a 1/3 chance of pushing the hero down by 1 to 3 levels.
pub fn calculate_mysterious_force(current_depth: usize, roll: u32) -> Option<usize> {
    if roll % 3 == 0 {
        let pushback = ((roll / 3) % 3 + 1) as usize;
        Some(current_depth.saturating_add(pushback))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
