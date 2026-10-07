//! Whole-wand source receipt transfer fixtures; no wand activation or settlement.
use nethacked_arena::{ItemDefinition, ItemId, ItemLocation};
use nethacked_data::{CharacterConfig, ItemKindId as I, Ruleset};
use nethacked_sim::world::{
    BillCreditor, BillError, BillTerms, BillValuation, StoredLevel, WholeDisposal,
};
use nethacked_sim::{ActionAst, SimulationWorld};
use nethacked_types::{BranchId, Buc, Coord, Tile};
use rand::RngCore;
use std::sync::Arc;

fn world() -> SimulationWorld {
    let mut rs = (*Ruleset::vanilla()).clone();
    let mut custom = rs.item_by_id(I::WAN_FIRE).unwrap().clone();
    custom.id = None;
    custom.name = "pack rod".into();
    rs.items.push(custom);
    rs.reindex();
    let mut w = SimulationWorld::new_with_character_and_ruleset(
        33,
        CharacterConfig::default(),
        Arc::new(rs),
        Default::default(),
    );
    w.arena.actors.retain(|id, _| id == w.player_id);
    w.arena.items.clear();
    w.unpaid_items.clear();
    w.equipment = Default::default();
    w.object_timers = Default::default();
    w.wielded_item = None;
    w.hero.quivered_item = None;
    w.level = nethacked_dungeon::DungeonLevel::new_solid(Tile::Room);
    w.arena.actors[w.player_id].coord = Coord::new_unchecked(20, 10);
    w.arena.actors[w.player_id].hp = 1000;
    w.arena.actors[w.player_id].max_hp = 1000;
    w.scheduler.hero_energy = 12;
    w.scheduler.hero_speed = 12;
    w
}

fn wand(w: &mut SimulationWorld, q: u32, custom: bool, legacy: bool) -> ItemId {
    let mut object = if custom {
        w.ruleset.create_item_record(
            "pack rod",
            ItemLocation::CarriedBy(w.player_id),
            Buc::Cursed,
        )
    } else {
        w.ruleset.create_item_record_by_id(
            I::WAN_FIRE,
            ItemLocation::CarriedBy(w.player_id),
            Buc::Cursed,
        )
    }
    .unwrap();
    object.quantity = q;
    object.weight = q * 7;
    object.enchantment = -1;
    object.recharged = 3;
    if legacy {
        object.definition = None;
    } else {
        object.name = "renamed display".into();
    }
    w.arena.spawn_item(object)
}

fn terms(w: &SimulationWorld, q: u32) -> BillTerms {
    BillTerms {
        billed_quantity: Some(q),
        unit_price: Some(40),
        legacy_base_cost: 30,
        creditor: Some(BillCreditor {
            branch: w.current_branch,
            depth: w.depth,
            shopkeeper: w.player_id,
        }),
    }
}

fn raw(w: &SimulationWorld) -> Vec<u8> {
    serde_json::to_vec(&serde_json::to_value(w).unwrap()).unwrap()
}

// Raw world includes all active/cache/arena/artifact/equipment/timer, event,
// gold, scheduler and RNG owners. Save bytes are an additional comparison.
fn atomic<T>(w: &mut SimulationWorld, operation: impl FnOnce(&mut SimulationWorld) -> T) -> T {
    let before = raw(w);
    let saved = w.to_save_json().unwrap();
    let next = w.rng.clone().next_u64();
    let result = operation(w);
    assert_eq!(raw(w), before);
    assert_eq!(w.to_save_json().unwrap(), saved);
    assert_eq!(w.rng.clone().next_u64(), next);
    result
}

fn restored(w: &SimulationWorld) -> SimulationWorld {
    SimulationWorld::from_save_json(
        &w.to_save_json().unwrap(),
        Arc::clone(&w.ruleset),
        &w.ruleset_ref,
    )
    .unwrap()
}

#[test]
fn four_source_transfer_cases_all_owned_and_legacy_identities() {
    // 2 quantities * 2 billing cases * 2 owned identities * 2 identity encodings = 16.
    for q in [1, 3] {
        for billed in [false, true] {
            for custom in [false, true] {
                for legacy in [false, true] {
                    let mut w = world();
                    let id = wand(&mut w, q, custom, legacy);
                    let receipt = terms(&w, q);
                    if billed {
                        w.unpaid_items.push((id, 30));
                        w.record_bill_terms(id, receipt.clone()).unwrap();
                    }
                    w.role_name = "Tourist".into();
                    w.arena.items[id].buc = Buc::Blessed;
                    w.arena.items[id].enchantment = 7;
                    w.arena.items[id].recharged = 4;
                    let mut snapshot = w.arena.items[id].clone();
                    snapshot.location = ItemLocation::Limbo;
                    let rng = w.rng.clone();
                    let scheduler = serde_json::to_value(w.scheduler).unwrap();
                    let events = w.event_log.clone();
                    let gold = w.player_gold;
                    assert_eq!(
                        w.dispose_whole_carried_item(id),
                        Ok(if billed {
                            WholeDisposal::RetainedBilled { origin: id }
                        } else {
                            WholeDisposal::Unbilled { origin: id }
                        })
                    );
                    assert!(w.arena.items.get(id).is_none());
                    assert!(!w.arena.items_carried_by(w.player_id).contains(&id));
                    assert!(!w.is_unpaid(id));
                    assert!(w.billing.live_terms.is_empty());
                    assert_eq!(w.rng, rng);
                    assert_eq!(serde_json::to_value(w.scheduler).unwrap(), scheduler);
                    assert_eq!(w.event_log, events);
                    assert_eq!(w.player_gold, gold);
                    if billed {
                        let view = &w.outstanding_bills()[0];
                        assert!(view.outstanding && view.useup);
                        assert_eq!(view.object.as_ref(), Some(&snapshot));
                        assert_eq!(view.terms, receipt);
                        assert_eq!(
                            view.valuation,
                            BillValuation::Known {
                                total: 40 * u64::from(q)
                            }
                        );
                    } else {
                        assert!(w.outstanding_bills().is_empty());
                    }
                    assert_eq!(
                        atomic(&mut w, |w| w.dispose_whole_carried_item(id)),
                        Err(if billed {
                            BillError::AlreadyConsumed
                        } else {
                            BillError::StaleItem
                        })
                    );
                    let replacement = wand(&mut w, q, custom, legacy);
                    assert_ne!(replacement, id);
                    w.unpaid_items.push((replacement, 30));
                    let replacement_terms = terms(&w, q);
                    w.record_bill_terms(replacement, replacement_terms).unwrap();
                    assert_eq!(
                        atomic(&mut w, |w| w.dispose_whole_carried_item(id)),
                        Err(if billed {
                            BillError::AlreadyConsumed
                        } else {
                            BillError::StaleItem
                        })
                    );
                    assert!(w.arena.items.get(replacement).is_some());
                    assert!(w.is_unpaid(replacement));
                }
            }
        }
    }
}

#[test]
fn frozen_admission_repeats_precede_replacement_validation() {
    for q in [1, 3] {
        for custom in [false, true] {
            let mut w = world();
            let id = wand(&mut w, q, custom, false);
            w.unpaid_items.push((id, 30));
            let original = terms(&w, q);
            w.record_bill_terms(id, original.clone()).unwrap();
            let mut variants = vec![original.clone()];
            let mut t = original.clone();
            t.unit_price = Some(90);
            variants.push(t);
            let mut t = original.clone();
            t.billed_quantity = Some(q + 1);
            variants.push(t);
            let mut t = original.clone();
            t.creditor = None;
            variants.push(t);
            let mut t = original.clone();
            t.legacy_base_cost = 9;
            variants.push(t);
            let mut t = original.clone();
            t.billed_quantity = Some(0);
            variants.push(t);
            let mut t = original.clone();
            t.unit_price = None;
            variants.push(t);
            for t in variants {
                assert_eq!(
                    atomic(&mut w, |w| w.record_bill_terms(id, t)),
                    Err(BillError::ReceiptAlreadyRecorded)
                );
                assert_eq!(w.billing.live_terms[0].1, original);
            }
            w.billing.live_terms.push((id, original.clone()));
            assert_eq!(
                atomic(&mut w, |w| w.record_bill_terms(id, original)),
                Err(BillError::DuplicateLedger)
            );
        }
    }
}

#[test]
fn clearing_and_requested_pay_are_complete_refusals_before_forced_wait() {
    for q in [1, 3] {
        for custom in [false, true] {
            for gold in [0, 1000] {
                let mut w = world();
                let id = wand(&mut w, q, custom, false);
                let mixed = wand(&mut w, 1, false, true);
                w.unpaid_items.extend([(id, 30), (mixed, 5)]);
                let original = terms(&w, q);
                w.record_bill_terms(id, original.clone()).unwrap();
                w.role_name = "Tourist".into();
                w.player_gold = gold;
                // Invalid armor reference would be reconciled; helpless would force Wait.
                w.equipment
                    .armor
                    .push((nethacked_core::ac::ArmorSlot::Suit, mixed));
                w.equipment.dressing = Some(nethacked_sim::actions::equipment::Dressing {
                    item: mixed,
                    putting_on: true,
                    turns_left: 2,
                });
                w.hero.afflictions.transient.helpless = 2;
                assert_eq!(
                    atomic(&mut w, |w| w.try_remove_unpaid(id)),
                    Err(BillError::SettlementDeferred)
                );
                atomic(&mut w, |w| w.remove_unpaid(id));
                assert!(atomic(&mut w, |w| w.step_player_action(ActionAst::Pay)).is_empty());
                assert!(w.economy_ledger().is_unpaid(id));
                assert_eq!(
                    w.outstanding_bills()[0].valuation,
                    BillValuation::Known {
                        total: 40 * u64::from(q)
                    }
                );
                w.dispose_whole_carried_item(id).unwrap();
                assert_eq!(w.billing.consumed[0].terms, original);
                assert_eq!(
                    atomic(&mut w, |w| w.try_remove_unpaid(id)),
                    Err(BillError::SettlementDeferred)
                );
                atomic(&mut w, |w| w.remove_unpaid(id));
                assert!(atomic(&mut w, |w| w.step_player_action(ActionAst::Pay)).is_empty());
                assert_eq!(w.billing.consumed.len(), 1);
            }
        }
    }
}

#[test]
fn unknown_legacy_liability_retains_exact_base_object_and_blocks_pay() {
    for q in [1, 3] {
        for custom in [false, true] {
            for legacy in [false, true] {
                let mut w = world();
                let id = wand(&mut w, q, custom, legacy);
                w.unpaid_items.push((id, 30));
                let mut object = w.arena.items[id].clone();
                object.location = ItemLocation::Limbo;
                w.dispose_whole_carried_item(id).unwrap();
                let view = &w.outstanding_bills()[0];
                assert_eq!(view.object.as_ref(), Some(&object));
                assert_eq!(
                    view.valuation,
                    BillValuation::UnresolvedTerms {
                        base: 30,
                        current_consumed_quantity: Some(q)
                    }
                );
                assert_eq!(view.terms.billed_quantity, None);
                assert_eq!(view.terms.unit_price, None);
                assert_eq!(view.terms.creditor, None);
                atomic(&mut w, |w| w.remove_unpaid(id));
                assert!(atomic(&mut w, |w| w.step_player_action(ActionAst::Pay)).is_empty());
            }
        }
    }
}

#[test]
fn invalid_disposal_and_admission_are_atomic() {
    for case in 0..17 {
        let mut w = world();
        let id = wand(&mut w, 3, false, false);
        w.unpaid_items.push((id, 30));
        let mut receipt = terms(&w, 3);
        let expected = match case {
            0 => {
                w.arena.items[id].definition = Some(ItemDefinition::Custom("missing".into()));
                BillError::InvalidDefinition
            }
            1 => {
                w.arena.items[id].location = ItemLocation::Floor(Coord::new_unchecked(20, 10));
                BillError::NotCarried
            }
            2 => {
                w.arena.items[id].quantity = 0;
                BillError::ZeroQuantity
            }
            3 => {
                w.unpaid_items.push((id, 30));
                BillError::DuplicateLedger
            }
            4 => {
                receipt.billed_quantity = Some(2);
                BillError::UnsupportedBillingScope
            }
            5 => {
                receipt.unit_price = None;
                BillError::InvalidReceipt
            }
            6 => {
                receipt.legacy_base_cost = 31;
                BillError::InvalidReceipt
            }
            7 => {
                receipt.billed_quantity = Some(u32::MAX);
                receipt.unit_price = Some(u32::MAX);
                BillError::InvalidReceipt
            }
            8 => {
                w.arena.items[id].is_container = true;
                BillError::UnsupportedBillingScope
            }
            9 => {
                let child = wand(&mut w, 1, false, true);
                w.arena.items[child].location = ItemLocation::InContainer(id);
                BillError::UnsupportedBillingScope
            }
            10 => {
                w.wielded_item = Some(id);
                BillError::LifecycleReferencesPresent
            }
            11 => {
                w.hero.quivered_item = Some(id);
                BillError::LifecycleReferencesPresent
            }
            12 => {
                w.equipment
                    .armor
                    .push((nethacked_core::ac::ArmorSlot::Suit, id));
                BillError::LifecycleReferencesPresent
            }
            13 => {
                w.equipment.dressing = Some(nethacked_sim::actions::equipment::Dressing {
                    item: id,
                    putting_on: true,
                    turns_left: 2,
                });
                BillError::LifecycleReferencesPresent
            }
            14 => {
                w.object_timers = serde_json::from_value(serde_json::json!({"entries":[{"target":{"Active":id},"kind":"RotCorpse","due":999,"serial":0}],"next_serial":1})).unwrap();
                BillError::LifecycleReferencesPresent
            }
            15 => {
                let object = w
                    .ruleset
                    .create_item_record_by_id(
                        I::DAGGER,
                        ItemLocation::CarriedBy(w.player_id),
                        Buc::Uncursed,
                    )
                    .unwrap();
                w.arena.items[id] = object;
                BillError::UnsupportedBillingScope
            }
            _ => {
                w.billing.live_terms.push((id, receipt.clone()));
                w.billing.live_terms.push((id, receipt.clone()));
                BillError::DuplicateLedger
            }
        };
        if (4..=7).contains(&case) {
            assert_eq!(
                atomic(&mut w, |w| w.record_bill_terms(id, receipt.clone())),
                Err(expected.clone())
            );
            w.billing.live_terms.push((id, receipt));
        }
        assert_eq!(
            atomic(&mut w, |w| w.dispose_whole_carried_item(id)),
            Err(expected)
        );
    }
}

#[test]
fn absent_billing_owner_loads_without_fabricating_receipts_active_and_cached() {
    for q in [1, 3] {
        for custom in [false, true] {
            for absent in [false, true] {
                let mut w = world();
                let id = wand(&mut w, q, custom, true);
                let cached = wand(&mut w, q, custom, true);
                let mut cached_object = w.arena.items.remove(cached).unwrap();
                cached_object.location = ItemLocation::Floor(Coord::new_unchecked(20, 10));
                w.unpaid_items.push((id, 30));
                w.stored_levels.push((
                    (BranchId::DungeonsOfDoom, 8),
                    StoredLevel {
                        level: w.level.clone(),
                        monsters: vec![],
                        items: vec![(cached, cached_object.clone())],
                        unpaid_items: vec![(cached, 31)],
                    },
                ));
                let mut wire: serde_json::Value =
                    serde_json::from_str(&w.to_save_json().unwrap()).unwrap();
                if absent {
                    wire.as_object_mut().unwrap().remove("billing");
                }
                let mut loaded = SimulationWorld::from_save_json(
                    &wire.to_string(),
                    Arc::clone(&w.ruleset),
                    &w.ruleset_ref,
                )
                .unwrap();
                let mut control = restored(&w);
                assert_eq!(raw(&loaded), raw(&control));
                assert!(loaded.billing.live_terms.is_empty() && loaded.billing.consumed.is_empty());
                assert_eq!(loaded.stored_levels[0].1.items[0], (cached, cached_object));
                loaded.dispose_whole_carried_item(id).unwrap();
                control.dispose_whole_carried_item(id).unwrap();
                assert_eq!(
                    loaded.outstanding_bills()[1].valuation,
                    BillValuation::UnresolvedTerms {
                        base: 30,
                        current_consumed_quantity: Some(q)
                    }
                );
                assert_eq!(
                    loaded.step_player_action(ActionAst::Wait),
                    control.step_player_action(ActionAst::Wait)
                );
                assert_eq!(raw(&loaded), raw(&control));
                assert_eq!(loaded.rng.next_u64(), control.rng.next_u64());
            }
        }
    }
}

#[test]
fn stable_active_receipts_roundtrip_and_consumption_repeat() {
    for q in [1, 3] {
        for custom in [false, true] {
            let mut w = world();
            let id = wand(&mut w, q, custom, false);
            w.unpaid_items.push((id, 30));
            let receipt = terms(&w, q);
            w.record_bill_terms(id, receipt.clone()).unwrap();
            atomic(&mut w, |w| w.remove_unpaid(id));
            assert!(atomic(&mut w, |w| w.step_player_action(ActionAst::Pay)).is_empty());
            let mut loaded = restored(&w);
            assert_eq!(loaded.outstanding_bills(), w.outstanding_bills());
            assert_eq!(
                w.dispose_whole_carried_item(id),
                loaded.dispose_whole_carried_item(id)
            );
            let mut loaded = restored(&loaded);
            assert_eq!(loaded.billing.consumed[0].terms, receipt);
            assert_eq!(
                atomic(&mut loaded, |w| w.dispose_whole_carried_item(id)),
                Err(BillError::AlreadyConsumed)
            );
            assert!(atomic(&mut loaded, |w| w.step_player_action(ActionAst::Pay)).is_empty());
            assert_eq!(
                w.step_player_action(ActionAst::Wait),
                loaded.step_player_action(ActionAst::Wait)
            );
            assert_eq!(raw(&w), raw(&loaded));
            assert_eq!(w.rng.next_u64(), loaded.rng.next_u64());
        }
    }
}

#[test]
fn cached_only_receipt_blocks_pay_and_empty_owner_keeps_original_paths() {
    let mut w = world();
    let id = wand(&mut w, 3, false, false);
    w.unpaid_items.push((id, 30));
    let receipt = terms(&w, 3);
    w.record_bill_terms(id, receipt.clone()).unwrap();
    let object = w.arena.items.remove(id).unwrap();
    w.unpaid_items.clear();
    w.stored_levels.push((
        (BranchId::DungeonsOfDoom, 8),
        StoredLevel {
            level: w.level.clone(),
            monsters: vec![],
            items: vec![(id, object)],
            unpaid_items: vec![(id, 30)],
        },
    ));
    assert_eq!(w.outstanding_bills()[0].terms, receipt);
    assert!(atomic(&mut w, |w| w.step_player_action(ActionAst::Pay)).is_empty());
    let mut w = world();
    let id = wand(&mut w, 3, false, true);
    w.unpaid_items.push((id, 30));
    w.player_gold = 1000;
    let price = w.get_unpaid_cost(id).unwrap();
    let events = w.step_player_action(ActionAst::Pay);
    assert!(!events.is_empty());
    assert_eq!(w.player_gold, 1000 - price);
    assert!(!w.is_unpaid(id));
    w.unpaid_items.push((id, 30));
    assert_eq!(w.try_remove_unpaid(id), Ok(()));
    assert!(!w.is_unpaid(id));
    w.unpaid_items.push((id, 30));
    w.remove_unpaid(id);
    assert!(!w.is_unpaid(id));
}

#[test]
fn receipt_consistency_stale_admission_and_global_duplicate_preflight() {
    for case in 0..9 {
        let mut w = world();
        let id = wand(&mut w, 3, false, false);
        let receipt = terms(&w, 3);
        if case == 0 {
            assert_eq!(
                atomic(&mut w, |w| w.record_bill_terms(id, receipt)),
                Err(BillError::InvalidReceipt)
            );
            continue;
        }
        w.unpaid_items.push((id, 30));
        w.record_bill_terms(id, receipt.clone()).unwrap();
        let expected = match case {
            1 => {
                w.arena.items[id].quantity = 1;
                BillError::UnsupportedBillingScope
            }
            2 => {
                w.unpaid_items[0].1 = 31;
                BillError::InvalidReceipt
            }
            3 => {
                w.unpaid_items.clear();
                BillError::InvalidReceipt
            }
            4 => {
                w.billing.live_terms[0].1.unit_price = Some(0);
                BillError::InvalidReceipt
            }
            5 => {
                w.arena.items[id].definition = Some(ItemDefinition::Vanilla("UNKNOWN_WAND".into()));
                w.arena.items[id].name = "wand of fire".into();
                BillError::InvalidDefinition
            }
            6 => {
                w.stored_levels.push((
                    (BranchId::DungeonsOfDoom, 8),
                    StoredLevel {
                        level: w.level.clone(),
                        monsters: vec![],
                        items: vec![],
                        unpaid_items: vec![(id, 30)],
                    },
                ));
                BillError::DuplicateLedger
            }
            7 => {
                w.object_timers = serde_json::from_value(serde_json::json!({"entries":[{"target":{"Cached":{"branch":"DungeonsOfDoom","depth":8,"item":id}},"kind":"RotCorpse","due":999,"serial":0}],"next_serial":1})).unwrap();
                BillError::LifecycleReferencesPresent
            }
            _ => {
                w.arena.items.remove(id);
                BillError::StaleItem
            }
        };
        assert_eq!(
            atomic(&mut w, |w| w.dispose_whole_carried_item(id)),
            Err(expected.clone())
        );
        if case == 8 {
            assert_eq!(
                atomic(&mut w, |w| w.record_bill_terms(id, receipt)),
                Err(expected)
            );
        }
    }
    let mut w = world();
    let first = wand(&mut w, 1, true, false);
    let second = wand(&mut w, 3, false, false);
    // Admission order differs from ledger order; live view follows ledger order.
    w.unpaid_items.extend([(first, 30), (second, 30)]);
    let t = terms(&w, 3);
    w.record_bill_terms(second, t).unwrap();
    let t = terms(&w, 1);
    w.record_bill_terms(first, t).unwrap();
    assert_eq!(
        w.outstanding_bills()
            .iter()
            .map(|v| v.origin)
            .collect::<Vec<_>>(),
        vec![first, second]
    );
    w.dispose_whole_carried_item(second).unwrap();
    w.dispose_whole_carried_item(first).unwrap();
    assert_eq!(
        w.outstanding_bills()
            .iter()
            .map(|v| v.origin)
            .collect::<Vec<_>>(),
        vec![second, first]
    );
}
