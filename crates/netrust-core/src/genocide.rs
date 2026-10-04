use netrust_types::{GenocideRegistry, GenocideTarget};

/// Checks if a given monster name or its glyph class has been genocided.
pub fn is_genocided(registry: &GenocideRegistry, monster_name: &str, glyph: char) -> bool {
    registry.genocided_species.contains(monster_name) || registry.genocided_classes.contains(&glyph)
}

/// Applies a genocide target to a registry.
pub fn apply_genocide(registry: &mut GenocideRegistry, target: GenocideTarget) {
    match target {
        GenocideTarget::Species(name) => {
            registry.genocided_species.insert(name);
        }
        GenocideTarget::Class(glyph) => {
            registry.genocided_classes.insert(glyph);
        }
    }
}

/// Calculates the number of monsters to summon when reading a cursed scroll of genocide.
pub fn cursed_genocide_summon_count() -> usize {
    4 // According to the instructions: "summon 4 hostiles adjacent to hero."
}

/// Attempts to write with a magic marker.
/// Reduces the ink by `cost` if sufficient, returns error otherwise.
// TODO(fidelity): unit error type kept for API stability; see review
#[allow(clippy::result_unit_err)]
pub fn write_with_marker(current_ink: u8, cost: u8) -> Result<u8, ()> {
    if current_ink >= cost {
        Ok(current_ink - cost)
    } else {
        Err(())
    }
}
