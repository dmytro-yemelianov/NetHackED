//! Declarative data tables, bestiary, and item catalog for NetHackED.
//!
//! Separates static game ontology and archetypes completely from the simulation runner.

pub mod items;
pub mod monsters;
pub mod pantheons;
pub mod roles;
pub mod ruleset;

pub use items::{
    create_item_record, get_item_archetype, initial_wand_charges, item_archetype_by_name,
    ItemArchetype, ItemKindId, WandDir, ITEM_CATALOG,
};
pub use monsters::{
    create_ghost_record, create_monster_record, get_monster_species, monster_archetype_by_name,
    monster_class_of, AiBehavior, Attack, AttackType, DamageType, MonsterArchetype, MonsterSize,
    MonsterSound, MonsterSpeciesId, BESTIARY, LEGACY_SPECIES,
};
pub use nethacked_types::ItemClass;
pub use pantheons::{get_pantheon_for_role, get_patron_deity};
pub use roles::{
    get_race, get_role, race_hatemask, race_hostile, race_lovemask, race_peaceful,
    spawn_player_character, spawn_starting_pet, starting_item_spe, starting_skills,
    CharacterConfig, Gender, RaceId, RaceSpec, RoleId, RoleSpec, RACES, ROLES,
};
pub use ruleset::{
    ArmorDef, ItemDef, MechanicsSection, MonsterDef, PackManifest, QuestDef, RaceDef, RoleDef,
    Ruleset, RulesetRef, StartingItem, ENGINE_REQUIRED_ITEMS, ENGINE_REQUIRED_MONSTERS,
};

#[cfg(test)]
mod tests {
    use super::*;
    use nethacked_arena::{EntityArena, ItemLocation};
    use nethacked_types::Coord;

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
        use nethacked_types::Buc;
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
        use nethacked_arena::EntityArena;
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

    #[test]
    fn starting_items_carry_c_trspe() {
        // C u_init.c:91 Knight LONG_SWORD +1; u_init.c:136 Rogue LEATHER_ARMOR +1;
        // ini_inv applies trspe (u_init.c:1233-1234).
        for (role, kind, spe) in [
            (RoleId::Knight, ItemKindId::LongSword, 1),
            (RoleId::Rogue, ItemKindId::LeatherArmor, 1),
            (RoleId::Rogue, ItemKindId::Dagger, 0),
            (RoleId::Rogue, ItemKindId::ShortSword, 0),
            (RoleId::Wizard, ItemKindId::CloakOfMagicResistance, 0),
        ] {
            let mut arena = EntityArena::new();
            let config = CharacterConfig {
                role,
                ..CharacterConfig::default()
            };
            let (_, items) =
                spawn_player_character(&config, Coord::new_unchecked(5, 5), &mut arena);
            let want = create_item_record(
                kind,
                ItemLocation::Floor(Coord::new_unchecked(0, 0)),
                nethacked_types::Buc::Uncursed,
            )
            .name;
            let item = items
                .iter()
                .filter_map(|&id| arena.items.get(id))
                .find(|it| it.name == want)
                .unwrap_or_else(|| panic!("{role:?} carries {want}"));
            assert_eq!(item.enchantment, spe, "{role:?} {want}");
            assert_eq!(starting_item_spe(role, kind), Some(spe));
        }
    }

    fn level_of(role: RoleId, class: nethacked_types::SkillClass) -> nethacked_types::SkillLevel {
        starting_skills(role)
            .into_iter()
            .find(|(c, _)| *c == class)
            .map(|(_, l)| l)
            .unwrap_or(nethacked_types::SkillLevel::Unskilled)
    }

    #[test]
    fn starting_skills_follow_c_skill_init() {
        use nethacked_types::{SkillClass as C, SkillLevel as L};
        // weapon.c:1752 skill_init: every inventory weapon's skill starts Basic.
        assert!(level_of(RoleId::Valkyrie, C::LongSword) >= L::Basic); // NetHackED inventory
        assert_eq!(level_of(RoleId::Valkyrie, C::Dagger), L::Basic); // u_init.c:160 Valkyrie[]
        assert_eq!(level_of(RoleId::Knight, C::LongSword), L::Basic);
        assert_eq!(level_of(RoleId::Rogue, C::Dagger), L::Basic);
        assert_eq!(level_of(RoleId::Rogue, C::ShortSword), L::Basic);
        assert_eq!(level_of(RoleId::Archaeologist, C::ShortSword), L::Basic);
        // Wizard: C starts with a quarterstaff (no NetHackED class); dagger is only
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
