//! Native command witnesses for the opt-in ordinary, uncursed fire-wand slice.
use super::super::*;
use crate::owner_persistence::wand_ports::*;
use nethacked_arena::{ItemId, ItemLocation, ItemRecord};
use nethacked_core::ActionAst;
use nethacked_types::{Buc, Coord, Direction, Tile};
use rand::{Rng, SeedableRng};
fn target(id: ItemId) -> ObjectRef {
    ObjectRef {
        id,
        residency: Residency::Active,
    }
}
fn fixture(charge: i8, success: bool, opted: bool) -> (OwnerBridge, ItemId, OwnerBridgeConfig) {
    let mut w = SimulationWorld::new_with_seed(724);
    w.hero.skills.skills.clear();
    w.arena.items.clear();
    w.arena.actors.retain(|id, _| id == w.player_id);
    w.unpaid_items.clear();
    w.billing = Default::default();
    w.equipment = Default::default();
    w.level = nethacked_dungeon::DungeonLevel::new_solid(Tile::Room);
    w.arena.actors[w.player_id].coord = Coord::new_unchecked(20, 10);
    let item = w
        .ruleset
        .create_item_record(
            "wand of fire",
            ItemLocation::CarriedBy(w.player_id),
            Buc::Uncursed,
        )
        .unwrap();
    let id = w.arena.spawn_item(ItemRecord {
        enchantment: charge,
        quantity: if charge < 0 { 3 } else { 1 },
        ..item
    });
    w.wielded_item = Some(id);
    w.hero.quivered_item = Some(id);
    w.unpaid_items.push((id, 30));
    w.start_object_timer(
        crate::ObjectTimerTarget::Active(id),
        crate::ObjectTimerKind::RotCorpse,
        40,
    )
    .unwrap();
    if success {
        let mut actor = w
            .ruleset
            .create_monster_record("goblin", Coord::new_unchecked(21, 10), &mut w.rng)
            .unwrap();
        actor.hp = 96;
        actor.max_hp = 96;
        actor.speed = 0;
        actor.movement = 0;
        w.arena.spawn_actor(actor);
    }
    // Independent native stream oracle chooses a prescribed rn2 result.
    let seed = (0..10000)
        .find(|seed| {
            let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(*seed);
            rng.set_stream(17);
            (rng.random_range(0..121_u32) == 0) == success
        })
        .unwrap();
    w.rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
    w.rng.set_stream(17);
    let cfg = OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
        .unwrap()
        .with_item_lifetime();
    let cfg = if opted {
        cfg.with_wand_charge_attempts()
    } else {
        cfg
    };
    let json = w.to_save_json().unwrap();
    let b = OwnerBridge::import_native(&json, json.len(), cfg.clone()).unwrap();
    b.record_bill_terms(
        id,
        BillTerms {
            billed_quantity: Some(if charge < 0 { 3 } else { 1 }),
            unit_price: Some(40),
            creditor: None,
            legacy_base_cost: 30,
        },
    )
    .unwrap();
    (b, id, cfg)
}
fn zap(b: &OwnerBridge, id: ItemId, energy: u32) -> WandGameplayReply {
    b.dispatch_wand_action(ActionAst::ZapSelectedWand {
        item_index: 0,
        expected_item: Some(id),
        dir: Direction::East,
        energy,
    })
    .unwrap()
}
fn bytes(b: &OwnerBridge) -> serde_json::Value {
    serde_json::from_str(&b.export_native().unwrap().to_save_json().unwrap()).unwrap()
}
fn same(b: &OwnerBridge, expected: &serde_json::Value) {
    assert!(bytes(b) == *expected, "complete native state changed");
}
#[test]
fn zero_failure_commits_only_draw_and_refuses_stale_attempt() {
    let (b, id, _) = fixture(0, false, true);
    let object_witness = b
        .observe_wand(WandRead {
            query: WandQuery::Objects(ObjectRequest::Identity(target(id))),
            expected_owner: None,
        })
        .unwrap()
        .witness();
    let before = b.export_native().unwrap();
    let mut expected = before.rng.clone();
    assert_ne!(expected.random_range(0..121_u32), 0);
    let old = b
        .wand_operation(WandOwnerCommand::Objects(ObjectOperation::ChargeAttempt {
            target: target(id),
            expected: 0,
        }))
        .unwrap();
    let out = zap(&b, id, 0);
    assert!(out.error.is_none(), "{out:?}");
    let after = b.export_native().unwrap();
    assert_eq!(after.rng.get_word_pos(), expected.get_word_pos());
    assert_eq!(after.rng.get_stream(), 17);
    let mut scheduler = before.scheduler;
    scheduler.hero_act(nethacked_core::energy::NORMAL_SPEED);
    assert_eq!(
        serde_json::to_value(after.scheduler).unwrap(),
        serde_json::to_value(scheduler).unwrap()
    );
    assert_eq!(
        b.observe_wand(WandRead {
            query: WandQuery::Objects(ObjectRequest::Identity(target(id))),
            expected_owner: None
        })
        .unwrap()
        .witness(),
        object_witness
    );
    assert_eq!(after.arena.items[id], before.arena.items[id]);
    assert_eq!(after.billing, before.billing);
    let stable = bytes(&b);
    assert!(b.operate_wand(old).is_err());
    same(&b, &stable);
}
#[test]
fn same_selected_wrest_effect_retirement_receipt_and_reload() {
    let (b, id, cfg) = fixture(0, true, true);
    let before = b.export_native().unwrap();
    let mut expected = before.rng.clone();
    assert_eq!(expected.random_range(0..121_u32), 0);
    let old = b
        .wand_operation(WandOwnerCommand::Objects(ObjectOperation::ChargeAttempt {
            target: target(id),
            expected: 0,
        }))
        .unwrap();
    let out = zap(&b, id, 6);
    assert!(out.error.is_none(), "{out:?}");
    assert!(
        out.events
            .iter()
            .any(|e| matches!(e, crate::GameEvent::BeamPropagated { .. })),
        "{:?}",
        out.events
    );
    let logs = out
        .events
        .iter()
        .filter_map(|e| {
            if let crate::GameEvent::LogMessage { text } = e {
                Some(text.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        logs.first(),
        Some(&"You wrest one last charge from the worn-out wand.")
    );
    assert_eq!(logs.last(), Some(&"The wand of fire turns to dust."));
    let after = b.export_native().unwrap();
    assert!(after.arena.items.get(id).is_none());
    let mut scheduler = before.scheduler;
    scheduler.hero_act(nethacked_core::energy::NORMAL_SPEED);
    assert_eq!(
        serde_json::to_value(after.scheduler).unwrap(),
        serde_json::to_value(scheduler).unwrap()
    );
    assert!(after.wielded_item.is_none() && after.hero.quivered_item.is_none());
    assert!(after
        .object_timers
        .entries()
        .iter()
        .all(|t| t.target != crate::ObjectTimerTarget::Active(id)));
    assert_eq!(
        after
            .arena
            .actors
            .iter()
            .find(|(actor, _)| *actor != after.player_id)
            .unwrap()
            .1
            .hp,
        84
    );
    assert!(after.unpaid_items.is_empty() && after.billing.live_terms.is_empty());
    assert_eq!(after.billing.consumed.len(), 1);
    assert_eq!(after.billing.consumed[0].object.enchantment, -1);
    assert_eq!(
        after.billing.consumed[0].object.definition,
        before.arena.items[id].definition
    );
    let stable = bytes(&b);
    assert!(b.operate_wand(old).is_err());
    assert!(b
        .record_bill_terms(id, after.billing.consumed[0].terms.clone())
        .is_err());
    same(&b, &stable);
    let captured = b.capture().unwrap();
    let restored = OwnerBridge::reconstruct(&captured, cfg.clone()).unwrap();
    same(&restored, &stable);
    let json = serde_json::to_string(&stable).unwrap();
    let native = OwnerBridge::import_native(&json, json.len(), cfg).unwrap();
    same(&native, &stable);
}
#[test]
fn nonzero_charge_does_not_draw_and_default_is_isolated() {
    for charge in [1, -1] {
        let (b, id, _) = fixture(charge, false, true);
        let before = b.export_native().unwrap();
        let out = zap(&b, id, 0);
        assert!(out.error.is_none(), "{out:?}");
        let after = b.export_native().unwrap();
        assert_eq!(after.rng.get_word_pos(), before.rng.get_word_pos());
        if charge > 0 {
            assert_eq!(after.arena.items[id].enchantment, 0);
        } else {
            assert!(after.arena.items.get(id).is_none());
            assert!(out.events.iter().any(|e|matches!(e,crate::GameEvent::LogMessage{text} if text=="The wands of fire turn to dust.")));
        }
    }
    for charge in [0, -1] {
        let (b, id, _) = fixture(charge, true, false);
        let before = b.export_native().unwrap();
        let out = zap(&b, id, 0);
        assert!(out.error.is_none());
        let after = b.export_native().unwrap();
        assert_eq!(after.arena.items[id], before.arena.items[id]);
        assert_eq!(after.rng.get_word_pos(), before.rng.get_word_pos());
        assert!(b
            .wand_operation(WandOwnerCommand::Objects(ObjectOperation::ChargeAttempt {
                target: target(id),
                expected: charge
            }))
            .is_err());
    }
}
#[test]
fn unsupported_cursed_and_vertical_refuse_without_draw() {
    let (b, id, cfg) = fixture(0, true, true);
    let mut w = b.export_native().unwrap();
    w.arena.items[id].buc = Buc::Cursed;
    let json = w.to_save_json().unwrap();
    let b = OwnerBridge::import_native(&json, json.len(), cfg).unwrap();
    let before = bytes(&b);
    let out = zap(&b, id, 6);
    assert!(out.error.is_some());
    same(&b, &before);
    assert!(b
        .wand_operation(WandOwnerCommand::Objects(ObjectOperation::ChargeAttempt {
            target: target(id),
            expected: 0
        }))
        .is_err());
    same(&b, &before);
    let (b, id, _) = fixture(0, true, true);
    let before = bytes(&b);
    let out = b
        .dispatch_wand_action(ActionAst::ZapSelectedWand {
            item_index: 0,
            expected_item: Some(id),
            dir: Direction::Up,
            energy: 6,
        })
        .unwrap();
    assert!(out.error.is_some());
    same(&b, &before);
}

#[test]
fn historical_live_workflow_checkpoint_keeps_old_policy_through_charge_facade() {
    let (b, id, cfg) = fixture(0, true, false);
    let old_cfg = cfg
        .with_wand_extensions(vec![])
        .unwrap()
        .with_wand_workflows()
        .unwrap()
        .with_wand_workflow_replies()
        .unwrap();
    let json = serde_json::to_string(&bytes(&b)).unwrap();
    let old = OwnerBridge::import_native(&json, json.len(), old_cfg.clone()).unwrap();
    let ticket = old
        .pause_wand(WandGameplayAction::Zap {
            index: Some(0),
            expected: Some(id),
            direction: Direction::East,
            energy: 6,
        })
        .unwrap();
    let approval = old.approve_live_wands().unwrap();
    let saved = old.capture().unwrap();
    let new_cfg = old_cfg
        .with_wand_charge_attempts()
        .with_wand_workflow_approval(approval)
        .unwrap();
    let restored = OwnerBridge::reconstruct(&saved, new_cfg).unwrap();
    let fresh = restored.pending_wands().unwrap().remove(0);
    assert_ne!(fresh, ticket);
    let before = restored.export_native().unwrap();
    assert!(restored.resume_wand(&fresh).unwrap());
    let after = restored.export_native().unwrap();
    assert_eq!(after.arena.items[id].enchantment, 0);
    assert_eq!(after.rng.get_word_pos(), before.rng.get_word_pos());
}
