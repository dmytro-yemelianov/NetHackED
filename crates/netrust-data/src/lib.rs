//! Declarative data tables, bestiary, and item catalog for NetRust.
//!
//! Separates static game ontology and archetypes completely from the simulation runner.

pub mod items;
pub mod monsters;
pub mod pantheons;
pub mod roles;

pub use items::{
    create_item_record, get_item_archetype, initial_wand_charges, item_archetype_by_name,
    ItemArchetype, ItemKindId, WandDir, ITEM_CATALOG,
};
pub use monsters::{
    create_ghost_record, create_monster_record, get_monster_species, monster_archetype_by_name,
    monster_class_of, AiBehavior, Attack, AttackType, DamageType, MonsterArchetype, MonsterSize,
    MonsterSpeciesId, BESTIARY,
};
pub use pantheons::{get_pantheon_for_role, get_patron_deity};
pub use roles::{
    get_race, get_role, spawn_player_character, spawn_starting_pet, starting_skills,
    CharacterConfig, Gender, RaceId, RaceSpec, RoleId, RoleSpec, RACES, ROLES,
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
            let record = create_item_record(
                arch.id,
                ItemLocation::Floor(Coord::new_unchecked(5, 5)),
                Buc::Uncursed,
            );
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
            let (actor_id, items) =
                spawn_player_character(&config, Coord::new_unchecked(5, 5), &mut arena);
            let actor = arena.actors.get(actor_id).unwrap();
            assert_eq!(actor.hp, role.base_hp);
            assert_eq!(actor.ac, role.ac);
            assert_eq!(items.len(), role.starting_items.len());
        }
    }

    fn level_of(role: RoleId, class: netrust_types::SkillClass) -> netrust_types::SkillLevel {
        starting_skills(role)
            .into_iter()
            .find(|(c, _)| *c == class)
            .map(|(_, l)| l)
            .unwrap_or(netrust_types::SkillLevel::Unskilled)
    }

    #[test]
    fn starting_skills_follow_c_skill_init() {
        use netrust_types::{SkillClass as C, SkillLevel as L};
        // weapon.c:1752 skill_init: every inventory weapon's skill starts Basic.
        assert!(level_of(RoleId::Valkyrie, C::LongSword) >= L::Basic); // NetRust inventory
        assert_eq!(level_of(RoleId::Valkyrie, C::Dagger), L::Basic); // u_init.c:160 Valkyrie[]
        assert_eq!(level_of(RoleId::Knight, C::LongSword), L::Basic);
        assert_eq!(level_of(RoleId::Rogue, C::Dagger), L::Basic);
        assert_eq!(level_of(RoleId::Rogue, C::ShortSword), L::Basic);
        assert_eq!(level_of(RoleId::Archaeologist, C::ShortSword), L::Basic);
        // Wizard: C starts with a quarterstaff (no NetRust class); dagger is only
        // Unskilled-but-allowed in Skill_W, so no weapon class is Basic.
        assert_eq!(level_of(RoleId::Wizard, C::Dagger), L::Unskilled);
        // weapon.c:1784: max skill above Expert => bare hands start Basic.
        assert_eq!(level_of(RoleId::Monk, C::BareHanded), L::Basic); // Skill_Mon martial arts GM
        assert_eq!(level_of(RoleId::Barbarian, C::BareHanded), L::Basic); // Skill_B Master
        assert_eq!(level_of(RoleId::Valkyrie, C::BareHanded), L::Unskilled); // Skill_V Expert
        assert_eq!(level_of(RoleId::Healer, C::BareHanded), L::Unskilled);
        // A role whose table lacks the class never gets it.
        assert_eq!(level_of(RoleId::Monk, C::LongSword), L::Unskilled);
    }
}
