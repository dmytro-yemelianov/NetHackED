use super::*;

struct Consume;
impl Schema<u64> for Consume {
    fn validate(&self, s: &u64) -> Result<()> {
        Mirror.validate(s)
    }
    fn encode(&self, s: &u64) -> Result<Vec<u8>> {
        Mirror.encode(s)
    }
    fn query(&self, s: &u64, n: &str, r: &[u8], o: &[Observation]) -> Result<Vec<u8>> {
        Mirror.query(s, n, r, o)
    }
}
impl NativeBehavior<u64> for Consume {
    fn prepare(&self, s: &u64, _: &str, _: &[u8], o: &[Observation]) -> Result<Proposal<u64>> {
        let value = u32::from_le_bytes(o[0].bytes().try_into().unwrap());
        Ok(Proposal {
            next: *s + u64::from(value),
            events: vec![b"consumed".to_vec()],
            reply: value.to_le_bytes().to_vec(),
        })
    }
}
fn consumer<T: 'static>(h: &mut Harness, owner: &Owner<T>) -> Rc<Owner<u64>> {
    let a = h
        .host
        .enroll(
            &h.kernel,
            h.spec(
                8,
                vec![Dependency {
                    owner: owner.id(),
                    query: "derived".into(),
                    schema: owner.stamp().unwrap().schema,
                }],
            ),
            0u64,
            Backend::Native(Rc::new(Consume)),
        )
        .unwrap();
    h.admit(&a);
    h.refresh();
    a
}

#[test]
fn retained_derived_dependency_survives_future_behavior_only() {
    for nested in [false, true] {
        for negative in 0..3 {
            let (mut h, item, b, count) = setup(ADD_OBSERVATION);
            let bridge = consumer(&mut h, &b);
            let a = if nested {
                consumer(&mut h, &bridge)
            } else {
                bridge.clone()
            };
            let cache = b.query_cache().unwrap();
            let input = observe(&h, item.clone());
            let saved = Rc::new(RefCell::new(None));
            let sink = saved.clone();
            let target = b.clone();
            let c = cache.clone();
            h.run(2, move |v| {
                let result = target
                    .query_cached(&c, "derived", &[1], std::slice::from_ref(&input), v)
                    .unwrap();
                assert!(!result.hit);
                assert_eq!(result.observation.bytes(), 9u32.to_le_bytes());
                *sink.borrow_mut() = Some(result.observation);
                Ok(vec![])
            });
            let mut old = saved.borrow_mut().take().unwrap();
            if nested {
                let sink = saved.clone();
                h.run(2, move |v| {
                    *sink.borrow_mut() = Some(
                        bridge
                            .query("derived", &[], std::slice::from_ref(&old), v)
                            .unwrap(),
                    );
                    Ok(vec![])
                });
                old = saved.borrow_mut().take().unwrap();
            }
            let staged = Rc::new(RefCell::new(None));
            let sink = staged.clone();
            let target = a.clone();
            let input = old.clone();
            h.run(1, move |v| {
                let candidate = target
                    .prepare("prepare", &[], vec![input.clone()], v)
                    .unwrap();
                candidate.validate(v).unwrap();
                let id = target.id();
                let batch = target
                    .host
                    .batch(
                        vec![Candidate {
                            stage: candidate.stage.clone(),
                        }
                        .erase()],
                        vec![input.clone()],
                        &[id],
                        vec![id],
                        [7; 32],
                        Rc::new(Accept),
                        target.capability("prepare").unwrap(),
                        v,
                    )
                    .unwrap();
                *sink.borrow_mut() = Some((candidate, batch));
                Ok(vec![])
            });
            let ext = h.extension(MULTIPLY);
            b.admit_compatible_wasm(
                &h.kernel,
                [3; 32],
                Rc::new(CountCodec(count.clone())),
                0,
                h.future_binding(ext),
            )
            .unwrap();
            b.set_query_policy(QueryPolicy::Deterministic {
                profile: [9; 32],
                input_policy: [10; 32],
            })
            .unwrap();
            if negative == 1 {
                commit_inventory(&h, item.clone(), 1);
            } else if negative == 2 {
                h.kernel.revoke_grant(h.root, 2).unwrap();
            }
            let (candidate, batch) = staged.borrow_mut().take().unwrap();
            let target = a.clone();
            let readonly = old.clone();
            h.run(1, move |v| {
                if negative == 0 {
                    // Fresh command and readonly batch admission still require
                    // current behavior, even in the old operation's invocation.
                    assert!(matches!(
                        target.prepare("prepare", &[], vec![readonly.clone()], v),
                        Err(Error::Stale)
                    ));
                    let id = target.id();
                    assert!(matches!(
                        target.host.batch(
                            vec![Candidate {
                                stage: candidate.stage.clone()
                            }
                            .erase()],
                            vec![readonly.clone()],
                            &[id],
                            vec![id],
                            [7; 32],
                            Rc::new(Accept),
                            target.capability("prepare").unwrap(),
                            v,
                        ),
                        Err(Error::Stale)
                    ));
                }
                let validation = candidate.validate(v);
                if negative == 0 {
                    validation.unwrap();
                } else {
                    assert!(validation.is_err());
                }
                let result = batch.commit(v);
                if negative == 0 {
                    result.unwrap();
                } else {
                    assert!(result.is_err());
                }
                assert_eq!(
                    target.inspect().unwrap(),
                    (if negative == 0 { 9u64 } else { 0u64 }).to_le_bytes()
                );
                Ok(vec![])
            });
            assert_eq!(count.get(), 1, "commit must not recompute B");
            if negative == 0 {
                let target = a;
                h.current(2, move |v| {
                    assert!(matches!(
                        target.query("derived", &[], std::slice::from_ref(&old), v),
                        Err(Error::Stale)
                    ));
                    Ok(vec![])
                });
                cached(&h, item, b, cache, 20, false);
                assert_eq!(count.get(), 2);
            }
        }
    }
}

fn descendant_cap() -> Capability {
    Capability {
        provider: "descendant".into(),
        endpoint: "compute".into(),
        major: 1,
    }
}
fn descendant_manifest(id: &str, extension: Option<Extension>) -> Manifest {
    Manifest {
        id: id.into(),
        exports: BTreeMap::from([("compute".into(), endpoint(Access::Read))]),
        dependencies: vec![],
        extension_points: BTreeSet::from(["compute".into()]),
        extension,
    }
}
// Descendant computes state + observation; this helper really calls it, checks
// its returned length, then adds a reached marker to the computed output.
fn calling_helper(marker: u32) -> Vec<u8> {
    wat::parse_str(
        r#"(module
        (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 1)
        (func (export "alloc") (param i32) (result i32) i32.const 0)
        (func (export "compute") (param $p i32) (param $n i32) (result i64)
            i32.const 0 local.get $p local.get $n i32.const 1024 i32.const 1024 call $call
            i32.const 4 i32.ne if unreachable end
            i32.const 1024 i32.const 1024 i32.load i32.const 100 i32.add i32.store
            i64.const 4398046511108))"#
            .replace("i32.const 100", &format!("i32.const {marker}")),
    )
    .unwrap()
}
fn chain_setup() -> (Setup, ModuleHandle, BTreeSet<Resource>) {
    let mut kernel = Kernel::new(KernelLimits::default()).unwrap();
    let descendant = kernel
        .register_guest(
            descendant_manifest("descendant", None),
            &helper_wasm(ADD_OBSERVATION),
        )
        .unwrap();
    let helper = kernel
        .register_guest(
            Manifest {
                id: "algorithm".into(),
                exports: BTreeMap::from([("compute".into(), endpoint(Access::Read))]),
                dependencies: vec![KernelDependency {
                    capability: descendant_cap(),
                    access: Access::Read,
                }],
                extension_points: BTreeSet::from(["compute".into()]),
                extension: None,
            },
            &calling_helper(100),
        )
        .unwrap();
    let ceiling = BTreeSet::from([
        Resource {
            owner: "descendant".into(),
            key: "one".into(),
        },
        Resource {
            owner: "descendant".into(),
            key: "two".into(),
        },
    ]);
    kernel
        .issue_grant(
            helper,
            0,
            Authority {
                access: Access::Read,
                scope: Scope::Keys(ceiling.clone()),
            },
        )
        .unwrap();
    let mut h = Harness::with_helper(Host::new(Limits::default()).unwrap(), kernel, helper);
    // Each nested effective authority intersects with this finite ceiling.
    h.kernel
        .issue_grant(
            h.module,
            0,
            Authority {
                access: Access::Read,
                scope: Scope::Keys(ceiling.clone()),
            },
        )
        .unwrap();
    let item = inventory(&mut h);
    let count = Rc::new(Cell::new(0));
    let codec = Rc::new(CountCodec(count.clone()));
    let b = h
        .host
        .enroll(
            &h.kernel,
            h.spec(3, vec![dep(&item)]),
            5u32.to_le_bytes().to_vec(),
            Backend::Wasm {
                codec: codec.clone(),
                slot: 0,
                binding: h.binding(),
            },
        )
        .unwrap();
    h.admit(&b);
    h.keys.extend(ceiling.iter().cloned());
    h.keys.insert(h.host.publication_resource());
    h.refresh();
    b.admit_compatible_wasm(&h.kernel, [3; 32], codec, 0, h.binding())
        .unwrap();
    b.set_query_policy(QueryPolicy::Deterministic {
        profile: [1; 32],
        input_policy: [2; 32],
    })
    .unwrap();
    ((h, item, b, count), descendant, ceiling)
}
fn save_chain_observation(
    h: &Harness,
    item: Rc<Owner<Inventory>>,
    b: Rc<Owner<Vec<u8>>>,
    cache: Rc<QueryCache>,
    expected: u32,
) -> Observation {
    let saved = Rc::new(RefCell::new(None));
    let sink = saved.clone();
    let input = observe(h, item);
    h.current(2, move |v| {
        let first = b
            .query_cached(&cache, "derived", &[1], std::slice::from_ref(&input), v)
            .unwrap();
        assert!(!first.hit);
        assert_eq!(
            first.observation.bytes(),
            expected.to_le_bytes(),
            "descendant and helper both reached"
        );
        let hit = b
            .query_cached(&cache, "derived", &[1], std::slice::from_ref(&input), v)
            .unwrap();
        assert!(hit.hit);
        assert_eq!(hit.observation.bytes(), first.observation.bytes());
        *sink.borrow_mut() = Some(first.observation);
        Ok(vec![])
    });
    let observation = saved.borrow_mut().take().unwrap();
    observation
}
fn consume_chain(h: &Harness, a: Rc<Owner<u64>>, input: Observation, success: bool) {
    h.current(2, move |v| {
        let result = a.query("derived", &[], std::slice::from_ref(&input), v);
        if success {
            assert_eq!(result.unwrap().bytes(), input.bytes());
        } else {
            assert!(result.is_err());
        }
        Ok(vec![])
    });
}

#[test]
fn guest_descendant_positive_then_live_revocation_and_finite_narrowing() {
    for revoke in [true, false] {
        let ((mut h, item, b, count), _, ceiling) = chain_setup();
        let a = consumer(&mut h, &b);
        let cache = b.query_cache().unwrap();
        let old = save_chain_observation(&h, item.clone(), b.clone(), cache.clone(), 109);
        consume_chain(&h, a.clone(), old.clone(), true);
        assert_eq!(count.get(), 1);
        let owner_before = b.inspect().unwrap();
        let history_before = h.host.history().unwrap();
        if revoke {
            h.kernel.revoke_grant(h.helper, 0).unwrap();
        } else {
            h.kernel
                .issue_grant(
                    h.helper,
                    0,
                    Authority {
                        access: Access::Read,
                        scope: Scope::Keys(BTreeSet::from([ceiling.first().unwrap().clone()])),
                    },
                )
                .unwrap();
        }
        let target = b.clone();
        let c = cache.clone();
        let input = observe(&h, item);
        h.current(2, move |v| {
            assert!(target
                .query_cached(&c, "derived", &[1], std::slice::from_ref(&input), v)
                .is_err());
            Ok(vec![])
        });
        consume_chain(&h, a, old, false);
        assert_eq!(
            count.get(),
            1,
            "nonexecuting capture validation must refuse before codec"
        );
        assert_eq!(b.inspect().unwrap(), owner_before);
        assert_eq!(h.host.history().unwrap(), history_before);
    }
}

#[test]
fn guest_descendant_replacement_current_refuses_retained_executes_original() {
    let ((mut h, item, b, count), _, _) = chain_setup();
    let a = consumer(&mut h, &b);
    let cache = b.query_cache().unwrap();
    let old = save_chain_observation(&h, item.clone(), b.clone(), cache.clone(), 109);
    consume_chain(&h, a.clone(), old.clone(), true);
    let replacement = h
        .kernel
        .register_guest(
            descendant_manifest(
                "descendant-v2",
                Some(Extension {
                    target: descendant_cap(),
                    export: "compute".into(),
                }),
            ),
            &helper_wasm(MULTIPLY),
        )
        .unwrap();
    let input = observe(&h, item.clone());
    let target = b.clone();
    let c = cache.clone();
    h.current(2, move |v| {
        assert!(target
            .query_cached(&c, "derived", &[1], std::slice::from_ref(&input), v)
            .is_err());
        Ok(vec![])
    });
    consume_chain(&h, a.clone(), old.clone(), false);
    assert_eq!(count.get(), 1);
    // Original issued operation captured the original descendant route.
    let target = b.clone();
    let input = observe(&h, item.clone());
    let c = cache.clone();
    let consumer = a.clone();
    h.run(2, move |v| {
        let hit = target
            .query_cached(&c, "derived", &[1], std::slice::from_ref(&input), v)
            .unwrap();
        assert!(hit.hit);
        assert_eq!(hit.observation.bytes(), 109u32.to_le_bytes());
        let fresh = target
            .query("derived", &[1], std::slice::from_ref(&input), v)
            .unwrap();
        assert_eq!(
            fresh.bytes(),
            109u32.to_le_bytes(),
            "old descendant actually executed"
        );
        assert_eq!(
            consumer
                .query("derived", &[], std::slice::from_ref(&old), v)
                .unwrap()
                .bytes(),
            old.bytes()
        );
        Ok(vec![])
    });
    assert_eq!(count.get(), 2);
    // Explicit future admission enables the new recursive code chain.
    b.admit_compatible_wasm(
        &h.kernel,
        [3; 32],
        Rc::new(CountCodec(count.clone())),
        0,
        h.binding(),
    )
    .unwrap();
    let new = save_chain_observation(&h, item, b.clone(), cache, 120);
    consume_chain(&h, a, new, true);
    assert_eq!(count.get(), 3);
    h.kernel.managed_detach(replacement).unwrap();
    assert_eq!(b.inspect().unwrap(), 5u32.to_le_bytes());
    assert!(h.host.history().unwrap().is_empty());
}

#[test]
fn extension_descendant_native_refusal_before_callback_current_and_retained() {
    let ((mut h, item, b, count), _, ceiling) = chain_setup();
    let a = consumer(&mut h, &b);
    let wrapper = h
        .kernel
        .register_guest(
            Manifest {
                id: "calling-wrapper".into(),
                exports: BTreeMap::from([("compute".into(), endpoint(Access::Read))]),
                dependencies: vec![KernelDependency {
                    capability: descendant_cap(),
                    access: Access::Read,
                }],
                extension_points: BTreeSet::new(),
                extension: Some(Extension {
                    target: h.binding().capability,
                    export: "compute".into(),
                }),
            },
            &calling_helper(200),
        )
        .unwrap();
    h.kernel
        .issue_grant(
            wrapper,
            0,
            Authority {
                access: Access::Read,
                scope: Scope::Keys(ceiling.clone()),
            },
        )
        .unwrap();
    h.refresh();
    b.admit_compatible_wasm(
        &h.kernel,
        [3; 32],
        Rc::new(CountCodec(count.clone())),
        0,
        h.future_binding(wrapper),
    )
    .unwrap();
    let cache = b.query_cache().unwrap();
    let old = save_chain_observation(&h, item.clone(), b.clone(), cache.clone(), 209);
    consume_chain(&h, a.clone(), old.clone(), true);
    assert_eq!(
        count.get(),
        1,
        "extension wrapper really reached guest descendant"
    );
    let calls = Rc::new(Cell::new(0));
    let reached = calls.clone();
    let callback: NativePort = Rc::new(move |_, _| {
        reached.set(reached.get() + 1);
        Ok(700u32.to_le_bytes().to_vec())
    });
    let native = h
        .kernel
        .register_native(
            descendant_manifest(
                "native-descendant",
                Some(Extension {
                    target: descendant_cap(),
                    export: "compute".into(),
                }),
            ),
            b"observable native descendant",
            BTreeMap::from([("compute".into(), callback)]),
        )
        .unwrap();
    let input = observe(&h, item.clone());
    let target = b.clone();
    let c = cache.clone();
    h.current(2, move |v| {
        assert!(matches!(
            target.query_cached(&c, "derived", &[1], std::slice::from_ref(&input), v),
            Err(Error::Kernel(nethacked_kernel::Error::NativeRefusal))
        ));
        assert!(matches!(
            target.query("derived", &[1], std::slice::from_ref(&input), v),
            Err(Error::Kernel(nethacked_kernel::Error::NativeRefusal))
        ));
        Ok(vec![])
    });
    let target = a.clone();
    let input = old.clone();
    h.current(2, move |v| {
        assert!(matches!(
            target.query("derived", &[], std::slice::from_ref(&input), v),
            Err(Error::Kernel(nethacked_kernel::Error::NativeRefusal))
        ));
        Ok(vec![])
    });
    assert_eq!(count.get(), 1, "refusal before codec/guest execution");
    assert_eq!(
        calls.get(),
        0,
        "native callback never reached through pure closure"
    );
    let input = observe(&h, item);
    let target = b.clone();
    h.run(2, move |v| {
        let hit = target
            .query_cached(&cache, "derived", &[1], std::slice::from_ref(&input), v)
            .unwrap();
        assert!(hit.hit);
        assert_eq!(hit.observation.bytes(), 209u32.to_le_bytes());
        let fresh = target
            .query("derived", &[1], std::slice::from_ref(&input), v)
            .unwrap();
        assert_eq!(fresh.bytes(), 209u32.to_le_bytes());
        assert_eq!(
            a.query("derived", &[], std::slice::from_ref(&old), v)
                .unwrap()
                .bytes(),
            old.bytes()
        );
        Ok(vec![])
    });
    assert_eq!(count.get(), 2);
    assert_eq!(calls.get(), 0);
    assert_eq!(b.inspect().unwrap(), 5u32.to_le_bytes());
    assert!(h.host.history().unwrap().is_empty());
    // Verify the counter would record execution: a trusted direct native call
    // remains available outside the pure bridge.
    assert_eq!(
        h.kernel
            .invoke(
                native,
                "compute",
                &[],
                Authority {
                    access: Access::Read,
                    scope: Scope::Keys(ceiling)
                }
            )
            .unwrap(),
        700u32.to_le_bytes()
    );
    assert_eq!(calls.get(), 1);
}
