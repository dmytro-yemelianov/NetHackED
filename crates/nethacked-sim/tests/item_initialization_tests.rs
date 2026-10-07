use nethacked_sim::SimulationWorld;
use nethacked_types::{BranchId, Buc, ItemClass};
use rand::RngCore;

#[test]
fn ordinary_rooms_initialize_objects_and_save_replays_same_rng() {
    let mut sampled_buc = 0;
    let mut sampled_stacks = 0;
    let mut sampled_spe = 0;
    for seed in 0..32 {
        let mut world = SimulationWorld::new_with_seed(seed);
        let save = world.to_save_json().unwrap();
        let mut loaded =
            SimulationWorld::from_save_json(&save, world.ruleset.clone(), &world.ruleset_ref)
                .unwrap();
        world.unpack_or_generate_level(BranchId::DungeonsOfDoom, 4);
        loaded.unpack_or_generate_level(BranchId::DungeonsOfDoom, 4);
        let items: Vec<_> = world.arena.items.iter().collect();
        let replayed: Vec<_> = loaded.arena.items.iter().collect();
        assert_eq!(items, replayed, "seed {seed}");
        assert_eq!(world.rng.next_u64(), loaded.rng.next_u64());
        for (_, item) in items {
            if !matches!(item.location, nethacked_arena::ItemLocation::Floor(_)) {
                continue;
            }
            sampled_buc += usize::from(item.buc != Buc::Uncursed);
            sampled_stacks += usize::from(item.quantity > 1);
            sampled_spe += usize::from(
                matches!(
                    item.class,
                    ItemClass::Weapon | ItemClass::Armor | ItemClass::Ring
                ) && item.enchantment != 0,
            );
            let encoded = serde_json::to_string(item).unwrap();
            assert_eq!(
                &serde_json::from_str::<nethacked_arena::ItemRecord>(&encoded).unwrap(),
                item
            );
        }
    }
    assert!(sampled_buc > 0);
    assert!(sampled_stacks > 0);
    assert!(sampled_spe > 0);
}

#[test]
fn ordinary_room_containers_attach_contents_and_roundtrip_rng() {
    use nethacked_arena::ItemLocation;
    let mut populated = 0;
    let mut locked = 0;
    for seed in 0..384 {
        let mut world = SimulationWorld::new_with_seed(seed);
        world.unpack_or_generate_level(BranchId::DungeonsOfDoom, 4);
        for (id, item) in world.arena.items.iter() {
            if let ItemLocation::InContainer(parent) = item.location {
                assert!(world.arena.items[parent].is_container);
                assert!(world.arena.items_in_container(parent).contains(&id));
                assert!(
                    world.arena.calculate_total_weight(parent) >= world.arena.items[parent].weight
                );
                populated += 1;
            }
            locked += usize::from(item.locked);
        }
        let save = world.to_save_json().unwrap();
        let mut loaded =
            SimulationWorld::from_save_json(&save, world.ruleset.clone(), &world.ruleset_ref)
                .unwrap();
        assert_eq!(
            world.arena.items.iter().collect::<Vec<_>>(),
            loaded.arena.items.iter().collect::<Vec<_>>()
        );
        assert_eq!(world.rng.next_u64(), loaded.rng.next_u64());
        world.unpack_or_generate_level(BranchId::DungeonsOfDoom, 5);
        loaded.unpack_or_generate_level(BranchId::DungeonsOfDoom, 5);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&world.to_save_json().unwrap()).unwrap(),
            serde_json::from_str::<serde_json::Value>(&loaded.to_save_json().unwrap()).unwrap()
        );
    }
    assert!(
        populated > 0,
        "real ordinary rooms must generate contained objects"
    );
    assert!(locked > 0, "real room containers must sample lock state");
}

#[test]
fn artifact_history_survives_destruction_cache_legacy_and_renamed_saves() {
    use nethacked_arena::ItemLocation;
    use nethacked_data::{ItemKindId as K, Ruleset};
    let mut rules = (*Ruleset::vanilla()).clone();
    rules
        .items
        .iter_mut()
        .find(|d| d.id == Some(K::ART_EXCALIBUR))
        .unwrap()
        .name = "renamed sword".into();
    rules.reindex();
    let mut world = SimulationWorld::new_with_seed(10);
    world.ruleset = std::sync::Arc::new(rules);
    let item = world
        .ruleset
        .create_item_record_by_id(
            K::ART_EXCALIBUR,
            ItemLocation::Floor(nethacked_types::Coord::new(3, 4).unwrap()),
            Buc::Uncursed,
        )
        .unwrap();
    let id = world.arena.spawn_item(item);
    // Imported cached records participate even when no active record remains.
    let cached = world.arena.destroy_item(id).unwrap();
    world.stored_levels.push((
        (BranchId::DungeonsOfDoom, 1),
        nethacked_sim::world::StoredLevel {
            level: world.level.clone(),
            monsters: vec![],
            items: vec![(id, cached)],
            unpaid_items: vec![],
        },
    ));
    assert!(!world.arena.items.contains_key(id));
    world.unpack_or_generate_level(BranchId::DungeonsOfDoom, 2);
    assert!(world.arena.artifact_history.contains("ART_EXCALIBUR"));
    let mut save =
        serde_json::from_str::<serde_json::Value>(&world.to_save_json().unwrap()).unwrap();
    save["arena"]
        .as_object_mut()
        .unwrap()
        .remove("artifact_history");
    fn strip(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                map.remove("artifact_id");
                for child in map.values_mut() {
                    strip(child);
                }
            }
            serde_json::Value::Array(values) => {
                for child in values {
                    strip(child);
                }
            }
            _ => {}
        }
    }
    strip(&mut save);
    let legacy = serde_json::to_string(&save).unwrap();
    let mut loaded =
        SimulationWorld::from_save_json(&legacy, world.ruleset.clone(), &world.ruleset_ref)
            .unwrap();
    assert!(loaded.arena.artifact_history.contains("ART_EXCALIBUR"));
    loaded.unpack_or_generate_level(BranchId::DungeonsOfDoom, 1);
    let restored = loaded
        .arena
        .items
        .iter()
        .find(|(_, i)| i.name == "renamed sword")
        .map(|(id, _)| id)
        .unwrap();
    loaded.arena.destroy_item(restored).unwrap();
    let save = loaded.to_save_json().unwrap();
    let loaded =
        SimulationWorld::from_save_json(&save, loaded.ruleset.clone(), &loaded.ruleset_ref)
            .unwrap();
    assert!(loaded.arena.artifact_history.contains("ART_EXCALIBUR"));
    // A bare old non-artifact record has no display-name guessing.
    assert!(!loaded.arena.items.contains_key(id));
}

#[test]
fn bones_preserves_stable_artifact_identity_with_renamed_import_and_destroyed_history() {
    use nethacked_arena::ItemLocation;
    use nethacked_data::ItemKindId as K;
    let mut world = SimulationWorld::new_with_seed(50);
    world.depth = 4;
    let mut item = world
        .ruleset
        .create_item_record_by_id(
            K::ART_GRIMTOOTH,
            ItemLocation::CarriedBy(world.player_id),
            Buc::Blessed,
        )
        .unwrap();
    item.name = "external renamed artifact".into();
    let id = world.arena.spawn_item(item);
    let bones = world.save_bones("test").unwrap();
    assert!(bones
        .items
        .iter()
        .any(|i| i.artifact_id.as_deref() == Some("ART_GRIMTOOTH")));
    world.arena.destroy_item(id).unwrap();
    let mut recipient = SimulationWorld::new_with_seed(60);
    recipient.depth = 4;
    recipient.level = nethacked_dungeon::DungeonLevel::new_solid(nethacked_types::Tile::Room);
    recipient.bones_storage.push(bones);
    recipient.check_and_load_bones();
    let id = recipient
        .arena
        .items
        .iter()
        .find(|(_, i)| i.artifact_id.as_deref() == Some("ART_GRIMTOOTH"))
        .map(|(id, _)| id)
        .unwrap();
    assert!(recipient.arena.artifact_history.contains("ART_GRIMTOOTH"));
    recipient.arena.destroy_item(id).unwrap();
    assert!(recipient.arena.artifact_history.contains("ART_GRIMTOOTH"));
}

#[test]
fn explicitly_mutated_and_imported_artifacts_register_without_rng() {
    use nethacked_arena::ItemLocation;
    use nethacked_data::ItemKindId as K;
    let mut world = SimulationWorld::new_with_seed(2);
    let mut expected_rng = world.rng.clone();
    let item = world
        .ruleset
        .create_item_record_by_id(K::LONG_SWORD, ItemLocation::Limbo, Buc::Uncursed)
        .unwrap();
    let id = world.arena.spawn_item(item);
    world.arena.items[id].artifact_id = Some("ART_FIRE_BRAND".into());
    let mut imported = world
        .ruleset
        .create_item_record_by_id(K::ART_ORB_OF_FATE, ItemLocation::Limbo, Buc::Uncursed)
        .unwrap();
    imported.artifact_id = None;
    world.arena.items.insert(imported);
    world.register_existing_artifacts();
    assert!(world.arena.artifact_history.contains("ART_FIRE_BRAND"));
    assert!(world.arena.artifact_history.contains("ART_ORB_OF_FATE"));
    assert_eq!(world.rng.next_u64(), expected_rng.next_u64());
}

#[test]
fn public_room_generation_artifact_hook_and_save_continuation() {
    // Pinned vanilla seed: room generation creates Grimtooth on floor 4.
    // Requires the public world path to pass its role/history to the hook.
    // Keep the pinned item stream independent of variable monster sex.
    let mut rules = (*nethacked_data::Ruleset::vanilla()).clone();
    for def in &mut rules.monsters {
        if def.fixed_sex().is_none() {
            def.flags.push("male".into());
        }
    }
    rules.reindex();
    let mut world = SimulationWorld::new_with_character_and_ruleset(
        121,
        nethacked_data::CharacterConfig::default(),
        std::sync::Arc::new(rules),
        nethacked_data::ruleset::RulesetRef::vanilla(),
    );
    assert!(world.arena.artifact_history.is_empty());
    let initial_ids: Vec<_> = world.arena.items.iter().map(|(id, _)| id).collect();
    world.unpack_or_generate_level(BranchId::DungeonsOfDoom, 4);
    let artifacts: Vec<_> = world
        .arena
        .items
        .iter()
        .filter(|(_, item)| item.artifact_id.is_some())
        .collect();
    assert_eq!(
        artifacts.len(),
        1,
        "public ordinary-room generation must create the pinned artifact"
    );
    let (id, item) = artifacts[0];
    assert!(
        !initial_ids.contains(&id),
        "artifact must be newly generated"
    );
    assert_eq!(item.artifact_id.as_deref(), Some("ART_GRIMTOOTH"));
    assert_eq!(item.name, "Grimtooth");
    assert!(item.poisoned);
    assert!(matches!(
        item.location,
        nethacked_arena::ItemLocation::Floor(_)
    ));
    assert_eq!(
        world.arena.artifact_history,
        std::collections::BTreeSet::from(["ART_GRIMTOOTH".to_owned()])
    );

    let save = world.to_save_json().unwrap();
    let mut loaded =
        SimulationWorld::from_save_json(&save, world.ruleset.clone(), &world.ruleset_ref).unwrap();
    assert_eq!(
        world.arena.items.iter().collect::<Vec<_>>(),
        loaded.arena.items.iter().collect::<Vec<_>>()
    );
    assert_eq!(world.arena.artifact_history, loaded.arena.artifact_history);
    assert_eq!(world.rng.next_u64(), loaded.rng.next_u64());
    world.unpack_or_generate_level(BranchId::DungeonsOfDoom, 5);
    loaded.unpack_or_generate_level(BranchId::DungeonsOfDoom, 5);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&world.to_save_json().unwrap()).unwrap(),
        serde_json::from_str::<serde_json::Value>(&loaded.to_save_json().unwrap()).unwrap()
    );
    assert_eq!(world.rng.next_u64(), loaded.rng.next_u64());
}
