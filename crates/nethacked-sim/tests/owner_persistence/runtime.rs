use super::*;
use nethacked_sim::owner_persistence::{OwnerBridge, OwnerBridgeConfig};
use nethacked_sim::world::BillError;

fn config(w: &SimulationWorld) -> OwnerBridgeConfig {
    OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone()).unwrap()
}
fn bridge(w: &SimulationWorld) -> OwnerBridge {
    let json = w.to_save_json().unwrap();
    OwnerBridge::import_native(&json, json.len(), config(w)).unwrap()
}
fn terms(q: u32) -> BillTerms {
    BillTerms {
        billed_quantity: Some(q),
        unit_price: Some(40),
        legacy_base_cost: 30,
        creditor: None,
    }
}
fn wand(w: &mut SimulationWorld, q: u32) -> ItemId {
    let mut record = w
        .ruleset
        .create_item_record(
            "wand of fire",
            ItemLocation::CarriedBy(w.player_id),
            Buc::Cursed,
        )
        .unwrap();
    record.quantity = q;
    let id = w.arena.spawn_item(record);
    w.unpaid_items.push((id, 30));
    id
}

#[test]
fn owner_runtime_source_free_receipt_native_export_and_public_reload() {
    for custom in [false, true] {
        for quantity in [1, 3] {
            let mut w = world();
            if custom {
                let mut ruleset = (*w.ruleset).clone();
                let mut item = ruleset.item_by_id(K::WAN_FIRE).unwrap().clone();
                item.id = None;
                item.name = "pack rod".into();
                ruleset.items.push(item);
                ruleset.reindex();
                w.ruleset = Arc::new(ruleset);
                w.ruleset_ref.id = "approved-pack".into();
                w.ruleset_ref.hash = "approved-hash".into();
            }
            let id = wand(&mut w, quantity);
            if custom {
                w.arena.items[id] = w
                    .ruleset
                    .create_item_record(
                        "pack rod",
                        ItemLocation::CarriedBy(w.player_id),
                        Buc::Cursed,
                    )
                    .unwrap();
                w.arena.items[id].quantity = quantity;
            }
            // Arena vacancies, native cached routing/ledger order and consumed
            // histories come from the accepted fixtures rather than a new format.
            let vacant = corpse(&mut w, ItemLocation::Limbo);
            w.arena.items.remove(vacant);
            let later = corpse(&mut w, ItemLocation::Limbo);
            w.start_object_timer(Target::Active(later), Kind::RotCorpse, 18)
                .unwrap();
            let cached = w.arena.items[later].clone();
            w.stored_levels.push((
                (BranchId::DungeonsOfDoom, 9),
                StoredLevel {
                    level: w.level.clone(),
                    monsters: vec![],
                    items: vec![(later, cached)],
                    unpaid_items: vec![(later, 17)],
                },
            ));
            let source = bridge(&w);
            same(&source.export_native().unwrap(), &w);
            let bytes = source.capture().unwrap();
            let config = config(&w);
            drop(source);
            let restored = OwnerBridge::reconstruct(&bytes, config.clone()).unwrap();
            same(&restored.export_native().unwrap(), &w);
            let receipt = BillTerms {
                creditor: Some(BillCreditor {
                    branch: w.current_branch,
                    depth: w.depth,
                    shopkeeper: w.player_id,
                }),
                ..terms(quantity)
            };
            w.record_bill_terms(id, receipt.clone()).unwrap();
            restored.record_bill_terms(id, receipt).unwrap();
            same(&restored.export_native().unwrap(), &w);
            assert_eq!(
                restored.record_bill_terms(id, terms(quantity)),
                Err(BillError::ReceiptAlreadyRecorded)
            );
            same(&restored.export_native().unwrap(), &w);
            let resave = restored.capture().unwrap();
            drop(restored);
            let again = OwnerBridge::reconstruct(&resave, config).unwrap();
            same(&again.export_native().unwrap(), &w);
            let exported = again.export_native().unwrap();
            let json = exported.to_save_json().unwrap();
            same(
                &SimulationWorld::from_save_json(&json, w.ruleset.clone(), &w.ruleset_ref).unwrap(),
                &w,
            );
        }
    }
}

#[test]
fn owner_runtime_refusals_match_world_and_preserve_all_nine_states() {
    for case in 0..13 {
        let mut w = world();
        let id = wand(&mut w, 3);
        let mut receipt = terms(3);
        match case {
            0 => {
                w.arena.items.remove(id);
            }
            1 => {
                w.arena.items[id].location = ItemLocation::Limbo;
            }
            2 => {
                w.arena.items[id].is_container = true;
            }
            3 => {
                let child = corpse(&mut w, ItemLocation::InContainer(id));
                assert_ne!(id, child);
            }
            4 => {
                w.arena.items[id].definition = None;
                w.arena.items[id].name = "unavailable definition".into();
            }
            5 => {
                w.arena.items[id].quantity = 0;
            }
            6 => {
                receipt.billed_quantity = Some(1);
            }
            7 => {
                w.unpaid_items.push((id, 30));
            }
            8 => {
                w.unpaid_items.clear();
            }
            9 => {
                receipt.unit_price = Some(0);
            }
            10 => {
                receipt.legacy_base_cost = 31;
            }
            11 => {
                receipt.billed_quantity = None;
            }
            12 => {
                w.stored_levels.push((
                    (BranchId::DungeonsOfDoom, 9),
                    StoredLevel {
                        level: w.level.clone(),
                        monsters: vec![],
                        items: vec![(id, w.arena.items[id].clone())],
                        unpaid_items: vec![],
                    },
                ));
            }
            _ => unreachable!(),
        }
        let source = bridge(&w);
        let bytes = source.capture().unwrap();
        let c = config(&w);
        drop(source);
        let restored = OwnerBridge::reconstruct(&bytes, c).unwrap();
        let before = restored.export_native().unwrap();
        let expected = w.record_bill_terms(id, receipt.clone());
        assert!(expected.is_err(), "case {case}");
        assert_eq!(
            restored.record_bill_terms(id, receipt),
            expected,
            "case {case}"
        );
        same(&restored.export_native().unwrap(), &before);
        same(&w, &before);
    }
    let mut w = world();
    let id = wand(&mut w, 1);
    w.record_bill_terms(id, terms(1)).unwrap();
    w.dispose_whole_carried_item(id).unwrap();
    let restored = OwnerBridge::reconstruct(&bridge(&w).capture().unwrap(), config(&w)).unwrap();
    let before = restored.export_native().unwrap();
    assert_eq!(
        restored.record_bill_terms(id, terms(1)),
        Err(BillError::AlreadyConsumed)
    );
    same(&restored.export_native().unwrap(), &before);
}

#[test]
fn owner_runtime_due_timer_import_and_ruleset_refusal() {
    let mut w = world();
    let a = corpse(&mut w, ItemLocation::Limbo);
    let b = corpse(&mut w, ItemLocation::Limbo);
    for id in [a, b] {
        w.start_object_timer(Target::Active(id), Kind::RotCorpse, 0)
            .unwrap();
    }
    let json = w.to_save_json().unwrap();
    let expected = compare_load(&w, &json);
    let source = bridge(&w);
    same(&source.export_native().unwrap(), &expected);
    let bytes = source.capture().unwrap();
    let c = config(&w);
    drop(source);
    let restored = OwnerBridge::reconstruct(&bytes, c).unwrap();
    same(&restored.export_native().unwrap(), &expected);
    let mut wrong = w.ruleset_ref.clone();
    wrong.hash.push_str("-wrong");
    assert!(OwnerBridge::reconstruct(
        &bytes,
        OwnerBridgeConfig::new(w.ruleset.clone(), wrong).unwrap()
    )
    .is_err());
    assert!(OwnerBridge::reconstruct(&bytes[..bytes.len() - 1], config(&w)).is_err());
}
