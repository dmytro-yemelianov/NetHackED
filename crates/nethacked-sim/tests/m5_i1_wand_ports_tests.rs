//! I1 checkpoint: exercise public owner/native ports, including process disposal.
use nethacked_arena::{ItemDefinition, ItemId, ItemLocation};
use nethacked_sim::owner_persistence::{wand_ports::*, OwnerBridge, OwnerBridgeConfig};
use nethacked_sim::SimulationWorld;
use nethacked_types::Buc;
use rand::RngCore;
use std::{fs, process::Command};

fn fixture() -> (SimulationWorld, ItemId, ItemId) {
    let mut world = SimulationWorld::new_with_seed(804);
    world.hero.skills.skills.clear();
    world.arena.items.clear();
    world.equipment = Default::default();
    world.wielded_item = None;
    let record = world
        .ruleset
        .create_item_record(
            "wand of fire",
            ItemLocation::CarriedBy(world.player_id),
            Buc::Blessed,
        )
        .unwrap();
    let stale = world.arena.spawn_item(record.clone());
    world.arena.items.remove(stale).unwrap();
    let id = world.arena.spawn_item(record);
    (world, id, stale)
}
fn config(w: &SimulationWorld) -> OwnerBridgeConfig {
    OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
        .unwrap()
        .with_wand_ports()
}
fn import(w: &SimulationWorld) -> OwnerBridge {
    let json = w.to_save_json().unwrap();
    OwnerBridge::import_native(&json, json.len(), config(w)).unwrap()
}
fn read(
    b: &OwnerBridge,
    q: WandQuery,
) -> Result<WandObservation, nethacked_sim::owner_persistence::OwnerBridgeError> {
    b.observe_wand(WandRead {
        query: q,
        expected_owner: None,
    })
}

#[test]
fn i1_random_reservation_is_bounded_and_does_not_draw() {
    let (mut w, _, _) = fixture();
    w.rng.set_stream(17);
    for _ in 0..73 {
        w.rng.next_u32();
    }
    let bridge = import(&w);
    let restored = OwnerBridge::reconstruct(&bridge.capture().unwrap(), config(&w)).unwrap();
    for bridge in [&bridge, &restored] {
        let before = bridge.export_native().unwrap().to_save_json().unwrap();
        for draws in [0, 1, MAX_DRAWS] {
            let observation = read(bridge, WandQuery::Random { draws }).unwrap();
            let WandFacts::Random(random) = observation.facts else {
                panic!("random");
            };
            assert_eq!(random.stream, w.rng.get_stream());
            assert_eq!(random.word_position, w.rng.get_word_pos());
            assert_eq!(random.max_draws, draws);
            assert_eq!(random.algorithm_version, 1);
            assert_eq!(observation.stamp.schema, [108; 32]);
            assert!(observation.dependencies.is_empty());
        }
        assert!(read(
            bridge,
            WandQuery::Random {
                draws: MAX_DRAWS + 1
            }
        )
        .is_err());
        assert_eq!(
            bridge.export_native().unwrap().to_save_json().unwrap(),
            before
        );
    }
}

fn assert_nine_owner_reads(bridge: &OwnerBridge, w: &SimulationWorld, id: ItemId) {
    let before = bridge.export_native().unwrap().to_save_json().unwrap();
    let target = ObjectRef {
        id,
        residency: Residency::Active,
    };
    let queries = [
        WandQuery::Geography(GeographyRequest {
            residency: Residency::Active,
            cells: vec![w.arena.actors[w.player_id].coord],
            trajectory: None,
        }),
        WandQuery::Objects(ObjectRequest::Identity(target)),
        WandQuery::Actors(ActorsRequest {
            residency: Residency::Active,
            origin: w.player_id,
            targets: vec![],
        }),
        WandQuery::Hero,
        WandQuery::Equipment(target),
        WandQuery::Billing {
            target,
            receipt: None,
        },
        WandQuery::Clock(target),
        WandQuery::Random { draws: MAX_DRAWS },
        WandQuery::Session,
    ];
    let mut owners = std::collections::BTreeSet::new();
    for (index, query) in queries.into_iter().enumerate() {
        let observation = read(bridge, query).unwrap();
        assert_eq!(observation.stamp.schema, [101 + index as u8; 32]);
        assert!(owners.insert(observation.stamp.owner));
        assert!(matches!(
            (index, observation.facts),
            (0, WandFacts::Geography(_))
                | (1, WandFacts::Objects(_))
                | (2, WandFacts::Actors(_))
                | (3, WandFacts::Hero(_))
                | (4, WandFacts::Equipment(_))
                | (5, WandFacts::Billing(_))
                | (6, WandFacts::Clock(_))
                | (7, WandFacts::Random(_))
                | (8, WandFacts::Session(_))
        ));
    }
    assert_eq!(owners.len(), 9);
    assert_eq!(
        bridge.export_native().unwrap().to_save_json().unwrap(),
        before
    );
    let mut actual = bridge.export_native().unwrap().rng;
    let mut expected = w.rng.clone();
    for _ in 0..32 {
        assert_eq!(actual.next_u64(), expected.next_u64());
    }
}

#[test]
fn i1_all_nine_independent_owner_ports_survive_reconstruction() {
    let (w, id, _) = fixture();
    let bridge = import(&w);
    assert_nine_owner_reads(&bridge, &w, id);
    let bytes = bridge.capture().unwrap();
    drop(bridge);
    let restored = OwnerBridge::reconstruct(&bytes, config(&w)).unwrap();
    assert_nine_owner_reads(&restored, &w, id);
}

#[test]
fn i1_public_selection_uses_generation_and_expected_identity_without_replacing_state() {
    let (w, id, stale) = fixture();
    let bridge = import(&w);
    let before = bridge.export_native().unwrap().to_save_json().unwrap();
    let observation = read(
        &bridge,
        WandQuery::Objects(ObjectRequest::Selection(SelectionRequest {
            index: 0,
            expected: Some(id),
        })),
    )
    .unwrap();
    let WandFacts::Objects(o) = observation.facts else {
        panic!("objects");
    };
    assert_eq!(o.target.id, id);
    assert_eq!(o.definition, w.arena.items[id].definition);
    assert!(read(
        &bridge,
        WandQuery::Objects(ObjectRequest::Selection(SelectionRequest {
            index: 0,
            expected: Some(stale)
        }))
    )
    .is_err());
    assert!(read(
        &bridge,
        WandQuery::Objects(ObjectRequest::Identity(ObjectRef {
            id: stale,
            residency: Residency::Active
        }))
    )
    .is_err());
    assert_eq!(
        bridge.export_native().unwrap().to_save_json().unwrap(),
        before
    );
    // Native-save reconstruction enrolls fresh owners and keeps arena identity.
    let native = bridge.export_native().unwrap().to_save_json().unwrap();
    drop(bridge);
    let rebuilt = OwnerBridge::import_native(&native, native.len(), config(&w)).unwrap();
    let observed = read(
        &rebuilt,
        WandQuery::Objects(ObjectRequest::Identity(ObjectRef {
            id,
            residency: Residency::Active,
        })),
    )
    .unwrap();
    assert_ne!(observation.stamp.owner, observed.stamp.owner);
    assert_eq!(
        rebuilt.export_native().unwrap().to_save_json().unwrap(),
        native
    );
}

#[test]
fn i1_unavailable_explicit_definition_never_falls_back_to_a_live_display_name() {
    let (mut w, id, _) = fixture();
    w.arena.items[id].definition = Some(ItemDefinition::Custom("unavailable-pack-key".into()));
    // Native admission already protects explicit identities. No query can
    // manufacture a fallback definition or exchange a populated destination.
    let bridge = import(&fixture().0);
    let before = bridge.export_native().unwrap().to_save_json().unwrap();
    let stamp = read(&bridge, WandQuery::Hero).unwrap().stamp;
    let json = w.to_save_json().unwrap();
    assert!(bridge.restore_native(&json, json.len()).is_err());
    assert!(bridge.export_native().unwrap().to_save_json().unwrap() == before);
    assert_eq!(read(&bridge, WandQuery::Hero).unwrap().stamp, stamp);
}

#[test]
fn i1_fresh_process_owner_bytes_and_native_bytes_keep_typed_ports() {
    if let Ok(role) = std::env::var("NETRUST_I1_ROLE") {
        let path = std::path::PathBuf::from(std::env::var_os("NETRUST_I1_DIR").unwrap());
        let (w, id, _) = fixture();
        if role == "produce" {
            let bridge = import(&w);
            fs::write(path.join("owners.bin"), bridge.capture().unwrap()).unwrap();
            fs::write(
                path.join("native.json"),
                bridge.export_native().unwrap().to_save_json().unwrap(),
            )
            .unwrap();
            fs::write(path.join("producer.pid"), std::process::id().to_string()).unwrap();
        } else {
            assert_ne!(
                fs::read_to_string(path.join("producer.pid")).unwrap(),
                std::process::id().to_string()
            );
            let bytes = fs::read(path.join("owners.bin")).unwrap();
            let bridge = OwnerBridge::reconstruct(&bytes, config(&w)).unwrap();
            assert_nine_owner_reads(&bridge, &w, id);
            let WandFacts::Objects(o) = read(
                &bridge,
                WandQuery::Objects(ObjectRequest::Selection(SelectionRequest {
                    index: 0,
                    expected: Some(id),
                })),
            )
            .unwrap()
            .facts
            else {
                panic!("objects");
            };
            assert_eq!(o.target.id, id);
            assert_eq!(o.buc, Buc::Blessed);
            let native = fs::read_to_string(path.join("native.json")).unwrap();
            assert_eq!(
                bridge.export_native().unwrap().to_save_json().unwrap(),
                native
            );
            let native_bridge =
                OwnerBridge::import_native(&native, native.len(), config(&w)).unwrap();
            assert_nine_owner_reads(&native_bridge, &w, id);
            assert!(matches!(
                read(&native_bridge, WandQuery::Session).unwrap().facts,
                WandFacts::Session(_)
            ));
        }
        return;
    }
    let path = std::env::temp_dir().join(format!("netrust-i1-process-{}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    for role in ["produce", "consume"] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "i1_fresh_process_owner_bytes_and_native_bytes_keep_typed_ports",
                "--nocapture",
            ])
            .env("NETRUST_I1_ROLE", role)
            .env("NETRUST_I1_DIR", &path)
            .output()
            .unwrap();
        fs::write(path.join(format!("{role}.stdout.log")), &output.stdout).unwrap();
        fs::write(path.join(format!("{role}.stderr.log")), &output.stderr).unwrap();
        assert!(
            output.status.success(),
            "I1 subprocess {role} failed; raw logs: {}",
            path.display()
        );
    }
}
