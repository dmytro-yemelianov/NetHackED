//! Aggregate compatibility witness for the supported production commands.
//! Positive-only zaps cannot produce a negative survivor: retirement below uses
//! a separate, already negative object. This does not prove wrest/dust parity.
use super::super::*;
use crate::{
    actions::equipment::Dressing, owner_persistence::wand_ports::*, ObjectTimerKind,
    ObjectTimerTarget,
};
use nethacked_arena::{ItemId, ItemLocation, ItemRecord};
use nethacked_core::{ac::ArmorSlot, ActionAst};
use nethacked_kernel::{Access, Capability, Endpoint, Extension, Manifest};
use nethacked_types::{Buc, Coord, Direction, Tile};
use rand::{RngCore, SeedableRng};
use std::collections::BTreeSet;
fn guest() -> WandGuest {
    let endpoint = Endpoint {
        major: 1,
        access: Access::Read,
        max_request: 8192,
        max_reply: 8192,
    };
    let manifest = Manifest {
        id: "wand-cancel-extension".into(),
        exports: [("run".into(), endpoint)].into(),
        dependencies: vec![],
        extension_points: BTreeSet::new(),
        extension: Some(Extension {
            target: Capability {
                provider: "native-wand-action-policy-i4".into(),
                endpoint: "select_action_i4".into(),
                major: 1,
            },
            export: "run".into(),
        }),
    };
    let code = wat::parse_str(
        r#"(module
      (memory (export "memory") 1)
      (data (i32.const 8192) "\22Cancel\22")
      (func (export "alloc") (param i32) (result i32) i32.const 0)
      (func (export "run") (param i32 i32) (result i64) i64.const 35184372088840))"#,
    )
    .unwrap();
    WandGuest::approve(manifest, code).unwrap()
}

fn bytes(b: &OwnerBridge) -> String {
    b.export_native().unwrap().to_save_json().unwrap()
}
fn target(id: ItemId) -> ObjectRef {
    ObjectRef {
        id,
        residency: Residency::Active,
    }
}
fn action(id: ItemId, index: u32) -> ActionAst {
    ActionAst::ZapSelectedWand {
        item_index: index,
        expected_item: Some(id),
        dir: Direction::East,
        energy: 6,
    }
}

#[test]
fn aggregate_retained_effect_extension_receipt_cleanup_and_native_compatibility() {
    let mut world = SimulationWorld::new_with_seed(724);
    world.hero.skills.skills.clear();
    world.arena.items.clear();
    world.arena.actors.retain(|id, _| id == world.player_id);
    world.unpaid_items.clear();
    world.billing = Default::default();
    world.equipment = Default::default();
    world.level = nethacked_dungeon::DungeonLevel::new_solid(Tile::Room);
    world.arena.actors[world.player_id].coord = Coord::new_unchecked(20, 10);
    let make = |name: &str| {
        world
            .ruleset
            .create_item_record(
                name,
                ItemLocation::CarriedBy(world.player_id),
                Buc::Uncursed,
            )
            .unwrap()
    };
    let selected = world.arena.spawn_item(ItemRecord {
        enchantment: 3,
        ..make("wand of striking")
    });
    let retired = world.arena.spawn_item(ItemRecord {
        enchantment: -1,
        quantity: 3,
        ..make("wand of fire")
    });
    let unrelated = world.arena.spawn_item(make("helmet"));
    let dressing = world.arena.spawn_item(make("ring mail"));
    let selected_index = world
        .arena
        .items
        .iter()
        .filter(|(_, item)| item.location == ItemLocation::CarriedBy(world.player_id))
        .position(|(id, _)| id == selected)
        .unwrap() as u32;
    world.wielded_item = Some(retired);
    world.hero.quivered_item = Some(retired);
    world.equipment.armor = vec![(ArmorSlot::Helmet, unrelated), (ArmorSlot::Suit, dressing)];
    world.equipment.dressing = Some(Dressing {
        item: dressing,
        putting_on: true,
        turns_left: 3,
    });
    world.unpaid_items.push((retired, 30));
    for (id, kind) in [
        (unrelated, ObjectTimerKind::RotCorpse),
        (retired, ObjectTimerKind::RotCorpse),
        (unrelated, ObjectTimerKind::ReviveMonster),
        (retired, ObjectTimerKind::ReviveMonster),
    ] {
        world
            .start_object_timer(ObjectTimerTarget::Active(id), kind, 40)
            .unwrap();
    }
    // A real beam target makes effects and draw-prefix assertions non-vacuous.
    let mut actor = world
        .ruleset
        .create_monster_record("goblin", Coord::new_unchecked(21, 10), &mut world.rng)
        .unwrap();
    actor.hp = 1;
    actor.max_hp = 1;
    actor.movement = 0;
    actor.speed = 0;
    world.arena.spawn_actor(actor);
    world.rng = rand_chacha::ChaCha8Rng::seed_from_u64(100);
    let cfg = OwnerBridgeConfig::new(world.ruleset.clone(), world.ruleset_ref.clone())
        .unwrap()
        .with_item_lifetime()
        .with_wand_extensions(vec![guest()])
        .unwrap()
        .with_wand_workflows()
        .unwrap();
    let json = world.to_save_json().unwrap();
    let source = OwnerBridge::import_native(&json, json.len(), cfg.clone()).unwrap();
    let terms = BillTerms {
        billed_quantity: Some(3),
        unit_price: Some(40),
        creditor: None,
        legacy_base_cost: 30,
    };
    source.record_bill_terms(retired, terms.clone()).unwrap();
    // Native import applies the established equipment/timer normalization.
    // Use that actual production checkpoint for the borrowed legacy oracle.
    world = source.export_native().unwrap();
    assert!(world.equipment.dressing.is_some());
    let before = bytes(&source);
    assert!(source.record_bill_terms(retired, terms.clone()).is_err());
    assert_eq!(bytes(&source), before);
    let stale_request = source
        .wand_operation(WandOwnerCommand::Objects(ObjectOperation::PositiveCharge {
            target: target(selected),
            expected: 3,
        }))
        .unwrap();
    let read = WandQuery::Objects(ObjectRequest::Identity(target(selected)));
    let old_observation = source
        .observe_wand(WandRead {
            query: read.clone(),
            expected_owner: None,
        })
        .unwrap();
    let ticket = source
        .pause_wand(WandGameplayAction::Zap {
            index: Some(selected_index),
            expected: Some(selected),
            direction: Direction::East,
            energy: 6,
        })
        .unwrap();
    let approval = source.approve_live_wands().unwrap();
    source
        .attach_wand_extension("wand-cancel-extension")
        .unwrap();
    assert_eq!(bytes(&source), before);
    let saved = source.capture().unwrap();
    drop(source);
    let restored = OwnerBridge::reconstruct(
        &saved,
        cfg.clone().with_wand_workflow_approval(approval).unwrap(),
    )
    .unwrap();
    let before = bytes(&restored);
    assert!(restored.resume_wand(&ticket).is_err());
    assert_eq!(bytes(&restored), before);
    let fresh = restored.pending_wands().unwrap().remove(0);
    assert!(restored.resume_wand(&fresh).unwrap());
    let legacy_events =
        world.handle_zap_selected_wand(selected_index, Some(selected), Direction::East, 6);
    assert!(
        legacy_events
            .iter()
            .any(|e| matches!(e, crate::GameEvent::BeamPropagated { .. })),
        "{legacy_events:?}"
    );
    assert!(world.rng.get_word_pos() > 0);
    assert_eq!(bytes(&restored), world.to_save_json().unwrap());
    assert_eq!(world.arena.items[selected].enchantment, 2);
    let before = bytes(&restored);
    assert!(restored.resume_wand(&fresh).is_err());
    assert!(restored.operate_wand(stale_request).is_err());
    assert!(restored
        .observe_wand(WandRead {
            query: read,
            expected_owner: Some(old_observation.witness())
        })
        .is_err());
    assert_eq!(bytes(&restored), before);
    // A future command uses the executable guest, with the legacy event order.
    let cancel = restored
        .dispatch_wand_action(action(selected, selected_index))
        .unwrap();
    assert!(cancel.error.is_none());
    assert_eq!(
        cancel.events,
        vec![crate::GameEvent::LogMessage {
            text: "Never mind.".into()
        }]
    );
    assert_eq!(bytes(&restored), before);
    // Detaching routes the next real effect through the shared legacy algorithm.
    restored
        .detach_wand_extension("wand-cancel-extension")
        .unwrap();
    let next = restored
        .dispatch_wand_action(action(selected, selected_index))
        .unwrap();
    assert!(next.error.is_none());
    let events = world.handle_zap_selected_wand(selected_index, Some(selected), Direction::East, 6);
    assert_eq!(next.events, events);
    assert_eq!(bytes(&restored), world.to_save_json().unwrap());
    let before_retirement = restored.export_native().unwrap();
    let mut detached = before_retirement.arena.items[retired].clone();
    detached.location = ItemLocation::Limbo;
    assert_eq!(
        restored
            .finalize_retained_object(Some(target(retired)))
            .unwrap(),
        Ok(LifetimeReply::Retired { billed: true })
    );
    let after = restored.export_native().unwrap();
    assert!(after.arena.items.get(retired).is_none());
    assert_eq!(
        after.arena.items[selected],
        before_retirement.arena.items[selected]
    );
    assert_eq!(
        after.arena.items[unrelated],
        before_retirement.arena.items[unrelated]
    );
    assert_eq!(after.equipment, before_retirement.equipment);
    assert!(after.wielded_item.is_none() && after.hero.quivered_item.is_none());
    assert!(after.unpaid_items.is_empty() && after.billing.live_terms.is_empty());
    assert_eq!(after.billing.consumed.len(), 1);
    assert_eq!(after.billing.consumed[0].object, detached);
    assert_eq!(after.billing.consumed[0].terms, terms);
    let kept = before_retirement
        .object_timers
        .entries()
        .iter()
        .filter(|t| t.target != ObjectTimerTarget::Active(retired))
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(after.object_timers.entries(), kept);
    assert_eq!(
        after.rng.get_word_pos(),
        before_retirement.rng.get_word_pos()
    );
    assert_eq!(
        serde_json::to_value(after.scheduler).unwrap(),
        serde_json::to_value(before_retirement.scheduler).unwrap()
    );
    let before = bytes(&restored);
    assert_eq!(
        restored
            .finalize_retained_object(Some(target(retired)))
            .unwrap(),
        Ok(LifetimeReply::Unchanged)
    );
    assert!(restored.record_bill_terms(retired, terms).is_err());
    assert_eq!(bytes(&restored), before);
    let approval = restored.approve_live_wands().unwrap();
    let saved = restored.capture().unwrap();
    drop(restored);
    let reloaded = OwnerBridge::reconstruct(
        &saved,
        cfg.clone().with_wand_workflow_approval(approval).unwrap(),
    )
    .unwrap();
    assert_eq!(bytes(&reloaded), before);
    let native = OwnerBridge::import_native(&before, before.len(), cfg).unwrap();
    // Legacy native loading reinserts tied timers before equals; owner checkpoint
    // restore preserves exact owner queue order. Verify both complete states.
    let mut native_reference =
        SimulationWorld::from_save_json(&before, world.ruleset.clone(), &world.ruleset_ref)
            .unwrap();
    assert_eq!(bytes(&native), native_reference.to_save_json().unwrap());
    assert_ne!(
        native_reference.object_timers.entries(),
        after.object_timers.entries()
    );
    // Actual authenticated raw words continue the same post-effect stream.
    let mut reference = after;
    for _ in 0..4 {
        let expected = reference.rng.next_u64();
        assert_eq!(native_reference.rng.next_u64(), expected);
        assert_eq!(reloaded.wand_rng().next_u64(), expected);
        assert_eq!(native.wand_rng().next_u64(), expected);
    }
    assert_eq!(bytes(&reloaded), reference.to_save_json().unwrap());
    assert_eq!(bytes(&native), native_reference.to_save_json().unwrap());
}
