//! Public world-clock, persistence and actual rot checks; revival stays neutral.
//! Literal queue oracle: pinned C timeout.c:2222-2291,2467-2482.
use nethacked_arena::{CorpseSpecies, ItemId, ItemLocation};
use nethacked_data::{ItemKindId as K, MonsterSpeciesId as M};
use nethacked_sim::{
    ActionAst, GameEvent, ObjectTimer, ObjectTimerError, ObjectTimerKind as Kind,
    ObjectTimerTarget as Target, SimulationWorld, StoredLevel,
};
use nethacked_types::{BranchId, Buc, Tile};
use rand::RngCore;

fn world() -> SimulationWorld {
    let mut w = SimulationWorld::new_with_seed(804);
    w.arena.actors.retain(|id, _| id == w.player_id);
    w.arena.items.clear();
    w.equipment = Default::default();
    w.wielded_item = None;
    w
}
fn corpse(w: &mut SimulationWorld, location: ItemLocation) -> ItemId {
    let name = &w.ruleset.monster_by_id(M::HUMAN).unwrap().name;
    let mut item = w.ruleset.create_corpse_record(name, location).unwrap();
    item.corpse_age = 7;
    w.arena.spawn_item(item)
}
fn expired(events: &[GameEvent]) -> Vec<ObjectTimer> {
    events
        .iter()
        .filter_map(|event| match event {
            GameEvent::ObjectTimerExpired { timer } => Some(*timer),
            _ => None,
        })
        .collect()
}
fn load(w: &SimulationWorld) -> SimulationWorld {
    SimulationWorld::from_save_json(
        &w.to_save_json().unwrap(),
        w.ruleset.clone(),
        &w.ruleset_ref,
    )
    .unwrap()
}

#[test]
fn literal_due_and_newest_first_ties_dispatch_through_wait() {
    let mut w = world();
    let a = corpse(&mut w, ItemLocation::Limbo);
    let b = corpse(&mut w, ItemLocation::Limbo);
    let c = corpse(&mut w, ItemLocation::Limbo);
    assert_eq!(w.scheduler.turn, 1);
    assert_eq!(
        w.start_object_timer(Target::Active(a), Kind::RotCorpse, 2),
        Ok(0)
    );
    assert_eq!(
        w.start_object_timer(Target::Active(b), Kind::RotCorpse, 1),
        Ok(1)
    );
    assert_eq!(
        w.start_object_timer(Target::Active(c), Kind::ReviveMonster, 2),
        Ok(2)
    );
    assert_eq!(
        w.start_object_timer(Target::Active(a), Kind::ReviveMonster, 2),
        Ok(3)
    );
    let first = expired(&w.step_player_action(ActionAst::Wait));
    assert_eq!(
        first,
        vec![ObjectTimer {
            target: Target::Active(b),
            kind: Kind::RotCorpse,
            due: 2,
            serial: 1
        }]
    );
    assert!(!w.arena.items.contains_key(b));
    assert_eq!(w.arena.items[a].corpse_age, 8);
    assert_eq!(
        w.arena.items[a].corpse_species,
        Some(CorpseSpecies::Vanilla("HUMAN".into()))
    );
    let second = expired(&w.step_player_action(ActionAst::Wait));
    assert_eq!(
        second,
        vec![
            ObjectTimer {
                target: Target::Active(a),
                kind: Kind::ReviveMonster,
                due: 3,
                serial: 3
            },
            ObjectTimer {
                target: Target::Active(c),
                kind: Kind::ReviveMonster,
                due: 3,
                serial: 2
            },
            ObjectTimer {
                target: Target::Active(a),
                kind: Kind::RotCorpse,
                due: 3,
                serial: 0
            },
        ]
    );
    assert!(w.object_timers.entries().is_empty());
    assert!(!w.arena.items.contains_key(a));
    // Disposal invalidates the old generation; a fresh body retains serial4.
    assert_eq!(
        w.start_object_timer(Target::Active(a), Kind::RotCorpse, 1),
        Err(ObjectTimerError::MissingObject)
    );
    let fresh = corpse(&mut w, ItemLocation::Limbo);
    assert_eq!(
        w.start_object_timer(Target::Active(fresh), Kind::RotCorpse, 1),
        Ok(4)
    );
    assert_eq!(w.arena.items[c].corpse_age, 9);
    assert_eq!(
        w.arena.items[c].corpse_species,
        Some(CorpseSpecies::Vanilla("HUMAN".into()))
    );
}

#[test]
fn local_save_restore_reverses_ties_in_literal_c_record_order() {
    let mut w = world();
    let floor = ItemLocation::Floor(w.level.stairs_up);
    let a = corpse(&mut w, floor.clone());
    let b = corpse(&mut w, floor);
    w.start_object_timer(Target::Active(a), Kind::RotCorpse, 2)
        .unwrap();
    w.start_object_timer(Target::Active(b), Kind::RotCorpse, 2)
        .unwrap();
    // C saved records are B,A; sequential insert-before-equals restores A,B.
    assert_eq!(
        w.object_timers
            .entries()
            .iter()
            .map(|t| t.serial)
            .collect::<Vec<_>>(),
        vec![1, 0]
    );
    let mut restored = load(&w);
    assert_eq!(
        restored.object_timers.entries(),
        &[
            ObjectTimer {
                target: Target::Active(a),
                kind: Kind::RotCorpse,
                due: 3,
                serial: 0
            },
            ObjectTimer {
                target: Target::Active(b),
                kind: Kind::RotCorpse,
                due: 3,
                serial: 1
            },
        ]
    );
    // A second saved restoration reverses that valid serial-ascending queue.
    assert_eq!(
        load(&restored)
            .object_timers
            .entries()
            .iter()
            .map(|t| t.serial)
            .collect::<Vec<_>>(),
        vec![1, 0]
    );
    assert_eq!(
        restored.start_object_timer(Target::Active(a), Kind::ReviveMonster, 2),
        Ok(2)
    );
    assert!(expired(&restored.step_player_action(ActionAst::Wait)).is_empty());
    let restored_due = expired(&restored.step_player_action(ActionAst::Wait));
    assert_eq!(
        restored_due.iter().map(|t| t.serial).collect::<Vec<_>>(),
        vec![2, 0, 1]
    );
    w.step_player_action(ActionAst::Wait);
    assert_eq!(
        expired(&w.step_player_action(ActionAst::Wait))
            .iter()
            .map(|t| t.serial)
            .collect::<Vec<_>>(),
        vec![1, 0]
    );
    assert_eq!(w.rng.next_u64(), restored.rng.next_u64());
}

#[test]
fn full_load_restores_global_then_local_batches_using_outer_ancestry() {
    let mut w = world();
    let player = w.player_id;
    let global_a = corpse(&mut w, ItemLocation::CarriedBy(player));
    let floor = ItemLocation::Floor(w.level.stairs_up);
    let local_a = corpse(&mut w, floor.clone());
    let sack = w.arena.spawn_item(
        w.ruleset
            .create_item_record_by_id(K::SACK, ItemLocation::CarriedBy(player), Buc::Uncursed)
            .unwrap(),
    );
    let global_b = corpse(&mut w, ItemLocation::InContainer(sack));
    let local_b = corpse(&mut w, floor);
    for item in [global_a, local_a, global_b, local_b] {
        w.start_object_timer(Target::Active(item), Kind::RotCorpse, 2)
            .unwrap();
    }
    assert_eq!(
        w.object_timers
            .entries()
            .iter()
            .map(|t| t.serial)
            .collect::<Vec<_>>(),
        vec![3, 2, 1, 0]
    );
    let mut restored = load(&w);
    // Globals saved B,A -> A,B; locals saved B,A -> A,B, ahead of globals.
    assert_eq!(
        restored
            .object_timers
            .entries()
            .iter()
            .map(|t| t.serial)
            .collect::<Vec<_>>(),
        vec![1, 3, 0, 2]
    );
    assert_eq!(
        restored.start_object_timer(Target::Active(global_a), Kind::ReviveMonster, 2),
        Ok(4)
    );
    assert!(expired(&restored.step_player_action(ActionAst::Wait)).is_empty());
    let due = expired(&restored.step_player_action(ActionAst::Wait));
    assert_eq!(
        due.iter().map(|t| (t.serial, t.due)).collect::<Vec<_>>(),
        vec![(4, 3), (1, 3), (3, 3), (0, 3), (2, 3)]
    );
    assert!(restored.object_timers.entries().is_empty());
    let mut control = world();
    control.step_player_action(ActionAst::Wait);
    control.step_player_action(ActionAst::Wait);
    assert_eq!(restored.rng.next_u64(), control.rng.next_u64());
}

#[test]
fn actual_cache_return_reinserts_local_ties_before_live_global_and_new_timers() {
    let mut w = world();
    let floor = ItemLocation::Floor(w.level.stairs_down);
    let a = corpse(&mut w, floor.clone());
    let b = corpse(&mut w, floor);
    w.arena.items[a].name = "local A".into();
    w.arena.items[b].name = "local B".into();
    let player = w.player_id;
    let global = corpse(&mut w, ItemLocation::CarriedBy(player));
    w.arena.items[global].weight = 0; // Isolate timer ordering from burden.
    w.start_object_timer(Target::Active(a), Kind::RotCorpse, 4)
        .unwrap();
    w.start_object_timer(Target::Active(b), Kind::RotCorpse, 4)
        .unwrap();
    w.stored_levels.push((
        (BranchId::DungeonsOfDoom, 2),
        StoredLevel {
            level: w.level.clone(),
            monsters: vec![],
            items: vec![],
            unpaid_items: vec![],
        },
    ));
    w.level
        .set_tile(w.level.stairs_down, Tile::Stairs { up: false });
    w.arena.actors[player].coord = w.level.stairs_down;
    assert!(expired(&w.step_player_action(ActionAst::Descend)).is_empty());
    assert_eq!(w.scheduler.turn, 2);
    assert_eq!(
        w.start_object_timer(Target::Active(global), Kind::RotCorpse, 3),
        Ok(2)
    );
    // Full recovery restores/writes off-level B,A as A,B; returning then
    // reinserts those records as B,A. Live return has only the latter step.
    let mut recovered = load(&w);
    assert_eq!(
        recovered
            .object_timers
            .entries()
            .iter()
            .filter(|t| matches!(t.target, Target::Cached { .. }))
            .map(|t| t.serial)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    for (sim, expected) in [(&mut w, vec![0, 1, 2]), (&mut recovered, vec![1, 0, 2])] {
        sim.level
            .set_tile(sim.level.stairs_up, Tile::Stairs { up: true });
        sim.arena.actors[player].coord = sim.level.stairs_up;
        assert!(expired(&sim.step_player_action(ActionAst::Ascend)).is_empty());
        assert_eq!(sim.scheduler.turn, 3);
        assert_eq!(
            sim.object_timers
                .entries()
                .iter()
                .map(|t| t.serial)
                .collect::<Vec<_>>(),
            expected
        );
        let new_a = sim
            .arena
            .items
            .iter()
            .find(|(_, record)| record.name == "local A")
            .unwrap()
            .0;
        let new_b = sim
            .arena
            .items
            .iter()
            .find(|(_, record)| record.name == "local B")
            .unwrap()
            .0;
        assert_ne!(new_a, a);
        assert_ne!(new_b, b);
        assert_eq!(
            sim.object_timers
                .entries()
                .iter()
                .find(|t| t.serial == 0)
                .unwrap()
                .target,
            Target::Active(new_a)
        );
        assert_eq!(
            sim.object_timers
                .entries()
                .iter()
                .find(|t| t.serial == 1)
                .unwrap()
                .target,
            Target::Active(new_b)
        );
        assert_eq!(
            sim.start_object_timer(Target::Active(new_a), Kind::ReviveMonster, 2),
            Ok(3)
        );
        assert!(expired(&sim.step_player_action(ActionAst::Wait)).is_empty());
        let due = expired(&sim.step_player_action(ActionAst::Wait));
        let mut expected_due = vec![3];
        expected_due.extend(expected);
        assert_eq!(
            due.iter().map(|t| t.serial).collect::<Vec<_>>(),
            expected_due
        );
        assert!(due.iter().all(|t| t.due == 5));
        assert!(sim.object_timers.entries().is_empty());
    }
    // The source-defined ordering differs; both orders delete the same bodies,
    // while non-event world state and native RNG continue identically.
    let mut live_value = serde_json::to_value(&w).unwrap();
    let mut loaded_value = serde_json::to_value(&recovered).unwrap();
    live_value.as_object_mut().unwrap().remove("event_log");
    loaded_value.as_object_mut().unwrap().remove("event_log");
    assert_eq!(live_value, loaded_value);
    assert_eq!(w.rng.next_u64(), recovered.rng.next_u64());
}

#[test]
fn fast_and_slow_hero_actions_use_one_world_clock() {
    for (speed, energy, actions, turns) in [(24, 24, 4, vec![1, 2, 2, 3]), (6, 12, 1, vec![3])] {
        let mut w = world();
        w.scheduler.hero_speed = speed;
        w.scheduler.hero_energy = energy;
        let item = corpse(&mut w, ItemLocation::Limbo);
        w.start_object_timer(Target::Active(item), Kind::RotCorpse, 2)
            .unwrap();
        let mut deliveries = Vec::new();
        for &turn in turns.iter().take(actions) {
            deliveries.extend(expired(&w.step_player_action(ActionAst::Wait)));
            assert_eq!(w.scheduler.turn, turn);
            if turn < 3 {
                assert_eq!(w.arena.items[item].corpse_age, 7 + (turn - 1) as u32);
            } else {
                assert!(!w.arena.items.contains_key(item));
            }
        }
        assert_eq!(deliveries.len(), 1);
        assert_eq!(deliveries[0].due, 3);
    }
}

#[test]
fn duplicates_cancel_and_deleted_generations_cannot_deliver() {
    let mut w = world();
    let a = corpse(&mut w, ItemLocation::Limbo);
    let b = corpse(&mut w, ItemLocation::Limbo);
    w.start_object_timer(Target::Active(a), Kind::RotCorpse, 3)
        .unwrap();
    let before = w.object_timers.clone();
    assert_eq!(
        w.start_object_timer(Target::Active(a), Kind::RotCorpse, 1),
        Err(ObjectTimerError::Duplicate)
    );
    assert_eq!(w.object_timers, before);
    assert_eq!(
        w.stop_object_timer(Target::Active(a), Kind::RotCorpse),
        Some(3)
    );
    assert_eq!(
        w.stop_object_timer(Target::Active(a), Kind::RotCorpse),
        None
    );
    w.start_object_timer(Target::Active(a), Kind::RotCorpse, 1)
        .unwrap();
    w.start_object_timer(Target::Active(a), Kind::ReviveMonster, 1)
        .unwrap();
    assert_eq!(w.cancel_object_timers(Target::Active(a)), 2);
    w.start_object_timer(Target::Active(b), Kind::RotCorpse, 1)
        .unwrap();
    w.arena.destroy_item(b);
    let replacement = corpse(&mut w, ItemLocation::Limbo);
    assert_ne!(replacement, b);
    assert_eq!(
        w.start_object_timer(Target::Active(b), Kind::RotCorpse, 1),
        Err(ObjectTimerError::MissingObject)
    );
    assert!(expired(&w.step_player_action(ActionAst::Wait)).is_empty());
    assert!(w.object_timers.entries().is_empty());
    assert!(w.arena.items.contains_key(a));
    assert!(w.arena.items.contains_key(replacement));
}

#[test]
fn midsave_preserves_queue_counter_canonical_state_events_and_rng() {
    let mut original = world();
    let a = corpse(&mut original, ItemLocation::Limbo);
    let player = original.player_id;
    let b = corpse(&mut original, ItemLocation::CarriedBy(player));
    original
        .start_object_timer(Target::Active(a), Kind::RotCorpse, 3)
        .unwrap();
    original
        .start_object_timer(Target::Active(b), Kind::ReviveMonster, 4)
        .unwrap();
    original.step_player_action(ActionAst::Wait);
    let mut restored = load(&original);
    assert_eq!(
        serde_json::to_value(&original).unwrap(),
        serde_json::to_value(&restored).unwrap()
    );
    for _ in 0..3 {
        assert_eq!(
            original.step_player_action(ActionAst::Wait),
            restored.step_player_action(ActionAst::Wait)
        );
    }
    assert_eq!(
        original.start_object_timer(Target::Active(a), Kind::RotCorpse, 1),
        Err(ObjectTimerError::MissingObject)
    );
    assert_eq!(
        restored.start_object_timer(Target::Active(a), Kind::RotCorpse, 1),
        Err(ObjectTimerError::MissingObject)
    );
    assert!(!original.arena.items.contains_key(a));
    assert!(!restored.arena.items.contains_key(a));
    let fresh_original = corpse(&mut original, ItemLocation::Limbo);
    let fresh_restored = corpse(&mut restored, ItemLocation::Limbo);
    assert_eq!(fresh_original, fresh_restored);
    assert_eq!(
        original.start_object_timer(Target::Active(fresh_original), Kind::RotCorpse, 1),
        Ok(2)
    );
    assert_eq!(
        restored.start_object_timer(Target::Active(fresh_restored), Kind::RotCorpse, 1),
        Ok(2)
    );
    assert_eq!(
        serde_json::to_value(&original).unwrap(),
        serde_json::to_value(&restored).unwrap()
    );
    assert_eq!(original.rng.next_u64(), restored.rng.next_u64());
}

#[test]
fn absent_and_explicit_empty_legacy_owner_never_schedule_or_draw() {
    let mut w = world();
    let item = corpse(&mut w, ItemLocation::Limbo);
    let ice = w.arena.spawn_item(
        w.ruleset
            .create_item_record_by_id(K::ICE_BOX, ItemLocation::Limbo, Buc::Uncursed)
            .unwrap(),
    );
    w.arena.items[item].location = ItemLocation::InContainer(ice);
    let mut value = serde_json::to_value(&w).unwrap();
    value.as_object_mut().unwrap().remove("object_timers");
    assert!(value.get("object_timers").is_none());
    for explicit in [false, true] {
        if explicit {
            value["object_timers"] = serde_json::json!({"entries": [], "next_serial": 0});
        }
        let mut restored =
            SimulationWorld::from_save_json(&value.to_string(), w.ruleset.clone(), &w.ruleset_ref)
                .unwrap();
        assert!(restored.object_timers.entries().is_empty());
        assert_eq!(restored.arena.items[item].corpse_age, 7);
        assert_eq!(restored.scheduler.turn, 1);
        assert_eq!(restored.rng.next_u64(), w.rng.clone().next_u64());
        assert!(expired(&restored.step_player_action(ActionAst::Wait)).is_empty());
    }
}

#[test]
fn load_catches_up_active_overdue_after_references_without_advancing_time() {
    let mut w = world();
    let item = corpse(&mut w, ItemLocation::Limbo);
    w.start_object_timer(Target::Active(item), Kind::RotCorpse, 0)
        .unwrap();
    let mut restored = load(&w);
    assert_eq!(restored.scheduler.turn, 1);
    assert!(!restored.arena.items.contains_key(item));
    assert_eq!(
        expired(&restored.event_log),
        vec![ObjectTimer {
            target: Target::Active(item),
            kind: Kind::RotCorpse,
            due: 1,
            serial: 0,
        }]
    );
    assert!(expired(&restored.step_player_action(ActionAst::Wait)).is_empty());
    assert!(restored.object_timers.entries().is_empty());
    let mut without_timer = world();
    without_timer.step_player_action(ActionAst::Wait);
    assert_eq!(restored.rng.next_u64(), without_timer.rng.next_u64());
}

#[test]
fn offlevel_same_handle_isolation_and_return_remap_catchup() {
    let mut w = world();
    let item = corpse(&mut w, ItemLocation::Limbo);
    let mut stored_item = w.arena.items[item].clone();
    stored_item.name = "cached human corpse".into();
    let branch = BranchId::DungeonsOfDoom;
    w.stored_levels.push((
        (branch, 9),
        StoredLevel {
            level: w.level.clone(),
            monsters: vec![],
            items: vec![(item, stored_item)],
            unpaid_items: vec![],
        },
    ));
    let cached = Target::Cached {
        branch,
        depth: 9,
        item,
    };
    w.start_object_timer(Target::Active(item), Kind::RotCorpse, 1)
        .unwrap();
    w.start_object_timer(cached, Kind::RotCorpse, 1).unwrap();
    assert_eq!(
        expired(&w.step_player_action(ActionAst::Wait))[0].target,
        Target::Active(item)
    );
    w.step_player_action(ActionAst::Wait);
    assert_eq!(w.object_timers.entries().len(), 1);
    let mut restored = load(&w);
    assert!(expired(&restored.event_log)
        .iter()
        .all(|timer| timer.target != cached));
    assert_eq!(restored.object_timers.entries()[0].due, 2);
    let before_turn = restored.scheduler.turn;
    assert_eq!(restored.stored_levels[0].1.items[0].1.corpse_age, 9);
    let returned = expired(&restored.unpack_or_generate_level(branch, 9));
    let Target::Active(new) = returned[0].target else {
        panic!("not remapped")
    };
    assert_ne!(new, item);
    assert_eq!(
        returned,
        vec![ObjectTimer {
            target: Target::Active(new),
            kind: Kind::RotCorpse,
            due: 2,
            serial: 1
        }]
    );
    assert_eq!(restored.scheduler.turn, before_turn);
    assert!(!restored.arena.items.contains_key(new));
    assert!(restored.object_timers.entries().is_empty());
}

#[test]
fn actual_stairs_pack_nested_records_and_remap_timer_targets_once() {
    let mut w = world();
    let ice = w.arena.spawn_item(
        w.ruleset
            .create_item_record_by_id(
                K::ICE_BOX,
                ItemLocation::Floor(w.level.stairs_down),
                Buc::Uncursed,
            )
            .unwrap(),
    );
    let sack = w.arena.spawn_item(
        w.ruleset
            .create_item_record_by_id(K::ICE_BOX, ItemLocation::InContainer(ice), Buc::Uncursed)
            .unwrap(),
    );
    let item = corpse(&mut w, ItemLocation::InContainer(sack));
    let player = w.player_id;
    let carried = corpse(&mut w, ItemLocation::CarriedBy(player));
    w.start_object_timer(Target::Active(item), Kind::RotCorpse, 1)
        .unwrap();
    w.start_object_timer(Target::Active(carried), Kind::ReviveMonster, 1)
        .unwrap();
    // Preload an empty destination, avoiding generation RNG and monsters.
    w.stored_levels.push((
        (BranchId::DungeonsOfDoom, 2),
        StoredLevel {
            level: w.level.clone(),
            monsters: vec![],
            items: vec![],
            unpaid_items: vec![],
        },
    ));
    w.level
        .set_tile(w.level.stairs_down, Tile::Stairs { up: false });
    w.arena.actors[w.player_id].coord = w.level.stairs_down;
    let departing = expired(&w.step_player_action(ActionAst::Descend));
    assert_eq!(w.depth, 2);
    assert_eq!(departing.len(), 1);
    assert_eq!(departing[0].target, Target::Active(carried));
    let cached = Target::Cached {
        branch: BranchId::DungeonsOfDoom,
        depth: 1,
        item,
    };
    assert_eq!(w.object_timers.entries()[0].target, cached);
    assert_eq!(w.object_timers.entries()[0].due, 2);
    w.step_player_action(ActionAst::Wait);
    let mut restored = load(&w);
    for sim in [&mut w, &mut restored] {
        sim.level
            .set_tile(sim.level.stairs_up, Tile::Stairs { up: true });
        sim.arena.actors[sim.player_id].coord = sim.level.stairs_up;
        assert_eq!(
            sim.stored_levels
                .iter()
                .find(|(key, _)| *key == (BranchId::DungeonsOfDoom, 1))
                .unwrap()
                .1
                .items
                .iter()
                .find(|(id, _)| *id == item)
                .unwrap()
                .1
                .corpse_age,
            7
        );
        let returned = expired(&sim.step_player_action(ActionAst::Ascend));
        let Target::Active(new) = returned[0].target else {
            panic!("not remapped")
        };
        assert_ne!(new, item);
        assert_eq!(returned.len(), 1);
        assert_eq!(returned[0].target, Target::Active(new));
        assert_eq!((returned[0].due, returned[0].serial), (2, 0));
        assert!(!sim.arena.items.contains_key(new));
        let (new_sack, inner) = sim
            .arena
            .items
            .iter()
            .find(|(_, record)| {
                record.is_ice_box && matches!(record.location, ItemLocation::InContainer(_))
            })
            .unwrap();
        let ItemLocation::InContainer(new_ice) = inner.location else {
            panic!("lost ice box");
        };
        assert!(sim.arena.items[new_sack].is_ice_box);
        assert!(sim.arena.items_in_container(new_sack).is_empty());
        assert!(sim.arena.items[new_ice].is_ice_box);
        assert!(sim.object_timers.entries().is_empty());
        assert!(expired(&sim.step_player_action(ActionAst::Wait)).is_empty());
    }
    assert_eq!(
        serde_json::to_value(&w).unwrap(),
        serde_json::to_value(&restored).unwrap()
    );
    assert_eq!(w.rng.next_u64(), restored.rng.next_u64());
}

#[test]
fn cached_remint_preserves_live_keys_and_matches_saved_allocator_with_actor_holes() {
    let mut w = world();
    let carried = corpse(&mut w, ItemLocation::Limbo);
    let mut live_actor = w.arena.actors[w.player_id].clone();
    live_actor.is_player = false;
    live_actor.name = "live actor".into();
    let hole_a = w.arena.spawn_actor(live_actor.clone());
    let live = w.arena.spawn_actor(live_actor.clone());
    let hole_b = w.arena.spawn_actor(live_actor.clone());
    // Removal history deliberately differs from serde's highest-slot-first.
    w.arena.destroy_actor(hole_b);
    w.arena.destroy_actor(hole_a);
    let old_actor = hole_b;
    live_actor.name = "cached actor".into();
    let mut stored_item = w.arena.items[carried].clone();
    stored_item.location = ItemLocation::CarriedBy(old_actor);
    stored_item.name = "cached actor corpse".into();
    w.stored_levels.push((
        (BranchId::DungeonsOfDoom, 8),
        StoredLevel {
            level: w.level.clone(),
            monsters: vec![(old_actor, live_actor)],
            items: vec![(carried, stored_item)],
            unpaid_items: vec![],
        },
    ));
    w.start_object_timer(
        Target::Cached {
            branch: BranchId::DungeonsOfDoom,
            depth: 8,
            item: carried,
        },
        Kind::RotCorpse,
        0,
    )
    .unwrap();
    let live_before = w.arena.actors[live].clone();
    let hero_before = w.arena.actors[w.player_id].clone();
    let item_before = w.arena.items[carried].clone();
    let mut restored = load(&w);
    let events = w.unpack_or_generate_level(BranchId::DungeonsOfDoom, 8);
    assert_eq!(
        events,
        restored.unpack_or_generate_level(BranchId::DungeonsOfDoom, 8)
    );
    for sim in [&w, &restored] {
        assert_eq!(sim.arena.actors[live], live_before);
        assert_eq!(sim.arena.actors[sim.player_id], hero_before);
        assert_eq!(sim.arena.items[carried], item_before);
        let new_actor = sim
            .arena
            .actors
            .iter()
            .find(|(_, a)| a.name == "cached actor")
            .unwrap()
            .0;
        assert_ne!(new_actor, old_actor);
        let Target::Active(new_item) = expired(&events)[0].target else {
            panic!("not remapped")
        };
        assert_ne!(new_item, carried);
        assert!(!sim.arena.items.contains_key(new_item));
        assert!(sim.arena.items_carried_by(new_actor).is_empty());
        assert_eq!(expired(&events)[0].target, Target::Active(new_item));
    }
    assert_eq!(
        serde_json::to_value(&w).unwrap(),
        serde_json::to_value(&restored).unwrap()
    );
    assert_eq!(w.rng.next_u64(), restored.rng.next_u64());
}

#[test]
fn explicit_cancel_restart_freeze_prerequisite_preserves_existing_age_owner() {
    let mut w = world();
    let ice = w.arena.spawn_item(
        w.ruleset
            .create_item_record_by_id(K::ICE_BOX, ItemLocation::Limbo, Buc::Uncursed)
            .unwrap(),
    );
    let sack = w.arena.spawn_item(
        w.ruleset
            .create_item_record_by_id(K::SACK, ItemLocation::Limbo, Buc::Uncursed)
            .unwrap(),
    );
    let item = corpse(&mut w, ItemLocation::InContainer(sack));
    w.start_object_timer(Target::Active(item), Kind::RotCorpse, 2)
        .unwrap();
    w.arena.items[sack].location = ItemLocation::InContainer(ice);
    w.arena.items[item].location = ItemLocation::InContainer(ice);
    // Isolate timer/age prerequisites with explicit primitive operations;
    // separate public transfer tests exercise the connected source sampling.
    assert_eq!(
        w.stop_object_timer(Target::Active(item), Kind::RotCorpse),
        Some(2)
    );
    for _ in 0..3 {
        assert!(expired(&w.step_player_action(ActionAst::Wait)).is_empty());
    }
    assert_eq!(w.arena.items[item].corpse_age, 7);
    let mut restored = load(&w);
    for sim in [&mut w, &mut restored] {
        sim.arena.items[sack].location = ItemLocation::Limbo;
        sim.arena.items[item].location = ItemLocation::InContainer(sack);
        assert_eq!(
            sim.start_object_timer(Target::Active(item), Kind::RotCorpse, 2),
            Ok(1)
        );
        assert!(expired(&sim.step_player_action(ActionAst::Wait)).is_empty());
        assert_eq!(sim.arena.items[item].corpse_age, 8);
        let delivery = expired(&sim.step_player_action(ActionAst::Wait));
        assert_eq!(delivery.len(), 1);
        assert_eq!(delivery[0].due, 6);
        assert!(!sim.arena.items.contains_key(item));
    }
    assert_eq!(
        serde_json::to_value(&w).unwrap(),
        serde_json::to_value(&restored).unwrap()
    );
}

#[test]
fn malformed_saved_duplicate_timer_is_rejected_and_deleted_targets_are_pruned() {
    let mut w = world();
    let item = corpse(&mut w, ItemLocation::Limbo);
    w.start_object_timer(Target::Active(item), Kind::RotCorpse, 5)
        .unwrap();
    let mut value = serde_json::to_value(&w).unwrap();
    let duplicate = value["object_timers"]["entries"][0].clone();
    value["object_timers"]["entries"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    assert!(
        SimulationWorld::from_save_json(&value.to_string(), w.ruleset.clone(), &w.ruleset_ref)
            .is_err()
    );
    w.arena.destroy_item(item);
    assert!(load(&w).object_timers.entries().is_empty());
}
