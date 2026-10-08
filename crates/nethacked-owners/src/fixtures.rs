use super::*;
use nethacked_kernel::{
    Authority, Dependency as KernelDependency, Endpoint, Limits as KernelLimits, Manifest,
    NativePort, Scope,
};

mod publication_tests;

type Action = Box<dyn FnMut(&CallerView<'_>) -> nethacked_kernel::Result<Vec<u8>>>;
struct Harness {
    kernel: Kernel,
    module: ModuleHandle,
    root: ModuleHandle,
    host: Host,
    action: Rc<RefCell<Option<Action>>>,
    lease: nethacked_kernel::PipelineLease,
    operation: OperationBinding,
    keys: BTreeSet<Resource>,
    helper: ModuleHandle,
}
fn cap(name: &str) -> Capability {
    Capability {
        provider: "owners".into(),
        endpoint: name.into(),
        major: 1,
    }
}
fn endpoint(access: Access) -> Endpoint {
    Endpoint {
        major: 1,
        access,
        max_request: 1024,
        max_reply: 1024,
    }
}
fn helper_wasm(body: &str) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(module (memory (export "memory") 1)
        (func (export "alloc") (param i32) (result i32) i32.const 0)
        (func (export "compute") (param $p i32) (param i32) (result i64) {body}))"#
    ))
    .unwrap()
}
impl Harness {
    fn new(limits: Limits, kernel_limits: KernelLimits, body: &str) -> Self {
        let host = Host::new(limits).unwrap();
        let mut kernel = Kernel::new(kernel_limits).unwrap();
        let helper = kernel
            .register_guest(
                Manifest {
                    id: "algorithm".into(),
                    exports: BTreeMap::from([("compute".into(), endpoint(Access::Read))]),
                    dependencies: vec![],
                    extension_points: BTreeSet::new(),
                    extension: None,
                },
                &helper_wasm(body),
            )
            .unwrap();
        let action: Rc<RefCell<Option<Action>>> = Rc::new(RefCell::new(None));
        let ports: BTreeMap<_, _> = ["query", "prepare", "derived"]
            .into_iter()
            .map(|name| {
                let a = action.clone();
                (
                    name.into(),
                    Rc::new(move |_: &[u8], view: &CallerView| {
                        a.borrow_mut().as_mut().unwrap()(view)
                    }) as NativePort,
                )
            })
            .collect();
        let module = kernel
            .register_native(
                Manifest {
                    id: "owners".into(),
                    exports: BTreeMap::from([
                        ("query".into(), endpoint(Access::Read)),
                        ("prepare".into(), endpoint(Access::Write)),
                        ("derived".into(), endpoint(Access::Read)),
                    ]),
                    dependencies: vec![KernelDependency {
                        capability: Capability {
                            provider: "algorithm".into(),
                            endpoint: "compute".into(),
                            major: 1,
                        },
                        access: Access::Read,
                    }],
                    extension_points: BTreeSet::new(),
                    extension: None,
                },
                b"trusted owner adapters",
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
        let bytes = wat::parse_str(r#"(module
            (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (func (export "alloc") (param i32) (result i32) i32.const 0)
            (func (export "run") (param $p i32) (param i32) (result i64) (local $n i32)
                local.get $p i32.load8_u i32.const 0 i32.const 0 i32.const 2048 i32.const 1024 call $call local.tee $n
                i32.const 0 i32.lt_s if unreachable end
                i64.const 8796093022208 local.get $n i64.extend_i32_u i64.or))"#).unwrap();
        let root = kernel
            .register_guest(
                Manifest {
                    id: "caller".into(),
                    exports: BTreeMap::from([("run".into(), endpoint(Access::Write))]),
                    dependencies: vec![
                        KernelDependency {
                            capability: cap("query"),
                            access: Access::Read,
                        },
                        KernelDependency {
                            capability: cap("prepare"),
                            access: Access::Write,
                        },
                        KernelDependency {
                            capability: cap("derived"),
                            access: Access::Read,
                        },
                    ],
                    extension_points: BTreeSet::new(),
                    extension: None,
                },
                &bytes,
            )
            .unwrap();
        // Install empty grants before capture; refresh the lease after enrollment.
        for slot in 0..3 {
            kernel
                .issue_grant(
                    root,
                    slot,
                    Authority {
                        access: if slot == 1 {
                            Access::Write
                        } else {
                            Access::Read
                        },
                        scope: Scope::Keys(BTreeSet::new()),
                    },
                )
                .unwrap();
        }
        let ceiling = Authority {
            access: Access::Write,
            scope: Scope::All,
        };
        let lease = kernel.capture_pipeline(root, "run", ceiling).unwrap();
        let id = kernel.issue_operation().unwrap();
        let operation = kernel
            .operation_binding_with_cleanup(
                id,
                lease,
                1,
                "prepare",
                &[host.cleanup_identity().unwrap()],
            )
            .unwrap();
        Self {
            kernel,
            module,
            root,
            host,
            action,
            lease,
            operation,
            keys: BTreeSet::new(),
            helper,
        }
    }
    fn spec(&self, schema: u8, dependencies: Vec<Dependency>) -> OwnerSpec {
        OwnerSpec {
            module: self.module,
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
                        dependencies: dependencies.clone(),
                    },
                ),
                (
                    "derived".into(),
                    Port {
                        access: Access::Read,
                        major: 1,
                        dependencies,
                    },
                ),
            ]),
        }
    }
    fn admit<T: 'static>(&mut self, owner: &Owner<T>) {
        for name in ["query", "prepare", "derived"] {
            self.keys.insert(owner.resource(name).unwrap());
        }
    }
    fn refresh(&mut self) {
        for slot in 0..3 {
            self.kernel
                .issue_grant(
                    self.root,
                    slot,
                    Authority {
                        access: if slot == 1 {
                            Access::Write
                        } else {
                            Access::Read
                        },
                        scope: Scope::Keys(self.keys.clone()),
                    },
                )
                .unwrap();
        }
        self.kernel.release_pipeline(self.lease).unwrap();
        self.lease = self
            .kernel
            .capture_pipeline(
                self.root,
                "run",
                Authority {
                    access: Access::Write,
                    scope: Scope::All,
                },
            )
            .unwrap();
        let op = self.kernel.issue_operation().unwrap();
        self.operation = self
            .kernel
            .operation_binding_with_cleanup(
                op,
                self.lease,
                1,
                "prepare",
                &[self.host.cleanup_identity().unwrap()],
            )
            .unwrap();
    }
    fn run(
        &self,
        slot: u8,
        action: impl FnMut(&CallerView<'_>) -> nethacked_kernel::Result<Vec<u8>> + 'static,
    ) {
        self.run_result(slot, action).unwrap();
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
    fn binding(&self) -> CapabilityPermit {
        CapabilityPermit {
            capability: Capability {
                provider: "algorithm".into(),
                endpoint: "compute".into(),
                major: 1,
            },
            resolution: vec![ProviderBinding {
                handle: self.helper,
                artifact: self.kernel.artifact(self.helper).unwrap(),
                endpoint: "compute".into(),
            }],
        }
    }
}
#[derive(Debug, Eq, PartialEq)]
struct Inventory {
    amount: u64,
    capacity: u64,
}
struct InventoryDomain;
impl Schema<Inventory> for InventoryDomain {
    fn validate(&self, s: &Inventory) -> Result<()> {
        if s.amount <= s.capacity {
            Ok(())
        } else {
            Err(Error::Invariant)
        }
    }
    fn encode(&self, s: &Inventory) -> Result<Vec<u8>> {
        Ok([s.amount.to_le_bytes(), s.capacity.to_le_bytes()].concat())
    }
    fn query(&self, s: &Inventory, _: &str, _: &[u8], _: &[Observation]) -> Result<Vec<u8>> {
        Ok(s.amount.to_le_bytes().to_vec())
    }
}
impl NativeBehavior<Inventory> for InventoryDomain {
    fn prepare(
        &self,
        s: &Inventory,
        _: &str,
        req: &[u8],
        _: &[Observation],
    ) -> Result<Proposal<Inventory>> {
        let delta = u64::from(*req.first().ok_or(Error::Invalid)?);
        Ok(Proposal {
            next: Inventory {
                amount: s.amount.checked_add(delta).ok_or(Error::Limit)?,
                capacity: s.capacity,
            },
            events: vec![vec![b'i', delta as u8]],
            reply: vec![delta as u8],
        })
    }
}
#[derive(Debug, Eq, PartialEq)]
struct Ledger {
    debt: u64,
}
struct LedgerDomain;
impl Schema<Ledger> for LedgerDomain {
    fn validate(&self, _: &Ledger) -> Result<()> {
        Ok(())
    }
    fn encode(&self, s: &Ledger) -> Result<Vec<u8>> {
        Ok(s.debt.to_le_bytes().to_vec())
    }
    fn query(&self, s: &Ledger, _: &str, _: &[u8], obs: &[Observation]) -> Result<Vec<u8>> {
        let observed = obs
            .first()
            .map(|o| u64::from_le_bytes(o.bytes().try_into().unwrap()))
            .unwrap_or(0);
        Ok(s.debt
            .checked_add(observed)
            .ok_or(Error::Limit)?
            .to_le_bytes()
            .to_vec())
    }
}
impl NativeBehavior<Ledger> for LedgerDomain {
    fn prepare(
        &self,
        s: &Ledger,
        _: &str,
        req: &[u8],
        obs: &[Observation],
    ) -> Result<Proposal<Ledger>> {
        let amount = u64::from_le_bytes(
            obs.first()
                .ok_or(Error::Invalid)?
                .bytes()
                .try_into()
                .map_err(|_| Error::Invalid)?,
        );
        let multiplier = u64::from(*req.first().ok_or(Error::Invalid)?);
        Ok(Proposal {
            next: Ledger {
                debt: s
                    .debt
                    .checked_add(amount.checked_mul(multiplier).ok_or(Error::Limit)?)
                    .ok_or(Error::Limit)?,
            },
            events: vec![b"charge".to_vec()],
            reply: vec![],
        })
    }
}
struct BlobCodec;
impl Schema<Vec<u8>> for BlobCodec {
    fn validate(&self, s: &Vec<u8>) -> Result<()> {
        if s.len() == 4 && u32::from_le_bytes(s.as_slice().try_into().unwrap()) <= 100 {
            Ok(())
        } else {
            Err(Error::Invariant)
        }
    }
    fn encode(&self, s: &Vec<u8>) -> Result<Vec<u8>> {
        Ok(s.clone())
    }
    fn query(&self, s: &Vec<u8>, _: &str, _: &[u8], _: &[Observation]) -> Result<Vec<u8>> {
        Ok(s.clone())
    }
}
impl WasmCodec<Vec<u8>> for BlobCodec {
    fn input(&self, s: &Vec<u8>, _: &str, req: &[u8], obs: &[Observation]) -> Result<Vec<u8>> {
        let amount = u64::from_le_bytes(
            obs.first()
                .ok_or(Error::Invalid)?
                .bytes()
                .try_into()
                .map_err(|_| Error::Invalid)?,
        );
        Ok([s.as_slice(), &amount.to_le_bytes()[..4], req].concat())
    }
    fn decode(&self, bytes: &[u8]) -> Result<Proposal<Vec<u8>>> {
        if bytes.len() != 4 {
            return Err(Error::Invalid);
        }
        Ok(Proposal {
            next: bytes.to_vec(),
            events: vec![b"blob-increment".to_vec()],
            reply: bytes.to_vec(),
        })
    }
}
const ADD_OBSERVATION: &str = "i32.const 0 local.get $p i32.load local.get $p i32.const 4 i32.add i32.load i32.add local.get $p i32.const 8 i32.add i32.load8_u i32.mul i32.store i64.const 4";
fn inventory(h: &mut Harness) -> Rc<Owner<Inventory>> {
    let owner = h
        .host
        .enroll(
            &h.kernel,
            h.spec(1, vec![]),
            Inventory {
                amount: 4,
                capacity: 20,
            },
            Backend::Native(Rc::new(InventoryDomain)),
        )
        .unwrap();
    h.admit(&owner);
    owner
}
fn dep<T: 'static>(owner: &Owner<T>) -> Dependency {
    Dependency {
        owner: owner.id(),
        query: "query".into(),
        schema: owner.stamp().unwrap().schema,
    }
}
fn observe(h: &Harness, owner: Rc<Owner<Inventory>>) -> Observation {
    let result = Rc::new(RefCell::new(None));
    let sink = result.clone();
    h.run(0, move |v| {
        *sink.borrow_mut() = Some(owner.query("query", &[], &[], v).unwrap());
        Ok(vec![])
    });
    let observation = result.borrow_mut().take().unwrap();
    observation
}
#[test]
fn heterogeneous_storage_native_and_actual_wasm_candidates_are_independent() {
    let mut h = Harness::new(Limits::default(), KernelLimits::default(), ADD_OBSERVATION);
    let inv = inventory(&mut h);
    let inv2 = inventory(&mut h); // One behavior/module serves separate instances.
    let ledger = h
        .host
        .enroll(
            &h.kernel,
            h.spec(2, vec![dep(&inv)]),
            Ledger { debt: 3 },
            Backend::Native(Rc::new(LedgerDomain)),
        )
        .unwrap();
    h.admit(&ledger);
    let blob = h
        .host
        .enroll(
            &h.kernel,
            h.spec(3, vec![dep(&inv)]),
            5u32.to_le_bytes().to_vec(),
            Backend::Wasm {
                codec: Rc::new(BlobCodec),
                slot: 0,
                binding: h.binding(),
            },
        )
        .unwrap();
    h.admit(&blob);
    h.refresh();
    let initial = [
        inv.inspect().unwrap(),
        ledger.inspect().unwrap(),
        blob.inspect().unwrap(),
    ];
    assert!(!std::ptr::eq(
        &inv.slot.borrow().state,
        &inv2.slot.borrow().state
    ));
    let source = inv.clone();
    h.run(1, move |v| {
        let candidate = source.prepare("prepare", &[2], vec![], v).unwrap();
        assert_eq!(
            candidate.stage.prepared.borrow().as_ref().unwrap().next,
            Inventory {
                amount: 6,
                capacity: 20
            }
        );
        assert_eq!(
            candidate.stage.prepared.borrow().as_ref().unwrap().events,
            vec![vec![b'i', 2]]
        );
        candidate.validate(v).unwrap();
        assert_eq!(candidate.receipt().operation, *v.operation().unwrap());
        assert_eq!(source.host.retire(source.id()), Err(Error::Referenced));
        candidate.discard();
        assert_eq!(source.host.pending().unwrap(), 0);
        Ok(vec![])
    });
    let obs = Rc::new(RefCell::new(Some(observe(&h, inv.clone()))));
    let target = ledger.clone();
    h.run(1, move |v| {
        let c = target
            .prepare("prepare", &[2], vec![obs.borrow_mut().take().unwrap()], v)
            .unwrap();
        assert_eq!(c.stage.prepared.borrow().as_ref().unwrap().next.debt, 11);
        c.validate(v).unwrap();
        Ok(vec![])
    });
    let obs = Rc::new(RefCell::new(Some(observe(&h, inv.clone()))));
    let target = blob.clone();
    h.run(1, move |v| {
        let c = target
            .prepare("prepare", &[2], vec![obs.borrow_mut().take().unwrap()], v)
            .unwrap();
        assert_eq!(
            c.stage.prepared.borrow().as_ref().unwrap().next,
            18u32.to_le_bytes()
        );
        assert_eq!(
            c.stage.prepared.borrow().as_ref().unwrap().events,
            vec![b"blob-increment".to_vec()]
        );
        assert_eq!(
            c.stage.prepared.borrow().as_ref().unwrap().reply,
            18u32.to_le_bytes()
        );
        assert_eq!(c.stage.prepared.borrow().as_ref().unwrap().code.len(), 3);
        c.validate(v).unwrap();
        Ok(vec![])
    });
    assert_eq!(
        initial,
        [
            inv.inspect().unwrap(),
            ledger.inspect().unwrap(),
            blob.inspect().unwrap()
        ]
    );
    assert_eq!(inv2.inspect().unwrap(), initial[0]);
    assert_eq!(h.host.registry().unwrap().len(), 4);
    assert_eq!(h.host.pending().unwrap(), 0);
}
#[test]
fn query_owner_grant_is_required_even_when_owner_is_absent_from_dependencies() {
    let mut h = Harness::new(Limits::default(), KernelLimits::default(), ADD_OBSERVATION);
    let inv = inventory(&mut h);
    let ledger = h
        .host
        .enroll(
            &h.kernel,
            h.spec(2, vec![dep(&inv)]),
            Ledger { debt: 3 },
            Backend::Native(Rc::new(LedgerDomain)),
        )
        .unwrap();
    h.admit(&ledger);
    h.refresh();
    let obs = observe(&h, inv.clone());
    let input = obs.clone();
    let target = ledger.clone();
    h.run(2, move |v| {
        let result = target
            .query("derived", &[], std::slice::from_ref(&input), v)
            .unwrap();
        assert_eq!(result.bytes(), 7u64.to_le_bytes());
        assert_eq!(result.dependencies.len(), 1);
        assert_eq!(result.stamp.owner, target.id());
        Ok(result.bytes().to_vec())
    });
    // Narrow only the derived/query-owner grant. Its dependency query grant survives.
    h.kernel
        .issue_grant(
            h.root,
            2,
            Authority {
                access: Access::Read,
                scope: Scope::Keys(BTreeSet::from([inv.resource("query").unwrap()])),
            },
        )
        .unwrap();
    let input = obs.clone();
    let target = ledger.clone();
    h.run(2, move |v| {
        assert!(v
            .permit(
                &cap("query"),
                &resource(input.route.metadata(), "query"),
                Access::Read
            )
            .is_some());
        assert!(matches!(
            target.query("derived", &[], std::slice::from_ref(&input), v),
            Err(Error::Authority)
        ));
        Ok(vec![])
    });
    h.kernel.revoke_grant(h.root, 2).unwrap();
    assert_eq!(
        h.kernel.invoke_operation(
            &h.operation,
            &[2],
            Authority {
                access: Access::Write,
                scope: Scope::All
            }
        ),
        Err(nethacked_kernel::Error::Trap)
    );
    h.refresh();
    h.kernel.revoke_grant(h.root, 0).unwrap();
    let input = obs.clone();
    let target = ledger.clone();
    h.run(2, move |v| {
        assert!(matches!(
            target.query("derived", &[], std::slice::from_ref(&input), v),
            Err(Error::Authority)
        ));
        Ok(vec![])
    });
    h.refresh();
    let mut bad = obs.clone();
    bad.stamp.schema = [99; 32];
    let target = ledger.clone();
    h.run(2, move |v| {
        assert!(matches!(
            target.query("derived", &[], std::slice::from_ref(&bad), v),
            Err(Error::Schema)
        ));
        Ok(vec![])
    });
    let gate = h.host.close_visibility().unwrap();
    assert_eq!(ledger.inspect(), Err(Error::Busy));
    let input = obs.clone();
    let target = ledger.clone();
    h.run(2, move |v| {
        assert!(matches!(
            target.query("derived", &[], std::slice::from_ref(&input), v),
            Err(Error::Busy)
        ));
        Ok(vec![])
    });
    drop(gate);
    h.host.retire(ledger.id()).unwrap();
    let target = ledger.clone();
    h.run(2, move |v| {
        assert!(matches!(
            target.query("derived", &[], std::slice::from_ref(&obs), v),
            Err(Error::Stale)
        ));
        Ok(vec![])
    });
    assert_eq!(
        inv.inspect().unwrap(),
        [4u64.to_le_bytes(), 20u64.to_le_bytes()].concat()
    );
}
#[test]
fn stale_foreign_recreated_and_detached_captures_refuse() {
    let mut h = Harness::new(Limits::default(), KernelLimits::default(), ADD_OBSERVATION);
    let inv = inventory(&mut h);
    let ledger = h
        .host
        .enroll(
            &h.kernel,
            h.spec(2, vec![dep(&inv)]),
            Ledger { debt: 3 },
            Backend::Native(Rc::new(LedgerDomain)),
        )
        .unwrap();
    h.admit(&ledger);
    h.refresh();
    let obs = observe(&h, inv.clone());
    let target = inv.clone();
    h.run(1, move |v| {
        let detached = v.clone();
        assert!(matches!(
            target.prepare("prepare", &[1], vec![], &detached),
            Err(Error::Kernel(nethacked_kernel::Error::Authority))
        ));
        let c = target.prepare("prepare", &[1], vec![], v).unwrap();
        assert_eq!(
            c.validate(&detached),
            Err(Error::Kernel(nethacked_kernel::Error::Authority))
        );
        Ok(vec![])
    });
    let foreign = Host::new(Limits::default()).unwrap();
    assert_eq!(foreign.retire(inv.id()), Err(Error::Stale));
    let mut bad = obs.clone();
    bad.stamp.revision += 1;
    let target = ledger.clone();
    h.run(1, move |v| {
        assert!(matches!(
            target.prepare("prepare", &[2], vec![bad.clone()], v),
            Err(Error::Stale)
        ));
        Ok(vec![])
    });
    let mut bad = obs.clone();
    bad.caller_artifact = [88; 32];
    let target = ledger.clone();
    h.run(1, move |v| {
        assert!(matches!(
            target.prepare("prepare", &[2], vec![bad.clone()], v),
            Err(Error::Authority)
        ));
        Ok(vec![])
    });
    h.host.retire(inv.id()).unwrap();
    let recreated = inventory(&mut h);
    assert_ne!(inv.id(), recreated.id());
    let target = ledger.clone();
    h.run(1, move |v| {
        assert!(matches!(
            target.prepare("prepare", &[2], vec![obs.clone()], v),
            Err(Error::Stale)
        ));
        Ok(vec![])
    });
    assert_eq!(ledger.inspect().unwrap(), 3u64.to_le_bytes());
    assert_eq!(h.host.pending().unwrap(), 0);
}
#[test]
fn invariant_malformed_oversized_trapped_and_fuel_refused_guest_candidates_preserve_state() {
    for (body, expected, exhausted_fuel) in [
        (
            "i32.const 0 i32.const 101 i32.store i64.const 4",
            Error::Invariant,
            false,
        ),
        ("i64.const 3", Error::Invalid, false),
        (
            "i64.const 1025",
            Error::Kernel(nethacked_kernel::Error::Buffer),
            false,
        ),
        (
            "unreachable",
            Error::Kernel(nethacked_kernel::Error::Trap),
            false,
        ),
        (
            "(loop $forever br $forever) i64.const 4",
            Error::Kernel(nethacked_kernel::Error::Trap),
            true,
        ),
    ] {
        let mut h = Harness::new(Limits::default(), KernelLimits::default(), body);
        let inv = inventory(&mut h);
        let blob = h
            .host
            .enroll(
                &h.kernel,
                h.spec(3, vec![dep(&inv)]),
                5u32.to_le_bytes().to_vec(),
                Backend::Wasm {
                    codec: Rc::new(BlobCodec),
                    slot: 0,
                    binding: h.binding(),
                },
            )
            .unwrap();
        h.admit(&blob);
        h.refresh();
        let obs = observe(&h, inv.clone());
        let target = blob.clone();
        let serial = h.host.0.registry.borrow().next_candidate;
        let checked = Rc::new(Cell::new(0));
        let marker = checked.clone();
        let result = h.run_result(1, move |v| {
            assert!(matches!(
                target.prepare("prepare", &[2], vec![obs.clone()], v),
                Err(e) if e == expected
            ));
            marker.set(marker.get() + 1);
            Ok(vec![])
        });
        assert_eq!(
            checked.get(),
            1,
            "expected preparation refusal was not checked"
        );
        assert!(result.is_ok() || (exhausted_fuel && result == Err(nethacked_kernel::Error::Trap)));
        assert_eq!(blob.inspect().unwrap(), 5u32.to_le_bytes());
        assert_eq!(inv.slot.borrow().state.amount, 4);
        assert_eq!(h.host.pending().unwrap(), 0);
        assert_eq!(h.host.0.registry.borrow().next_candidate, serial);
    }
}
#[test]
fn bounded_candidates_identity_counters_and_code_lifetimes() {
    let mut h = Harness::new(
        Limits {
            owners: 2,
            candidates: 1,
            ..Limits::default()
        },
        KernelLimits::default(),
        ADD_OBSERVATION,
    );
    let inv = inventory(&mut h);
    h.refresh();
    assert!(matches!(
        h.host
            .enroll(&h.kernel, h.spec(4, vec![]), 0u64, Backend::CommitCallback),
        Err(Error::Unsupported)
    ));
    let holder = Rc::new(RefCell::new(None));
    let sink = holder.clone();
    let target = inv.clone();
    h.run(1, move |v| {
        let c = target.prepare("prepare", &[2], vec![], v).unwrap();
        assert!(matches!(
            target.prepare("prepare", &[2], vec![], v),
            Err(Error::Limit)
        ));
        *sink.borrow_mut() = Some(c);
        Ok(vec![])
    });
    assert_eq!(h.host.pending().unwrap(), 1);
    assert_eq!(h.host.retire(inv.id()), Err(Error::Referenced));
    h.kernel.release_pipeline(h.lease).unwrap();
    assert_eq!(
        h.kernel.unregister(h.module),
        Err(nethacked_kernel::Error::Referenced)
    );
    assert_eq!(
        h.kernel.unregister(h.root),
        Err(nethacked_kernel::Error::Referenced)
    );
    let id = inv.id();
    drop(inv);
    assert!(h.host.route(id).is_ok());
    holder.borrow_mut().take().unwrap().discard();
    assert!(h.host.route(id).is_err());
    assert_eq!(h.host.pending().unwrap(), 0);
    h.kernel.unregister(h.root).unwrap();
    h.kernel.unregister(h.module).unwrap();
    h.kernel.unregister(h.helper).unwrap();
    // Tombstones count toward enrollment capacity; no silent eviction.
    let mut h = Harness::new(
        Limits {
            owners: 1,
            ..Limits::default()
        },
        KernelLimits::default(),
        ADD_OBSERVATION,
    );
    let inv = inventory(&mut h);
    h.host.retire(inv.id()).unwrap();
    drop(inv);
    assert!(matches!(
        h.host.enroll(
            &h.kernel,
            h.spec(1, vec![]),
            Inventory {
                amount: 0,
                capacity: 1
            },
            Backend::Native(Rc::new(InventoryDomain))
        ),
        Err(Error::Limit)
    ));
    assert_eq!(h.host.registry().unwrap().len(), 1);
    h.host.0.registry.borrow_mut().next_owner = u64::MAX;
    assert!(matches!(
        h.host.enroll(
            &h.kernel,
            h.spec(1, vec![]),
            Inventory {
                amount: 0,
                capacity: 1
            },
            Backend::Native(Rc::new(InventoryDomain))
        ),
        Err(Error::Limit)
    ));
}
struct StressDomain {
    mode: u8,
    calls: Rc<Cell<usize>>,
}
impl Schema<Inventory> for StressDomain {
    fn validate(&self, s: &Inventory) -> Result<()> {
        InventoryDomain.validate(s)
    }
    fn encode(&self, s: &Inventory) -> Result<Vec<u8>> {
        if self.mode == 3 && s.amount > 4 {
            Ok(vec![0; 17])
        } else {
            InventoryDomain.encode(s)
        }
    }
    fn query(&self, s: &Inventory, n: &str, r: &[u8], o: &[Observation]) -> Result<Vec<u8>> {
        InventoryDomain.query(s, n, r, o)
    }
}
impl NativeBehavior<Inventory> for StressDomain {
    fn prepare(
        &self,
        s: &Inventory,
        n: &str,
        r: &[u8],
        o: &[Observation],
    ) -> Result<Proposal<Inventory>> {
        self.calls.set(self.calls.get() + 1);
        let mut p = InventoryDomain.prepare(s, n, r, o)?;
        match self.mode {
            0 => p.events.push(vec![0]),
            2 => p.reply.push(0),
            _ => {}
        }
        Ok(p)
    }
}
#[test]
fn native_refusal_all_staging_bounds_and_counter_exhaustion_preserve_slots() {
    for mode in 0..4 {
        let limits = match mode {
            0 => Limits {
                events: 1,
                ..Limits::default()
            },
            1 => Limits {
                event_bytes: 1,
                ..Limits::default()
            },
            2 => Limits {
                reply_bytes: 1,
                ..Limits::default()
            },
            _ => Limits {
                payload_bytes: 16,
                ..Limits::default()
            },
        };
        let mut h = Harness::new(limits, KernelLimits::default(), ADD_OBSERVATION);
        let calls = Rc::new(Cell::new(0));
        let inv = h
            .host
            .enroll(
                &h.kernel,
                h.spec(1, vec![]),
                Inventory {
                    amount: 4,
                    capacity: 20,
                },
                Backend::Native(Rc::new(StressDomain {
                    mode,
                    calls: calls.clone(),
                })),
            )
            .unwrap();
        h.admit(&inv);
        h.refresh();
        let serial = h.host.0.registry.borrow().next_candidate;
        let target = inv.clone();
        h.run(1, move |v| {
            assert!(matches!(
                target.prepare("prepare", &[1], vec![], v),
                Err(Error::Limit)
            ));
            Ok(vec![])
        });
        assert_eq!(calls.get(), 1);
        assert_eq!(inv.slot.borrow().state.amount, 4);
        assert_eq!(h.host.pending().unwrap(), 0);
        assert_eq!(h.host.0.registry.borrow().next_candidate, serial);
    }
    let mut h = Harness::new(Limits::default(), KernelLimits::default(), ADD_OBSERVATION);
    let inv = inventory(&mut h);
    h.refresh();
    let target = inv.clone();
    h.run(1, move |v| {
        assert!(matches!(
            target.prepare("prepare", &[21], vec![], v),
            Err(Error::Invariant)
        ));
        assert!(matches!(
            target.prepare("prepare", &vec![0; 16_385], vec![], v),
            Err(Error::Limit)
        ));
        Ok(vec![])
    });
    h.host.0.registry.borrow_mut().next_candidate = u64::MAX;
    let target = inv.clone();
    h.run(1, move |v| {
        assert!(matches!(
            target.prepare("prepare", &[1], vec![], v),
            Err(Error::Limit)
        ));
        Ok(vec![])
    });
    assert_eq!(h.host.pending().unwrap(), 0);
    assert_eq!(inv.slot.borrow().state.amount, 4);
    let h = Harness::new(Limits::default(), KernelLimits::default(), ADD_OBSERVATION);
    h.host.0.registry.borrow_mut().next_owner = u64::MAX;
    assert!(matches!(
        h.host.enroll(
            &h.kernel,
            h.spec(1, vec![]),
            Inventory {
                amount: 0,
                capacity: 1
            },
            Backend::Native(Rc::new(InventoryDomain))
        ),
        Err(Error::Limit)
    ));
}
#[test]
fn observation_closure_bounds_and_transitive_dependency_revocation_are_checked() {
    let mut h = Harness::new(Limits::default(), KernelLimits::default(), ADD_OBSERVATION);
    let inv = inventory(&mut h);
    let ledger = h
        .host
        .enroll(
            &h.kernel,
            h.spec(2, vec![dep(&inv)]),
            Ledger { debt: 3 },
            Backend::Native(Rc::new(LedgerDomain)),
        )
        .unwrap();
    h.admit(&ledger);
    let nested = h
        .host
        .enroll(
            &h.kernel,
            h.spec(
                2,
                vec![Dependency {
                    owner: ledger.id(),
                    query: "derived".into(),
                    schema: [2; 32],
                }],
            ),
            Ledger { debt: 1 },
            Backend::Native(Rc::new(LedgerDomain)),
        )
        .unwrap();
    h.admit(&nested);
    h.refresh();
    let obs = observe(&h, inv.clone());
    let result = Rc::new(RefCell::new(None));
    let sink = result.clone();
    let source = ledger.clone();
    h.run(2, move |v| {
        *sink.borrow_mut() = Some(
            source
                .query("derived", &[], std::slice::from_ref(&obs), v)
                .unwrap(),
        );
        Ok(vec![])
    });
    let derived = result.borrow_mut().take().unwrap();
    let input = derived.clone();
    let target = nested.clone();
    h.run(1, move |v| {
        let c = target
            .prepare("prepare", &[2], vec![input.clone()], v)
            .unwrap();
        assert_eq!(c.stage.prepared.borrow().as_ref().unwrap().next.debt, 15);
        Ok(vec![])
    });
    h.kernel.revoke_grant(h.root, 0).unwrap();
    let target = nested.clone();
    h.run(1, move |v| {
        assert!(matches!(
            target.prepare("prepare", &[2], vec![derived.clone()], v),
            Err(Error::Authority)
        ));
        Ok(vec![])
    });
    let mut h = Harness::new(
        Limits {
            observation_bytes: 4,
            ..Limits::default()
        },
        KernelLimits::default(),
        ADD_OBSERVATION,
    );
    let inv = inventory(&mut h);
    h.refresh();
    h.run(0, move |v| {
        assert!(matches!(inv.query("query", &[], &[], v), Err(Error::Limit)));
        Ok(vec![])
    });
}
#[test]
fn candidate_revalidation_binds_operation_phase_lease_and_live_grants() {
    let mut h = Harness::new(Limits::default(), KernelLimits::default(), ADD_OBSERVATION);
    let inv = inventory(&mut h);
    h.refresh();
    let holder = Rc::new(RefCell::new(None));
    let sink = holder.clone();
    let target = inv.clone();
    h.run(1, move |v| {
        *sink.borrow_mut() = Some(target.prepare("prepare", &[1], vec![], v).unwrap());
        Ok(vec![])
    });
    let operation = h.kernel.issue_operation().unwrap();
    h.operation = h
        .kernel
        .operation_binding(operation, h.lease, 2, "other-phase")
        .unwrap();
    let staged = holder.clone();
    h.run(1, move |v| {
        assert_eq!(
            staged.borrow().as_ref().unwrap().validate(v),
            Err(Error::Authority)
        );
        Ok(vec![])
    });
    h.kernel
        .issue_grant(
            h.root,
            1,
            Authority {
                access: Access::Write,
                scope: Scope::Keys(BTreeSet::new()),
            },
        )
        .unwrap();
    let staged = holder.clone();
    h.run(1, move |v| {
        assert_eq!(
            staged.borrow().as_ref().unwrap().validate(v),
            Err(Error::Authority)
        );
        Ok(vec![])
    });
    holder.borrow_mut().take().unwrap().discard();
    assert_eq!(h.host.pending().unwrap(), 0);
    assert_eq!(inv.slot.borrow().state.amount, 4);
}
