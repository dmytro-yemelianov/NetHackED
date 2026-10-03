//! Castle Drawbridge and Astral Plane Ascension Mechanics.
//!
//! Formally verified in `NetMechanics.Endgame`.
//! Models drawbridge raising/lowering, crushing damage, and the High Altar ascension theorem.

use netrust_types::{Alignment, AscensionOutcome, DrawbridgeState, DrawbridgeTransition};

/// Toggles drawbridge state between Open (lowered) and Closed (raised portcullis).
///
/// Matches Lean theorem `raise_crushes_occupant_fatal`.
pub fn toggle_drawbridge(
    current: DrawbridgeState,
    has_occupant: bool,
) -> (DrawbridgeState, DrawbridgeTransition) {
    match current {
        DrawbridgeState::Closed => (DrawbridgeState::Open, DrawbridgeTransition::Lowered),
        DrawbridgeState::Open => {
            let crushed_damage = if has_occupant { 9999 } else { 0 };
            (
                DrawbridgeState::Closed,
                DrawbridgeTransition::Raised { crushed_damage },
            )
        }
        DrawbridgeState::Destroyed => (
            DrawbridgeState::Destroyed,
            DrawbridgeTransition::DestroyedAndFellInMoat,
        ),
    }
}

/// Shatters drawbridge (e.g., via Wand of Striking or destruction effect), collapsing it into the moat.
pub fn destroy_drawbridge() -> (DrawbridgeState, DrawbridgeTransition) {
    (
        DrawbridgeState::Destroyed,
        DrawbridgeTransition::DestroyedAndFellInMoat,
    )
}

/// Offers the Amulet of Yendor on a High Altar on the Astral Plane.
///
/// Matches Lean theorem `ascension_iff_aligned`.
pub fn offer_amulet_on_high_altar(
    has_real_amulet: bool,
    hero_align: Alignment,
    altar_align: Alignment,
) -> AscensionOutcome {
    if !has_real_amulet {
        AscensionOutcome::Rejected("An imitation Amulet cannot grant immortality!".into())
    } else if hero_align == altar_align {
        AscensionOutcome::Ascended(altar_align)
    } else {
        AscensionOutcome::Rejected(
            "The deity furiously rejects your cross-aligned offering!".into(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drawbridge_lowered_is_open() {
        let (state, trans) = toggle_drawbridge(DrawbridgeState::Closed, false);
        assert_eq!(state, DrawbridgeState::Open);
        assert_eq!(trans, DrawbridgeTransition::Lowered);
    }

    #[test]
    fn test_drawbridge_raised_with_occupant_crushes() {
        let (state, trans) = toggle_drawbridge(DrawbridgeState::Open, true);
        assert_eq!(state, DrawbridgeState::Closed);
        assert_eq!(trans, DrawbridgeTransition::Raised { crushed_damage: 9999 });
    }

    #[test]
    fn test_drawbridge_raised_empty_no_damage() {
        let (state, trans) = toggle_drawbridge(DrawbridgeState::Open, false);
        assert_eq!(state, DrawbridgeState::Closed);
        assert_eq!(trans, DrawbridgeTransition::Raised { crushed_damage: 0 });
    }

    #[test]
    fn test_ascension_matching_alignment_succeeds() {
        let outcome = offer_amulet_on_high_altar(true, Alignment::Lawful, Alignment::Lawful);
        assert_eq!(outcome, AscensionOutcome::Ascended(Alignment::Lawful));
    }

    #[test]
    fn test_ascension_cross_aligned_rejected() {
        let outcome = offer_amulet_on_high_altar(true, Alignment::Lawful, Alignment::Chaotic);
        assert!(matches!(outcome, AscensionOutcome::Rejected(_)));
    }

    #[test]
    fn test_ascension_fake_amulet_rejected() {
        let outcome = offer_amulet_on_high_altar(false, Alignment::Neutral, Alignment::Neutral);
        assert!(matches!(outcome, AscensionOutcome::Rejected(_)));
    }
}
