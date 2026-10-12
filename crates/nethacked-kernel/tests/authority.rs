mod common;
use common::*;
use nethacked_kernel::*;

fn status_guest(body: &str) -> Vec<u8> {
    wasm(&format!(
        "i32.const 512 {body} i32.store i64.const 2199023255556"
    ))
}
fn status(k: &Kernel, h: ModuleHandle, access: Access, keys: &[&str]) -> i32 {
    number(
        &k.invoke(h, "run", &bytes(21), authority(access, keys))
            .unwrap(),
    ) as i32
}

#[test]
fn slots_live_grants_revocation_scope_and_atomic_port_refusal() {
    let mut k = Kernel::new(Limits::default()).unwrap();
    let (_, state) = device(&mut k);
    let mut m = manifest("caller", Access::Write);
    // Slot zero is readonly; slot one mutates. No guest-provided identity exists.
    m.dependencies = vec![
        dependency("device", "read", Access::Read),
        dependency("device", "set", Access::Write),
    ];
    let h = k
        .register_guest(
            m.clone(),
            &status_guest(
                "i32.const 1 local.get $ptr local.get $len i32.const 256 i32.const 4 call $call",
            ),
        )
        .unwrap();
    let before = state.borrow().clone();
    assert_eq!(
        status(&k, h, Access::Write, &["scalar"]),
        Error::MissingGrant.abi_code()
    );
    assert_eq!(*state.borrow(), before);
    assert_eq!(
        k.issue_grant(h, 7, authority(Access::Write, &["scalar"])),
        Err(Error::InvalidSlot)
    );
    assert_eq!(
        k.issue_grant(h, 0, authority(Access::Write, &["scalar"])),
        Err(Error::Authority)
    );
    assert_eq!(
        k.issue_grant(h, 1, trusted(Access::Write)),
        Err(Error::Authority)
    );
    grant(&mut k, h, 1, Access::Read, &["scalar"]);
    assert_eq!(
        status(&k, h, Access::Write, &["scalar"]),
        Error::Authority.abi_code()
    );
    assert_eq!(*state.borrow(), before);
    grant(&mut k, h, 1, Access::Write, &["scalar", "unrelated"]);
    assert_eq!(
        status(&k, h, Access::Write, &["other"]),
        Error::Authority.abi_code()
    );
    assert_eq!(*state.borrow(), before);
    assert_eq!(status(&k, h, Access::Write, &["scalar"]), 4);
    assert_eq!(state.borrow().value, 21);
    let committed = state.borrow().clone();
    k.revoke_grant(h, 1).unwrap();
    assert_eq!(
        status(&k, h, Access::Write, &["scalar"]),
        Error::Revoked.abi_code()
    );
    assert_eq!(*state.borrow(), committed);
    grant(&mut k, h, 1, Access::Write, &["scalar"]);
    let reply = k
        .invoke(
            h,
            "run",
            &bytes(1_000_001),
            authority(Access::Write, &["scalar"]),
        )
        .unwrap();
    assert_eq!(number(&reply) as i32, Error::NativeRefusal.abi_code());
    assert_eq!(*state.borrow(), committed);
    m.id = "bad-slot".into();
    let bad = k
        .register_guest(
            m,
            &status_guest(
                "i32.const -1 local.get $ptr local.get $len i32.const 256 i32.const 4 call $call",
            ),
        )
        .unwrap();
    assert_eq!(
        status(&k, bad, Access::Write, &["scalar"]),
        Error::InvalidSlot.abi_code()
    );
    assert_eq!(*state.borrow(), committed);
}

#[test]
fn handle_instances_generations_and_provider_replacement_cannot_inherit_grants() {
    let mut k = Kernel::new(Limits::default()).unwrap();
    let (provider, state) = device(&mut k);
    let mut m = manifest("caller", Access::Write);
    m.dependencies
        .push(dependency("device", "set", Access::Write));
    let caller = k
        .register_guest(
            m,
            &status_guest(
                "i32.const 0 local.get $ptr local.get $len i32.const 256 i32.const 4 call $call",
            ),
        )
        .unwrap();
    grant(&mut k, caller, 0, Access::Write, &["scalar"]);
    let mut other = Kernel::new(Limits::default()).unwrap();
    assert_eq!(
        other.invoke(caller, "run", &[], trusted(Access::Write)),
        Err(Error::InvalidHandle)
    );
    assert_eq!(
        other.issue_grant(caller, 0, authority(Access::Write, &["scalar"])),
        Err(Error::InvalidHandle)
    );
    k.unregister(provider).unwrap();
    assert_eq!(
        k.invoke(provider, "set", &bytes(5), trusted(Access::Write)),
        Err(Error::InvalidHandle)
    );
    let (replacement, replacement_state) = device(&mut k);
    assert_ne!(provider, replacement);
    let before = replacement_state.borrow().clone();
    assert_eq!(
        status(&k, caller, Access::Write, &["scalar"]),
        Error::InvalidHandle.abi_code()
    );
    assert_eq!(*replacement_state.borrow(), before);
    assert_eq!(state.borrow().value, 6);
    grant(&mut k, caller, 0, Access::Write, &["scalar"]);
    assert_eq!(status(&k, caller, Access::Write, &["scalar"]), 4);
    assert_eq!(replacement_state.borrow().value, 21);
    k.unregister(caller).unwrap();
    assert_eq!(k.artifact(caller), Err(Error::InvalidHandle));
}

#[test]
fn nested_read_ceiling_and_scope_intersection_cannot_expand_authority() {
    let mut k = Kernel::new(Limits::default()).unwrap();
    let (_, state) = device(&mut k);
    let mut provider = manifest("provider", Access::Write);
    provider
        .dependencies
        .push(dependency("device", "set", Access::Write));
    let middle = k
        .register_guest(
            provider.clone(),
            &status_guest(
                "i32.const 0 local.get $ptr local.get $len i32.const 256 i32.const 4 call $call",
            ),
        )
        .unwrap();
    grant(&mut k, middle, 0, Access::Write, &["scalar", "other"]);
    let mut outer = manifest("outer", Access::Write);
    outer
        .dependencies
        .push(dependency("provider", "run", Access::Write));
    let outer = k.register_guest(outer, &forward(0)).unwrap();
    grant(&mut k, outer, 0, Access::Write, &["other"]);
    let before = state.borrow().clone();
    assert_eq!(
        status(&k, outer, Access::Write, &["scalar", "other"]),
        Error::Authority.abi_code()
    );
    assert_eq!(*state.borrow(), before);
    grant(&mut k, outer, 0, Access::Write, &["scalar"]);
    assert_eq!(status(&k, outer, Access::Write, &["scalar"]), 4);
    assert_eq!(state.borrow().value, 21);
    let committed = state.borrow().clone();
    assert_eq!(
        k.invoke(outer, "run", &bytes(99), trusted(Access::Read)),
        Err(Error::Authority)
    );
    assert_eq!(*state.borrow(), committed);
    // An endpoint declared readonly enforces that ceiling even when the host
    // invokes it with write authority and the provider owns a write grant.
    provider.id = "readonly-provider".into();
    provider.exports.get_mut("run").unwrap().access = Access::Read;
    let read = k
        .register_guest(
            provider,
            &status_guest(
                "i32.const 0 local.get $ptr local.get $len i32.const 256 i32.const 4 call $call",
            ),
        )
        .unwrap();
    grant(&mut k, read, 0, Access::Write, &["scalar"]);
    assert_eq!(
        number(
            &k.invoke(read, "run", &bytes(99), trusted(Access::Write))
                .unwrap()
        ) as i32,
        Error::Authority.abi_code()
    );
    assert_eq!(*state.borrow(), committed);
    let mut consumer = manifest("read-consumer", Access::Read);
    consumer
        .dependencies
        .push(dependency("readonly-provider", "run", Access::Read));
    let consumer = k.register_guest(consumer, &forward(0)).unwrap();
    grant(&mut k, consumer, 0, Access::Read, &["scalar"]);
    assert_eq!(
        number(
            &k.invoke(consumer, "run", &bytes(99), trusted(Access::Read))
                .unwrap()
        ) as i32,
        Error::Authority.abi_code()
    );
    assert_eq!(*state.borrow(), committed);
}

#[test]
fn legitimate_prefix_survives_later_refusal_and_trap() {
    for trap in [false, true] {
        let mut k = Kernel::new(Limits::default()).unwrap();
        let (_, state) = device(&mut k);
        let mut m = manifest("prefix", Access::Write);
        m.dependencies = vec![
            dependency("device", "set", Access::Write),
            dependency("device", "set", Access::Write),
        ];
        let tail = if trap {
            "unreachable"
        } else {
            "i32.const 512 i32.const 1 i32.const 0 i32.const 4 i32.const 256 i32.const 4 call $call i32.store"
        };
        let h = k.register_guest(m, &wasm(&format!("i32.const 0 local.get $ptr local.get $len i32.const 256 i32.const 4 call $call drop {tail} i64.const 2199023255556"))).unwrap();
        grant(&mut k, h, 0, Access::Write, &["scalar"]);
        grant(&mut k, h, 1, Access::Write, &["scalar"]);
        k.revoke_grant(h, 1).unwrap();
        let result = k.invoke(h, "run", &bytes(33), trusted(Access::Write));
        if trap {
            assert_eq!(result, Err(Error::Trap));
        } else {
            assert_eq!(number(&result.unwrap()) as i32, Error::Revoked.abi_code());
        }
        let after = state.borrow();
        assert_eq!(after.value, 33);
        assert_eq!(after.history, vec![6, 33]);
        assert_eq!(after.unrelated, "populated-before-extension");
    }
}

#[test]
fn unauthorized_read_and_positive_wrong_slot_preserve_exact_state() {
    let mut k = Kernel::new(Limits::default()).unwrap();
    let (_, state) = device(&mut k);
    let mut m = manifest("read", Access::Read);
    m.dependencies
        .push(dependency("device", "read", Access::Read));
    let h = k
        .register_guest(
            m.clone(),
            &status_guest(
                "i32.const 0 i32.const 0 i32.const 0 i32.const 256 i32.const 4 call $call",
            ),
        )
        .unwrap();
    let before = state.borrow().clone();
    grant(&mut k, h, 0, Access::Read, &["other"]);
    assert_eq!(
        status(&k, h, Access::Read, &["scalar", "other"]),
        Error::Authority.abi_code()
    );
    assert_eq!(*state.borrow(), before);
    grant(&mut k, h, 0, Access::Read, &["scalar"]);
    assert_eq!(status(&k, h, Access::Read, &["scalar"]), 4);
    assert_eq!(*state.borrow(), before);
    m.id = "positive-slot".into();
    let h = k
        .register_guest(
            m,
            &status_guest(
                "i32.const 4 i32.const 0 i32.const 0 i32.const 256 i32.const 4 call $call",
            ),
        )
        .unwrap();
    grant(&mut k, h, 0, Access::Read, &["scalar"]);
    assert_eq!(
        status(&k, h, Access::Read, &["scalar"]),
        Error::InvalidSlot.abi_code()
    );
    assert_eq!(*state.borrow(), before);
}
