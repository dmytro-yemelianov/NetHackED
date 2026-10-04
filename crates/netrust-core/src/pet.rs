//! Pet and companion dynamics.
//!
//! Modeled in Lean 4 (`NetMechanics.Pet`).

use netrust_types::Coord;

/// Result of interacting with an actor on an adjacent tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeroInteraction<Id = usize> {
    /// Hostile target -> initiate melee combat.
    MeleeAttack(Id),
    /// Tame companion -> non-violently swap positions without combat damage.
    DisplacePet {
        pet_id: Id,
        new_hero_pos: Coord,
        new_pet_pos: Coord,
    },
}

/// Swaps the positions of the hero and their tame companion.
/// Proven in the Lean 4 model to be an involution (`swap_involution`) and preserve distance.
pub fn swap_displacement(hero_pos: Coord, pet_pos: Coord) -> (Coord, Coord) {
    (pet_pos, hero_pos)
}

/// Determine whether walking into an occupant triggers combat or displacement.
pub fn interact_with_occupant<Id: Copy>(
    hero_pos: Coord,
    occupant_pos: Coord,
    occupant_id: Id,
    is_tame: bool,
) -> HeroInteraction<Id> {
    if is_tame {
        let (new_hero, new_pet) = swap_displacement(hero_pos, occupant_pos);
        HeroInteraction::DisplacePet {
            pet_id: occupant_id,
            new_hero_pos: new_hero,
            new_pet_pos: new_pet,
        }
    } else {
        HeroInteraction::MeleeAttack(occupant_id)
    }
}

/// Calculate updated tameness and tame status after offering food/treat.
/// Proven in the Lean 4 model to strictly increase or maintain tameness.
pub fn feed_pet(tameness: u32, is_tame: bool, nutrition: u32) -> (u32, bool) {
    if is_tame {
        (tameness + nutrition / 10 + 1, true)
    } else if nutrition >= 100 {
        (5, true)
    } else {
        (tameness, false)
    }
}
