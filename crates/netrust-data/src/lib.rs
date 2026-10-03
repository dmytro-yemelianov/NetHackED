//! Declarative data tables, bestiary, and item catalog for NetRust.
//!
//! Separates static game ontology and archetypes completely from the simulation runner.

pub mod items;
pub mod monsters;
pub mod roles;

pub use items::{create_item_record, get_item_archetype, ItemArchetype, ItemKindId, ITEM_CATALOG};
pub use monsters::{
    create_monster_record, get_monster_species, AiBehavior, MonsterArchetype, MonsterSpeciesId,
    BESTIARY,
};
pub use roles::{
    get_race, get_role, spawn_player_character, CharacterConfig, Gender, RaceId, RaceSpec, RoleId,
    RoleSpec, RACES, ROLES,
};

#[cfg(test)]
mod tests {
    use super::*;
    use netrust_arena::ItemLocation;
    use netrust_types::Coord;

    #[test]
    fn test_all_monsters_registered_and_spawnable() {
        for arch in BESTIARY {
            let record = create_monster_record(arch.id, Coord::new_unchecked(10, 10));
            assert_eq!(record.name, arch.name);
            assert_eq!(record.hp, arch.base_hp);
            assert_eq!(record.ac, arch.ac);
        }
    }

    #[test]
    fn test_all_items_registered_and_spawnable() {
        use netrust_types::Buc;
        for arch in ITEM_CATALOG {
            let record = create_item_record(arch.id, ItemLocation::Floor(Coord::new_unchecked(5, 5)), Buc::Uncursed);
            assert_eq!(record.name, arch.name);
            assert_eq!(record.weight, arch.weight);
            assert_eq!(record.is_container, arch.is_container);
            assert_eq!(record.buc, Buc::Uncursed);
        }
    }

    #[test]
    fn test_character_creation_for_all_roles() {
        use netrust_arena::EntityArena;
        for role in ROLES {
            let mut arena = EntityArena::new();
            let config = CharacterConfig {
                name: format!("Hero_{}", role.name),
                role: role.id,
                race: RaceId::Human,
                gender: Gender::Female,
                alignment: role.default_alignment,
            };
            let (actor_id, items) = spawn_player_character(&config, Coord::new_unchecked(5, 5), &mut arena);
            let actor = arena.actors.get(actor_id).unwrap();
            assert_eq!(actor.hp, role.base_hp);
            assert_eq!(actor.ac, role.ac);
            assert_eq!(items.len(), role.starting_items.len());
        }
    }
}
