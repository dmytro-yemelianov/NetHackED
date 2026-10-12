use super::super::*;
#[test]
fn dressing_install_profiles_fit_actual_manifests() {
    for dressing in [false, true] {
        let w = SimulationWorld::new_with_seed(721);
        let mut c = OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
            .unwrap()
            .with_natural_hp_regeneration();
        if dressing {
            c = c.with_dressing_ticks();
        }
        assert_eq!(profile_caller(&c).dependencies.len(), 64);
        assert_eq!(profile_provider(&c).exports.len(), 64);
        OwnerBridge::install(NativePartitions::from_native(w), c).unwrap();
    }
}
fn fixture(
    turns: u32,
    putting_on: bool,
    dead: bool,
) -> (OwnerBridge, OwnerBridgeConfig, SimulationWorld) {
    use crate::actions::equipment::Dressing;
    use nethacked_core::ac::ArmorSlot;
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
            "ring mail",
            nethacked_arena::ItemLocation::CarriedBy(w.player_id),
            nethacked_types::Buc::Uncursed,
        )
        .unwrap();
    let id = w.arena.spawn_item(item);
    w.equipment.armor = vec![(ArmorSlot::Suit, id), (ArmorSlot::Suit, id)];
    w.equipment.dressing = Some(Dressing {
        item: id,
        putting_on,
        turns_left: turns,
    });
    w.divine_protection = 2;
    w.hero.afflictions.transient.helpless = 5;
    w.arena.actors[w.player_id].ac = 99;
    w.arena.actors[w.player_id].is_dead = dead;
    let c = OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
        .unwrap()
        .with_dressing_ticks();
    (
        OwnerBridge::install(NativePartitions::from_native(w.clone()), c.clone()).unwrap(),
        c,
        w,
    )
}
fn native(b: &OwnerBridge) -> String {
    b.export_native().unwrap().to_save_json().unwrap()
}
fn snapshot(b: &OwnerBridge) -> (Vec<Vec<u8>>, Vec<owners::CommitReceipt>) {
    b.session
        .with_runtime(&b.credential.borrow(), |r| {
            Ok::<_, owners::Error>((
                vec![
                    r.owner::<EquipmentState>("Equipment")
                        .unwrap()
                        .owner
                        .inspect()?,
                    r.owner::<ActorsState>("Actors").unwrap().owner.inspect()?,
                    r.owner::<HeroState>("Hero").unwrap().owner.inspect()?,
                    r.owner::<ClockState>("Clock").unwrap().owner.inspect()?,
                    r.owner::<RandomState>("Random").unwrap().owner.inspect()?,
                    r.owner::<GeographyState>("Geography")
                        .unwrap()
                        .owner
                        .inspect()?,
                    r.owner::<BillingState>("Billing")
                        .unwrap()
                        .owner
                        .inspect()?,
                    r.owner::<ObjectsState>("Objects")
                        .unwrap()
                        .owner
                        .inspect()?,
                    r.owner::<SessionState>("Session")
                        .unwrap()
                        .owner
                        .inspect()?,
                ],
                r.host().history()?,
            ))
        })
        .unwrap()
        .unwrap()
}
#[test]
fn raw_dressing_exact_native_countdown_and_restore() {
    for turns in [0, 1, 2] {
        for putting_on in [false, true] {
            for dead in [false, true] {
                let (b, c, mut oracle) = fixture(turns, putting_on, dead);
                let item = oracle.equipment.armor[0].1;
                let clock = serde_json::to_value(oracle.scheduler).unwrap();
                let rng = oracle.rng.get_word_pos();
                let mut events = vec![];
                oracle.tick_dressing(&mut events);
                oracle.record_events(&events);
                let reply = b.tick_dressing().unwrap();
                let finished = turns == 1 && !dead;
                let literal_events = if finished {
                    vec![crate::GameEvent::DressingFinished { item, putting_on }]
                } else {
                    vec![]
                };
                assert_eq!(reply.events, literal_events);
                let next = b.export_native().unwrap();
                let expected_armor = if finished && !putting_on {
                    vec![]
                } else {
                    vec![(nethacked_core::ac::ArmorSlot::Suit, item)]
                };
                assert_eq!(next.equipment.armor, expected_armor);
                if turns == 2 && !dead {
                    assert_eq!(
                        next.equipment.dressing,
                        Some(crate::actions::equipment::Dressing {
                            item,
                            putting_on,
                            turns_left: 1
                        })
                    );
                } else {
                    assert!(next.equipment.dressing.is_none());
                }
                assert_eq!(
                    next.arena.actors[next.player_id].ac,
                    if finished {
                        if putting_on {
                            5
                        } else {
                            8
                        }
                    } else {
                        99
                    }
                );
                assert_eq!(reply.events, events);
                assert!(reply.error.is_none());
                assert_eq!(native(&b), oracle.to_save_json().unwrap());
                assert_eq!(serde_json::to_value(oracle.scheduler).unwrap(), clock);
                assert_eq!(oracle.rng.get_word_pos(), rng);
                let saved = b.capture().unwrap();
                let restored = OwnerBridge::reconstruct(&saved, c).unwrap();
                assert_eq!(native(&restored), native(&b));
                assert_eq!(snapshot(&restored).0, snapshot(&b).0);
                assert_eq!(persistent_history(&restored), persistent_history(&b));
            }
        }
    }
}
#[test]
fn raw_dressing_authentic_stale_and_two_calls() {
    let (b, _, _) = fixture(2, true, false);
    let request = b.prepare_dressing_tick().unwrap();
    b.tick_dressing().unwrap();
    let before = (native(&b), snapshot(&b));
    let stale = super::super::dressing_ticks::stale_refusals();
    assert!(matches!(
        b.execute_dressing_tick(request),
        Err(OwnerBridgeError::Kernel(kernel::Error::NativeRefusal))
    ));
    assert_eq!(super::super::dressing_ticks::stale_refusals(), stale + 1);
    assert_eq!((native(&b), snapshot(&b)), before);
    let mut oracle = b.export_native().unwrap();
    let mut events = vec![];
    oracle.tick_dressing(&mut events);
    oracle.record_events(&events);
    assert_eq!(b.tick_dressing().unwrap().events, events);
    assert_eq!(native(&b), oracle.to_save_json().unwrap());
}
#[test]
fn actual_provisional_wrong_alive_reaches_invariant() {
    let (b, _, _) = fixture(1, true, true);
    let before = snapshot(&b);
    let request = b.prepare_dressing_tick().unwrap();
    let refusals = super::super::dressing_ticks::refusals();
    super::super::dressing_ticks::wrong_alive(true);
    assert!(b.execute_dressing_tick(request).is_err());
    super::super::dressing_ticks::wrong_alive(false);
    assert_eq!(super::super::dressing_ticks::refusals(), refusals + 1);
    assert_eq!(snapshot(&b), before);
}
#[test]
fn raw_dressing_finite32_accepts33_refuses_without_history() {
    for count in [32, 33] {
        let (_, c, mut w) = fixture(2, true, false);
        let id = w.equipment.armor[0].1;
        w.equipment.armor = vec![(nethacked_core::ac::ArmorSlot::Suit, id); count];
        w.equipment.dressing = None;
        let mut oracle = w.clone();
        let b = OwnerBridge::install(NativePartitions::from_native(w), c).unwrap();
        let before = snapshot(&b);
        if count == 32 {
            let mut events = vec![];
            oracle.tick_dressing(&mut events);
            oracle.record_events(&events);
            assert!(b.tick_dressing().unwrap().events.is_empty());
            assert_eq!(native(&b), oracle.to_save_json().unwrap());
        } else {
            assert!(b.prepare_dressing_tick().is_err());
            assert_eq!(snapshot(&b), before);
        }
    }
}
#[test]
fn actual_session_denial_keeps_committed_prefix_and_reload() {
    let (b, c, mut oracle) = fixture(1, false, false);
    let oldlog = oracle.event_log.clone();
    let mut events = vec![];
    oracle.tick_dressing(&mut events);
    oracle.event_log = oldlog;
    let req = b.prepare_dressing_tick().unwrap();
    let scope = b
        .session
        .with_runtime(&b.credential.borrow(), |r| {
            let mut keys = BTreeSet::from([r.host().publication_resource()]);
            keys.insert(
                r.owner::<HeroState>("Hero")
                    .unwrap()
                    .owner
                    .resource("frame_hero_i1")?,
            );
            keys.insert(
                r.owner::<ObjectsState>("Objects")
                    .unwrap()
                    .owner
                    .resource("frame_items_i1")?,
            );
            for n in ["frame_actor_i1", "frame_ac_i1"] {
                keys.insert(
                    r.owner::<ActorsState>("Actors")
                        .unwrap()
                        .owner
                        .resource(n)?,
                );
            }
            for n in ["frame_equipment_i1", "frame_reconcile_i1"] {
                keys.insert(
                    r.owner::<EquipmentState>("Equipment")
                        .unwrap()
                        .owner
                        .resource(n)?,
                );
            }
            Ok::<_, owners::Error>(Scope::Keys(keys))
        })
        .unwrap()
        .unwrap();
    let before = snapshot(&b);
    let reply = b.execute_dressing_tick_scoped(req, scope).unwrap();
    assert_eq!(reply.events, events);
    assert_eq!(reply.error.as_deref(), Some("Authority"));
    assert_eq!(native(&b), oracle.to_save_json().unwrap());
    let after = snapshot(&b);
    assert_eq!(after.0[8], before.0[8]);
    assert_eq!(after.1[..before.1.len()], before.1);
    assert_eq!(after.1.len(), before.1.len() + 1);
    let restored = OwnerBridge::reconstruct(&b.capture().unwrap(), c).unwrap();
    assert_eq!(native(&restored), native(&b));
    assert_eq!(snapshot(&restored).0, after.0);
    assert_eq!(persistent_history(&restored), persistent_history(&b));
}

fn persistent_history(b: &OwnerBridge) -> Vec<op::HistoricalCommit> {
    b.session
        .with_runtime(&b.credential.borrow(), |r| r.host().persistence_history())
        .unwrap()
        .unwrap()
}
#[test]
fn missing_stale_and_wrong_definition_reconcile_without_ac() {
    for case in 0..6 {
        let (_, c, mut w) = fixture(2, true, false);
        let id = w.equipment.armor[0].1;
        match case {
            0 => {
                w.equipment.dressing = None;
            }
            1 => {
                w.arena.items.remove(id);
            }
            2 => {
                w.arena.items.get_mut(id).unwrap().location =
                    nethacked_arena::ItemLocation::Floor(w.arena.actors[w.player_id].coord);
            }
            3 => {
                w.arena.items.get_mut(id).unwrap().class = nethacked_types::ItemClass::Weapon;
            }
            4 => {
                w.arena.items.get_mut(id).unwrap().name = "helmet".into();
            }
            _ => {
                w.arena.actors.remove(w.player_id);
            }
        }
        let mut oracle = w.clone();
        let mut events = vec![];
        oracle.tick_dressing(&mut events);
        assert!(events.is_empty());
        let expected = NativePartitions::from_native(oracle);
        let b = OwnerBridge::install(NativePartitions::from_native(w), c).unwrap();
        let before = snapshot(&b);
        assert!(b.tick_dressing().unwrap().events.is_empty());
        let after = snapshot(&b);
        assert_eq!(after.0[0], encode(&expected.equipment, PAYLOAD).unwrap());
        assert_eq!(after.0[1], encode(&expected.actors, PAYLOAD).unwrap());
        assert_eq!(after.0[2..], before.0[2..]);
        assert_eq!(after.1[..before.1.len()], before.1);
        assert_eq!(after.1.last().unwrap().events, Vec::<Vec<u8>>::new());
    }
}
fn historical_continuation(b: &OwnerBridge, phase: u8) -> Vec<u8> {
    match phase {
        0 => encode(
            &b.execute_default_cancel(b.prepare_default_cancel().unwrap())
                .unwrap(),
            wand::MAX_RESPONSE_BYTES,
        )
        .unwrap(),
        1 => encode(
            &b.execute_affliction_phase(b.prepare_affliction_phase().unwrap())
                .unwrap(),
            wand::MAX_RESPONSE_BYTES,
        )
        .unwrap(),
        2 => encode(
            &b.execute_metabolic_phase(b.prepare_metabolic_phase().unwrap())
                .unwrap(),
            wand::MAX_RESPONSE_BYTES,
        )
        .unwrap(),
        _ => encode(
            &b.execute_regeneration_phase(b.prepare_regeneration_phase(true).unwrap())
                .unwrap(),
            wand::MAX_RESPONSE_BYTES,
        )
        .unwrap(),
    }
}
#[test]
fn old_profile_capture_has_no_dressing_authority_and_supported_continuation() {
    use rand::RngCore;
    for phase in 0..4 {
        let (_, c, mut w) = fixture(2, true, false);
        w.equipment.armor.truncate(1);
        w.equipment.dressing = None;
        w.hero.afflictions.transient.helpless = 0;
        w.arena.actors[w.player_id].hp = 3;
        w.arena.actors[w.player_id].max_hp = 30;
        w.scheduler.turn = 19;
        let base = OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone()).unwrap();
        let old = match phase {
            0 => base.with_default_commands(),
            1 => base.with_passive_affliction_ticks(),
            2 => base.with_passive_metabolic_ticks(),
            _ => base.with_natural_hp_regeneration(),
        };
        let original = OwnerBridge::install(NativePartitions::from_native(w), old).unwrap();
        let restored = OwnerBridge::reconstruct(&original.capture().unwrap(), c).unwrap();
        assert_eq!(snapshot(&restored).0, snapshot(&original).0);
        assert_eq!(persistent_history(&restored), persistent_history(&original));
        let before = (snapshot(&restored), persistent_history(&restored));
        assert!(restored.prepare_dressing_tick().is_err());
        assert_eq!((snapshot(&restored), persistent_history(&restored)), before);
        assert_eq!(
            historical_continuation(&restored, phase),
            historical_continuation(&original, phase)
        );
        assert_eq!(native(&restored), native(&original));
        let mut left = restored.export_native().unwrap();
        let mut right = original.export_native().unwrap();
        assert_eq!(left.rng.get_word_pos(), right.rng.get_word_pos());
        for _ in 0..8 {
            assert_eq!(left.rng.next_u64(), right.rng.next_u64());
        }
    }
}
#[test]
fn genuine_objects_revision_invalidates_prepared_dressing_guard() {
    let (_, c, mut w) = fixture(2, true, false);
    w.equipment.armor.truncate(1);
    w.arena.actors[w.player_id].hp = 3;
    w.arena.actors[w.player_id].max_hp = 30;
    w.scheduler.turn = 19;
    let mut wand = w
        .ruleset
        .create_item_record(
            "wand of fire",
            nethacked_arena::ItemLocation::CarriedBy(w.player_id),
            nethacked_types::Buc::Uncursed,
        )
        .unwrap();
    wand.enchantment = 3;
    let id = w.arena.spawn_item(wand);
    let b = OwnerBridge::install(NativePartitions::from_native(w.clone()), c).unwrap();
    let dressing = b.prepare_dressing_tick().unwrap();
    let regeneration = b.prepare_regeneration_phase(true).unwrap();
    let revision = b
        .session
        .with_runtime(&b.credential.borrow(), |r| {
            r.owner::<ObjectsState>("Objects")
                .unwrap()
                .owner
                .stamp()
                .map(|s| s.revision)
        })
        .unwrap()
        .unwrap();
    let before = snapshot(&b);
    super::super::regeneration_ticks::mutate_object_on_run(id);
    assert!(matches!(
        b.execute_regeneration_phase(regeneration),
        Err(OwnerBridgeError::Kernel(kernel::Error::NativeRefusal))
    ));
    let post = super::super::regeneration_ticks::object_mutation_state().unwrap();
    let after = snapshot(&b);
    let after_revision = b
        .session
        .with_runtime(&b.credential.borrow(), |r| {
            r.owner::<ObjectsState>("Objects")
                .unwrap()
                .owner
                .stamp()
                .map(|s| s.revision)
        })
        .unwrap()
        .unwrap();
    assert_eq!(after_revision, revision + 1);
    assert_eq!(
        (&after.0[7], &after.0[1], &after.0[4], &after.1),
        (&post.0, &post.1, &post.2, &post.3)
    );
    for index in [0, 1, 2, 3, 4, 5, 6, 8] {
        assert_eq!(after.0[index], before.0[index]);
    }
    assert_eq!(after.1[..before.1.len()], before.1);
    assert_eq!(after.1.len(), before.1.len() + 1);
    w.arena.items[id].enchantment = 2;
    assert_eq!(native(&b), w.to_save_json().unwrap());
    let stale = super::super::dressing_ticks::stale_refusals();
    assert!(matches!(
        b.execute_dressing_tick(dressing),
        Err(OwnerBridgeError::Kernel(kernel::Error::NativeRefusal))
    ));
    assert_eq!(super::super::dressing_ticks::stale_refusals(), stale + 1);
    assert_eq!(snapshot(&b), after);
    assert_eq!(native(&b), w.to_save_json().unwrap());
}
