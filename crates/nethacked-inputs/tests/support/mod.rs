#![allow(dead_code)]
use nethacked_inputs::{Policy, Reply, Request, Service};
use nethacked_kernel::*;
use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
pub const WIRE: u32 = 16384;
pub fn resource() -> Resource {
    Resource {
        owner: "inputs".into(),
        key: "source".into(),
    }
}
pub fn authority() -> Authority {
    Authority {
        access: Access::Write,
        scope: Scope::Keys(BTreeSet::from([resource()])),
    }
}
pub fn cap() -> Capability {
    Capability {
        provider: "inputs".into(),
        endpoint: "input".into(),
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
        dependencies: vec![Dependency {
            capability: cap(),
            access: Access::Write,
        }],
        extension_points: BTreeSet::from(["step".into()]),
        extension: None,
    }
}
pub fn escaped(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("\\{b:02x}")).collect()
}
pub fn forward(extra: &str) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(module
      (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (func (export "alloc") (param i32) (result i32) i32.const 0)
      (func (export "step") (param $p i32) (param $n i32) (result i64) (local $r i32)
        local.get $n i32.eqz if
          i32.const 32768 i32.const 1885956979 i32.store
          i64.const 140737488355332 return end
        local.get $p i32.load8_u i32.const 32 i32.eq if (result i32)
          i32.const 1
          local.get $p i32.const 1 i32.add local.set $p
          local.get $n i32.const 1 i32.sub local.set $n
        else i32.const 0 end
        local.get $p local.get $n i32.const 32768 i32.const {WIRE} call $call local.set $r
        {extra}
        local.get $r i32.const 0 i32.lt_s if
          i32.const 32768 local.get $r i32.store i32.const 4 local.set $r end
        i64.const 140737488355328 local.get $r i64.extend_i32_u i64.or))"#
    ))
    .unwrap()
}
pub struct Host {
    pub k: Kernel,
    pub s: Service,
    pub calls: Rc<Cell<u32>>,
    pub guest: ModuleHandle,
    pub delegate: ModuleHandle,
    pub operation: OperationId,
    pub lease: PipelineLease,
}
impl Host {
    pub fn new() -> Self {
        Self::with(nethacked_inputs::Limits::default(), None)
    }
    pub fn with(limits: nethacked_inputs::Limits, policy: Option<Policy>) -> Self {
        let mut k = Kernel::new(Limits::default()).unwrap();
        let calls = Rc::new(Cell::new(0));
        let count = calls.clone();
        let policy = policy.unwrap_or(Policy {
            identity: "queue".into(),
            version: 1,
            transition: Rc::new(move |s, r| {
                count.set(count.get() + 1);
                if r == b"error" {
                    return Err(Error::NativeRefusal);
                }
                let (v, rest) = s.split_first().ok_or(Error::NativeRefusal)?;
                Ok((rest.to_vec(), vec![*v]))
            }),
        });
        let s = Service::new(limits, policy, vec![10, 20, 30, 40]).unwrap();
        s.install(&mut k, "inputs").unwrap();
        let delegate = k
            .register_guest(manifest("delegate"), &forward("nop"))
            .unwrap();
        k.issue_grant(delegate, 0, authority()).unwrap();
        let mut m = manifest("guest");
        m.dependencies.push(Dependency {
            capability: Capability {
                provider: "delegate".into(),
                endpoint: "step".into(),
                major: 1,
            },
            access: Access::Write,
        });
        let guest = k.register_guest(m, &forward("")).unwrap();
        k.issue_grant(guest, 0, authority()).unwrap();
        k.issue_grant(guest, 1, authority()).unwrap();
        let (operation, lease) = k.capture_operation(guest, "step", authority()).unwrap();
        Self {
            k,
            s,
            calls,
            guest,
            delegate,
            operation,
            lease,
        }
    }
    pub fn binding(&self, checkpoint: u64, phase: &str) -> OperationBinding {
        self.k
            .operation_binding(self.operation, self.lease, checkpoint, phase)
            .unwrap()
    }
    pub fn raw(&self, b: &OperationBinding, req: &[u8], a: Authority) -> Vec<u8> {
        self.k.invoke_operation(b, req, a).unwrap()
    }
    pub fn call(&self, b: &OperationBinding, req: &Request) -> Reply {
        serde_json::from_slice(&self.raw(b, &serde_json::to_vec(req).unwrap(), authority()))
            .unwrap()
    }
    pub fn refuse(&self, b: &OperationBinding, req: &Request, a: Authority) {
        let before = self.s.inspect().unwrap();
        let calls = self.calls.get();
        let out = self.raw(b, &serde_json::to_vec(req).unwrap(), a);
        assert_eq!(
            out.len(),
            4,
            "expected guest-visible kernel error, got {out:?}"
        );
        assert!(i32::from_le_bytes(out.try_into().unwrap()) < 0);
        assert_eq!(self.s.inspect().unwrap(), before);
        assert_eq!(self.calls.get(), calls);
    }
}
pub fn sample(r: &[u8]) -> Request {
    Request::Sample {
        request: r.to_vec(),
    }
}
pub fn replay(reply: &Reply, r: &[u8]) -> Request {
    Request::Replay {
        id: reply.id,
        request: r.to_vec(),
    }
}
