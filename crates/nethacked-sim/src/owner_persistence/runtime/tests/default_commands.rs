use super::super::*;
use nethacked_arena::{ItemId, ItemLocation};
use nethacked_core::{ac::ArmorSlot, ActionAst};
use nethacked_types::Buc;
fn fixture() -> (OwnerBridge, OwnerBridgeConfig, ItemId) {
    let mut w = SimulationWorld::new_with_seed(721);
    w.hero.skills.skills.clear();
    w.arena.items.clear();
    w.arena.actors.retain(|id, _| id == w.player_id);
    w.equipment = Default::default();
    w.wielded_item = None;
    w.unpaid_items.clear();
    w.billing = Default::default();
    let item = w
        .ruleset
        .create_item_record(
            "helmet",
            ItemLocation::CarriedBy(w.player_id),
            Buc::Uncursed,
        )
        .unwrap();
    let id = w.arena.spawn_item(item);
    w.equipment.armor = vec![(ArmorSlot::Helmet, id)];
    w.divine_protection = 2;
    w.max_event_log_len = Some(1);
    w.record_events(&[crate::GameEvent::LogMessage { text: "old".into() }]);
    let cfg = OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
        .unwrap()
        .with_default_commands();
    let json = w.to_save_json().unwrap();
    (
        OwnerBridge::import_native(&json, json.len(), cfg.clone()).unwrap(),
        cfg,
        id,
    )
}
fn value(b: &OwnerBridge) -> serde_json::Value {
    serde_json::from_str(&b.export_native().unwrap().to_save_json().unwrap()).unwrap()
}
#[test]
fn populated_default_cancel_matches_legacy_and_reloads() {
    let (b, cfg, _) = fixture();
    let mut legacy = b.export_native().unwrap();
    let before = legacy.rng.get_word_pos();
    let scheduler = serde_json::to_value(legacy.scheduler).unwrap();
    let events = legacy.step_player_action(ActionAst::CancelZapSelection);
    let result = b
        .dispatch_default_action(ActionAst::CancelZapSelection)
        .unwrap();
    assert!(result.error.is_none());
    assert_eq!(result.events, events);
    assert!(
        value(&b)
            == serde_json::from_str::<serde_json::Value>(&legacy.to_save_json().unwrap()).unwrap()
    );
    let after = b.export_native().unwrap();
    assert_eq!(after.rng.get_word_pos(), before);
    assert_eq!(serde_json::to_value(after.scheduler).unwrap(), scheduler);
    let saved = b.capture().unwrap();
    let restored = OwnerBridge::reconstruct(&saved, cfg.clone()).unwrap();
    assert!(value(&restored) == value(&b));
    let json = after.to_save_json().unwrap();
    let native = OwnerBridge::import_native(&json, json.len(), cfg).unwrap();
    assert!(value(&native) == value(&b));
}
#[test]
fn authentic_request_replay_and_unsupported_actions_preserve_state() {
    let (b, _, _) = fixture();
    let req = b.prepare_default_cancel().unwrap();
    let before = value(&b);
    assert!(b.dispatch_default_action(ActionAst::Wait).is_err());
    assert!(value(&b) == before);
    b.execute_default_cancel(req.clone()).unwrap();
    let after = value(&b);
    assert!(b.execute_default_cancel(req).is_err());
    assert!(value(&b) == after);
}
#[test]
fn helpless_and_valid_dressing_refuse_while_dead_is_exact_noop() {
    let (b, cfg, _) = fixture();
    for dead in [false, true] {
        let mut w = b.export_native().unwrap();
        w.hero.afflictions.transient.helpless = 2;
        w.arena.actors[w.player_id].is_dead = dead;
        let json = w.to_save_json().unwrap();
        let host = OwnerBridge::import_native(&json, json.len(), cfg.clone()).unwrap();
        let before = value(&host);
        let result = host.dispatch_default_action(ActionAst::CancelZapSelection);
        if dead {
            assert!(result.unwrap().events.is_empty());
        } else {
            assert!(result.is_err());
        }
        assert!(value(&host) == before);
    }
}
#[test]
fn historical_profile_reconstructs_without_new_command_authority() {
    let (b, cfg, _) = fixture();
    let json = b.export_native().unwrap().to_save_json().unwrap();
    let mut old_cfg = cfg.clone();
    old_cfg.default_commands = false;
    let old = OwnerBridge::import_native(&json, json.len(), old_cfg).unwrap();
    let saved = old.capture().unwrap();
    let restored = OwnerBridge::reconstruct(&saved, cfg).unwrap();
    assert!(restored
        .dispatch_default_action(ActionAst::CancelZapSelection)
        .is_err());
    assert!(value(&restored) == value(&old));
}
#[test]
fn shared_stale_equipment_and_dressing_preview_preserves_law() {
    use crate::actions::{
        command_frame as law,
        equipment::{Dressing, EquipmentState},
    };
    let (b, _, id) = fixture();
    let mut w = b.export_native().unwrap();
    let input = EquipmentState {
        armor: vec![(ArmorSlot::Helmet, id), (ArmorSlot::Helmet, id)],
        dressing: Some(Dressing {
            item: id,
            putting_on: true,
            turns_left: 4,
        }),
    };
    let borrowed = law::BorrowedObjects {
        items: &w.arena.items,
        rules: &w.ruleset,
    };
    let expected = law::reconcile(&input, &borrowed, w.player_id);
    assert_eq!(expected.armor, vec![(ArmorSlot::Helmet, id)]);
    assert!(expected.dressing.is_none());
    w.equipment = input;
    let events = w.step_player_action(ActionAst::CancelZapSelection);
    assert_eq!(w.equipment, expected);
    assert_eq!(
        events,
        vec![crate::GameEvent::LogMessage {
            text: "Never mind.".into()
        }]
    );
}

#[test]
fn actual_populated_owner_stale_refs_and_ac_use_preview_reconciliation() {
    use crate::actions::equipment::Dressing;
    let (b, cfg, id) = fixture();
    let mut w = b.export_native().unwrap();
    w.equipment.armor.push((ArmorSlot::Helmet, id));
    w.equipment.dressing = Some(Dressing {
        item: id,
        putting_on: true,
        turns_left: 4,
    });
    w.arena.actors[w.player_id].ac = 99;
    let mut legacy = w.clone();
    let host = OwnerBridge::install(NativePartitions::from_native(w), cfg).unwrap();
    let events = legacy.step_player_action(ActionAst::CancelZapSelection);
    let out = host
        .dispatch_default_action(ActionAst::CancelZapSelection)
        .unwrap();
    assert!(out.error.is_none());
    assert_eq!(out.events, events);
    assert!(
        value(&host)
            == serde_json::from_str::<serde_json::Value>(&legacy.to_save_json().unwrap()).unwrap()
    );
}
#[test]
fn actual_valid_dressing_refuses_and_missing_hero_returns_without_publication() {
    use crate::actions::equipment::Dressing;
    let (b, cfg, _) = fixture();
    let mut w = b.export_native().unwrap();
    let item = w
        .ruleset
        .create_item_record(
            "ring mail",
            ItemLocation::CarriedBy(w.player_id),
            Buc::Uncursed,
        )
        .unwrap();
    let suit = w.arena.spawn_item(item);
    w.equipment.armor.push((ArmorSlot::Suit, suit));
    w.equipment.dressing = Some(Dressing {
        item: suit,
        putting_on: true,
        turns_left: 4,
    });
    let host = OwnerBridge::install(NativePartitions::from_native(w.clone()), cfg.clone()).unwrap();
    let before = value(&host);
    assert!(host
        .dispatch_default_action(ActionAst::CancelZapSelection)
        .is_err());
    assert!(value(&host) == before);
    w.arena.actors.remove(w.player_id);
    let mut legacy = w.clone();
    assert!(legacy
        .step_player_action(ActionAst::CancelZapSelection)
        .is_empty());
    let host = OwnerBridge::install(NativePartitions::from_native(w), cfg).unwrap();
    let snapshots = || {
        host.session
            .with_runtime(&host.credential.borrow(), |b| {
                Ok::<_, owners::Error>((
                    b.owner::<ActorsState>("Actors").unwrap().owner.inspect()?,
                    b.owner::<EquipmentState>("Equipment")
                        .unwrap()
                        .owner
                        .inspect()?,
                    b.owner::<SessionState>("Session")
                        .unwrap()
                        .owner
                        .inspect()?,
                    b.host().history()?,
                ))
            })
            .unwrap()
            .unwrap()
    };
    let before = snapshots();
    assert!(host
        .dispatch_default_action(ActionAst::CancelZapSelection)
        .unwrap()
        .events
        .is_empty());
    assert_eq!(snapshots(), before);
}

#[test]
fn session_authority_refusal_preserves_committed_equipment_and_ac_prefix() {
    let (b, cfg, id) = fixture();
    let mut w = b.export_native().unwrap();
    w.equipment.armor.push((ArmorSlot::Helmet, id));
    w.arena.actors[w.player_id].ac = 99;
    let mut expected = w.clone();
    let old_log = expected.event_log.clone();
    let events = expected.step_player_action(ActionAst::CancelZapSelection);
    expected.event_log = old_log;
    let host = OwnerBridge::install(NativePartitions::from_native(w), cfg.clone()).unwrap();
    let request = host.prepare_default_cancel().unwrap();
    let scope = host
        .session
        .with_runtime(&host.credential.borrow(), |b| {
            let mut keys = BTreeSet::from([b.host().publication_resource()]);
            for (owner, name) in [
                ("Hero", "frame_hero_i1"),
                ("Objects", "frame_items_i1"),
                ("Actors", "frame_actor_i1"),
                ("Actors", "frame_ac_i1"),
                ("Equipment", "frame_equipment_i1"),
                ("Equipment", "frame_reconcile_i1"),
            ] {
                let resource = match owner {
                    "Hero" => b.owner::<HeroState>(owner).unwrap().owner.resource(name)?,
                    "Objects" => b
                        .owner::<ObjectsState>(owner)
                        .unwrap()
                        .owner
                        .resource(name)?,
                    "Actors" => b
                        .owner::<ActorsState>(owner)
                        .unwrap()
                        .owner
                        .resource(name)?,
                    "Equipment" => b
                        .owner::<EquipmentState>(owner)
                        .unwrap()
                        .owner
                        .resource(name)?,
                    _ => unreachable!(),
                };
                keys.insert(resource);
            }
            Ok::<_, owners::Error>(Scope::Keys(keys))
        })
        .unwrap()
        .unwrap();
    // Real delegated authority permits the frame state, excludes Session record.
    let reply = host.execute_default_cancel_scoped(request, scope).unwrap();
    assert_eq!(reply.events, events);
    assert_eq!(reply.error.as_deref(), Some("Authority"));
    assert!(
        value(&host)
            == serde_json::from_str::<serde_json::Value>(&expected.to_save_json().unwrap())
                .unwrap()
    );
    let saved = host.capture().unwrap();
    let restored = OwnerBridge::reconstruct(&saved, cfg).unwrap();
    assert!(value(&restored) == value(&host));
}
