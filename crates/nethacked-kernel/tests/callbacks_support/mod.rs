#![allow(dead_code)]
use super::kernel_api::*;
use serde::Deserialize;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

pub fn key(name: &str) -> Resource {
    Resource {
        owner: "device".into(),
        key: name.into(),
    }
}
pub fn auth() -> Authority {
    Authority {
        access: Access::Write,
        scope: Scope::Keys(BTreeSet::from([key("value")])),
    }
}
pub fn cap(provider: &str, endpoint: &str) -> Capability {
    Capability {
        provider: provider.into(),
        endpoint: endpoint.into(),
        major: 1,
    }
}
pub fn dep(provider: &str, endpoint: &str) -> Dependency {
    Dependency {
        capability: cap(provider, endpoint),
        access: Access::Write,
    }
}
pub fn ep() -> Endpoint {
    Endpoint {
        major: 1,
        access: Access::Write,
        max_request: 4096,
        max_reply: 4096,
    }
}
pub fn manifest(id: &str, exports: &[&str]) -> Manifest {
    Manifest {
        id: id.into(),
        exports: exports.iter().map(|e| ((*e).into(), ep())).collect(),
        dependencies: vec![],
        extension_points: BTreeSet::new(),
        extension: None,
    }
}
pub fn call(slot: u32, add: u32) -> String {
    format!(
        r#"local.get $p local.get $p i32.load i32.const {add} i32.add i32.store
        i32.const {slot} local.get $p local.get $n i32.const 4096 i32.const 4096 call $call local.set $r
        local.get $r i32.const 0 i32.lt_s if i32.const 4096 local.get $r i32.store i64.const 17592186044420 return end
        i64.const 17592186044416 local.get $r i64.extend_i32_u i64.or"#
    )
}
pub fn guest_code(trap: bool) -> Vec<u8> {
    let run = call(0, 0);
    let callback = call(1, 1);
    wat::parse_str(format!(r#"(module
      (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (func (export "alloc") (param i32) (result i32) i32.const 0)
      (func (export "run") (param $p i32) (param $n i32) (result i64) (local $r i32)
        {run} {tail})
      (func (export "callback") (param $p i32) (param $n i32) (result i64) (local $r i32) {callback}))"#,
      tail=if trap { "drop unreachable" } else { "" })).unwrap()
}
pub fn extension_code(add: u32) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(module
      (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (func (export "alloc") (param i32) (result i32) i32.const 0)
      (func (export "callback") (param $p i32) (param $n i32) (result i64) (local $r i32) {}))"#,
        call(0, add)
    ))
    .unwrap()
}
#[derive(Clone, Copy, Debug)]
pub enum Mode {
    Store,
    Drop,
    DropRefuse,
    StoreRefuse,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    endpoint: String,
    ceiling: Authority,
}
pub fn request(endpoint: &str, ceiling: Authority) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({ "endpoint": endpoint, "ceiling": ceiling })).unwrap()
}
pub fn abi(bytes: &[u8]) -> i32 {
    i32::from_le_bytes(bytes.try_into().unwrap())
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Observation {
    pub value: u32,
    pub binding: Option<OperationBinding>,
    pub caller: ModuleHandle,
    pub artifact: [u8; 32],
}
pub struct Host {
    pub k: Kernel,
    pub schedule: ModuleHandle,
    pub observer: ModuleHandle,
    pub actor: ModuleHandle,
    pub stored: Rc<RefCell<Vec<RetainedCallback>>>,
    pub mode: Rc<RefCell<Mode>>,
    pub state: Rc<RefCell<BTreeMap<Resource, Vec<u8>>>>,
    pub observations: Rc<RefCell<Vec<Observation>>>,
}
impl Host {
    pub fn new(trap: bool) -> Self {
        let mut k = Kernel::new(Limits::default()).unwrap();
        let stored = Rc::new(RefCell::new(vec![]));
        let mode = Rc::new(RefCell::new(Mode::Store));
        let captures = stored.clone();
        let choice = mode.clone();
        let schedule = k
            .register_native(
                manifest("schedule", &["run"]),
                b"schedule-v1",
                BTreeMap::from([(
                    "run".into(),
                    Rc::new(move |bytes: &[u8], view: &CallerView| {
                        let req: Request =
                            serde_json::from_slice(bytes).map_err(|_| Error::NativeRefusal)?;
                        let handle = view.capture_callback(&req.endpoint, req.ceiling)?;
                        let mode = *choice.try_borrow().map_err(|_| Error::NativeRefusal)?;
                        match mode {
                            Mode::Store | Mode::StoreRefuse => captures
                                .try_borrow_mut()
                                .map_err(|_| Error::NativeRefusal)?
                                .push(handle),
                            _ => drop(handle),
                        }
                        match mode {
                            Mode::DropRefuse | Mode::StoreRefuse => Err(Error::NativeRefusal),
                            _ => Ok(0i32.to_le_bytes().to_vec()),
                        }
                    }) as NativePort,
                )]),
            )
            .unwrap();
        let state = Rc::new(RefCell::new(BTreeMap::from([
            (key("value"), 10u32.to_le_bytes().to_vec()),
            (key("unrelated"), b"untouched".to_vec()),
        ])));
        let observations = Rc::new(RefCell::new(vec![]));
        let seen = observations.clone();
        let values = state.clone();
        let mut observer_manifest = manifest("observer", &["run"]);
        observer_manifest.extension_points.insert("run".into());
        let observer = k
            .register_native(
                observer_manifest,
                b"observer-v1",
                BTreeMap::from([(
                    "run".into(),
                    Rc::new(move |bytes: &[u8], view: &CallerView| {
                        if !view.authority().scope.allows(&key("value")) {
                            return Err(Error::Authority);
                        }
                        let value =
                            u32::from_le_bytes(bytes.try_into().map_err(|_| Error::NativeRefusal)?);
                        let record = Observation {
                            value,
                            binding: view.operation().cloned(),
                            caller: view.caller().ok_or(Error::Authority)?,
                            artifact: view.caller_artifact().ok_or(Error::Authority)?,
                        };
                        values
                            .borrow_mut()
                            .insert(key("value"), value.to_le_bytes().to_vec());
                        seen.borrow_mut().push(record);
                        Ok(value.to_le_bytes().to_vec())
                    }) as NativePort,
                )]),
            )
            .unwrap();
        let mut m = manifest("actor", &["run", "callback"]);
        m.dependencies = vec![dep("schedule", "run"), dep("observer", "run")];
        m.extension_points.insert("callback".into());
        m.extension_points.insert("run".into());
        let actor = k.register_guest(m, &guest_code(trap)).unwrap();
        for slot in [0, 1] {
            k.issue_grant(actor, slot, auth()).unwrap();
        }
        Self {
            k,
            schedule,
            observer,
            actor,
            stored,
            mode,
            state,
            observations,
        }
    }
    pub fn source(&mut self) -> (OperationId, PipelineLease, OperationBinding) {
        let (op, lease) = self.k.capture_operation(self.actor, "run", auth()).unwrap();
        let binding = self.k.operation_binding(op, lease, 17, "schedule").unwrap();
        (op, lease, binding)
    }
    pub fn capture(&mut self) -> (RetainedCallback, OperationBinding) {
        let (_, lease, source) = self.source();
        assert_eq!(
            abi(&self
                .k
                .invoke_operation(&source, &request("callback", auth()), auth())
                .unwrap()),
            0
        );
        let callback = self.stored.borrow_mut().pop().unwrap();
        self.k.release_pipeline(lease).unwrap();
        (callback, source)
    }
    pub fn extension(&mut self, id: &str, add: u32) -> ModuleHandle {
        let mut m = manifest(id, &["callback"]);
        m.dependencies.push(dep("observer", "run"));
        m.extension = Some(Extension {
            target: cap("actor", "callback"),
            export: "callback".into(),
        });
        let h = self.k.register_guest(m, &extension_code(add)).unwrap();
        self.k.issue_grant(h, 0, auth()).unwrap();
        h
    }
}
