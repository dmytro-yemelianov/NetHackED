#![cfg(feature = "persistence")]
use nethacked_kernel::{
    persistence::*, Access, Authority, CallerView, Capability, CapabilityPermit, Extension, Kernel,
    Manifest, ModuleHandle, NativePort, OperationBinding, ProviderBinding, Resource, Scope,
};
use nethacked_owners::{persistence::*, *};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
#[derive(Clone)]
enum Material {
    Guest(Manifest, Vec<u8>),
    Native(Manifest, Vec<u8>),
}
include!("support/persistence_execution.rs");
const LIMIT: usize = 1_048_576;
fn identity(n: u8) -> CodecIdentity {
    let id = |name: &str| ArtifactIdentity {
        name: name.into(),
        version: 1,
        artifact: [n; 32],
    };
    CodecIdentity {
        schema: id("schema"),
        codec: id("codec"),
        policy: id("policy"),
        schema_digest: [1; 32],
    }
}
fn old_policy() -> QueryPolicy {
    QueryPolicy::Deterministic {
        profile: [1; 32],
        input_policy: [1; 32],
    }
}
fn new_policy() -> QueryPolicy {
    QueryPolicy::Deterministic {
        profile: [2; 32],
        input_policy: [2; 32],
    }
}
fn bytes(n: u32) -> Vec<u8> {
    n.to_le_bytes().to_vec()
}
fn number(b: &[u8]) -> u32 {
    u32::from_le_bytes(b.try_into().unwrap())
}
struct Domain {
    add: u32,
    effects: Rc<Cell<usize>>,
}
impl Schema<Vec<u8>> for Domain {
    fn validate(&self, s: &Vec<u8>) -> Result<()> {
        if s.len() == 4 {
            Ok(())
        } else {
            Err(Error::Invariant)
        }
    }
    fn encode(&self, s: &Vec<u8>) -> Result<Vec<u8>> {
        Ok(s.clone())
    }
    fn query(&self, s: &Vec<u8>, _: &str, _: &[u8], _: &[Observation]) -> Result<Vec<u8>> {
        self.effects.set(self.effects.get() + 1);
        Ok(s.clone())
    }
}
impl WasmCodec<Vec<u8>> for Domain {
    fn input(&self, s: &Vec<u8>, _: &str, _: &[u8], _: &[Observation]) -> Result<Vec<u8>> {
        self.effects.set(self.effects.get() + 1);
        Ok(bytes(number(s) + self.add))
    }
    fn decode(&self, b: &[u8]) -> Result<Proposal<Vec<u8>>> {
        self.effects.set(self.effects.get() + 1);
        Ok(Proposal {
            next: b.to_vec(),
            reply: b.to_vec(),
            events: vec![b.to_vec()],
        })
    }
}
struct StateCodec;
impl PersistenceCodec<Vec<u8>> for StateCodec {
    fn identity(&self) -> CodecIdentity {
        identity(3)
    }
    fn encode_state(&self, s: &Vec<u8>, limit: usize) -> Result<Vec<u8>> {
        if s.len() > limit {
            Err(Error::Limit)
        } else {
            Ok(s.clone())
        }
    }
    fn decode_state(&self, b: &[u8], limit: usize, _: &Remap) -> Result<Vec<u8>> {
        if b.len() > limit {
            Err(Error::Limit)
        } else {
            Ok(b.to_vec())
        }
    }
    fn validate_state(&self, s: &Vec<u8>) -> Result<()> {
        if s.len() == 4 {
            Ok(())
        } else {
            Err(Error::Invariant)
        }
    }
    fn references(&self, _: &Vec<u8>) -> Result<Vec<StateReference>> {
        Ok(vec![])
    }
    fn validate_configuration(
        &self,
        _: &Vec<u8>,
        p: &BTreeMap<String, Port>,
        _: &QueryPolicy,
    ) -> Result<()> {
        config(p)
    }
}
fn config(p: &BTreeMap<String, Port>) -> Result<()> {
    if p.len() == 3
        && p.get("query").is_some_and(|p| p.access == Access::Read)
        && p.get("prepare").is_some_and(|p| p.access == Access::Write)
        && p.get("derived").is_some_and(|p| p.access == Access::Read)
    {
        Ok(())
    } else {
        Err(Error::Schema)
    }
}
thread_local! {static NATIVE_EXECUTIONS:Cell<usize>=const {Cell::new(0)};}
struct NativeDomain;
impl Schema<u64> for NativeDomain {
    fn validate(&self, _: &u64) -> Result<()> {
        Ok(())
    }
    fn encode(&self, s: &u64) -> Result<Vec<u8>> {
        Ok(s.to_le_bytes().to_vec())
    }
    fn query(&self, s: &u64, _: &str, _: &[u8], _: &[Observation]) -> Result<Vec<u8>> {
        NATIVE_EXECUTIONS.with(|n| n.set(n.get() + 1));
        self.encode(s)
    }
}
impl NativeBehavior<u64> for NativeDomain {
    fn prepare(&self, s: &u64, _: &str, _: &[u8], _: &[Observation]) -> Result<Proposal<u64>> {
        NATIVE_EXECUTIONS.with(|n| n.set(n.get() + 1));
        Ok(Proposal {
            next: s + 1,
            reply: vec![],
            events: vec![],
        })
    }
}
impl PersistenceCodec<u64> for NativeDomain {
    fn identity(&self) -> CodecIdentity {
        identity(4)
    }
    fn encode_state(&self, s: &u64, _: usize) -> Result<Vec<u8>> {
        self.encode(s)
    }
    fn decode_state(&self, b: &[u8], limit: usize, _: &Remap) -> Result<u64> {
        if b.len() > limit {
            return Err(Error::Limit);
        }
        Ok(u64::from_le_bytes(
            b.try_into().map_err(|_| Error::Invalid)?,
        ))
    }
    fn validate_state(&self, s: &u64) -> Result<()> {
        self.validate(s)
    }
    fn references(&self, _: &u64) -> Result<Vec<StateReference>> {
        Ok(vec![])
    }
    fn validate_configuration(
        &self,
        _: &u64,
        p: &BTreeMap<String, Port>,
        policy: &QueryPolicy,
    ) -> Result<()> {
        if policy != &QueryPolicy::Noncacheable {
            return Err(Error::Schema);
        }
        config(p)
    }
}
struct Approved<'a> {
    reservation: &'a OwnerIdentityReservation,
    materials: &'a BTreeMap<[u8; 32], Material>,
    contexts: &'a [SavedOperationContext],
    action: Rc<RefCell<Option<Action>>>,
    factories: Cell<usize>,
    deny: bool,
}
impl RegistryAuthority for Approved<'_> {
    fn resolve(&self, row: &SavedModule) -> nethacked_kernel::Result<ArtifactMaterial> {
        self.factories.set(self.factories.get() + 1);
        match self
            .materials
            .get(&row.artifact)
            .ok_or(nethacked_kernel::Error::InvalidModule)?
        {
            Material::Guest(m, b) => Ok(ArtifactMaterial::Guest {
                manifest: m.clone(),
                bytes: b.clone(),
            }),
            Material::Native(m, b) => {
                let ports = m
                    .exports
                    .keys()
                    .map(|name| {
                        let a = self.action.clone();
                        let p: NativePort = Rc::new(move |_: &[u8], view: &CallerView| {
                            a.borrow_mut()
                                .as_mut()
                                .ok_or(nethacked_kernel::Error::NativeRefusal)?(
                                view
                            )
                        });
                        (name.clone(), p)
                    })
                    .collect();
                Ok(ArtifactMaterial::Native {
                    manifest: m.clone(),
                    identity: b.clone(),
                    ports,
                })
            }
        }
    }
    fn bind_resource(&self, r: &Resource) -> nethacked_kernel::Result<Resource> {
        self.reservation
            .resource(r)
            .map_err(|_| nethacked_kernel::Error::Authority)
    }
    fn approve_grant(
        &self,
        _: &SavedModule,
        _: usize,
        _: &SavedModule,
        a: &Authority,
    ) -> nethacked_kernel::Result<Authority> {
        if self.deny {
            Err(nethacked_kernel::Error::Authority)
        } else {
            Ok(a.clone())
        }
    }
}
impl RetainedAuthority for Approved<'_> {
    fn approve_cleanup(&self, _: u64) -> nethacked_kernel::Result<()> {
        if self.deny {
            Err(nethacked_kernel::Error::Authority)
        } else {
            Ok(())
        }
    }
    fn approve_operation_context(&self, c: &SavedOperationContext) -> nethacked_kernel::Result<()> {
        if !self.deny && self.contexts.contains(c) {
            Ok(())
        } else {
            Err(nethacked_kernel::Error::Authority)
        }
    }
}
fn factory_catalog(
    effects: Rc<Cell<usize>>,
    base: &CapabilityPermit,
    base_ids: &[u64],
    new: &CapabilityPermit,
    new_ids: &[u64],
    old: bool,
) -> RestoreCatalog<Vec<u8>> {
    factory_catalog_tracked(effects, base, base_ids, new, new_ids, old, None)
}
#[allow(clippy::too_many_arguments)]
fn factory_catalog_tracked(
    effects: Rc<Cell<usize>>,
    base: &CapabilityPermit,
    base_ids: &[u64],
    new: &CapabilityPermit,
    new_ids: &[u64],
    old: bool,
    registrations: Option<Rc<RefCell<CaptureCatalog<Vec<u8>>>>>,
) -> RestoreCatalog<Vec<u8>> {
    let mut c = RestoreCatalog::default();
    for (n, add, p, b, ids) in [
        (1, 1, old_policy(), base, base_ids),
        (2, 20, new_policy(), new, new_ids),
    ] {
        if n == 1 && !old {
            continue;
        }
        let b = b.clone();
        let ids = ids.to_vec();
        let effects = effects.clone();
        let registrations = registrations.clone();
        let policy = p.clone();
        c.allow(identity(n), p, move |r| {
            let binding = CapabilityPermit {
                capability: b.capability.clone(),
                resolution: b
                    .resolution
                    .iter()
                    .zip(&ids)
                    .map(|(b, id)| r.binding(b, *id))
                    .collect::<Result<_>>()?,
            };
            let codec: Rc<dyn WasmCodec<Vec<u8>>> = Rc::new(Domain {
                add,
                effects: effects.clone(),
            });
            if let Some(c) = &registrations {
                c.borrow_mut().register(
                    identity(n),
                    policy.clone(),
                    Backend::Wasm {
                        codec: codec.clone(),
                        slot: 0,
                        binding: binding.clone(),
                    },
                )?;
            }
            Ok(Backend::Wasm {
                codec,
                slot: 0,
                binding,
            })
        })
        .unwrap();
    }
    c
}
struct Fixture {
    owners: ExecutionCheckpoint,
    kernel: KernelSnapshot,
    materials: BTreeMap<[u8; 32], Material>,
    contexts: Vec<SavedOperationContext>,
    wasm: OwnerId,
    native: OwnerId,
    ids: Vec<BehaviorId>,
    base: CapabilityPermit,
    new: CapabilityPermit,
    base_ids: Vec<u64>,
    new_ids: Vec<u64>,
    callback: u64,
}
fn fixture() -> Fixture {
    let mut h = Harness::new(Limits::default(), KernelLimits::default(), "");
    let effects = Rc::new(Cell::new(0));
    let old: Rc<dyn WasmCodec<Vec<u8>>> = Rc::new(Domain {
        add: 1,
        effects: effects.clone(),
    });
    let base = h.binding();
    let w = h
        .host
        .enroll(
            &h.kernel,
            h.spec(1, vec![]),
            bytes(5),
            Backend::Wasm {
                codec: old.clone(),
                slot: 0,
                binding: base.clone(),
            },
        )
        .unwrap();
    let domain: Rc<dyn NativeBehavior<u64>> = Rc::new(NativeDomain);
    let n = h
        .host
        .enroll(
            &h.kernel,
            h.spec(1, vec![]),
            41_u64,
            Backend::Native(domain.clone()),
        )
        .unwrap();
    #[cfg(feature = "recorded-inputs")]
    {
        let m = Manifest {
            id: "inputs".into(),
            exports: [(nethacked_inputs::ENDPOINT.into(), endpoint(Access::Read))].into(),
            dependencies: vec![],
            extension_points: BTreeSet::new(),
            extension: None,
        };
        let ports = [(
            nethacked_inputs::ENDPOINT.into(),
            Rc::new(|_: &[u8], _: &CallerView| Err(nethacked_kernel::Error::NativeRefusal))
                as NativePort,
        )]
        .into();
        let input = h
            .kernel
            .register_native(m.clone(), b"input policy", ports)
            .unwrap();
        let artifact = h.kernel.artifact(input).unwrap();
        h.materials
            .insert(artifact, Material::Native(m, b"input policy".to_vec()));
        n.declare_recorded_inputs(
            "derived",
            vec![RecordedDeclaration {
                policy: "dice".into(),
                version: 1,
                provider: ProviderBinding {
                    handle: input,
                    artifact,
                    endpoint: nethacked_inputs::ENDPOINT.into(),
                },
                request: b"roll".to_vec(),
            }],
        )
        .unwrap();
    }
    h.admit(&n);
    h.admit(&w);
    h.refresh();
    w.set_query_policy(old_policy()).unwrap();
    let callback_scope = Scope::Keys(h.keys.clone());
    let captured = Rc::new(RefCell::new(None));
    let callbacks = Rc::new(RefCell::new(None));
    let twins = Rc::new(RefCell::new(None));
    h.run(1, {
        let w = w.clone();
        let captured = captured.clone();
        let callbacks = callbacks.clone();
        let twins = twins.clone();
        move |view| {
            let b = w.retain_behavior("prepare", view).unwrap();
            let twin = view.capture_callback(
                "run",
                Authority {
                    access: Access::Write,
                    scope: callback_scope.clone(),
                },
            )?;
            *twins.borrow_mut() = Some(twin);
            let cb = view.capture_callback(
                "run",
                Authority {
                    access: Access::Write,
                    scope: callback_scope.clone(),
                },
            )?;
            *captured.borrow_mut() = Some(b);
            *callbacks.borrow_mut() = Some(cb);
            Ok(vec![])
        }
    });
    assert_eq!(effects.get(), 0); // retained BEFORE first preparation or query
    let b = captured.borrow_mut().take().unwrap();
    let cb = callbacks.borrow_mut().take().unwrap();
    let scoped = b.bind_callback(&cb).unwrap();
    let clone = scoped.clone();
    assert_eq!(h.host.retained_behaviors().unwrap(), 2);
    let new_bytes=helper_wasm("i32.const 1024 local.get $p i32.load i32.const 100 i32.add i32.store i64.const 4398046511108");
    let manifest = Manifest {
        id: "new-algorithm".into(),
        exports: [("compute".into(), endpoint(Access::Read))].into(),
        dependencies: vec![],
        extension_points: BTreeSet::new(),
        extension: Some(Extension {
            target: base.capability.clone(),
            export: "compute".into(),
        }),
    };
    let ext = h
        .kernel
        .register_guest(manifest.clone(), &new_bytes)
        .unwrap();
    h.materials.insert(
        h.kernel.artifact(ext).unwrap(),
        Material::Guest(manifest, new_bytes),
    );
    let new = CapabilityPermit {
        capability: base.capability.clone(),
        resolution: [
            base.resolution.clone(),
            vec![ProviderBinding {
                handle: ext,
                artifact: h.kernel.artifact(ext).unwrap(),
                endpoint: "compute".into(),
            }],
        ]
        .concat(),
    };
    let future: Rc<dyn WasmCodec<Vec<u8>>> = Rc::new(Domain {
        add: 20,
        effects: effects.clone(),
    });
    w.admit_compatible_wasm(&h.kernel, [1; 32], future.clone(), 0, new.clone())
        .unwrap();
    w.set_query_policy(new_policy()).unwrap();
    h.kernel.release_pipeline(h.lease).unwrap(); // disposed source provenance remains mandatory
    let (resume_id, resume_lease) = h.kernel.adopt_callback(&cb).unwrap();
    let resume = h
        .kernel
        .operation_binding_with_cleanup(
            resume_id,
            resume_lease,
            1,
            "prepare",
            &[h.host.cleanup_identity().unwrap()],
        )
        .unwrap();
    let successor = Rc::new(RefCell::new(None));
    *h.action.borrow_mut() = Some(Box::new({
        let w = w.clone();
        let scoped = scoped.clone();
        let successor = successor.clone();
        move |view| {
            *successor.borrow_mut() =
                Some(w.retain_from_behavior(&scoped, "prepare", view).unwrap());
            Ok(vec![])
        }
    }));
    h.kernel
        .invoke_operation(
            &resume,
            &[1],
            Authority {
                access: Access::Write,
                scope: Scope::Keys(h.keys.clone()),
            },
        )
        .unwrap();
    h.action.borrow_mut().take();
    let successor = successor.borrow_mut().take().unwrap();
    let mut roots = ExecutionRoots::default();
    roots.behavior(&b).unwrap();
    roots.behavior(&scoped).unwrap();
    roots.behavior(&clone).unwrap();
    roots.behavior(&successor).unwrap();
    let live = roots.live_operations(&h.kernel).unwrap();
    assert_eq!(live.len(), 1);
    let snapshot = h.kernel.kernel_snapshot(&roots.enrollment(&live)).unwrap();
    let index = h.kernel.execution_capture_index(&snapshot).unwrap();
    let mut wc = CaptureCatalog::default();
    wc.register(
        identity(1),
        old_policy(),
        Backend::Wasm {
            codec: old,
            slot: 0,
            binding: base.clone(),
        },
    )
    .unwrap();
    wc.register(
        identity(2),
        new_policy(),
        Backend::Wasm {
            codec: future,
            slot: 0,
            binding: new.clone(),
        },
    )
    .unwrap();
    let mut nc = CaptureCatalog::default();
    nc.register(
        identity(5),
        QueryPolicy::Noncacheable,
        Backend::Native(domain),
    )
    .unwrap();
    assert!(matches!(h.host.capture_state(), Err(Error::Unsupported)));
    {
        let mut missing = snapshot.clone();
        missing.pure_captures.clear();
        let missing_index = h.kernel.execution_capture_index(&missing).unwrap();
        let mut capture = h.host.capture_execution(&missing_index).unwrap();
        assert!(capture
            .owner(&w, &StateCodec, &wc, &[&b, &scoped, &successor])
            .is_err());
    }
    {
        let mut capture = h.host.capture_execution(&index).unwrap();
        assert!(matches!(
            capture.owner(&w, &StateCodec, &wc, &[&b, &scoped]),
            Err(Error::Unsupported)
        ));
    }
    {
        let mut capture = h.host.capture_execution(&index).unwrap();
        assert!(matches!(
            capture.owner(&w, &StateCodec, &wc, &[&b, &b, &scoped, &successor]),
            Err(Error::Invalid)
        ));
    }
    let mut capture = h.host.capture_execution(&index).unwrap();
    let ids = capture
        .owner(&w, &StateCodec, &wc, &[&b, &scoped, &successor])
        .unwrap();
    capture.owner(&n, &NativeDomain, &nc, &[]).unwrap();
    let saved = capture.finish().unwrap();
    let wire = saved.to_bytes(LIMIT).unwrap();
    let owners = ExecutionCheckpoint::from_bytes(&wire, LIMIT).unwrap();
    let base_ids = base
        .resolution
        .iter()
        .map(|b| index.module(b.handle).unwrap())
        .collect();
    let new_ids = new
        .resolution
        .iter()
        .map(|b| index.module(b.handle).unwrap())
        .collect();
    if let Ok(dir) = std::env::var("OWNER_EXECUTION_EVIDENCE") {
        let dir = std::path::Path::new(&dir);
        std::fs::write(dir.join(format!("owner-{}.json", index.namespace())), &wire).unwrap();
        std::fs::write(
            dir.join(format!("kernel-{}.json", index.namespace())),
            serde_json::to_vec(&snapshot).unwrap(),
        )
        .unwrap();
    }
    let callback = index.callback(&cb).unwrap();
    assert_eq!(snapshot.callbacks.len(), 2);
    assert_eq!(callback, snapshot.callbacks[1].id);
    let contexts = snapshot.operation_contexts().unwrap();
    let result = Fixture {
        owners,
        kernel: snapshot,
        materials: h.materials.clone(),
        contexts,
        wasm: w.id(),
        native: n.id(),
        ids,
        base,
        new,
        base_ids,
        new_ids,
        callback,
    };
    assert_eq!(effects.get(), 0);
    drop(h);
    drop(w);
    drop(n);
    drop(b);
    drop(scoped);
    drop(clone);
    drop(cb);
    drop(successor);
    result
}
struct Loaded {
    kernel: Kernel,
    host: Host,
    wasm: Rc<Owner<Vec<u8>>>,
    native: Rc<Owner<u64>>,
    old: RetainedBehavior<Vec<u8>>,
    resumed: RetainedBehavior<Vec<u8>>,
    scope: RetainedBehavior<Vec<u8>>,
    callback: nethacked_kernel::RetainedCallback,
    action: Rc<RefCell<Option<Action>>>,
    effects: Rc<Cell<usize>>,
    mapped: RestoredKernel,
    wc: Rc<RefCell<CaptureCatalog<Vec<u8>>>>,
    nc: Rc<RefCell<CaptureCatalog<u64>>>,
}
fn load(f: &Fixture) -> Loaded {
    let reservation = OwnerIdentityReservation::new(
        f.owners.clone(),
        &f.kernel,
        Limits::default(),
        PublicationLimits::default(),
    )
    .unwrap();
    let action = Rc::new(RefCell::new(None));
    let authority = Approved {
        reservation: &reservation,
        materials: &f.materials,
        contexts: &f.contexts,
        action: action.clone(),
        factories: Cell::new(0),
        deny: false,
    };
    let mut kernel = Kernel::new(KernelLimits::default()).unwrap();
    let mapped = kernel.restore_kernel(&f.kernel, &authority).unwrap();
    drop(authority);
    let mut restore = reservation.bind(&kernel, &mapped).unwrap();
    let effects = Rc::new(Cell::new(0));
    let wc = Rc::new(RefCell::new(CaptureCatalog::default()));
    let catalog = factory_catalog_tracked(
        effects.clone(),
        &f.base,
        &f.base_ids,
        &f.new,
        &f.new_ids,
        true,
        Some(wc.clone()),
    );
    let wasm = restore
        .owner(&kernel, &mapped, f.wasm, &StateCodec, &catalog)
        .unwrap();
    assert!(wasm.inspect().is_err());
    let nc = Rc::new(RefCell::new(CaptureCatalog::default()));
    let mut native_catalog = RestoreCatalog::default();
    native_catalog
        .allow(identity(5), QueryPolicy::Noncacheable, {
            let nc = nc.clone();
            move |_| {
                let domain: Rc<dyn NativeBehavior<u64>> = Rc::new(NativeDomain);
                nc.borrow_mut().register(
                    identity(5),
                    QueryPolicy::Noncacheable,
                    Backend::Native(domain.clone()),
                )?;
                Ok(Backend::Native(domain))
            }
        })
        .unwrap();
    let native = restore
        .owner(&kernel, &mapped, f.native, &NativeDomain, &native_catalog)
        .unwrap();
    let old = restore.behavior(f.ids[0]).unwrap();
    let scope = restore.behavior(f.ids[1]).unwrap();
    let resumed = restore.behavior(f.ids[2]).unwrap();
    let callback = mapped.callbacks()[&f.callback].clone();
    let host = restore.finish().unwrap();
    assert_eq!(effects.get(), 0);
    NATIVE_EXECUTIONS.with(|n| assert_eq!(n.get(), 0));
    assert_eq!(host.retained_behaviors().unwrap(), 3);
    Loaded {
        kernel,
        host,
        wasm,
        native,
        old,
        resumed,
        scope,
        callback,
        action,
        effects,
        mapped,
        wc,
        nc,
    }
}
fn recapture(l: &mut Loaded, f: &Fixture) -> Fixture {
    let mut roots = ExecutionRoots::default();
    for b in [&l.old, &l.scope, &l.resumed] {
        roots.behavior(b).unwrap();
    }
    let live = roots.live_operations(&l.kernel).unwrap();
    let snapshot = l.kernel.kernel_snapshot(&roots.enrollment(&live)).unwrap();
    let index = l.kernel.execution_capture_index(&snapshot).unwrap();
    let mut capture = l.host.capture_execution(&index).unwrap();
    let ids = capture
        .owner(
            &l.wasm,
            &StateCodec,
            &l.wc.borrow(),
            &[&l.old, &l.scope, &l.resumed],
        )
        .unwrap();
    capture
        .owner(&l.native, &NativeDomain, &l.nc.borrow(), &[])
        .unwrap();
    let owners = capture.finish().unwrap();
    let owners = ExecutionCheckpoint::from_bytes(&owners.to_bytes(LIMIT).unwrap(), LIMIT).unwrap();
    let callback = index.callback(&l.callback).unwrap();
    let contexts = snapshot.operation_contexts().unwrap();
    let kernel: KernelSnapshot =
        serde_json::from_slice(&serde_json::to_vec(&snapshot).unwrap()).unwrap();
    Fixture {
        owners,
        kernel,
        materials: f.materials.clone(),
        contexts,
        wasm: l.wasm.id(),
        native: l.native.id(),
        ids,
        base: f.base.clone(),
        new: f.new.clone(),
        base_ids: f.base_ids.clone(),
        new_ids: f.new_ids.clone(),
        callback,
    }
}
trait ExecutionKeys {
    fn keys(&self) -> Vec<Resource>;
}
impl<T: 'static> ExecutionKeys for Rc<Owner<T>> {
    fn keys(&self) -> Vec<Resource> {
        ["query", "prepare", "derived"]
            .into_iter()
            .map(|n| self.resource(n).unwrap())
            .collect()
    }
}
fn execute(
    l: &Loaded,
    op: &OperationBinding,
    slot: u8,
    b: Option<RetainedBehavior<Vec<u8>>>,
    prepare: bool,
) -> Vec<u8> {
    let w = l.wasm.clone();
    *l.action.borrow_mut() = Some(Box::new(move |view| {
        if prepare {
            let c = match &b {
                Some(b) => w.prepare_retained(b, "prepare", &[], vec![], view),
                None => w.prepare("prepare", &[], vec![], view),
            }
            .unwrap();
            use sha2::Digest;
            let expected = bytes(if b.is_some() { 17 } else { 125 });
            assert_eq!(
                c.receipt().next_digest,
                <[u8; 32]>::from(sha2::Sha256::digest(&expected))
            );
            assert_eq!(c.receipt().event_count, 1);
            drop(c);
            Ok(expected)
        } else {
            Ok(match &b {
                Some(b) => w.query_retained(b, "query", &[], &[], view),
                None => w.query("query", &[], &[], view),
            }
            .unwrap()
            .bytes()
            .to_vec())
        }
    }));
    let keys = [
        &l.wasm as &dyn ExecutionKeys,
        &l.native as &dyn ExecutionKeys,
    ]
    .into_iter()
    .flat_map(|o| o.keys())
    .chain([l.host.publication_resource()])
    .collect();
    let result = l
        .kernel
        .invoke_operation(
            op,
            &[slot],
            Authority {
                access: Access::Write,
                scope: Scope::Keys(keys),
            },
        )
        .unwrap();
    l.action.borrow_mut().take();
    result
}
#[test]
fn independent_restart_old_first_execution_current_future_work_shared_scopes_and_cleanup() {
    let f = fixture();
    let mut first = load(&f);
    let first_id = first.wasm.id();
    let f = recapture(&mut first, &f);
    drop(first); // actual second save; all source storage destroyed
    let mut l = load(&f);
    assert_ne!(first_id, l.wasm.id());
    assert_eq!(l.native.inspect().unwrap(), 41_u64.to_le_bytes());
    let resumed = l.resumed.source().clone();
    let resumed_lease = resumed.pipeline();
    assert_eq!(
        number(&execute(&l, &resumed, 0, Some(l.resumed.clone()), false)),
        17
    );
    assert_eq!(
        number(&execute(&l, &resumed, 1, Some(l.resumed.clone()), true)),
        17
    );
    let (id, lease) = l.kernel.adopt_callback(&l.callback).unwrap();
    let op = l
        .kernel
        .operation_binding_with_cleanup(
            id,
            lease,
            1,
            "prepare",
            &[l.host.cleanup_identity().unwrap()],
        )
        .unwrap();
    assert!(op.admits_cleanup(l.host.cleanup_identity().unwrap()));
    assert_eq!(
        number(&execute(&l, &op, 0, Some(l.scope.clone()), false)),
        17
    );
    assert_eq!(
        number(&execute(&l, &op, 1, Some(l.scope.clone()), true)),
        17
    );
    let root = *l
        .mapped
        .modules()
        .get(
            &f.kernel
                .registry
                .modules
                .iter()
                .find(|m| m.manifest.id == "caller")
                .unwrap()
                .id,
        )
        .unwrap();
    let fresh = l
        .kernel
        .capture_pipeline(
            root,
            "run",
            Authority {
                access: Access::Write,
                scope: Scope::All,
            },
        )
        .unwrap();
    let id = l.kernel.issue_operation().unwrap();
    let new = l
        .kernel
        .operation_binding_with_cleanup(
            id,
            fresh,
            1,
            "prepare",
            &[l.host.cleanup_identity().unwrap()],
        )
        .unwrap();
    assert_eq!(number(&execute(&l, &new, 0, None, false)), 125);
    assert_eq!(number(&execute(&l, &new, 1, None, true)), 125);
    let leaf_id = f
        .kernel
        .registry
        .modules
        .iter()
        .find(|m| m.manifest.id == "leaf")
        .unwrap()
        .id;
    let leaf = l.mapped.modules()[&leaf_id];
    assert!(l.kernel.unregister(leaf).is_err());
    let clone = l.scope.clone();
    drop(l.scope);
    assert_eq!(l.host.retained_behaviors().unwrap(), 3);
    drop(clone);
    assert_eq!(l.host.retained_behaviors().unwrap(), 2);
    drop(l.old);
    assert_eq!(l.host.retained_behaviors().unwrap(), 1);
    drop(l.resumed);
    assert_eq!(l.host.retained_behaviors().unwrap(), 0);
    assert!(l.effects.get() > 0);
    l.kernel.release_pipeline(resumed_lease).unwrap();
    l.kernel.release_pipeline(lease).unwrap();
    l.kernel.release_pipeline(fresh).unwrap();
    drop(l.callback);
    drop(l.mapped);
    assert!(l.kernel.unregister(leaf).is_ok());
    // A second independent restart excludes saved owner namespaces again.
    let second = load(&f);
    assert_ne!(second.wasm.id(), l.wasm.id());
    assert_eq!(second.effects.get(), 0);
}
fn mutate(c: &ExecutionCheckpoint, f: impl FnOnce(&mut serde_json::Value)) -> ExecutionCheckpoint {
    let mut v: serde_json::Value = serde_json::from_slice(&c.to_bytes(LIMIT).unwrap()).unwrap();
    f(&mut v);
    ExecutionCheckpoint::from_bytes(&serde_json::to_vec(&v).unwrap(), LIMIT).unwrap()
}
#[test]
fn structural_bounds_and_missing_exact_old_factory_refuse_preserving_destination() {
    let f = fixture();
    let destination = load(&f);
    let before = destination.wasm.inspect().unwrap().clone();
    for bad in [
        mutate(&f.owners, |v| v["behaviors"][0]["generation"] = 999.into()),
        mutate(&f.owners, |v| {
            v["behaviors"][0]["owner"]["host"] = 999.into()
        }),
        mutate(&f.owners, |v| v["scopes"][0]["behavior"] = 999.into()),
        mutate(&f.owners, |v| {
            v["behaviors"][0]["source"]["checkpoint"] = 999.into()
        }),
        mutate(&f.owners, |v| v["behaviors"][0]["capture"] = 999.into()),
        mutate(&f.owners, |v| v["behaviors"][0]["caller"] = 999.into()),
        mutate(&f.owners, |v| v["scopes"][0]["callback"] = 999.into()),
        mutate(&f.owners, |v| v["current"][0]["module"] = 999.into()),
        mutate(&f.owners, |v| {
            v["behaviors"][0]["target"]["owner"] = "owners-999".into()
        }),
        mutate(&f.owners, |v| {
            v["current"][0]["retained_count"] = 999.into()
        }),
        mutate(&f.owners, |v| v["kernel_namespace"] = 999.into()),
        mutate(&f.owners, |v| {
            v["state"]["origins"] = serde_json::json!([0])
        }),
    ] {
        assert!(OwnerIdentityReservation::new(
            bad,
            &f.kernel,
            Limits::default(),
            PublicationLimits::default()
        )
        .is_err());
        assert_eq!(destination.wasm.inspect().unwrap().as_slice(), before);
    }
    let reservation = OwnerIdentityReservation::new(
        f.owners.clone(),
        &f.kernel,
        Limits::default(),
        PublicationLimits::default(),
    )
    .unwrap();
    let action = Rc::new(RefCell::new(None));
    let authority = Approved {
        reservation: &reservation,
        materials: &f.materials,
        contexts: &f.contexts,
        action,
        factories: Cell::new(0),
        deny: false,
    };
    let mut kernel = Kernel::new(KernelLimits::default()).unwrap();
    let mapped = kernel.restore_kernel(&f.kernel, &authority).unwrap();
    drop(authority);
    let mut restore = reservation.bind(&kernel, &mapped).unwrap();
    let effects = Rc::new(Cell::new(0));
    let missing = factory_catalog(
        effects.clone(),
        &f.base,
        &f.base_ids,
        &f.new,
        &f.new_ids,
        false,
    );
    assert!(matches!(
        restore.owner(&kernel, &mapped, f.wasm, &StateCodec, &missing),
        Err(Error::Schema)
    ));
    assert_eq!(effects.get(), 0);
    assert_eq!(destination.wasm.inspect().unwrap().as_slice(), before);
    let catalog = factory_catalog(effects, &f.base, &f.base_ids, &f.new, &f.new_ids, true);
    let escaped = restore
        .owner(&kernel, &mapped, f.wasm, &StateCodec, &catalog)
        .unwrap();
    drop(restore);
    assert!(escaped.inspect().is_err());
    assert_eq!(destination.wasm.inspect().unwrap().as_slice(), before);
}

#[test]
fn independent_authority_exact_old_artifacts_codec_policy_and_narrow_bounds() {
    let f = fixture();
    let mut destination = load(&f);
    let before = destination.kernel.lifecycle_state();
    let owner_before = destination.wasm.inspect().unwrap();
    let small = Limits {
        candidates: 2,
        ..Limits::default()
    };
    assert!(matches!(
        OwnerIdentityReservation::new(
            f.owners.clone(),
            &f.kernel,
            small,
            PublicationLimits::default()
        ),
        Err(Error::Limit)
    ));
    let small = Limits {
        payload_bytes: 3,
        ..Limits::default()
    };
    assert!(matches!(
        OwnerIdentityReservation::new(
            f.owners.clone(),
            &f.kernel,
            small,
            PublicationLimits::default()
        ),
        Err(Error::Limit)
    ));
    assert!(ExecutionCheckpoint::from_bytes(&f.owners.to_bytes(LIMIT).unwrap(), 16).is_err());
    for denied in [true, false] {
        let reservation = OwnerIdentityReservation::new(
            f.owners.clone(),
            &f.kernel,
            Limits::default(),
            PublicationLimits::default(),
        )
        .unwrap();
        let contexts = if denied { f.contexts.as_slice() } else { &[] };
        let authority = Approved {
            reservation: &reservation,
            materials: &f.materials,
            contexts,
            action: Rc::new(RefCell::new(None)),
            factories: Cell::new(0),
            deny: denied,
        };
        assert!(destination
            .kernel
            .restore_kernel(&f.kernel, &authority)
            .is_err());
        assert_eq!(authority.factories.get(), 0);
        assert_eq!(destination.kernel.lifecycle_state(), before);
        assert_eq!(destination.wasm.inspect().unwrap(), owner_before);
    }
    let old_helper = f
        .kernel
        .registry
        .modules
        .iter()
        .find(|m| m.manifest.id == "leaf")
        .unwrap()
        .artifact;
    let mut materials = f.materials.clone();
    materials.remove(&old_helper);
    let reservation = OwnerIdentityReservation::new(
        f.owners.clone(),
        &f.kernel,
        Limits::default(),
        PublicationLimits::default(),
    )
    .unwrap();
    let authority = Approved {
        reservation: &reservation,
        materials: &materials,
        contexts: &f.contexts,
        action: Rc::new(RefCell::new(None)),
        factories: Cell::new(0),
        deny: false,
    };
    assert!(destination
        .kernel
        .restore_kernel(&f.kernel, &authority)
        .is_err());
    assert_eq!(destination.kernel.lifecycle_state(), before);
    drop(authority);
    for changed in [
        mutate(&f.owners, |v| {
            v["behaviors"][0]["build"]["codec"]["artifact"] = serde_json::json!(vec![9; 32])
        }),
        mutate(&f.owners, |v| {
            v["behaviors"][0]["build"]["policy"]["version"] = 2.into()
        }),
        mutate(&f.owners, |v| {
            v["behaviors"][0]["build"]["schema"]["version"] = 2.into()
        }),
        mutate(&f.owners, |v| {
            v["behaviors"][0]["policy"] = serde_json::to_value(QueryPolicy::Noncacheable).unwrap()
        }),
    ] {
        let reservation = OwnerIdentityReservation::new(
            changed,
            &f.kernel,
            Limits::default(),
            PublicationLimits::default(),
        )
        .unwrap();
        let authority = Approved {
            reservation: &reservation,
            materials: &f.materials,
            contexts: &f.contexts,
            action: Rc::new(RefCell::new(None)),
            factories: Cell::new(0),
            deny: false,
        };
        let mut staging = Kernel::new(KernelLimits::default()).unwrap();
        let mapped = staging.restore_kernel(&f.kernel, &authority).unwrap();
        drop(authority);
        let mut restore = reservation.bind(&staging, &mapped).unwrap();
        let effects = Rc::new(Cell::new(0));
        let catalog = factory_catalog(
            effects.clone(),
            &f.base,
            &f.base_ids,
            &f.new,
            &f.new_ids,
            true,
        );
        assert!(matches!(
            restore.owner(&staging, &mapped, f.wasm, &StateCodec, &catalog),
            Err(Error::Schema)
        ));
        assert_eq!(effects.get(), 0);
        assert_eq!(destination.kernel.lifecycle_state(), before);
        assert_eq!(destination.wasm.inspect().unwrap(), owner_before);
    }
}

#[cfg(feature = "recorded-inputs")]
include!("support/persistence_prepared.rs");

#[test]
fn pass1_wire_without_pending_table_loads_default_empty_graph() {
    let f = fixture();
    let mut value = serde_json::to_value(&f.owners).unwrap();
    value.as_object_mut().unwrap().remove("pending");
    let checkpoint =
        ExecutionCheckpoint::from_bytes(&serde_json::to_vec(&value).unwrap(), LIMIT).unwrap();
    assert!(checkpoint.candidate_ids().is_empty());
    assert!(checkpoint.batch_ids().is_empty());
    assert!(OwnerIdentityReservation::new(
        checkpoint,
        &f.kernel,
        Limits::default(),
        PublicationLimits::default()
    )
    .is_ok());
}

#[cfg(feature = "recorded-inputs")]
include!("support/persistence_repair.rs");
