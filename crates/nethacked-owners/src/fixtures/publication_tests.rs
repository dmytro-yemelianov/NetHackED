use super::*;
use crate::publication::SWAP_PROBE;

struct Multi {
    kernel: Kernel,
    host: Host,
    root: ModuleHandle,
    modules: Vec<ModuleHandle>,
    helper: ModuleHandle,
    action: Rc<RefCell<Option<Action>>>,
    lease: nethacked_kernel::PipelineLease,
    operation: OperationBinding,
}
const NAMES: [&str; 5] = ["source", "destination", "item", "debt", "device"];
mod destructors;
mod domains;
mod failures;
mod guest;
#[cfg(feature = "workflows")]
mod workflow;
use domains::*;
type Trio = (Multi, Rc<Owner<Stock>>, Rc<Owner<Stock>>, Rc<Owner<Item>>);
fn transfer_host() -> Trio {
    let m = Multi::new();
    let source = m
        .host
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
    let dest = m
        .host
        .enroll(
            &m.kernel,
            m.spec(1, 1),
            Stock {
                qty: 0,
                capacity: 2,
            },
            Backend::Native(Rc::new(StockDomain)),
        )
        .unwrap();
    let item = m
        .host
        .enroll(
            &m.kernel,
            m.spec(2, 2),
            Item {
                place: 0,
                live: true,
            },
            Backend::Native(Rc::new(ItemDomain)),
        )
        .unwrap();
    (m, source, dest, item)
}
fn transfer_batch(
    m: &Multi,
    s: &Rc<Owner<Stock>>,
    d: &Rc<Owner<Stock>>,
    i: &Rc<Owner<Item>>,
    reached: Rc<Cell<usize>>,
) -> Rc<Batch> {
    let candidates = vec![
        m.prepare(0, s.clone(), 0).erase(),
        m.prepare(1, d.clone(), 1).erase(),
        m.prepare(2, i.clone(), 1).erase(),
    ];
    m.batch(
        candidates,
        vec![s.id(), d.id(), i.id()],
        vec![i.id(), s.id(), d.id()],
        Rc::new(Transfer {
            source: s.id(),
            dest: d.id(),
            item: i.id(),
            reached,
        }),
        vec![],
    )
}
fn named_cap(provider: &str, endpoint: &str) -> Capability {
    Capability {
        provider: provider.into(),
        endpoint: endpoint.into(),
        major: 1,
    }
}

#[test]
fn heterogeneous_transfer_excludes_every_read_between_swaps_and_publishes_profile_prefix() {
    let (m, s, d, i) = transfer_host();
    let reached = Rc::new(Cell::new(0));
    let batch = transfer_batch(&m, &s, &d, &i, reached.clone());
    let swaps = Rc::new(RefCell::new(Vec::with_capacity(3)));
    let marks = swaps.clone();
    let host = m.host.clone();
    let source = s.clone();
    let dest = d.clone();
    let item = i.clone();
    let b = batch.clone();
    m.run(10, move |view| {
        let detached = view.clone();
        let host = host.clone();
        let source = source.clone();
        let dest = dest.clone();
        let item = item.clone();
        let marks = marks.clone();
        let reentry = b.clone();
        let ids = vec![item.id(), source.id(), dest.id()];
        SWAP_PROBE.with(|p| {
            *p.borrow_mut() = Some(Box::new(move || {
                assert_eq!(source.inspect(), Err(Error::Busy));
                assert_eq!(dest.stamp(), Err(Error::Busy));
                assert!(matches!(
                    item.query("query", &[], &[], &detached),
                    Err(Error::Busy)
                ));
                assert!(matches!(
                    source.prepare("prepare", &[1], vec![], &detached),
                    Err(Error::Busy)
                ));
                assert_eq!(host.registry(), Err(Error::Busy));
                assert_eq!(host.history(), Err(Error::Busy));
                assert_eq!(host.pending(), Err(Error::Busy));
                assert_eq!(reentry.commit(&detached), Err(Error::Busy));
                marks.borrow_mut().push([
                    source.metadata.revision.get(),
                    dest.metadata.revision.get(),
                    item.metadata.revision.get(),
                ]);
            }))
        });
        let receipt = b.commit(view).unwrap();
        SWAP_PROBE.with(|p| p.borrow_mut().take());
        assert_eq!(crate::probes::counts(), [0; 5]);
        assert_eq!(
            receipt.events,
            vec![vec![b'i', 1], vec![b's', 0], vec![b's', 1]]
        );
        assert_eq!(
            receipt
                .participants
                .iter()
                .map(|r| r.stamp.owner)
                .collect::<Vec<_>>(),
            ids
        );
        assert_eq!(b.commit(view), Err(Error::Stale));
        Ok(receipt.reply)
    });
    assert_eq!(reached.get(), 1);
    assert_eq!(*swaps.borrow(), [[1, 0, 0], [1, 1, 0], [1, 1, 1]]);
    assert_eq!(s.inspect().unwrap(), [0, 2]);
    assert_eq!(d.inspect().unwrap(), [1, 2]);
    assert_eq!(i.inspect().unwrap(), [1, 1]);
    assert_eq!(m.host.pending().unwrap(), 0);
    assert_eq!(m.host.history().unwrap().len(), 1);
    assert!(m.host.registry().unwrap().iter().all(|s| s.revision == 1));
}

#[test]
fn debt_retirement_requires_accounting_owner_and_preserves_terms_and_prior_transfer() {
    let (m, s, d, i) = transfer_host();
    let transfer = transfer_batch(&m, &s, &d, &i, Rc::new(Cell::new(0)));
    m.run(10, move |v| Ok(transfer.commit(v).unwrap().reply));
    let debt = m
        .host
        .enroll(
            &m.kernel,
            m.spec(3, 3),
            Debt {
                quantity: 1,
                consumed: 0,
                terms: 7,
                creditor: 9,
                liability: 11,
            },
            Backend::Native(Rc::new(DebtDomain)),
        )
        .unwrap();
    let reached = Rc::new(Cell::new(0));
    let omitted = m.batch(
        vec![m.prepare(2, i.clone(), 2).erase()],
        vec![i.id()],
        vec![i.id()],
        Rc::new(Retirement {
            item: i.id(),
            debt: debt.id(),
            reached: reached.clone(),
        }),
        vec![],
    );
    let prefix = m.host.history().unwrap();
    let retry = omitted.clone();
    m.run(10, move |v| {
        assert_eq!(retry.commit(v), Err(Error::Invariant));
        Ok(vec![])
    });
    assert_eq!(reached.get(), 1);
    assert_eq!(m.host.history().unwrap(), prefix);
    assert_eq!(i.inspect().unwrap(), [1, 1]);
    assert_eq!(debt.inspect().unwrap(), [1, 0, 7, 9, 11]);
    assert_eq!(m.host.pending().unwrap(), 1);
    omitted.discard().unwrap();
    omitted.discard().unwrap();
    let retired = m.batch(
        vec![
            m.prepare(2, i.clone(), 2).erase(),
            m.prepare(3, debt.clone(), 0).erase(),
        ],
        vec![i.id(), debt.id()],
        vec![debt.id(), i.id()],
        Rc::new(Retirement {
            item: i.id(),
            debt: debt.id(),
            reached: reached.clone(),
        }),
        vec![],
    );
    let b = retired.clone();
    m.run(10, move |v| {
        let r = b.commit(v).unwrap();
        assert_eq!(r.events, vec![b"debt-retired".to_vec(), vec![b'i', 2]]);
        Ok(r.reply)
    });
    assert_eq!(reached.get(), 2);
    assert_eq!(m.host.history().unwrap()[0], prefix[0]);
    assert_eq!(m.host.history().unwrap().len(), 2);
    assert_eq!(i.inspect().unwrap(), [1, 0]);
    assert_eq!(debt.inspect().unwrap(), [0, 1, 7, 9, 11]);
}
impl Multi {
    fn new() -> Self {
        let host = Host::new(Limits::default()).unwrap();
        let mut kernel = Kernel::new(KernelLimits::default()).unwrap();
        let helper = kernel
            .register_guest(
                Manifest {
                    id: "algorithm".into(),
                    exports: BTreeMap::from([("compute".into(), endpoint(Access::Read))]),
                    dependencies: vec![],
                    extension_points: BTreeSet::from(["compute".into()]),
                    extension: None,
                },
                &helper_wasm(ADD_OBSERVATION),
            )
            .unwrap();
        let action: Rc<RefCell<Option<Action>>> = Rc::new(RefCell::new(None));
        let mut modules = vec![];
        for name in NAMES.into_iter().chain(["coordinator"]) {
            let exports = if name == "coordinator" {
                vec![("commit", Access::Write)]
            } else {
                vec![("query", Access::Read), ("prepare", Access::Write)]
            };
            let ports = exports
                .iter()
                .map(|(ep, _)| {
                    let a = action.clone();
                    (
                        (*ep).into(),
                        Rc::new(move |_: &[u8], view: &CallerView| {
                            a.borrow_mut().as_mut().unwrap()(view)
                        }) as NativePort,
                    )
                })
                .collect();
            let module = kernel
                .register_native(
                    Manifest {
                        id: name.into(),
                        exports: exports
                            .into_iter()
                            .map(|(ep, a)| (ep.into(), endpoint(a)))
                            .collect(),
                        dependencies: vec![KernelDependency {
                            capability: named_cap("algorithm", "compute"),
                            access: Access::Read,
                        }],
                        extension_points: BTreeSet::new(),
                        extension: None,
                    },
                    name.as_bytes(),
                    ports,
                )
                .unwrap();
            kernel
                .issue_grant(
                    module,
                    0,
                    Authority {
                        access: Access::Read,
                        scope: Scope::Keys(BTreeSet::new()),
                    },
                )
                .unwrap();
            modules.push(module);
        }
        let dependencies = NAMES
            .into_iter()
            .flat_map(|p| {
                [("query", Access::Read), ("prepare", Access::Write)].map(|(ep, a)| {
                    KernelDependency {
                        capability: named_cap(p, ep),
                        access: a,
                    }
                })
            })
            .chain([KernelDependency {
                capability: named_cap("coordinator", "commit"),
                access: Access::Write,
            }])
            .collect();
        let bytes = wat::parse_str(r#"(module
          (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
          (memory (export "memory") 1)
          (func (export "alloc") (param i32) (result i32) i32.const 0)
          (func (export "run") (param $p i32) (param i32) (result i64) (local $n i32)
            local.get $p i32.load8_u i32.const 0 i32.const 0 i32.const 2048 i32.const 1024 call $call local.tee $n
            i32.const 0 i32.lt_s if unreachable end
            i64.const 8796093022208 local.get $n i64.extend_i32_u i64.or)
          (func (export "workflow") (param i32) (param i32) (result i64) (local $n i32)
            i32.const 1 i32.const 0 i32.const 0 i32.const 2048 i32.const 1024 call $call i32.const 0 i32.lt_s if unreachable end
            i32.const 3 i32.const 0 i32.const 0 i32.const 2048 i32.const 1024 call $call i32.const 0 i32.lt_s if unreachable end
            i32.const 5 i32.const 0 i32.const 0 i32.const 2048 i32.const 1024 call $call i32.const 0 i32.lt_s if unreachable end
            i32.const 10 i32.const 0 i32.const 0 i32.const 2048 i32.const 1024 call $call local.tee $n i32.const 0 i32.lt_s if unreachable end
            i64.const 8796093022208 local.get $n i64.extend_i32_u i64.or))"#).unwrap();
        let root = kernel
            .register_guest(
                Manifest {
                    id: "multi-caller".into(),
                    exports: BTreeMap::from([
                        ("run".into(), endpoint(Access::Write)),
                        ("workflow".into(), endpoint(Access::Write)),
                    ]),
                    dependencies,
                    extension_points: BTreeSet::new(),
                    extension: None,
                },
                &bytes,
            )
            .unwrap();
        let mut keys = BTreeSet::from([host.publication_resource()]);
        for serial in 1..=16 {
            for port in ["query", "prepare"] {
                keys.insert(Resource {
                    owner: format!("owners-{}", host.0.id),
                    key: format!("{serial}.{port}"),
                });
            }
        }
        for slot in 0..11 {
            kernel
                .issue_grant(
                    root,
                    slot,
                    Authority {
                        access: if slot % 2 == 0 && slot != 10 {
                            Access::Read
                        } else {
                            Access::Write
                        },
                        scope: Scope::Keys(keys.clone()),
                    },
                )
                .unwrap();
        }
        let lease = kernel
            .capture_pipeline(
                root,
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
                "transfer",
                &[host.cleanup_identity().unwrap()],
            )
            .unwrap();
        Self {
            kernel,
            host,
            root,
            modules,
            helper,
            action,
            lease,
            operation,
        }
    }
    fn spec(&self, module: usize, schema: u8) -> OwnerSpec {
        OwnerSpec {
            module: self.modules[module],
            schema: [schema; 32],
            ports: BTreeMap::from([
                (
                    "query".into(),
                    Port {
                        access: Access::Read,
                        major: 1,
                        dependencies: vec![],
                    },
                ),
                (
                    "prepare".into(),
                    Port {
                        access: Access::Write,
                        major: 1,
                        dependencies: vec![],
                    },
                ),
            ]),
        }
    }
    fn run_result(
        &self,
        slot: u8,
        action: impl FnMut(&CallerView<'_>) -> nethacked_kernel::Result<Vec<u8>> + 'static,
    ) -> nethacked_kernel::Result<Vec<u8>> {
        *self.action.borrow_mut() = Some(Box::new(action));
        let result = self.kernel.invoke_operation(
            &self.operation,
            &[slot],
            Authority {
                access: Access::Write,
                scope: Scope::All,
            },
        );
        self.action.borrow_mut().take();
        result
    }
    fn run(
        &self,
        slot: u8,
        action: impl FnMut(&CallerView<'_>) -> nethacked_kernel::Result<Vec<u8>> + 'static,
    ) {
        self.run_result(slot, action).unwrap();
    }
    fn prepare<T: 'static>(&self, index: u8, owner: Rc<Owner<T>>, request: u8) -> Candidate<T> {
        let result = Rc::new(RefCell::new(None));
        let sink = result.clone();
        self.run(index * 2 + 1, move |view| {
            *sink.borrow_mut() = Some(owner.prepare("prepare", &[request], vec![], view).unwrap());
            Ok(vec![])
        });
        let candidate = result.borrow_mut().take().unwrap();
        candidate
    }
    fn batch(
        &self,
        candidates: Vec<ErasedCandidate>,
        writes: Vec<OwnerId>,
        order: Vec<OwnerId>,
        accounting: Rc<dyn Accounting>,
        readonly: Vec<Observation>,
    ) -> Rc<Batch> {
        let result = Rc::new(RefCell::new(None));
        let sink = result.clone();
        let host = self.host.clone();
        let mut candidates = Some(candidates);
        self.run(10, move |view| {
            *sink.borrow_mut() = Some(Rc::new(
                host.batch(
                    candidates.take().unwrap(),
                    readonly.clone(),
                    &writes,
                    order.clone(),
                    [42; 32],
                    accounting.clone(),
                    named_cap("coordinator", "commit"),
                    view,
                )
                .unwrap(),
            ));
            Ok(vec![])
        });
        let batch = result.borrow_mut().take().unwrap();
        batch
    }
}
