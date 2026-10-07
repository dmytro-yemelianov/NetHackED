use nethacked_arena::{ItemDefinition, ItemId, ItemLocation};
use nethacked_data::{CharacterConfig, ItemKindId as I, MonsterSpeciesId as M, Ruleset};
use nethacked_sim::{world::StoredLevel, ActionAst, GameEvent, SimulationWorld};
use nethacked_types::{BonesData, BranchId, Buc, Coord, Direction, Tile};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::sync::Arc;

const HERO: Coord = Coord::new_unchecked(20, 10);

fn rules() -> Ruleset {
    let mut rules = (*Ruleset::vanilla()).clone();
    for m in &mut rules.monsters {
        m.frequency = 0;
    }
    let m = rules
        .monsters
        .iter_mut()
        .find(|m| m.id == Some(M::GIANT_ANT))
        .unwrap();
    m.name = "Owned candidate".into();
    m.frequency = 1;
    m.difficulty = 1;
    m.level = 1;
    m.sample_hp = false;
    m.base_hp = 71;
    m.max_hp = 83;
    m.speed = 0;
    m.gen_flags.clear();
    m.abilities.clear();
    let def = rules
        .items
        .iter_mut()
        .find(|d| d.id == Some(I::WAN_CREATE_MONSTER))
        .unwrap();
    def.name = "Owned rod".into();
    let mut custom = def.clone();
    custom.id = None;
    custom.name = "Pack ROD".into();
    rules.items.push(custom);
    rules.reindex();
    rules
}

fn world(rules: Ruleset) -> SimulationWorld {
    let mut world = SimulationWorld::new_with_character_and_ruleset(
        33,
        CharacterConfig::default(),
        Arc::new(rules),
        nethacked_data::ruleset::RulesetRef::vanilla(),
    );
    world.arena.actors.retain(|id, _| id == world.player_id);
    world.arena.items.clear();
    world.equipment = Default::default();
    world.wielded_item = None;
    world.unpaid_items.clear();
    world.level = nethacked_dungeon::DungeonLevel::new_solid(Tile::Room);
    world.arena.actors[world.player_id].coord = HERO;
    world.arena.actors[world.player_id].level = 2;
    world.arena.actors[world.player_id].hp = 1000;
    world.arena.actors[world.player_id].max_hp = 1000;
    world.scheduler.turn = 0;
    world.scheduler.hero_energy = 12;
    world.scheduler.hero_speed = 12;
    world.depth = 7;
    world.rng = ChaCha8Rng::seed_from_u64(33);
    world
}

fn wand(w: &mut SimulationWorld, custom: bool, display: &str) -> ItemId {
    let mut item = if custom {
        w.ruleset
            .create_item_record(
                "pack rod",
                ItemLocation::CarriedBy(w.player_id),
                Buc::Uncursed,
            )
            .unwrap()
    } else {
        w.ruleset
            .create_item_record_by_id(
                I::WAN_CREATE_MONSTER,
                ItemLocation::CarriedBy(w.player_id),
                Buc::Uncursed,
            )
            .unwrap()
    };
    item.name = display.into();
    item.enchantment = 2;
    w.arena.spawn_item(item)
}

fn zap(w: &mut SimulationWorld) -> Vec<GameEvent> {
    w.step_player_action(ActionAst::ZapWand {
        dir: Direction::East,
        energy: 9,
    })
}
fn save(w: &SimulationWorld) -> serde_json::Value {
    serde_json::from_str(&w.to_save_json().unwrap()).unwrap()
}
fn load(w: &SimulationWorld) -> SimulationWorld {
    SimulationWorld::from_save_json(
        &w.to_save_json().unwrap(),
        w.ruleset.clone(),
        &w.ruleset_ref,
    )
    .unwrap()
}
fn omit_definitions(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.remove("definition");
            for child in map.values_mut() {
                omit_definitions(child);
            }
        }
        serde_json::Value::Array(array) => {
            for child in array {
                omit_definitions(child);
            }
        }
        _ => {}
    }
}

#[test]
fn public_runtime_rename_and_owned_override_preserve_create_charge_turn_and_rng() {
    let mut a = world(rules());
    let id = wand(&mut a, false, "arbitrary display");
    let mut b = load(&a);
    b.arena.items[id].name = "Owned rod".into();
    let mut first_draw = a.rng.clone();
    assert_ne!(first_draw.random_range(0..23u32), 0); // one creation, no charge draw
    let events = zap(&mut a);
    assert_eq!(events, zap(&mut b));
    assert!(!events
        .iter()
        .any(|e| matches!(e, GameEvent::BeamPropagated { .. })));
    let monsters: Vec<_> = a
        .arena
        .actors
        .iter()
        .filter(|(id, _)| *id != a.player_id)
        .collect();
    assert_eq!(monsters.len(), 1);
    assert_eq!(
        (
            monsters[0].1.name.as_str(),
            monsters[0].1.hp,
            monsters[0].1.max_hp
        ),
        ("Owned candidate", 71, 83)
    );
    assert_eq!(
        serde_json::to_value(&a.arena.actors).unwrap(),
        serde_json::to_value(&b.arena.actors).unwrap()
    );
    assert_eq!(a.rng, b.rng);
    assert_eq!(a.arena.items[id].enchantment, 1);
    assert_eq!(a.scheduler.turn, 1);
    assert_eq!(a.scheduler.hero_energy, 12);
    assert_eq!(
        a.arena.items[id].definition,
        Some(ItemDefinition::Vanilla("WAN_CREATE_MONSTER".into()))
    );
}

#[test]
fn custom_identity_ignores_runtime_create_and_effect_fragments() {
    let mut a = world(rules());
    let id = wand(
        &mut a,
        true,
        "wand of create monster cold death secret door",
    );
    let mut b = load(&a);
    b.arena.items[id].name = "Pack ROD".into();
    assert_eq!(zap(&mut a), zap(&mut b));
    assert_eq!(a.arena.actors.len(), 1);
    assert_eq!(a.arena.items[id].enchantment, 1);
    assert_eq!(a.rng, b.rng);
    assert_eq!(
        a.arena.items[id].definition,
        Some(ItemDefinition::Custom("pack rod".into()))
    );
    assert_eq!(
        a.ruleset
            .item_for_definition(a.arena.items[id].definition.as_ref().unwrap())
            .unwrap()
            .id,
        None
    );
}

#[test]
fn explicit_unknown_identity_refuses_before_charge_rng_scheduler_beam_or_target() {
    for key in [
        ItemDefinition::Vanilla("UNKNOWN_SYMBOL".into()),
        ItemDefinition::Vanilla("WAN_CREATE_MONSTER".into()),
        ItemDefinition::Custom("missing key".into()),
    ] {
        let mut rs = rules();
        let mut w = world(rs.clone());
        wand(
            &mut w,
            false,
            "wand of create monster cold death secret door",
        );
        wand(&mut w, true, "unrelated charged wand");
        let id = w.arena.items_carried_by(w.player_id)[0];
        w.arena.items[id].name = "wand of create monster cold death secret door".into();
        w.arena.items[id].definition = Some(key.clone());
        if key == ItemDefinition::Vanilla("WAN_CREATE_MONSTER".into()) {
            rs.items.retain(|d| d.id != Some(I::WAN_CREATE_MONSTER));
            rs.reindex();
            w.ruleset = Arc::new(rs);
        }
        let witness = w
            .ruleset
            .monster_by_id(M::GIANT_ANT)
            .unwrap()
            .create_record_with_hp(Coord::new_unchecked(21, 10), 77, 77, 1);
        w.arena.spawn_actor(witness);
        w.level
            .set_tile(Coord::new_unchecked(22, 10), Tile::Pool { frozen: false });
        let before = save(&w);
        let events = zap(&mut w);
        assert_eq!(
            events,
            vec![GameEvent::LogMessage {
                text: "You cannot zap that wand: its definition is unavailable.".into()
            }]
        );
        let mut after = save(&w);
        assert_eq!(after["rng"], before["rng"]);
        assert_eq!(after["scheduler"], before["scheduler"]);
        assert_eq!(after["arena"], before["arena"]);
        assert_eq!(after["level"], before["level"]);
        after["event_log"] = before["event_log"].clone();
        assert_eq!(after, before);
    }
}

#[test]
fn active_cached_and_legacy_saves_keep_identity_omission_and_native_continuation() {
    let mut w = world(rules());
    let id = wand(&mut w, false, "Owned rod");
    let custom = w
        .ruleset
        .create_item_record("Pack ROD", ItemLocation::Floor(HERO), Buc::Uncursed)
        .unwrap();
    w.stored_levels.push((
        (BranchId::DungeonsOfDoom, 8),
        StoredLevel {
            level: w.level.clone(),
            monsters: vec![],
            items: vec![(id, custom)],
            unpaid_items: vec![],
        },
    ));
    let mut restored = load(&w);
    assert_eq!(save(&w), save(&restored));
    assert_eq!(
        restored.stored_levels[0].1.items[0].1.definition,
        Some(ItemDefinition::Custom("pack rod".into()))
    );
    assert_eq!(zap(&mut w), zap(&mut restored));
    assert_eq!(save(&w), save(&restored));
    // Capture a real active+cached item save, then remove only the new optional key.
    let mut old_world = world(rules());
    let id = wand(&mut old_world, false, "Owned rod");
    old_world.stored_levels = restored.stored_levels.clone();
    let mut old = save(&old_world);
    omit_definitions(&mut old);
    let mut legacy = SimulationWorld::from_save_json(
        &serde_json::to_string(&old).unwrap(),
        old_world.ruleset.clone(),
        &old_world.ruleset_ref,
    )
    .unwrap();
    assert!(legacy.arena.items[id].definition.is_none());
    assert!(legacy.stored_levels[0].1.items[0].1.definition.is_none());
    assert_eq!(
        serde_json::to_vec(&save(&legacy)).unwrap(),
        serde_json::to_vec(&old).unwrap()
    );
    let mut next = load(&legacy);
    assert_eq!(zap(&mut legacy), zap(&mut next));
    assert_eq!(save(&legacy), save(&next));
    assert!(legacy.arena.items[id].definition.is_none());
    assert_eq!(legacy.arena.items[id].enchantment, 1);
    assert_eq!(legacy.arena.actors.len(), 2); // legacy exact owned-name create path
}

#[test]
fn public_local_bones_json_storage_restoration_and_pickup_keep_definition() {
    let mut source = world(rules());
    wand(&mut source, false, "arbitrary canonical display");
    wand(&mut source, true, "wand of create monster cold death");
    let artifact = source
        .ruleset
        .create_item_record_by_id(
            I::ART_EXCALIBUR,
            ItemLocation::CarriedBy(source.player_id),
            Buc::Uncursed,
        )
        .unwrap();
    source.arena.spawn_item(artifact);
    let expected_definitions: Vec<_> = source
        .arena
        .items_carried_by(source.player_id)
        .into_iter()
        .map(|id| source.arena.items[id].definition.clone())
        .collect();
    let mut oracle = source.rng.clone();
    let rolls: Vec<u32> = (0..3).map(|_| oracle.random_range(0..5u32)).collect();
    let bones = source.save_bones("test").unwrap();
    assert_eq!(source.rng, oracle);
    for (item, roll) in bones.items.iter().zip(rolls) {
        assert_eq!(
            item.buc,
            if roll != 0 {
                Buc::Cursed
            } else {
                Buc::Uncursed
            }
        );
    }
    assert_eq!(
        bones
            .items
            .iter()
            .map(|i| i.definition.clone())
            .collect::<Vec<_>>(),
        expected_definitions
    );
    assert!(bones
        .items
        .iter()
        .any(|i| i.definition == Some(ItemDefinition::Vanilla("WAN_CREATE_MONSTER".into()))));
    let custom_bones_item = bones
        .items
        .iter()
        .find(|i| i.definition == Some(ItemDefinition::Custom("pack rod".into())))
        .unwrap()
        .clone();
    assert!(bones
        .items
        .iter()
        .any(|i| i.artifact_id.as_deref() == Some("ART_EXCALIBUR")));
    let json = serde_json::to_string(&bones).unwrap();
    let decoded: BonesData = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, bones);
    assert_eq!(load(&source).bones_storage, source.bones_storage);
    let mut target = world(rules());
    target.bones_storage.push(decoded);
    let mut restored = load(&target);
    assert_eq!(
        target.check_and_load_bones(),
        restored.check_and_load_bones()
    );
    assert_eq!(save(&target), save(&restored));
    let ids: Vec<_> = target.arena.items.keys().collect();
    for id in ids {
        assert_eq!(
            target.arena.items[id].definition,
            restored.arena.items[id].definition
        );
        assert_eq!(
            target.arena.items[id].artifact_id,
            restored.arena.items[id].artifact_id
        );
    }
    let canonical = target
        .arena
        .items
        .iter()
        .find(|(_, i)| i.definition == Some(ItemDefinition::Vanilla("WAN_CREATE_MONSTER".into())))
        .map(|(id, _)| id)
        .unwrap();
    target.arena.actors.retain(|id, _| id == target.player_id);
    let at = match target.arena.items[canonical].location {
        ItemLocation::Floor(c) => c,
        _ => panic!("bones item not scattered"),
    };
    target.arena.actors[target.player_id].coord = at;
    target.arena.items[canonical].name = "second runtime rename".into();
    target.arena.items[canonical].buc = Buc::Uncursed;
    let mut restored = load(&target);
    assert_eq!(
        target.step_player_action(ActionAst::PickUp),
        restored.step_player_action(ActionAst::PickUp)
    );
    assert_eq!(
        target.arena.items[canonical].location,
        ItemLocation::CarriedBy(target.player_id)
    );
    assert_eq!(zap(&mut target), zap(&mut restored));
    assert_eq!(save(&target), save(&restored));
    assert_eq!(target.arena.items[canonical].enchantment, 1);
    assert!(target
        .arena
        .actors
        .values()
        .any(|a| a.name == "Owned candidate"));
    // Restore and pick up custom separately; its runtime fragment cannot grant creation.
    let mut custom_target = world(rules());
    let mut custom_bones = bones.clone();
    custom_bones.items = vec![custom_bones_item];
    custom_target.bones_storage.push(custom_bones);
    custom_target.check_and_load_bones();
    custom_target
        .arena
        .actors
        .retain(|id, _| id == custom_target.player_id);
    let custom = custom_target.arena.items.keys().next().unwrap();
    custom_target.arena.items[custom].buc = Buc::Uncursed;
    custom_target.step_player_action(ActionAst::PickUp);
    let mut next = load(&custom_target);
    assert_eq!(zap(&mut custom_target), zap(&mut next));
    assert_eq!(save(&custom_target), save(&next));
    assert_eq!(custom_target.arena.actors.len(), 1);
    assert_eq!(
        custom_target.arena.items[custom].definition,
        Some(ItemDefinition::Custom("pack rod".into()))
    );
}

#[test]
fn old_bones_json_omits_identity_through_full_save_and_actual_restoration() {
    let mut source = world(rules());
    wand(&mut source, false, "Owned rod");
    let mut old = serde_json::to_value(source.save_bones("old").unwrap()).unwrap();
    omit_definitions(&mut old);
    let bytes = serde_json::to_vec(&old).unwrap();
    let bones: BonesData = serde_json::from_value(old).unwrap();
    assert_eq!(
        serde_json::to_vec(&serde_json::to_value(&bones).unwrap()).unwrap(),
        bytes
    );
    let mut target = world(rules());
    target.bones_storage.push(bones);
    let mut restored = load(&target);
    assert!(restored.bones_storage[0].items[0].definition.is_none());
    assert_eq!(save(&target), save(&restored));
    target.check_and_load_bones();
    restored.check_and_load_bones();
    assert_eq!(save(&target), save(&restored));
    let id = target.arena.items.keys().next().unwrap();
    assert!(target.arena.items[id].definition.is_none());
    target.arena.actors.retain(|id, _| id == target.player_id);
    target.arena.items[id].buc = Buc::Uncursed;
    target.step_player_action(ActionAst::PickUp);
    let mut next = load(&target);
    assert_eq!(zap(&mut target), zap(&mut next));
    assert_eq!(save(&target), save(&next));
    assert_eq!(target.arena.actors.len(), 2);
    assert!(target.arena.items[id].definition.is_none());
}
