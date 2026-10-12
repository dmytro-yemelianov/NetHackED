use super::super::*;
use crate::owner_persistence::wand_ports::*;
use nethacked_arena::{ItemId, ItemLocation, ItemRecord};
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

fn fixture() -> (SimulationWorld, ItemId, OwnerBridgeConfig) {
    let mut w = SimulationWorld::new_with_seed(53);
    w.hero.skills.skills.clear();
    w.arena.items.clear();
    w.arena.actors.retain(|id, _| id == w.player_id);
    w.wielded_item = None;
    w.hero.quivered_item = None;
    w.equipment = Default::default();
    w.unpaid_items.clear();
    w.billing = Default::default();
    w.level = nethacked_dungeon::DungeonLevel::new_solid(Tile::Room);
    w.arena.actors[w.player_id].coord = Coord::new_unchecked(20, 10);
    let item = w
        .ruleset
        .create_item_record(
            "wand of death",
            ItemLocation::CarriedBy(w.player_id),
            Buc::Uncursed,
        )
        .unwrap();
    let id = w.arena.spawn_item(ItemRecord {
        enchantment: 3,
        ..item
    });
    let mut actor = w
        .ruleset
        .create_monster_record("goblin", Coord::new_unchecked(21, 10), &mut w.rng)
        .unwrap();
    actor.hp = 1;
    actor.max_hp = 1;
    actor.speed = 0;
    actor.movement = 0;
    w.arena.spawn_actor(actor);
    w.rng = rand_chacha::ChaCha8Rng::seed_from_u64(100);
    let cfg = OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
        .unwrap()
        .with_wand_extensions(vec![guest()])
        .unwrap()
        .with_wand_workflows()
        .unwrap();
    (w, id, cfg)
}
fn action(id: ItemId) -> WandGameplayAction {
    WandGameplayAction::Zap {
        index: Some(0),
        expected: Some(id),
        direction: Direction::East,
        energy: 6,
    }
}
fn bytes(b: &OwnerBridge) -> String {
    b.export_native().unwrap().to_save_json().unwrap()
}
fn import(w: &SimulationWorld, cfg: OwnerBridgeConfig) -> OwnerBridge {
    let json = w.to_save_json().unwrap();
    OwnerBridge::import_native(&json, json.len(), cfg).unwrap()
}
#[test]
fn detailed_retained_reply_survives_restore_and_matches_legacy_ordered_events() {
    let (w, id, cfg) = fixture();
    let cfg = cfg.with_wand_workflow_replies().unwrap();
    let source = import(&w, cfg.clone());
    let mut legacy = source.export_native().unwrap();
    let old = source.pause_wand(action(id)).unwrap();
    let approval = source.approve_live_wands().unwrap();
    source
        .attach_wand_extension("wand-cancel-extension")
        .unwrap();
    let saved = source.capture().unwrap();
    drop(source);
    let restored = OwnerBridge::reconstruct(
        &saved,
        cfg.clone().with_wand_workflow_approval(approval).unwrap(),
    )
    .unwrap();
    let before = bytes(&restored);
    assert!(restored.resume_wand_reply(&old).is_err());
    assert_eq!(bytes(&restored), before);
    let ticket = restored.pending_wands().unwrap().remove(0);
    let reply = restored.resume_wand_reply(&ticket).unwrap();
    assert!(reply.error.is_none());
    assert!(reply.dependency_calls > 0);
    let events = legacy.handle_zap_selected_wand(0, Some(id), Direction::East, 6);
    assert_eq!(reply.events, events);
    assert!(reply
        .events
        .iter()
        .any(|e| matches!(e, crate::GameEvent::AttackLanded { lethal: true, .. })));
    assert!(legacy.rng.get_word_pos() > 0);
    assert_eq!(bytes(&restored), legacy.to_save_json().unwrap());
    let before = bytes(&restored);
    assert!(restored.resume_wand_reply(&ticket).is_err());
    assert_eq!(bytes(&restored), before);
    let approval = restored.approve_live_wands().unwrap();
    let saved = restored.capture().unwrap();
    drop(restored);
    let finished =
        OwnerBridge::reconstruct(&saved, cfg.with_wand_workflow_approval(approval).unwrap())
            .unwrap();
    assert!(finished.pending_wands().unwrap().is_empty());
    assert_eq!(bytes(&finished), before);
    let frame = finished
        .session
        .with_runtime(&finished.credential.borrow(), |b| {
            let wf = &b.services().workflows.as_ref().unwrap().0;
            wf.inspect(wf.operations()?.remove(0))
        })
        .unwrap()
        .unwrap();
    // Restored operation IDs are remapped, so inspect the actual restored frame.
    assert!(!frame.trace.is_empty());
    assert_eq!(finished.wand_rng().next_u64(), legacy.rng.next_u64());
}
#[test]
fn detailed_facade_preserves_historical_boolean_artifact_and_ticket() {
    let (w, id, old_cfg) = fixture();
    let source = import(&w, old_cfg.clone());
    let mut legacy = source.export_native().unwrap();
    source.pause_wand(action(id)).unwrap();
    let approval = source.approve_live_wands().unwrap();
    let saved = source.capture().unwrap();
    drop(source);
    let cfg = old_cfg
        .with_wand_workflow_replies()
        .unwrap()
        .with_wand_workflow_approval(approval)
        .unwrap();
    let restored = OwnerBridge::reconstruct(&saved, cfg).unwrap();
    let ticket = restored.pending_wands().unwrap().remove(0);
    let before = bytes(&restored);
    assert!(restored.resume_wand_reply(&ticket).is_err());
    assert_eq!(bytes(&restored), before);
    assert_eq!(restored.pending_wands().unwrap(), vec![ticket.clone()]);
    assert!(restored.resume_wand(&ticket).unwrap());
    legacy.handle_zap_selected_wand(0, Some(id), Direction::East, 6);
    assert_eq!(bytes(&restored), legacy.to_save_json().unwrap());
    assert!(restored.approve_live_wands().is_ok());
}
#[test]
fn detailed_workflows_keep_boolean_summary_compatible() {
    let (w, id, cfg) = fixture();
    let b = import(&w, cfg.with_wand_workflow_replies().unwrap());
    let mut legacy = b.export_native().unwrap();
    let ticket = b.pause_wand(action(id)).unwrap();
    assert!(b.resume_wand(&ticket).unwrap());
    legacy.handle_zap_selected_wand(0, Some(id), Direction::East, 6);
    assert_eq!(bytes(&b), legacy.to_save_json().unwrap());
}

mod process;
