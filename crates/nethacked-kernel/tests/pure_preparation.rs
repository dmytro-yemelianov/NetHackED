use nethacked_kernel::*;
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

fn cap(provider: &str) -> Capability {
    Capability {
        provider: provider.into(),
        endpoint: "run".into(),
        major: 1,
    }
}
fn manifest(id: &str, access: Access, dependencies: Vec<Dependency>) -> Manifest {
    Manifest {
        id: id.into(),
        exports: BTreeMap::from([(
            "run".into(),
            Endpoint {
                major: 1,
                access,
                max_request: 32,
                max_reply: 32,
            },
        )]),
        dependencies,
        extension_points: BTreeSet::new(),
        extension: None,
    }
}
fn dependency(provider: &str) -> Dependency {
    Dependency {
        capability: cap(provider),
        access: Access::Read,
    }
}
fn authority() -> Authority {
    Authority {
        access: Access::Write,
        scope: Scope::All,
    }
}
fn grant(k: &mut Kernel, h: ModuleHandle) {
    k.issue_grant(
        h,
        0,
        Authority {
            access: Access::Read,
            scope: Scope::Keys(BTreeSet::new()),
        },
    )
    .unwrap();
}
fn algorithm(extra: &str, body: &str) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(module {extra}
        (memory (export "memory") 1)
        (func (export "alloc") (param i32) (result i32) i32.const 0)
        (func (export "run") (param $p i32) (param i32) (result i64)
            {body} i64.const 4))"#
    ))
    .unwrap()
}
fn add(n: u32) -> Vec<u8> {
    algorithm(
        "",
        &format!("i32.const 0 local.get $p i32.load i32.const {n} i32.add i32.store"),
    )
}
fn bridge(k: &mut Kernel, helper: &str, times: usize) -> ModuleHandle {
    k.register_native(
        manifest("bridge", Access::Write, vec![dependency(helper)]),
        b"bridge",
        BTreeMap::from([(
            "run".into(),
            Rc::new(move |request: &[u8], view: &CallerView| {
                let detached = view.clone();
                assert_eq!(
                    detached.call_pure(0, request).unwrap_err(),
                    Error::Authority
                );
                let mut bytes = request.to_vec();
                for _ in 0..times {
                    bytes = view.call_pure(0, &bytes)?.bytes;
                }
                Ok(bytes)
            }) as NativePort,
        )]),
    )
    .unwrap()
}
#[test]
fn pure_bridge_computes_and_detached_clone_has_no_dispatcher() {
    let mut k = Kernel::new(Limits::default()).unwrap();
    k.register_guest(manifest("helper", Access::Read, vec![]), &add(7))
        .unwrap();
    let b = bridge(&mut k, "helper", 2);
    grant(&mut k, b);
    assert_eq!(
        k.invoke(b, "run", &5u32.to_le_bytes(), authority())
            .unwrap(),
        19u32.to_le_bytes()
    );
    k.revoke_grant(b, 0).unwrap();
    assert_eq!(
        k.invoke(b, "run", &5u32.to_le_bytes(), authority()),
        Err(Error::Revoked)
    );
}
#[test]
fn even_read_native_descendants_are_refused_before_callback() {
    let mut k = Kernel::new(Limits::default()).unwrap();
    let count = Rc::new(Cell::new(0));
    let seen = count.clone();
    k.register_native(
        manifest("native", Access::Read, vec![]),
        b"read",
        BTreeMap::from([(
            "run".into(),
            Rc::new(move |_: &[u8], _: &CallerView| {
                seen.set(seen.get() + 1);
                Ok(vec![9u8])
            }) as NativePort,
        )]),
    )
    .unwrap();
    let guest = algorithm(r#"(import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))"#,
        "i32.const 64 i32.const 0 local.get $p i32.const 4 i32.const 128 i32.const 32 call $call i32.store i32.const 0 i32.const 64 i32.load i32.store");
    let g = k
        .register_guest(
            manifest("helper", Access::Read, vec![dependency("native")]),
            &guest,
        )
        .unwrap();
    grant(&mut k, g);
    let b = bridge(&mut k, "helper", 1);
    grant(&mut k, b);
    assert_eq!(
        k.invoke(b, "run", &5u32.to_le_bytes(), authority())
            .unwrap(),
        Error::Authority.abi_code().to_le_bytes()
    );
    assert_eq!(count.get(), 0);
    // Normal guest dispatch still invokes the Read native endpoint.
    assert_eq!(
        k.invoke(g, "run", &5u32.to_le_bytes(), authority())
            .unwrap(),
        1i32.to_le_bytes()
    );
    assert_eq!(count.get(), 1);
}
#[test]
fn nested_guest_calls_share_calls_depth_and_fuel() {
    for limits in [
        Limits {
            calls: 2,
            ..Limits::default()
        },
        Limits {
            depth: 2,
            ..Limits::default()
        },
    ] {
        let mut k = Kernel::new(limits).unwrap();
        k.register_guest(manifest("leaf", Access::Read, vec![]), &add(1))
            .unwrap();
        let bytes = algorithm(r#"(import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))"#,
            "i32.const 64 i32.const 0 local.get $p i32.const 4 i32.const 128 i32.const 32 call $call i32.store i32.const 0 i32.const 64 i32.load i32.store");
        let h = k
            .register_guest(
                manifest("helper", Access::Read, vec![dependency("leaf")]),
                &bytes,
            )
            .unwrap();
        grant(&mut k, h);
        let b = bridge(&mut k, "helper", 1);
        grant(&mut k, b);
        assert_eq!(
            k.invoke(b, "run", &5u32.to_le_bytes(), authority())
                .unwrap(),
            Error::Limit.abi_code().to_le_bytes()
        );
    }
    let mut k = Kernel::new(Limits {
        calls: 2,
        ..Limits::default()
    })
    .unwrap();
    k.register_guest(manifest("helper", Access::Read, vec![]), &add(1))
        .unwrap();
    let b = bridge(&mut k, "helper", 2);
    grant(&mut k, b);
    assert_eq!(
        k.invoke(b, "run", &5u32.to_le_bytes(), authority()),
        Err(Error::Limit)
    );
    // One actual loop fits; two helper calls in one invocation cannot refill fuel.
    let work = algorithm(
        "",
        "(local $i i32) i32.const 5000 local.set $i
        (loop $loop local.get $i i32.const 1 i32.sub local.tee $i br_if $loop)
        i32.const 0 local.get $p i32.load i32.const 1 i32.add i32.store",
    );
    for times in [1, 2] {
        let mut k = Kernel::new(Limits {
            fuel: 50_000,
            ..Limits::default()
        })
        .unwrap();
        k.register_guest(manifest("helper", Access::Read, vec![]), &work)
            .unwrap();
        let b = bridge(&mut k, "helper", times);
        grant(&mut k, b);
        let result = k.invoke(b, "run", &5u32.to_le_bytes(), authority());
        if times == 1 {
            assert_eq!(result.unwrap(), 6u32.to_le_bytes());
        } else {
            assert_eq!(result, Err(Error::Trap));
        }
    }
}
#[test]
fn retained_routes_and_live_grants_apply_and_pins_survive_until_drop() {
    let mut k = Kernel::new(Limits::default()).unwrap();
    let mut base = manifest("helper", Access::Read, vec![]);
    base.extension_points.insert("run".into());
    let h = k.register_guest(base, &add(1)).unwrap();
    let mut ext = manifest("old", Access::Read, vec![]);
    ext.extension = Some(Extension {
        target: cap("helper"),
        export: "run".into(),
    });
    let old = k.register_guest(ext, &add(7)).unwrap();
    let captured = Rc::new(RefCell::new(None));
    let sink = captured.clone();
    let b = k
        .register_native(
            manifest("bridge", Access::Write, vec![dependency("helper")]),
            b"retaining",
            BTreeMap::from([(
                "run".into(),
                Rc::new(move |r: &[u8], v: &CallerView| {
                    let reply = v.call_pure(0, r)?;
                    let bytes = reply.bytes.clone();
                    *sink.borrow_mut() = Some(reply);
                    Ok(bytes)
                }) as NativePort,
            )]),
        )
        .unwrap();
    grant(&mut k, b);
    let lease = k.capture_pipeline(b, "run", authority()).unwrap();
    k.managed_detach(old).unwrap();
    assert_eq!(
        k.invoke_pipeline(lease, &5u32.to_le_bytes(), authority())
            .unwrap(),
        12u32.to_le_bytes()
    );
    assert_eq!(
        captured
            .borrow()
            .as_ref()
            .unwrap()
            .binding
            .resolution
            .last()
            .unwrap()
            .handle,
        old
    );
    k.release_pipeline(lease).unwrap();
    assert_eq!(k.managed_unload(old), Err(Error::Referenced));
    captured.borrow_mut().take();
    k.managed_unload(old).unwrap();
    assert_eq!(
        k.invoke(b, "run", &5u32.to_le_bytes(), authority())
            .unwrap(),
        6u32.to_le_bytes()
    );
    k.revoke_grant(b, 0).unwrap();
    assert_eq!(
        k.invoke(b, "run", &5u32.to_le_bytes(), authority()),
        Err(Error::Revoked)
    );
    captured.borrow_mut().take();
    k.managed_unload(b).unwrap();
    k.managed_unload(h).unwrap();
}
#[test]
fn write_helpers_refuse_under_forced_read_ceiling() {
    let mut k = Kernel::new(Limits::default()).unwrap();
    k.register_guest(manifest("helper", Access::Write, vec![]), &add(7))
        .unwrap();
    let b = bridge(&mut k, "helper", 1);
    grant(&mut k, b);
    assert_eq!(
        k.invoke(b, "run", &5u32.to_le_bytes(), authority()),
        Err(Error::Authority)
    );
}
