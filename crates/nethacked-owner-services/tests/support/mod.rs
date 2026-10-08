#![allow(dead_code)]
use nethacked_kernel::*;
use nethacked_owner_services::*;
use nethacked_owners::{self as owners, *};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
#[path = "../../../nethacked-timers/tests/support.rs"]
pub mod timer_fixture;
pub use timer_fixture::Clock;

pub type Action = Box<dyn FnMut(&str, &[u8], &CallerView<'_>) -> nethacked_kernel::Result<Vec<u8>>>;
pub fn cap(provider: &str, endpoint: &str) -> Capability {
    Capability {
        provider: provider.into(),
        endpoint: endpoint.into(),
        major: 1,
    }
}
pub fn ep(access: Access) -> Endpoint {
    Endpoint {
        major: 1,
        access,
        max_request: 16384,
        max_reply: 16384,
    }
}
pub fn manifest(
    id: &str,
    exports: &[(&str, Access)],
    dependencies: &[(&str, &str, Access)],
) -> Manifest {
    Manifest {
        id: id.into(),
        exports: exports
            .iter()
            .map(|(n, a)| (n.to_string(), ep(*a)))
            .collect(),
        dependencies: dependencies
            .iter()
            .map(|(p, e, a)| nethacked_kernel::Dependency {
                capability: cap(p, e),
                access: *a,
            })
            .collect(),
        extension_points: BTreeSet::new(),
        extension: None,
    }
}
pub fn all() -> Authority {
    Authority {
        access: Access::Write,
        scope: Scope::All,
    }
}
pub fn queue_key() -> Resource {
    Resource {
        owner: "timers".into(),
        key: "queue".into(),
    }
}
pub fn input_key() -> Resource {
    Resource {
        owner: "inputs".into(),
        key: "source".into(),
    }
}
pub fn owner_error(e: owners::Error) -> nethacked_kernel::Error {
    match e {
        owners::Error::Kernel(e) => e,
        _ => nethacked_kernel::Error::NativeRefusal,
    }
}
pub fn wire<T: serde::Serialize>(t: &T) -> Vec<u8> {
    serde_json::to_vec(t).unwrap()
}

/// Transparent test-only witness of the actual queue admission boundary.
pub struct RetentionProbe {
    pub delegate: Rc<CallbackBehaviors>,
    pub preflight: RefCell<Vec<nethacked_kernel::Result<()>>>,
    pub retained: Cell<usize>,
}
impl nethacked_timers::CallbackRetention for RetentionProbe {
    fn preflight(
        &self,
        view: &CallerView<'_>,
        targets: &[nethacked_resources::Stamp],
    ) -> nethacked_kernel::Result<()> {
        let result =
            nethacked_timers::CallbackRetention::preflight(self.delegate.as_ref(), view, targets);
        self.preflight.borrow_mut().push(result);
        result
    }
    fn retain(
        &self,
        callback: &RetainedCallback,
        targets: &[nethacked_resources::Stamp],
    ) -> nethacked_kernel::Result<Rc<dyn nethacked_timers::CallbackReference>> {
        self.retained.set(self.retained.get() + 1);
        nethacked_timers::CallbackRetention::retain(self.delegate.as_ref(), callback, targets)
    }
}

pub struct Harness {
    pub kernel: Kernel,
    pub host: owners::Host,
    pub module: ModuleHandle,
    pub actor: ModuleHandle,
    pub algorithm: ModuleHandle,
    pub input_module: ModuleHandle,
    pub inputs: nethacked_inputs::Service,
    pub timers: nethacked_timers::Service,
    pub targets: Rc<Targets>,
    pub behaviors: Rc<CallbackBehaviors>,
    pub retention: Rc<RetentionProbe>,
    pub review_bindings: bool,
    pub clock: Rc<Clock>,
    pub action: Rc<RefCell<Option<Action>>>,
    pub samples: Rc<Cell<usize>>,
    pub keys: BTreeSet<Resource>,
}
impl Harness {
    pub fn new() -> Self {
        Self::with_owner_limits(owners::Limits::default())
    }
    pub fn with_owner_limits(limits: owners::Limits) -> Self {
        Self::build(limits, false)
    }
    pub fn for_binding_review() -> Self {
        Self::build(owners::Limits::default(), true)
    }
    fn build(limits: owners::Limits, review_bindings: bool) -> Self {
        let mut kernel = Kernel::new(nethacked_kernel::Limits::default()).unwrap();
        let host = owners::Host::new(limits).unwrap();
        // Real import chain: algorithm calls an independent Wasm arithmetic module.
        let math = wat::parse_str(
            r#"(module (memory (export "memory") 1)
          (func (export "alloc") (param i32) (result i32) i32.const 0)
          (func (export "compute") (param $p i32) (param i32) (result i64)
            i32.const 0 local.get $p i32.load i32.const 1 i32.add i32.store i64.const 4))"#,
        )
        .unwrap();
        kernel
            .register_guest(manifest("math", &[("compute", Access::Read)], &[]), &math)
            .unwrap();
        let algorithm_bytes = wat::parse_str(
            r#"(module
          (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
          (memory (export "memory") 1)
          (func (export "alloc") (param i32) (result i32) i32.const 0)
          (func (export "compute") (param $p i32) (param i32) (result i64) (local $second i32)
            local.get $p i32.const 4 i32.add i32.load local.set $second
            i32.const 1024 local.get $p i32.load
              local.get $p i32.const 8 i32.add i32.load i32.add
              local.get $p i32.const 12 i32.add i32.load i32.add i32.store
            i32.const 0 i32.const 1024 i32.const 4 i32.const 2048 i32.const 16384 call $call
            i32.const 4 i32.ne if unreachable end
            i32.const 2052 local.get $second i32.store
            i64.const 8796093022216))"#,
        )
        .unwrap();
        let mut algorithm_manifest = manifest(
            "algorithm",
            &[("compute", Access::Read)],
            &[("math", "compute", Access::Read)],
        );
        algorithm_manifest.extension_points.insert("compute".into());
        let algorithm = kernel
            .register_guest(algorithm_manifest, &algorithm_bytes)
            .unwrap();
        let read_empty = Authority {
            access: Access::Read,
            scope: Scope::Keys(BTreeSet::new()),
        };
        kernel
            .issue_grant(algorithm, 0, read_empty.clone())
            .unwrap();
        let action: Rc<RefCell<Option<Action>>> = Rc::new(RefCell::new(None));
        let ports = ["query", "prepare", "derived", "effect"]
            .into_iter()
            .map(|name| {
                let a = action.clone();
                (
                    name.to_string(),
                    Rc::new(move |bytes: &[u8], v: &CallerView| {
                        a.borrow_mut().as_mut().unwrap()(name, bytes, v)
                    }) as NativePort,
                )
            })
            .collect();
        let module = kernel
            .register_native(
                manifest(
                    "owners",
                    &[
                        ("query", Access::Read),
                        ("prepare", Access::Write),
                        ("derived", Access::Read),
                        ("effect", Access::Write),
                    ],
                    &[("algorithm", "compute", Access::Read)],
                ),
                b"independent owner adapter fixture v1",
                ports,
            )
            .unwrap();
        kernel.issue_grant(module, 0, read_empty).unwrap();
        let samples = Rc::new(Cell::new(0));
        let calls = samples.clone();
        let inputs = nethacked_inputs::Service::new(
            nethacked_inputs::Limits::default(),
            nethacked_inputs::Policy {
                identity: "recorded-nondeterministic-fixture".into(),
                version: 1,
                transition: Rc::new(move |source, request| {
                    calls.set(calls.get() + 1);
                    if request != b"choice" {
                        return Err(nethacked_kernel::Error::NativeRefusal);
                    }
                    Ok((vec![source[0] + 1], vec![source[0]]))
                }),
            },
            vec![3],
        )
        .unwrap();
        let input_module = inputs.install(&mut kernel, "inputs").unwrap();
        let targets = Rc::new(Targets::new(32).unwrap());
        let behaviors = Rc::new(CallbackBehaviors::new(32).unwrap());
        let clock = Rc::new(Clock::default());
        let timers = nethacked_timers::Service::new(
            nethacked_timers::Limits::default(),
            queue_key(),
            clock.clone(),
            targets.clone(),
            true,
            vec![Rc::new(host.clone())],
        )
        .unwrap();
        let retention = Rc::new(RetentionProbe {
            delegate: behaviors.clone(),
            preflight: RefCell::new(vec![]),
            retained: Cell::new(0),
        });
        timers.retain_callbacks(retention.clone()).unwrap();
        if review_bindings {
            let a = action.clone();
            kernel
                .register_native(
                    manifest("review-service", &[("effect", Access::Write)], &[]),
                    b"test-only binding review service",
                    BTreeMap::from([(
                        "effect".into(),
                        Rc::new(move |b: &[u8], v: &CallerView| {
                            a.borrow_mut().as_mut().unwrap()("effect", b, v)
                        }) as NativePort,
                    )]),
                )
                .unwrap();
        }
        timers.install(&mut kernel, "timers").unwrap();
        let actor_wat = r#"(module
          (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
          (memory (export "memory") 1)
          (func (export "alloc") (param i32) (result i32) i32.const 0)
          (func $forward (param $s i32) (param $p i32) (param $n i32) (result i32) (local $r i32)
            local.get $s local.get $p local.get $n i32.const 32768 i32.const 16384 call $call local.tee $r
            i32.const 0 i32.lt_s if unreachable end local.get $r)
          (func (export "run") (param $p i32) (param $n i32) (result i64)
            local.get $p i32.load8_u local.get $p i32.const 1 i32.add local.get $n i32.const 1 i32.sub call $forward
            i64.extend_i32_u i64.const 140737488355328 i64.or)
          (func $workflow (param $p i32) (param $n i32) (result i64) (local $r i32)
            i32.const 5 local.get $p local.get $n call $forward local.set $r
            i32.const 3 i32.const 32768 local.get $r call $forward local.set $r
            i32.const 5 i32.const 32768 local.get $r call $forward
            i64.extend_i32_u i64.const 140737488355328 i64.or)
          (func (export "workflow") (param i32 i32) (result i64) local.get 0 local.get 1 call $workflow)
          (func (export "fault") (param i32 i32) (result i64) local.get 0 local.get 1 call $workflow drop unreachable)
          (func (export "callback") (param $p i32) (param $n i32) (result i64)
            i32.const 5 local.get $p local.get $n call $forward i64.extend_i32_u i64.const 140737488355328 i64.or)
          (func (export "successor") (param $p i32) (param $n i32) (result i64) (local $r i32)
            i32.const 5 local.get $p local.get $n call $forward local.set $r
            i32.const 4 i32.const 32768 local.get $r call $forward local.set $r
            i32.const 5 i32.const 32768 local.get $r call $forward i64.extend_i32_u i64.const 140737488355328 i64.or))"#;
        let actor_wat = if review_bindings {
            format!(
                "{} {} )",
                &actor_wat[..actor_wat.len() - 1],
                r#"
              (func (export "review-route") (param i32 i32) (result i64) local.get 0 i32.load8_u local.get 0 local.get 1 call $forward i64.extend_i32_u i64.const 140737488355328 i64.or)
              (func (export "review-endpoint") (param i32 i32) (result i64) i32.const 5 local.get 0 local.get 1 call $forward i64.extend_i32_u i64.const 140737488355328 i64.or)
              (func (export "review-slot") (param i32 i32) (result i64) i32.const 6 local.get 0 local.get 1 call $forward i64.extend_i32_u i64.const 140737488355328 i64.or)
              (func (export "review-service") (param i32 i32) (result i64) i32.const 7 local.get 0 local.get 1 call $forward i64.extend_i32_u i64.const 140737488355328 i64.or)"#
            )
        } else {
            actor_wat.to_string()
        };
        let actor_bytes = wat::parse_str(actor_wat).unwrap();
        let mut actor_manifest = manifest(
            "actor",
            &[
                ("run", Access::Write),
                ("workflow", Access::Write),
                ("fault", Access::Write),
                ("callback", Access::Write),
                ("successor", Access::Write),
            ],
            &[
                ("owners", "query", Access::Read),
                ("owners", "prepare", Access::Write),
                ("owners", "derived", Access::Read),
                ("inputs", "input", Access::Write),
                ("timers", "queue", Access::Write),
                ("owners", "effect", Access::Write),
            ],
        );
        if review_bindings {
            for endpoint in [
                "review-route",
                "review-endpoint",
                "review-slot",
                "review-service",
            ] {
                actor_manifest
                    .exports
                    .insert(endpoint.into(), ep(Access::Write));
            }
            actor_manifest.dependencies.extend([
                nethacked_kernel::Dependency {
                    capability: cap("owners", "effect"),
                    access: Access::Write,
                },
                nethacked_kernel::Dependency {
                    capability: cap("review-service", "effect"),
                    access: Access::Write,
                },
            ]);
        }
        let actor = kernel.register_guest(actor_manifest, &actor_bytes).unwrap();
        let mut h = Self {
            kernel,
            host,
            module,
            actor,
            algorithm,
            input_module,
            inputs,
            timers,
            targets,
            behaviors,
            retention,
            review_bindings,
            clock,
            action,
            samples,
            keys: BTreeSet::new(),
        };
        h.keys
            .extend([queue_key(), input_key(), h.host.publication_resource()]);
        h.grants();
        h
    }
    pub fn binding(&self) -> CapabilityPermit {
        CapabilityPermit {
            capability: cap("algorithm", "compute"),
            resolution: vec![ProviderBinding {
                handle: self.algorithm,
                artifact: self.kernel.artifact(self.algorithm).unwrap(),
                endpoint: "compute".into(),
            }],
        }
    }
    pub fn spec(&self, schema: u8, dependencies: Vec<owners::Dependency>) -> OwnerSpec {
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
    pub fn admit<T: 'static>(&mut self, owner: &Rc<Owner<T>>) {
        for name in ["query", "prepare", "derived"] {
            self.keys.insert(owner.resource(name).unwrap());
        }
        self.grants();
    }
    pub fn grants(&mut self) {
        for slot in 0..if self.review_bindings { 8 } else { 6 } {
            self.kernel
                .issue_grant(
                    self.actor,
                    slot,
                    Authority {
                        access: if slot == 0 || slot == 2 {
                            Access::Read
                        } else {
                            Access::Write
                        },
                        scope: Scope::Keys(self.keys.clone()),
                    },
                )
                .unwrap();
        }
    }
    pub fn operation(&mut self) -> OperationBinding {
        let (op, lease) = self
            .kernel
            .capture_operation(self.actor, "run", all())
            .unwrap();
        self.kernel
            .operation_binding_with_cleanup(
                op,
                lease,
                0,
                "compose",
                &[self.host.cleanup_identity().unwrap()],
            )
            .unwrap()
    }
    pub fn run(
        &self,
        operation: &OperationBinding,
        slot: u8,
        bytes: &[u8],
        action: impl FnMut(&str, &[u8], &CallerView<'_>) -> nethacked_kernel::Result<Vec<u8>> + 'static,
    ) -> Vec<u8> {
        *self.action.borrow_mut() = Some(Box::new(action));
        let out = self
            .kernel
            .invoke_operation(operation, &[&[slot], bytes].concat(), all());
        self.action.borrow_mut().take();
        out.unwrap()
    }
    pub fn run_scoped(
        &self,
        operation: &OperationBinding,
        slot: u8,
        action: impl FnMut(&str, &[u8], &CallerView<'_>) -> nethacked_kernel::Result<Vec<u8>> + 'static,
    ) -> Vec<u8> {
        *self.action.borrow_mut() = Some(Box::new(action));
        let out = self.kernel.invoke_operation(
            operation,
            &[slot],
            Authority {
                access: Access::Write,
                scope: Scope::Keys(self.keys.clone()),
            },
        );
        self.action.borrow_mut().take();
        out.unwrap()
    }
    pub fn replace(&mut self) -> (ModuleHandle, CapabilityPermit) {
        let bytes=wat::parse_str(r#"(module
          (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
          (memory (export "memory") 1)
          (func (export "alloc") (param i32) (result i32) i32.const 0)
          (func (export "compute") (param $p i32) (param $n i32) (result i64)
            i32.const 2052 local.get $p i32.const 4 i32.add i32.load i32.store
            i32.const 1024 local.get $p i32.load local.get $p i32.const 8 i32.add i32.load i32.add local.get $p i32.const 12 i32.add i32.load i32.add i32.store
            i32.const 0 i32.const 1024 i32.const 4 i32.const 2048 i32.const 16384 call $call i32.const 4 i32.ne if unreachable end
            i32.const 2048 i32.const 2048 i32.load i32.const 2 i32.mul i32.store i64.const 8796093022216))"#).unwrap();
        let mut m = manifest(
            "future",
            &[("compute", Access::Read)],
            &[("math", "compute", Access::Read)],
        );
        m.extension = Some(Extension {
            target: cap("algorithm", "compute"),
            export: "compute".into(),
        });
        let extension = self.kernel.register_guest(m, &bytes).unwrap();
        self.kernel
            .issue_grant(
                extension,
                0,
                Authority {
                    access: Access::Read,
                    scope: Scope::Keys(BTreeSet::new()),
                },
            )
            .unwrap();
        let mut binding = self.binding();
        binding.resolution.push(ProviderBinding {
            handle: extension,
            artifact: self.kernel.artifact(extension).unwrap(),
            endpoint: "compute".into(),
        });
        (extension, binding)
    }
    pub fn program(&self, endpoint: &str) -> nethacked_workflows::Program {
        nethacked_workflows::Program {
            provider: self.actor,
            endpoint: endpoint.into(),
            authority: all(),
            initial: "capture".into(),
            phases: BTreeSet::from(["capture".into(), "apply".into()]),
            allowed: BTreeSet::from([("capture".into(), "apply".into())]),
        }
    }
}

pub struct Accept(pub Rc<Cell<usize>>);
impl Accounting for Accept {
    fn validate(&self, _: &[ProposalView], _: &[Observation]) -> owners::Result<()> {
        self.0.set(self.0.get() + 1);
        Ok(())
    }
}
pub fn commit<T: 'static>(
    host: &owners::Host,
    owner: &Rc<Owner<T>>,
    candidate: Candidate<T>,
    view: &CallerView<'_>,
    reached: Rc<Cell<usize>>,
) -> CommitReceipt {
    let id = owner.id();
    host.batch(
        vec![candidate.erase()],
        vec![],
        &[id],
        vec![id],
        [7; 32],
        Rc::new(Accept(reached)),
        cap("owners", "effect"),
        view,
    )
    .unwrap()
    .commit(view)
    .unwrap()
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Device {
    pub boost: u32,
}
pub struct DeviceDomain;
impl Schema<Device> for DeviceDomain {
    fn validate(&self, _: &Device) -> owners::Result<()> {
        Ok(())
    }
    fn encode(&self, s: &Device) -> owners::Result<Vec<u8>> {
        Ok(s.boost.to_le_bytes().to_vec())
    }
    fn query(&self, s: &Device, _: &str, _: &[u8], _: &[Observation]) -> owners::Result<Vec<u8>> {
        self.encode(s)
    }
}
impl NativeBehavior<Device> for DeviceDomain {
    fn prepare(
        &self,
        _: &Device,
        _: &str,
        r: &[u8],
        _: &[Observation],
    ) -> owners::Result<Proposal<Device>> {
        Ok(Proposal {
            next: Device {
                boost: u32::from_le_bytes(r.try_into().map_err(|_| owners::Error::Invalid)?),
            },
            events: vec![b"device".to_vec()],
            reply: vec![],
        })
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Field {
    cells: Vec<u32>,
}
impl Field {
    pub fn new(a: u32, b: u32) -> Self {
        Self { cells: vec![a, b] }
    }
}
pub struct ScalarCodec(pub Rc<Cell<usize>>);
pub struct FieldCodec(pub Rc<Cell<usize>>);
fn input(
    values: [u32; 2],
    obs: &[Observation],
    recorded: &[nethacked_inputs::Observation],
) -> owners::Result<Vec<u8>> {
    let boost = u32::from_le_bytes(
        obs.first()
            .ok_or(owners::Error::Invalid)?
            .bytes()
            .try_into()
            .map_err(|_| owners::Error::Invalid)?,
    );
    let choice = recorded.first().map_or(0, |r| u32::from(r.bytes()[0]));
    Ok([
        values[0].to_le_bytes(),
        values[1].to_le_bytes(),
        boost.to_le_bytes(),
        choice.to_le_bytes(),
    ]
    .concat())
}
impl Schema<u32> for ScalarCodec {
    fn validate(&self, s: &u32) -> owners::Result<()> {
        if *s <= 1000 {
            Ok(())
        } else {
            Err(owners::Error::Invariant)
        }
    }
    fn encode(&self, s: &u32) -> owners::Result<Vec<u8>> {
        Ok(s.to_le_bytes().to_vec())
    }
    fn query(&self, _: &u32, _: &str, _: &[u8], _: &[Observation]) -> owners::Result<Vec<u8>> {
        panic!("guest query required")
    }
}
impl WasmCodec<u32> for ScalarCodec {
    fn input(&self, s: &u32, _: &str, _: &[u8], o: &[Observation]) -> owners::Result<Vec<u8>> {
        self.0.set(self.0.get() + 1);
        input([*s, 0], o, &[])
    }
    fn input_recorded(
        &self,
        s: &u32,
        _: &str,
        _: &[u8],
        o: &[Observation],
        r: &[nethacked_inputs::Observation],
    ) -> owners::Result<Vec<u8>> {
        self.0.set(self.0.get() + 1);
        input([*s, 0], o, r)
    }
    fn decode(&self, b: &[u8]) -> owners::Result<Proposal<u32>> {
        if b.len() != 8 || b[4..] != [0; 4] {
            return Err(owners::Error::Invalid);
        }
        Ok(Proposal {
            next: u32::from_le_bytes(b[..4].try_into().unwrap()),
            events: vec![b"scalar".to_vec()],
            reply: b[..4].to_vec(),
        })
    }
    fn decode_query(&self, b: &[u8]) -> owners::Result<Vec<u8>> {
        Ok(self.decode(b)?.reply)
    }
}
impl Schema<Field> for FieldCodec {
    fn validate(&self, s: &Field) -> owners::Result<()> {
        if s.cells.len() == 2 && s.cells.iter().all(|n| *n <= 1000) {
            Ok(())
        } else {
            Err(owners::Error::Invariant)
        }
    }
    fn encode(&self, s: &Field) -> owners::Result<Vec<u8>> {
        Ok(s.cells.iter().flat_map(|n| n.to_le_bytes()).collect())
    }
    fn query(&self, _: &Field, _: &str, _: &[u8], _: &[Observation]) -> owners::Result<Vec<u8>> {
        panic!("guest query required")
    }
}
impl WasmCodec<Field> for FieldCodec {
    fn input(&self, s: &Field, _: &str, _: &[u8], o: &[Observation]) -> owners::Result<Vec<u8>> {
        self.0.set(self.0.get() + 1);
        input([s.cells[0], s.cells[1]], o, &[])
    }
    fn decode(&self, b: &[u8]) -> owners::Result<Proposal<Field>> {
        if b.len() != 8 {
            return Err(owners::Error::Invalid);
        }
        Ok(Proposal {
            next: Field::new(
                u32::from_le_bytes(b[..4].try_into().unwrap()),
                u32::from_le_bytes(b[4..].try_into().unwrap()),
            ),
            events: vec![b"field".to_vec()],
            reply: b.to_vec(),
        })
    }
    fn decode_query(&self, b: &[u8]) -> owners::Result<Vec<u8>> {
        Ok(self.decode(b)?.reply)
    }
}
pub fn dep<T: 'static>(owner: &Owner<T>, query: &str) -> owners::Dependency {
    owners::Dependency {
        owner: owner.id(),
        query: query.into(),
        schema: owner.stamp().unwrap().schema,
    }
}
pub fn observe<T: 'static>(h: &Harness, op: &OperationBinding, owner: Rc<Owner<T>>) -> Observation {
    let saved = Rc::new(RefCell::new(None));
    let sink = saved.clone();
    h.run(op, 0, &[], move |_, _, v| {
        *sink.borrow_mut() = Some(owner.query("query", &[], &[], v).unwrap());
        Ok(vec![])
    });
    let observation = saved.borrow_mut().take().unwrap();
    observation
}
pub fn device(h: &mut Harness) -> Rc<Owner<Device>> {
    let o = h
        .host
        .enroll(
            &h.kernel,
            h.spec(1, vec![]),
            Device { boost: 4 },
            Backend::Native(Rc::new(DeviceDomain)),
        )
        .unwrap();
    h.admit(&o);
    o
}
pub fn scalar(h: &mut Harness, d: &Owner<Device>, calls: Rc<Cell<usize>>) -> Rc<Owner<u32>> {
    let codec = Rc::new(ScalarCodec(calls));
    let o = h
        .host
        .enroll(
            &h.kernel,
            h.spec(2, vec![dep(d, "query")]),
            5,
            Backend::Wasm {
                codec: codec.clone(),
                slot: 0,
                binding: h.binding(),
            },
        )
        .unwrap();
    o.admit_compatible_wasm(&h.kernel, [2; 32], codec, 0, h.binding())
        .unwrap();
    h.admit(&o);
    o
}
pub fn field(h: &mut Harness, d: &Owner<Device>, calls: Rc<Cell<usize>>) -> Rc<Owner<Field>> {
    let codec = Rc::new(FieldCodec(calls));
    let o = h
        .host
        .enroll(
            &h.kernel,
            h.spec(3, vec![dep(d, "query")]),
            Field::new(7, 11),
            Backend::Wasm {
                codec: codec.clone(),
                slot: 0,
                binding: h.binding(),
            },
        )
        .unwrap();
    o.admit_compatible_wasm(&h.kernel, [3; 32], codec, 0, h.binding())
        .unwrap();
    h.admit(&o);
    o
}
