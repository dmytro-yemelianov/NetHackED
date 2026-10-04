//! Dynamic lighting, darkness, raycasting, blindness, and telepathy.
//!
//! Formally verified in `NetMechanics.Lighting`.

use serde::{Deserialize, Serialize};

/// Standard light emission radii in NetHack.
pub const OIL_LAMP_RADIUS: u32 = 3;
pub const LANTERN_RADIUS: u32 = 2;
pub const CANDLE_RADIUS: u32 = 1;
pub const DEFAULT_LAMP_FUEL: u32 = 1000;

/// Representation of a mobile or stationary light source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LightSource {
    pub radius: u32,
    pub fuel: u32,
    pub is_lit: bool,
}

impl LightSource {
    pub fn new_oil_lamp(fuel: u32) -> Self {
        Self {
            radius: OIL_LAMP_RADIUS,
            fuel,
            is_lit: false,
        }
    }

    pub fn new_magic_lamp() -> Self {
        Self {
            radius: OIL_LAMP_RADIUS,
            fuel: u32::MAX,
            is_lit: false,
        }
    }

    pub fn new_candle(fuel: u32) -> Self {
        Self {
            radius: CANDLE_RADIUS,
            fuel,
            is_lit: false,
        }
    }

    /// Check if the light source is actively illuminating.
    pub fn is_active(&self) -> bool {
        self.is_lit && self.fuel > 0
    }

    /// Effective luminescence radius.
    pub fn effective_radius(&self) -> u32 {
        if self.is_active() {
            self.radius
        } else {
            0
        }
    }

    /// Toggle light source lit state.
    pub fn toggle(&mut self) -> bool {
        if self.fuel > 0 {
            self.is_lit = !self.is_lit;
        } else {
            self.is_lit = false;
        }
        self.is_lit
    }
}

/// Decrement fuel by 1 tick, saturating at 0.
pub fn tick_light_fuel(fuel: u32) -> u32 {
    fuel.saturating_sub(1)
}

/// Determine whether a tile is visible to the hero.
///
/// - Blind heroes cannot see any tiles visually.
/// - In illuminated rooms, tiles within FOV are visible.
/// - In dark rooms/caverns, tiles are visible only if illuminated by a light source or felt in melee (distance <= 1).
pub fn can_see_tile(blind: bool, dist: u32, is_dark: bool, is_illuminated: bool) -> bool {
    if blind {
        false
    } else if !is_dark {
        true
    } else {
        is_illuminated || dist <= 1
    }
}

/// Determine whether a monster is detectable by the hero.
///
/// - Visible directly if the tile is visible.
/// - Sensed telepathically if hero has telepathy and monster has a mind, even through blindness/darkness.
pub fn can_detect_monster(_blind: bool, telepathy: bool, has_mind: bool, tile_visible: bool) -> bool {
    if tile_visible {
        true
    } else {
        // Telepathy works regardless of blindness, so no separate `blind` branch is needed.
        telepathy && has_mind
    }
}

/// Check if a monster has a conscious mind detectable by telepathy.
/// Mindless creatures in NetHack (undead, golems, non-thinking entities) cannot be sensed via telepathy.
pub fn monster_has_mind(species_name: &str) -> bool {
    let lower = species_name.to_lowercase();
    let mindless = lower.contains("skeleton")
        || lower.contains("zombie")
        || lower.contains("golem")
        || lower.contains("lich")
        || lower.contains("ghost")
        || lower.contains("shade");
    !mindless
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_light_source_lifecycle() {
        let mut lamp = LightSource::new_oil_lamp(10);
        assert!(!lamp.is_active());
        assert_eq!(lamp.effective_radius(), 0);

        lamp.toggle();
        assert!(lamp.is_active());
        assert_eq!(lamp.effective_radius(), OIL_LAMP_RADIUS);

        for _ in 0..10 {
            lamp.fuel = tick_light_fuel(lamp.fuel);
        }
        assert_eq!(lamp.fuel, 0);
        assert!(!lamp.is_active());
        assert_eq!(lamp.effective_radius(), 0);
    }

    #[test]
    fn test_tile_visibility_and_darkness() {
        // Blind hero
        assert!(!can_see_tile(true, 1, false, false));
        assert!(!can_see_tile(true, 5, true, true));

        // Normal room
        assert!(can_see_tile(false, 5, false, false));

        // Dark room without light
        assert!(can_see_tile(false, 1, true, false)); // melee touch
        assert!(!can_see_tile(false, 2, true, false)); // obscured

        // Dark room with light
        assert!(can_see_tile(false, 3, true, true));
    }

    #[test]
    fn test_monster_telepathy() {
        // Blind hero with telepathy detecting conscious monsters
        assert!(can_detect_monster(true, true, true, false));
        // Blind hero with telepathy on mindless monster
        assert!(!can_detect_monster(true, true, false, false));
        // Blind hero without telepathy
        assert!(!can_detect_monster(true, false, true, false));
        // Sighted hero can see visible monsters regardless
        assert!(can_detect_monster(false, false, false, true));
    }
}
