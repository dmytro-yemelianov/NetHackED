#![allow(dead_code)]
use nethacked_kernel::*;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

pub fn endpoint(access: Access) -> Endpoint {
    Endpoint {
        major: 1,
        access,
        max_request: 4,
        max_reply: 4,
    }
}
pub fn manifest(id: &str, access: Access) -> Manifest {
    Manifest {
        id: id.into(),
        exports: BTreeMap::from([("run".into(), endpoint(access))]),
        dependencies: vec![],
        extension_points: BTreeSet::new(),
        extension: None,
    }
}
pub fn dependency(provider: &str, endpoint: &str, access: Access) -> Dependency {
    Dependency {
        capability: Capability {
            provider: provider.into(),
            endpoint: endpoint.into(),
            major: 1,
        },
        access,
    }
}
pub fn resource(key: &str) -> Resource {
    Resource {
        owner: "device".into(),
        key: key.into(),
    }
}
pub fn authority(access: Access, keys: &[&str]) -> Authority {
    Authority {
        access,
        scope: Scope::Keys(keys.iter().map(|key| resource(key)).collect()),
    }
}
pub fn trusted(access: Access) -> Authority {
    Authority {
        access,
        scope: Scope::All,
    }
}
pub fn bytes(n: u32) -> Vec<u8> {
    n.to_le_bytes().to_vec()
}
pub fn number(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes.try_into().unwrap())
}
pub fn wasm(body: &str) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(module
      (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (func (export "alloc") (param i32) (result i32) i32.const 0)
      (func (export "run") (param $ptr i32) (param $len i32) (result i64) {body}))"#
    ))
    .unwrap()
}
pub fn forward(slot: i32) -> Vec<u8> {
    wasm(&format!("i32.const {slot} local.get $ptr local.get $len i32.const 256 i32.const 4 call $call drop i64.const 1099511627780"))
}
pub fn grant(
    kernel: &mut Kernel,
    handle: ModuleHandle,
    slot: usize,
    access: Access,
    keys: &[&str],
) {
    kernel
        .issue_grant(handle, slot, authority(access, keys))
        .unwrap();
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Device {
    pub value: u32,
    pub unrelated: String,
    pub history: Vec<u32>,
    pub callers: Vec<Option<ModuleHandle>>,
    pub artifacts: Vec<Option<[u8; 32]>>,
}
pub fn device(kernel: &mut Kernel) -> (ModuleHandle, Rc<RefCell<Device>>) {
    let state = Rc::new(RefCell::new(Device {
        value: 6,
        unrelated: "populated-before-extension".into(),
        history: vec![6],
        callers: vec![],
        artifacts: vec![],
    }));
    let read_state = state.clone();
    let read: NativePort = Rc::new(move |input, view| {
        if !input.is_empty() && input.len() != 4 {
            return Err(Error::NativeRefusal);
        }
        if !view.authority().scope.allows(&resource("scalar")) {
            return Err(Error::Authority);
        }
        Ok(bytes(read_state.borrow().value))
    });
    let write_state = state.clone();
    let write: NativePort = Rc::new(move |input, view| {
        if input.len() != 4 {
            return Err(Error::NativeRefusal);
        }
        if view.authority().access != Access::Write
            || !view.authority().scope.allows(&resource("scalar"))
        {
            return Err(Error::Authority);
        }
        let value = number(input);
        if value > 1_000_000 {
            return Err(Error::NativeRefusal);
        }
        let mut state = write_state.borrow_mut();
        state.value = value;
        state.history.push(value);
        state.callers.push(view.caller());
        state.artifacts.push(view.caller_artifact());
        Ok(bytes(value))
    });
    let mut m = manifest("device", Access::Read);
    m.exports = BTreeMap::from([
        ("read".into(), endpoint(Access::Read)),
        ("set".into(), endpoint(Access::Write)),
    ]);
    let handle = kernel
        .register_native(
            m,
            b"device-port-v1",
            BTreeMap::from([("read".into(), read), ("set".into(), write)]),
        )
        .unwrap();
    (handle, state)
}
pub fn ports(value: u32) -> BTreeMap<String, NativePort> {
    let port: NativePort = Rc::new(move |_, _| Ok(bytes(value)));
    BTreeMap::from([("run".into(), port)])
}
