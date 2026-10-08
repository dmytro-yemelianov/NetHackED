#![allow(dead_code)]
use nethacked_kernel::*;
use nethacked_resources::{OwnerPolicy, Plan, Service, Stamp, Write};
use std::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
pub const WIRE: u32 = 16384;
pub fn authority() -> Authority {
    Authority {
        access: Access::Write,
        scope: Scope::Keys(
            [address("charge"), address("time"), address("unrelated")]
                .into_iter()
                .collect(),
        ),
    }
}
pub fn cap(id: &str, endpoint: &str) -> Capability {
    Capability {
        provider: id.into(),
        endpoint: endpoint.into(),
        major: 1,
    }
}
pub fn endpoint() -> Endpoint {
    Endpoint {
        major: 1,
        access: Access::Write,
        max_request: WIRE,
        max_reply: WIRE,
    }
}
pub fn manifest(id: &str) -> Manifest {
    Manifest {
        id: id.into(),
        exports: BTreeMap::from([("step".into(), endpoint())]),
        dependencies: vec![],
        extension_points: BTreeSet::from(["step".into()]),
        extension: None,
    }
}
pub fn address(key: &str) -> Resource {
    Resource {
        owner: "ledger".into(),
        key: key.into(),
    }
}
pub fn bytes(b: &[u8]) -> String {
    b.iter().map(|b| format!("\\{b:02x}")).collect()
}
pub fn plan(stamp: Stamp, value: &[u8]) -> Plan {
    Plan {
        readonly: vec![],
        writes: vec![Write {
            stamp,
            replacement: Some(value.into()),
        }],
        events: vec![value.into()],
    }
}
pub struct Host {
    pub k: Kernel,
    pub service: Service,
    pub guard: ModuleHandle,
    pub charge: Stamp,
    pub time: Stamp,
}
impl Host {
    pub fn new() -> Self {
        let mut k = Kernel::new(Limits {
            payload_bytes: WIRE,
            ..Limits::default()
        })
        .unwrap();
        let service = Service::new(
            nethacked_resources::Limits {
                wire: WIRE,
                ..Default::default()
            },
            Rc::new(|_| true),
        )
        .unwrap();
        service.install(&mut k, "resources").unwrap();
        let guard = k
            .register_native(
                manifest("ledger"),
                b"ledger",
                BTreeMap::from([(
                    "step".into(),
                    Rc::new(|_: &[u8], _: &CallerView| Ok(vec![])) as NativePort,
                )]),
            )
            .unwrap();
        service
            .register_owner(
                "ledger",
                OwnerPolicy {
                    capability: cap("ledger", "step"),
                    provider: guard,
                    artifact: k.artifact(guard).unwrap(),
                    validate: Rc::new(|_| true),
                },
            )
            .unwrap();
        let charge = service.put(address("charge"), b"0".to_vec()).unwrap();
        let time = service.put(address("time"), b"0".to_vec()).unwrap();
        service
            .put(address("unrelated"), b"untouched".to_vec())
            .unwrap();
        Self {
            k,
            service,
            guard,
            charge,
            time,
        }
    }
    pub fn guest(&mut self, id: &str, source: &str, extension: Option<&str>) -> ModuleHandle {
        let mut m = manifest(id);
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
                capability: cap("ledger", "step"),
                access: Access::Write,
            },
        ];
        if let Some(target) = extension {
            m.extension = Some(Extension {
                target: cap(target, "step"),
                export: "step".into(),
            });
        }
        let h = self
            .k
            .register_guest(m, &wat::parse_str(source).unwrap())
            .unwrap();
        for slot in 0..3 {
            self.k.issue_grant(h, slot, authority()).unwrap();
        }
        h
    }
}
/// Real guest port wrapper. Refusals are returned as ABI bytes, not root traps.
pub fn transaction_guest() -> String {
    format!(
        r#"(module
 (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
 (memory (export "memory") 1) (func (export "alloc") (param i32) (result i32) i32.const 0)
 (func (export "step") (param $p i32) (param $n i32) (result i64) (local $r i32)
 local.get $p i32.load8_u i32.const 67 i32.eq
 local.get $p i32.const 1 i32.add local.get $n i32.const 1 i32.sub i32.const 32768 i32.const {WIRE} call $call local.set $r
 local.get $r i32.const 0 i32.lt_s if i32.const 32768 local.get $r i32.store i32.const 4 local.set $r end
 i64.const 140737488355328 local.get $r i64.extend_i32_u i64.or))"#
    )
}
pub fn command(kind: u8, json: &[u8]) -> Vec<u8> {
    let mut b = vec![kind];
    b.extend(json);
    b
}
pub fn abi(reply: Vec<u8>) -> std::result::Result<Vec<u8>, i32> {
    if reply.len() == 4 {
        let n = i32::from_le_bytes(reply.as_slice().try_into().unwrap());
        if n < 0 {
            return Err(n);
        }
    }
    Ok(reply)
}
