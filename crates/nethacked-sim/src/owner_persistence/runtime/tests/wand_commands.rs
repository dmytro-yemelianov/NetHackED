use super::super::wand_commands as commands;
use super::*;
use crate::owner_persistence::wand_ports::*;
use rand::RngCore;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

fn fixture() -> OwnerBridge {
    let mut w = SimulationWorld::new_with_seed(873);
    w.hero.skills.skills.clear();
    let config = OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
        .unwrap()
        .with_wand_commands();
    let json = w.to_save_json().unwrap();
    OwnerBridge::import_native(&json, json.len(), config).unwrap()
}
fn counts(b: &OwnerBridge) -> (usize, usize) {
    b.session
        .with_runtime(&b.credential.borrow(), |r| {
            (
                r.host().pending().unwrap(),
                r.host().history().unwrap().len(),
            )
        })
        .unwrap()
}
fn renew(b: &OwnerBridge) {
    b.session
        .replace_configuration(&b.key, restore_configuration(&b.config).unwrap())
        .unwrap();
    b.credential
        .replace(b.session.issue_credential(&b.key).unwrap());
}
fn random_request(b: &OwnerBridge) -> WandOperation {
    let o = b
        .observe_wand(WandRead {
            query: WandQuery::Random { draws: 1 },
            expected_owner: None,
        })
        .unwrap();
    let witness = o.witness();
    let WandFacts::Random(reservation) = o.facts else {
        panic!()
    };
    WandOperation {
        expected_owner: witness,
        dependencies: vec![],
        command: WandOwnerCommand::Random {
            reservation,
            word: RawWord::U32,
        },
    }
}
#[test]
fn command_native_refuses_missing_or_wrong_operation_and_revoked_grant() {
    let b = fixture();
    let request = random_request(&b);
    let encoded = encode(&request, MAX_REQUEST_BYTES).unwrap();
    let before = b.export_native().unwrap().to_save_json().unwrap();
    let initial_counts = counts(&b);
    b.session
        .with_runtime(&b.credential.borrow(), |r| {
            let caller = bridge_caller(r, &b.config).unwrap();
            assert!(r
                .kernel()
                .unwrap()
                .invoke(
                    caller,
                    "operate_v2",
                    &encoded,
                    Authority {
                        access: Access::Write,
                        scope: Scope::All
                    }
                )
                .is_err());
            let cleanup = r.host().cleanup_identity().unwrap();
            let (lease, operation) = r
                .update_kernel(|k| {
                    let lease = k
                        .capture_pipeline(
                            caller,
                            "operate_v2",
                            Authority {
                                access: Access::Write,
                                scope: Scope::All,
                            },
                        )
                        .unwrap();
                    let id = k.issue_operation().unwrap();
                    (
                        lease,
                        k.operation_binding_with_cleanup(
                            id,
                            lease,
                            1,
                            "wrong-operation",
                            &[cleanup],
                        )
                        .unwrap(),
                    )
                })
                .unwrap();
            assert!(r
                .kernel()
                .unwrap()
                .invoke_operation(
                    &operation,
                    &encoded,
                    Authority {
                        access: Access::Write,
                        scope: Scope::All
                    }
                )
                .is_err());
            r.update_kernel(|k| k.release_pipeline(lease))
                .unwrap()
                .unwrap();
            let slot = profile_caller(&b.config)
                .dependencies
                .iter()
                .position(|d| d.capability.endpoint == "wand_random_command_v2")
                .unwrap();
            r.update_kernel(|k| k.revoke_grant(caller, slot))
                .unwrap()
                .unwrap();
        })
        .unwrap();
    renew(&b);
    assert!(b.operate_wand(request).is_err());
    assert_eq!(counts(&b), initial_counts);
    assert!(b.export_native().unwrap().to_save_json().unwrap() == before);
}

fn rewrite_kernel(bytes: &[u8], mutate: impl FnOnce(&mut Value)) -> Vec<u8> {
    let mut outer: Value =
        serde_json::from_slice(&bytes[nethacked_persistence::HEADER_LEN..]).unwrap();
    let section = outer["sections"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| r["name"] == "kernel")
        .unwrap();
    let hex: nethacked_persistence::Hex =
        serde_json::from_value(section["local_payload"].clone()).unwrap();
    let mut kernel: Value =
        serde_json::from_slice(&hex.bytes(SaveLimits::default()).unwrap()).unwrap();
    mutate(&mut kernel);
    section["local_payload"] = json!(nethacked_persistence::Hex::from_bytes(
        &serde_json::to_vec(&kernel).unwrap()
    ));
    let body = serde_json::to_vec(&outer).unwrap();
    let mut framed = bytes[..12].to_vec();
    framed.extend_from_slice(&(body.len() as u64).to_le_bytes());
    framed.extend_from_slice(&Sha256::digest(&body));
    framed.extend_from_slice(&body);
    framed
}
#[test]
fn unavailable_command_code_and_malformed_profile_refuse_before_factories_or_exchange() {
    let b = fixture();
    let bytes = b.capture().unwrap();
    let before = b.export_native().unwrap().to_save_json().unwrap();
    let initial_counts = counts(&b);
    for malformed in [
        rewrite_kernel(&bytes, |k| {
            k["registry"]["modules"]
                .as_array_mut()
                .unwrap()
                .retain(|m| m["manifest"]["id"] != commands::CALLER);
        }),
        rewrite_kernel(&bytes, |k| {
            k["registry"]["modules"][0]["artifact"] = json!(vec![0; 32]);
        }),
        rewrite_kernel(&bytes, |k| {
            k["registry"]["modules"][0]["manifest"]["exports"]
                .as_object_mut()
                .unwrap()
                .remove("wand_objects_command_v2");
        }),
    ] {
        FACTORY_BUILDS.with(|c| c.set(0));
        assert!(b.restore(&malformed).is_err());
        FACTORY_BUILDS.with(|c| assert_eq!(c.get(), 0));
        assert!(b.export_native().unwrap().to_save_json().unwrap() == before);
        assert_eq!(counts(&b), initial_counts);
    }
}

#[test]
fn raw_candidate_publication_is_once_only_and_stale_tickets_do_not_draw() {
    // Independent actual native enrollment uses the production RandomCodec and
    // accepted sealed Batch API. No fabricated CallerView or authority payload.
    let config = fixture().config.clone();
    let host = Host::new(limits()).unwrap();
    host.configure_publication(publication_limits()).unwrap();
    let mut kernel = Kernel::new(kernel::Limits::default()).unwrap();
    let slot: Rc<RefCell<Option<Rc<owners::Owner<RandomState>>>>> = Rc::new(RefCell::new(None));
    let stage = slot.clone();
    let enrolled = host.clone();
    let cap = Capability {
        provider: "raw-ticket-fixture".into(),
        endpoint: "wand_random_command_v2".into(),
        major: 1,
    };
    let coordinator = cap.clone();
    let command: NativePort = Rc::new(move |request, view| {
        let owner = stage.borrow().as_ref().unwrap().clone();
        let q: u8 = decode(request, 8).map_err(native_refusal)?;
        let stamp = owner.stamp().map_err(native_refusal)?;
        let observation = owner
            .query(
                "wand_random",
                &encode(&WandQuery::Random { draws: 1 }, MAX_REQUEST_BYTES)
                    .map_err(native_refusal)?,
                &[],
                view,
            )
            .map_err(native_refusal)?;
        let WandFacts::Random(reservation) =
            decode(observation.bytes(), MAX_RESPONSE_BYTES).map_err(native_refusal)?
        else {
            return Err(kernel::Error::NativeRefusal);
        };
        let req = WandOperation {
            expected_owner: OwnerWitness::from(&stamp),
            dependencies: vec![],
            command: WandOwnerCommand::Random {
                reservation,
                word: RawWord::U32,
            },
        };
        let candidate = owner
            .prepare(
                "wand_random_command_v2",
                &encode(&req, MAX_REQUEST_BYTES).map_err(native_refusal)?,
                vec![],
                view,
            )
            .map_err(native_refusal)?;
        struct One;
        impl Accounting for One {
            fn validate(&self, p: &[ProposalView], _: &[Observation]) -> owners::Result<()> {
                if p.len() == 1 {
                    Ok(())
                } else {
                    Err(owners::Error::Invariant)
                }
            }
        }
        let batch = enrolled
            .batch(
                vec![candidate.erase()],
                vec![],
                &[owner.id()],
                vec![owner.id()],
                [227; 32],
                Rc::new(One),
                coordinator.clone(),
                view,
            )
            .map_err(native_refusal)?;
        if q == 1 {
            let replacement = owner
                .prepare(
                    "wand_random_command_v2",
                    &encode(&req, MAX_REQUEST_BYTES).map_err(native_refusal)?,
                    vec![],
                    view,
                )
                .map_err(native_refusal)?;
            let newer = enrolled
                .batch(
                    vec![replacement.erase()],
                    vec![],
                    &[owner.id()],
                    vec![owner.id()],
                    [227; 32],
                    Rc::new(One),
                    coordinator.clone(),
                    view,
                )
                .map_err(native_refusal)?;
            newer.commit(view).map_err(native_refusal)?;
            assert!(batch.commit(view).is_err());
            batch.discard().map_err(native_refusal)?;
        } else {
            batch.commit(view).map_err(native_refusal)?;
            assert!(batch.commit(view).is_err());
        }
        Ok(vec![])
    });
    let provider = kernel
        .register_native(
            Manifest {
                id: cap.provider.clone(),
                exports: [
                    (cap.endpoint.clone(), endpoint(Access::Write)),
                    ("wand_random".into(), endpoint(Access::Read)),
                ]
                .into(),
                dependencies: vec![],
                extension_points: BTreeSet::new(),
                extension: None,
            },
            b"native raw ticket fixture",
            [
                (cap.endpoint.clone(), command),
                (
                    "wand_random".into(),
                    Rc::new(|_: &[u8], _: &CallerView| Err(kernel::Error::NativeRefusal))
                        as NativePort,
                ),
            ]
            .into(),
        )
        .unwrap();
    let state = RandomState {
        rng: rand::SeedableRng::seed_from_u64(719),
        seed: 719,
    };
    let mut expected = state.rng.clone();
    let owner = host
        .enroll(
            &kernel,
            OwnerSpec {
                module: provider,
                schema: [208; 32],
                ports: [
                    (cap.endpoint.clone(), port(Access::Write, vec![])),
                    ("wand_random".into(), port(Access::Read, vec![])),
                ]
                .into(),
            },
            state,
            Backend::Native(Rc::new(RandomCodec(config))),
        )
        .unwrap();
    *slot.borrow_mut() = Some(owner.clone());
    let caller = kernel
        .register_native(
            Manifest {
                id: "ticket-caller".into(),
                exports: [("run".into(), endpoint(Access::Write))].into(),
                dependencies: vec![
                    kernel::Dependency {
                        capability: cap.clone(),
                        access: Access::Write,
                    },
                    kernel::Dependency {
                        capability: Capability {
                            endpoint: "wand_random".into(),
                            ..cap
                        },
                        access: Access::Read,
                    },
                ],
                extension_points: BTreeSet::new(),
                extension: None,
            },
            b"native ticket caller",
            [(
                "run".into(),
                Rc::new(|request: &[u8], view: &CallerView| view.call_dependency(0, request))
                    as NativePort,
            )]
            .into(),
        )
        .unwrap();
    kernel
        .issue_grant(
            caller,
            0,
            Authority {
                access: Access::Write,
                scope: Scope::Keys(
                    [
                        owner.resource("wand_random_command_v2").unwrap(),
                        owner.resource("wand_random").unwrap(),
                        host.publication_resource(),
                    ]
                    .into(),
                ),
            },
        )
        .unwrap();
    kernel
        .issue_grant(
            caller,
            1,
            Authority {
                access: Access::Read,
                scope: Scope::Keys([owner.resource("wand_random").unwrap()].into()),
            },
        )
        .unwrap();
    for q in [0u8, 1] {
        let lease = kernel
            .capture_pipeline(
                caller,
                "run",
                Authority {
                    access: Access::Write,
                    scope: Scope::All,
                },
            )
            .unwrap();
        let id = kernel.issue_operation().unwrap();
        let operation = kernel
            .operation_binding_with_cleanup(
                id,
                lease,
                1,
                "raw-ticket",
                &[host.cleanup_identity().unwrap()],
            )
            .unwrap();
        kernel
            .invoke_operation(
                &operation,
                &encode(&q, 8).unwrap(),
                Authority {
                    access: Access::Write,
                    scope: Scope::All,
                },
            )
            .unwrap();
        kernel.release_pipeline(lease).unwrap();
        expected.next_u32();
    }
    let mut actual: RandomState = decode(&owner.inspect().unwrap(), PAYLOAD).unwrap();
    for _ in 0..8 {
        assert_eq!(actual.rng.next_u64(), expected.next_u64());
    }
    assert_eq!(host.pending().unwrap(), 0);
    assert_eq!(host.history().unwrap().len(), 2);
}
