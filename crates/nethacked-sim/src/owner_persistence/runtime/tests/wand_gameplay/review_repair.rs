use crate::{
    owner_persistence::{wand_ports::*, OwnerBridge, OwnerBridgeConfig},
    ObjectTimerKind, ObjectTimerTarget, SimulationWorld, StoredLevel,
};
use nethacked_arena::{ItemDefinition, ItemId, ItemLocation, ItemRecord};
use nethacked_core::ActionAst;
use nethacked_kernel::Scope;
use nethacked_types::{BranchId, Buc, Coord, Direction, DoorState, Tile};
use rand::{RngCore, SeedableRng};

fn fixture(name: &str) -> (SimulationWorld, ItemId) {
    let mut w = SimulationWorld::new_with_seed(53);
    w.hero.skills.skills.clear();
    w.arena.actors.retain(|id, _| id == w.player_id);
    w.arena.items.clear();
    w.unpaid_items.clear();
    w.billing = Default::default();
    w.equipment = Default::default();
    w.wielded_item = None;
    w.level = nethacked_dungeon::DungeonLevel::new_solid(Tile::Room);
    w.arena.actors[w.player_id].coord = Coord::new_unchecked(20, 10);
    w.arena.actors[w.player_id].hp = 1000;
    w.arena.actors[w.player_id].max_hp = 1000;
    w.arena.actors[w.player_id].level = 2;
    w.progress.level_max = 2;
    w.scheduler.hero_energy = 120;
    w.scheduler.hero_speed = 12;
    w.scheduler.turn = 0;
    let item = w
        .ruleset
        .create_item_record(name, ItemLocation::CarriedBy(w.player_id), Buc::Uncursed)
        .unwrap();
    let id = w.arena.spawn_item(ItemRecord {
        enchantment: 3,
        ..item
    });
    w.rng = rand_chacha::ChaCha8Rng::seed_from_u64(100);
    (w, id)
}
fn bridge(w: &SimulationWorld) -> OwnerBridge {
    let json = w.to_save_json().unwrap();
    OwnerBridge::import_native(
        &json,
        json.len(),
        OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
            .unwrap()
            .with_wand_gameplay(),
    )
    .unwrap()
}
fn zap(id: ItemId) -> ActionAst {
    ActionAst::ZapSelectedWand {
        item_index: 0,
        expected_item: Some(id),
        dir: Direction::East,
        energy: 6,
    }
}
fn assert_world(b: &OwnerBridge, w: &SimulationWorld) {
    let mut actual = b.export_native().unwrap();
    assert_json(
        "",
        &serde_json::from_str::<serde_json::Value>(&actual.to_save_json().unwrap()).unwrap(),
        &serde_json::from_str::<serde_json::Value>(&w.to_save_json().unwrap()).unwrap(),
    );
    assert_eq!(actual.rng.get_word_pos(), w.rng.get_word_pos());
    assert_eq!(actual.rng.next_u64(), w.rng.clone().next_u64());
}
fn differential(mut w: SimulationWorld, id: ItemId, created: bool) {
    let b = bridge(&w);
    let reply = b.dispatch_wand_action(zap(id)).unwrap();
    assert!(reply.error.is_none(), "{:?}", reply.error);
    let events = w.handle_zap_selected_wand(0, Some(id), Direction::East, 6);
    assert_eq!(reply.events, events);
    assert!(reply.dependency_calls > 0);
    assert_eq!(w.arena.items[id].enchantment, 2);
    assert_eq!(w.arena.actors.len() > 1, created);
    assert_world(&b, &w);
}

#[test]
fn public_identity_less_unknown_name_spends_charge_and_preserves_beam_events_and_rng() {
    let (mut w, id) = fixture("wand of fire");
    w.arena.items[id].definition = None;
    w.arena.items[id].name = "unregistered fire wand".into();
    differential(w, id, false);
}

#[test]
fn public_explicit_invalid_identity_and_direct_charge_still_refuse_without_state_or_rng() {
    let (mut w, id) = fixture("wand of fire");
    w.arena.items[id].definition = Some(ItemDefinition::Custom("unavailable fire wand".into()));
    let b = bridge(&w);
    let reply = b.dispatch_wand_action(zap(id)).unwrap();
    assert!(reply.error.is_none());
    assert_eq!(
        reply.events,
        w.handle_zap_selected_wand(0, Some(id), Direction::East, 6)
    );
    assert_eq!(w.arena.items[id].enchantment, 3);
    assert_world(&b, &w);
    let request = b
        .wand_operation(WandOwnerCommand::Objects(ObjectOperation::PositiveCharge {
            target: ObjectRef {
                id,
                residency: Residency::Active,
            },
            expected: 3,
        }))
        .unwrap();
    assert!(b.operate_wand(request).is_err());
    assert_world(&b, &w);
    w.arena.items[id].definition = Some(ItemDefinition::Vanilla(
        nethacked_data::ItemKindId::LONG_SWORD.c_name().into(),
    ));
    let json = w.to_save_json().unwrap();
    assert!(OwnerBridge::import_native(
        &json,
        json.len(),
        OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
            .unwrap()
            .with_wand_gameplay()
    )
    .is_err());
}

#[test]
fn public_creation_uses_species_terrain_contract_with_exact_draws_and_publication() {
    for (flag, tile) in [
        ("fly", Tile::Pool { frozen: false }),
        ("float", Tile::Moat),
        ("swim", Tile::Pool { frozen: false }),
        ("wallwalk", Tile::Stone),
        (
            "amorphous",
            Tile::Door {
                state: DoorState::Closed,
                trapped: false,
            },
        ),
        ("", Tile::Pit { filled: false }),
    ] {
        let (mut w, id) = fixture("wand of create monster");
        let original = w
            .ruleset
            .original_monster_for_role(w.role_id.unwrap())
            .unwrap()
            .name
            .clone();
        let mut rs = (*w.ruleset).clone();
        for def in &mut rs.monsters {
            def.frequency = 0;
            if (def.name == original || def.name == "lichen") && !flag.is_empty() {
                def.flags.push(flag.into());
            }
            if def.name == "lichen" {
                def.frequency = 1;
            }
        }
        rs.reindex();
        w.ruleset = std::sync::Arc::new(rs);
        w.level = nethacked_dungeon::DungeonLevel::new_solid(tile);
        w.level.tiles[20][10] = Tile::Room;
        differential(w, id, true);
    }
}

#[test]
fn publication_rejects_unsupported_terrain_occupied_and_invalid_initialized_record() {
    let (mut w, _) = fixture("wand of create monster");
    let coord = Coord::new_unchecked(21, 10);
    w.level.tiles[21][10] = Tile::Pool { frozen: false };
    let b = bridge(&w);
    let def = w.ruleset.monster("lichen").unwrap();
    let actor = def.create_record_with_hp(coord, def.base_hp, def.max_hp, def.level);
    for record in [
        actor.clone(),
        {
            let mut a = actor.clone();
            a.coord = w.arena.actors[w.player_id].coord;
            a
        },
        {
            let mut a = actor;
            a.coord = Coord::new_unchecked(22, 10);
            a.ac += 1;
            a
        },
    ] {
        let request = b
            .wand_operation(WandOwnerCommand::Actors(
                ActorOperation::PublishInitialized { actor: record },
            ))
            .unwrap();
        assert!(b.operate_wand(request).is_err());
        assert_world(&b, &w);
    }
}

#[test]
fn timer_admission_is_pure_and_publication_retains_multiple_tied_and_cached_registrations() {
    let (mut w, wand) = fixture("wand of death");
    let mut ids = Vec::new();
    for x in [25, 26, 27] {
        let a = w
            .ruleset
            .create_monster_record("lichen", Coord::new_unchecked(x, 10), &mut w.rng)
            .unwrap();
        let corpse = w
            .ruleset
            .create_corpse_record_for_actor(&a, ItemLocation::Floor(a.coord))
            .unwrap();
        ids.push(w.arena.spawn_item(corpse));
    }
    let branch = BranchId::DungeonsOfDoom;
    for depth in [8, 9] {
        w.stored_levels.push((
            (branch, depth),
            StoredLevel {
                level: w.level.clone(),
                monsters: vec![],
                items: vec![(ids[0], w.arena.items[ids[0]].clone())],
                unpaid_items: vec![],
            },
        ));
    }
    for target in [
        ObjectTimerTarget::Active(ids[0]),
        ObjectTimerTarget::Cached {
            branch,
            depth: 8,
            item: ids[0],
        },
        ObjectTimerTarget::Cached {
            branch,
            depth: 9,
            item: ids[0],
        },
    ] {
        w.start_object_timer(target, ObjectTimerKind::RotCorpse, 600)
            .unwrap();
    }
    w.start_object_timer(
        ObjectTimerTarget::Active(ids[0]),
        ObjectTimerKind::ReviveMonster,
        500,
    )
    .unwrap();
    let b = bridge(&w);
    // Native import retains C-style reinsertion: restored equal deadlines
    // reverse order. Compare command execution from that exact imported state.
    w = b.export_native().unwrap();
    assert_eq!(
        w.object_timers
            .entries()
            .iter()
            .map(|t| t.serial)
            .collect::<Vec<_>>(),
        vec![3, 0, 1, 2]
    );
    for (target, legacy) in [
        (
            ObjectRef {
                id: ids[1],
                residency: Residency::Active,
            },
            ObjectTimerTarget::Active(ids[1]),
        ),
        (
            ObjectRef {
                id: ids[0],
                residency: Residency::Cached { branch, depth: 8 },
            },
            ObjectTimerTarget::Cached {
                branch,
                depth: 8,
                item: ids[0],
            },
        ),
    ] {
        let kind = if matches!(target.residency, Residency::Active) {
            ObjectTimerKind::RotCorpse
        } else {
            ObjectTimerKind::ReviveMonster
        };
        let observation = b
            .observe_wand(WandRead {
                query: WandQuery::Command(CommandQuery::TimerAdmission {
                    target,
                    kind,
                    due: Some(600),
                }),
                expected_owner: None,
            })
            .unwrap();
        assert!(matches!(
            observation.facts,
            WandFacts::Command(CommandFacts::TimerAdmission { result: Ok(_) })
        ));
        assert_world(&b, &w);
        let request = b
            .wand_operation(WandOwnerCommand::Clock(ClockOperation::StartCorpseTimer {
                target,
                kind,
                due: Some(600),
            }))
            .unwrap();
        let serial = w.start_object_timer(legacy, kind, 600).unwrap();
        assert!(
            matches!(b.operate_wand(request.clone()).unwrap(), OperationResult::Timer { serial: s } if s == serial)
        );
        assert_world(&b, &w);
        assert!(b.operate_wand(request).is_err());
        let duplicate = b
            .wand_operation(WandOwnerCommand::Clock(ClockOperation::StartCorpseTimer {
                target,
                kind,
                due: None,
            }))
            .unwrap();
        assert!(matches!(
            b.operate_wand(duplicate).unwrap(),
            OperationResult::TimerRefused {
                reason: TimerFailure::Duplicate
            }
        ));
        assert_world(&b, &w);
    }
    assert_eq!(w.object_timers.entries().len(), 6);
    let command = WandOwnerCommand::Clock(ClockOperation::StartCorpseTimer {
        target: ObjectRef {
            id: ids[2],
            residency: Residency::Active,
        },
        kind: ObjectTimerKind::RotCorpse,
        due: Some(600),
    });
    let stale = b.wand_operation(command.clone()).unwrap();
    assert!(b
        .operate_wand_scoped(stale.clone(), Scope::Keys(Default::default()))
        .is_err());
    assert_world(&b, &w);
    let charge = b
        .wand_operation(WandOwnerCommand::Objects(ObjectOperation::PositiveCharge {
            target: ObjectRef {
                id: wand,
                residency: Residency::Active,
            },
            expected: 3,
        }))
        .unwrap();
    b.operate_wand(charge).unwrap();
    w.arena.items[wand].enchantment = 2;
    assert!(b.operate_wand(stale).is_err());
    assert_world(&b, &w);
    let missing = b.wand_operation(WandOwnerCommand::Clock(ClockOperation::StartCorpseTimer {
        target: ObjectRef {
            id: ids[2],
            residency: Residency::Cached { branch, depth: 8 },
        },
        kind: ObjectTimerKind::RotCorpse,
        due: Some(600),
    }));
    assert!(missing.is_err() || b.operate_wand(missing.unwrap()).is_err());
    assert_world(&b, &w);
}

fn assert_json(path: &str, actual: &serde_json::Value, expected: &serde_json::Value) {
    match (actual, expected) {
        (serde_json::Value::Object(a), serde_json::Value::Object(e)) => {
            assert_eq!(
                a.keys().collect::<Vec<_>>(),
                e.keys().collect::<Vec<_>>(),
                "{path}"
            );
            for (key, value) in a {
                assert_json(&format!("{path}/{key}"), value, &e[key]);
            }
        }
        (serde_json::Value::Array(a), serde_json::Value::Array(e)) => {
            assert_eq!(a.len(), e.len(), "{path}");
            for (i, (a, e)) in a.iter().zip(e).enumerate() {
                assert_json(&format!("{path}/{i}"), a, e);
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}

#[test]
fn bounded_timer_insertion_preserves_unobserved_stale_entries_and_legacy_reconciliation() {
    use crate::{ObjectTimerError, ObjectTimerQueue};
    let (mut w, id) = fixture("wand of death");
    let stale = w.arena.items.remove(id).unwrap();
    let live = w.arena.spawn_item(stale);
    let stale_target = ObjectTimerTarget::Active(id);
    let live_target = ObjectTimerTarget::Active(live);
    let mut queue: ObjectTimerQueue = serde_json::from_value(serde_json::json!({
        "entries": [{"target": stale_target, "kind": "RotCorpse", "due": 600, "serial": 0}],
        "next_serial": 1
    }))
    .unwrap();
    let mut legacy = queue.clone();
    let exists = |target| match target {
        ObjectTimerTarget::Active(id) => w.arena.items.contains_key(id),
        _ => false,
    };
    assert_eq!(
        queue.start_at_preserving(
            live_target,
            ObjectTimerKind::RotCorpse,
            Some(600),
            exists(live_target)
        ),
        Ok(1)
    );
    assert_eq!(queue.entries().len(), 2);
    assert_eq!(queue.entries()[1].target, stale_target);
    let before = queue.clone();
    assert_eq!(
        queue.start_at_preserving(
            stale_target,
            ObjectTimerKind::ReviveMonster,
            None,
            exists(stale_target)
        ),
        Err(ObjectTimerError::MissingObject)
    );
    assert_eq!(queue, before);
    assert_eq!(
        queue.start_at_preserving(
            live_target,
            ObjectTimerKind::RotCorpse,
            None,
            exists(live_target)
        ),
        Err(ObjectTimerError::Duplicate)
    );
    assert_eq!(queue, before);
    assert_eq!(
        legacy.start_at(live_target, ObjectTimerKind::RotCorpse, Some(600), exists),
        Ok(1)
    );
    assert_eq!(legacy.entries().len(), 1);
    assert_eq!(legacy.entries()[0], queue.entries()[0]);
}

#[test]
fn public_kill_timer_preserves_unrelated_corpse_and_exact_event_and_rng_prefix() {
    let (mut w, id) = fixture("wand of death");
    let mut victim = w
        .ruleset
        .create_monster_record("lichen", Coord::new_unchecked(21, 10), &mut w.rng)
        .unwrap();
    victim.hp = 1;
    victim.max_hp = 1;
    victim.speed = 0;
    victim.movement = 0;
    w.arena.spawn_actor(victim.clone());
    let corpse = w
        .ruleset
        .create_corpse_record_for_actor(&victim, ItemLocation::Floor(Coord::new_unchecked(30, 10)))
        .unwrap();
    let corpse = w.arena.spawn_item(corpse);
    w.start_object_timer(
        ObjectTimerTarget::Active(corpse),
        ObjectTimerKind::RotCorpse,
        600,
    )
    .unwrap();
    let registration = w.object_timers.entries()[0];
    w.rng = rand_chacha::ChaCha8Rng::seed_from_u64(100);
    let b = bridge(&w);
    let reply = b.dispatch_wand_action(zap(id)).unwrap();
    assert!(reply.error.is_none(), "{:?}", reply.error);
    assert_eq!(
        reply.events,
        w.handle_zap_selected_wand(0, Some(id), Direction::East, 6)
    );
    // Lichen birth starts and then cancels its own timer. The consumed serial
    // proves that scheduling ran, while the unrelated registration survives.
    assert_eq!(w.object_timers.entries().len(), 1);
    assert_eq!(
        serde_json::to_value(&w.object_timers).unwrap()["next_serial"],
        2
    );
    assert_eq!(w.arena.items.len(), 3);
    assert!(w.object_timers.entries().contains(&registration));
    assert_world(&b, &w);
}

#[test]
fn cached_identity_less_unknown_name_charge_preserves_active_alias() {
    let (mut w, id) = fixture("wand of fire");
    let mut item = w.arena.items[id].clone();
    item.definition = None;
    item.name = "unregistered fire wand".into();
    let branch = BranchId::DungeonsOfDoom;
    w.stored_levels.push((
        (branch, 8),
        StoredLevel {
            level: w.level.clone(),
            monsters: vec![],
            items: vec![(id, item)],
            unpaid_items: vec![],
        },
    ));
    let b = bridge(&w);
    let request = b
        .wand_operation(WandOwnerCommand::Objects(ObjectOperation::PositiveCharge {
            target: ObjectRef {
                id,
                residency: Residency::Cached { branch, depth: 8 },
            },
            expected: 3,
        }))
        .unwrap();
    assert!(matches!(
        b.operate_wand(request).unwrap(),
        OperationResult::Charge { remaining: 2 }
    ));
    w.stored_levels[0].1.items[0].1.enchantment = 2;
    assert_eq!(w.arena.items[id].enchantment, 3);
    assert_world(&b, &w);
}
