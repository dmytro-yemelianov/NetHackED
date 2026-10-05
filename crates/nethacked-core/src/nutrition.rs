//! Nutrition and hunger clock state machine.
//!
//! Modeled in Lean 4 (`NetMechanics.Nutrition`).

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

/// Convert signed nutrition points into a [`HungerState`].
///
/// C reference `eat.c:3362` (`newuhs`): Satiated `h > 1000`, Normal `h > 150`,
/// Hungry `h > 50`, Weak `h > 0`, otherwise Fainting; `eat.c:3437`: the hero
/// starves when `h < -(100 + 10 * ACURR(A_CON))`. `con` is the hero's
/// Constitution (`ACURR(A_CON)`); the formula is evaluated in 64-bit arithmetic.
pub fn hunger_of_nutrition(nutrition: i32, con: i32) -> HungerState {
    let h = i64::from(nutrition);
    if h > 1000 {
        HungerState::Satiated
    } else if h > 150 {
        HungerState::Normal
    } else if h > 50 {
        HungerState::Hungry
    } else if h > 0 {
        HungerState::Weak
    } else if h < -(100 + 10 * i64::from(con)) {
        HungerState::Starved
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

/// Passive metabolic consumption per turn (`eat.c` `gethungry`: `u.uhunger--`).
///
/// Nutrition goes negative (fainting/starving range), so this is signed.
pub fn metabolic_tick(n: i32) -> i32 {
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

    /// Reference written directly from `eat.c:3362` / `eat.c:3437`.
    fn c_ref(h: i32, con: i32) -> HungerState {
        let h = h as i64;
        let con = con as i64;
        if h > 1000 {
            HungerState::Satiated
        } else if h > 150 {
            HungerState::Normal
        } else if h > 50 {
            HungerState::Hungry
        } else if h > 0 {
            HungerState::Weak
        } else if h < -(100 + 10 * con) {
            HungerState::Starved
        } else {
            HungerState::Fainting
        }
    }

    #[test]
    fn test_hunger_boundaries() {
        use HungerState::*;
        let con = 10;
        let cases = [
            (1001, Satiated),
            (1000, Normal),
            (151, Normal),
            (150, Hungry),
            (51, Hungry),
            (50, Weak),
            (1, Weak),
            (0, Fainting),
            (-(100 + 10 * con), Fainting),
            (-(101 + 10 * con), Starved),
        ];
        for (n, want) in cases {
            assert_eq!(hunger_of_nutrition(n, con), want, "n={n}");
            assert_eq!(c_ref(n, con), want, "ref n={n}");
        }
    }

    #[test]
    fn test_starved_threshold_depends_on_con() {
        assert_eq!(hunger_of_nutrition(-150, 5), HungerState::Fainting);
        assert_eq!(hunger_of_nutrition(-151, 5), HungerState::Starved);
        assert_eq!(hunger_of_nutrition(-250, 15), HungerState::Fainting);
        assert_eq!(hunger_of_nutrition(-251, 15), HungerState::Starved);
    }

    #[test]
    fn test_metabolic_decay() {
        assert_eq!(metabolic_tick(10), 9);
        assert_eq!(metabolic_tick(0), -1);
        assert_eq!(metabolic_tick(i32::MIN), i32::MIN);
    }
}
