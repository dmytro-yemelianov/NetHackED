use nethacked_kernel::{persistence::*, *};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

pub fn auth() -> Authority {
    Authority {
        access: Access::Write,
        scope: Scope::Keys(
            [Resource {
                owner: "service".into(),
                key: "value".into(),
            }]
            .into(),
        ),
    }
}
pub fn read() -> Authority {
    Authority {
        access: Access::Read,
        ..auth()
    }
}
pub fn bytes(n: u32) -> Vec<u8> {
    n.to_le_bytes().to_vec()
}
pub fn number(b: &[u8]) -> u32 {
    u32::from_le_bytes(b.try_into().unwrap())
}
fn endpoint(access: Access) -> Endpoint {
    Endpoint {
        major: 1,
        access,
        max_request: 4,
        max_reply: 4,
    }
}
fn dep(id: &str, access: Access) -> Dependency {
    Dependency {
        capability: Capability {
            provider: id.into(),
            endpoint: "run".into(),
            major: 1,
        },
        access,
    }
}
fn manifest(id: &str, access: Access, deps: Vec<Dependency>) -> Manifest {
    Manifest {
        id: id.into(),
        exports: [("run".into(), endpoint(access))].into(),
        dependencies: deps,
        extension_points: BTreeSet::new(),
        extension: None,
    }
}
fn guest(exports: &str) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(module
    (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
    (memory (export "memory") 1)
    (func (export "alloc") (param i32) (result i32) i32.const 0)
    {exports})"#
    ))
    .unwrap()
}
fn forward(slot: usize, extra: u32) -> String {
    format!("i32.const {slot} local.get $p local.get $n i32.const 256 i32.const 4 call $call i32.const 4 i32.ne if unreachable end i32.const 256 i32.const 256 i32.load i32.const {extra} i32.add i32.store i64.const 1099511627780")
}
fn helper(extra: u32) -> Vec<u8> {
    guest(&format!(
        "(func (export \"run\") (param $p i32) (param $n i32) (result i64) {})",
        forward(0, extra)
    ))
}
#[derive(Clone)]
pub enum Material {
    Guest(Manifest, Vec<u8>),
    Native(Manifest, Vec<u8>, u32),
}
#[derive(Default)]
pub struct Effects {
    pub calls: Cell<usize>,
    pub callbacks: RefCell<Vec<RetainedCallback>>,
    pub pure: RefCell<Option<PureCapture>>,
    pub pin: RefCell<Option<ModulePin>>,
    pub observed: RefCell<Vec<OperationBinding>>,
}
pub struct Host {
    contexts: Box<[SavedOperationContext]>,
    pub materials: BTreeMap<[u8; 32], Material>,
    pub effects: Rc<Effects>,
    pub factories: Cell<usize>,
    pub narrow: bool,
    pub deny: bool,
    pub alias_resources: bool,
    pub deny_cleanup: bool,
    pub capture: bool,
    pub resource_owner: String,
}
impl Host {
    pub fn fresh(materials: BTreeMap<[u8; 32], Material>) -> Self {
        Self {
            materials,
            contexts: Box::new([]),
            effects: Rc::new(Effects::default()),
            factories: Cell::new(0),
            narrow: false,
            deny: false,
            alias_resources: false,
            deny_cleanup: false,
            capture: false,
            resource_owner: "service".into(),
        }
    }
    // Only call on an already trusted locally captured baseline, never an
    // incoming/mutated save. Fresh factories carry no original credentials.
    pub fn approved(materials: BTreeMap<[u8; 32], Material>, baseline: &KernelSnapshot) -> Self {
        Self::with_contexts(materials, &baseline.operation_contexts().unwrap())
    }
    pub fn with_contexts(
        materials: BTreeMap<[u8; 32], Material>,
        trusted: &[SavedOperationContext],
    ) -> Self {
        let mut host = Self::fresh(materials);
        host.contexts = trusted.into();
        host
    }
    pub fn authority(&self) -> Authority {
        Authority {
            access: Access::Write,
            scope: Scope::Keys(
                [Resource {
                    owner: self.resource_owner.clone(),
                    key: "value".into(),
                }]
                .into(),
            ),
        }
    }
    pub fn ports(&self, id: &str, value: u32) -> BTreeMap<String, NativePort> {
        let effects = self.effects.clone();
        let capture = self.capture;
        let schedule = id == "schedule";
        let old_codec = id == "codec";
        // Independent native allocation for every factory, never source state.
        let local = Cell::new(0usize);
        [(
            "run".into(),
            Rc::new(move |request: &[u8], view: &CallerView| {
                if schedule {
                    effects
                        .callbacks
                        .borrow_mut()
                        .push(view.capture_callback("resume", auth())?);
                    return Ok(bytes(0));
                }
                if old_codec {
                    if let Some(pure) = effects.pure.borrow().as_ref() {
                        view.validate_pure(pure)?;
                    }
                }
                if capture && effects.pure.borrow().is_none() {
                    *effects.pure.borrow_mut() = Some(view.capture_pure(0)?);
                    *effects.pin.borrow_mut() = Some(view.pin_invocation()?);
                }
                let answer = view.call_pure(0, request)?.bytes;
                if let Some(binding) = view.operation() {
                    effects.observed.borrow_mut().push(binding.clone());
                }
                local.set(local.get() + 1);
                effects.calls.set(effects.calls.get() + 1);
                Ok(bytes(number(&answer) + value))
            }) as NativePort,
        )]
        .into()
    }
}
impl RegistryAuthority for Host {
    fn resolve(&self, r: &SavedModule) -> Result<ArtifactMaterial> {
        self.factories.set(self.factories.get() + 1);
        match self
            .materials
            .get(&r.artifact)
            .ok_or(Error::InvalidModule)?
        {
            Material::Guest(m, b) => Ok(ArtifactMaterial::Guest {
                manifest: m.clone(),
                bytes: b.clone(),
            }),
            Material::Native(m, identity, value) => Ok(ArtifactMaterial::Native {
                manifest: m.clone(),
                identity: identity.clone(),
                ports: self.ports(&m.id, *value),
            }),
        }
    }
    fn bind_resource(&self, r: &Resource) -> Result<Resource> {
        if !["service", "restart", "second"].contains(&r.owner.as_str())
            || !["value", "other"].contains(&r.key.as_str())
        {
            return Err(Error::Authority);
        }
        Ok(Resource {
            owner: self.resource_owner.clone(),
            key: if self.alias_resources {
                "value".into()
            } else {
                r.key.clone()
            },
        })
    }
    fn approve_grant(
        &self,
        _: &SavedModule,
        _: usize,
        _: &SavedModule,
        a: &Authority,
    ) -> Result<Authority> {
        if self.deny {
            return Err(Error::Authority);
        }
        if self.narrow {
            return Ok(Authority {
                access: Access::Read,
                scope: Scope::Keys(BTreeSet::new()),
            });
        }
        Ok(a.clone())
    }
}
impl RetainedAuthority for Host {
    fn approve_operation_context(&self, context: &SavedOperationContext) -> Result<()> {
        if self.contexts.contains(context) {
            Ok(())
        } else {
            Err(Error::Authority)
        }
    }
    fn approve_cleanup(&self, _: u64) -> Result<()> {
        if self.deny_cleanup {
            Err(Error::Authority)
        } else {
            Ok(())
        }
    }
}
pub struct Fixture {
    pub kernel: Kernel,
    pub host: Host,
    pub actor: ModuleHandle,
    pub old: ModuleHandle,
    pub new: ModuleHandle,
    pub operation: OperationBinding,
    pub callback: RetainedCallback,
    pub owner: OwnerReference,
    pub cleanup: CleanupIdentity,
    pub disposed: PipelineLease,
}
fn install(k: &mut Kernel, host: &mut Host, mat: Material) -> ModuleHandle {
    let handle = match &mat {
        Material::Guest(m, b) => k.register_guest(m.clone(), b).unwrap(),
        Material::Native(m, identity, value) => k
            .register_native(m.clone(), identity, host.ports(&m.id, *value))
            .unwrap(),
    };
    let m = k.manifest(handle).unwrap().clone();
    for (slot, d) in m.dependencies.iter().enumerate() {
        k.issue_grant(
            handle,
            slot,
            if d.access == Access::Read {
                read()
            } else {
                auth()
            },
        )
        .unwrap();
    }
    host.materials.insert(k.artifact(handle).unwrap(), mat);
    handle
}
fn extension(id: &str, provider: &str, access: Access, deps: Vec<Dependency>) -> Manifest {
    let mut m = manifest(id, access, deps);
    m.extension_points.insert("run".into());
    m.extension = Some(Extension {
        target: Capability {
            provider: provider.into(),
            endpoint: "run".into(),
            major: 1,
        },
        export: "run".into(),
    });
    m
}
pub fn fixture() -> Fixture {
    fixture_with_retention(RetentionLimits::default())
}
pub fn fixture_with_retention(limits: RetentionLimits) -> Fixture {
    let mut kernel = Kernel::new(Limits::default()).unwrap();
    kernel.set_retention_limits(limits).unwrap();
    let mut host = Host::fresh(BTreeMap::new());
    host.capture = true;
    install(&mut kernel, &mut host, Material::Guest(manifest("sub", Access::Read, vec![]), guest("(func (export \"run\") (param $p i32) (param i32) (result i64) i32.const 0 local.get $p i32.load i32.const 1 i32.add i32.store i64.const 4)")));
    let mut hm = manifest("helper", Access::Read, vec![dep("sub", Access::Read)]);
    hm.extension_points.insert("run".into());
    install(&mut kernel, &mut host, Material::Guest(hm, helper(0)));
    let old = install(
        &mut kernel,
        &mut host,
        Material::Guest(
            extension(
                "old-helper",
                "helper",
                Access::Read,
                vec![dep("sub", Access::Read)],
            ),
            helper(10),
        ),
    );
    install(
        &mut kernel,
        &mut host,
        Material::Native(
            manifest("schedule", Access::Write, vec![]),
            b"schedule".to_vec(),
            0,
        ),
    );
    let mut cm = manifest("codec", Access::Write, vec![dep("helper", Access::Read)]);
    cm.extension_points.insert("run".into());
    install(
        &mut kernel,
        &mut host,
        Material::Native(cm, b"old-codec".to_vec(), 100),
    );
    let mut actor_m = manifest(
        "actor",
        Access::Write,
        vec![dep("schedule", Access::Write), dep("codec", Access::Write)],
    );
    actor_m
        .exports
        .insert("resume".into(), endpoint(Access::Write));
    let actor = install(
        &mut kernel,
        &mut host,
        Material::Guest(
            actor_m,
            guest(&format!(
                r#"
        (func (export "run") (param $p i32) (param $n i32) (result i64)
            {} drop {})
        (func (export "resume") (param $p i32) (param $n i32) (result i64) {})"#,
                forward(1, 0),
                forward(0, 0),
                forward(1, 0)
            )),
        ),
    );
    // Counter holes that must remain holes.
    let (_, retired) = kernel.capture_operation(actor, "resume", auth()).unwrap();
    kernel.release_pipeline(retired).unwrap();
    let cleanup = CleanupIdentity::issue().unwrap();
    let (source_op, disposed) = kernel.capture_operation(actor, "run", auth()).unwrap();
    let source = kernel
        .operation_binding_with_cleanup(source_op, disposed, 7, "captured", &[cleanup])
        .unwrap();
    assert_eq!(
        kernel.invoke_operation(&source, &bytes(5), auth()).unwrap(),
        bytes(0)
    );
    let callback = host.effects.callbacks.borrow_mut().pop().unwrap();
    kernel.release_pipeline(disposed).unwrap();
    let (op, lease) = kernel.capture_operation(actor, "resume", auth()).unwrap();
    let operation = kernel
        .operation_binding_with_cleanup(op, lease, 9, "paused", &[cleanup])
        .unwrap();
    let owner = kernel.retain_owner(&[actor, old]).unwrap();
    kernel.managed_detach(old).unwrap();
    let new = install(
        &mut kernel,
        &mut host,
        Material::Guest(
            extension(
                "new-helper",
                "helper",
                Access::Read,
                vec![dep("sub", Access::Read)],
            ),
            helper(30),
        ),
    );
    kernel.issue_grant(new, 0, read()).unwrap();
    let new_codec = install(
        &mut kernel,
        &mut host,
        Material::Native(
            extension(
                "new-codec",
                "codec",
                Access::Write,
                vec![dep("helper", Access::Read)],
            ),
            b"new-codec".to_vec(),
            200,
        ),
    );
    kernel.issue_grant(new_codec, 0, read()).unwrap();
    Fixture {
        kernel,
        host,
        actor,
        old,
        new,
        operation,
        callback,
        owner,
        cleanup,
        disposed,
    }
}
pub fn saved(f: &mut Fixture) -> KernelSnapshot {
    let pure = f.host.effects.pure.borrow().as_ref().unwrap().clone();
    f.kernel
        .kernel_snapshot(&KernelEnrollment {
            operations: &[f.operation.clone()],
            pure_captures: &[pure],
        })
        .unwrap()
}
pub fn encoded(s: &KernelSnapshot) -> KernelSnapshot {
    let bytes = serde_json::to_vec(s).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

// An untrusted record can recompute its own digest. These mutations must still
// fail semantic admission; the digest is corruption evidence, not authority.
pub fn rehash(
    namespace: u64,
    root: &SavedBinding,
    ceiling: &Authority,
    routing: &SavedRouting,
) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    fn bind(d: &mut Sha256, ns: u64, b: &SavedBinding) {
        d.update(ns.to_le_bytes());
        d.update(b.module.to_le_bytes());
        d.update(b.artifact);
        d.update((b.endpoint.len() as u64).to_le_bytes());
        d.update(b.endpoint.as_bytes());
    }
    fn canonical(v: &impl serde::Serialize) -> Vec<u8> {
        serde_json::to_vec(&serde_json::to_value(v).unwrap()).unwrap()
    }
    let mut d = Sha256::new();
    d.update(b"nethacked-retained-v1");
    bind(&mut d, namespace, root);
    d.update((routing.routes.len() as u64).to_le_bytes());
    for route in &routing.routes {
        d.update((route.chain.len() as u64).to_le_bytes());
        for b in &route.chain {
            bind(&mut d, namespace, b);
        }
    }
    d.update(canonical(ceiling));
    for slot in &routing.slots {
        d.update(namespace.to_le_bytes());
        d.update(slot.caller.to_le_bytes());
        d.update((slot.slot as u64).to_le_bytes());
        d.update(namespace.to_le_bytes());
        d.update(slot.target.to_le_bytes());
        d.update(canonical(&slot.ceiling));
    }
    d.finalize().into()
}

// A registry host opting into retained cleanup but supplying no context policy.
// RetainedAuthority's default must refuse authenticated operation reconstruction.
pub struct DefaultContextPolicy(pub Host);
impl RegistryAuthority for DefaultContextPolicy {
    fn resolve(&self, r: &SavedModule) -> Result<ArtifactMaterial> {
        self.0.resolve(r)
    }
    fn bind_resource(&self, r: &Resource) -> Result<Resource> {
        self.0.bind_resource(r)
    }
    fn approve_grant(
        &self,
        c: &SavedModule,
        s: usize,
        t: &SavedModule,
        a: &Authority,
    ) -> Result<Authority> {
        self.0.approve_grant(c, s, t, a)
    }
}
impl RetainedAuthority for DefaultContextPolicy {
    fn approve_cleanup(&self, identity: u64) -> Result<()> {
        self.0.approve_cleanup(identity)
    }
}
