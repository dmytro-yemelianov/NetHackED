// O2 fixture support reused verbatim; original fixture files are unchanged.
use super::*;
use nethacked_kernel::{
    Authority, Dependency as KernelDependency, Endpoint, Limits as KernelLimits, Manifest,
    NativePort, Scope,
};

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
use nethacked_kernel::Extension;
impl Harness {
    fn live(limits: Limits, kernel_limits: KernelLimits, body: &str) -> Self {
        let host = Host::new(limits).unwrap();
        let mut kernel = Kernel::new(kernel_limits).unwrap();
        let helper = kernel
            .register_guest(
                Manifest {
                    id: "algorithm".into(),
                    exports: BTreeMap::from([("compute".into(), endpoint(Access::Read))]),
                    dependencies: vec![],
                    extension_points: BTreeSet::from(["compute".into()]),
                    extension: None,
                },
                &helper_wasm(body),
            )
            .unwrap();
        Self::with_helper(host, kernel, helper)
    }
    fn with_helper(host: Host, mut kernel: Kernel, helper: ModuleHandle) -> Self {
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
}

impl Harness {
    fn current(
        &self,
        slot: u8,
        action: impl FnMut(&CallerView<'_>) -> nethacked_kernel::Result<Vec<u8>> + 'static,
    ) {
        *self.action.borrow_mut() = Some(Box::new(action));
        let result = self.kernel.invoke(
            self.root,
            "run",
            &[slot],
            Authority {
                access: Access::Write,
                scope: Scope::All,
            },
        );
        self.action.borrow_mut().take();
        result.unwrap();
    }
    fn extension(&mut self, body: &str) -> ModuleHandle {
        self.kernel
            .register_guest(
                Manifest {
                    id: "live-multiply".into(),
                    exports: BTreeMap::from([("compute".into(), endpoint(Access::Read))]),
                    dependencies: vec![],
                    extension_points: BTreeSet::new(),
                    extension: Some(Extension {
                        target: self.binding().capability,
                        export: "compute".into(),
                    }),
                },
                &helper_wasm(body),
            )
            .unwrap()
    }
    fn future_binding(&self, extension: ModuleHandle) -> CapabilityPermit {
        let mut b = self.binding();
        b.resolution.push(ProviderBinding {
            handle: extension,
            artifact: self.kernel.artifact(extension).unwrap(),
            endpoint: "compute".into(),
        });
        b
    }
}
const MULTIPLY: &str = "i32.const 0 local.get $p i32.load local.get $p i32.const 4 i32.add i32.load i32.mul local.get $p i32.const 8 i32.add i32.load8_u i32.mul i32.store i64.const 4";
struct CountCodec(Rc<Cell<usize>>);
impl Schema<Vec<u8>> for CountCodec {
    fn validate(&self, s: &Vec<u8>) -> Result<()> {
        BlobCodec.validate(s)
    }
    fn encode(&self, s: &Vec<u8>) -> Result<Vec<u8>> {
        BlobCodec.encode(s)
    }
    fn query(&self, _: &Vec<u8>, _: &str, _: &[u8], _: &[Observation]) -> Result<Vec<u8>> {
        panic!("live query must execute guest")
    }
}
impl WasmCodec<Vec<u8>> for CountCodec {
    fn input(&self, s: &Vec<u8>, n: &str, r: &[u8], o: &[Observation]) -> Result<Vec<u8>> {
        self.0.set(self.0.get() + 1);
        BlobCodec.input(s, n, r, o)
    }
    fn decode(&self, b: &[u8]) -> Result<Proposal<Vec<u8>>> {
        BlobCodec.decode(b)
    }
    fn decode_query(&self, b: &[u8]) -> Result<Vec<u8>> {
        if b.len() != 4 {
            return Err(Error::Invalid);
        }
        Ok(b.to_vec())
    }
}
struct Accept;
impl Accounting for Accept {
    fn validate(&self, _: &[ProposalView], _: &[Observation]) -> Result<()> {
        Ok(())
    }
}
type Setup = (
    Harness,
    Rc<Owner<Inventory>>,
    Rc<Owner<Vec<u8>>>,
    Rc<Cell<usize>>,
);
fn setup(body: &str) -> Setup {
    let mut h = Harness::live(Limits::default(), KernelLimits::default(), body);
    let item = inventory(&mut h);
    let count = Rc::new(Cell::new(0));
    let codec = Rc::new(CountCodec(count.clone()));
    let owner = h
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
    h.admit(&owner);
    h.keys.insert(h.host.publication_resource());
    h.refresh();
    owner
        .admit_compatible_wasm(&h.kernel, [3; 32], codec, 0, h.binding())
        .unwrap();
    owner
        .set_query_policy(QueryPolicy::Deterministic {
            profile: [1; 32],
            input_policy: [2; 32],
        })
        .unwrap();
    (h, item, owner, count)
}
fn cached(
    h: &Harness,
    item: Rc<Owner<Inventory>>,
    owner: Rc<Owner<Vec<u8>>>,
    cache: Rc<QueryCache>,
    expected: u32,
    hit: bool,
) {
    let obs = observe(h, item);
    h.current(2, move |v| {
        let r = owner
            .query_cached(&cache, "derived", &[1], std::slice::from_ref(&obs), v)
            .unwrap();
        assert_eq!(r.hit, hit);
        assert_eq!(r.observation.bytes(), expected.to_le_bytes());
        Ok(vec![])
    });
}
fn commit_inventory(h: &Harness, owner: Rc<Owner<Inventory>>, amount: u8) {
    h.run(1, move |v| {
        let candidate = owner.prepare("prepare", &[amount], vec![], v).unwrap();
        let id = owner.id();
        owner
            .host
            .batch(
                vec![candidate.erase()],
                vec![],
                &[id],
                vec![id],
                [7; 32],
                Rc::new(Accept),
                owner.capability("prepare").unwrap(),
                v,
            )
            .unwrap()
            .commit(v)
            .unwrap();
        Ok(vec![])
    });
}

#[test]
fn populated_owner_dynamic_algorithms_retain_old_command_and_new_queries() {
    let (mut h, item, owner, count) = setup(ADD_OBSERVATION);
    let unrelated = inventory(&mut h);
    h.refresh();
    let unchanged = unrelated.inspect().unwrap();
    let cache = owner.query_cache().unwrap();
    cached(&h, item.clone(), owner.clone(), cache.clone(), 9, false);
    cached(&h, item.clone(), owner.clone(), cache.clone(), 9, true);
    let saved = Rc::new(RefCell::new(None));
    let sink = saved.clone();
    let target = owner.clone();
    let obs = observe(&h, item.clone());
    h.run(1, move |v| {
        *sink.borrow_mut() = Some(
            target
                .prepare("prepare", &[2], vec![obs.clone()], v)
                .unwrap(),
        );
        Ok(vec![])
    });
    let ext = h.extension(MULTIPLY);
    assert_eq!(
        owner.admit_compatible_wasm(
            &h.kernel,
            [9; 32],
            Rc::new(CountCodec(count.clone())),
            0,
            h.future_binding(ext)
        ),
        Err(Error::Schema)
    );
    owner
        .admit_compatible_wasm(
            &h.kernel,
            [3; 32],
            Rc::new(CountCodec(count.clone())),
            0,
            h.future_binding(ext),
        )
        .unwrap();
    cached(&h, item.clone(), owner.clone(), cache.clone(), 20, false);
    let target = owner.clone();
    let saved2 = saved.clone();
    h.run(1, move |v| {
        let c = saved2.borrow_mut().take().unwrap();
        c.validate(v).unwrap();
        assert_eq!(
            c.stage.prepared.borrow().as_ref().unwrap().next,
            18u32.to_le_bytes()
        );
        let id = target.id();
        target
            .host
            .batch(
                vec![c.erase()],
                vec![],
                &[id],
                vec![id],
                [7; 32],
                Rc::new(Accept),
                target.capability("prepare").unwrap(),
                v,
            )
            .unwrap()
            .commit(v)
            .unwrap();
        Ok(vec![])
    });
    cached(&h, item.clone(), owner.clone(), cache.clone(), 72, false);
    h.refresh();
    let target = owner.clone();
    let obs = observe(&h, item);
    h.run(1, move |v| {
        let c = target
            .prepare("prepare", &[1], vec![obs.clone()], v)
            .unwrap();
        assert_eq!(
            c.stage.prepared.borrow().as_ref().unwrap().next,
            72u32.to_le_bytes()
        );
        c.validate(v).unwrap();
        Ok(vec![])
    });
    assert_eq!(unrelated.inspect().unwrap(), unchanged);
    assert_eq!(owner.inspect().unwrap(), 18u32.to_le_bytes());
}

#[test]
fn cache_dependency_changes_unrelated_hits_context_and_own_authority() {
    let (mut h, item, owner, count) = setup(ADD_OBSERVATION);
    let unrelated = inventory(&mut h);
    h.refresh();
    let cache = owner.query_cache().unwrap();
    assert!(Rc::ptr_eq(&cache, &owner.query_cache().unwrap()));
    cached(&h, item.clone(), owner.clone(), cache.clone(), 9, false);
    let calls = count.get();
    cached(&h, item.clone(), owner.clone(), cache.clone(), 9, true);
    assert_eq!(count.get(), calls);
    commit_inventory(&h, unrelated, 1);
    cached(&h, item.clone(), owner.clone(), cache.clone(), 9, true);
    assert_eq!(count.get(), calls);
    commit_inventory(&h, item.clone(), 2);
    cached(&h, item.clone(), owner.clone(), cache.clone(), 11, false);
    owner
        .set_query_policy(QueryPolicy::Deterministic {
            profile: [8; 32],
            input_policy: [2; 32],
        })
        .unwrap();
    cached(&h, item.clone(), owner.clone(), cache.clone(), 11, false);
    owner
        .set_query_policy(QueryPolicy::Deterministic {
            profile: [8; 32],
            input_policy: [9; 32],
        })
        .unwrap();
    cached(&h, item.clone(), owner.clone(), cache.clone(), 11, false);
    let obs = observe(&h, item.clone());
    h.kernel
        .issue_grant(
            h.root,
            2,
            Authority {
                access: Access::Read,
                scope: Scope::Keys(BTreeSet::from([item.resource("query").unwrap()])),
            },
        )
        .unwrap();
    let target = owner.clone();
    let c = cache.clone();
    h.current(2, move |v| {
        assert!(matches!(
            target.query_cached(&c, "derived", &[1], std::slice::from_ref(&obs), v),
            Err(Error::Authority)
        ));
        Ok(vec![])
    });
    h.kernel.revoke_grant(h.root, 2).unwrap();
    assert!(h
        .kernel
        .invoke(
            h.root,
            "run",
            &[2],
            Authority {
                access: Access::Write,
                scope: Scope::All
            }
        )
        .is_err());
}

#[test]
fn noncacheable_malformed_fuel_oversize_and_capture_bounds_refuse() {
    for body in [
        "unreachable",
        "(loop $spin br $spin) i64.const 4",
        "i64.const 1025",
        "i64.const 3",
    ] {
        let (h, item, owner, _) = setup(body);
        let obs = observe(&h, item);
        let target = owner.clone();
        h.current(2, move |v| {
            assert!(target
                .query("derived", &[1], std::slice::from_ref(&obs), v)
                .is_err());
            assert!(v.clone().capture_pure(0).is_err());
            Ok(vec![])
        });
        assert_eq!(owner.inspect().unwrap(), 5u32.to_le_bytes());
    }
    let (h, item, owner, _) = setup(ADD_OBSERVATION);
    let cache = owner.query_cache().unwrap();
    owner.set_query_policy(QueryPolicy::Noncacheable).unwrap();
    let obs = observe(&h, item);
    let target = owner.clone();
    h.current(2, move |v| {
        assert!(matches!(
            target.query_cached(&cache, "derived", &[1], std::slice::from_ref(&obs), v),
            Err(Error::Unsupported)
        ));
        Ok(vec![])
    });
    owner.metadata.behavior_revision.set(u64::MAX);
    assert_eq!(
        owner.admit_compatible_wasm(&h.kernel, [3; 32], Rc::new(BlobCodec), 0, h.binding()),
        Err(Error::Limit)
    );
}

#[test]
fn transitive_helper_revocation_and_retirement_release_cache_code() {
    let (mut h, item, owner, _) = setup(ADD_OBSERVATION);
    let derived = h
        .host
        .enroll(
            &h.kernel,
            h.spec(
                4,
                vec![Dependency {
                    owner: owner.id(),
                    query: "derived".into(),
                    schema: [3; 32],
                }],
            ),
            Ledger { debt: 1 },
            Backend::Native(Rc::new(LedgerDomain)),
        )
        .unwrap();
    h.admit(&derived);
    h.refresh();
    let obs = observe(&h, item);
    let saved = Rc::new(RefCell::new(None));
    let sink = saved.clone();
    let target = owner.clone();
    h.current(2, move |v| {
        *sink.borrow_mut() = Some(
            target
                .query("derived", &[1], std::slice::from_ref(&obs), v)
                .unwrap(),
        );
        Ok(vec![])
    });
    h.kernel.revoke_grant(h.module, 0).unwrap();
    let target = derived.clone();
    let input = saved.borrow().as_ref().unwrap().clone();
    h.current(2, move |v| {
        assert!(target
            .query("derived", &[], std::slice::from_ref(&input), v)
            .is_err());
        Ok(vec![])
    });
    assert_eq!(owner.inspect().unwrap(), 5u32.to_le_bytes());
    h.host.retire(owner.id()).unwrap();
    drop(owner);
    drop(derived);
    saved.borrow_mut().take();
    h.kernel.release_pipeline(h.lease).unwrap();
    drop(h.action.borrow_mut().take());
    drop(h);
}

struct EquipmentCodec(Rc<Cell<usize>>);
impl Schema<Vec<u8>> for EquipmentCodec {
    fn validate(&self, s: &Vec<u8>) -> Result<()> {
        BlobCodec.validate(s)
    }
    fn encode(&self, s: &Vec<u8>) -> Result<Vec<u8>> {
        BlobCodec.encode(s)
    }
    fn query(&self, _: &Vec<u8>, _: &str, _: &[u8], _: &[Observation]) -> Result<Vec<u8>> {
        Err(Error::Unsupported)
    }
}
impl WasmCodec<Vec<u8>> for EquipmentCodec {
    fn input(&self, s: &Vec<u8>, _: &str, r: &[u8], o: &[Observation]) -> Result<Vec<u8>> {
        self.0.set(self.0.get() + 1);
        let mut sum = 0u32;
        for input in o {
            let value = u64::from_le_bytes(input.bytes().try_into().map_err(|_| Error::Invalid)?);
            sum = sum
                .checked_add(u32::try_from(value).map_err(|_| Error::Limit)?)
                .ok_or(Error::Limit)?;
        }
        Ok([s.as_slice(), &sum.to_le_bytes(), r].concat())
    }
    fn decode(&self, b: &[u8]) -> Result<Proposal<Vec<u8>>> {
        BlobCodec.decode(b)
    }
    fn decode_query(&self, b: &[u8]) -> Result<Vec<u8>> {
        if b.len() != 4 {
            Err(Error::Invalid)
        } else {
            Ok(b.to_vec())
        }
    }
}
#[test]
fn equipment_item_atomic_cache_validation_busy_and_prefix_unchanged() {
    let mut h = Harness::live(Limits::default(), KernelLimits::default(), ADD_OBSERVATION);
    let item = inventory(&mut h);
    let equipment = inventory(&mut h);
    let count = Rc::new(Cell::new(0));
    let codec = Rc::new(EquipmentCodec(count.clone()));
    let owner = h
        .host
        .enroll(
            &h.kernel,
            h.spec(3, vec![dep(&equipment), dep(&item)]),
            5u32.to_le_bytes().to_vec(),
            Backend::Wasm {
                codec: codec.clone(),
                slot: 0,
                binding: h.binding(),
            },
        )
        .unwrap();
    h.admit(&owner);
    h.keys.insert(h.host.publication_resource());
    h.refresh();
    owner
        .admit_compatible_wasm(&h.kernel, [3; 32], codec, 0, h.binding())
        .unwrap();
    owner
        .set_query_policy(QueryPolicy::Deterministic {
            profile: [1; 32],
            input_policy: [2; 32],
        })
        .unwrap();
    let cache = owner.query_cache().unwrap();
    let before = vec![observe(&h, equipment.clone()), observe(&h, item.clone())];
    let target = owner.clone();
    let c = cache.clone();
    let inputs = before.clone();
    h.current(2, move |v| {
        let result = target
            .query_cached(&c, "derived", &[1], &inputs, v)
            .unwrap();
        assert_eq!(result.observation.bytes(), 13u32.to_le_bytes());
        Ok(vec![])
    });
    let target = owner.clone();
    let c = cache.clone();
    let host = h.host.clone();
    let i = item.clone();
    let e = equipment.clone();
    let probes = Rc::new(Cell::new(0));
    let checked = probes.clone();
    h.run(1, move |v| {
        let first = e.prepare("prepare", &[2], vec![], v).unwrap();
        let second = i.prepare("prepare", &[2], vec![], v).unwrap();
        let ids = vec![e.id(), i.id()];
        let batch = host
            .batch(
                vec![first.erase(), second.erase()],
                vec![],
                &ids,
                ids.clone(),
                [7; 32],
                Rc::new(Accept),
                e.capability("prepare").unwrap(),
                v,
            )
            .unwrap();
        let detached = v.clone();
        let target = target.clone();
        let c = c.clone();
        let inputs = before.clone();
        let checked = checked.clone();
        crate::publication::SWAP_PROBE.with(|p| {
            *p.borrow_mut() = Some(Box::new(move || {
                assert!(matches!(
                    target.query_cached(&c, "derived", &[1], &inputs, &detached),
                    Err(Error::Busy)
                ));
                assert_eq!(
                    c.entry.borrow().as_ref().unwrap().observation.bytes(),
                    13u32.to_le_bytes()
                );
                checked.set(checked.get() + 1);
            }));
        });
        batch.commit(v).unwrap();
        crate::publication::SWAP_PROBE.with(|p| p.borrow_mut().take());
        Ok(vec![])
    });
    assert_eq!(probes.get(), 2);
    let inputs = vec![observe(&h, equipment), observe(&h, item)];
    let target = owner;
    let c = cache;
    h.current(2, move |v| {
        let result = target
            .query_cached(&c, "derived", &[1], &inputs, v)
            .unwrap();
        assert!(!result.hit);
        assert_eq!(result.observation.bytes(), 17u32.to_le_bytes());
        assert!(
            target
                .query_cached(&c, "derived", &[1], &inputs, v)
                .unwrap()
                .hit
        );
        Ok(vec![])
    });
    assert_eq!(count.get(), 2);
}

struct Mirror;
impl Schema<u64> for Mirror {
    fn validate(&self, _: &u64) -> Result<()> {
        Ok(())
    }
    fn encode(&self, s: &u64) -> Result<Vec<u8>> {
        Ok(s.to_le_bytes().to_vec())
    }
    fn query(&self, _: &u64, _: &str, _: &[u8], o: &[Observation]) -> Result<Vec<u8>> {
        Ok(o.first().ok_or(Error::Invalid)?.bytes().to_vec())
    }
}
impl NativeBehavior<u64> for Mirror {
    fn prepare(&self, s: &u64, _: &str, _: &[u8], _: &[Observation]) -> Result<Proposal<u64>> {
        Ok(Proposal {
            next: *s,
            events: vec![],
            reply: vec![],
        })
    }
}
#[test]
fn derived_observation_positive_then_transitive_helper_narrowing_refuses() {
    let (mut h, item, owner, _) = setup(ADD_OBSERVATION);
    // Give the helper a real scoped ceiling so narrowing is observable.
    h.kernel
        .issue_grant(
            h.module,
            0,
            Authority {
                access: Access::Read,
                scope: Scope::Keys(h.keys.clone()),
            },
        )
        .unwrap();
    let mirror = h
        .host
        .enroll(
            &h.kernel,
            h.spec(
                8,
                vec![Dependency {
                    owner: owner.id(),
                    query: "derived".into(),
                    schema: [3; 32],
                }],
            ),
            0u64,
            Backend::Native(Rc::new(Mirror)),
        )
        .unwrap();
    h.admit(&mirror);
    h.refresh();
    let input = observe(&h, item);
    let saved = Rc::new(RefCell::new(None));
    let sink = saved.clone();
    let target = owner;
    h.current(2, move |v| {
        *sink.borrow_mut() = Some(
            target
                .query("derived", &[1], std::slice::from_ref(&input), v)
                .unwrap(),
        );
        Ok(vec![])
    });
    let target = mirror.clone();
    let input = saved.borrow().as_ref().unwrap().clone();
    h.current(2, move |v| {
        assert_eq!(
            target
                .query("derived", &[], std::slice::from_ref(&input), v)
                .unwrap()
                .bytes(),
            9u32.to_le_bytes()
        );
        Ok(vec![])
    });
    h.kernel
        .issue_grant(
            h.module,
            0,
            Authority {
                access: Access::Read,
                scope: Scope::Keys(BTreeSet::new()),
            },
        )
        .unwrap();
    let target = mirror;
    let input = saved.borrow().as_ref().unwrap().clone();
    h.current(2, move |v| {
        assert!(target
            .query("derived", &[], std::slice::from_ref(&input), v)
            .is_err());
        Ok(vec![])
    });
}
#[test]
fn detached_cache_stale_vectors_foreign_cache_and_request_binding() {
    let (mut h, item, owner, count) = setup(ADD_OBSERVATION);
    let cache = owner.query_cache().unwrap();
    cached(&h, item.clone(), owner.clone(), cache.clone(), 9, false);
    for tamper in 0..3 {
        {
            let mut entry = cache.entry.borrow_mut();
            let obs = &mut entry.as_mut().unwrap().observation;
            match tamper {
                0 => obs.stamp.generation += 1,
                1 => obs.stamp.schema = [99; 32],
                _ => obs.behavior_revision += 1,
            }
        }
        cached(&h, item.clone(), owner.clone(), cache.clone(), 9, false);
    }
    let input = observe(&h, item.clone());
    let target = owner.clone();
    let c = cache.clone();
    h.current(2, move |v| {
        let r = target
            .query_cached(&c, "derived", &[2], std::slice::from_ref(&input), v)
            .unwrap();
        assert!(!r.hit);
        assert_eq!(r.observation.bytes(), 18u32.to_le_bytes());
        Ok(vec![])
    });
    let other = h
        .host
        .enroll(
            &h.kernel,
            h.spec(3, vec![dep(&item)]),
            5u32.to_le_bytes().to_vec(),
            Backend::Wasm {
                codec: Rc::new(BlobCodec),
                slot: 0,
                binding: h.binding(),
            },
        )
        .unwrap();
    h.admit(&other);
    h.refresh();
    let input = observe(&h, item.clone());
    let c = cache.clone();
    h.current(2, move |v| {
        assert!(matches!(
            other.query_cached(&c, "derived", &[1], std::slice::from_ref(&input), v),
            Err(Error::Stale)
        ));
        Ok(vec![])
    });
    let mut input = observe(&h, item.clone());
    input.stamp.schema = [99; 32];
    let target = owner.clone();
    let c = cache.clone();
    h.current(2, move |v| {
        assert!(matches!(
            target.query_cached(&c, "derived", &[1], std::slice::from_ref(&input), v),
            Err(Error::Schema)
        ));
        Ok(vec![])
    });
    let input = observe(&h, item.clone());
    commit_inventory(&h, item, 1);
    let target = owner.clone();
    let c = cache.clone();
    h.current(2, move |v| {
        assert!(matches!(
            target.query_cached(&c, "derived", &[1], std::slice::from_ref(&input), v),
            Err(Error::Stale)
        ));
        Ok(vec![])
    });
    assert_eq!(count.get(), 5);
    h.host.retire(owner.id()).unwrap();
    let target = owner;
    let c = cache;
    h.current(2, move |v| {
        assert!(matches!(
            target.query_cached(&c, "derived", &[1], &[], v),
            Err(Error::Stale)
        ));
        Ok(vec![])
    });
}
#[test]
fn unload_waits_for_candidate_and_cache_observation_last_reference() {
    let (mut h, item, owner, count) = setup(ADD_OBSERVATION);
    let ext = h.extension(MULTIPLY);
    owner
        .admit_compatible_wasm(
            &h.kernel,
            [3; 32],
            Rc::new(CountCodec(count)),
            0,
            h.future_binding(ext),
        )
        .unwrap();
    h.refresh();
    let cache = owner.query_cache().unwrap();
    let saved = Rc::new(RefCell::new(None));
    let sink = saved.clone();
    let target = owner.clone();
    let c = cache.clone();
    let input = observe(&h, item.clone());
    h.current(2, move |v| {
        *sink.borrow_mut() = Some(
            target
                .query_cached(&c, "derived", &[1], std::slice::from_ref(&input), v)
                .unwrap()
                .observation,
        );
        Ok(vec![])
    });
    let pending = Rc::new(RefCell::new(None));
    let sink = pending.clone();
    let target = owner.clone();
    let input = observe(&h, item);
    h.run(1, move |v| {
        *sink.borrow_mut() = Some(
            target
                .prepare("prepare", &[1], vec![input.clone()], v)
                .unwrap(),
        );
        Ok(vec![])
    });
    assert_eq!(h.host.retire(owner.id()), Err(Error::Referenced));
    h.kernel.managed_detach(ext).unwrap();
    owner
        .admit_compatible_wasm(&h.kernel, [3; 32], Rc::new(BlobCodec), 0, h.binding())
        .unwrap();
    h.refresh();
    assert!(cache.entry.borrow().is_none());
    assert_eq!(
        h.kernel.managed_unload(ext),
        Err(nethacked_kernel::Error::Referenced)
    );
    pending.borrow_mut().take().unwrap().discard();
    assert_eq!(h.host.pending().unwrap(), 0);
    assert_eq!(
        h.kernel.managed_unload(ext),
        Err(nethacked_kernel::Error::Referenced)
    );
    saved.borrow_mut().take();
    h.kernel.managed_unload(ext).unwrap();
    h.host.retire(owner.id()).unwrap();
    drop(owner);
    drop(cache);
    assert_eq!(h.host.pending().unwrap(), 0);
}

#[path = "live_queries/repair.rs"]
mod repair;
