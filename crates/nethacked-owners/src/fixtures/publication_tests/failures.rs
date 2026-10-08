use super::*;
#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    slots: Vec<Vec<u8>>,
    registry: Vec<Stamp>,
    history: Vec<CommitReceipt>,
    pending: usize,
    publication: (u64, u64, usize, usize, usize),
    staging: (u64, usize),
}
fn snapshot(m: &Multi, s: &Owner<Stock>, d: &Owner<Stock>, i: &Owner<Item>) -> Snapshot {
    Snapshot {
        slots: vec![
            s.inspect().unwrap(),
            d.inspect().unwrap(),
            i.inspect().unwrap(),
        ],
        registry: m.host.registry().unwrap(),
        history: m.host.history().unwrap(),
        pending: m.host.pending().unwrap(),
        publication: m.host.0.publication.borrow().counters(),
        staging: {
            let r = m.host.0.registry.borrow();
            (r.next_candidate, r.staging_bytes)
        },
    }
}
fn refused(m: &Multi, b: Rc<Batch>, error: Error) {
    let reached = Rc::new(Cell::new(0));
    let marks = reached.clone();
    let swaps = Rc::new(Cell::new(0));
    let seen = swaps.clone();
    let consumed = b.consumed_for_test();
    let batch = b.clone();
    SWAP_PROBE.with(|p| *p.borrow_mut() = Some(Box::new(move || seen.set(seen.get() + 1))));
    m.run(10, move |v| {
        assert_eq!(b.commit(v), Err(error));
        assert_eq!(b.commit(v), Err(error));
        marks.set(marks.get() + 1);
        Ok(vec![])
    });
    SWAP_PROBE.with(|p| p.borrow_mut().take());
    assert_eq!(swaps.get(), 0);
    assert_eq!(batch.consumed_for_test(), consumed);
    assert_eq!(reached.get(), 1);
}
#[test]
fn refusal_matrix_preserves_slots_prefix_pending_and_retryable_tickets() {
    for kind in [
        "revocation",
        "scope",
        "foreign-phase",
        "foreign-lease",
        "stale-write",
        "schema",
        "code",
        "digest",
        "epoch",
        "history",
        "reply",
        "events",
        "retention",
    ] {
        let (mut m, s, d, i) = transfer_host();
        if ["history", "reply", "events", "retention"].contains(&kind) {
            let mut limits = PublicationLimits::default();
            match kind {
                "history" => limits.history_bytes = 1,
                "reply" => limits.reply_bytes = 1,
                "events" => limits.history_events = 1,
                "retention" => limits.retained_bytes = 49_170,
                _ => unreachable!(),
            }
            m.host.configure_publication(limits).unwrap();
        }
        let reached = Rc::new(Cell::new(0));
        let mut candidates = vec![
            m.prepare(0, s.clone(), 0).erase(),
            m.prepare(1, d.clone(), 1).erase(),
        ];
        let candidate = m.prepare(2, i.clone(), 1);
        if kind == "code" {
            candidate
                .stage
                .prepared
                .borrow_mut()
                .as_mut()
                .unwrap()
                .code
                .clear();
        }
        if kind == "digest" {
            candidate
                .stage
                .prepared
                .borrow_mut()
                .as_mut()
                .unwrap()
                .next
                .live = false;
        }
        candidates.push(candidate.erase());
        let mut readonly = vec![];
        if kind == "schema" {
            let holder = Rc::new(RefCell::new(None));
            let out = holder.clone();
            let source = s.clone();
            m.run(0, move |v| {
                *out.borrow_mut() = Some(source.query("query", &[], &[], v).unwrap());
                Ok(vec![])
            });
            let mut o = holder.borrow_mut().take().unwrap();
            o.stamp.schema = [99; 32];
            readonly.push(o);
        }
        let batch = m.batch(
            candidates,
            vec![s.id(), d.id(), i.id()],
            vec![i.id(), s.id(), d.id()],
            Rc::new(Transfer {
                source: s.id(),
                dest: d.id(),
                item: i.id(),
                reached: reached.clone(),
            }),
            readonly,
        );
        let expected = match kind {
            "revocation" => {
                m.kernel.revoke_grant(m.root, 3).unwrap();
                Error::Authority
            }
            "scope" => {
                m.kernel
                    .issue_grant(
                        m.root,
                        3,
                        Authority {
                            access: Access::Write,
                            scope: Scope::Keys(BTreeSet::new()),
                        },
                    )
                    .unwrap();
                Error::Authority
            }
            "foreign-phase" => {
                let id = m.kernel.issue_operation().unwrap();
                m.operation = m
                    .kernel
                    .operation_binding_with_cleanup(
                        id,
                        m.lease,
                        2,
                        "other",
                        &[m.host.cleanup_identity().unwrap()],
                    )
                    .unwrap();
                Error::Authority
            }
            "foreign-lease" => {
                let lease = m
                    .kernel
                    .capture_pipeline(
                        m.root,
                        "run",
                        Authority {
                            access: Access::Write,
                            scope: Scope::All,
                        },
                    )
                    .unwrap();
                m.operation = m
                    .kernel
                    .operation_binding_with_cleanup(
                        m.operation.operation(),
                        lease,
                        1,
                        "transfer",
                        &[m.host.cleanup_identity().unwrap()],
                    )
                    .unwrap();
                Error::Authority
            }
            "stale-write" => {
                let other = m.batch(
                    vec![m.prepare(0, s.clone(), 1).erase()],
                    vec![s.id()],
                    vec![s.id()],
                    Rc::new(Allow),
                    vec![],
                );
                m.run(10, move |v| Ok(other.commit(v).unwrap().reply));
                Error::Stale
            }
            "schema" | "code" => Error::Stale,
            "digest" => Error::Invariant,
            "epoch" => {
                m.host.0.publication.borrow_mut().epoch = u64::MAX;
                Error::Limit
            }
            _ => Error::Limit,
        };
        let before = snapshot(&m, &s, &d, &i);
        refused(&m, batch.clone(), expected);
        assert_eq!(snapshot(&m, &s, &d, &i), before, "branch {kind}");
        assert_eq!(
            reached.get(),
            if ["epoch", "history", "reply", "events", "retention"].contains(&kind) {
                2
            } else {
                0
            },
            "branch {kind}"
        );
        batch.discard().unwrap();
        assert_eq!(m.host.pending().unwrap(), 0);
    }
}

#[test]
fn stale_readonly_and_detached_coordinator_refuse_and_cleanup_is_checkpoint_exact() {
    let (m, s, d, i) = transfer_host();
    let holder = Rc::new(RefCell::new(None));
    let sink = holder.clone();
    let dest = d.clone();
    m.run(2, move |v| {
        *sink.borrow_mut() = Some(dest.query("query", &[], &[], v).unwrap());
        Ok(vec![])
    });
    let observation = holder.borrow_mut().take().unwrap();
    let external = m.prepare(0, s.clone(), 0);
    let batch = m.batch(
        vec![m.prepare(2, i.clone(), 1).erase()],
        vec![i.id()],
        vec![i.id()],
        Rc::new(Allow),
        vec![observation],
    );
    let mutate = m.batch(
        vec![m.prepare(1, d.clone(), 1).erase()],
        vec![d.id()],
        vec![d.id()],
        Rc::new(Allow),
        vec![],
    );
    m.run(10, move |v| Ok(mutate.commit(v).unwrap().reply));
    let before = snapshot(&m, &s, &d, &i);
    refused(&m, batch.clone(), Error::Stale);
    assert_eq!(snapshot(&m, &s, &d, &i), before);
    let b = batch.clone();
    m.run(10, move |v| {
        assert_eq!(
            b.commit(&v.clone()),
            Err(Error::Kernel(nethacked_kernel::Error::Authority))
        );
        Ok(vec![])
    });
    let foreign = m
        .kernel
        .operation_binding_with_cleanup(
            m.operation.operation(),
            m.lease,
            2,
            "other",
            &[m.host.cleanup_identity().unwrap()],
        )
        .unwrap();
    m.host.discard_operation(&foreign).unwrap();
    assert_eq!(snapshot(&m, &s, &d, &i), before);
    let gate = m.host.close_visibility().unwrap();
    assert_eq!(m.host.discard_operation(&m.operation), Err(Error::Busy));
    drop(gate);
    m.host.discard_operation(&m.operation).unwrap();
    m.host.discard_operation(&m.operation).unwrap();
    assert!(external.stage.prepared.borrow().is_none());
    assert!(external.stage.owner.upgrade().is_some());
    assert_eq!(m.host.pending().unwrap(), 0);
    assert_eq!(m.host.history().unwrap(), before.history);
    refused(&m, batch, Error::Stale);
    let source = s.clone();
    m.run(1, move |v| {
        assert_eq!(external.validate(v), Err(Error::Stale));
        assert_eq!(source.inspect().unwrap(), [1, 2]);
        Ok(vec![])
    });
}

#[test]
fn duplicate_and_omitted_declarations_refuse_before_accounting_without_consumption() {
    for duplicate in [false, true] {
        let (m, s, d, i) = transfer_host();
        let c = m.prepare(0, s.clone(), 0);
        let stage: Rc<dyn Staged> = c.stage.clone();
        let candidates = if duplicate {
            vec![c.erase(), ErasedCandidate { stage }]
        } else {
            vec![c.erase()]
        };
        let reached = Rc::new(Cell::new(0));
        let b = m.batch(
            candidates,
            vec![s.id(), d.id(), i.id()],
            vec![s.id(), d.id(), i.id()],
            Rc::new(Transfer {
                source: s.id(),
                dest: d.id(),
                item: i.id(),
                reached: reached.clone(),
            }),
            vec![],
        );
        let before = snapshot(&m, &s, &d, &i);
        refused(&m, b.clone(), Error::Invalid);
        assert_eq!(snapshot(&m, &s, &d, &i), before);
        assert_eq!(reached.get(), 0);
        assert_eq!(before.pending, 1);
        b.discard().unwrap();
        assert_eq!(m.host.pending().unwrap(), 0);
        refused(&m, b, Error::Stale);
    }
}
struct PanicPolicy {
    panic: Rc<Cell<bool>>,
    reached: Rc<Cell<usize>>,
}
impl Accounting for PanicPolicy {
    fn validate(&self, _: &[ProposalView], _: &[Observation]) -> Result<()> {
        self.reached.set(self.reached.get() + 1);
        if self.panic.get() {
            panic!("contained-pure-preflight-marker");
        }
        Ok(())
    }
}

#[test]
fn foreign_gate_candidates_remain_owned_by_their_host_and_missing_cleanup_refuses_prepare() {
    let (mut m, s, _, _) = transfer_host();
    let foreign = Host::new(Limits::default()).unwrap();
    let owner = foreign
        .enroll(
            &m.kernel,
            m.spec(0, 1),
            Stock {
                qty: 1,
                capacity: 2,
            },
            Backend::Native(Rc::new(StockDomain)),
        )
        .unwrap();
    m.operation = m
        .kernel
        .operation_binding_with_cleanup(
            m.operation.operation(),
            m.lease,
            1,
            "transfer",
            &[
                m.host.cleanup_identity().unwrap(),
                foreign.cleanup_identity().unwrap(),
            ],
        )
        .unwrap();
    m.kernel
        .issue_grant(
            m.root,
            1,
            Authority {
                access: Access::Write,
                scope: Scope::Keys(BTreeSet::from([
                    owner.resource("prepare").unwrap(),
                    s.resource("prepare").unwrap(),
                ])),
            },
        )
        .unwrap();
    m.kernel.release_pipeline(m.lease).unwrap();
    m.lease = m
        .kernel
        .capture_pipeline(
            m.root,
            "run",
            Authority {
                access: Access::Write,
                scope: Scope::All,
            },
        )
        .unwrap();
    m.operation = m
        .kernel
        .operation_binding_with_cleanup(
            m.operation.operation(),
            m.lease,
            1,
            "transfer",
            &[
                m.host.cleanup_identity().unwrap(),
                foreign.cleanup_identity().unwrap(),
            ],
        )
        .unwrap();
    let candidate = m.prepare(0, owner.clone(), 0);
    let batch = m.batch(
        vec![candidate.erase()],
        vec![owner.id()],
        vec![owner.id()],
        Rc::new(Allow),
        vec![],
    );
    let before = foreign.registry().unwrap();
    refused(&m, batch.clone(), Error::Stale);
    assert_eq!(foreign.pending().unwrap(), 1);
    assert_eq!(foreign.registry().unwrap(), before);
    assert_eq!(owner.inspect().unwrap(), [1, 2]);
    assert_eq!(m.host.history().unwrap(), vec![]);
    foreign.discard_operation(&m.operation).unwrap();
    batch.discard().unwrap();
    assert_eq!(foreign.pending().unwrap(), 0);
    m.operation = m
        .kernel
        .operation_binding(m.operation.operation(), m.lease, 2, "unregistered")
        .unwrap();
    let reached = Rc::new(Cell::new(false));
    let marker = reached.clone();
    let source = s.clone();
    m.run(1, move |v| {
        assert!(!v.operation().unwrap().admits_cleanup(source.host.0.cleanup));
        assert!(matches!(
            source.prepare("prepare", &[0], vec![], v),
            Err(Error::Authority)
        ));
        marker.set(true);
        Ok(vec![])
    });
    assert!(reached.get());
    assert_eq!(m.host.pending().unwrap(), 0);
    assert_eq!(s.inspect().unwrap(), [1, 2]);
}

#[test]
fn authentic_foreign_caller_and_artifact_cannot_adopt_the_checkpoint() {
    let (mut m, s, d, i) = transfer_host();
    let b = transfer_batch(&m, &s, &d, &i, Rc::new(Cell::new(0)));
    let foreign=m.kernel.register_guest(Manifest {id:"foreign-caller".into(),
        exports:BTreeMap::from([("run".into(),endpoint(Access::Write))]),
        dependencies:vec![KernelDependency {capability:named_cap("coordinator","commit"),access:Access::Write}],
        extension_points:BTreeSet::new(),extension:None},
        &wat::parse_str(r#"(module
        (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 1)
        (func (export "alloc") (param i32) (result i32) i32.const 0)
        (func (export "run") (param i32 i32) (result i64)
         i32.const 0 i32.const 0 i32.const 0 i32.const 2048 i32.const 1024 call $call drop i64.const 0))"#).unwrap()).unwrap();
    m.kernel
        .issue_grant(
            foreign,
            0,
            Authority {
                access: Access::Write,
                scope: Scope::Keys(BTreeSet::from([m.host.publication_resource()])),
            },
        )
        .unwrap();
    let captured = m.kernel.artifact(m.root).unwrap();
    let lease = m
        .kernel
        .capture_pipeline(
            foreign,
            "run",
            Authority {
                access: Access::Write,
                scope: Scope::All,
            },
        )
        .unwrap();
    m.operation = m
        .kernel
        .operation_binding_with_cleanup(
            m.operation.operation(),
            lease,
            1,
            "transfer",
            &[m.host.cleanup_identity().unwrap()],
        )
        .unwrap();
    let before = snapshot(&m, &s, &d, &i);
    let reached = Rc::new(Cell::new(false));
    let marker = reached.clone();
    let batch = b.clone();
    m.run(10, move |v| {
        assert_eq!(v.caller(), Some(foreign));
        assert_ne!(v.caller_artifact(), Some(captured));
        assert_eq!(batch.commit(v), Err(Error::Authority));
        marker.set(true);
        Ok(vec![])
    });
    assert!(reached.get());
    assert_eq!(snapshot(&m, &s, &d, &i), before);
}

#[test]
fn last_typed_guard_refusal_and_cleanup_borrow_refusal_have_zero_partial_effects() {
    let (m, s, d, i) = transfer_host();
    let reached = Rc::new(Cell::new(0));
    let b = transfer_batch(&m, &s, &d, &i, reached.clone());
    let before = snapshot(&m, &s, &d, &i);
    // Immutable last-slot borrow permits preflight encoding but prevents its
    // write guard, after the first two canonical guards have been acquired.
    let held = i.slot.borrow();
    refused(&m, b.clone(), Error::Busy);
    assert_eq!(reached.get(), 2);
    assert_eq!(snapshot(&m, &s, &d, &i), before);
    drop(held);
    let external = m.prepare(2, i.clone(), 1);
    let held = external.stage.prepared.borrow_mut();
    let pending = m.host.pending().unwrap();
    assert_eq!(m.host.discard_operation(&m.operation), Err(Error::Busy));
    assert_eq!(m.host.pending().unwrap(), pending);
    assert_eq!(m.host.history().unwrap(), before.history);
    drop(held);
    let batch = b.clone();
    m.run(10, move |v| Ok(batch.commit(v).unwrap().reply));
    assert_eq!(reached.get(), 3);
    assert_eq!(m.host.pending().unwrap(), 1);
    m.host.discard_operation(&m.operation).unwrap();
    assert_eq!(m.host.pending().unwrap(), 0);
    assert!(external.stage.prepared.borrow().is_none());
    assert_eq!(m.host.history().unwrap().len(), 1);
}
#[test]
fn contained_pure_preflight_panic_preserves_retryable_proposals_and_no_swap_occurs() {
    let (m, s, d, i) = transfer_host();
    let panic = Rc::new(Cell::new(true));
    let reached = Rc::new(Cell::new(0));
    let b = m.batch(
        vec![m.prepare(0, s.clone(), 0).erase()],
        vec![s.id()],
        vec![s.id()],
        Rc::new(PanicPolicy {
            panic: panic.clone(),
            reached: reached.clone(),
        }),
        vec![],
    );
    let before = snapshot(&m, &s, &d, &i);
    let batch = b.clone();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        m.run(10, move |v| Ok(batch.commit(v).unwrap().reply))
    }));
    assert!(result.is_err());
    m.action.borrow_mut().take();
    assert_eq!(reached.get(), 1);
    assert_eq!(snapshot(&m, &s, &d, &i), before);
    panic.set(false);
    m.run(10, move |v| Ok(b.commit(v).unwrap().reply));
    assert_eq!(reached.get(), 2);
    assert_eq!(m.host.history().unwrap().len(), 1);
    assert_eq!(s.inspect().unwrap(), [0, 2]);
}
