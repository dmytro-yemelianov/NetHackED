mod common;
use common::*;
use nethacked_kernel::*;

#[test]
fn admission_rejects_forbidden_imports_starts_signatures_and_initial_limits() {
    // Each policy fixture has a valid ABI, so missing exports cannot mask it.
    let abi = r#"(memory (export "memory") 1)
        (func (export "alloc") (param i32) (result i32) i32.const 0)
        (func (export "run") (param i32 i32) (result i64) i64.const 0)"#;
    let mut k = Kernel::new(Limits::default()).unwrap();
    let control = wat::parse_str(format!("(module {abi})")).unwrap();
    let valid = k
        .register_guest(manifest("valid-control", Access::Read), &control)
        .unwrap();
    assert_eq!(
        k.invoke(valid, "run", &[], trusted(Access::Read)),
        Ok(vec![])
    );
    k.unregister(valid).unwrap();
    for (name, declaration, expected) in [
        (
            "wasi",
            r#"(import "wasi_snapshot_preview1" "fd_write" (func))"#,
            Error::Signature,
        ),
        (
            "ambient",
            r#"(import "env" "service" (func))"#,
            Error::Signature,
        ),
        (
            "wrong-import",
            r#"(import "kernel" "call" (func (param i32) (result i32)))"#,
            Error::Signature,
        ),
        (
            "duplicate-import",
            r#"(import "kernel" "call" (func (param i32 i32 i32 i32 i32) (result i32)))
                (import "kernel" "call" (func (param i32 i32 i32 i32 i32) (result i32)))"#,
            Error::Signature,
        ),
        (
            "start",
            "(func $start) (start $start)",
            Error::InvalidModule,
        ),
    ] {
        let bytes = wat::parse_str(format!("(module {declaration} {abi})")).unwrap();
        assert_eq!(
            k.register_guest(manifest(name, Access::Read), &bytes),
            Err(expected),
            "{name}"
        );
        // Admission refusal leaves the same ID available for a valid guest.
        let admitted = k
            .register_guest(manifest(name, Access::Read), &control)
            .unwrap();
        k.unregister(admitted).unwrap();
    }
    let cases = [
        (
            "missing-memory",
            r#"(module (func (export "alloc") (param i32) (result i32) i32.const 0) (func (export "run") (param i32 i32) (result i64) i64.const 0))"#,
        ),
        (
            "wrong-alloc",
            r#"(module (memory (export "memory") 1) (func (export "alloc") (result i32) i32.const 0) (func (export "run") (param i32 i32) (result i64) i64.const 0))"#,
        ),
        (
            "wrong-export",
            r#"(module (memory (export "memory") 1) (func (export "alloc") (param i32) (result i32) i32.const 0) (func (export "run") (param i32 i32) (result i32) i32.const 0))"#,
        ),
        (
            "undeclared-export",
            r#"(module (memory (export "memory") 1) (func (export "alloc") (param i32) (result i32) i32.const 0) (func (export "run") (param i32 i32) (result i64) i64.const 0) (func (export "ambient")))"#,
        ),
        (
            "initial-memory",
            r#"(module (memory (export "memory") 5) (func (export "alloc") (param i32) (result i32) i32.const 0) (func (export "run") (param i32 i32) (result i64) i64.const 0))"#,
        ),
        (
            "initial-table",
            r#"(module (memory (export "memory") 1) (table 129 funcref) (func (export "alloc") (param i32) (result i32) i32.const 0) (func (export "run") (param i32 i32) (result i64) i64.const 0))"#,
        ),
        (
            "table-count",
            r#"(module (memory (export "memory") 1) (table 1 funcref) (table 1 funcref) (func (export "alloc") (param i32) (result i32) i32.const 0) (func (export "run") (param i32 i32) (result i64) i64.const 0))"#,
        ),
        (
            "bad-data",
            r#"(module (memory (export "memory") 1) (data (i32.const 65536) "x") (func (export "alloc") (param i32) (result i32) i32.const 0) (func (export "run") (param i32 i32) (result i64) i64.const 0))"#,
        ),
    ];
    for (name, wat) in cases {
        let bytes = wat::parse_str(wat).unwrap();
        assert!(
            k.register_guest(manifest(name, Access::Read), &bytes)
                .is_err(),
            "{name}"
        );
        // Refused registration did not reserve the ID or consume capacity.
        let h = k
            .register_native(manifest(name, Access::Read), b"native", ports(17))
            .unwrap();
        assert_eq!(
            number(&k.invoke(h, "run", &[], trusted(Access::Read)).unwrap()),
            17
        );
        k.unregister(h).unwrap();
    }
    assert_eq!(
        k.register_guest(manifest("malformed", Access::Read), b"not wasm"),
        Err(Error::InvalidModule)
    );
    let mut m = manifest("missing-callable", Access::Read);
    m.exports.insert("absent".into(), endpoint(Access::Read));
    assert_eq!(
        k.register_guest(m, &wasm("i64.const 0")),
        Err(Error::Signature)
    );
}

#[test]
fn buffer_preflight_refuses_before_mutating_native_calls() {
    let mut k = Kernel::new(Limits::default()).unwrap();
    let (_, state) = device(&mut k);
    let before = state.borrow().clone();
    for (index, args) in [
        [-1, 4, 256, 4],
        [65535, 4, 256, 4],
        [0, -1, 256, 4],
        [0, 5, 256, 4],
        [0, 4, -1, 4],
        [0, 4, 65535, 4],
        [0, 4, 256, -1],
        [0, 4, 256, 3],
        [0, 4, 256, 65537],
        [i32::MAX, i32::MAX, 256, 4],
    ]
    .into_iter()
    .enumerate()
    {
        let [rp, rl, sp, sc] = args;
        let body = format!("i32.const 512 i32.const 0 i32.const {rp} i32.const {rl} i32.const {sp} i32.const {sc} call $call i32.store i64.const 2199023255556");
        let mut m = manifest(&format!("buffer-{index}"), Access::Write);
        m.dependencies
            .push(dependency("device", "set", Access::Write));
        let h = k.register_guest(m, &wasm(&body)).unwrap();
        grant(&mut k, h, 0, Access::Write, &["scalar"]);
        let reply = k
            .invoke(h, "run", &bytes(45), trusted(Access::Write))
            .unwrap();
        assert_eq!(
            number(&reply) as i32,
            Error::Buffer.abi_code(),
            "case {index}"
        );
        assert_eq!(*state.borrow(), before, "case {index}");
    }
}

#[test]
fn allocation_execution_output_and_growth_are_bounded() {
    let mut k = Kernel::new(Limits {
        fuel: 1000,
        wasm_recursion: 8,
        stack_bytes: 256,
        ..Limits::default()
    })
    .unwrap();
    for (id, body, error) in [
        ("loop", "(loop $again br $again) i64.const 0", Error::Trap),
        (
            "grow",
            "i32.const 20 memory.grow drop i64.const 0",
            Error::Trap,
        ),
        ("bad-pointer", "i64.const 281474976710660", Error::Buffer),
        ("bad-length", "i64.const 5", Error::Buffer),
        ("unsigned-pointer", "i64.const -4294967292", Error::Buffer),
        ("unsigned-length", "i64.const 4294967295", Error::Buffer),
    ] {
        let h = k
            .register_guest(manifest(id, Access::Read), &wasm(body))
            .unwrap();
        assert_eq!(
            k.invoke(h, "run", &[], trusted(Access::Read)),
            Err(error),
            "{id}"
        );
    }
    for (id, alloc) in [
        ("alloc-pointer", "i32.const -1"),
        ("alloc-fuel", "(loop $again br $again) i32.const 0"),
    ] {
        let wasm_bytes = wat::parse_str(format!(
            r#"(module (memory (export "memory") 1)
            (func (export "alloc") (param i32) (result i32) {alloc})
            (func (export "run") (param i32 i32) (result i64) i64.const 0))"#
        ))
        .unwrap();
        let h = k
            .register_guest(manifest(id, Access::Read), &wasm_bytes)
            .unwrap();
        let expected = if id == "alloc-pointer" {
            Error::Buffer
        } else {
            Error::Trap
        };
        assert_eq!(
            k.invoke(h, "run", &bytes(1), trusted(Access::Read)),
            Err(expected)
        );
    }
    let recursive = wat::parse_str(r#"(module (memory (export "memory") 1)
        (func (export "alloc") (param i32) (result i32) i32.const 0)
        (func $run (export "run") (param i32 i32) (result i64) local.get 0 local.get 1 call $run))"#).unwrap();
    let h = k
        .register_guest(manifest("recursive", Access::Read), &recursive)
        .unwrap();
    assert_eq!(
        k.invoke(h, "run", &[], trusted(Access::Read)),
        Err(Error::Trap)
    );
    let table = wat::parse_str(r#"(module (memory (export "memory") 1) (table 1 funcref)
        (func (export "alloc") (param i32) (result i32) i32.const 0)
        (func (export "run") (param i32 i32) (result i64) ref.null func i32.const 128 table.grow drop i64.const 0))"#).unwrap();
    let h = k
        .register_guest(manifest("table-grow", Access::Read), &table)
        .unwrap();
    assert_eq!(
        k.invoke(h, "run", &[], trusted(Access::Read)),
        Err(Error::Trap)
    );
    let h = k
        .register_guest(manifest("request", Access::Read), &wasm("i64.const 0"))
        .unwrap();
    assert_eq!(
        k.invoke(h, "run", &[0; 5], trusted(Access::Read)),
        Err(Error::Buffer)
    );
    let mut tiny = Kernel::new(Limits {
        module_bytes: 8,
        ..Limits::default()
    })
    .unwrap();
    assert_eq!(
        tiny.register_guest(manifest("large", Access::Read), &wasm("i64.const 0")),
        Err(Error::Limit)
    );
}

#[test]
fn guest_ram_is_fresh_per_call() {
    let mut k = Kernel::new(Limits::default()).unwrap();
    let h = k.register_guest(manifest("ram", Access::Read), &wasm("i32.const 256 i32.const 256 i32.load i32.const 1 i32.add i32.store i64.const 1099511627780")).unwrap();
    assert_eq!(
        number(&k.invoke(h, "run", &[], trusted(Access::Read)).unwrap()),
        1
    );
    assert_eq!(
        number(&k.invoke(h, "run", &[], trusted(Access::Read)).unwrap()),
        1
    );
}
