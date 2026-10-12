mod support;
use nethacked_kernel::*;
use nethacked_timers::{Limits as TimerLimits, Request, Status};
use std::{collections::BTreeSet, rc::Rc};
use support::*;

#[test]
fn complete_schedule_validation_and_capture_failures_publish_nothing() {
    let mut h = Host::new();
    let (lease, binding) = h.source(h.actor);
    let valid = h.request(vec![1], 1, 1);
    let mut cases = vec![];
    for field in 0..8 {
        let mut req = valid.clone();
        if let Request::Schedule {
            callback,
            payload,
            targets,
            deadline,
            residency,
            ceiling,
        } = &mut req
        {
            match field {
                0 => *callback = "absent".into(),
                1 => *callback = "x".repeat(129),
                2 => *payload = vec![0; 4097],
                3 => targets.push(targets[0].clone()),
                4 => *deadline = vec![1, 2],
                5 => *residency = vec![],
                6 => ceiling.scope = Scope::All,
                7 => targets[0].revision += 1,
                _ => unreachable!(),
            }
        }
        cases.push(req);
    }
    for req in cases {
        let before = h.snapshot();
        let response = h.invoke(&binding, &req);
        assert!(abi(&response) < 0);
        assert_eq!(h.snapshot(), before);
    }
    let baseline = h.snapshot();
    for bytes in [
        br#"{"kind":"Schedule","caller":1}"#.as_slice(),
        b"null",
        b"{}",
    ] {
        assert_eq!(
            abi(&h.k.invoke_operation(&binding, bytes, auth()).unwrap()),
            Error::NativeRefusal.abi_code()
        );
        assert_eq!(h.snapshot(), baseline);
    }
    // Root host calls have no authenticated caller/operation.
    assert_eq!(
        h.k.invoke(
            h.timer,
            "queue",
            &serde_json::to_vec(&valid).unwrap(),
            auth()
        ),
        Err(Error::Authority)
    );
    assert_eq!(h.snapshot(), baseline);
    let reply = h.invoke(&binding, &valid);
    let id: nethacked_timers::TaskId = serde_json::from_slice(&reply).unwrap();
    assert_eq!(h.s.state().unwrap().tasks[&id].status, Status::Pending);
    assert_eq!(h.k.lifecycle_state().callbacks, baseline.1.callbacks + 1);
    assert_eq!(
        h.k.lifecycle_state().next_callback,
        baseline.1.next_callback + 1
    );
    h.k.release_pipeline(lease).unwrap();
    assert_eq!(h.deliver(1, 1).id, id);
}
#[test]
fn callback_request_and_adoption_capacity_refuse_before_identity_allocation() {
    let mut h = Host::new();
    let id = h.schedule(vec![0; 4096], 1, 1); // legal callback root and dependency request size
    assert_eq!(h.deliver(1, 1).id, id); // otherwise valid control
    h.s.dispose(&mut h.k, id).unwrap();
    // New guest root is smaller; the schedule payload remains valid timer data.
    let mut m = manifest("small", &["run", "callback"]);
    m.dependencies = vec![dep("timers", "queue"), dep("effect", "run")];
    m.exports.get_mut("callback").unwrap().max_request = 1;
    let actor = h.k.register_guest(m, &guest("", "")).unwrap();
    for slot in 0..2 {
        h.k.issue_grant(actor, slot, auth()).unwrap();
    }
    let (lease, binding) = h.source(actor);
    let req = h.request(vec![1, 2], 1, 1);
    let id: nethacked_timers::TaskId = serde_json::from_slice(&h.invoke(&binding, &req)).unwrap();
    h.k.release_pipeline(lease).unwrap();
    let snapshot = h.snapshot();
    assert_eq!(
        h.s.deliver_next(&mut h.k, &context(1, 1)),
        Err(Error::Buffer)
    );
    assert_eq!(h.snapshot(), snapshot);
    let (lease, binding) = h.source(actor);
    h.invoke(&binding, &Request::Cancel { task: id });
    h.k.release_pipeline(lease).unwrap();
    h.s.dispose(&mut h.k, id).unwrap();
    // Exhaust live adopted lease capacity while the callback itself remains valid.
    h.k.set_retention_limits(RetentionLimits {
        leases: 1,
        ..RetentionLimits::default()
    })
    .unwrap();
    let pending = h.schedule(vec![1], 1, 1);
    let (blocker_op, blocker) = h.k.capture_operation(h.actor, "run", auth()).unwrap();
    let snapshot = h.snapshot();
    assert_eq!(
        h.s.deliver_next(&mut h.k, &context(1, 1)),
        Err(Error::Limit)
    );
    assert_eq!(h.snapshot(), snapshot);
    h.k.release_pipeline(blocker).unwrap();
    let delivered = h.deliver(1, 1);
    assert_eq!(delivered.id, pending);
    assert_eq!(
        delivered.invocation.as_ref().unwrap().operation().serial(),
        blocker_op.serial() + 1
    );
}
#[test]
fn reply_size_refusal_precedes_capture_and_finite_records_never_evict() {
    let mut h = Host::with(
        TimerLimits {
            records: 1,
            ..TimerLimits::default()
        },
        Rc::new(Clock::default()),
        "",
        "",
        vec![],
        true,
    );
    let code = wat::parse_str(
        r#"(module
      (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (func (export "alloc") (param i32) (result i32) i32.const 0)
      (func (export "callback") (param i32 i32) (result i64) i64.const 0)
      (func (export "run") (param $p i32) (param $n i32) (result i64) (local $r i32)
        i32.const 0 local.get $p local.get $n i32.const 32768 i32.const 1 call $call local.set $r
        i32.const 0 local.get $r i32.store
        i64.const 4))"#,
    )
    .unwrap();
    let mut m = manifest("tiny", &["run", "callback"]);
    m.dependencies = vec![dep("timers", "queue")];
    let tiny = h.k.register_guest(m, &code).unwrap();
    h.k.issue_grant(tiny, 0, auth()).unwrap();
    let (lease, binding) = h.source(tiny);
    let snapshot = h.snapshot();
    let valid = h.request(vec![1], 1, 1);
    let response = h.invoke(&binding, &valid);
    assert_eq!(h.snapshot(), snapshot);
    assert_eq!(abi(&response), Error::Buffer.abi_code());
    h.k.release_pipeline(lease).unwrap();
    let id = h.schedule(vec![1], 1, 1);
    let (lease, binding) = h.source(h.actor);
    let snapshot = h.snapshot();
    assert_eq!(abi(&h.invoke(&binding, &valid)), Error::Limit.abi_code());
    assert_eq!(h.snapshot(), snapshot);
    h.k.release_pipeline(lease).unwrap();
    h.deliver(1, 1);
    let (lease, binding) = h.source(h.actor);
    let snapshot = h.snapshot();
    assert_eq!(abi(&h.invoke(&binding, &valid)), Error::Limit.abi_code());
    assert_eq!(h.snapshot(), snapshot);
    h.k.release_pipeline(lease).unwrap();
    h.s.dispose(&mut h.k, id).unwrap();
    let next = h.schedule(vec![2], 1, 1);
    assert!(next.serial > id.serial);
    assert_eq!(h.s.state().unwrap().tasks.len(), 1);
}
#[test]
fn scoped_queue_target_and_callback_authority_checks_have_valid_controls() {
    let mut h = Host::new();
    let (lease, binding) = h.source(h.actor);
    let req = h.request(vec![1], 1, 1);
    for scope in [
        BTreeSet::from([queue_key()]),
        BTreeSet::from([key("value")]),
    ] {
        let narrow = Authority {
            access: Access::Write,
            scope: Scope::Keys(scope),
        };
        let snapshot = h.snapshot();
        assert_eq!(
            abi(&h
                .k
                .invoke_operation(&binding, &serde_json::to_vec(&req).unwrap(), narrow)
                .unwrap()),
            Error::Authority.abi_code()
        );
        assert_eq!(h.snapshot(), snapshot);
    }
    let id: nethacked_timers::TaskId = serde_json::from_slice(&h.invoke(&binding, &req)).unwrap();
    h.k.release_pipeline(lease).unwrap();
    assert_eq!(h.deliver(1, 1).id, id);
}
