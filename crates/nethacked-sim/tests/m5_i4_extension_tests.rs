use nethacked_arena::{ItemId, ItemLocation, ItemRecord};
use nethacked_core::ActionAst;
use nethacked_kernel::{Access, Capability, Endpoint, Extension, Manifest};
use nethacked_sim::{
    owner_persistence::{wand_ports::*, OwnerBridge, OwnerBridgeConfig},
    SimulationWorld,
};
use nethacked_types::{Buc, Direction};
use std::{collections::BTreeSet, sync::Mutex};

// Kernel capture includes the process-wide cleanup identity high-water mark.
// Serialize these tests so unrelated fresh hosts cannot alter byte witnesses.
static CAPTURE_WITNESS: Mutex<()> = Mutex::new(());

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
fn fixture() -> (SimulationWorld, ItemId) {
    let mut world = SimulationWorld::new_with_seed(724);
    world.hero.skills.skills.clear();
    world.arena.items.clear();
    world.wielded_item = None;
    world.equipment = Default::default();
    world.hero.quivered_item = None;
    let item = world
        .ruleset
        .create_item_record(
            "wand of fire",
            ItemLocation::CarriedBy(world.player_id),
            Buc::Uncursed,
        )
        .unwrap();
    let id = world.arena.spawn_item(ItemRecord {
        enchantment: 3,
        ..item
    });
    (world, id)
}
fn config(w: &SimulationWorld, guests: Vec<WandGuest>) -> OwnerBridgeConfig {
    OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
        .unwrap()
        .with_wand_extensions(guests)
        .unwrap()
}
fn action(id: ItemId) -> ActionAst {
    ActionAst::ZapSelectedWand {
        item_index: 0,
        expected_item: Some(id),
        dir: Direction::East,
        energy: 0,
    }
}
fn bytes(b: &OwnerBridge) -> String {
    b.export_native().unwrap().to_save_json().unwrap()
}
fn charges(b: &OwnerBridge, id: ItemId) -> i8 {
    b.export_native().unwrap().arena.items[id].enchantment
}

#[test]
fn executable_guest_attaches_to_populated_host_and_restores() {
    let _witness = CAPTURE_WITNESS.lock().unwrap_or_else(|e| e.into_inner());
    let (world, id) = fixture();
    let json = world.to_save_json().unwrap();
    let b = OwnerBridge::import_native(&json, json.len(), config(&world, vec![guest()])).unwrap();
    assert!(b.dispatch_wand_action(action(id)).unwrap().error.is_none());
    assert_eq!(charges(&b, id), 2);
    let populated = bytes(&b);
    b.attach_wand_extension("wand-cancel-extension").unwrap();
    assert_eq!(bytes(&b), populated);
    assert!(b.dispatch_wand_action(action(id)).unwrap().error.is_none());
    assert_eq!(bytes(&b), populated);
    let saved = b.capture().unwrap();
    let restored = OwnerBridge::reconstruct(&saved, config(&world, vec![guest()])).unwrap();
    assert_eq!(bytes(&restored), populated);
    assert!(restored
        .dispatch_wand_action(action(id))
        .unwrap()
        .error
        .is_none());
    assert_eq!(bytes(&restored), populated);
    restored
        .detach_wand_extension("wand-cancel-extension")
        .unwrap();
    assert!(restored
        .dispatch_wand_action(action(id))
        .unwrap()
        .error
        .is_none());
    assert_eq!(charges(&restored, id), 1);
    restored
        .attach_wand_extension("wand-cancel-extension")
        .unwrap();
    let before = bytes(&restored);
    assert!(restored
        .dispatch_wand_action(action(id))
        .unwrap()
        .error
        .is_none());
    assert_eq!(bytes(&restored), before);
}
#[test]
fn missing_approval_and_duplicate_attachment_refuse_without_exchange() {
    let _witness = CAPTURE_WITNESS.lock().unwrap_or_else(|e| e.into_inner());
    let (world, _) = fixture();
    let json = world.to_save_json().unwrap();
    let b = OwnerBridge::import_native(&json, json.len(), config(&world, vec![guest()])).unwrap();
    let before = b.capture().unwrap();
    assert!(b.attach_wand_extension("unknown").is_err());
    let after = b.capture().unwrap();
    assert!(after == before, "refusal changed capture");
    b.attach_wand_extension("wand-cancel-extension").unwrap();
    let before = b.capture().unwrap();
    assert!(b.attach_wand_extension("wand-cancel-extension").is_err());
    let after = b.capture().unwrap();
    assert!(after == before, "refusal changed capture");
    let destination =
        OwnerBridge::import_native(&json, json.len(), config(&world, vec![])).unwrap();
    let untouched = destination.capture().unwrap();
    assert!(destination.restore(&before).is_err());
    assert!(
        destination.capture().unwrap() == untouched,
        "restore refusal changed destination"
    );
}
#[test]
fn extension_facade_preserves_all_historical_gameplay_profiles() {
    let _witness = CAPTURE_WITNESS.lock().unwrap_or_else(|e| e.into_inner());
    let (world, id) = fixture();
    let json = world.to_save_json().unwrap();
    for lifetime in [false, true] {
        let old = OwnerBridgeConfig::new(world.ruleset.clone(), world.ruleset_ref.clone())
            .unwrap()
            .with_wand_gameplay();
        let old = if lifetime {
            old.with_item_lifetime()
        } else {
            old
        };
        let original = OwnerBridge::import_native(&json, json.len(), old).unwrap();
        let saved = original.capture().unwrap();
        let restored = OwnerBridge::reconstruct(&saved, config(&world, vec![guest()])).unwrap();
        let before = restored.capture().unwrap();
        assert!(restored
            .attach_wand_extension("wand-cancel-extension")
            .is_err());
        assert_eq!(restored.capture().unwrap(), before);
        let expected = original.dispatch_wand_action(action(id)).unwrap();
        let actual = restored.dispatch_wand_action(action(id)).unwrap();
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
        assert_eq!(bytes(&restored), bytes(&original));
    }
}

#[test]
fn paused_old_code_survives_attach_source_drop_and_fresh_native_restore() {
    let _witness = CAPTURE_WITNESS.lock().unwrap_or_else(|e| e.into_inner());
    let (world, id) = fixture();
    let json = world.to_save_json().unwrap();
    let config = config(&world, vec![guest()]).with_wand_workflows().unwrap();
    let source = OwnerBridge::import_native(&json, json.len(), config.clone()).unwrap();
    let old_ticket = source
        .pause_wand(WandGameplayAction::Zap {
            index: Some(0),
            expected: Some(id),
            direction: Direction::East,
            energy: 0,
        })
        .unwrap();
    assert_eq!(
        source
            .export_native()
            .unwrap()
            .arena
            .items
            .get(id)
            .unwrap()
            .enchantment,
        3
    );
    // The host explicitly approves actual live execution, before saved input exists.
    let approval = source.approve_live_wands().unwrap();
    source
        .attach_wand_extension("wand-cancel-extension")
        .unwrap();
    let saved = source.capture().unwrap();
    drop(source);
    let restored = OwnerBridge::reconstruct(
        &saved,
        config.with_wand_workflow_approval(approval).unwrap(),
    )
    .unwrap();
    assert!(restored.resume_wand(&old_ticket).is_err());
    let tickets = restored.pending_wands().unwrap();
    assert_eq!(tickets.len(), 1);
    assert_ne!(tickets[0], old_ticket);
    assert!(restored.resume_wand(&tickets[0]).unwrap());
    assert_eq!(
        restored
            .export_native()
            .unwrap()
            .arena
            .items
            .get(id)
            .unwrap()
            .enchantment,
        2
    );
    assert!(restored.resume_wand(&tickets[0]).is_err());
    let before = bytes(&restored);
    assert!(restored
        .dispatch_wand_action(action(id))
        .unwrap()
        .error
        .is_none());
    assert_eq!(bytes(&restored), before);
    assert!(restored.pending_wands().unwrap().is_empty());
}

#[test]
fn absent_independent_workflow_approval_and_bad_native_migration_preserve_destination() {
    let _witness = CAPTURE_WITNESS.lock().unwrap_or_else(|e| e.into_inner());
    let (world, id) = fixture();
    let json = world.to_save_json().unwrap();
    let config = config(&world, vec![guest()]).with_wand_workflows().unwrap();
    let source = OwnerBridge::import_native(&json, json.len(), config.clone()).unwrap();
    source
        .pause_wand(WandGameplayAction::Zap {
            index: Some(0),
            expected: Some(id),
            direction: Direction::East,
            energy: 0,
        })
        .unwrap();
    let approval = source.approve_live_wands().unwrap();
    let saved = source.capture().unwrap();
    let mut revoked = OwnerBridge::import_native(
        &json,
        json.len(),
        config
            .clone()
            .with_wand_workflow_approval(approval)
            .unwrap(),
    )
    .unwrap();
    let world_before = bytes(&revoked);
    revoked.revoke_wand_workflow_approval().unwrap();
    assert_eq!(bytes(&revoked), world_before);
    let untouched = revoked.capture().unwrap();
    assert!(revoked.restore(&saved).is_err());
    assert!(revoked.capture().unwrap() == untouched);
    let destination = OwnerBridge::import_native(&json, json.len(), config).unwrap();
    let before = destination.capture().unwrap();
    assert!(destination.restore(&saved).is_err());
    assert!(destination.capture().unwrap() == before);
    assert!(destination.restore_native("{}", 2).is_err());
    assert!(destination.capture().unwrap() == before);
}

fn rewrite_section(wire: &[u8], name: &str, edit: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let mut body: serde_json::Value =
        serde_json::from_slice(&wire[nethacked_persistence::HEADER_LEN..]).unwrap();
    let section = body["sections"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|s| s["name"] == name)
        .unwrap();
    let payload = nethacked_persistence::Hex(section["local_payload"].as_str().unwrap().into())
        .bytes(Default::default())
        .unwrap();
    let mut payload: serde_json::Value = serde_json::from_slice(&payload).unwrap();
    edit(&mut payload);
    section["local_payload"] = serde_json::Value::String(
        nethacked_persistence::Hex::from_bytes(&serde_json::to_vec(&payload).unwrap()).0,
    );
    let body = serde_json::to_vec(&body).unwrap();
    let mut out = wire[..12].to_vec();
    out.extend_from_slice(&(body.len() as u64).to_le_bytes());
    out.extend_from_slice(&Sha256::digest(&body));
    out.extend_from_slice(&body);
    out
}

#[test]
fn changed_executable_artifact_and_incompatible_owner_schema_refuse_atomically() {
    let _witness = CAPTURE_WITNESS.lock().unwrap_or_else(|e| e.into_inner());
    let (world, _) = fixture();
    let json = world.to_save_json().unwrap();
    let source =
        OwnerBridge::import_native(&json, json.len(), config(&world, vec![guest()])).unwrap();
    source
        .attach_wand_extension("wand-cancel-extension")
        .unwrap();
    let saved = source.capture().unwrap();
    let approved = guest();
    let mut code = approved.bytes().to_vec();
    code.extend_from_slice(&[0, 1, 0]); // A valid additional empty-name custom section.
    let changed = WandGuest::approve(approved.manifest().clone(), code).unwrap();
    let destination =
        OwnerBridge::import_native(&json, json.len(), config(&world, vec![changed])).unwrap();
    let before = destination.capture().unwrap();
    assert!(destination.restore(&saved).is_err());
    assert!(destination.capture().unwrap() == before);
    let destination =
        OwnerBridge::import_native(&json, json.len(), config(&world, vec![guest()])).unwrap();
    let before = destination.capture().unwrap();
    let incompatible = rewrite_section(&saved, "owners", |owners| {
        owners["current"][0]["build"]["schema"]["version"] = 999.into();
    });
    assert!(destination.restore(&incompatible).is_err());
    assert!(destination.capture().unwrap() == before);
}
