#![allow(dead_code)]
use nethacked_kernel::{persistence::*, *};
use nethacked_workflows::{persistence::*, *};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
pub fn identity(name: &str) -> Identity {
    Identity {
        name: name.into(),
        version: 1,
        artifact: [7; 32],
    }
}
pub fn auth() -> Authority {
    Authority {
        access: Access::Write,
        scope: Scope::Keys(
            [Resource {
                owner: "workflow".into(),
                key: "state".into(),
            }]
            .into(),
        ),
    }
}
pub fn manifest(id: &str, dependency: Option<&str>) -> Manifest {
    Manifest {
        id: id.into(),
        exports: BTreeMap::from([(
            "step".into(),
            Endpoint {
                major: 1,
                access: Access::Write,
                max_request: 16384,
                max_reply: 16384,
            },
        )]),
        dependencies: dependency
            .into_iter()
            .map(|provider| Dependency {
                capability: Capability {
                    provider: provider.into(),
                    endpoint: "step".into(),
                    major: 1,
                },
                access: Access::Write,
            })
            .collect(),
        extension_points: BTreeSet::from(["step".into()]),
        extension: None,
    }
}
pub fn guest() -> Vec<u8> {
    wat::parse_str(
        r#"(module
    (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
    (memory (export "memory") 1)
    (func (export "alloc") (param i32) (result i32) i32.const 0)
    (func (export "step") (param $p i32) (param $n i32) (result i64) (local $len i32)
      i32.const 0 local.get $p local.get $n i32.const 20000 i32.const 16384 call $call
      local.tee $len i32.const 0 i32.lt_s if unreachable end
      i64.const 85899345920000 local.get $len i64.extend_i32_u i64.or))"#,
    )
    .unwrap()
}
#[derive(Clone)]
pub enum Material {
    Guest(Manifest, Vec<u8>),
    Native(Manifest, Vec<u8>, String),
}
type Callback = Box<dyn Fn()>;
#[derive(Default)]
pub struct Effects {
    pub calls: Cell<usize>,
    pub requests: RefCell<Vec<(String, StepRequest)>>,
    pub callback: RefCell<Option<Callback>>,
}
pub struct Host {
    pub materials: BTreeMap<[u8; 32], Material>,
    pub contexts: Vec<SavedOperationContext>,
    pub effects: Rc<Effects>,
    pub factories: Cell<usize>,
    pub deny: bool,
}
impl Host {
    pub fn fresh(
        materials: BTreeMap<[u8; 32], Material>,
        contexts: Vec<SavedOperationContext>,
    ) -> Self {
        Self {
            materials,
            contexts,
            effects: Rc::new(Effects::default()),
            factories: Cell::new(0),
            deny: false,
        }
    }
    pub fn ports(&self, label: String) -> BTreeMap<String, NativePort> {
        let effects = self.effects.clone();
        // A fresh allocation per native factory. No source closure/state reused.
        let local = Cell::new(0usize);
        BTreeMap::from([(
            "step".into(),
            Rc::new(move |bytes: &[u8], _: &CallerView| {
                effects.calls.set(effects.calls.get() + 1);
                local.set(local.get() + 1);
                if let Some(f) = effects.callback.borrow().as_ref() {
                    f();
                }
                let req: StepRequest =
                    serde_json::from_slice(bytes).map_err(|_| nethacked_kernel::Error::Buffer)?;
                effects
                    .requests
                    .borrow_mut()
                    .push((label.clone(), req.clone()));
                let outcome = match req.phase.as_str() {
                    "one" => Outcome::Advance {
                        next: "two".into(),
                        state: req.state,
                        events: vec![format!("{label}:one").into_bytes()],
                    },
                    "two" => Outcome::Suspend {
                        next: "three".into(),
                        prompt: b"choose".to_vec(),
                        state: req.state,
                        events: vec![format!("{label}:two").into_bytes()],
                    },
                    "reject" => Outcome::Reject {
                        events: vec![b"rejected".to_vec()],
                    },
                    "trap" => return Err(nethacked_kernel::Error::Trap),
                    "invalid" => return Ok(b"invalid".to_vec()),
                    _ => Outcome::Finish {
                        result: label.as_bytes().to_vec(),
                        state: req.state,
                        events: vec![format!("{label}:three").into_bytes()],
                    },
                };
                serde_json::to_vec(&outcome).map_err(|_| nethacked_kernel::Error::Buffer)
            }) as NativePort,
        )])
    }
}
impl RegistryAuthority for Host {
    fn resolve(&self, m: &SavedModule) -> nethacked_kernel::Result<ArtifactMaterial> {
        self.factories.set(self.factories.get() + 1);
        match self
            .materials
            .get(&m.artifact)
            .ok_or(nethacked_kernel::Error::InvalidModule)?
        {
            Material::Guest(manifest, bytes) => Ok(ArtifactMaterial::Guest {
                manifest: manifest.clone(),
                bytes: bytes.clone(),
            }),
            Material::Native(manifest, identity, label) => Ok(ArtifactMaterial::Native {
                manifest: manifest.clone(),
                identity: identity.clone(),
                ports: self.ports(label.clone()),
            }),
        }
    }
    fn bind_resource(&self, r: &Resource) -> nethacked_kernel::Result<Resource> {
        if r.owner == "workflow" && r.key == "state" {
            Ok(Resource {
                owner: "fresh-workflow".into(),
                key: r.key.clone(),
            })
        } else {
            Err(nethacked_kernel::Error::Authority)
        }
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
impl RetainedAuthority for Host {
    fn approve_cleanup(&self, _: u64) -> nethacked_kernel::Result<()> {
        Ok(())
    }
    fn approve_operation_context(&self, c: &SavedOperationContext) -> nethacked_kernel::Result<()> {
        if self.contexts.contains(c) {
            Ok(())
        } else {
            Err(nethacked_kernel::Error::Authority)
        }
    }
}
pub struct Opaque;
impl StateCodec for Opaque {
    fn identity(&self) -> Identity {
        identity("opaque")
    }
    fn validate(&self, b: &[u8], limit: usize) -> nethacked_workflows::Result<Vec<Reference>> {
        if b.len() > limit {
            Err(nethacked_workflows::Error::Bounds)
        } else {
            Ok(vec![])
        }
    }
    fn remap(
        &self,
        b: &[u8],
        _: &LocalRemap<'_>,
        limit: usize,
    ) -> nethacked_workflows::Result<Vec<u8>> {
        self.validate(b, limit)?;
        Ok(b.to_vec())
    }
}
pub struct RefCodec;
impl StateCodec for RefCodec {
    fn identity(&self) -> Identity {
        identity("refs")
    }
    fn validate(&self, b: &[u8], limit: usize) -> nethacked_workflows::Result<Vec<Reference>> {
        if b.len() > limit {
            return Err(nethacked_workflows::Error::Bounds);
        }
        let refs: Vec<Reference> =
            serde_json::from_slice(b).map_err(|_| nethacked_workflows::Error::Status)?;
        if refs.is_empty() {
            return Err(nethacked_workflows::Error::Status);
        }
        Ok(refs)
    }
    fn remap(
        &self,
        b: &[u8],
        r: &LocalRemap<'_>,
        limit: usize,
    ) -> nethacked_workflows::Result<Vec<u8>> {
        let refs = self
            .validate(b, limit)?
            .iter()
            .map(|v| r.reference(v))
            .collect::<nethacked_workflows::Result<Vec<_>>>()?;
        serde_json::to_vec(&refs).map_err(|_| nethacked_workflows::Error::Bounds)
    }
}
pub struct Approved(pub Checkpoint);
impl FrameAuthority for Approved {
    fn approve(
        &self,
        ns: u64,
        f: &FrameRow,
        p: &[ParticipantRow],
    ) -> nethacked_workflows::Result<()> {
        if ns == self.0.namespace && self.0.frames.contains(f) && p == self.0.participants {
            Ok(())
        } else {
            Err(nethacked_workflows::Error::Status)
        }
    }
}
pub struct Participant {
    pub id: Option<CleanupIdentity>,
    pub fail: Cell<bool>,
    pub calls: Cell<usize>,
    pub bindings: RefCell<Vec<OperationBinding>>,
}
impl CleanupParticipant for Participant {
    fn identity(&self) -> nethacked_kernel::Result<Option<CleanupIdentity>> {
        Ok(self.id)
    }
    fn discard(&self, b: &OperationBinding) -> nethacked_kernel::Result<()> {
        self.calls.set(self.calls.get() + 1);
        self.bindings.borrow_mut().push(b.clone());
        if self.fail.get() {
            Err(nethacked_kernel::Error::NativeRefusal)
        } else {
            Ok(())
        }
    }
}
pub fn participant(id: Option<CleanupIdentity>, fail: bool) -> Rc<Participant> {
    Rc::new(Participant {
        id,
        fail: Cell::new(fail),
        calls: Cell::new(0),
        bindings: RefCell::new(vec![]),
    })
}
pub struct Fixture {
    pub kernel: Kernel,
    pub host: Host,
    pub workflows: Workflows,
    pub actor: ModuleHandle,
    pub old: ModuleHandle,
    pub catalog: CaptureCatalog,
}
impl Fixture {
    pub fn new() -> Self {
        let mut kernel = fresh_kernel();
        let workflows = Workflows::new(nethacked_workflows::Limits::default()).unwrap();
        let catalog = CaptureCatalog::new(&workflows);
        let mut host = Host::fresh(BTreeMap::new(), vec![]);
        install(
            &mut kernel,
            &mut host,
            Material::Native(manifest("codec", None), b"old-codec".to_vec(), "old".into()),
        );
        install(
            &mut kernel,
            &mut host,
            Material::Guest(manifest("helper", Some("codec")), guest()),
        );
        let actor = install(
            &mut kernel,
            &mut host,
            Material::Guest(manifest("actor", Some("helper")), guest()),
        );
        Self {
            kernel,
            host,
            workflows,
            actor,
            old: actor,
            catalog,
        }
    }
    pub fn install(&mut self, material: Material) -> ModuleHandle {
        install(&mut self.kernel, &mut self.host, material)
    }
    pub fn program(&self, initial: &str) -> Program {
        Program {
            provider: self.actor,
            endpoint: "step".into(),
            authority: auth(),
            initial: initial.into(),
            phases: ["one", "two", "three", "reject", "trap", "invalid"]
                .map(String::from)
                .into(),
            allowed: [("one".into(), "two".into()), ("two".into(), "three".into())].into(),
        }
    }
    pub fn start(
        &mut self,
        initial: &str,
        state: Vec<u8>,
        participants: Vec<Rc<dyn CleanupParticipant>>,
        codec: Rc<dyn StateCodec>,
    ) -> OperationId {
        let program = self.program(initial);
        let id = self
            .workflows
            .start(&mut self.kernel, program, state, participants)
            .unwrap();
        self.catalog
            .program(id, identity(&format!("program-{initial}")), codec)
            .unwrap();
        id
    }
    pub fn save(&mut self) -> (KernelSnapshot, Checkpoint) {
        let ops = self.workflows.persistence_enrollment(&self.kernel).unwrap();
        let k = self
            .kernel
            .kernel_snapshot(&KernelEnrollment {
                operations: &ops,
                pure_captures: &[],
            })
            .unwrap();
        let index = self.kernel.execution_capture_index(&k).unwrap();
        let c = self
            .workflows
            .capture_local(&self.kernel, &index, &self.catalog)
            .unwrap();
        let k = serde_json::from_slice(&serde_json::to_vec(&k).unwrap()).unwrap();
        let c = Checkpoint::from_bytes(&c.to_bytes(1 << 20).unwrap(), 1 << 20).unwrap();
        (k, c)
    }
    pub fn replacement(&mut self) {
        let mut codec = manifest("new-codec", None);
        codec.extension = Some(Extension {
            target: Capability {
                provider: "codec".into(),
                endpoint: "step".into(),
                major: 1,
            },
            export: "step".into(),
        });
        self.install(Material::Native(codec, b"new-codec".to_vec(), "new".into()));
        // Future dependency resolution uses replacement; frozen workflow closure
        // retains the full old guest/import/native graph.
    }
}
fn install(kernel: &mut Kernel, host: &mut Host, material: Material) -> ModuleHandle {
    let h = match &material {
        Material::Guest(m, b) => kernel.register_guest(m.clone(), b).unwrap(),
        Material::Native(m, i, label) => kernel
            .register_native(m.clone(), i, host.ports(label.clone()))
            .unwrap(),
    };
    for slot in 0..kernel.manifest(h).unwrap().dependencies.len() {
        kernel.issue_grant(h, slot, auth()).unwrap();
    }
    host.materials.insert(kernel.artifact(h).unwrap(), material);
    h
}
pub fn catalog(c: &Checkpoint, codec: Rc<dyn StateCodec>) -> RestoreCatalog {
    let mut catalog = RestoreCatalog::default();
    for f in &c.frames {
        if !c
            .frames
            .iter()
            .take_while(|v| *v != f)
            .any(|v| v.program.identity == f.program.identity)
        {
            catalog.program(f.program.clone(), codec.clone()).unwrap();
        }
    }
    catalog
}
pub fn fresh_kernel() -> Kernel {
    Kernel::new(nethacked_kernel::Limits {
        payload_bytes: 16384,
        ..Default::default()
    })
    .unwrap()
}
