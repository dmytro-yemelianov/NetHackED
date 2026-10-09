use super::super::*;
use crate::owner_persistence::wand_ports::*;
use nethacked_arena::{ItemLocation, ItemRecord};
use nethacked_types::{Buc, Coord, Direction, Tile};
use rand::{RngCore, SeedableRng};
use wand_gameplay::*;
fn fixture(name: &str, target: Option<&str>) -> (SimulationWorld, nethacked_arena::ItemId) {
    let mut w = SimulationWorld::new_with_seed(53);
    w.hero.skills.skills.clear();
    w.arena.actors.retain(|id, _| id == w.player_id);
    w.arena.items.clear();
    w.unpaid_items.clear();
    w.billing = Default::default();
    w.equipment = Default::default();
    w.wielded_item = None;
    w.level = nethacked_dungeon::DungeonLevel::new_solid(Tile::Room);
    w.arena.actors[w.player_id].coord = Coord::new_unchecked(20, 10);
    w.arena.actors[w.player_id].hp = 1000;
    w.arena.actors[w.player_id].max_hp = 1000;
    w.arena.actors[w.player_id].level = 2;
    w.progress.level_max = 2;
    w.scheduler.hero_energy = 120;
    w.scheduler.hero_speed = 12;
    w.scheduler.turn = 0;
    let item = w
        .ruleset
        .create_item_record(name, ItemLocation::CarriedBy(w.player_id), Buc::Uncursed)
        .unwrap();
    let id = w.arena.spawn_item(ItemRecord {
        enchantment: 3,
        ..item
    });
    if let Some(name) = target {
        let mut a = w
            .ruleset
            .create_monster_record(name, Coord::new_unchecked(21, 10), &mut w.rng)
            .unwrap();
        a.hp = 1;
        a.max_hp = 1;
        a.speed = 0;
        a.movement = 0;
        w.arena.spawn_actor(a);
    }
    w.rng = rand_chacha::ChaCha8Rng::seed_from_u64(100);
    (w, id)
}
fn bridge(w: &SimulationWorld) -> OwnerBridge {
    let json = w.to_save_json().unwrap();
    OwnerBridge::import_native(
        &json,
        json.len(),
        OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
            .unwrap()
            .with_wand_gameplay(),
    )
    .unwrap()
}
fn action(id: nethacked_arena::ItemId) -> WandGameplayAction {
    WandGameplayAction::Zap {
        index: Some(0),
        expected: Some(id),
        direction: Direction::East,
        energy: 6,
    }
}
fn comparable_state(w: &SimulationWorld) -> serde_json::Value {
    fn sort_registry(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(fields) => {
                for (key, value) in fields {
                    if key == "genocided_species" || key == "genocided_classes" {
                        value
                            .as_array_mut()
                            .unwrap()
                            .sort_by_cached_key(serde_json::Value::to_string);
                    } else {
                        sort_registry(value);
                    }
                }
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    sort_registry(value);
                }
            }
            _ => {}
        }
    }
    let mut value: serde_json::Value = serde_json::from_str(&w.to_save_json().unwrap()).unwrap();
    sort_registry(&mut value);
    value
}
fn differential(mut w: SimulationWorld, id: nethacked_arena::ItemId) {
    let b = bridge(&w);
    let reply = b.dispatch_wand(action(id)).unwrap();
    assert!(reply.error.is_none(), "{:?}", reply.error);
    let events = w.handle_zap_selected_wand(0, Some(id), Direction::East, 6);
    assert_eq!(reply.events, events);
    assert!(reply.dependency_calls > 0);
    let mut actual = b.export_native().unwrap();
    assert!(
        comparable_state(&actual) == comparable_state(&w),
        "state differs (genocide sets compared without unordered encoding)"
    );
    assert_eq!(actual.rng.next_u64(), w.rng.next_u64());
    let restored = OwnerBridge::reconstruct(&b.capture().unwrap(), b.config.clone()).unwrap();
    assert!(
        comparable_state(&restored.export_native().unwrap())
            == comparable_state(&b.export_native().unwrap())
    );
}
#[test]
fn native_gameplay_differential_beams_and_creation() {
    for (name, target) in [
        ("wand of fire", None),
        ("wand of striking", Some("lichen")),
        ("wand of death", Some("troll")),
        ("wand of polymorph", Some("lichen")),
        ("wand of create monster", None),
        ("wand of secret door detection", None),
    ] {
        let (w, id) = fixture(name, target);
        differential(w, id);
    }
}
#[test]
fn native_gameplay_empty_and_cancel() {
    let (mut w, id) = fixture("wand of fire", None);
    w.arena.items[id].enchantment = 0;
    w.locale = nethacked_types::Locale::Uk;
    differential(w, id);
    let (w, _) = fixture("wand of fire", None);
    let b = bridge(&w);
    let r = b.dispatch_wand(WandGameplayAction::Cancel).unwrap();
    assert_eq!(
        r.events,
        vec![crate::GameEvent::LogMessage {
            text: "Never mind.".into()
        }]
    );
    assert_eq!(r.dependency_calls, 0);
    assert!(b.export_native().unwrap().to_save_json().unwrap() == w.to_save_json().unwrap());
}
#[test]
fn native_gameplay_terrain_anger_polymorphed_and_custom() {
    use nethacked_core::engraving::{Engraving, EngravingMedium};
    let (mut w, id) = fixture("wand of teleportation", Some("giant ant"));
    let enemy = w
        .arena
        .actors
        .iter()
        .find_map(|(a, _)| (a != w.player_id).then_some(a))
        .unwrap();
    w.arena.actors[enemy].is_peaceful = true;
    w.arena.actors[enemy].hp = 1000;
    w.arena.actors[enemy].max_hp = 1000;
    w.level.set_engraving(
        Coord::new_unchecked(20, 10),
        Engraving::new("Elbereth", EngravingMedium::Burned),
    );
    differential(w, id);
    let (mut w, id) = fixture("wand of cold", None);
    w.level.tiles[21][10] = Tile::Pool { frozen: false };
    differential(w, id);
    let (mut w, id) = fixture("wand of striking", None);
    w.level.tiles[21][10] = Tile::Door {
        state: nethacked_types::DoorState::Closed,
        trapped: false,
    };
    differential(w, id);
    let (mut w, id) = fixture("wand of create monster", None);
    w.hero.polymorph = Some(nethacked_types::PolymorphForm {
        monster_id: 1,
        hp: 5,
        max_hp: 20,
        duration: 100,
    });
    differential(w, id);
    let (mut w, id) = fixture("wand of create monster", None);
    let mut rs = (*w.ruleset).clone();
    let mut def = rs.monster("lichen").unwrap().clone();
    def.name = "custom moss".into();
    def.id = None;
    rs.monsters = vec![def];
    rs.reindex();
    w.ruleset = Arc::new(rs);
    differential(w, id);
    let (mut w, id) = fixture("wand of fire", None);
    w.arena.items[id].definition = Some(nethacked_arena::ItemDefinition::Custom(
        "absent wand".into(),
    ));
    let before = w.to_save_json().unwrap();
    let json = before.clone();
    let config = OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
        .unwrap()
        .with_wand_gameplay();
    let b = OwnerBridge::import_native(&json, json.len(), config.clone()).unwrap();
    assert_eq!(
        comparable_state(&b.export_native().unwrap()),
        comparable_state(&w)
    );
    let restored = OwnerBridge::reconstruct(&b.capture().unwrap(), config.clone()).unwrap();
    restored.restore_native(&json, json.len()).unwrap();
    let reply = restored
        .dispatch_wand_action(nethacked_core::ActionAst::ZapSelectedWand {
            item_index: 0,
            expected_item: Some(id),
            dir: Direction::East,
            energy: 6,
        })
        .unwrap();
    assert!(reply.error.is_none());
    assert_eq!(
        comparable_state(&restored.export_native().unwrap()),
        comparable_state(&w)
    );
    for old in [
        OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone()).unwrap(),
        OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
            .unwrap()
            .with_wand_ports(),
        OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
            .unwrap()
            .with_wand_commands(),
    ] {
        assert!(matches!(
            OwnerBridge::import_native(&json, json.len(), old),
            Err(OwnerBridgeError::Migration(_))
        ));
    }
    let e = w.handle_zap_selected_wand(0, Some(id), Direction::East, 6);
    assert!(
        matches!(&e[..],[crate::GameEvent::LogMessage{text}] if text=="You cannot zap that wand: its definition is unavailable.")
    );
    assert_eq!(reply.events, e);
    assert!(w.to_save_json().unwrap() == before);
}

#[test]
fn native_gameplay_import_binding_and_registry_restrictions() {
    use crate::owner_persistence::{
        native_migration_binding, native_wand_gameplay_migration_binding,
    };
    let (mut w, id) = fixture("wand of fire", None);
    let base = OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone()).unwrap();
    let v3 = native_wand_gameplay_migration_binding();
    let strict = native_migration_binding();
    assert_ne!(v3, strict);
    for config in [
        base.clone()
            .with_native_import(NativeImportRegistry::default(), v3)
            .with_wand_gameplay(),
        base.clone()
            .with_wand_gameplay()
            .with_native_import(NativeImportRegistry::default(), v3),
        base.clone()
            .with_native_import(NativeImportRegistry::current(), v3)
            .with_wand_gameplay(),
        base.clone()
            .with_native_import(NativeImportRegistry::wand_gameplay_v3(), strict)
            .with_wand_gameplay(),
    ] {
        let json = w.to_save_json().unwrap();
        assert!(matches!(
            OwnerBridge::import_native(&json, json.len(), config),
            Err(OwnerBridgeError::Migration(_))
        ));
    }
    w.arena.items[id].definition = Some(nethacked_arena::ItemDefinition::Custom(
        "absent wand".into(),
    ));
    w.arena.items[id].quantity = 7;
    differential(w.clone(), id);
    let json = w.to_save_json().unwrap();
    assert!(OwnerBridge::import_native(
        &json,
        json.len(),
        base.clone()
            .with_native_import(NativeImportRegistry::current(), strict)
            .with_wand_gameplay()
    )
    .is_err());
    // Known identities still require the exact declared class in V3.
    w.arena.items[id].definition = Some(nethacked_arena::ItemDefinition::Vanilla(
        nethacked_data::ItemKindId::WAN_FIRE.c_name().into(),
    ));
    w.arena.items[id].class = nethacked_types::ItemClass::Food;
    let json = w.to_save_json().unwrap();
    assert!(matches!(
        OwnerBridge::import_native(&json, json.len(), base.with_wand_gameplay()),
        Err(OwnerBridgeError::Migration(_))
    ));
}
fn excluding(b: &OwnerBridge, key: &str) -> Scope {
    let bytes = b.capture().unwrap();
    let wire: serde_json::Value =
        serde_json::from_slice(&bytes[nethacked_persistence::HEADER_LEN..]).unwrap();
    let payload = wire["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "kernel")
        .unwrap()["local_payload"]
        .as_str()
        .unwrap();
    let payload = nethacked_persistence::Hex(payload.to_owned())
        .bytes(SaveLimits::default())
        .unwrap();
    let kernel: kp::KernelSnapshot = serde_json::from_slice(&payload).unwrap();
    let mut keys = BTreeSet::new();
    for grant in kernel.registry.grants {
        if let Scope::Keys(k) = grant.requested.scope {
            keys.extend(k);
        }
    }
    keys.retain(|r| r.key != key);
    Scope::Keys(keys)
}
#[test]
fn native_gameplay_draw_and_dependency_refusal_keep_exact_prefix() {
    let (w, id) = fixture("wand of death", Some("troll"));
    let b = bridge(&w);
    let r = b
        .dispatch_wand_scoped(action(id), excluding(&b, "9.wand_random_command_v2"))
        .unwrap();
    assert!(matches!(
        r.error,
        Some(GameplayError::Dependency { slot: 29, .. })
    ));
    assert_eq!(r.events.len(), 3);
    assert!(matches!(
        r.events[0],
        crate::GameEvent::BeamPropagated { .. }
    ));
    let a = b.export_native().unwrap();
    assert_eq!(a.arena.items[id].enchantment, 2);
    assert_eq!(a.scheduler.hero_energy, 120);
    assert_eq!(a.rng.get_word_pos(), w.rng.get_word_pos());
    assert_eq!(a.arena.items.len(), 1);
    assert!(a.progress.exp > w.progress.exp);
    assert!(a
        .arena
        .actors
        .iter()
        .any(|(id, a)| id != w.player_id && a.is_dead));
    let (w, id) = fixture("wand of fire", None);
    let b = bridge(&w);
    let r = b
        .dispatch_wand_scoped(action(id), excluding(&b, "5.wand_actors_gameplay_v3"))
        .unwrap();
    assert!(r.error.is_some());
    assert!(r.events.is_empty());
    assert!(b.export_native().unwrap().to_save_json().unwrap() == w.to_save_json().unwrap());
}
#[test]
fn native_gameplay_timer_failure_and_legacy_vec_policy() {
    let (mut w, id) = fixture("wand of death", Some("lichen"));
    w.object_timers =
        serde_json::from_value(serde_json::json!({"entries":[],"next_serial":u64::MAX})).unwrap();
    let b = bridge(&w);
    let r = b.dispatch_wand(action(id)).unwrap();
    assert!(matches!(
        r.error,
        Some(GameplayError::Timer(TimerFailure::SerialOverflow))
    ));
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        w.handle_zap_selected_wand(0, Some(id), Direction::East, 6)
    }))
    .unwrap_err();
    let text = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .unwrap();
    assert!(text.contains("death corpse deadline fits the world clock"));
    assert_eq!(r.events.len(), 4);
    assert!(matches!(
        r.events[0],
        crate::GameEvent::BeamPropagated { .. }
    ));
    assert!(matches!(
        r.events[1],
        crate::GameEvent::AttackLanded { lethal: true, .. }
    ));
    assert!(
        matches!(&r.events[2], crate::GameEvent::LogMessage { text } if text.contains("destroyed by the wand beam"))
    );
    assert!(
        matches!(&r.events[3], crate::GameEvent::LogMessage { text } if text == "lichen is killed!")
    );
    assert!(b.export_native().unwrap().to_save_json().unwrap() == w.to_save_json().unwrap());
    assert_eq!(w.arena.items.len(), 2);
    assert_eq!(w.scheduler.hero_energy, 120);
}
#[test]
fn native_gameplay_semantic_duplicate_refusal_keeps_rng_prefix() {
    let (mut w, _) = fixture("wand of fire", None);
    let b = bridge(&w);
    let read = b
        .observe_wand(WandRead {
            query: WandQuery::Random { draws: 1 },
            expected_owner: None,
        })
        .unwrap();
    let WandFacts::Random(reservation) = read.facts else {
        panic!()
    };
    let request = b
        .wand_operation(WandOwnerCommand::Sample {
            reservation,
            draw: SemanticDraw::SignedDie(7),
        })
        .unwrap();
    let OperationResult::Sample(n) = b.operate_wand(request.clone()).unwrap() else {
        panic!()
    };
    assert_eq!(
        n,
        crate::actions::wand::ports::WandDraw::SignedDie(7).sample(&mut w.rng)
    );
    assert!(b.operate_wand(request).is_err());
    let mut a = b.export_native().unwrap();
    assert_eq!(a.rng.get_word_pos(), w.rng.get_word_pos());
    assert_eq!(a.rng.next_u64(), w.rng.next_u64());
}

#[test]
fn native_gameplay_custom_item_and_paged_genocide() {
    let (mut w, id) = fixture("wand of fire", None);
    let mut rs = (*w.ruleset).clone();
    let mut item = rs.item("wand of fire").unwrap().clone();
    item.id = None;
    item.name = "custom fire baton".into();
    rs.items.push(item);
    rs.reindex();
    w.ruleset = Arc::new(rs);
    w.arena.items[id].definition = Some(nethacked_arena::ItemDefinition::Custom(
        "custom fire baton".into(),
    ));
    w.arena.items[id].name = "display alias".into();
    differential(w, id);
    let (mut w, id) = fixture("wand of create monster", None);
    for n in 0..35 {
        w.genocide_registry
            .genocided_species
            .insert(format!("unused species {n}"));
    }
    w.genocide_registry.genocided_classes.insert('F');
    differential(w, id);
}
#[test]
fn native_gameplay_old_profiles_and_missing_caller_refuse() {
    use sha2::{Digest, Sha256};
    let (w, _) = fixture("wand of fire", None);
    let new = bridge(&w);
    let config = new.config.clone();
    for profile in [
        OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone()).unwrap(),
        OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
            .unwrap()
            .with_wand_ports(),
        OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
            .unwrap()
            .with_wand_commands(),
    ] {
        let json = w.to_save_json().unwrap();
        let old = OwnerBridge::import_native(&json, json.len(), profile).unwrap();
        let saved = old.capture().unwrap();
        let restored = OwnerBridge::reconstruct(&saved, config.clone()).unwrap();
        assert!(
            restored.export_native().unwrap().to_save_json().unwrap() == w.to_save_json().unwrap()
        );
        assert!(restored.dispatch_wand(WandGameplayAction::Cancel).is_err());
    }
    let saved = new.capture().unwrap();
    let before = new.export_native().unwrap().to_save_json().unwrap();
    let mut outer: serde_json::Value =
        serde_json::from_slice(&saved[nethacked_persistence::HEADER_LEN..]).unwrap();
    let section = outer["sections"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|s| s["name"] == "kernel")
        .unwrap();
    let hex: nethacked_persistence::Hex =
        serde_json::from_value(section["local_payload"].clone()).unwrap();
    let mut k: serde_json::Value =
        serde_json::from_slice(&hex.bytes(SaveLimits::default()).unwrap()).unwrap();
    k["registry"]["modules"]
        .as_array_mut()
        .unwrap()
        .retain(|m| m["manifest"]["id"] != wand_gameplay::CALLER);
    section["local_payload"] = serde_json::to_value(nethacked_persistence::Hex::from_bytes(
        &serde_json::to_vec(&k).unwrap(),
    ))
    .unwrap();
    let body = serde_json::to_vec(&outer).unwrap();
    let mut framed = saved[..12].to_vec();
    framed.extend_from_slice(&(body.len() as u64).to_le_bytes());
    framed.extend_from_slice(&Sha256::digest(&body));
    framed.extend_from_slice(&body);
    FACTORY_BUILDS.with(|n| n.set(0));
    assert!(new.restore(&framed).is_err());
    FACTORY_BUILDS.with(|n| assert_eq!(n.get(), 0));
    assert!(new.export_native().unwrap().to_save_json().unwrap() == before);
}

mod review_repair;
