//! Monster abilities, dragon breath weapons, gaze attacks, and monster spellcasting.
//!
//! Formally defined and verified in Lean 4 (`NetMechanics.MonsterAbilities`).

use netrust_types::{BreathType, GazeEffect, GazeType, Intrinsics};

/// Resolves incoming breath damage against defender's reflection and elemental resistances.
/// Returns `(actual_damage, was_reflected)`.
pub fn resolve_breath_damage(
    raw_damage: u32,
    breath: BreathType,
    intrinsics: &Intrinsics,
) -> (u32, bool) {
    if intrinsics.reflection {
        (0, true)
    } else {
        let has_res = match breath {
            BreathType::Fire => intrinsics.fire_resistance,
            BreathType::Cold => intrinsics.cold_resistance,
            BreathType::Shock => intrinsics.shock_resistance,
            BreathType::Poison => intrinsics.poison_resistance,
            BreathType::Disintegration => intrinsics.disintegration_resistance,
            BreathType::Sleep => intrinsics.sleep_resistance,
        };
        if has_res {
            (0, false)
        } else {
            (raw_damage, false)
        }
    }
}

/// Resolves gaze attacks (e.g. Medusa, Floating Eye).
pub fn resolve_gaze(
    gaze: GazeType,
    has_reflection: bool,
    is_blind: bool,
) -> GazeEffect {
    if has_reflection {
        GazeEffect::ReflectedToAttacker
    } else if is_blind {
        GazeEffect::BlindImmune
    } else {
        GazeEffect::Afflicted(gaze)
    }
}

/// Calculates bounded number of monsters a summon spell can spawn without exceeding level cap.
pub fn calculate_summon_count(
    current_monsters: usize,
    max_capacity: usize,
    desired_summons: usize,
) -> usize {
    if current_monsters >= max_capacity {
        0
    } else {
        desired_summons.min(max_capacity - current_monsters)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_breath_damage_resolution() {
        let none = Intrinsics::empty();
        assert_eq!(resolve_breath_damage(25, BreathType::Fire, &none), (25, false));

        let fire_res = Intrinsics::empty().with_fire_resistance();
        assert_eq!(resolve_breath_damage(25, BreathType::Fire, &fire_res), (0, false));
        assert_eq!(resolve_breath_damage(25, BreathType::Cold, &fire_res), (25, false));

        let reflect = Intrinsics::empty().with_reflection();
        assert_eq!(resolve_breath_damage(25, BreathType::Fire, &reflect), (0, true));
        assert_eq!(resolve_breath_damage(25, BreathType::Cold, &reflect), (0, true));
    }

    #[test]
    fn test_gaze_resolution() {
        // Unprotected: afflicted
        assert_eq!(resolve_gaze(GazeType::Petrification, false, false), GazeEffect::Afflicted(GazeType::Petrification));
        // Blindfold: immune
        assert_eq!(resolve_gaze(GazeType::Paralysis, false, true), GazeEffect::BlindImmune);
        // Reflection / Mirror: bounces back to attacker
        assert_eq!(resolve_gaze(GazeType::Petrification, true, false), GazeEffect::ReflectedToAttacker);
        assert_eq!(resolve_gaze(GazeType::Petrification, true, true), GazeEffect::ReflectedToAttacker);
    }

    #[test]
    fn test_summon_bounds() {
        assert_eq!(calculate_summon_count(10, 15, 3), 3);
        assert_eq!(calculate_summon_count(13, 15, 4), 2);
        assert_eq!(calculate_summon_count(15, 15, 4), 0);
        assert_eq!(calculate_summon_count(20, 15, 4), 0);
    }
}
