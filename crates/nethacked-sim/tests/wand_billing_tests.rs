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

// SAVE1 fixtures are appended independently of the nine accepted API1 bodies.
fn save1_world() -> SimulationWorld {
    let mut w = world();
    // Empty learned skills avoid unrelated HashMap serialization order in native save bytes.
    w.hero.skills.skills.clear();
    w
}

fn save1_same(a: &SimulationWorld, b: &SimulationWorld) {
    assert_eq!(raw(a), raw(b));
    assert_eq!(a.to_save_json().unwrap(), b.to_save_json().unwrap());
    assert_eq!(a.rng.clone().next_u64(), b.rng.clone().next_u64());
    assert_eq!(a.outstanding_bills(), b.outstanding_bills());
}

fn save1_wait(a: &mut SimulationWorld, b: &mut SimulationWorld) {
    assert_eq!(
        a.step_player_action(ActionAst::Wait),
        b.step_player_action(ActionAst::Wait)
    );
    save1_same(a, b);
}

fn save1_refuse(w: &mut SimulationWorld, id: ItemId) {
    assert_eq!(
        atomic(w, |w| w.try_remove_unpaid(id)),
        Err(BillError::SettlementDeferred)
    );
    atomic(w, |w| w.remove_unpaid(id));
    assert!(atomic(w, |w| w.step_player_action(ActionAst::Pay)).is_empty());
}

fn save1_load_wire(
    w: &SimulationWorld,
    wire: &serde_json::Value,
) -> Result<SimulationWorld, nethacked_sim::world::LoadError> {
    SimulationWorld::from_save_json(&wire.to_string(), Arc::clone(&w.ruleset), &w.ruleset_ref)
}

#[test]
fn save1_active_admitted_refused_loaded_disposed_repeat_and_allocation() {
    // SAVE1-ACTIVE: 16 native chains, canonical/custom, explicit/absent identity, q1/q3, mixed/only.
    for q in [1, 3] {
        for custom in [false, true] {
            for legacy in [false, true] {
                for mixed in [false, true] {
                    let mut control = save1_world();
                    let id = wand(&mut control, q, custom, legacy);
                    control.unpaid_items.push((id, 30));
                    let receipt = terms(&control, q);
                    control.record_bill_terms(id, receipt.clone()).unwrap();
                    if mixed {
                        let other = wand(&mut control, 1, false, true);
                        control.unpaid_items.push((other, 5));
                    }
                    control.role_name = "Tourist".into();
                    control.player_gold = 1000;
                    save1_refuse(&mut control, id);
                    let mut loaded = restored(&control);
                    save1_same(&control, &loaded);
                    save1_refuse(&mut loaded, id);
                    save1_same(&control, &loaded);
                    assert_eq!(
                        control.dispose_whole_carried_item(id),
                        Ok(WholeDisposal::RetainedBilled { origin: id })
                    );
                    assert_eq!(
                        loaded.dispose_whole_carried_item(id),
                        Ok(WholeDisposal::RetainedBilled { origin: id })
                    );
                    loaded = restored(&loaded);
                    save1_same(&control, &loaded);
                    assert_eq!(loaded.billing.consumed[0].terms, receipt);
                    assert_eq!(loaded.billing.consumed[0].object.quantity, q);
                    assert_eq!(
                        loaded.outstanding_bills().last().unwrap().valuation,
                        BillValuation::Known {
                            total: 40 * u64::from(q)
                        }
                    );
                    save1_refuse(&mut control, id);
                    save1_refuse(&mut loaded, id);
                    assert_eq!(
                        atomic(&mut loaded, |w| w.dispose_whole_carried_item(id)),
                        Err(BillError::AlreadyConsumed)
                    );
                    let a = wand(&mut control, q, custom, legacy);
                    let b = wand(&mut loaded, q, custom, legacy);
                    assert_eq!(a, b);
                    assert_ne!(a, id);
                    save1_same(&control, &loaded);
                    save1_wait(&mut control, &mut loaded);
                }
            }
        }
    }
}

fn save1_depart(w: &mut SimulationWorld) -> Vec<nethacked_sim::GameEvent> {
    // Real public drop and stairs, never constructing a StoredLevel fixture.
    let down = Coord::new_unchecked(20, 10);
    w.level.stairs_down = down;
    w.level.set_tile(down, Tile::Stairs { up: false });
    w.arena.actors[w.player_id].coord = down;
    assert!(!w.step_player_action(ActionAst::Drop(0)).is_empty());
    let events = w.step_player_action(ActionAst::Descend);
    assert_eq!(w.depth, 2);
    events
}

fn save1_return(w: &mut SimulationWorld) -> Vec<nethacked_sim::GameEvent> {
    w.arena.actors[w.player_id].coord = w.level.stairs_up;
    let events = w.step_player_action(ActionAst::Ascend);
    assert_eq!(w.depth, 1);
    events
}

#[test]
fn save1_cached_admitted_refused_loaded_stairs_remap_disposed_and_consumed_trace() {
    // SAVE1-CACHE: 16 chains, two quantities/definitions/identity encodings/consumed presence.
    for q in [1, 3] {
        for custom in [false, true] {
            for legacy in [false, true] {
                for consumed in [false, true] {
                    let mut control = save1_world();
                    if consumed {
                        let old = wand(&mut control, q, custom, legacy);
                        control.unpaid_items.push((old, 30));
                        let receipt = terms(&control, q);
                        control.record_bill_terms(old, receipt).unwrap();
                        control.dispose_whole_carried_item(old).unwrap();
                    }
                    let id = wand(&mut control, q, custom, legacy);
                    control.unpaid_items.push((id, 30));
                    let receipt = terms(&control, q);
                    control.record_bill_terms(id, receipt.clone()).unwrap();
                    let history = control.billing.consumed.clone();
                    save1_refuse(&mut control, id);
                    save1_depart(&mut control);
                    assert!(control.arena.items.get(id).is_none());
                    assert!(control
                        .stored_levels
                        .iter()
                        .any(|(_, level)| level.items.iter().any(|(i, _)| *i == id)));
                    assert_eq!(control.billing.live_terms, vec![(id, receipt.clone())]);
                    assert!(!control.unpaid_items.iter().any(|(i, _)| *i == id));
                    save1_refuse(&mut control, id);
                    let mut loaded = restored(&control);
                    save1_same(&control, &loaded);
                    save1_refuse(&mut loaded, id);
                    save1_wait(&mut control, &mut loaded);
                    assert_eq!(save1_return(&mut control), save1_return(&mut loaded));
                    save1_same(&control, &loaded);
                    let remapped = control.billing.live_terms[0].0;
                    assert_ne!(remapped, id);
                    assert_eq!(loaded.billing.live_terms[0].0, remapped);
                    assert_eq!(control.billing.live_terms[0].1, receipt);
                    assert_eq!(control.billing.consumed, history);
                    assert_eq!(loaded.billing.consumed, history);
                    assert_eq!(
                        control.step_player_action(ActionAst::PickUp),
                        loaded.step_player_action(ActionAst::PickUp)
                    );
                    assert_eq!(
                        control.arena.items[remapped].location,
                        ItemLocation::CarriedBy(control.player_id)
                    );
                    save1_refuse(&mut control, remapped);
                    save1_refuse(&mut loaded, remapped);
                    loaded = restored(&loaded);
                    save1_same(&control, &loaded);
                    assert_eq!(
                        control.dispose_whole_carried_item(remapped),
                        loaded.dispose_whole_carried_item(remapped)
                    );
                    loaded = restored(&loaded);
                    save1_same(&control, &loaded);
                    assert_eq!(loaded.billing.consumed.last().unwrap().terms, receipt);
                    assert_eq!(loaded.billing.consumed[..history.len()], history);
                    assert_eq!(
                        atomic(&mut loaded, |w| w.dispose_whole_carried_item(remapped)),
                        Err(BillError::AlreadyConsumed)
                    );
                    save1_refuse(&mut control, remapped);
                    save1_refuse(&mut loaded, remapped);
                    let a = wand(&mut control, q, custom, legacy);
                    let b = wand(&mut loaded, q, custom, legacy);
                    assert_eq!(a, b);
                    save1_wait(&mut control, &mut loaded);
                }
            }
        }
    }
}

#[test]
fn save1_absent_and_explicit_empty_owner_active_cached_unknown_conservation() {
    // SAVE1-LEGACY: 32 chains; only billing is removed from legacy wire.
    for q in [1, 3] {
        for custom in [false, true] {
            for legacy in [false, true] {
                for absent in [false, true] {
                    for cached in [false, true] {
                        let mut control = save1_world();
                        let id = wand(&mut control, q, custom, legacy);
                        control.unpaid_items.push((id, 30));
                        if cached {
                            save1_depart(&mut control);
                        }
                        let mut wire = serde_json::to_value(&control).unwrap();
                        assert_eq!(
                            wire["billing"],
                            serde_json::json!({"live_terms":[],"consumed":[]})
                        );
                        if absent {
                            wire.as_object_mut().unwrap().remove("billing");
                        }
                        let mut loaded = save1_load_wire(&control, &wire).unwrap();
                        save1_same(&control, &loaded);
                        assert!(loaded.billing.live_terms.is_empty());
                        let current = if cached {
                            assert_eq!(save1_return(&mut control), save1_return(&mut loaded));
                            assert_eq!(
                                control.step_player_action(ActionAst::PickUp),
                                loaded.step_player_action(ActionAst::PickUp)
                            );
                            control.unpaid_items[0].0
                        } else {
                            id
                        };
                        save1_same(&control, &loaded);
                        assert_eq!(
                            control.dispose_whole_carried_item(current),
                            loaded.dispose_whole_carried_item(current)
                        );
                        loaded = restored(&loaded);
                        save1_same(&control, &loaded);
                        let view = loaded.outstanding_bills().last().unwrap().clone();
                        assert_eq!(
                            view.terms,
                            BillTerms {
                                billed_quantity: None,
                                unit_price: None,
                                creditor: None,
                                legacy_base_cost: 30
                            }
                        );
                        assert_eq!(
                            view.valuation,
                            BillValuation::UnresolvedTerms {
                                base: 30,
                                current_consumed_quantity: Some(q)
                            }
                        );
                        save1_refuse(&mut control, current);
                        save1_refuse(&mut loaded, current);
                        assert_eq!(
                            atomic(&mut loaded, |w| w.dispose_whole_carried_item(current)),
                            Err(BillError::AlreadyConsumed)
                        );
                        save1_wait(&mut control, &mut loaded);
                    }
                }
            }
        }
    }
}

#[test]
fn save1_malformed_billing_fails_before_catchup_and_ruleset_mismatch_wins() {
    use nethacked_sim::world::LoadError;
    // SAVE1-MALFORMED: active/cached/consumed corruption; no timer path may mask billing errors.
    for cached in [false, true] {
        for case in 0..12 {
            let mut w = save1_world();
            let id = wand(&mut w, 3, true, false);
            w.unpaid_items.push((id, 30));
            let receipt = terms(&w, 3);
            w.record_bill_terms(id, receipt).unwrap();
            if cached {
                save1_depart(&mut w);
            }
            let mut wire = serde_json::to_value(&w).unwrap();
            match case {
                0 => {
                    let t = wire["billing"]["live_terms"][0].clone();
                    wire["billing"]["live_terms"]
                        .as_array_mut()
                        .unwrap()
                        .push(t);
                }
                1 => {
                    wire["billing"]["live_terms"][0][0] =
                        serde_json::to_value(wand(&mut w, 1, false, false)).unwrap();
                }
                2 => {
                    wire["billing"]["live_terms"][0][1]["legacy_base_cost"] = 31.into();
                }
                3 => {
                    wire["billing"]["live_terms"][0][1]["unit_price"] = 0.into();
                }
                4 => {
                    wire["billing"]["live_terms"][0][1]["billed_quantity"] = 1.into();
                }
                5 => {
                    wire["billing"]["live_terms"][0][1]["unit_price"] = serde_json::Value::Null;
                }
                6 => {
                    wire["billing"]["live_terms"][0][1]["billed_quantity"] = u32::MAX.into();
                    wire["billing"]["live_terms"][0][1]["unit_price"] = u32::MAX.into();
                }
                7..=11 => {
                    // Corrupt the unique object/ledger at its genuine owner.
                    let owner = if cached {
                        &mut wire["stored_levels"][0][1]
                    } else {
                        &mut wire
                    };
                    if case == 7 {
                        owner["unpaid_items"] = serde_json::json!([]);
                    }
                    if case == 8 {
                        let t = owner["unpaid_items"][0].clone();
                        owner["unpaid_items"].as_array_mut().unwrap().push(t);
                    }
                    if case >= 9 {
                        let object = if cached {
                            &mut owner["items"][0][1]
                        } else {
                            // Slotmap JSON contains occupied records under value.
                            owner["arena"]["items"]
                                .as_array_mut()
                                .unwrap()
                                .iter_mut()
                                .find_map(|slot| slot.get_mut("value").filter(|v| !v.is_null()))
                                .unwrap()
                        };
                        match case {
                            9 => {
                                object["definition"] = serde_json::json!({"Custom":"missing"});
                            }
                            10 => {
                                object["quantity"] = 0.into();
                            }
                            _ => {
                                object["is_container"] = true.into();
                            }
                        }
                    }
                }
                _ => unreachable!(),
            }
            // Invalid timer owner would itself fail; billing must win before validation/catchup.
            wire["object_timers"] = serde_json::json!({"entries":[{"target":{"Active":id},"kind":"RotCorpse","due":0,"serial":0}],"next_serial":1});
            let input = wire.to_string();
            let before = raw(&w);
            let next = w.rng.clone().next_u64();
            assert!(
                matches!(save1_load_wire(&w, &wire), Err(LoadError::Json(message)) if message.starts_with("billing:")),
                "cached={cached} case={case}"
            );
            assert_eq!(wire.to_string(), input);
            assert_eq!(raw(&w), before);
            assert_eq!(w.rng.clone().next_u64(), next);
            let mut wrong = w.ruleset_ref.clone();
            wrong.hash = "wrong hash".into();
            assert!(matches!(
                SimulationWorld::from_save_json(&input, Arc::clone(&w.ruleset), &wrong),
                Err(LoadError::RulesetMismatch { .. })
            ));
        }
    }
}

#[test]
fn save1_consumed_corruption_and_historical_origin_validation() {
    use nethacked_sim::world::LoadError;
    // SAVE1-CONSUMED: known and unknown detached records, malformed snapshots and terms.
    for known in [false, true] {
        let mut w = save1_world();
        let mut source_actor = w.arena.actors[w.player_id].clone();
        source_actor.is_player = false;
        source_actor.name = "receipt source actor".into();
        let source_id = w.arena.spawn_actor(source_actor);
        let id = wand(&mut w, 3, true, false);
        w.unpaid_items.push((id, 30));
        if known {
            let mut t = terms(&w, 3);
            t.creditor.as_mut().unwrap().shopkeeper = source_id;
            w.record_bill_terms(id, t).unwrap();
        }
        w.dispose_whole_carried_item(id).unwrap();
        w.arena.destroy_actor(source_id).unwrap();
        let replacement = wand(&mut w, 1, false, true);
        assert_ne!(id, replacement);
        // Historical shopkeeper need not be active. Origin stays the retired full generation.
        let good = serde_json::to_value(&w).unwrap();
        let loaded = save1_load_wire(&w, &good).unwrap();
        assert_eq!(loaded.billing.consumed[0].origin, id);
        assert!(loaded.arena.items.get(id).is_none());
        if known {
            assert_eq!(
                loaded.billing.consumed[0]
                    .terms
                    .creditor
                    .as_ref()
                    .unwrap()
                    .shopkeeper,
                source_id
            );
            assert!(loaded.arena.actors.get(source_id).is_none());
        }
        for case in 0..9 {
            let mut bad = good.clone();
            match case {
                0 => {
                    let c = bad["billing"]["consumed"][0].clone();
                    bad["billing"]["consumed"].as_array_mut().unwrap().push(c);
                }
                1 => {
                    bad["billing"]["consumed"][0]["object"]["quantity"] = 0.into();
                }
                2 => {
                    bad["billing"]["consumed"][0]["object"]["definition"] =
                        serde_json::json!({"Custom":"missing"});
                }
                3 => {
                    bad["billing"]["consumed"][0]["object"]["location"] =
                        serde_json::to_value(ItemLocation::CarriedBy(w.player_id)).unwrap();
                }
                4 => {
                    bad["billing"]["consumed"][0]["origin"] =
                        serde_json::to_value(replacement).unwrap();
                }
                5 => {
                    bad["billing"]["consumed"][0]["terms"]["billed_quantity"] = 2.into();
                    bad["billing"]["consumed"][0]["terms"]["unit_price"] = 40.into();
                }
                6 => {
                    bad["billing"]["consumed"][0]["terms"]["unit_price"] = 0.into();
                }
                7 => {
                    bad["billing"]["consumed"][0]["object"]["is_container"] = true.into();
                }
                _ => {
                    bad["billing"]["live_terms"] = serde_json::json!([[id, terms(&w, 3)]]);
                }
            }
            bad["object_timers"] = serde_json::json!({"entries":[{"target":{"Active":id},"kind":"RotCorpse","due":0,"serial":0}],"next_serial":1});
            assert!(
                matches!(save1_load_wire(&w, &bad), Err(LoadError::Json(message)) if message.starts_with("billing:")),
                "known={known} case={case}"
            );
        }
    }
}

#[test]
fn save1_duplicate_json_owners_rejected_and_legacy_pay_unchanged_after_load() {
    use nethacked_sim::world::LoadError;
    let w = save1_world();
    let json = w.to_save_json().unwrap();
    let duplicate = format!(
        "{{\"billing\":{{\"live_terms\":[],\"consumed\":[]}},{}",
        &json[1..]
    );
    assert!(
        matches!(SimulationWorld::from_save_json(&duplicate, Arc::clone(&w.ruleset), &w.ruleset_ref), Err(LoadError::Json(message)) if message.contains("duplicate field"))
    );
    for absent in [false, true] {
        let mut control = save1_world();
        let id = wand(&mut control, 3, false, true);
        control.unpaid_items.push((id, 30));
        control.player_gold = 1000;
        let mut wire = serde_json::to_value(&control).unwrap();
        if absent {
            wire.as_object_mut().unwrap().remove("billing");
        }
        let mut loaded = save1_load_wire(&control, &wire).unwrap();
        let price = control.get_unpaid_cost(id).unwrap();
        assert_eq!(
            control.step_player_action(ActionAst::Pay),
            loaded.step_player_action(ActionAst::Pay)
        );
        assert_eq!(loaded.player_gold, 1000 - price);
        assert!(!loaded.is_unpaid(id));
        save1_same(&control, &loaded);
        control.unpaid_items.push((id, 30));
        loaded.unpaid_items.push((id, 30));
        assert_eq!(control.try_remove_unpaid(id), Ok(()));
        assert_eq!(loaded.try_remove_unpaid(id), Ok(()));
        save1_wait(&mut control, &mut loaded);
    }
}

#[test]
fn save1_global_receipt_order_remaps_only_restored_ids() {
    // SAVE1-ORDER: three live receipts in admission order, two restored and one carried.
    let mut control = save1_world();
    let first = wand(&mut control, 1, false, false);
    let second = wand(&mut control, 3, true, false);
    let stay = wand(&mut control, 1, true, true);
    control
        .unpaid_items
        .extend([(first, 30), (second, 30), (stay, 30)]);
    for (id, q) in [(stay, 1), (second, 3), (first, 1)] {
        let t = terms(&control, q);
        control.record_bill_terms(id, t).unwrap();
    }
    let before_terms = control.billing.live_terms.clone();
    let down = Coord::new_unchecked(20, 10);
    control.level.stairs_down = down;
    control.level.set_tile(down, Tile::Stairs { up: false });
    control.arena.actors[control.player_id].coord = down;
    for id in [first, second] {
        let index = control
            .arena
            .items_carried_by(control.player_id)
            .iter()
            .position(|i| *i == id)
            .unwrap();
        assert!(!control
            .step_player_action(ActionAst::Drop(index))
            .is_empty());
    }
    assert!(!control.step_player_action(ActionAst::Descend).is_empty());
    assert_eq!(control.depth, 2);
    assert_eq!(control.billing.live_terms, before_terms);
    let mut loaded = restored(&control);
    save1_same(&control, &loaded);
    assert_eq!(save1_return(&mut control), save1_return(&mut loaded));
    save1_same(&control, &loaded);
    assert_eq!(loaded.billing.live_terms[0], before_terms[0]);
    for index in [1, 2] {
        let (new, t) = &loaded.billing.live_terms[index];
        assert_ne!(*new, before_terms[index].0);
        assert_eq!(t, &before_terms[index].1);
        assert!(loaded.is_unpaid(*new));
        assert_eq!(
            loaded.arena.items[*new].quantity,
            t.billed_quantity.unwrap()
        );
    }
    assert_eq!(
        wand(&mut control, 1, false, false),
        wand(&mut loaded, 1, false, false)
    );
    save1_wait(&mut control, &mut loaded);
}

#[test]
fn save1_cross_owner_duplicate_dangling_and_contents_rejected() {
    use nethacked_sim::world::LoadError;
    // SAVE1-OWNERS: corrupt actual public-stair cache at the JSON input boundary.
    let mut w = save1_world();
    let id = wand(&mut w, 3, false, false);
    w.unpaid_items.push((id, 30));
    let t = terms(&w, 3);
    w.record_bill_terms(id, t).unwrap();
    save1_depart(&mut w);
    let good = serde_json::to_value(&w).unwrap();
    for case in 0..5 {
        let mut bad = good.clone();
        match case {
            0 => {
                let owner = bad["stored_levels"][0].clone();
                bad["stored_levels"].as_array_mut().unwrap().push(owner);
            }
            1 => {
                let object = bad["stored_levels"][0][1]["items"][0].clone();
                bad["stored_levels"][0][1]["items"]
                    .as_array_mut()
                    .unwrap()
                    .push(object);
            }
            2 => {
                let tuple = bad["stored_levels"][0][1]["unpaid_items"][0].clone();
                bad["unpaid_items"].as_array_mut().unwrap().push(tuple);
            }
            3 => {
                let tuple = bad["stored_levels"][0][1]["unpaid_items"][0].clone();
                bad["stored_levels"][0][1]["unpaid_items"] = serde_json::json!([]);
                bad["unpaid_items"].as_array_mut().unwrap().push(tuple);
            }
            _ => {
                let mut child = bad["stored_levels"][0][1]["items"][0].clone();
                child[0] = serde_json::json!({"idx":999,"version":1});
                child[1]["location"] = serde_json::to_value(ItemLocation::InContainer(id)).unwrap();
                bad["stored_levels"][0][1]["items"]
                    .as_array_mut()
                    .unwrap()
                    .push(child);
            }
        }
        assert!(
            matches!(save1_load_wire(&w, &bad), Err(LoadError::Json(message)) if message.starts_with("billing:")),
            "case={case}"
        );
    }
    save1_same(&w, &save1_load_wire(&w, &good).unwrap());
}

#[test]
fn save1_consumed_only_public_stairs_load_repeat_preserves_history() {
    // SAVE1-HISTORY: consumed-only liability never reinserted/remapped during public stairs.
    for q in [1, 3] {
        for custom in [false, true] {
            let mut control = save1_world();
            let id = wand(&mut control, q, custom, false);
            control.unpaid_items.push((id, 30));
            let t = terms(&control, q);
            control.record_bill_terms(id, t).unwrap();
            control.dispose_whole_carried_item(id).unwrap();
            let history = control.billing.consumed.clone();
            let down = Coord::new_unchecked(20, 10);
            control.level.stairs_down = down;
            control.level.set_tile(down, Tile::Stairs { up: false });
            control.arena.actors[control.player_id].coord = down;
            control.step_player_action(ActionAst::Descend);
            assert_eq!(control.depth, 2);
            let mut loaded = restored(&control);
            save1_same(&control, &loaded);
            save1_refuse(&mut loaded, id);
            assert_eq!(save1_return(&mut control), save1_return(&mut loaded));
            save1_same(&control, &loaded);
            loaded = restored(&loaded);
            assert_eq!(loaded.billing.consumed, history);
            assert!(loaded.billing.live_terms.is_empty());
            assert!(!loaded.is_unpaid(id));
            assert!(loaded.arena.items.get(id).is_none());
            assert_eq!(
                atomic(&mut loaded, |w| w.dispose_whole_carried_item(id)),
                Err(BillError::AlreadyConsumed)
            );
            save1_wait(&mut control, &mut loaded);
        }
    }
}
