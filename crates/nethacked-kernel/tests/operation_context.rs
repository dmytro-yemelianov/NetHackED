use nethacked_kernel::*;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
fn authority() -> Authority {
    Authority {
        access: Access::Write,
        scope: Scope::Keys(BTreeSet::new()),
    }
}
fn manifest(id: &str, target: Option<&str>) -> Manifest {
    Manifest {
        id: id.into(),
        exports: BTreeMap::from([(
            "run".into(),
            Endpoint {
                major: 1,
                access: Access::Write,
                max_request: 256,
                max_reply: 256,
            },
        )]),
        dependencies: target
            .map(|t| {
                vec![Dependency {
                    capability: Capability {
                        provider: t.into(),
                        endpoint: "run".into(),
                        major: 1,
                    },
                    access: Access::Write,
                }]
            })
            .unwrap_or_default(),
        extension_points: BTreeSet::new(),
        extension: None,
    }
}
fn forwarding() -> Vec<u8> {
    wat::parse_str(r#"(module
 (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
 (memory (export "memory") 1) (func (export "alloc") (param i32) (result i32) i32.const 0)
 (func (export "run") (param i32 i32) (result i64) (local $n i32)
 i32.const 0 local.get 0 local.get 1 i32.const 4096 i32.const 256 call $call local.set $n
 local.get $n i32.const 0 i32.lt_s if i32.const 4096 local.get $n i32.store i32.const 4 local.set $n end
 i64.const 17592186044416 local.get $n i64.extend_i32_u i64.or))"#).unwrap()
}
#[test]
fn private_binding_survives_nested_guest_calls_and_plain_calls_have_none() {
    let mut k = Kernel::new(Limits::default()).unwrap();
    let seen = Rc::new(RefCell::new(vec![]));
    let sink = seen.clone();
    let port: NativePort = Rc::new(move |_, v| {
        sink.borrow_mut().push(v.operation().cloned());
        Ok(b"ok".to_vec())
    });
    let native = k
        .register_native(
            manifest("native", None),
            b"native",
            BTreeMap::from([("run".into(), port)]),
        )
        .unwrap();
    let inner = k
        .register_guest(manifest("inner", Some("native")), &forwarding())
        .unwrap();
    let outer = k
        .register_guest(manifest("outer", Some("inner")), &forwarding())
        .unwrap();
    k.issue_grant(inner, 0, authority()).unwrap();
    k.issue_grant(outer, 0, authority()).unwrap();
    let (id, lease) = k.capture_operation(outer, "run", authority()).unwrap();
    let binding = k.operation_binding(id, lease, 7, "phase").unwrap();
    assert_eq!(
        k.invoke_operation(&binding, b"forged JSON context", authority())
            .unwrap(),
        b"ok"
    );
    assert_eq!(seen.borrow().as_slice(), &[Some(binding.clone())]);
    k.invoke_pipeline(lease, b"plain", authority()).unwrap();
    k.invoke(outer, "run", b"plain", authority()).unwrap();
    assert_eq!(
        seen.borrow().as_slice(),
        &[Some(binding.clone()), None, None]
    );
    let before = k.lifecycle_state();
    let calls = seen.borrow().clone();
    assert_eq!(
        k.invoke_operation(&binding, &[0; 257], authority()),
        Err(Error::Buffer)
    );
    assert_eq!(k.lifecycle_state(), before);
    assert_eq!(*seen.borrow(), calls);
    let other = Kernel::new(Limits::default()).unwrap();
    assert_eq!(
        other.invoke_operation(&binding, b"", authority()),
        Err(Error::Authority)
    );
    assert_eq!(
        other.operation_binding(id, lease, 7, "phase"),
        Err(Error::Authority)
    );
    assert_eq!(k.managed_unload(native), Err(Error::Referenced));
    k.release_pipeline(lease).unwrap();
}
