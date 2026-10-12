//! Native bytes and separately approved ruleset cross real process lifetimes.
use super::*;
use crate::owner_persistence::migration::TRANSFORMS;
use crate::{ObjectTimerKind as Kind, ObjectTimerTarget as Target, StoredLevel};
use nethacked_arena::{ActorId, ItemLocation};
use nethacked_data::ItemKindId as K;
use nethacked_persistence::migration::MigrationError;
use nethacked_types::BranchId;
use serde_json::Value;
use std::{path::Path, process::Command};

#[derive(Serialize, Deserialize)]
struct Fixture {
    producer_pid: u32,
    ruleset: Ruleset,
    reference: RulesetRef,
    native: String,
    owners: Vec<u8>,
    target: ItemId,
    consumed: ItemId,
    quantity: u32,
    before: Value,
    after: Value,
    repeat: String,
    continuation: Value,
}

fn terms(quantity: u32) -> BillTerms {
    BillTerms {
        billed_quantity: Some(quantity),
        unit_price: Some(40),
        legacy_base_cost: 30,
        creditor: None,
    }
}
fn wand(world: &mut SimulationWorld, custom: bool, quantity: u32) -> ItemId {
    let mut item = world
        .ruleset
        .create_item_record(
            if custom { "pack rod" } else { "wand of fire" },
            ItemLocation::CarriedBy(world.player_id),
            Buc::Cursed,
        )
        .unwrap();
    item.quantity = quantity;
    let id = world.arena.spawn_item(item);
    world.unpaid_items.push((id, 30));
    id
}

fn produce(path: &Path, custom: bool, quantity: u32) {
    let mut world = SimulationWorld::new_with_seed(804);
    world.arena.actors.retain(|id, _| id == world.player_id);
    world.arena.items.clear();
    world.equipment = Default::default();
    world.wielded_item = None;
    world.hero.skills.skills.clear();
    if custom {
        let mut ruleset = (*world.ruleset).clone();
        let mut item = ruleset.item_by_id(K::WAN_FIRE).unwrap().clone();
        item.id = None;
        item.name = "pack rod".into();
        ruleset.items.push(item);
        ruleset.reindex();
        world.ruleset = Arc::new(ruleset);
        world.ruleset_ref.id = "approved-pack".into();
        world.ruleset_ref.hash = "approved-hash".into();
    }
    let target = wand(&mut world, custom, quantity);
    let known = wand(&mut world, custom, quantity);
    world.record_bill_terms(known, terms(quantity)).unwrap();
    let consumed = wand(&mut world, custom, quantity);
    world.record_bill_terms(consumed, terms(quantity)).unwrap();
    world.dispose_whole_carried_item(consumed).unwrap();
    // Tie order, future continuation, and a deferred cached timer all travel.
    for due in [0, 0, 3] {
        let item = world
            .ruleset
            .create_corpse_record("human", ItemLocation::Limbo)
            .unwrap();
        let id = world.arena.spawn_item(item);
        world
            .start_object_timer(Target::Active(id), Kind::RotCorpse, due)
            .unwrap();
        if due == 3 {
            world.stored_levels.push((
                (BranchId::DungeonsOfDoom, 9),
                StoredLevel {
                    level: world.level.clone(),
                    monsters: vec![],
                    items: vec![(id, world.arena.items[id].clone())],
                    unpaid_items: vec![],
                },
            ));
            world
                .start_object_timer(
                    Target::Cached {
                        branch: BranchId::DungeonsOfDoom,
                        depth: 9,
                        item: id,
                    },
                    Kind::RotCorpse,
                    0,
                )
                .unwrap();
        }
    }
    let mut legacy = serde_json::to_value(&world).unwrap();
    legacy.as_object_mut().unwrap().remove("equipment");
    legacy["accepted_unknown_legacy_field"] = true.into();
    let native = legacy.to_string();
    let config = OwnerBridgeConfig::new(world.ruleset.clone(), world.ruleset_ref.clone()).unwrap();
    let bridge = OwnerBridge::import_native(&native, native.len(), config).unwrap();
    let owners = bridge.capture().unwrap();
    drop(bridge);
    let mut reference =
        SimulationWorld::from_save_json(&native, world.ruleset.clone(), &world.ruleset_ref)
            .unwrap();
    let before = serde_json::to_value(&reference).unwrap();
    reference
        .record_bill_terms(target, terms(quantity))
        .unwrap();
    let after = serde_json::to_value(&reference).unwrap();
    let repeat = format!("{:?}", reference.record_bill_terms(target, terms(quantity)));
    for _ in 0..4 {
        reference.step_player_action(crate::ActionAst::Wait);
    }
    let fixture = Fixture {
        producer_pid: std::process::id(),
        ruleset: (*world.ruleset).clone(),
        reference: world.ruleset_ref.clone(),
        native,
        owners,
        target,
        consumed,
        quantity,
        before,
        after,
        repeat,
        continuation: serde_json::to_value(reference).unwrap(),
    };
    std::fs::write(path, serde_json::to_vec(&fixture).unwrap()).unwrap();
}

fn consume(path: &Path) {
    let bytes = std::fs::read(path).unwrap();
    assert!(bytes.len() < 64 * 1024 * 1024);
    let mut fixture: Fixture = serde_json::from_slice(&bytes).unwrap();
    assert_ne!(fixture.producer_pid, std::process::id());
    fixture.ruleset.reindex();
    let config = OwnerBridgeConfig::new(Arc::new(fixture.ruleset), fixture.reference).unwrap();
    let mut destination = OwnerBridge::reconstruct(&fixture.owners, config).unwrap();
    assert_eq!(
        serde_json::to_value(destination.export_native().unwrap()).unwrap(),
        fixture.before
    );
    let before = snapshots(&destination);
    let saved = destination.capture().unwrap();
    // Old schema errors with an available migrator do not run it or factories,
    // and cannot change a populated destination or invalidate its credential.
    let original: Value = serde_json::from_str(&fixture.native).unwrap();
    for case in 0..4 {
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
            2 => value["ruleset_ref"]["hash"] = "unapproved".into(),
            3 => (),
            _ => unreachable!(),
        }
        let json = if case == 3 {
            "{".to_string()
        } else {
            value.to_string()
        };
        TRANSFORMS.with(|n| n.set(0));
        FACTORY_BUILDS.with(|n| n.set(0));
        assert!(matches!(
            destination.restore_native(&json, json.len()),
            Err(OwnerBridgeError::Migration(MigrationError::Source(_)))
        ));
        assert_eq!(TRANSFORMS.with(|n| n.get()), 0);
        assert_eq!(FACTORY_BUILDS.with(|n| n.get()), 0);
        assert!(snapshots(&destination) == before);
        assert!(destination.capture().unwrap() == saved);
    }
    let approved_binding = destination.config.native_binding;
    destination.config.native_binding.transform_version += 1;
    TRANSFORMS.with(|n| n.set(0));
    assert!(matches!(
        destination.restore_native(&fixture.native, fixture.native.len()),
        Err(OwnerBridgeError::Migration(MigrationError::Unavailable))
    ));
    assert_eq!(TRANSFORMS.with(|n| n.get()), 0);
    assert!(snapshots(&destination) == before);
    assert!(destination.capture().unwrap() == saved);
    destination.config.native_binding = approved_binding;
    // Valid framed owner bytes with an invalid native reference must refuse
    // before executable factories, even in a different process from capture.
    let bad = super::admission::rewrite(&fixture.owners, "Hero", |value| {
        let mut hero: HeroState = serde_json::from_value(value.take()).unwrap();
        let mut actors = nethacked_arena::EntityArena::new();
        let stale: ActorId = actors
            .actors
            .insert(destination.export_native().unwrap().arena.actors[hero.player_id].clone());
        actors.actors.remove(stale);
        // A removed generational handle is absent from the transported arena.
        let other = actors
            .actors
            .insert(destination.export_native().unwrap().arena.actors[hero.player_id].clone());
        hero.player_id = other;
        *value = serde_json::to_value(hero).unwrap();
    });
    FACTORY_BUILDS.with(|n| n.set(0));
    assert!(destination.restore(&bad).is_err());
    assert_eq!(FACTORY_BUILDS.with(|n| n.get()), 0);
    assert!(snapshots(&destination) == before);
    assert!(destination.capture().unwrap() == saved);
    // Positive native replacement uses the same nonempty facade. Catchup is
    // transferred once; restoring its resulting owner bytes schedules nothing.
    TRANSFORMS.with(|n| n.set(0));
    destination
        .restore_native(&fixture.native, fixture.native.len())
        .unwrap();
    assert_eq!(TRANSFORMS.with(|n| n.get()), 1);
    assert_eq!(
        serde_json::to_value(destination.export_native().unwrap()).unwrap(),
        fixture.before
    );
    assert_eq!(
        destination.record_bill_terms(fixture.consumed, terms(fixture.quantity)),
        Err(BillError::AlreadyConsumed)
    );
    assert_eq!(
        serde_json::to_value(destination.export_native().unwrap()).unwrap(),
        fixture.before
    );
    destination
        .record_bill_terms(fixture.target, terms(fixture.quantity))
        .unwrap();
    assert_eq!(
        serde_json::to_value(destination.export_native().unwrap()).unwrap(),
        fixture.after
    );
    assert_eq!(
        format!(
            "{:?}",
            destination.record_bill_terms(fixture.target, terms(fixture.quantity))
        ),
        fixture.repeat
    );
    assert_eq!(
        serde_json::to_value(destination.export_native().unwrap()).unwrap(),
        fixture.after
    );
    let json = destination.export_native().unwrap().to_save_json().unwrap();
    let mut native = SimulationWorld::from_save_json(
        &json,
        destination.config.ruleset.clone(),
        &destination.config.ruleset_ref,
    )
    .unwrap();
    for _ in 0..4 {
        native.step_player_action(crate::ActionAst::Wait);
    }
    assert_eq!(serde_json::to_value(native).unwrap(), fixture.continuation);
}

#[test]
fn a5_fresh_process_native_legacy_receipt_and_refusal_acceptance() {
    if let Ok(mode) = std::env::var("NETHACKED_A5_NATIVE_MODE") {
        let path = std::env::var_os("NETHACKED_A5_NATIVE_FILE").unwrap();
        match mode.as_str() {
            "produce" => produce(
                Path::new(&path),
                std::env::var("NETHACKED_A5_NATIVE_CUSTOM").unwrap() == "true",
                std::env::var("NETHACKED_A5_NATIVE_QUANTITY")
                    .unwrap()
                    .parse()
                    .unwrap(),
            ),
            "consume" => consume(Path::new(&path)),
            _ => panic!("unknown child phase"),
        }
        return;
    }
    let directory = std::env::temp_dir().join(format!(
        "netrust-a5-native-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let test = format!(
        "{}::a5_fresh_process_native_legacy_receipt_and_refusal_acceptance",
        module_path!().split_once("::").unwrap().1
    );
    for custom in [false, true] {
        for quantity in [1, 3] {
            let fixture = directory.join(format!("{custom}-{quantity}.json"));
            for mode in ["produce", "consume"] {
                let output = Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", &test, "--nocapture"])
                    .env("NETHACKED_A5_NATIVE_MODE", mode)
                    .env("NETHACKED_A5_NATIVE_FILE", &fixture)
                    .env("NETHACKED_A5_NATIVE_CUSTOM", custom.to_string())
                    .env("NETHACKED_A5_NATIVE_QUANTITY", quantity.to_string())
                    .output()
                    .unwrap();
                let log = directory.join(format!("{custom}-{quantity}-{mode}.log"));
                let bytes = [output.stdout, output.stderr].concat();
                std::fs::write(&log, &bytes).unwrap();
                assert!(
                    output.status.success(),
                    "child failed; log: {}",
                    log.display()
                );
                assert!(
                    String::from_utf8_lossy(&bytes).contains("1 passed; 0 failed;"),
                    "child did not execute exactly one test"
                );
            }
        }
    }
    std::fs::remove_dir_all(directory).unwrap();
}
