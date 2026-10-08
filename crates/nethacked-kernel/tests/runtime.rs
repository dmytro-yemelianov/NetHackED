mod common;
use common::*;
use nethacked_kernel::*;
use std::collections::BTreeMap;

fn algorithm(op: &str, extra: bool) -> Vec<u8> {
    let command = if extra {
        r#"(func (export "bonus") (param i32 i32) (result i64)
        i32.const 0 i32.const 0 i32.const 0 i32.const 256 i32.const 4 call $call drop
        i32.const 8 i32.const 256 i32.load i32.const 10 i32.add i32.store
        i32.const 1 i32.const 8 i32.const 4 i32.const 256 i32.const 4 call $call drop
        i64.const 1099511627780)"#
    } else {
        ""
    };
    wat::parse_str(format!(
        r#"(module
        (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 1)
        (func (export "alloc") (param i32) (result i32) i32.const 0)
        (func (export "run") (param $ptr i32) (param i32) (result i64)
          i32.const 0 i32.const 0 i32.const 0 i32.const 256 i32.const 4 call $call drop
          i32.const 8 i32.const 256 i32.load local.get $ptr i32.load {op} i32.store
          i32.const 1 i32.const 8 i32.const 4 i32.const 256 i32.const 4 call $call drop
          i64.const 1099511627780) {command})"#
    ))
    .unwrap()
}

#[test]
fn computational_live_extension_and_added_command_preserve_populated_host() {
    let mut k = Kernel::new(Limits::default()).unwrap();
    let (_, state) = device(&mut k);
    let mut m = manifest("algorithm", Access::Write);
    m.dependencies = vec![
        dependency("device", "read", Access::Read),
        dependency("device", "set", Access::Write),
    ];
    m.extension_points.insert("run".into());
    let base = k
        .register_guest(m.clone(), &algorithm("i32.add", false))
        .unwrap();
    grant(&mut k, base, 0, Access::Read, &["scalar"]);
    grant(&mut k, base, 1, Access::Write, &["scalar"]);
    assert_eq!(
        number(
            &k.invoke(base, "run", &bytes(2), trusted(Access::Write))
                .unwrap()
        ),
        8
    );
    let original_artifact = k.artifact(base).unwrap();
    m.id = "extension".into();
    m.extension_points.clear();
    m.exports.insert("bonus".into(), endpoint(Access::Write));
    m.extension = Some(Extension {
        target: dependency("algorithm", "run", Access::Write).capability,
        export: "run".into(),
    });
    let extension = k.register_guest(m, &algorithm("i32.mul", true)).unwrap();
    grant(&mut k, extension, 0, Access::Read, &["scalar"]);
    grant(&mut k, extension, 1, Access::Write, &["scalar"]);
    assert_eq!(
        number(
            &k.invoke(base, "run", &bytes(3), trusted(Access::Write))
                .unwrap()
        ),
        24
    );
    assert_eq!(
        number(
            &k.invoke(extension, "bonus", &[], trusted(Access::Write))
                .unwrap()
        ),
        34
    );
    // A guest consumer routes through the existing provider identity to the extension.
    let mut consumer = manifest("consumer", Access::Write);
    consumer
        .dependencies
        .push(dependency("algorithm", "run", Access::Write));
    let consumer = k.register_guest(consumer, &forward(0)).unwrap();
    grant(&mut k, consumer, 0, Access::Write, &["scalar"]);
    assert_eq!(
        number(
            &k.invoke(consumer, "run", &bytes(2), trusted(Access::Write))
                .unwrap()
        ),
        68
    );
    assert_eq!(k.artifact(base).unwrap(), original_artifact);
    let state = state.borrow();
    assert_eq!(state.history, vec![6, 8, 24, 34, 68]);
    assert_eq!(state.unrelated, "populated-before-extension");
    assert_eq!(
        state.callers,
        vec![
            Some(base),
            Some(extension),
            Some(extension),
            Some(extension)
        ]
    );
    assert_eq!(state.artifacts[3], Some(k.artifact(extension).unwrap()));
}

#[test]
fn canonical_artifacts_bind_bytes_manifest_slot_order_and_configuration() {
    assert_eq!(
        serde_json::to_vec(&serde_json::json!({"z": 1, "a": 2})).unwrap(),
        br#"{"a":2,"z":1}"#
    );
    let mut a = Kernel::new(Limits::default()).unwrap();
    let mut b = Kernel::new(Limits::default()).unwrap();
    let mut m = manifest("native", Access::Read);
    m.exports.insert("second".into(), endpoint(Access::Read));
    let mut p = ports(1);
    p.insert("second".into(), ports(2).remove("run").unwrap());
    let x = a.register_native(m.clone(), b"code", p.clone()).unwrap();
    let y = b.register_native(m.clone(), b"code", p.clone()).unwrap();
    assert_eq!(a.artifact(x).unwrap(), b.artifact(y).unwrap());
    assert_eq!(b.artifact(x), Err(Error::InvalidHandle));
    b.unregister(y).unwrap();
    let changed = b
        .register_native(m.clone(), b"changed-code", p.clone())
        .unwrap();
    assert_ne!(a.artifact(x).unwrap(), b.artifact(changed).unwrap());
    b.unregister(changed).unwrap();
    m.exports.get_mut("second").unwrap().major = 2;
    let changed = b.register_native(m, b"code", p).unwrap();
    assert_ne!(a.artifact(x).unwrap(), b.artifact(changed).unwrap());
    let mut c = Kernel::new(Limits {
        fuel: 999,
        ..Limits::default()
    })
    .unwrap();
    let z = c
        .register_native(manifest("simple", Access::Read), b"code", ports(1))
        .unwrap();
    let z2 = a
        .register_native(manifest("simple", Access::Read), b"code", ports(1))
        .unwrap();
    assert_ne!(a.artifact(z2).unwrap(), c.artifact(z).unwrap());
    let mut one = manifest("ordered", Access::Read);
    one.dependencies = vec![
        dependency("native", "run", Access::Read),
        dependency("native", "second", Access::Read),
    ];
    let mut two = one.clone();
    two.dependencies.reverse();
    let h1 = a.register_guest(one, &forward(0)).unwrap();
    let h1_hash = a.artifact(h1).unwrap();
    a.unregister(h1).unwrap();
    let h2 = a.register_guest(two, &forward(0)).unwrap();
    assert_ne!(a.artifact(h2).unwrap(), h1_hash);
    let old_hash = a.artifact(h2).unwrap();
    let same_manifest = a.manifest(h2).unwrap().clone();
    a.unregister(h2).unwrap();
    let altered = a
        .register_guest(same_manifest, &wasm("i64.const 0"))
        .unwrap();
    assert_ne!(a.artifact(altered).unwrap(), old_hash);
}

#[test]
fn admission_failures_do_not_publish_or_replace_extensions() {
    let mut k = Kernel::new(Limits::default()).unwrap();
    let mut host = manifest("host", Access::Read);
    host.extension_points.insert("run".into());
    let h = k.register_native(host.clone(), b"host", ports(7)).unwrap();
    assert_eq!(
        k.register_native(host, b"duplicate", ports(8)),
        Err(Error::Duplicate)
    );
    let mut ext = manifest("ext", Access::Read);
    ext.extension = Some(Extension {
        target: dependency("host", "run", Access::Read).capability,
        export: "run".into(),
    });
    let mut bad = ext.clone();
    bad.extension.as_mut().unwrap().target.endpoint = "missing".into();
    assert_eq!(
        k.register_guest(bad, &forward(0)),
        Err(Error::MissingDependency)
    );
    let mut bad = ext.clone();
    bad.exports.get_mut("run").unwrap().major = 2;
    assert_eq!(k.register_guest(bad, &forward(0)), Err(Error::Signature));
    let mut bad = ext.clone();
    bad.dependencies
        .push(dependency("absent", "run", Access::Read));
    assert_eq!(
        k.register_guest(bad, &forward(0)),
        Err(Error::MissingDependency)
    );
    let e = k
        .register_native(ext.clone(), b"extension", ports(11))
        .unwrap();
    assert_eq!(
        number(&k.invoke(h, "run", &[], trusted(Access::Read)).unwrap()),
        11
    );
    ext.id = "duplicate-point".into();
    assert_eq!(
        k.register_native(ext, b"other", ports(99)),
        Err(Error::Duplicate)
    );
    k.unregister(e).unwrap();
    assert_eq!(
        number(&k.invoke(h, "run", &[], trusted(Access::Read)).unwrap()),
        7
    );
    let plain = k
        .register_native(manifest("plain", Access::Read), b"plain", ports(1))
        .unwrap();
    let mut bad = manifest("bad-point", Access::Read);
    bad.extension = Some(Extension {
        target: dependency("plain", "run", Access::Read).capability,
        export: "run".into(),
    });
    assert_eq!(
        k.register_native(bad, b"bad", ports(1)),
        Err(Error::InvalidManifest)
    );
    assert_eq!(
        number(&k.invoke(plain, "run", &[], trusted(Access::Read)).unwrap()),
        1
    );
    let mut bad = manifest("bad-major", Access::Read);
    let mut dep = dependency("host", "run", Access::Read);
    dep.capability.major = 9;
    bad.dependencies.push(dep);
    assert_eq!(
        k.register_native(bad, b"bad", ports(1)),
        Err(Error::MissingDependency)
    );
    assert_eq!(
        k.register_native(
            manifest("wrong-port", Access::Read),
            b"bad",
            BTreeMap::new()
        ),
        Err(Error::Signature)
    );
}
