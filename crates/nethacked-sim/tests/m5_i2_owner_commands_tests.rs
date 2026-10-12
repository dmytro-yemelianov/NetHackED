use nethacked_arena::{ItemLocation, ItemRecord};
use nethacked_kernel::Scope;
use nethacked_sim::{
    owner_persistence::{wand_ports::*, OwnerBridge, OwnerBridgeConfig},
    ObjectTimerKind, SimulationWorld,
};
use nethacked_types::{Buc, Coord, Tile};
use rand::{Rng, RngCore};

fn fixture() -> (OwnerBridge, SimulationWorld, nethacked_arena::ItemId) {
    let mut world = SimulationWorld::new_with_seed(724);
    world.hero.skills.skills.clear();
    let item = world
        .ruleset
        .create_item_record(
            "wand of fire",
            ItemLocation::CarriedBy(world.player_id),
            Buc::Uncursed,
        )
        .unwrap();
    let id = world.arena.spawn_item(ItemRecord {
        enchantment: 3,
        ..item
    });
    let c = OwnerBridgeConfig::new(world.ruleset.clone(), world.ruleset_ref.clone())
        .unwrap()
        .with_wand_commands();
    let json = world.to_save_json().unwrap();
    (
        OwnerBridge::import_native(&json, json.len(), c).unwrap(),
        world,
        id,
    )
}
fn apply(b: &OwnerBridge, c: WandOwnerCommand) -> OperationResult {
    b.operate_wand(b.wand_operation(c).unwrap()).unwrap()
}
fn read(b: &OwnerBridge, q: WandQuery) -> WandObservation {
    b.observe_wand(WandRead {
        query: q,
        expected_owner: None,
    })
    .unwrap()
}
fn target(id: nethacked_arena::ItemId) -> ObjectRef {
    ObjectRef {
        id,
        residency: Residency::Active,
    }
}
fn same_rng(b: &OwnerBridge, w: &SimulationWorld) {
    assert_eq!(
        b.export_native().unwrap().rng.get_word_pos(),
        w.rng.get_word_pos()
    );
}

#[test]
fn positive_charge_and_authenticated_refusals_preserve_unrelated_state() {
    let (b, w, id) = fixture();
    let command = WandOwnerCommand::Objects(ObjectOperation::PositiveCharge {
        target: target(id),
        expected: 3,
    });
    let request = b.wand_operation(command.clone()).unwrap();
    let before = b.export_native().unwrap().to_save_json().unwrap();
    let mut bad = request.clone();
    bad.expected_owner.generation += 1;
    assert!(b.operate_wand(bad).is_err());
    let mut bad = request.clone();
    bad.expected_owner.schema = [0; 32];
    assert!(b.operate_wand(bad).is_err());
    let mut bad = request.clone();
    bad.expected_owner.artifact = [0; 32];
    assert!(b.operate_wand(bad).is_err());
    let mut bad = request.clone();
    bad.dependencies[0].revision += 1;
    assert!(b.operate_wand(bad).is_err());
    let (foreign, _, _) = fixture();
    assert!(foreign.operate_wand(request.clone()).is_err());
    assert!(b
        .operate_wand_scoped(request.clone(), Scope::Keys(Default::default()))
        .is_err());
    assert_eq!(b.export_native().unwrap().to_save_json().unwrap(), before);
    assert!(matches!(
        b.operate_wand(request.clone()).unwrap(),
        OperationResult::Charge { remaining: 2 }
    ));
    assert!(b.operate_wand(request).is_err());
    let after = b.export_native().unwrap();
    assert_eq!(after.arena.items[id].enchantment, 2);
    assert_eq!(
        after
            .arena
            .items
            .iter()
            .filter(|(key, _)| *key != id)
            .collect::<Vec<_>>(),
        w.arena
            .items
            .iter()
            .filter(|(key, _)| *key != id)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        serde_json::to_value(&after.arena.actors).unwrap(),
        serde_json::to_value(&w.arena.actors).unwrap()
    );
    assert_eq!(after.object_timers, w.object_timers);
    same_rng(&b, &w);
}

#[test]
fn legal_actor_operations_publish_and_subsequent_facts_match() {
    let (b, w, _) = fixture();
    let p = w.player_id;
    let hp = w.arena.actors[p].hp;
    assert!(matches!(
        apply(
            &b,
            WandOwnerCommand::Actors(ActorOperation::Damage {
                target: p,
                amount: 1
            })
        ),
        OperationResult::Damage { dead: false }
    ));
    let WandFacts::Command(CommandFacts::Actor { actor, .. }) =
        read(&b, WandQuery::Command(CommandQuery::Actor { target: p })).facts
    else {
        panic!()
    };
    assert_eq!(actor.hp, hp - 1);
    apply(
        &b,
        WandOwnerCommand::Actors(ActorOperation::Polymorph {
            target: p,
            definition: "lichen".into(),
        }),
    );
    apply(
        &b,
        WandOwnerCommand::Actors(ActorOperation::ForceHostileMalign {
            target: p,
            malign: 7,
        }),
    );
    apply(
        &b,
        WandOwnerCommand::Actors(ActorOperation::MakeHostile { target: p }),
    );
    let current = b.export_native().unwrap().arena.actors[p].clone();
    apply(
        &b,
        WandOwnerCommand::Actors(ActorOperation::GainLevel {
            target: p,
            hp: 2,
            level: current.level + 1,
        }),
    );
    let coord = (1..nethacked_types::COLNO - 1)
        .flat_map(|x| (1..nethacked_types::ROWNO - 1).map(move |y| Coord::new(x, y).unwrap()))
        .find(|c| {
            w.level.get_tile(*c).is_passable()
                && !w
                    .arena
                    .actors
                    .iter()
                    .any(|(_, a)| a.coord == *c && !a.is_dead)
        })
        .unwrap();
    let def = w.ruleset.monster("lichen").unwrap();
    let mut spawned = def.create_record_with_hp(coord, 4, 4, def.level);
    spawned.is_peaceful = true;
    let OperationResult::Actor { id } = apply(
        &b,
        WandOwnerCommand::Actors(ActorOperation::PublishInitialized {
            actor: spawned.clone(),
        }),
    ) else {
        panic!()
    };
    let native = b.export_native().unwrap();
    assert_eq!(native.arena.actors[id], spawned);
    assert_eq!(
        serde_json::to_value(&native.arena.items).unwrap(),
        serde_json::to_value(&w.arena.items).unwrap()
    );
    assert_eq!(native.arena.actors[p].hp, current.hp + 2);
    same_rng(&b, &w);
}

#[test]
fn geography_and_hero_commands_are_bounded_and_observable() {
    let (_base, mut w, _) = fixture();
    let c = Coord::new(10, 10).unwrap();
    w.level.set_tile(c, Tile::SecretDoor { locked: false });
    let config = OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
        .unwrap()
        .with_wand_commands();
    let json = w.to_save_json().unwrap();
    let b = OwnerBridge::import_native(&json, json.len(), config).unwrap();
    assert!(matches!(
        apply(
            &b,
            WandOwnerCommand::Geography {
                coord: c,
                effect: GeographyOperation::RevealSecretDoor
            }
        ),
        OperationResult::Revealed { changed: true }
    ));
    apply(
        &b,
        WandOwnerCommand::Geography {
            coord: c,
            effect: GeographyOperation::BreakDoor,
        },
    );
    apply(
        &b,
        WandOwnerCommand::Geography {
            coord: c,
            effect: GeographyOperation::EraseEngraving,
        },
    );
    apply(&b, WandOwnerCommand::Hero(HeroOperation::RecordKill));
    apply(
        &b,
        WandOwnerCommand::Hero(HeroOperation::AdjustAlignment {
            delta: 2,
            limit: 20,
        }),
    );
    apply(
        &b,
        WandOwnerCommand::Hero(HeroOperation::AdjustLuck { delta: 1 }),
    );
    apply(
        &b,
        WandOwnerCommand::Hero(HeroOperation::AdjustFavor { delta: 1 }),
    );
    apply(
        &b,
        WandOwnerCommand::Hero(HeroOperation::ClearDivineProtection),
    );
    apply(
        &b,
        WandOwnerCommand::Hero(HeroOperation::MarkQuestLeaderKilled),
    );
    apply(
        &b,
        WandOwnerCommand::Hero(HeroOperation::DefeatQuestNemesis),
    );
    apply(
        &b,
        WandOwnerCommand::Hero(HeroOperation::CreditExperience { amount: 20 }),
    );
    apply(
        &b,
        WandOwnerCommand::Hero(HeroOperation::RecordLevelGain {
            from: 1,
            hp: 2,
            energy: 1,
            experience: 19,
        }),
    );
    let WandFacts::Command(CommandFacts::Hero {
        experience,
        pacifist,
        quest,
        ..
    }) = read(&b, WandQuery::Command(CommandQuery::Hero)).facts
    else {
        panic!()
    };
    assert_eq!(experience, 19);
    assert!(!pacifist);
    assert!(quest.killed_leader);
    let after = b.export_native().unwrap();
    assert_eq!(
        serde_json::to_value(&after.arena.items).unwrap(),
        serde_json::to_value(&w.arena.items).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&after.arena.actors).unwrap(),
        serde_json::to_value(&w.arena.actors).unwrap()
    );
    same_rng(&b, &w);
}

#[test]
fn generated_object_timers_and_energy_use_sealed_owner_publication() {
    let (b, w, _) = fixture();
    let c = w.arena.actors[w.player_id].coord;
    let object = w
        .ruleset
        .create_item_record("corpse", ItemLocation::Floor(c), Buc::Uncursed)
        .unwrap();
    let OperationResult::Object { id } = apply(
        &b,
        WandOwnerCommand::Objects(ObjectOperation::PublishGenerated {
            object: object.clone(),
        }),
    ) else {
        panic!()
    };
    assert_eq!(b.export_native().unwrap().arena.items[id], object);
    let request = b
        .wand_operation(WandOwnerCommand::Clock(ClockOperation::StartCorpseTimer {
            target: target(id),
            kind: ObjectTimerKind::RotCorpse,
            due: Some(999),
        }))
        .unwrap();
    assert!(matches!(
        b.operate_wand(request.clone()).unwrap(),
        OperationResult::Timer { .. }
    ));
    assert!(b.operate_wand(request).is_err());
    let WandFacts::Clock(f) = read(&b, WandQuery::Clock(target(id))).facts else {
        panic!()
    };
    assert_eq!(f.registrations.len(), 1);
    let duplicate = b
        .wand_operation(WandOwnerCommand::Clock(ClockOperation::StartCorpseTimer {
            target: target(id),
            kind: ObjectTimerKind::RotCorpse,
            due: Some(999),
        }))
        .unwrap();
    let before = b.export_native().unwrap().object_timers;
    assert!(matches!(
        b.operate_wand(duplicate).unwrap(),
        OperationResult::TimerRefused {
            reason: TimerFailure::Duplicate
        }
    ));
    assert_eq!(b.export_native().unwrap().object_timers, before);
    apply(
        &b,
        WandOwnerCommand::Clock(ClockOperation::CancelCorpseTimers { target: target(id) }),
    );
    apply(&b, WandOwnerCommand::Clock(ClockOperation::HeroAct));
    assert!(b
        .export_native()
        .unwrap()
        .object_timers
        .entries()
        .is_empty());
    same_rng(&b, &w);
}

#[test]
fn rng_raw_and_generic_sampling_continue_the_owned_legacy_stream() {
    let (b, mut w, _) = fixture();
    let read1 = read(&b, WandQuery::Random { draws: 1 });
    let witness = read1.witness();
    let WandFacts::Random(reservation) = read1.facts else {
        panic!()
    };
    same_rng(&b, &w);
    let request = WandOperation {
        expected_owner: witness,
        dependencies: vec![],
        command: WandOwnerCommand::Random {
            reservation,
            word: RawWord::U32,
        },
    };
    let mut bad = request.clone();
    if let WandOwnerCommand::Random { reservation, .. } = &mut bad.command {
        reservation.word_position += 1;
    }
    assert!(b.operate_wand(bad).is_err());
    same_rng(&b, &w);
    let OperationResult::Raw32(n) = b.operate_wand(request.clone()).unwrap() else {
        panic!()
    };
    assert_eq!(n, w.rng.next_u32());
    assert!(b.operate_wand(request).is_err());
    same_rng(&b, &w);
    {
        let mut rng = b.wand_rng();
        for _ in 0..12 {
            assert_eq!(rng.random_range(0..17u32), w.rng.random_range(0..17u32));
            assert_eq!(rng.random::<f64>(), w.rng.random::<f64>());
            assert_eq!(rng.next_u64(), w.rng.next_u64());
        }
        for len in [0, 1, 3, 4, 5, 7, 8, 9, 15, 16] {
            let mut actual = vec![0; len];
            let mut expected = vec![0; len];
            rng.fill_bytes(&mut actual);
            w.rng.fill_bytes(&mut expected);
            assert_eq!(actual, expected);
        }
    }
    same_rng(&b, &w);
}

#[test]
fn profiles_reconstruct_without_silently_upgrading_saved_bindings() {
    let (b, w, _) = fixture();
    let config = OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
        .unwrap()
        .with_wand_commands();
    let bytes = b.capture().unwrap();
    let restored = OwnerBridge::reconstruct(&bytes, config.clone()).unwrap();
    apply(&restored, WandOwnerCommand::Hero(HeroOperation::RecordKill));
    for config in [
        OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone()).unwrap(),
        OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
            .unwrap()
            .with_wand_ports(),
    ] {
        let json = w.to_save_json().unwrap();
        let old = OwnerBridge::import_native(&json, json.len(), config.clone()).unwrap();
        let saved = old.capture().unwrap();
        assert!(OwnerBridge::reconstruct(&bytes, config).is_err());
        b.restore(&saved).unwrap();
        assert_eq!(
            b.export_native().unwrap().to_save_json().unwrap(),
            old.export_native().unwrap().to_save_json().unwrap()
        );
        assert!(b
            .operate_wand(WandOperation {
                expected_owner: read(&restored, WandQuery::Hero).witness(),
                dependencies: vec![],
                command: WandOwnerCommand::Hero(HeroOperation::RecordKill)
            })
            .is_err());
    }
}

#[test]
fn pool_drawbridge_and_timer_overflow_refusals_preserve_the_published_prefix() {
    let (_, mut w, id) = fixture();
    let pool = Coord::new(9, 9).unwrap();
    let bridge = Coord::new(10, 9).unwrap();
    w.level.set_tile(pool, Tile::Pool { frozen: false });
    w.level.set_tile(bridge, Tile::Drawbridge { open: false });
    let config = OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
        .unwrap()
        .with_wand_commands();
    let json = w.to_save_json().unwrap();
    let b = OwnerBridge::import_native(&json, json.len(), config).unwrap();
    apply(
        &b,
        WandOwnerCommand::Geography {
            coord: pool,
            effect: GeographyOperation::FreezePool,
        },
    );
    apply(
        &b,
        WandOwnerCommand::Geography {
            coord: bridge,
            effect: GeographyOperation::CollapseDrawbridge,
        },
    );
    let bad = b
        .wand_operation(WandOwnerCommand::Clock(ClockOperation::StartCorpseTimer {
            target: target(id),
            kind: ObjectTimerKind::RotCorpse,
            due: None,
        }))
        .unwrap();
    let before = read(&b, WandQuery::Clock(target(id))).witness();
    assert!(matches!(
        b.operate_wand(bad).unwrap(),
        OperationResult::TimerRefused {
            reason: TimerFailure::ClockOverflow
        }
    ));
    assert_eq!(read(&b, WandQuery::Clock(target(id))).witness(), before);
    let actual = b.export_native().unwrap();
    assert_eq!(actual.level.get_tile(pool), &Tile::Pool { frozen: true });
    assert_eq!(actual.level.get_tile(bridge), &Tile::Moat);
    assert_eq!(actual.object_timers, w.object_timers);
    same_rng(&b, &w);
}

#[test]
fn unavailable_rng_authority_panics_without_fabricating_or_advancing_a_draw() {
    let (b, w, _) = fixture();
    let old = OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
        .unwrap()
        .with_wand_ports();
    let json = w.to_save_json().unwrap();
    let legacy = OwnerBridge::import_native(&json, json.len(), old).unwrap();
    b.restore(&legacy.capture().unwrap()).unwrap();
    let initial = b.export_native().unwrap().rng.get_word_pos();
    let mut rng = b.wand_rng();
    for _ in 0..2 {
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| rng.next_u32())).is_err());
    }
    assert_eq!(b.export_native().unwrap().rng.get_word_pos(), initial);
}

#[test]
fn cached_charge_publication_and_stale_item_generation_are_isolated() {
    let (_, mut w, id) = fixture();
    let item = w.arena.items.remove(id).unwrap();
    let replacement = w.arena.spawn_item(item.clone());
    assert_ne!(id, replacement);
    w.stored_levels.push((
        (nethacked_types::BranchId::DungeonsOfDoom, 9),
        nethacked_sim::StoredLevel {
            level: w.level.clone(),
            monsters: vec![],
            items: vec![(id, item.clone())],
            unpaid_items: vec![],
        },
    ));
    let config = OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
        .unwrap()
        .with_wand_commands();
    let json = w.to_save_json().unwrap();
    let b = OwnerBridge::import_native(&json, json.len(), config).unwrap();
    let stale = b
        .wand_operation(WandOwnerCommand::Objects(ObjectOperation::PositiveCharge {
            target: target(id),
            expected: 3,
        }))
        .unwrap();
    assert!(b.operate_wand(stale).is_err());
    let cached = ObjectRef {
        id,
        residency: Residency::Cached {
            branch: nethacked_types::BranchId::DungeonsOfDoom,
            depth: 9,
        },
    };
    assert!(matches!(
        apply(
            &b,
            WandOwnerCommand::Objects(ObjectOperation::PositiveCharge {
                target: cached,
                expected: 3
            })
        ),
        OperationResult::Charge { remaining: 2 }
    ));
    let WandFacts::Objects(f) = read(&b, WandQuery::Objects(ObjectRequest::Identity(cached))).facts
    else {
        panic!()
    };
    assert_eq!(f.charges, 2);
    let after = b.export_native().unwrap();
    assert_eq!(after.arena.items[replacement], item);
    assert_eq!(after.stored_levels[0].1.items[0].1.enchantment, 2);
    same_rng(&b, &w);
}

#[test]
fn generic_raw_client_rejects_wrong_reply_and_preserves_a_terminal_refusal() {
    let mut calls = 0;
    {
        let mut rng = AuthenticatedWandRng::new(|_| {
            calls += 1;
            Ok::<_, ()>(OperationResult::Applied)
        });
        assert_eq!(rng.try_next_u32(), Err(RawDrawError::WrongReply));
        assert_eq!(rng.try_next_u64(), Err(RawDrawError::Poisoned));
    }
    assert_eq!(calls, 1);
}
