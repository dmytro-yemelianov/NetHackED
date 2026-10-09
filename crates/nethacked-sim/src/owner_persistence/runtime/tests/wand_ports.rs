use super::*;
use crate::owner_persistence::wand_ports::*;
use crate::{ObjectTimerKind, ObjectTimerTarget, StoredLevel};
use nethacked_types::{ActorId, BranchId, Coord, Direction, Locale};
use rand::RngCore;

fn fixture() -> (OwnerBridge, ItemId, ItemId, ActorId) {
    let mut w = SimulationWorld::new_with_seed(804);
    w.hero.skills.skills.clear();
    w.locale = Locale::Uk;
    let mut object = w
        .ruleset
        .create_item_record(
            "wand of fire",
            ItemLocation::CarriedBy(w.player_id),
            Buc::Cursed,
        )
        .unwrap();
    object.quantity = 3;
    object.enchantment = -1;
    let id = w.arena.spawn_item(object.clone());
    w.wielded_item = Some(id);
    w.hero.quivered_item = Some(id);
    w.unpaid_items.push((id, 19));
    let unrelated = w.arena.spawn_item(object.clone());
    w.start_object_timer(
        ObjectTimerTarget::Active(id),
        ObjectTimerKind::RotCorpse,
        999,
    )
    .unwrap();
    w.start_object_timer(
        ObjectTimerTarget::Active(unrelated),
        ObjectTimerKind::RotCorpse,
        999,
    )
    .unwrap();
    let consumed = w.arena.spawn_item(object.clone());
    w.unpaid_items.push((consumed, 19));
    w.record_bill_terms(consumed, terms()).unwrap();
    w.dispose_whole_carried_item(consumed).unwrap();
    let cache_id = w.arena.spawn_item(object);
    let cache_item = w.arena.items.remove(cache_id).unwrap();
    w.stored_levels.push((
        (BranchId::DungeonsOfDoom, 9),
        StoredLevel {
            level: w.level.clone(),
            monsters: vec![],
            items: vec![(cache_id, cache_item)],
            unpaid_items: vec![(cache_id, 11)],
        },
    ));
    let player = w.player_id;
    let c = OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
        .unwrap()
        .with_wand_ports();
    let json = w.to_save_json().unwrap();
    (
        OwnerBridge::import_native(&json, json.len(), c).unwrap(),
        id,
        consumed,
        player,
    )
}
fn terms() -> BillTerms {
    BillTerms {
        billed_quantity: Some(3),
        unit_price: Some(7),
        creditor: None,
        legacy_base_cost: 19,
    }
}
fn read(bridge: &OwnerBridge, query: WandQuery) -> WandObservation {
    bridge
        .observe_wand(WandRead {
            query: query.clone(),
            expected_owner: None,
        })
        .unwrap_or_else(|e| panic!("{}: {e:?}", query.route().1))
}
fn target(id: ItemId) -> ObjectRef {
    ObjectRef {
        id,
        residency: Residency::Active,
    }
}
fn counters(b: &OwnerBridge) -> (usize, usize) {
    b.session
        .with_runtime(&b.credential.borrow(), |r| {
            (
                r.host().pending().unwrap(),
                r.host().history().unwrap().len(),
            )
        })
        .unwrap()
}

#[test]
fn i1_random_native_owner_requires_its_real_port_caller_scope_and_live_grant() {
    let (initial, _, _, _) = fixture();
    let foreign = read(&fixture().0, WandQuery::Random { draws: 1 }).witness();
    let restored =
        OwnerBridge::reconstruct(&initial.capture().unwrap(), initial.config.clone()).unwrap();
    for bridge in [initial, restored] {
        let before = snapshots(&bridge);
        let counts = counters(&bridge);
        let native = bridge.export_native().unwrap();
        let random = read(&bridge, WandQuery::Random { draws: MAX_DRAWS });
        let WandFacts::Random(facts) = &random.facts else {
            panic!("random");
        };
        assert_eq!(facts.stream, native.rng.get_stream());
        assert_eq!(facts.word_position, native.rng.get_word_pos());
        assert_eq!(facts.max_draws, MAX_DRAWS);
        assert_eq!(facts.algorithm_version, 1);
        assert!(random.dependencies.is_empty());
        let witness = random.witness();
        let query = WandQuery::Random { draws: 1 };
        let (caller, slot, resource) = bridge
            .session
            .with_runtime(&bridge.credential.borrow(), |b| {
                let caller = bridge_caller(b, &bridge.config).unwrap();
                let owner = &b.owner::<RandomState>("Random").unwrap().owner;
                let stamp = owner.stamp().unwrap();
                let k = b.kernel().unwrap();
                assert_ne!(caller, stamp.module.handle);
                assert_eq!(k.manifest(caller).unwrap(), &profile_caller(&bridge.config));
                assert_eq!(k.manifest(stamp.module.handle).unwrap(), &random_manifest());
                assert_eq!(
                    stamp.module.artifact,
                    k.artifact(stamp.module.handle).unwrap()
                );
                let request = encode(
                    &WandRead {
                        query: query.clone(),
                        expected_owner: None,
                    },
                    MAX_REQUEST_BYTES,
                )
                .unwrap();
                // Root invocation has no authenticated caller, even at the right provider.
                assert!(k
                    .invoke(
                        stamp.module.handle,
                        "wand_random",
                        &request,
                        Authority {
                            access: Access::Read,
                            scope: Scope::All
                        }
                    )
                    .is_err());
                // The same bytes cannot query Random through another owner's endpoint.
                let provider = b
                    .owner::<HeroState>("Hero")
                    .unwrap()
                    .owner
                    .stamp()
                    .unwrap()
                    .module
                    .handle;
                assert!(k
                    .invoke(
                        provider,
                        "wand_hero",
                        &request,
                        Authority {
                            access: Access::Read,
                            scope: Scope::All
                        }
                    )
                    .is_err());
                let slot = profile_caller(&bridge.config)
                    .dependencies
                    .iter()
                    .position(|d| d.capability == random_capability())
                    .unwrap();
                (caller, slot, owner.resource("wand_random").unwrap())
            })
            .unwrap();
        let scope = Scope::Keys([resource.clone()].into());
        assert!(bridge
            .observe_wand_scoped(
                WandRead {
                    query: query.clone(),
                    expected_owner: Some(witness.clone())
                },
                scope.clone()
            )
            .is_ok());
        assert!(bridge
            .observe_wand_scoped(
                WandRead {
                    query: query.clone(),
                    expected_owner: None
                },
                Scope::Keys(BTreeSet::new())
            )
            .is_err());
        let mut wrong = witness.clone();
        wrong.generation += 1;
        let mut wrong_schema = witness.clone();
        wrong_schema.schema = [0; 32];
        let mut wrong_revision = witness.clone();
        wrong_revision.revision += 1;
        let mut wrong_artifact = witness;
        wrong_artifact.artifact = [0; 32];
        for wrong in [
            foreign.clone(),
            wrong,
            wrong_schema,
            wrong_revision,
            wrong_artifact,
        ] {
            assert!(bridge
                .observe_wand(WandRead {
                    query: query.clone(),
                    expected_owner: Some(wrong)
                })
                .is_err());
        }
        bridge
            .session
            .with_runtime(&bridge.credential.borrow(), |b| {
                b.update_kernel(|k| k.revoke_grant(caller, slot))
            })
            .unwrap()
            .unwrap()
            .unwrap();
        bridge
            .credential
            .replace(bridge.session.issue_credential(&bridge.key).unwrap());
        assert!(matches!(
            bridge.observe_wand(WandRead {
                query: query.clone(),
                expected_owner: None
            }),
            Err(OwnerBridgeError::Kernel(kernel::Error::Revoked))
        ));
        bridge
            .session
            .with_runtime(&bridge.credential.borrow(), |b| {
                b.update_kernel(|k| {
                    k.issue_grant(
                        caller,
                        slot,
                        Authority {
                            access: Access::Read,
                            scope,
                        },
                    )
                })
            })
            .unwrap()
            .unwrap()
            .unwrap();
        bridge
            .credential
            .replace(bridge.session.issue_credential(&bridge.key).unwrap());
        assert!(bridge
            .observe_wand(WandRead {
                query,
                expected_owner: None
            })
            .is_ok());
        assert!(snapshots(&bridge) == before);
        assert_eq!(counters(&bridge), counts);
        let mut expected = native.rng;
        let mut actual = bridge.export_native().unwrap().rng;
        for _ in 0..32 {
            assert_eq!(actual.next_u64(), expected.next_u64());
        }
    }
}

#[test]
fn i1_missing_native_read_caller_refuses_before_destination_exchange() {
    let (destination, _, _, _) = fixture();
    let (source, _, _, _) = fixture();
    source
        .session
        .with_runtime(&source.credential.borrow(), |b| {
            let caller = bridge_caller(b, &source.config).unwrap();
            b.update_kernel(|k| k.managed_unload(caller))
                .unwrap()
                .unwrap();
        })
        .unwrap();
    source
        .credential
        .replace(source.session.issue_credential(&source.key).unwrap());
    let incomplete = source.capture().unwrap();
    let before = snapshots(&destination);
    let counts = counters(&destination);
    let witness = read(&destination, WandQuery::Random { draws: 1 }).witness();
    FACTORY_BUILDS.with(|count| count.set(0));
    assert!(destination.restore(&incomplete).is_err());
    FACTORY_BUILDS.with(|count| assert_eq!(count.get(), 0));
    assert!(snapshots(&destination) == before);
    assert_eq!(counters(&destination), counts);
    assert_eq!(
        read(&destination, WandQuery::Random { draws: 1 }).witness(),
        witness
    );
}

#[test]
fn i1_authenticated_reads_preserve_all_populated_owners_and_counters() {
    let (bridge, id, consumed, player) = fixture();
    let before = snapshots(&bridge);
    let effect_counts = counters(&bridge);
    let native = bridge.export_native().unwrap();
    let index = native
        .arena
        .items_carried_by(player)
        .iter()
        .position(|key| *key == id)
        .unwrap() as u32;
    let selected = read(
        &bridge,
        WandQuery::Objects(ObjectRequest::Selection(SelectionRequest {
            index,
            expected: Some(id),
        })),
    );
    let WandFacts::Objects(o) = selected.facts else {
        panic!("objects");
    };
    assert_eq!(o.target, target(id));
    assert_eq!(o.quantity, 3);
    assert_eq!(o.charges, -1);
    assert_eq!(o.buc, Buc::Cursed);
    assert_eq!(selected.stamp.schema, [102; 32]);
    assert_eq!(selected.dependencies.len(), 3);
    let WandFacts::Equipment(e) = read(&bridge, WandQuery::Equipment(target(id))).facts else {
        panic!("equipment");
    };
    assert!(e.wielded && e.quivered);
    assert!(e.armor.is_empty());
    assert!(e.dressing.is_none());
    let WandFacts::Clock(c) = read(&bridge, WandQuery::Clock(target(id))).facts else {
        panic!("clock");
    };
    assert_eq!(c.registrations.len(), 1);
    assert_eq!(
        c.registrations[0].timer.target,
        ObjectTimerTarget::Active(id)
    );
    assert_eq!(
        c.registrations[0].queue_order,
        native
            .object_timers
            .entries()
            .iter()
            .position(|timer| timer.target == ObjectTimerTarget::Active(id))
            .unwrap()
    );
    let WandFacts::Billing(b) = read(
        &bridge,
        WandQuery::Billing {
            target: target(id),
            receipt: Some(terms()),
        },
    )
    .facts
    else {
        panic!("billing");
    };
    assert!(matches!(
        b.liability,
        Liability::Unknown {
            legacy_base_cost: 19,
            quantity: 3
        }
    ));
    assert_eq!(b.receipt_admission, Some(Ok(())));
    let WandFacts::ConsumedBilling(b) =
        read(&bridge, WandQuery::ConsumedBilling { origin: consumed }).facts
    else {
        panic!("consumed");
    };
    assert_eq!(b.quantity, 3);
    assert_eq!(b.terms, terms());
    assert_eq!(b.receipt_order, 0);
    let origin = native.arena.actors[player].coord;
    let WandFacts::Geography(g) = read(
        &bridge,
        WandQuery::Geography(GeographyRequest {
            residency: Residency::Active,
            cells: vec![origin],
            trajectory: Some(TrajectoryRequest {
                version: 1,
                origin,
                direction: Direction::East,
                energy: 6,
            }),
        }),
    )
    .facts
    else {
        panic!("geography");
    };
    assert_eq!(g.cells[0].tile, native.level.get_tile(origin).clone());
    assert!(g.cells.len() <= 7);
    let actor_id = native
        .arena
        .actors
        .iter()
        .find(|(key, _)| *key != player)
        .unwrap()
        .0;
    let WandFacts::Actors(a) = read(
        &bridge,
        WandQuery::Actors(ActorsRequest {
            residency: Residency::Active,
            origin: player,
            targets: vec![actor_id],
        }),
    )
    .facts
    else {
        panic!("actors");
    };
    assert_eq!(a.origin.coord, origin);
    assert_eq!(a.targets[0].hp, native.arena.actors[actor_id].hp);
    let WandFacts::Hero(h) = read(&bridge, WandQuery::Hero).facts else {
        panic!("hero");
    };
    assert_eq!(h.player, player);
    let WandFacts::Session(s) = read(&bridge, WandQuery::Session).facts else {
        panic!("session");
    };
    assert_eq!(s.locale, Locale::Uk);
    let cache = &native.stored_levels[0];
    let cached = ObjectRef {
        id: cache.1.items[0].0,
        residency: Residency::Cached {
            branch: cache.0 .0,
            depth: cache.0 .1,
        },
    };
    let WandFacts::Objects(o) =
        read(&bridge, WandQuery::Objects(ObjectRequest::Identity(cached))).facts
    else {
        panic!("cached objects");
    };
    assert_eq!(o.target, cached);
    assert_eq!(o.quantity, 3);
    let WandFacts::Billing(b) = read(
        &bridge,
        WandQuery::Billing {
            target: cached,
            receipt: Some(terms()),
        },
    )
    .facts
    else {
        panic!("cached billing");
    };
    assert!(matches!(
        b.liability,
        Liability::Unknown {
            legacy_base_cost: 11,
            ..
        }
    ));
    assert_eq!(
        b.receipt_admission,
        Some(Err(BillError::UnsupportedBillingScope))
    );
    assert!(
        snapshots(&bridge) == before,
        "owner state or stamps changed"
    );
    assert_eq!(counters(&bridge), effect_counts);
    assert_eq!(
        bridge.export_native().unwrap().rng.next_u64(),
        native.rng.clone().next_u64()
    );
}

#[test]
fn i1_wrong_host_generation_schema_revision_scope_and_bounds_refuse_without_effects() {
    let (bridge, id, _, player) = fixture();
    let (other, _, _, _) = fixture();
    let observation = read(&bridge, WandQuery::Hero);
    let before = snapshots(&bridge);
    let counts = counters(&bridge);
    let mut witnesses = vec![read(&other, WandQuery::Hero).witness()];
    let witness = observation.witness();
    let mut wrong = witness.clone();
    wrong.generation += 1;
    witnesses.push(wrong);
    let mut wrong = witness.clone();
    wrong.schema = [0; 32];
    witnesses.push(wrong);
    let mut wrong = witness.clone();
    wrong.revision += 1;
    witnesses.push(wrong);
    let mut wrong = witness;
    wrong.artifact = [0; 32];
    witnesses.push(wrong);
    for expected_owner in witnesses {
        assert!(bridge
            .observe_wand(WandRead {
                query: WandQuery::Hero,
                expected_owner: Some(expected_owner)
            })
            .is_err());
    }
    assert!(bridge
        .observe_wand_scoped(
            WandRead {
                query: WandQuery::Objects(ObjectRequest::Identity(target(id))),
                expected_owner: None
            },
            Scope::Keys(BTreeSet::new())
        )
        .is_err());
    let origin = Coord::new_unchecked(1, 1);
    for query in [
        WandQuery::Geography(GeographyRequest {
            residency: Residency::Active,
            cells: vec![origin; MAX_CELLS + 1],
            trajectory: None,
        }),
        WandQuery::Geography(GeographyRequest {
            residency: Residency::Active,
            cells: vec![],
            trajectory: Some(TrajectoryRequest {
                version: 2,
                origin,
                direction: Direction::East,
                energy: 1,
            }),
        }),
        WandQuery::Geography(GeographyRequest {
            residency: Residency::Active,
            cells: vec![],
            trajectory: Some(TrajectoryRequest {
                version: 1,
                origin,
                direction: Direction::East,
                energy: MAX_CELLS as u32 + 1,
            }),
        }),
        WandQuery::Geography(GeographyRequest {
            residency: Residency::Active,
            cells: vec![Coord::new_unchecked(999, 999)],
            trajectory: None,
        }),
        WandQuery::Actors(ActorsRequest {
            residency: Residency::Active,
            origin: player,
            targets: vec![player; MAX_TARGETS + 1],
        }),
        WandQuery::Objects(ObjectRequest::Selection(SelectionRequest {
            index: u32::MAX,
            expected: Some(id),
        })),
        WandQuery::Objects(ObjectRequest::Identity(ObjectRef {
            id,
            residency: Residency::Cached {
                branch: BranchId::DungeonsOfDoom,
                depth: 999,
            },
        })),
        WandQuery::Random {
            draws: MAX_DRAWS + 1,
        },
    ] {
        assert!(bridge
            .observe_wand(WandRead {
                query,
                expected_owner: None
            })
            .is_err());
    }
    assert!(matches!(
        read(&bridge, WandQuery::Random { draws: 1 }).facts,
        WandFacts::Random(_)
    ));
    assert!(
        snapshots(&bridge) == before,
        "owner state or stamps changed"
    );
    assert_eq!(counters(&bridge), counts);
}

#[test]
fn i1_owner_byte_restore_retains_exact_profile_and_legacy_receipt_admission() {
    let (bridge, id, _, _) = fixture();
    let before = snapshots(&bridge);
    let bytes = bridge.capture().unwrap();
    let config = bridge.config.clone();
    drop(bridge);
    let restored = OwnerBridge::reconstruct(&bytes, config.clone()).unwrap();
    let after = snapshots(&restored);
    for ((old, bytes), (new, rebuilt)) in before.iter().zip(&after) {
        assert_ne!(old.owner, new.owner);
        assert_eq!(old.generation, new.generation);
        assert_eq!(old.revision, new.revision);
        assert_eq!(old.schema, new.schema);
        assert!(bytes == rebuilt, "rebuilt owner bytes changed");
    }
    assert!(matches!(
        read(
            &restored,
            WandQuery::Objects(ObjectRequest::Identity(target(id)))
        )
        .facts,
        WandFacts::Objects(_)
    ));
    restored.record_bill_terms(id, terms()).unwrap();
    let WandFacts::Billing(b) = read(
        &restored,
        WandQuery::Billing {
            target: target(id),
            receipt: Some(terms()),
        },
    )
    .facts
    else {
        panic!("billing");
    };
    assert!(matches!(b.liability, Liability::Known { quantity: 3, .. }));
    assert_eq!(
        b.receipt_admission,
        Some(Err(BillError::ReceiptAlreadyRecorded))
    );
    let accepted = snapshots(&restored);
    let counts = counters(&restored);
    assert_eq!(
        restored.record_bill_terms(id, terms()),
        Err(BillError::ReceiptAlreadyRecorded)
    );
    assert!(
        snapshots(&restored) == accepted,
        "receipt refusal changed owner state or stamps"
    );
    assert_eq!(counters(&restored), counts);
    let (old, old_id) = source();
    let old_before = snapshots(&old);
    let bytes = old.capture().unwrap();
    drop(old);
    let compatible = OwnerBridge::reconstruct(&bytes, config).unwrap();
    let loaded = snapshots(&compatible);
    for ((old, bytes), (new, rebuilt)) in old_before.iter().zip(&loaded) {
        assert_eq!(old.schema, new.schema);
        assert!(bytes == rebuilt, "rebuilt owner bytes changed");
    }
    compatible
        .record_bill_terms(
            old_id,
            BillTerms {
                billed_quantity: Some(1),
                unit_price: Some(19),
                creditor: None,
                legacy_base_cost: 19,
            },
        )
        .unwrap();
    assert!(compatible
        .observe_wand(WandRead {
            query: WandQuery::Hero,
            expected_owner: None
        })
        .is_err());
}

#[test]
fn i1_wire_schema_and_finite_byte_bounds_refuse_through_native_dispatch() {
    let (bridge, _, _, _) = fixture();
    let before = snapshots(&bridge);
    let counts = counters(&bridge);
    let requests = vec![
        br#"{"query":"Hero","expected_owner":null,"extra":0}"#.to_vec(),
        br#"{"query":"Hero","query":"Session","expected_owner":null}"#.to_vec(),
        vec![b' '; MAX_REQUEST_BYTES + 1],
    ];
    for bytes in requests {
        let result = bridge
            .session
            .with_runtime(&bridge.credential.borrow(), |b| {
                let caller = bridge_caller(b, &bridge.config).unwrap();
                b.kernel().unwrap().invoke(
                    caller,
                    "observe",
                    &bytes,
                    Authority {
                        access: Access::Read,
                        scope: Scope::All,
                    },
                )
            })
            .unwrap();
        assert!(result.is_err());
    }
    assert!(snapshots(&bridge) == before);
    assert_eq!(counters(&bridge), counts);
    // The endpoint's response ceiling also applies to genuinely populated
    // target facts, even when every per-field and per-target bound is legal.
    let mut w = bridge.export_native().unwrap();
    let mut record = w.arena.actors[w.player_id].clone();
    record.is_player = false;
    record.name = "a".repeat(MAX_TEXT_BYTES);
    let targets = (0..MAX_TARGETS)
        .map(|_| w.arena.spawn_actor(record.clone()))
        .collect();
    let json = w.to_save_json().unwrap();
    let large = OwnerBridge::import_native(&json, json.len(), bridge.config.clone()).unwrap();
    let before = snapshots(&large);
    let counts = counters(&large);
    assert!(large
        .observe_wand(WandRead {
            query: WandQuery::Actors(ActorsRequest {
                residency: Residency::Active,
                origin: w.player_id,
                targets,
            }),
            expected_owner: None
        })
        .is_err());
    assert!(snapshots(&large) == before);
    assert_eq!(counters(&large), counts);
}
