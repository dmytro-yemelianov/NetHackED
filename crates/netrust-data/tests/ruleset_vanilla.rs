use netrust_arena::{EntityArena, ItemLocation};
use netrust_core::ac::{armor_base_ac, armor_slot};
use netrust_core::quest::get_role_quest_config;
use netrust_data::ruleset::Ruleset;
use netrust_data::{
    create_item_record, create_monster_record, get_pantheon_for_role, spawn_player_character,
    starting_item_spe, starting_skills, CharacterConfig, Gender, ItemClass, BESTIARY, ITEM_CATALOG,
    RACES, ROLES,
};
use netrust_types::{Buc, Coord};
use serde_json::json;

#[test]
fn test_ruleset_vanilla_monsters_parity() {
    let vanilla = Ruleset::vanilla();
    for arch in BESTIARY {
        let def = vanilla
            .monster(arch.name)
            .unwrap_or_else(|| panic!("monster {} not found in vanilla ruleset", arch.name));
        assert_eq!(def.id, Some(arch.id));
        assert_eq!(def.name, arch.name);
        assert_eq!(def.glyph, arch.glyph);
        assert_eq!(def.base_hp, arch.base_hp);
        assert_eq!(def.max_hp, arch.max_hp);
        assert_eq!(def.ac, arch.ac);
        assert_eq!(def.level, arch.level);
        assert_eq!(def.speed, arch.speed);
        assert_eq!(def.alignment, arch.alignment);
        assert_eq!(def.intrinsics, arch.intrinsics);
        assert_eq!(def.attacks, arch.attacks);
        assert_eq!(def.size, arch.size);
        assert_eq!(def.peaceful_by_default, arch.peaceful_by_default);
        assert_eq!(def.always_hostile, arch.always_hostile);
        assert_eq!(def.maligntyp, arch.maligntyp);
        assert_eq!(def.msound, arch.msound);
        assert_eq!(def.m2_race, arch.m2_race);
        assert_eq!(def.is_human, arch.is_human);
        assert_eq!(def.is_unique, arch.is_unique);
        assert_eq!(def.mindless, arch.mindless);
        assert_eq!(def.ai_behavior, arch.ai_behavior);
        assert_eq!(def.abilities, arch.abilities);

        let by_id = vanilla.monster_by_id(arch.id).expect("monster_by_id");
        assert_eq!(by_id, def);
    }
}

#[test]
fn test_ruleset_vanilla_items_parity() {
    let vanilla = Ruleset::vanilla();
    for arch in ITEM_CATALOG {
        let def = vanilla
            .item(arch.name)
            .unwrap_or_else(|| panic!("item {} not found in vanilla ruleset", arch.name));
        assert_eq!(def.id, Some(arch.id));
        assert_eq!(def.name, arch.name);
        assert_eq!(def.class, arch.class);
        assert_eq!(def.weight, arch.weight);
        assert_eq!(def.cost, arch.cost);
        assert_eq!(def.damage_small, arch.damage_small);
        assert_eq!(def.damage_large, arch.damage_large);
        assert_eq!(def.ac_bonus, arch.ac_bonus);
        assert_eq!(def.is_container, arch.is_container);
        assert_eq!(def.is_bag_of_holding, arch.is_bag_of_holding);
        assert_eq!(def.oc_magic, arch.oc_magic);
        assert_eq!(def.wand_dir, arch.wand_dir);
        assert_eq!(def.nutrition, arch.nutrition);

        if arch.class == ItemClass::Armor {
            let slot = armor_slot(arch.name).expect("armor slot for catalog armor");
            let base_ac = armor_base_ac(arch.name);
            let armor_def = def
                .armor
                .as_ref()
                .unwrap_or_else(|| panic!("missing armor def for {}", arch.name));
            assert_eq!(armor_def.slot, slot);
            assert_eq!(armor_def.base_ac, base_ac);
        } else {
            assert!(
                def.armor.is_none(),
                "non-armor item {} should have armor: None",
                arch.name
            );
        }

        let by_id = vanilla.item_by_id(arch.id).expect("item_by_id");
        assert_eq!(by_id, def);
    }
}

#[test]
fn test_ruleset_vanilla_roles_parity() {
    let vanilla = Ruleset::vanilla();
    for role in ROLES {
        let def = vanilla
            .role(role.id)
            .unwrap_or_else(|| panic!("role {:?} not found in vanilla ruleset", role.id));
        assert_eq!(def.id, role.id);
        assert_eq!(def.name, role.name);
        assert_eq!(def.base_hp, role.base_hp);
        assert_eq!(def.ac, role.ac);
        assert_eq!(def.speed, role.speed);
        assert_eq!(def.default_alignment, role.default_alignment);
        assert_eq!(def.initial_alignment_record, role.initial_alignment_record);
        assert_eq!(def.skills, starting_skills(role.id));

        let pantheon = get_pantheon_for_role(role.id);
        assert_eq!(
            def.pantheon,
            [
                pantheon.lawful.name,
                pantheon.neutral.name,
                pantheon.chaotic.name
            ]
        );

        let quest = get_role_quest_config(role.name);
        match (quest, &def.quest) {
            (Some(q), Some(dq)) => {
                assert_eq!(dq.leader, q.leader_name);
                assert_eq!(dq.nemesis, q.nemesis_name);
                assert_eq!(dq.guardian, q.guardian_name);
                assert_eq!(dq.artifact, q.artifact_name);
                assert_eq!(dq.home_desc, q.home_desc);
                assert_eq!(dq.goal_desc, q.goal_desc);
            }
            (None, None) => {}
            _ => panic!("quest config mismatch for role {:?}", role.id),
        }

        assert_eq!(def.starting_items.len(), role.starting_items.len());
        for (st_def, &kind) in def.starting_items.iter().zip(role.starting_items.iter()) {
            let item_arch = netrust_data::items::get_item_archetype(kind);
            assert_eq!(st_def.item, item_arch.name);
            assert_eq!(st_def.spe, starting_item_spe(role.id, kind));
        }
    }
}

#[test]
fn test_ruleset_vanilla_races_parity() {
    let vanilla = Ruleset::vanilla();
    for race in RACES {
        let def = vanilla
            .race(race.id)
            .unwrap_or_else(|| panic!("race {:?} not found in vanilla ruleset", race.id));
        assert_eq!(def.id, race.id);
        assert_eq!(def.name, race.name);
        assert_eq!(def.intrinsics, race.intrinsics);
    }
}

#[test]
fn test_records_generation_parity() {
    let vanilla = Ruleset::vanilla();
    let coord = Coord::new_unchecked(10, 15);

    // Monsters record parity
    for arch in BESTIARY {
        let free_rec = create_monster_record(arch.id, coord);
        let rs_rec = vanilla
            .create_monster_record(arch.name, coord)
            .expect("create_monster_record");
        assert_eq!(rs_rec, free_rec);
    }

    // Items record parity
    let loc = ItemLocation::Floor(coord);
    for arch in ITEM_CATALOG {
        for buc in [Buc::Blessed, Buc::Uncursed, Buc::Cursed] {
            let free_rec = create_item_record(arch.id, loc.clone(), buc);
            let rs_rec = vanilla
                .create_item_record(arch.name, loc.clone(), buc)
                .expect("create_item_record");
            assert_eq!(rs_rec, free_rec);
        }
    }

    // Player spawn parity
    for role in ROLES {
        for race in RACES {
            let config = CharacterConfig {
                name: format!("Hero_{}_{}", role.name, race.name),
                role: role.id,
                race: race.id,
                gender: Gender::Male,
                alignment: role.default_alignment,
            };

            let mut arena_free = EntityArena::new();
            let (actor_free, items_free) = spawn_player_character(&config, coord, &mut arena_free);

            let mut arena_rs = EntityArena::new();
            let (actor_rs, items_rs) =
                vanilla.spawn_player_character(&config, coord, &mut arena_rs);

            let a_free = arena_free.actors.get(actor_free).unwrap();
            let a_rs = arena_rs.actors.get(actor_rs).unwrap();
            assert_eq!(a_rs, a_free);

            assert_eq!(items_rs.len(), items_free.len());
            for (&i_free_id, &i_rs_id) in items_free.iter().zip(items_rs.iter()) {
                let it_free = arena_free.items.get(i_free_id).unwrap();
                let it_rs = arena_rs.items.get(i_rs_id).unwrap();
                assert_eq!(it_rs.name, it_free.name);
                assert_eq!(it_rs.class, it_free.class);
                assert_eq!(it_rs.weight, it_free.weight);
                assert_eq!(it_rs.buc, it_free.buc);
                assert_eq!(it_rs.enchantment, it_free.enchantment);
                assert_eq!(it_rs.location, ItemLocation::CarriedBy(actor_rs));
            }
        }
    }
}

#[test]
fn test_lookup_parity() {
    let vanilla = Ruleset::vanilla();

    // Case-insensitivity and runtime prefixes
    assert_eq!(
        vanilla.monster("Hostile Djinni").map(|m| m.name.as_str()),
        Some("djinni")
    );
    assert_eq!(
        vanilla.monster("ghost of Bob").map(|m| m.name.as_str()),
        Some("ghost")
    );
    assert_eq!(
        vanilla.monster("JACKAL").map(|m| m.name.as_str()),
        Some("jackal")
    );
    assert!(vanilla.monster("unknown nonexistent monster").is_none());

    // Monster class of
    assert_eq!(vanilla.monster_class_of("jackal"), Some('d'));
    assert_eq!(vanilla.monster_class_of("JACKAL"), Some('d'));
    assert_eq!(vanilla.monster_class_of("nonexistent"), None);
}

#[test]
fn test_serde_roundtrip_and_reindex() {
    let vanilla = Ruleset::vanilla();
    let val = serde_json::to_value(&*vanilla).expect("serialize Ruleset");
    let mut deserialized: Ruleset = serde_json::from_value(val).expect("deserialize Ruleset");
    deserialized.reindex();

    assert_eq!(*vanilla, deserialized);
    assert_eq!(
        deserialized.monster("jackal").map(|m| m.name.as_str()),
        Some("jackal")
    );
    assert_eq!(
        deserialized.item("dagger").map(|i| i.name.as_str()),
        Some("dagger")
    );
}

#[test]
fn test_serde_json_preserve_order_is_off() {
    // Canonical JSON requires sorted keys; preserve_order feature must stay OFF.
    let json_str = serde_json::to_string(&json!({"b": 1, "a": 2})).expect("json string");
    assert_eq!(json_str, r#"{"a":2,"b":1}"#);
}
