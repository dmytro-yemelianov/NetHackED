use nethacked_arena::{ItemId, ItemLocation, ItemRecord};
use nethacked_core::ac::ArmorSlot;
use nethacked_sim::{
    actions::equipment::Dressing,
    owner_persistence::{wand_ports::*, OwnerBridge, OwnerBridgeConfig},
    world::BillTerms,
    SimulationWorld,
};
use nethacked_types::Buc;

fn fixture(quantity: u32, charges: i8, liability: u8) -> (SimulationWorld, ItemId) {
    let mut world = SimulationWorld::new_with_seed(724);
    world.hero.skills.skills.clear();
    world.arena.items.clear();
    world.wielded_item = None;
    world.equipment = Default::default();
    world.hero.quivered_item = None;
    world.unpaid_items.clear();
    world.billing = Default::default();
    let item = world
        .ruleset
        .create_item_record(
            "wand of fire",
            ItemLocation::CarriedBy(world.player_id),
            Buc::Uncursed,
        )
        .unwrap();
    let id = world.arena.spawn_item(ItemRecord {
        quantity,
        enchantment: charges,
        ..item
    });
    if liability > 0 {
        world.unpaid_items.push((id, 30));
    }
    if liability == 2 {
        world
            .record_bill_terms(
                id,
                BillTerms {
                    billed_quantity: Some(quantity),
                    unit_price: Some(40),
                    creditor: None,
                    legacy_base_cost: 30,
                },
            )
            .unwrap();
    }
    (world, id)
}
fn config(world: &SimulationWorld) -> OwnerBridgeConfig {
    OwnerBridgeConfig::new(world.ruleset.clone(), world.ruleset_ref.clone())
        .unwrap()
        .with_item_lifetime()
}
fn bridge(world: &SimulationWorld) -> OwnerBridge {
    let json = world.to_save_json().unwrap();
    OwnerBridge::import_native(&json, json.len(), config(world)).unwrap()
}
fn target(id: ItemId) -> ObjectRef {
    ObjectRef {
        id,
        residency: Residency::Active,
    }
}
fn bytes(bridge: &OwnerBridge) -> String {
    bridge.export_native().unwrap().to_save_json().unwrap()
}

#[test]
fn native_five_owner_retirement_preserves_whole_receipt_and_restores() {
    for quantity in [1, 3] {
        for liability in 0..3 {
            let (mut world, id) = fixture(quantity, -1, liability);
            let other = world.arena.spawn_item(
                world
                    .ruleset
                    .create_item_record(
                        "helmet",
                        ItemLocation::CarriedBy(world.player_id),
                        Buc::Uncursed,
                    )
                    .unwrap(),
            );
            world.wielded_item = Some(id);
            world.hero.quivered_item = Some(id);
            world.equipment.armor = vec![(ArmorSlot::Helmet, other)];
            world.equipment.dressing = Some(Dressing {
                item: other,
                putting_on: true,
                turns_left: 3,
            });
            let original = world.arena.items[id].clone();
            let b = bridge(&world);
            let imported = b.export_native().unwrap();
            assert_eq!(
                b.finalize_retained_object(Some(target(id))).unwrap(),
                Ok(LifetimeReply::Retired {
                    billed: liability > 0
                })
            );
            let result = b.export_native().unwrap();
            assert!(result.arena.items.get(id).is_none());
            assert_eq!(result.arena.items[other], world.arena.items[other]);
            assert!(result.wielded_item.is_none());
            assert!(result.hero.quivered_item.is_none());
            assert_eq!(result.equipment.dressing, imported.equipment.dressing);
            assert_eq!(result.equipment.armor, vec![(ArmorSlot::Helmet, other)]);
            assert!(result.unpaid_items.is_empty());
            assert!(result.billing.live_terms.is_empty());
            assert_eq!(result.billing.consumed.len(), usize::from(liability > 0));
            if liability > 0 {
                let receipt = &result.billing.consumed[0];
                let mut detached = original;
                detached.location = ItemLocation::Limbo;
                assert_eq!(receipt.object, detached);
                assert_eq!(receipt.terms.legacy_base_cost, 30);
                assert_eq!(
                    receipt.terms.billed_quantity,
                    (liability == 2).then_some(quantity)
                );
                assert_eq!(receipt.terms.unit_price, (liability == 2).then_some(40));
            }
            assert_eq!(result.rng.get_word_pos(), world.rng.get_word_pos());
            assert_eq!(
                serde_json::to_vec(&result.scheduler).unwrap(),
                serde_json::to_vec(&world.scheduler).unwrap()
            );
            let after = bytes(&b);
            assert_eq!(
                b.finalize_retained_object(Some(target(id))).unwrap(),
                Ok(LifetimeReply::Unchanged)
            );
            assert_eq!(bytes(&b), after);
            let saved = b.capture().unwrap();
            let restored = OwnerBridge::reconstruct(&saved, config(&world)).unwrap();
            assert_eq!(bytes(&restored), after);
            assert_eq!(
                restored.finalize_retained_object(Some(target(id))).unwrap(),
                Ok(LifetimeReply::Unchanged)
            );
        }
    }
}
#[test]
fn native_absent_nonnegative_and_bad_accounting_leave_every_owner_unchanged() {
    for charges in [0, 3] {
        let (world, id) = fixture(3, charges, 2);
        let b = bridge(&world);
        let before = bytes(&b);
        for survivor in [None, Some(target(id))] {
            assert_eq!(
                b.finalize_retained_object(survivor).unwrap(),
                Ok(LifetimeReply::Unchanged)
            );
            assert_eq!(bytes(&b), before);
        }
    }
    let (mut world, id) = fixture(3, -1, 2);
    world.billing.live_terms[0].1.billed_quantity = Some(1);
    world.wielded_item = Some(id);
    world.hero.quivered_item = Some(id);
    let json = world.to_save_json().unwrap();
    assert!(OwnerBridge::import_native(&json, json.len(), config(&world)).is_err());
    assert_eq!(world.to_save_json().unwrap(), json);
}

#[test]
fn native_cached_retirement_preserves_active_generation_and_timer_order() {
    use nethacked_sim::{ObjectTimer, ObjectTimerKind, ObjectTimerTarget, StoredLevel};
    use nethacked_types::BranchId;
    for quantity in [1, 3] {
        for liability in 0..3 {
            let (mut world, id) = fixture(quantity, -1, liability);
            let object = world.arena.items.remove(id).unwrap();
            let replacement = world.arena.spawn_item(ItemRecord {
                enchantment: 3,
                ..object.clone()
            });
            let unpaid_items = std::mem::take(&mut world.unpaid_items);
            world.stored_levels.push((
                (BranchId::DungeonsOfDoom, 9),
                StoredLevel {
                    level: world.level.clone(),
                    monsters: vec![],
                    items: vec![(id, object.clone())],
                    unpaid_items,
                },
            ));
            world.wielded_item = Some(replacement);
            world.hero.quivered_item = Some(replacement);
            let cached = ObjectTimerTarget::Cached {
                branch: BranchId::DungeonsOfDoom,
                depth: 9,
                item: id,
            };
            let entries = vec![
                ObjectTimer {
                    target: ObjectTimerTarget::Active(replacement),
                    kind: ObjectTimerKind::RotCorpse,
                    due: 9,
                    serial: 0,
                },
                ObjectTimer {
                    target: cached,
                    kind: ObjectTimerKind::RotCorpse,
                    due: 9,
                    serial: 1,
                },
                ObjectTimer {
                    target: ObjectTimerTarget::Active(replacement),
                    kind: ObjectTimerKind::ReviveMonster,
                    due: 9,
                    serial: 2,
                },
                ObjectTimer {
                    target: cached,
                    kind: ObjectTimerKind::ReviveMonster,
                    due: 9,
                    serial: 3,
                },
            ];
            world.object_timers =
                serde_json::from_value(serde_json::json!({ "entries": entries, "next_serial": 4 }))
                    .unwrap();
            let b = bridge(&world);
            let before_retirement = bytes(&b);
            assert_eq!(
                b.finalize_retained_object(Some(target(id))).unwrap(),
                Ok(LifetimeReply::Unchanged)
            );
            assert_eq!(bytes(&b), before_retirement);
            let imported_timers = b
                .export_native()
                .unwrap()
                .object_timers
                .entries()
                .iter()
                .copied()
                .filter(|timer| timer.target != cached)
                .collect::<Vec<_>>();
            let retained = ObjectRef {
                id,
                residency: Residency::Cached {
                    branch: BranchId::DungeonsOfDoom,
                    depth: 9,
                },
            };
            assert_eq!(
                b.finalize_retained_object(Some(retained)).unwrap(),
                Ok(LifetimeReply::Retired {
                    billed: liability > 0
                })
            );
            let result = b.export_native().unwrap();
            assert!(result.stored_levels[0].1.items.is_empty());
            assert!(result.stored_levels[0].1.unpaid_items.is_empty());
            assert_eq!(
                result.arena.items[replacement],
                world.arena.items[replacement]
            );
            assert_eq!(result.wielded_item, Some(replacement));
            assert_eq!(result.hero.quivered_item, Some(replacement));
            assert_eq!(result.object_timers.entries(), imported_timers);
            let timer_json = serde_json::to_value(&result.object_timers).unwrap();
            assert_eq!(timer_json["next_serial"], 4);
            assert_eq!(result.billing.consumed.len(), usize::from(liability > 0));
            if liability > 0 {
                let mut detached = object;
                detached.location = ItemLocation::Limbo;
                assert_eq!(result.billing.consumed[0].object, detached);
                assert_eq!(
                    result.billing.consumed[0].terms.billed_quantity,
                    (liability == 2).then_some(quantity)
                );
            }
            let saved = b.capture().unwrap();
            let restored = OwnerBridge::reconstruct(&saved, config(&world)).unwrap();
            assert_eq!(bytes(&restored), bytes(&b));
            assert_eq!(
                restored.finalize_retained_object(Some(retained)).unwrap(),
                Ok(LifetimeReply::Unchanged)
            );
        }
    }
}
#[test]
fn native_scope_and_ambiguous_alias_refuse_before_cleanup() {
    use nethacked_kernel::Scope;
    use nethacked_types::BranchId;
    let (mut world, id) = fixture(3, -1, 0);
    world.wielded_item = Some(id);
    world.hero.quivered_item = Some(id);
    let b = bridge(&world);
    let before = bytes(&b);
    assert!(matches!(
        b.finalize_retained_object_scoped(Some(target(id)), Scope::Keys(Default::default())),
        Err(_) | Ok(Err(_))
    ));
    assert_eq!(bytes(&b), before);
    world.stored_levels.push((
        (BranchId::DungeonsOfDoom, 9),
        nethacked_sim::StoredLevel {
            level: world.level.clone(),
            monsters: vec![],
            items: vec![(id, world.arena.items[id].clone())],
            unpaid_items: vec![],
        },
    ));
    let b = bridge(&world);
    let before = bytes(&b);
    for residency in [
        Residency::Active,
        Residency::Cached {
            branch: BranchId::DungeonsOfDoom,
            depth: 9,
        },
    ] {
        assert_eq!(
            b.finalize_retained_object(Some(ObjectRef { id, residency }))
                .unwrap(),
            Err(LifetimeError::Billing(
                nethacked_sim::world::BillError::DuplicateLedger
            ))
        );
        assert_eq!(bytes(&b), before);
    }
}
#[test]
fn native_production_zap_preserves_empty_policy_and_v3_gameplay() {
    use nethacked_core::ActionAst;
    use nethacked_types::Direction;
    for charges in [-1, 1, 3] {
        let (mut world, id) = fixture(3, charges, 2);
        world.wielded_item = Some(id);
        let new = bridge(&world);
        let json = world.to_save_json().unwrap();
        let historical = OwnerBridge::import_native(
            &json,
            json.len(),
            OwnerBridgeConfig::new(world.ruleset.clone(), world.ruleset_ref.clone())
                .unwrap()
                .with_wand_gameplay(),
        )
        .unwrap();
        let action = ActionAst::ZapSelectedWand {
            item_index: 0,
            expected_item: Some(id),
            dir: Direction::East,
            energy: 0,
        };
        let old = historical.dispatch_wand_action(action.clone()).unwrap();
        let new_reply = new.dispatch_wand_action(action).unwrap();
        assert!(new_reply.error.is_none());
        assert_eq!(new_reply.events, old.events);
        assert_eq!(bytes(&new), bytes(&historical));
        let saved = new.capture().unwrap();
        assert!(OwnerBridge::reconstruct(
            &saved,
            OwnerBridgeConfig::new(world.ruleset.clone(), world.ruleset_ref.clone())
                .unwrap()
                .with_wand_gameplay()
        )
        .is_err());
    }
}

#[test]
fn lifetime_facade_restores_historical_profiles_without_activating_new_code() {
    for version in [1, 2, 3] {
        let (world, id) = fixture(3, -1, 1);
        let json = world.to_save_json().unwrap();
        let old_config =
            OwnerBridgeConfig::new(world.ruleset.clone(), world.ruleset_ref.clone()).unwrap();
        let old_config = match version {
            1 => old_config.with_wand_ports(),
            2 => old_config.with_wand_commands(),
            _ => old_config.with_wand_gameplay(),
        };
        let old = OwnerBridge::import_native(&json, json.len(), old_config).unwrap();
        let saved = old.capture().unwrap();
        let restored = OwnerBridge::reconstruct(&saved, config(&world))
            .unwrap_or_else(|error| panic!("historical profile {version}: {error:?}"));
        let before = bytes(&restored);
        let observation = restored
            .observe_wand(WandRead {
                query: WandQuery::Objects(ObjectRequest::Identity(target(id))),
                expected_owner: None,
            })
            .unwrap();
        assert!(
            matches!(observation.facts, WandFacts::Objects(f) if f.target == target(id) && f.charges == -1)
        );
        assert!(restored.finalize_retained_object(Some(target(id))).is_err());
        assert_eq!(bytes(&restored), before);
        restored
            .record_bill_terms(
                id,
                BillTerms {
                    billed_quantity: Some(3),
                    unit_price: Some(40),
                    creditor: None,
                    legacy_base_cost: 30,
                },
            )
            .unwrap();
        assert_eq!(
            restored.export_native().unwrap().billing.live_terms.len(),
            1
        );
    }
}

#[test]
fn restored_v3_gameplay_executes_through_lifetime_facade() {
    use nethacked_core::ActionAst;
    use nethacked_types::Direction;
    for charges in [-1, 1, 3] {
        let (world, id) = fixture(3, charges, 2);
        let json = world.to_save_json().unwrap();
        let historical = OwnerBridge::import_native(
            &json,
            json.len(),
            OwnerBridgeConfig::new(world.ruleset.clone(), world.ruleset_ref.clone())
                .unwrap()
                .with_wand_gameplay(),
        )
        .unwrap();
        let saved = historical.capture().unwrap();
        let restored = OwnerBridge::reconstruct(&saved, config(&world)).unwrap();
        let action = ActionAst::ZapSelectedWand {
            item_index: 0,
            expected_item: Some(id),
            dir: Direction::East,
            energy: 0,
        };
        let expected = historical.dispatch_wand_action(action.clone()).unwrap();
        let actual = restored.dispatch_wand_action(action).unwrap();
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
        assert_eq!(bytes(&restored), bytes(&historical));
        let before = bytes(&restored);
        assert!(restored.finalize_retained_object(Some(target(id))).is_err());
        assert_eq!(bytes(&restored), before);
    }
}
