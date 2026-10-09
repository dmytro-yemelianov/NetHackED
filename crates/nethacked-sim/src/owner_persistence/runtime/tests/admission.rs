use super::*;
use nethacked_persistence::Hex;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

fn rewrite(bytes: &[u8], name: &str, mutate: impl FnOnce(&mut Value)) -> Vec<u8> {
    let mut wire: Value =
        serde_json::from_slice(&bytes[nethacked_persistence::HEADER_LEN..]).unwrap();
    let old = wire["owners"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["key"] == name)
        .unwrap()["owner"]
        .clone();
    let section = wire["sections"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|row| row["name"] == "owners")
        .unwrap();
    let hex: Hex = serde_json::from_value(section["local_payload"].clone()).unwrap();
    let mut checkpoint: Value =
        serde_json::from_slice(&hex.bytes(SaveLimits::default()).unwrap()).unwrap();
    let row = checkpoint["state"]["owners"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|row| row["old"] == old)
        .unwrap();
    let payload: Vec<u8> = serde_json::from_value(row["payload"].clone()).unwrap();
    let mut payload: Value = serde_json::from_slice(&payload).unwrap();
    mutate(&mut payload);
    row["payload"] = json!(serde_json::to_vec(&payload).unwrap());
    let checkpoint: op::ExecutionCheckpoint = serde_json::from_value(checkpoint).unwrap();
    section["local_payload"] = json!(Hex::from_bytes(
        &serde_json::to_vec(&serde_json::to_value(checkpoint).unwrap()).unwrap()
    ));
    let body = serde_json::to_vec(&wire).unwrap();
    let mut framed = bytes[..12].to_vec();
    framed.extend_from_slice(&(body.len() as u64).to_le_bytes());
    framed.extend_from_slice(&Sha256::digest(&body));
    framed.extend_from_slice(&body);
    framed
}

#[test]
fn malformed_native_edges_refuse_before_factories_preserving_nonempty_session() {
    // Captures include the process-wide cleanup allocation high-water mark.
    // Isolate this exact-byte test from unrelated parallel test allocations.
    const CHILD: &str = "NETHACKED_D5_REFERENCE_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let name = format!(
            "{}::malformed_native_edges_refuse_before_factories_preserving_nonempty_session",
            module_path!().split_once("::").unwrap().1
        );
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &name])
            .env(CHILD, "1")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "{stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            stdout.contains("1 passed; 0 failed;"),
            "isolated test did not execute: {stdout}"
        );
        return;
    }
    let (destination, item) = source();
    let wire = destination.capture().unwrap();
    let before = snapshots(&destination);
    let mut native = destination.export_native().unwrap();
    let stale_actor = native
        .arena
        .spawn_actor(native.arena.actors[native.player_id].clone());
    let stale_item = native.arena.spawn_item(native.arena.items[item].clone());
    let cases = [
        "Hero",
        "Hero",
        "Hero",
        "Equipment",
        "Equipment",
        "Clock",
        "Objects",
        "Objects",
        "Objects",
        "Objects",
    ];
    for (case, owner) in cases.into_iter().enumerate() {
        let bad = rewrite(&wire, owner, |payload| match case {
            0 => payload["player_id"] = json!(stale_actor),
            1 => payload["hero"]["quivered_item"] = json!(stale_item),
            2 => payload["hero"]["mount"] = json!({"steed_id":stale_actor,"saddle_equipped":true}),
            3 => payload["wielded_item"] = json!(stale_item),
            4 => {
                payload["equipment"]["dressing"] =
                    json!({"item":stale_item,"putting_on":true,"turns_left":3})
            }
            5 => {
                payload["object_timers"] = json!({"entries":[crate::ObjectTimer {
                    target: crate::ObjectTimerTarget::Active(stale_item),
                    kind: crate::ObjectTimerKind::RotCorpse, due: 0, serial: 0,
                }], "next_serial":1});
            }
            6..=9 => {
                let mut objects: ObjectsState = serde_json::from_value(payload.take()).unwrap();
                match case {
                    6 => objects.items[item].location = ItemLocation::InContainer(stale_item),
                    7 => objects.items[item].location = ItemLocation::CarriedBy(stale_actor),
                    8 => objects.items[item].location = ItemLocation::InContainer(item),
                    9 => {
                        objects.items[item].definition = Some(
                            nethacked_types::ItemDefinition::Custom("unavailable".into()),
                        )
                    }
                    _ => unreachable!(),
                }
                *payload = serde_json::to_value(objects).unwrap();
            }
            _ => unreachable!(),
        });
        // Preserve valid transport and generic graph admission: the refusal
        // below must be caused by the new native profile, not a broken fixture.
        let profile = ProfileFactory {
            inner: session::SessionFactory::new(restore_authority(&destination.config).unwrap()),
            config: destination.config.clone(),
        };
        let (rows, checkpoint) = profile.inner.inspect_owner_states(&bad).unwrap();
        assert!(profile.admit(&rows, &checkpoint).is_err(), "case {case}");
        FACTORY_BUILDS.with(|count| count.set(0));
        assert!(destination.restore(&bad).is_err(), "case {case}");
        FACTORY_BUILDS.with(|count| assert_eq!(count.get(), 0, "case {case}"));
        assert!(
            snapshots(&destination) == before,
            "case {case}: owner state changed"
        );
        assert!(
            destination.capture().unwrap() == wire,
            "case {case}: session bytes changed"
        );
    }
    // The refusal test must not pass just because every restore is disabled.
    destination.restore(&wire).unwrap();
    FACTORY_BUILDS.with(|count| assert!(count.get() > 0));
    assert_eq!(
        destination.export_native().unwrap().unpaid_items,
        native.unpaid_items
    );
}

#[test]
fn consumed_and_unresolved_debt_keep_historical_origins_without_live_targets() {
    let (source, item) = source();
    let mut native = source.export_native().unwrap();
    let historical_actor = native
        .arena
        .spawn_actor(native.arena.actors[native.player_id].clone());
    native.arena.actors.remove(historical_actor);
    let mut object = native.arena.items.remove(item).unwrap();
    object.location = ItemLocation::Limbo;
    native.unpaid_items.retain(|(id, _)| *id != item);
    let receipt = BillTerms {
        billed_quantity: Some(1),
        unit_price: Some(19),
        legacy_base_cost: 19,
        creditor: Some(crate::world::BillCreditor {
            branch: native.current_branch,
            depth: native.depth,
            shopkeeper: historical_actor,
        }),
    };
    native
        .billing
        .consumed
        .push(crate::world::ConsumedBillRecord {
            origin: item,
            object,
            terms: receipt.clone(),
        });
    // This second, absent origin has no exact receipt; preserve unresolved debt.
    let unknown = native
        .arena
        .spawn_item(native.billing.consumed[0].object.clone());
    native.arena.items.remove(unknown);
    native.unpaid_items.push((unknown, 27));
    let c = OwnerBridgeConfig::new(native.ruleset.clone(), native.ruleset_ref.clone()).unwrap();
    let json = native.to_save_json().unwrap();
    let imported = OwnerBridge::import_native(&json, json.len(), c.clone()).unwrap();
    let wire = imported.capture().unwrap();
    drop(imported);
    let restored = OwnerBridge::reconstruct(&wire, c).unwrap();
    assert_eq!(restored.export_native().unwrap().billing, native.billing);
    assert_eq!(
        restored.export_native().unwrap().unpaid_items,
        native.unpaid_items
    );
    let before = snapshots(&restored);
    assert_eq!(
        restored.record_bill_terms(item, receipt),
        Err(BillError::AlreadyConsumed)
    );
    assert_eq!(snapshots(&restored), before);
}

#[test]
fn cached_references_use_level_scope_even_when_handles_match_other_levels() {
    let (initial, item) = source();
    let mut native = initial.export_native().unwrap();
    let mut actor = native.arena.actors[native.player_id].clone();
    actor.is_player = false;
    actor.name = "kobold".into();
    actor.species = Some(nethacked_arena::ActorSpecies::Vanilla("KOBOLD".into()));
    let actor_id = native.arena.spawn_actor(actor.clone());
    native.arena.actors.remove(actor_id);
    let mut cached_item = native.arena.items[item].clone();
    cached_item.location = ItemLocation::CarriedBy(actor_id);
    for depth in [9, 3] {
        native.stored_levels.push((
            (native.current_branch, depth),
            crate::StoredLevel {
                level: native.level.clone(),
                items: vec![(item, cached_item.clone())],
                monsters: vec![(actor_id, actor.clone())],
                unpaid_items: vec![],
            },
        ));
    }
    native
        .start_object_timer(
            crate::ObjectTimerTarget::Cached {
                branch: native.current_branch,
                depth: 9,
                item,
            },
            crate::ObjectTimerKind::RotCorpse,
            77,
        )
        .unwrap();
    let c = OwnerBridgeConfig::new(native.ruleset.clone(), native.ruleset_ref.clone()).unwrap();
    let json = native.to_save_json().unwrap();
    let destination = OwnerBridge::import_native(&json, json.len(), c).unwrap();
    let wire = destination.capture().unwrap();
    let before = snapshots(&destination);
    for (case, name) in [
        "Actors",
        "Objects",
        "Geography",
        "Objects",
        "Geography",
        "Clock",
        "Actors",
        "Objects",
    ]
    .into_iter()
    .enumerate()
    {
        let bad = rewrite(&wire, name, |value| match case {
            0 => value["stored_monsters"][0] = json!([]),
            1 => value["stored_items"][0] = json!([]),
            2 => value["stored_keys"][1] = value["stored_keys"][0].clone(),
            3 => {
                value["stored_items"].as_array_mut().unwrap().pop();
            }
            4 => {
                value["stored_levels"].as_array_mut().unwrap().pop();
            }
            5 => {
                let mut clock: ClockState = serde_json::from_value(value.take()).unwrap();
                let mut timer_value = serde_json::to_value(&clock.object_timers).unwrap();
                timer_value["entries"][0]["target"] = json!(crate::ObjectTimerTarget::Cached {
                    branch: native.current_branch,
                    depth: 999,
                    item,
                });
                clock.object_timers = serde_json::from_value(timer_value).unwrap();
                *value = serde_json::to_value(clock).unwrap();
            }
            6 => {
                let row = value["stored_monsters"][0][0].clone();
                value["stored_monsters"][0]
                    .as_array_mut()
                    .unwrap()
                    .push(row);
            }
            7 => {
                let row = value["stored_items"][0][0].clone();
                value["stored_items"][0].as_array_mut().unwrap().push(row);
            }
            _ => unreachable!(),
        });
        let profile = ProfileFactory {
            inner: session::SessionFactory::new(restore_authority(&destination.config).unwrap()),
            config: destination.config.clone(),
        };
        let (rows, checkpoint) = profile.inner.inspect_owner_states(&bad).unwrap();
        assert!(
            profile.admit(&rows, &checkpoint).is_err(),
            "cached case {case}"
        );
        FACTORY_BUILDS.with(|count| count.set(0));
        assert!(destination.restore(&bad).is_err(), "cached case {case}");
        FACTORY_BUILDS.with(|count| assert_eq!(count.get(), 0));
        assert!(
            snapshots(&destination) == before,
            "cached case {case}: owner state changed"
        );
    }
    destination.restore(&wire).unwrap();
    assert_eq!(
        destination.export_native().unwrap().stored_levels.len(),
        native.stored_levels.len()
    );
}
