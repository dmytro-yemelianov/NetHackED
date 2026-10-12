use super::*;
use crate::owner_persistence::{OwnerBridge, OwnerBridgeConfig, OwnerBridgeError};
use crate::world::BillTerms;
use crate::{ObjectTimerKind, ObjectTimerTarget};
use nethacked_arena::ItemLocation;
use nethacked_types::Buc;

fn world() -> SimulationWorld {
    let mut world = SimulationWorld::new_with_seed(804);
    world.arena.actors.retain(|id, _| id == world.player_id);
    world.arena.items.clear();
    world.equipment = Default::default();
    world.wielded_item = None;
    world.hero.skills.skills.clear();
    world
}
fn prepare(
    world: &SimulationWorld,
    json: &str,
) -> Result<PreparedNativeImport, MigrationError<LoadError>> {
    NativeImportRegistry::current().prepare(
        native_migration_binding(),
        json,
        json.len(),
        world.ruleset.clone(),
        &world.ruleset_ref,
    )
}

#[test]
fn real_legacy_absence_policies_migrate_through_registry_and_match_native_load() {
    let world = world();
    let mut value = serde_json::to_value(&world).unwrap();
    for key in ["equipment", "object_timers", "billing"] {
        value.as_object_mut().unwrap().remove(key);
    }
    value["unknown_legacy_field"] = true.into();
    let json = value.to_string();
    TRANSFORMS.with(|n| n.set(0));
    let prepared = prepare(&world, &json).unwrap();
    assert_eq!(TRANSFORMS.with(|n| n.get()), 1);
    let expected =
        SimulationWorld::from_save_json(&json, world.ruleset.clone(), &world.ruleset_ref).unwrap();
    assert_eq!(
        serde_json::to_value(prepared.complete().unwrap()).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
    let config = OwnerBridgeConfig::new(world.ruleset.clone(), world.ruleset_ref.clone()).unwrap();
    let source = OwnerBridge::import_native(&json, json.len(), config.clone()).unwrap();
    let saved = source.capture().unwrap();
    drop(source);
    let restored = OwnerBridge::reconstruct(&saved, config).unwrap();
    let expected =
        SimulationWorld::from_save_json(&json, world.ruleset.clone(), &world.ruleset_ref).unwrap();
    assert_eq!(
        serde_json::to_value(restored.export_native().unwrap()).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
}

#[test]
fn malformed_real_receipt_queue_or_ruleset_never_invokes_available_migrator() {
    let mut world = world();
    let item = world
        .ruleset
        .create_item_record(
            "wand of fire",
            ItemLocation::CarriedBy(world.player_id),
            Buc::Uncursed,
        )
        .unwrap();
    let id = world.arena.spawn_item(item);
    world.unpaid_items.push((id, 30));
    world
        .record_bill_terms(
            id,
            BillTerms {
                billed_quantity: Some(1),
                unit_price: Some(30),
                legacy_base_cost: 30,
                creditor: None,
            },
        )
        .unwrap();
    let corpse = world
        .ruleset
        .create_corpse_record("human", ItemLocation::Limbo)
        .unwrap();
    let corpse = world.arena.spawn_item(corpse);
    world
        .start_object_timer(
            ObjectTimerTarget::Active(corpse),
            ObjectTimerKind::RotCorpse,
            0,
        )
        .unwrap();
    let original = serde_json::to_value(&world).unwrap();
    for case in 0..3 {
        let mut value = original.clone();
        match case {
            0 => value["billing"]["live_terms"][0][1]["billed_quantity"] = 0.into(),
            1 => {
                let entry = value["object_timers"]["entries"][0].clone();
                value["object_timers"]["entries"]
                    .as_array_mut()
                    .unwrap()
                    .push(entry);
            }
            2 => value["ruleset_ref"]["hash"] = "wrong".into(),
            _ => unreachable!(),
        }
        TRANSFORMS.with(|n| n.set(0));
        assert!(matches!(
            prepare(&world, &value.to_string()),
            Err(MigrationError::Source(_))
        ));
        assert_eq!(TRANSFORMS.with(|n| n.get()), 0);
        assert_eq!(serde_json::to_value(&world).unwrap(), original);
    }
    // Positive control: migration runs once; due work is still undispatched.
    TRANSFORMS.with(|n| n.set(0));
    let prepared = prepare(&world, &original.to_string()).unwrap();
    assert_eq!(TRANSFORMS.with(|n| n.get()), 1);
    assert!(prepared.world.arena.items.contains_key(corpse));
    let completed = prepared.complete().unwrap();
    assert!(!completed.arena.items.contains_key(corpse));
    assert!(completed.object_timers.entries().is_empty());
}

#[test]
fn public_bridge_refuses_unapproved_parser_and_changed_migration_before_decoding() {
    let world = world();
    let config = OwnerBridgeConfig::new(world.ruleset.clone(), world.ruleset_ref.clone()).unwrap();
    for empty in [true, false] {
        let mut binding = native_migration_binding();
        let registry = if empty {
            NativeImportRegistry::default()
        } else {
            binding.transform_version += 1;
            NativeImportRegistry::current()
        };
        TRANSFORMS.with(|n| n.set(0));
        let result = OwnerBridge::import_native(
            "{",
            1,
            config.clone().with_native_import(registry, binding),
        );
        assert!(matches!(
            result,
            Err(OwnerBridgeError::Migration(MigrationError::Unavailable))
        ));
        assert_eq!(TRANSFORMS.with(|n| n.get()), 0);
    }
}
