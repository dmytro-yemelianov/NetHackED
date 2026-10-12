mod common;
use common::*;
use nethacked_kernel::*;

fn status_body(slot: i32) -> String {
    format!("i32.const 512 i32.const {slot} local.get $ptr local.get $len i32.const 256 i32.const 4 call $call i32.store")
}

#[test]
fn call_count_and_depth_limits_keep_committed_prefixes() {
    let mut k = Kernel::new(Limits {
        calls: 3,
        ..Limits::default()
    })
    .unwrap();
    let (_, state) = device(&mut k);
    let mut m = manifest("calls", Access::Write);
    m.dependencies
        .push(dependency("device", "set", Access::Write));
    let body = format!(
        "{} {} {} i64.const 2199023255556",
        status_body(0),
        status_body(9),
        status_body(0)
    );
    let h = k.register_guest(m, &wasm(&body)).unwrap();
    grant(&mut k, h, 0, Access::Write, &["scalar"]);
    let reply = k
        .invoke(h, "run", &bytes(12), trusted(Access::Write))
        .unwrap();
    assert_eq!(number(&reply) as i32, Error::Limit.abi_code());
    assert_eq!(state.borrow().history, vec![6, 12]);
    let mut k = Kernel::new(Limits {
        depth: 2,
        ..Limits::default()
    })
    .unwrap();
    let (_, state) = device(&mut k);
    let mut middle = manifest("middle", Access::Write);
    middle
        .dependencies
        .push(dependency("device", "set", Access::Write));
    let middle = k
        .register_guest(
            middle,
            &wasm(&(status_body(0) + " i64.const 2199023255556")),
        )
        .unwrap();
    grant(&mut k, middle, 0, Access::Write, &["scalar"]);
    let mut outer = manifest("outer", Access::Write);
    outer
        .dependencies
        .push(dependency("middle", "run", Access::Write));
    let outer = k.register_guest(outer, &forward(0)).unwrap();
    grant(&mut k, outer, 0, Access::Write, &["scalar"]);
    let before = state.borrow().clone();
    let reply = k
        .invoke(outer, "run", &bytes(77), trusted(Access::Write))
        .unwrap();
    assert_eq!(number(&reply) as i32, Error::Limit.abi_code());
    assert_eq!(*state.borrow(), before);
}

#[test]
fn nested_guest_execution_shares_fuel_instead_of_refilling_each_store() {
    let mut k = Kernel::new(Limits {
        fuel: 2000,
        ..Limits::default()
    })
    .unwrap();
    let burner = wat::parse_str(
        r#"(module (memory (export "memory") 1)
        (func (export "alloc") (param i32) (result i32) i32.const 0)
        (func (export "run") (param i32 i32) (result i64) (local $n i32)
            i32.const 100 local.set $n
            (loop $again local.get $n i32.const 1 i32.sub local.tee $n br_if $again)
            i32.const 256 i32.const 7 i32.store i64.const 1099511627780))"#,
    )
    .unwrap();
    let provider = k
        .register_guest(manifest("burner", Access::Read), &burner)
        .unwrap();
    assert_eq!(
        number(
            &k.invoke(provider, "run", &[], trusted(Access::Read))
                .unwrap()
        ),
        7
    );
    let mut m = manifest("aggregate", Access::Read);
    m.dependencies
        .push(dependency("burner", "run", Access::Read));
    let calls =
        "i32.const 0 i32.const 0 i32.const 0 i32.const 256 i32.const 4 call $call drop ".repeat(20);
    let outer = k
        .register_guest(m, &wasm(&(calls + "i64.const 1099511627780")))
        .unwrap();
    grant(&mut k, outer, 0, Access::Read, &[]);
    assert_eq!(
        k.invoke(outer, "run", &[], trusted(Access::Read)),
        Err(Error::Trap)
    );
    // Independent invocations receive a new bounded budget.
    assert_eq!(
        number(
            &k.invoke(provider, "run", &[], trusted(Access::Read))
                .unwrap()
        ),
        7
    );
}

#[test]
fn manifest_scope_and_registration_limits_refuse_without_publication() {
    let mut k = Kernel::new(Limits {
        registrations: 1,
        manifest_entries: 1,
        scope_keys: 1,
        ..Limits::default()
    })
    .unwrap();
    let mut m = manifest("bad", Access::Read);
    m.exports.insert("extra".into(), endpoint(Access::Read));
    assert_eq!(
        k.register_native(m, b"bad", ports(1)),
        Err(Error::InvalidManifest)
    );
    let mut m = manifest("single", Access::Read);
    m.dependencies
        .push(dependency("single", "run", Access::Read));
    let h = k.register_guest(m, &wasm("i64.const 0")).unwrap();
    assert_eq!(
        k.issue_grant(h, 0, authority(Access::Read, &["a", "b"])),
        Err(Error::Authority)
    );
    assert_eq!(
        k.register_native(manifest("overflow", Access::Read), b"bad", ports(1)),
        Err(Error::Limit)
    );
    k.unregister(h).unwrap();
    let replacement = k
        .register_native(manifest("single", Access::Read), b"okay", ports(1))
        .unwrap();
    assert_ne!(h, replacement);
}
