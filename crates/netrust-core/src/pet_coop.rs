//! Tactical multi-agent cooperation: Pet BUC detection, promotion hierarchy, and tactical goal selection.
//!
//! Modeled in Lean 4 (`NetMechanics/PetCoop.lean`).

use netrust_types::{Buc, Coord};

/// Evaluates whether a floor tile with items is safe for a calm pet to step onto.
/// Proved in Lean 4: Returns false if ANY item is cursed (`pet_rejects_cursed_tile`),
/// and true if all items are non-cursed (`pet_accepts_safe_tile`).
pub fn pet_tile_steppable(tile_items: &[Buc]) -> bool {
    tile_items.iter().all(|&b| b != Buc::Cursed)
}

/// Biological family of the pet companion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PetFamily {
    Canine,
    Feline,
}

/// Species growth tier for companion animals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PetSpeciesTier {
    LittleDog,
    Dog,
    WarDog,
    Kitten,
    Housecat,
    LargeCat,
}

impl PetSpeciesTier {
    pub fn family(self) -> PetFamily {
        match self {
            PetSpeciesTier::LittleDog | PetSpeciesTier::Dog | PetSpeciesTier::WarDog => {
                PetFamily::Canine
            }
            PetSpeciesTier::Kitten | PetSpeciesTier::Housecat | PetSpeciesTier::LargeCat => {
                PetFamily::Feline
            }
        }
    }

    pub fn power_tier(self) -> u32 {
        match self {
            PetSpeciesTier::LittleDog | PetSpeciesTier::Kitten => 0,
            PetSpeciesTier::Dog | PetSpeciesTier::Housecat => 1,
            PetSpeciesTier::WarDog | PetSpeciesTier::LargeCat => 2,
        }
    }
}

/// Promotes a pet species based on accumulated experience level.
/// Proved in Lean 4: Strictly preserves biological family (`promote_preserves_family`)
/// and monotonic with respect to level (`promote_monotonic_level`).
pub fn promote_pet(species: PetSpeciesTier, level: u32) -> PetSpeciesTier {
    match species {
        PetSpeciesTier::LittleDog => {
            if level >= 7 {
                PetSpeciesTier::WarDog
            } else if level >= 4 {
                PetSpeciesTier::Dog
            } else {
                PetSpeciesTier::LittleDog
            }
        }
        PetSpeciesTier::Dog => {
            if level >= 7 {
                PetSpeciesTier::WarDog
            } else {
                PetSpeciesTier::Dog
            }
        }
        PetSpeciesTier::WarDog => PetSpeciesTier::WarDog,
        PetSpeciesTier::Kitten => {
            if level >= 7 {
                PetSpeciesTier::LargeCat
            } else if level >= 4 {
                PetSpeciesTier::Housecat
            } else {
                PetSpeciesTier::Kitten
            }
        }
        PetSpeciesTier::Housecat => {
            if level >= 7 {
                PetSpeciesTier::LargeCat
            } else {
                PetSpeciesTier::Housecat
            }
        }
        PetSpeciesTier::LargeCat => PetSpeciesTier::LargeCat,
    }
}

/// Tactical goal chosen by the pet in multi-agent co-op.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PetGoal<Id = usize> {
    AttackHostile(Id),
    FetchItem(Coord),
    FollowHero,
}

/// Co-op decision function:
/// Proved in Lean 4: Hostile defense strictly overrides item fetching or following (`pet_prioritizes_hero_defense`).
pub fn choose_pet_goal<Id: Copy>(
    hostile_near_hero: Option<Id>,
    safe_item_nearby: Option<Coord>,
) -> PetGoal<Id> {
    match hostile_near_hero {
        Some(hostile_id) => PetGoal::AttackHostile(hostile_id),
        None => match safe_item_nearby {
            Some(coord) => PetGoal::FetchItem(coord),
            None => PetGoal::FollowHero,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pet_tile_steppable_rejects_cursed() {
        assert!(pet_tile_steppable(&[]));
        assert!(pet_tile_steppable(&[Buc::Blessed, Buc::Uncursed]));
        assert!(!pet_tile_steppable(&[Buc::Blessed, Buc::Cursed]));
    }

    #[test]
    fn test_promotion_ladder() {
        assert_eq!(
            promote_pet(PetSpeciesTier::LittleDog, 1),
            PetSpeciesTier::LittleDog
        );
        assert_eq!(
            promote_pet(PetSpeciesTier::LittleDog, 4),
            PetSpeciesTier::Dog
        );
        assert_eq!(
            promote_pet(PetSpeciesTier::LittleDog, 7),
            PetSpeciesTier::WarDog
        );
        assert_eq!(
            promote_pet(PetSpeciesTier::Kitten, 5),
            PetSpeciesTier::Housecat
        );
        assert_eq!(
            promote_pet(PetSpeciesTier::Housecat, 8),
            PetSpeciesTier::LargeCat
        );
        assert_eq!(
            promote_pet(PetSpeciesTier::WarDog, 10),
            PetSpeciesTier::WarDog
        );
    }

    #[test]
    fn test_tactical_defense_priority() {
        let goal = choose_pet_goal(Some(42), Some(Coord::new_unchecked(5, 5)));
        assert_eq!(goal, PetGoal::AttackHostile(42));

        let goal2 = choose_pet_goal::<usize>(None, Some(Coord::new_unchecked(5, 5)));
        assert_eq!(goal2, PetGoal::FetchItem(Coord::new_unchecked(5, 5)));

        let goal3 = choose_pet_goal::<usize>(None, None);
        assert_eq!(goal3, PetGoal::FollowHero);
    }
}
