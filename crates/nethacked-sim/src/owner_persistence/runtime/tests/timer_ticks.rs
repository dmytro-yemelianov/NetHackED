use super::super::*;
use nethacked_arena::ItemLocation;
use nethacked_types::{Coord, Tile};
use rand::{RngCore, SeedableRng};

fn fixture(seed: u64, failure: bool) -> (SimulationWorld, ItemId) {
    let mut w = SimulationWorld::new_with_seed(seed);
    w.arena.actors.retain(|id, _| id == w.player_id);
    w.arena.items = Default::default();
    w.equipment = Default::default();
    w.wielded_item = None;
    w.hero.quivered_item = None;
    w.unpaid_items.clear();
    w.hero.skills.skills.clear();
    w.level = nethacked_dungeon::DungeonLevel::new_solid(Tile::Room);
    w.arena.actors[w.player_id].coord = Coord::new_unchecked(5, 5);
    w.arena.actors[w.player_id].level = 18;
    w.depth = 10;
    w.scheduler.turn = 73;
    w.rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
    let mut item = w
        .ruleset
        .create_corpse_record("troll", ItemLocation::Floor(Coord::new_unchecked(20, 10)))
        .unwrap();
    item.corpse_no_revive = failure;
    let id = w.arena.spawn_item(item);
    w.start_object_timer(
        crate::ObjectTimerTarget::Active(id),
        crate::ObjectTimerKind::ReviveMonster,
        0,
    )
    .unwrap();
    (w, id)
}

fn config(w: &SimulationWorld) -> OwnerBridgeConfig {
    OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
        .unwrap()
        .with_timer_ticks()
}

fn install_bridge(w: &SimulationWorld) -> OwnerBridge {
    OwnerBridge::install(NativePartitions::from_native(w.clone()), config(w)).unwrap()
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

fn persistent_history(b: &OwnerBridge) -> Vec<op::HistoricalCommit> {
    b.session
        .with_runtime(&b.credential.borrow(), |r| r.host().persistence_history())
        .unwrap()
        .unwrap()
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
        3 => encode(
            &b.execute_regeneration_phase(b.prepare_regeneration_phase(true).unwrap())
                .unwrap(),
            wand::MAX_RESPONSE_BYTES,
        )
        .unwrap(),
        _ => encode(
            &b.execute_dressing_tick(b.prepare_dressing_tick().unwrap())
                .unwrap(),
            wand::MAX_RESPONSE_BYTES,
        )
        .unwrap(),
    }
}

#[test]
fn timer_install_profiles_fit_actual_manifests() {
    for timer in [false, true] {
        let w = SimulationWorld::new_with_seed(721);
        let mut c = OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
            .unwrap()
            .with_dressing_ticks();
        if timer {
            c = c.with_timer_ticks();
        }
        assert_eq!(profile_caller(&c).dependencies.len(), 64);
        assert_eq!(profile_provider(&c).exports.len(), 64);
        OwnerBridge::install(NativePartitions::from_native(w), c).unwrap();
    }
}

#[test]
fn timer_tick_ordinary_troll_revival_matches_oracle() {
    let (mut oracle, id) = fixture(804, false);
    let bridge = install_bridge(&oracle);
    let expected_events = oracle.run_object_timers();
    let reply = bridge.tick_timer(id).unwrap();

    assert!(matches!(
        reply.outcome,
        timer_ticks::Outcome::Revived { .. }
    ));
    assert_eq!(reply.events, expected_events);

    let actual_world = bridge.export_native().unwrap();
    assert_eq!(
        actual_world.to_save_json().unwrap(),
        oracle.to_save_json().unwrap()
    );
    assert_eq!(actual_world.arena.items.get(id), None);
    assert_eq!(actual_world.arena.actors.len(), oracle.arena.actors.len());

    let mut actual_rng = actual_world.rng.clone();
    let mut oracle_rng = oracle.rng.clone();
    for _ in 0..10 {
        assert_eq!(actual_rng.next_u64(), oracle_rng.next_u64());
    }
}

#[test]
fn timer_tick_ordinary_troll_failure_schedules_rot_timer() {
    let (mut oracle, id) = fixture(804, true);
    let bridge = install_bridge(&oracle);
    let expected_events = oracle.run_object_timers();
    let reply = bridge.tick_timer(id).unwrap();

    assert!(matches!(reply.outcome, timer_ticks::Outcome::Failed { .. }));
    assert_eq!(reply.events, expected_events);

    let actual_world = bridge.export_native().unwrap();
    assert_eq!(
        actual_world.to_save_json().unwrap(),
        oracle.to_save_json().unwrap()
    );
    assert!(actual_world.arena.items.get(id).is_some());
    assert_eq!(
        actual_world.object_timers.entries(),
        oracle.object_timers.entries()
    );
}

#[test]
fn timer_tick_authentic_stale_refused() {
    let (oracle, id) = fixture(804, false);
    let bridge = install_bridge(&oracle);
    let mut req = bridge.prepare_timer_tick(id).unwrap();

    // Tampering with witness stamp triggers authentic stale refusal check
    req.guards[0].generation = req.guards[0].generation.wrapping_add(1);
    let err = bridge.execute_timer_tick(req);
    assert!(matches!(
        err,
        Err(OwnerBridgeError::Owner(owners::Error::Stale))
    ));
    assert!(timer_ticks::stale_refusals() >= 1);
}

#[test]
fn timer_tick_wrong_authority_refused() {
    let (oracle, id) = fixture(804, false);
    let bridge = install_bridge(&oracle);
    let req = bridge.prepare_timer_tick(id).unwrap();

    timer_ticks::trigger_wrong_resource(true);
    let res = bridge.execute_timer_tick(req.clone());
    timer_ticks::trigger_wrong_resource(false);

    assert!(res.is_err());

    // State remains uncorrupted and request can still be executed successfully with valid authority
    let reply = bridge.execute_timer_tick(req).unwrap();
    assert!(matches!(
        reply.outcome,
        timer_ticks::Outcome::Revived { .. }
    ));
}

#[test]
fn timer_tick_save_reload_continuation() {
    let (oracle, id) = fixture(804, false);
    let bridge = install_bridge(&oracle);
    let _ = bridge.tick_timer(id).unwrap();

    let saved = bridge.capture().unwrap();
    let restored = OwnerBridge::reconstruct(&saved, config(&oracle)).unwrap();

    let json = bridge.export_native().unwrap().to_save_json().unwrap();
    let reloaded = OwnerBridge::import_native(&json, json.len(), config(&oracle)).unwrap();

    assert_eq!(
        bridge.export_native().unwrap().to_save_json().unwrap(),
        restored.export_native().unwrap().to_save_json().unwrap()
    );
    assert_eq!(
        bridge.export_native().unwrap().to_save_json().unwrap(),
        reloaded.export_native().unwrap().to_save_json().unwrap()
    );

    let next = |mut w: SimulationWorld| (0..10).map(|_| w.rng.next_u64()).collect::<Vec<_>>();
    let continuation = next(bridge.export_native().unwrap());
    assert_eq!(next(restored.export_native().unwrap()), continuation);
    assert_eq!(next(reloaded.export_native().unwrap()), continuation);
}

#[test]
fn timer_tick_historical_profile_admission() {
    use crate::actions::equipment::Dressing;
    use nethacked_core::ac::ArmorSlot;

    for phase in 0..5 {
        let (mut w, _) = fixture(804, false);
        w.object_timers = Default::default();
        w.arena.items.clear();
        let suit = w
            .ruleset
            .create_item_record(
                "ring mail",
                ItemLocation::CarriedBy(w.player_id),
                nethacked_types::Buc::Uncursed,
            )
            .unwrap();
        let suit_id = w.arena.spawn_item(suit);
        w.equipment.armor = vec![(ArmorSlot::Suit, suit_id)];
        if phase == 4 {
            w.equipment.dressing = Some(Dressing {
                item: suit_id,
                putting_on: true,
                turns_left: 2,
            });
        } else {
            w.equipment.dressing = None;
        }

        let base = OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone()).unwrap();
        let old = match phase {
            0 => base.with_default_commands(),
            1 => base.with_passive_affliction_ticks(),
            2 => base.with_passive_metabolic_ticks(),
            3 => base.with_natural_hp_regeneration(),
            _ => base.with_dressing_ticks(),
        };

        let original = OwnerBridge::install(NativePartitions::from_native(w.clone()), old).unwrap();
        let c = OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
            .unwrap()
            .with_timer_ticks();
        let restored = OwnerBridge::reconstruct(&original.capture().unwrap(), c).unwrap();

        assert_eq!(snapshot(&restored).0, snapshot(&original).0);
        assert_eq!(persistent_history(&restored), persistent_history(&original));

        // Admitted historical session lacks unnegotiated timer authority
        assert!(restored.prepare_timer_tick(suit_id).is_err());

        // Historical continuation executes identically
        assert_eq!(
            historical_continuation(&restored, phase),
            historical_continuation(&original, phase)
        );

        let mut left = restored.export_native().unwrap();
        let mut right = original.export_native().unwrap();
        assert_eq!(left.rng.get_word_pos(), right.rng.get_word_pos());
        for _ in 0..8 {
            assert_eq!(left.rng.next_u64(), right.rng.next_u64());
        }
    }
}

#[test]
fn tick_timers_drains_due_queue() {
    let (mut oracle, id1) = fixture(804, false);
    // Add second troll corpse at adjacent tile
    let item2 = oracle
        .ruleset
        .create_corpse_record("troll", ItemLocation::Floor(Coord::new_unchecked(21, 10)))
        .unwrap();
    let id2 = oracle.arena.spawn_item(item2);
    oracle
        .start_object_timer(
            crate::ObjectTimerTarget::Active(id2),
            crate::ObjectTimerKind::ReviveMonster,
            0,
        )
        .unwrap();

    let bridge = install_bridge(&oracle);
    let events = bridge.tick_timers().unwrap();

    assert_eq!(events.len(), 2);
    assert!(events
        .iter()
        .all(|e| matches!(e, crate::GameEvent::ObjectTimerExpired { .. })));

    let final_world = bridge.export_native().unwrap();
    assert_eq!(final_world.arena.items.get(id1), None);
    assert_eq!(final_world.arena.items.get(id2), None);
    assert_eq!(final_world.arena.actors.len(), 3); // player + 2 revived trolls
    assert!(final_world.object_timers.entries().is_empty());
}
