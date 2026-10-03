//! Nutrition and hunger clock state machine.
//!
//! Formally verified in `NetMechanics.Nutrition`.

use serde::{Deserialize, Serialize};

/// Canonical NetHack hunger states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum HungerState {
    Starved,
    Fainting,
    Weak,
    Hungry,
    Normal,
    Satiated,
}

/// Convert discrete nutrition points into HungerState.
pub fn hunger_of_nutrition(n: u32) -> HungerState {
    if n > 1000 {
        HungerState::Satiated
    } else if n >= 150 {
        HungerState::Normal
    } else if n >= 50 {
        HungerState::Hungry
    } else if n >= 1 {
        HungerState::Weak
    } else {
        HungerState::Fainting
    }
}

/// Numeric ranking of hunger state severity (higher = better nourished).
pub fn hunger_tier(state: HungerState) -> u32 {
    match state {
        HungerState::Starved => 0,
        HungerState::Fainting => 1,
        HungerState::Weak => 2,
        HungerState::Hungry => 3,
        HungerState::Normal => 4,
        HungerState::Satiated => 5,
    }
}

/// Passive metabolic consumption per turn.
pub fn metabolic_tick(n: u32) -> u32 {
    n.saturating_sub(1)
}

pub fn is_corpse_tainted(age: u32, rot_threshold: u32) -> bool {
    age > rot_threshold
}

pub fn is_cannibalism(corpse_race: &str, hero_race: &str) -> bool {
    corpse_race == hero_race
}

pub fn intrinsic_from_corpse(monster_name: &str) -> Option<String> {
    match monster_name.to_lowercase().as_str() {
        "red dragon" | "fire giant" | "fire elemental" => Some("fire_resistance".to_string()),
        "floating eye" => Some("telepathy".to_string()),
        "winter wolf" | "white dragon" | "frost giant" => Some("cold_resistance".to_string()),
        "energy vortex" | "blue dragon" => Some("shock_resistance".to_string()),
        "green dragon" | "killer bee" => Some("poison_resistance".to_string()),
        "gelatinous cube" => Some("sleep_resistance".to_string()),
        "stalker" => Some("see_invisible".to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hunger_transitions() {
        assert_eq!(hunger_of_nutrition(0), HungerState::Fainting);
        assert_eq!(hunger_of_nutrition(25), HungerState::Weak);
        assert_eq!(hunger_of_nutrition(100), HungerState::Hungry);
        assert_eq!(hunger_of_nutrition(500), HungerState::Normal);
        assert_eq!(hunger_of_nutrition(1200), HungerState::Satiated);
    }

    #[test]
    fn test_metabolic_decay() {
        assert_eq!(metabolic_tick(10), 9);
        assert_eq!(metabolic_tick(0), 0);
    }
}
