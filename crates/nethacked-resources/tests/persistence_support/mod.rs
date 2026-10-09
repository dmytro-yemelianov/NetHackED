#![allow(dead_code)]
use nethacked_kernel::{persistence::*, *};
use nethacked_resources::{persistence::*, *};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
pub const WIRE: u32 = 16384;
pub fn resource(owner: &str) -> Resource {
    Resource {
        owner: owner.into(),
        key: "value".into(),
    }
}
pub fn auth(keys: impl IntoIterator<Item = Resource>) -> Authority {
    Authority {
        access: Access::Write,
        scope: Scope::Keys(keys.into_iter().collect()),
    }
}
pub fn cap(provider: &str, endpoint: &str) -> Capability {
    Capability {
        provider: provider.into(),
        endpoint: endpoint.into(),
        major: 1,
    }
}
pub fn manifest(id: &str) -> Manifest {
    Manifest {
        id: id.into(),
        exports: BTreeMap::from([(
            "step".into(),
            Endpoint {
                major: 1,
                access: Access::Write,
                max_request: WIRE,
                max_reply: WIRE,
            },
        )]),
        dependencies: vec![],
        extension_points: BTreeSet::from(["step".into()]),
        extension: None,
    }
}
pub fn guest() -> Vec<u8> {
    wat::parse_str(format!(r#"(module
(import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
(memory (export "memory") 1) (func (export "alloc") (param i32) (result i32) i32.const 0)
(func (export "step") (param $p i32) (param $n i32) (result i64) (local $r i32)
local.get $p i32.load8_u i32.const 67 i32.eq
local.get $p i32.const 1 i32.add local.get $n i32.const 1 i32.sub i32.const 32768 i32.const {WIRE} call $call local.set $r
local.get $r i32.const 0 i32.lt_s if i32.const 32768 local.get $r i32.store i32.const 4 local.set $r end
 i64.const 140737488355328 local.get $r i64.extend_i32_u i64.or))"#)).unwrap()
}
pub fn command(kind: u8, v: &impl serde::Serialize) -> Vec<u8> {
    let mut b = vec![kind];
    b.extend(serde_json::to_vec(v).unwrap());
    b
}
pub fn abi(b: Vec<u8>) -> Result<Vec<u8>> {
    if b.len() == 4 {
        let n = i32::from_le_bytes(b.as_slice().try_into().unwrap());
        if n < 0 {
            return Err(Error::NativeRefusal);
        }
    }
    Ok(b)
}
pub fn fresh_kernel() -> Kernel {
    Kernel::new(nethacked_kernel::Limits {
        payload_bytes: WIRE,
        ..Default::default()
    })
    .unwrap()
}
pub fn limits() -> nethacked_resources::Limits {
    nethacked_resources::Limits {
        wire: WIRE,
        ..Default::default()
    }
}
pub fn identity(role: Role, old: bool) -> CodecIdentity {
    CodecIdentity {
        name: format!("{role:?}-{}", if old { "old" } else { "new" }),
        version: 1,
        artifact: if old { [3; 32] } else { [4; 32] },
    }
}
#[derive(Default)]
pub struct Effects {
    pub predicates: Cell<usize>,
    pub encodes: Cell<usize>,
    pub validates: Cell<usize>,
    pub decodes: Cell<usize>,
    pub callback: RefCell<Option<Box<dyn Fn()>>>,
}
pub fn predicate(e: Rc<Effects>, old: bool) -> Validator {
    Rc::new(move |d| {
        e.predicates.set(e.predicates.get() + 1);
        if let Some(c) = e.callback.borrow().as_ref() {
            c();
        }
        old || d.plan.events.is_empty()
    })
}
pub struct Codec {
    pub role: Role,
    pub old: bool,
    pub effects: Rc<Effects>,
    pub fail: bool,
}
impl PolicyCodec for Codec {
    fn identity(&self) -> CodecIdentity {
        identity(self.role, self.old)
    }
    fn role(&self) -> Role {
        self.role
    }
    fn encode(&self, _: &Validator, limit: usize) -> Result<Vec<u8>> {
        self.effects.encodes.set(self.effects.encodes.get() + 1);
        if limit < 1 {
            return Err(Error::Limit);
        }
        Ok(vec![u8::from(self.old)])
    }
    fn validate(&self, b: &[u8], limit: usize) -> Result<()> {
        self.effects.validates.set(self.effects.validates.get() + 1);
        if b != [u8::from(self.old)] || b.len() > limit {
            return Err(Error::Authority);
        }
        Ok(())
    }
    fn decode(&self, b: &[u8], _: &RestoredKernel, limit: usize) -> Result<Validator> {
        self.effects.decodes.set(self.effects.decodes.get() + 1);
        self.validate(b, limit)?;
        if self.fail {
            return Err(Error::NativeRefusal);
        }
        Ok(predicate(self.effects.clone(), self.old))
    }
}
pub fn codec(role: Role, e: &Rc<Effects>) -> Rc<dyn PolicyCodec> {
    Rc::new(Codec {
        role,
        old: true,
        effects: e.clone(),
        fail: false,
    })
}
pub fn catalog(e: &Rc<Effects>) -> RestoreCatalog {
    let mut c = RestoreCatalog::default();
    c.codec(codec(Role::Owner, e)).unwrap();
    c.codec(codec(Role::Accounting, e)).unwrap();
    c
}
pub struct Approved {
    pub bytes: Vec<u8>,
    pub kernel: Vec<u8>,
}
impl Approved {
    pub fn new(k: &KernelSnapshot, c: &Checkpoint) -> Self {
        Self {
            bytes: c.to_bytes(1 << 20).unwrap(),
            kernel: serde_json::to_vec(k).unwrap(),
        }
    }
}
impl ResourceAuthority for Approved {
    fn approve(&self, c: &Checkpoint, k: &KernelSnapshot) -> Result<()> {
        if c.to_bytes(1 << 20)? == self.bytes && serde_json::to_vec(k).unwrap() == self.kernel {
            Ok(())
        } else {
            Err(Error::Authority)
        }
    }
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub enum Material {
    Native(Manifest, Vec<u8>),
    Guest(Manifest, Vec<u8>),
}
pub struct Host {
    pub materials: BTreeMap<[u8; 32], Material>,
    pub contexts: Vec<SavedOperationContext>,
    pub ports: Option<BTreeMap<String, NativePort>>,
    pub factories: Cell<usize>,
    pub deny: bool,
    pub alias: bool,
    pub prefix: String,
}
impl Host {
    pub fn fresh(
        materials: BTreeMap<[u8; 32], Material>,
        k: &KernelSnapshot,
        ports: BTreeMap<String, NativePort>,
    ) -> Self {
        Self {
            materials,
            contexts: k.operation_contexts().unwrap(),
            ports: Some(ports),
            factories: Cell::new(0),
            deny: false,
            alias: false,
            prefix: "fresh-".into(),
        }
    }
}
impl RegistryAuthority for Host {
    fn resolve(&self, m: &SavedModule) -> Result<ArtifactMaterial> {
        self.factories.set(self.factories.get() + 1);
        match self.materials.get(&m.artifact).ok_or(Error::Authority)? {
            Material::Guest(manifest, bytes) => Ok(ArtifactMaterial::Guest {
                manifest: manifest.clone(),
                bytes: bytes.clone(),
            }),
            Material::Native(manifest, identity) => {
                let ports = if manifest.id == "resources" {
                    self.ports.clone().ok_or(Error::Authority)?
                } else {
                    {
                        let result = if manifest.id == "future" {
                            b"future".to_vec()
                        } else {
                            b"old".to_vec()
                        };
                        manifest
                            .exports
                            .keys()
                            .map(|ep| {
                                let result = result.clone();
                                (
                                    ep.clone(),
                                    Rc::new(move |_: &[u8], _: &CallerView| Ok(result.clone()))
                                        as NativePort,
                                )
                            })
                            .collect()
                    }
                };
                Ok(ArtifactMaterial::Native {
                    manifest: manifest.clone(),
                    identity: identity.clone(),
                    ports,
                })
            }
        }
    }
    fn bind_resource(&self, r: &Resource) -> Result<Resource> {
        Ok(if self.alias {
            resource("alias")
        } else {
            Resource {
                owner: format!("{}{}", self.prefix, r.owner),
                key: r.key.clone(),
            }
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
            Err(Error::Revoked)
        } else {
            Ok(a.clone())
        }
    }
}
impl RetainedAuthority for Host {
    fn approve_cleanup(&self, _: u64) -> Result<()> {
        Ok(())
    }
    fn approve_operation_context(&self, c: &SavedOperationContext) -> Result<()> {
        if self.contexts.contains(c) {
            Ok(())
        } else {
            Err(Error::Authority)
        }
    }
}
pub struct Fixture {
    pub kernel: Kernel,
    pub service: Service,
    pub actor: ModuleHandle,
    pub provider: ModuleHandle,
    pub materials: BTreeMap<[u8; 32], Material>,
    pub capture: CaptureCatalog,
    pub effects: Rc<Effects>,
}
impl Fixture {
    pub fn new() -> Self {
        let mut kernel = fresh_kernel();
        let effects = Rc::new(Effects::default());
        let service = Service::new(limits(), predicate(effects.clone(), true)).unwrap();
        let binding = service.install(&mut kernel, "resources").unwrap();
        let mut materials = BTreeMap::new();
        let native = serde_json::to_vec(
            &serde_json::to_value((
                "nethacked-resources-ports-v1",
                env!("CARGO_PKG_VERSION"),
                limits(),
            ))
            .unwrap(),
        )
        .unwrap();
        materials.insert(
            kernel.artifact(binding).unwrap(),
            Material::Native(kernel.manifest(binding).unwrap().clone(), native),
        );
        let m = manifest("policy");
        let provider = kernel
            .register_native(
                m.clone(),
                b"policy-old",
                BTreeMap::from([(
                    "step".into(),
                    Rc::new(|_: &[u8], _: &CallerView| Ok(vec![])) as NativePort,
                )]),
            )
            .unwrap();
        materials.insert(
            kernel.artifact(provider).unwrap(),
            Material::Native(m, b"policy-old".to_vec()),
        );
        for owner in ["a", "b"] {
            service
                .register_owner(
                    owner,
                    OwnerPolicy {
                        capability: cap("policy", "step"),
                        provider,
                        artifact: kernel.artifact(provider).unwrap(),
                        validate: predicate(effects.clone(), true),
                    },
                )
                .unwrap();
            service
                .put(resource(owner), owner.as_bytes().to_vec())
                .unwrap();
        }
        let mut m = manifest("actor");
        m.dependencies = vec![
            Dependency {
                capability: cap("resources", "prepare"),
                access: Access::Write,
            },
            Dependency {
                capability: cap("resources", "commit"),
                access: Access::Write,
            },
            Dependency {
                capability: cap("policy", "step"),
                access: Access::Write,
            },
        ];
        let bytes = guest();
        let actor = kernel.register_guest(m.clone(), &bytes).unwrap();
        materials.insert(kernel.artifact(actor).unwrap(), Material::Guest(m, bytes));
        for slot in 0..3 {
            kernel
                .issue_grant(actor, slot, auth([resource("a"), resource("b")]))
                .unwrap();
        }
        let mut capture = CaptureCatalog::new(&service);
        for owner in ["a", "b"] {
            capture.owner(owner, codec(Role::Owner, &effects)).unwrap();
        }
        capture
            .accounting(codec(Role::Accounting, &effects))
            .unwrap();
        Self {
            kernel,
            service,
            actor,
            provider,
            materials,
            capture,
            effects,
        }
    }
    pub fn auth(&self) -> Authority {
        auth(self.service.state().unwrap().resources.keys().cloned())
    }
    pub fn plan(&self) -> Plan {
        let state = self.service.state().unwrap();
        Plan {
            readonly: vec![state.resources[&resource("b")].stamp.clone()],
            writes: vec![Write {
                stamp: state.resources[&resource("a")].stamp.clone(),
                replacement: Some(b"changed".to_vec()),
            }],
            events: vec![b"one".to_vec(), b"two".to_vec()],
        }
    }
    pub fn prepare(&self, plan: &Plan, op: Option<&OperationBinding>) -> Identity {
        let request = command(b'P', plan);
        let reply = if let Some(op) = op {
            self.kernel.invoke_operation(op, &request, self.auth())
        } else {
            self.kernel
                .invoke(self.actor, "step", &request, self.auth())
        }
        .unwrap();
        serde_json::from_slice(&abi(reply).unwrap()).unwrap()
    }
    pub fn binding(&mut self) -> OperationBinding {
        let (id, lease) = self
            .kernel
            .capture_operation(self.actor, "step", self.auth())
            .unwrap();
        self.kernel
            .operation_binding_with_cleanup(
                id,
                lease,
                0,
                "charge",
                &[self.service.cleanup_identity().unwrap()],
            )
            .unwrap()
    }
    pub fn save(&mut self) -> (KernelSnapshot, Checkpoint) {
        let ops = self.service.persistence_enrollment(&self.kernel).unwrap();
        let (cleanup, resources) = self.service.persistence_roots().unwrap();
        let k = self
            .kernel
            .kernel_snapshot_with_resource_roots(
                &KernelEnrollment {
                    operations: &ops,
                    pure_captures: &[],
                },
                &[cleanup],
                &resources,
                &self.service.persistence_history().unwrap(),
            )
            .unwrap();
        let index = self.kernel.execution_capture_index(&k).unwrap();
        let c = self
            .service
            .capture_local(&index, &k, &self.capture)
            .unwrap();
        let bytes = c.to_bytes(1 << 20).unwrap();
        (
            serde_json::from_slice(&serde_json::to_vec(&k).unwrap()).unwrap(),
            Checkpoint::from_bytes(&bytes, 1 << 20).unwrap(),
        )
    }
}
pub fn restore(
    k: &KernelSnapshot,
    c: &Checkpoint,
    materials: BTreeMap<[u8; 32], Material>,
    effects: &Rc<Effects>,
) -> (Kernel, RestoredKernel, Restored) {
    let approved = Approved::new(k, c);
    let staged = c
        .clone()
        .validate(&limits(), k, catalog(effects), &approved)
        .unwrap()
        .stage()
        .unwrap();
    let host = Host::fresh(materials, k, staged.ports());
    let mut kernel = fresh_kernel();
    let mapped = kernel.restore_kernel(k, &host).unwrap();
    let restored = staged.finish(&kernel, &mapped).unwrap();
    (kernel, mapped, restored)
}
pub fn commit(
    kernel: &Kernel,
    mapped: &RestoredKernel,
    restored: &Restored,
    actor: u64,
    serial: u64,
    op: Option<&nethacked_kernel::persistence::SavedOperation>,
) -> Result<Vec<u8>> {
    let id = restored.transaction(serial)?;
    let request = command(b'C', &id);
    let a = auth(restored.service().state()?.resources.keys().cloned());
    let reply = if let Some(op) = op {
        kernel.invoke_operation(&mapped.execution_source(op)?, &request, a)
    } else {
        kernel.invoke(mapped.modules()[&actor], "step", &request, a)
    }?;
    abi(reply)
}
