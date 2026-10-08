mod support;
use nethacked_inputs::*;
use nethacked_kernel::{Access, Authority, Error, Scope};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
    rc::Rc,
};
use support::*;

#[test]
fn real_guest_order_replay_conditional_and_external_policy() {
    let h = Host::new();
    let b = h.binding(4, "select");
    let before = h.s.inspect().unwrap();
    // A genuine guest branch that omits its dependency call consumes nothing.
    assert_eq!(h.raw(&b, b"", authority()), b"skip");
    assert_eq!(h.s.inspect().unwrap(), before);
    assert_eq!(h.calls.get(), 0);
    let a = h.call(&b, &sample(b"same"));
    let z = h.call(&b, &sample(b"same"));
    assert_eq!(a.value, [10]);
    assert_eq!(z.value, [20]);
    assert_ne!(a.id, z.id);
    let saved = h.s.inspect().unwrap();
    assert_eq!(
        saved.records.iter().map(|r| r.ordinal).collect::<Vec<_>>(),
        [0, 1]
    );
    assert_eq!(saved.source, [30, 40]);
    assert_eq!(saved.next_serial, before.next_serial + 2);
    assert_eq!(h.call(&b, &replay(&a, b"same")), a);
    assert_eq!(h.s.inspect().unwrap(), saved);
    assert_eq!(h.calls.get(), 2);
    let external = Policy {
        identity: "host-events".into(),
        version: 7,
        transition: Rc::new(|s, r| Ok((s.to_vec(), r.to_vec()))),
    };
    let other = Host::with(Limits::default(), Some(external));
    assert_eq!(
        other
            .call(&other.binding(0, "choose"), &sample(b"external-choice"))
            .value,
        b"external-choice"
    );
    assert_eq!(other.s.inspect().unwrap().source, [10, 20, 30, 40]);
}

#[test]
fn live_record_negative_matrix_keeps_owner_authorized() {
    let mut h = Host::new();
    let b = h.binding(4, "select");
    let original = h.call(&b, &sample(b"request"));
    let good = replay(&original, b"request");
    let same = h.s.inspect().unwrap();
    for bad in [h.binding(3, "select"), h.binding(4, "other")] {
        h.refuse(&bad, &good, authority());
        assert_eq!(h.call(&b, &good), original);
    }
    assert_eq!(h.call(&h.binding(5, "other"), &good), original);
    let op2 = h.k.issue_operation().unwrap();
    let other_op = h.k.operation_binding(op2, h.lease, 4, "select").unwrap();
    h.refuse(&other_op, &good, authority());
    assert_eq!(h.call(&b, &good), original);
    let lease2 = h.k.capture_pipeline(h.guest, "step", authority()).unwrap();
    let other_lease =
        h.k.operation_binding(h.operation, lease2, 4, "select")
            .unwrap();
    h.refuse(&other_lease, &good, authority());
    assert_eq!(h.call(&b, &good), original);
    // A nested caller/artifact under the EXACT SAME authenticated operation and lease.
    let delegated = |req: &Request| {
        let mut bytes = vec![b' '];
        bytes.extend(serde_json::to_vec(req).unwrap());
        bytes
    };
    let thief_choice: Reply =
        serde_json::from_slice(&h.raw(&b, &delegated(&sample(b"thief")), authority())).unwrap();
    let before = h.s.inspect().unwrap();
    let calls = h.calls.get();
    assert_eq!(before.records[1].operation, b);
    assert_eq!(before.records[1].caller, h.delegate);
    assert_ne!(
        before.records[1].caller_artifact,
        before.records[0].caller_artifact
    );
    let denied = h.raw(&b, &delegated(&good), authority());
    assert_eq!(
        i32::from_le_bytes(denied.try_into().unwrap()),
        Error::Authority.abi_code()
    );
    assert_eq!(h.s.inspect().unwrap(), before);
    assert_eq!(h.calls.get(), calls);
    assert_eq!(
        serde_json::from_slice::<Reply>(&h.raw(
            &b,
            &delegated(&replay(&thief_choice, b"thief")),
            authority()
        ))
        .unwrap(),
        thief_choice
    );
    assert_eq!(h.call(&b, &good), original);
    let mut unknown = original.clone();
    unknown.id.serial += 100;
    h.refuse(&b, &replay(&unknown, b"request"), authority());
    let mut foreign = original.clone();
    foreign.id.namespace += 100;
    h.refuse(&b, &replay(&foreign, b"request"), authority());
    h.refuse(&b, &replay(&original, b"mismatch"), authority());
    let mut forged = serde_json::to_value(&good).unwrap();
    forged["operation"] = serde_json::json!(h.operation.serial());
    forged["pipeline"] = serde_json::json!("guest supplied");
    forged["checkpoint"] = serde_json::json!(4);
    let mut forged_id = serde_json::to_value(&good).unwrap();
    forged_id["id"]["phase"] = serde_json::json!("select");
    for req in [
        serde_json::to_vec(&forged).unwrap(),
        serde_json::to_vec(&forged_id).unwrap(),
        br#"{"kind":"sample","request":[],"source":[99],"policy":"override","ordinal":0}"#.to_vec(),
    ] {
        let before = h.s.inspect().unwrap();
        let calls = h.calls.get();
        assert_eq!(
            i32::from_le_bytes(h.raw(&b, &req, authority()).try_into().unwrap()),
            Error::NativeRefusal.abi_code()
        );
        assert_eq!(h.s.inspect().unwrap(), before);
        assert_eq!(h.calls.get(), calls);
        assert_eq!(h.call(&b, &good), original);
    }
    let before = h.s.inspect().unwrap();
    let calls = h.calls.get();
    let encoded = serde_json::to_vec(&good).unwrap();
    assert_eq!(
        h.k.invoke_pipeline(h.lease, &encoded, authority())
            .unwrap()
            .len(),
        4
    );
    assert_eq!(
        h.k.invoke(h.guest, "step", &encoded, authority())
            .unwrap()
            .len(),
        4
    );
    assert_eq!(
        h.k.invoke(original_provider(&h), "input", &encoded, authority()),
        Err(Error::Authority)
    );
    // Cross-host operation credentials are rejected by the kernel before execution.
    let host2 = Host::new();
    let host2record = host2.call(&host2.binding(4, "select"), &sample(b"request"));
    assert_eq!(
        host2.k.invoke_operation(&b, &encoded, authority()),
        Err(Error::Authority)
    );
    assert_eq!(
        host2.call(
            &host2.binding(4, "select"),
            &replay(&host2record, b"request")
        ),
        host2record
    );
    assert_eq!(h.s.inspect().unwrap(), before);
    assert_eq!(h.calls.get(), calls);
    h.k.revoke_grant(h.guest, 0).unwrap();
    h.refuse(&b, &good, authority());
    h.k.issue_grant(
        h.guest,
        0,
        Authority {
            access: Access::Write,
            scope: Scope::Keys(BTreeSet::new()),
        },
    )
    .unwrap();
    h.refuse(&b, &good, authority());
    h.k.issue_grant(h.guest, 0, authority()).unwrap();
    h.refuse(
        &b,
        &good,
        Authority {
            access: Access::Write,
            scope: Scope::Keys(BTreeSet::new()),
        },
    );
    let before = h.s.inspect().unwrap();
    let calls = h.calls.get();
    assert_eq!(
        h.k.invoke_operation(
            &b,
            &encoded,
            Authority {
                access: Access::Read,
                scope: authority().scope
            }
        ),
        Err(Error::Authority)
    );
    assert_eq!(h.s.inspect().unwrap(), before);
    assert_eq!(h.calls.get(), calls);
    assert_eq!(h.call(&b, &good), original);
    assert_eq!(h.s.inspect().unwrap().records[0], same.records[0]);
}

fn original_provider(h: &Host) -> nethacked_kernel::ModuleHandle {
    h.s.inspect().unwrap().records[0].provider
}

#[test]
fn admission_bounds_and_policy_output_failures_do_not_publish() {
    let h = Host::with(
        Limits {
            records: 1,
            ..Limits::default()
        },
        None,
    );
    let b = h.binding(0, "select");
    let a = h.call(&b, &sample(b"ok"));
    h.refuse(&b, &sample(b"full"), authority());
    assert_eq!(h.call(&b, &replay(&a, b"ok")), a);
    let mut h = Host::with(
        Limits {
            operations: 1,
            ..Limits::default()
        },
        None,
    );
    let b = h.binding(0, "select");
    let a = h.call(&b, &sample(b"ok"));
    let op = h.k.issue_operation().unwrap();
    let other = h.k.operation_binding(op, h.lease, 0, "select").unwrap();
    h.refuse(&other, &sample(b"ok"), authority());
    assert_eq!(h.call(&b, &replay(&a, b"ok")), a);
    let h = Host::with(
        Limits {
            request: 2,
            ..Limits::default()
        },
        None,
    );
    let b = h.binding(0, "select");
    h.refuse(&b, &sample(b"big"), authority());
    assert_eq!(h.call(&b, &sample(b"ok")).value, [10]);
    let h = Host::new();
    let b = h.binding(0, "select");
    let before = h.s.inspect().unwrap();
    assert_eq!(
        h.raw(
            &b,
            &serde_json::to_vec(&sample(b"error")).unwrap(),
            authority()
        )
        .len(),
        4
    );
    assert_eq!(h.s.inspect().unwrap(), before);
    assert_eq!(h.calls.get(), 1);
    assert_eq!(h.call(&b, &sample(b"ok")).value, [10]);
    for fail_source in [true, false] {
        let mode = Rc::new(Cell::new(true));
        let m = mode.clone();
        let count = Rc::new(Cell::new(0));
        let c = count.clone();
        let p = Policy {
            identity: "output-bounds".into(),
            version: 1,
            transition: Rc::new(move |s, _| {
                c.set(c.get() + 1);
                if m.get() {
                    if fail_source {
                        Ok((vec![0; 5], vec![1]))
                    } else {
                        Ok((s.to_vec(), vec![0; 3]))
                    }
                } else {
                    Ok((s.to_vec(), vec![1]))
                }
            }),
        };
        let h = Host::with(
            Limits {
                source: 4,
                value: 2,
                ..Limits::default()
            },
            Some(p),
        );
        let b = h.binding(0, "select");
        let before = h.s.inspect().unwrap();
        assert_eq!(
            h.raw(
                &b,
                &serde_json::to_vec(&sample(b"ok")).unwrap(),
                authority()
            )
            .len(),
            4
        );
        assert_eq!(h.s.inspect().unwrap(), before);
        assert_eq!(count.get(), 1);
        mode.set(false);
        assert_eq!(h.call(&b, &sample(b"ok")).value, [1]);
        assert_eq!(count.get(), 2);
    }
    assert!(matches!(
        Service::new(
            Limits {
                source: 0,
                ..Limits::default()
            },
            Policy {
                identity: "size".into(),
                version: 1,
                transition: Rc::new(|s, r| Ok((s.to_vec(), r.to_vec())))
            },
            vec![1]
        ),
        Err(Error::Buffer)
    ));
    // Valid request but insufficient maximum-value reply capacity: no policy call.
    let h = Host::with(
        Limits {
            value: usize::MAX,
            ..Limits::default()
        },
        None,
    );
    let b = h.binding(0, "select");
    h.refuse(&b, &sample(b"ok"), authority());
    let h = Host::with(
        Limits {
            wire: 128,
            request: 2,
            value: 1,
            ..Limits::default()
        },
        None,
    );
    let b = h.binding(0, "select");
    let before = h.s.inspect().unwrap();
    // Whitespace is valid JSON; it reaches native endpoint wire admission first.
    let mut oversized = vec![b' '; 129];
    oversized.extend(serde_json::to_vec(&sample(b"ok")).unwrap());
    // Bypass the multiplex forwarding whitespace marker with a newline.
    oversized[0] = b'\n';
    assert_eq!(
        i32::from_le_bytes(h.raw(&b, &oversized, authority()).try_into().unwrap()),
        Error::Buffer.abi_code()
    );
    assert_eq!(h.s.inspect().unwrap(), before);
    assert_eq!(h.calls.get(), 0);
    assert_eq!(h.call(&b, &sample(b"ok")).value, [10]);
    let h = Host::with(
        Limits {
            wire: 128,
            value: 32,
            ..Limits::default()
        },
        None,
    );
    let b = h.binding(0, "select");
    h.refuse(&b, &sample(b"ok"), authority());
}

#[test]
fn native_reentrancy_refuses_inspection_install_and_sampling_without_panic() {
    let holder = Rc::new(RefCell::new(None::<Service>));
    let captured = holder.clone();
    let invocation = Rc::new(RefCell::new(
        None::<(
            Rc<nethacked_kernel::Kernel>,
            nethacked_kernel::OperationBinding,
        )>,
    ));
    let nested = invocation.clone();
    let count = Rc::new(Cell::new(0));
    let c = count.clone();
    let p = Policy {
        identity: "reentrant".into(),
        version: 1,
        transition: Rc::new(move |s, _| {
            c.set(c.get() + 1);
            let service = captured.borrow();
            let service = service.as_ref().unwrap();
            assert_eq!(service.inspect(), Err(Error::NativeRefusal));
            let mut spare =
                nethacked_kernel::Kernel::new(nethacked_kernel::Limits::default()).unwrap();
            assert_eq!(
                service.install(&mut spare, "again"),
                Err(Error::NativeRefusal)
            );
            let refs = nested.borrow();
            let (kernel, b) = refs.as_ref().unwrap();
            let denied = kernel
                .invoke_operation(
                    b,
                    &serde_json::to_vec(&sample(b"nested")).unwrap(),
                    authority(),
                )
                .unwrap();
            assert_eq!(
                i32::from_le_bytes(denied.try_into().unwrap()),
                Error::NativeRefusal.abi_code()
            );
            Ok((s[1..].to_vec(), vec![s[0]]))
        }),
    };
    let h = Host::with(Limits::default(), Some(p));
    let b = h.binding(0, "select");
    let service = h.s.clone();
    *holder.borrow_mut() = Some(service.clone());
    let kernel = Rc::new(h.k);
    *invocation.borrow_mut() = Some((kernel.clone(), b.clone()));
    let reply: Reply = serde_json::from_slice(
        &kernel
            .invoke_operation(
                &b,
                &serde_json::to_vec(&sample(b"ok")).unwrap(),
                authority(),
            )
            .unwrap(),
    )
    .unwrap();
    assert_eq!(reply.value, [10]);
    assert_eq!(count.get(), 1);
    let state = service.inspect().unwrap();
    assert_eq!(state.source, [20, 30, 40]);
    assert_eq!(state.records.len(), 1);
    let repeated: Reply = serde_json::from_slice(
        &kernel
            .invoke_operation(
                &b,
                &serde_json::to_vec(&replay(&reply, b"ok")).unwrap(),
                authority(),
            )
            .unwrap(),
    )
    .unwrap();
    assert_eq!(repeated, reply);
    assert_eq!(count.get(), 1);
    assert_eq!(service.inspect().unwrap(), state);
    *holder.borrow_mut() = None;
    *invocation.borrow_mut() = None;
}
