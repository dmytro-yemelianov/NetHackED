//! Religion, Divine Favor, Altar Sacrifices, and Water Consecration.
//!
//! Formally verified in `NetMechanics.Religion`.
//! Models piety calculus, prayer cooldowns, altar conversions, and holy water consecration.

use netrust_types::{Alignment, Buc, DivineState, SacrificeResult};

/// Clamps divine favor within canonical NetHack bounds [-20, 20].
pub const fn clamp_favor(f: i32) -> i32 {
    if f > 20 {
        20
    } else if f < -20 {
        -20
    } else {
        f
    }
}

/// Decrements prayer cooldown timer by 1 turn.
pub const fn tick_prayer_timeout(timeout: u32) -> u32 {
    timeout.saturating_sub(1)
}

/// Resolves the sacrifice of a corpse on an altar.
///
/// Matches Lean theorem `sacrifice_coaligned_increases_favor`.
pub fn resolve_sacrifice(
    state: DivineState,
    hero_align: Alignment,
    altar_align: Alignment,
    corpse_nutrition: u32,
) -> (DivineState, SacrificeResult) {
    if hero_align != altar_align {
        // Cross-aligned altar converts to hero's alignment
        let new_state = DivineState {
            favor: clamp_favor(state.favor + 2),
            prayer_timeout: state.prayer_timeout,
            gift_count: state.gift_count,
        };
        (new_state, SacrificeResult::AltarConverted(hero_align))
    } else {
        let favor_gain = if corpse_nutrition >= 200 { 3 } else { 1 };
        let new_favor = clamp_favor(state.favor + favor_gain);
        if new_favor >= 15 && state.gift_count == 0 {
            let new_state = DivineState {
                favor: new_favor,
                prayer_timeout: state.prayer_timeout,
                gift_count: 1,
            };
            (new_state, SacrificeResult::DivineGift("Excalibur".into()))
        } else {
            let new_state = DivineState {
                favor: new_favor,
                prayer_timeout: state.prayer_timeout,
                gift_count: state.gift_count,
            };
            (new_state, SacrificeResult::FavorIncreased(new_favor))
        }
    }
}

/// Consecrates water into Holy Water on a co-aligned altar with positive divine favor (> 5).
///
/// Matches Lean theorem `consecrate_water_yields_blessed`.
pub fn consecrate_water(water_buc: Buc, is_coaligned: bool, favor: i32) -> Buc {
    if is_coaligned && favor > 5 {
        Buc::Blessed
    } else {
        water_buc
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clamp_favor() {
        assert_eq!(clamp_favor(30), 20);
        assert_eq!(clamp_favor(-30), -20);
        assert_eq!(clamp_favor(12), 12);
    }

    #[test]
    fn test_tick_prayer_timeout() {
        assert_eq!(tick_prayer_timeout(5), 4);
        assert_eq!(tick_prayer_timeout(0), 0);
    }

    #[test]
    fn test_sacrifice_cross_aligned_converts() {
        let state = DivineState::default();
        let (new_state, res) = resolve_sacrifice(state, Alignment::Lawful, Alignment::Chaotic, 250);
        assert_eq!(res, SacrificeResult::AltarConverted(Alignment::Lawful));
        assert_eq!(new_state.favor, 7);
    }

    #[test]
    fn test_sacrifice_coaligned_increases_favor() {
        let state = DivineState::default();
        let (new_state, res) =
            resolve_sacrifice(state, Alignment::Neutral, Alignment::Neutral, 250);
        assert_eq!(res, SacrificeResult::FavorIncreased(8));
        assert_eq!(new_state.favor, 8);
    }

    #[test]
    fn test_sacrifice_divine_gift_at_high_piety() {
        let state = DivineState {
            favor: 14,
            prayer_timeout: 0,
            gift_count: 0,
        };
        let (new_state, res) = resolve_sacrifice(state, Alignment::Lawful, Alignment::Lawful, 300);
        assert_eq!(res, SacrificeResult::DivineGift("Excalibur".into()));
        assert_eq!(new_state.gift_count, 1);
        assert_eq!(new_state.favor, 17);
    }

    #[test]
    fn test_consecrate_water_blesses_at_high_favor() {
        assert_eq!(consecrate_water(Buc::Uncursed, true, 8), Buc::Blessed);
        assert_eq!(consecrate_water(Buc::Uncursed, false, 8), Buc::Uncursed);
        assert_eq!(consecrate_water(Buc::Uncursed, true, 2), Buc::Uncursed);
    }
}
