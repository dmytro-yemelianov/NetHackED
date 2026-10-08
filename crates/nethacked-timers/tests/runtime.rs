mod support;
use nethacked_kernel::*;
use nethacked_timers::{Limits as TimerLimits, Request, Status, TaskId};
use std::{cell::Cell, collections::BTreeSet, rc::Rc};
use support::*;

#[test]
fn real_schedule_retired_sources_two_profiles_and_readiness_refusals() {
    let mut h = Host::new();
    let before = h.resources.state().unwrap();
    let a = h.schedule(b"a".to_vec(), 3, 1);
    let b = h.schedule(b"b".to_vec(), 3, 1);
    let off = h.schedule(b"off".to_vec(), 1, 2);
    assert_eq!(h.k.lifecycle_state().leases, 0);
    let snapshot = h.snapshot();
    assert_eq!(h.s.deliver_next(&mut h.k, &context(2, 1)).unwrap(), None);
    assert_eq!(h.snapshot(), snapshot);
    h.clock.forced.set(Some(a));
    assert_eq!(
        h.s.deliver_next(&mut h.k, &context(2, 1)),
        Err(Error::NativeRefusal)
    );
    assert_eq!(h.snapshot(), snapshot);
    h.clock.forced.set(Some(TaskId {
        queue: a.queue + 1,
        ..a
    }));
    assert_eq!(
        h.s.deliver_next(&mut h.k, &context(3, 1)),
        Err(Error::InvalidHandle)
    );
    assert_eq!(h.snapshot(), snapshot);
    h.clock.forced.set(None);
    let first = h.deliver(3, 1);
    assert_eq!(first.id, a);
    assert_eq!(first.status, Status::Completed(vec![1]));
    assert_eq!(
        first.invocation.as_ref().unwrap().operation().serial(),
        snapshot.0.tasks[&off]
            .provenance
            .source
            .operation()
            .serial()
            + 1
    );
    assert_ne!(
        first.invocation.as_ref().unwrap().operation(),
        first.provenance.source.operation()
    );
    assert_eq!(first.invocation.as_ref().unwrap().checkpoint(), 0);
    assert_eq!(first.invocation.as_ref().unwrap().phase(), "timer");
    assert_eq!(h.deliver(3, 1).id, b);
    assert_eq!(h.deliver(3, 2).id, off);
    assert_eq!(h.resources.state().unwrap(), before);
    assert_eq!(
        h.seen
            .borrow()
            .iter()
            .map(|s| s.payload.clone())
            .collect::<Vec<_>>(),
        vec![b"a".to_vec(), b"b".to_vec(), b"off".to_vec()]
    );
    let duplicate = h.snapshot();
    h.clock.forced.set(Some(a));
    assert_eq!(
        h.s.deliver_next(&mut h.k, &context(9, 1)),
        Err(Error::NativeRefusal)
    );
    assert_eq!(h.snapshot(), duplicate);
    for id in [a, b, off] {
        h.s.dispose(&mut h.k, id).unwrap();
    }
    assert_eq!(h.k.lifecycle_state().references, 0);

    let mut inactive = Host::with(
        TimerLimits::default(),
        Rc::new(Clock {
            reverse: true,
            inactive: true,
            ..Clock::default()
        }),
        "",
        "",
        vec![],
        true,
    );
    let earlier = inactive.schedule(vec![1], 2, 9);
    let later = inactive.schedule(vec![2], 2, 9);
    assert_eq!(inactive.deliver(2, 1).id, later);
    assert_eq!(inactive.deliver(2, 1).id, earlier);
}
#[test]
fn retained_old_new_executables_live_grant_and_target_matrix() {
    let mut h = Host::new();
    let before = h.resources.state().unwrap();
    let old_extension = h.extension("old", 20);
    let old = h.schedule(vec![5], 1, 1);
    h.k.managed_detach(old_extension).unwrap();
    let new_extension = h.extension("new", 40);
    let new = h.schedule(vec![6], 1, 1);
    let mut foreign = Kernel::new(Limits::default()).unwrap();
    let snapshot = h.snapshot();
    let other = foreign.lifecycle_state();
    assert!(h.s.deliver_next(&mut foreign, &context(1, 1)).is_err());
    assert_eq!(h.snapshot(), snapshot);
    assert_eq!(foreign.lifecycle_state(), other);
    h.k.revoke_grant(h.actor, 0).unwrap();
    let snapshot = h.snapshot();
    assert_eq!(
        h.s.deliver_next(&mut h.k, &context(1, 1)),
        Err(Error::Revoked)
    );
    assert_eq!(h.snapshot(), snapshot);
    let narrow = Authority {
        access: Access::Write,
        scope: Scope::Keys(BTreeSet::from([queue_key()])),
    };
    h.k.issue_grant(h.actor, 0, narrow).unwrap();
    let snapshot = h.snapshot();
    assert_eq!(
        h.s.deliver_next(&mut h.k, &context(1, 1)),
        Err(Error::Authority)
    );
    assert_eq!(h.snapshot(), snapshot);
    h.k.issue_grant(h.actor, 0, auth()).unwrap();
    assert_eq!(h.deliver(1, 1).status, Status::Completed(vec![20]));
    assert_eq!(h.deliver(1, 1).status, Status::Completed(vec![40]));
    assert_eq!(h.seen.borrow()[0].caller, old_extension);
    assert_eq!(h.seen.borrow()[1].caller, new_extension);
    assert_eq!(h.resources.state().unwrap(), before);
    h.s.dispose(&mut h.k, old).unwrap();
    h.k.managed_unload(old_extension).unwrap();
    assert_eq!(h.k.managed_unload(new_extension), Err(Error::Referenced));
    h.s.dispose(&mut h.k, new).unwrap();
    h.k.managed_detach(new_extension).unwrap();
    h.k.managed_unload(new_extension).unwrap();

    let id = h.schedule(vec![9], 1, 1);
    // Same generation, changed revision.
    h.resources.put(key("value"), b"updated".to_vec()).unwrap();
    let snapshot = h.snapshot();
    assert_eq!(
        h.s.deliver_next(&mut h.k, &context(1, 1)),
        Err(Error::NativeRefusal)
    );
    assert_eq!(h.snapshot(), snapshot);
    h.resources.delete(&key("value")).unwrap();
    let snapshot = h.snapshot();
    assert_eq!(
        h.s.deliver_next(&mut h.k, &context(1, 1)),
        Err(Error::NativeRefusal)
    );
    assert_eq!(h.snapshot(), snapshot);
    let replacement = h.resources.put(key("value"), b"old".to_vec()).unwrap();
    assert_ne!(replacement.generation, h.stamp.generation);
    let snapshot = h.snapshot();
    assert_eq!(
        h.s.deliver_next(&mut h.k, &context(1, 1)),
        Err(Error::NativeRefusal)
    );
    assert_eq!(h.snapshot(), snapshot);
    assert_eq!(h.s.state().unwrap().tasks[&id].status, Status::Pending);
    // A current stamp is an otherwise valid positive control.
    h.stamp = replacement;
    let fresh = h.schedule(vec![10], 1, 1);
    h.clock.forced.set(Some(fresh));
    assert_eq!(h.deliver(1, 1).id, fresh);
}
#[test]
fn pending_cancel_requires_fresh_owner_artifact_permissions_and_retains_record() {
    let mut h = Host::new();
    let id = h.schedule(vec![1], 1, 1);
    let mut m = manifest("foreign", &["run", "callback"]);
    m.dependencies = vec![dep("timers", "queue"), dep("effect", "run")];
    let foreign = h.k.register_guest(m, &guest("", "")).unwrap();
    for slot in 0..2 {
        h.k.issue_grant(foreign, slot, auth()).unwrap();
    }
    let (lease, binding) = h.source(foreign);
    let snapshot = h.snapshot();
    assert_eq!(
        abi(&h.invoke(&binding, &Request::Cancel { task: id })),
        Error::Authority.abi_code()
    );
    assert_eq!(h.snapshot(), snapshot);
    h.k.release_pipeline(lease).unwrap();
    let (lease, binding) = h.source(h.actor);
    let snapshot = h.snapshot();
    let malformed = br#"{"kind":"Cancel","task":{"queue":1,"serial":1},"caller":1}"#;
    assert_eq!(
        abi(&h.k.invoke_operation(&binding, malformed, auth()).unwrap()),
        Error::NativeRefusal.abi_code()
    );
    for bad in [
        TaskId {
            queue: id.queue + 1,
            ..id
        },
        TaskId { serial: 0, ..id },
    ] {
        assert_eq!(
            abi(&h.invoke(&binding, &Request::Cancel { task: bad })),
            Error::InvalidHandle.abi_code()
        );
        assert_eq!(h.snapshot(), snapshot);
    }
    let narrow = Authority {
        access: Access::Write,
        scope: Scope::Keys(BTreeSet::from([queue_key()])),
    };
    h.k.issue_grant(h.actor, 0, narrow).unwrap();
    let snapshot = h.snapshot();
    assert_eq!(
        abi(&h.invoke(&binding, &Request::Cancel { task: id })),
        Error::Authority.abi_code()
    );
    assert_eq!(h.snapshot(), snapshot);
    h.k.revoke_grant(h.actor, 0).unwrap();
    let snapshot = h.snapshot();
    assert_eq!(
        abi(&h.invoke(&binding, &Request::Cancel { task: id })),
        Error::Revoked.abi_code()
    );
    assert_eq!(h.snapshot(), snapshot);
    h.k.issue_grant(h.actor, 0, auth()).unwrap();
    let reply: TaskId =
        serde_json::from_slice(&h.invoke(&binding, &Request::Cancel { task: id })).unwrap();
    assert_eq!(reply, id);
    assert_eq!(h.k.lifecycle_state().callbacks, 0);
    assert_eq!(h.s.state().unwrap().tasks[&id].status, Status::Cancelled);
    let snapshot = h.snapshot();
    assert_eq!(
        abi(&h.invoke(&binding, &Request::Cancel { task: id })),
        Error::NativeRefusal.abi_code()
    );
    assert_eq!(h.snapshot(), snapshot);
    assert!(h.seen.borrow().is_empty());
    h.k.release_pipeline(lease).unwrap();
    h.s.dispose(&mut h.k, id).unwrap();
    assert_eq!(h.k.lifecycle_state().references, 0);
}
#[test]
fn actual_callback_enqueues_successor_and_active_guard_refuses_invalidation() {
    // The first callback forwards its payload into queue, then calls the observer.
    let body = "i32.const 0 local.get $p local.get $n call $forward drop i32.const 1 local.get $p local.get $n call $forward";
    let mut h = Host::with(
        TimerLimits::default(),
        Rc::new(Clock::default()),
        body,
        "",
        vec![],
        true,
    );
    let next_req = h.request(b"final".to_vec(), 2, 1);
    let id = h.schedule(serde_json::to_vec(&next_req).unwrap(), 1, 1);
    let s = h.s.clone();
    let observations = Rc::new(Cell::new(0));
    let probes = observations.clone();
    *h.hook.borrow_mut() = Some(Rc::new(move |_, view| {
        let before = s.state().unwrap();
        assert_eq!(before.active, Some(id));
        assert_eq!(before.tasks[&id].status, Status::Running);
        assert_eq!(before.tasks[&id].invocation.as_ref(), view.operation());
        assert_eq!(before.tasks.len(), 2);
        let mut other = Kernel::new(Limits::default()).unwrap();
        assert_eq!(
            s.deliver_next(&mut other, &context(9, 1)),
            Err(Error::NativeRefusal)
        );
        assert_eq!(s.dispose(&mut other, id), Err(Error::NativeRefusal));
        assert_eq!(s.retry_cleanup(id), Err(Error::NativeRefusal));
        assert_eq!(
            s.install(&mut other, "invalidates"),
            Err(Error::NativeRefusal)
        );
        assert_eq!(s.state().unwrap(), before);
        probes.set(probes.get() + 1);
        Ok(vec![])
    }));
    assert_eq!(h.deliver(1, 1).id, id);
    assert_eq!(observations.get(), 1);
    let state = h.s.state().unwrap();
    let successor = *state.tasks.keys().find(|k| **k != id).unwrap();
    assert_eq!(state.tasks[&successor].status, Status::Pending);
    assert_eq!(
        state.tasks[&successor].provenance.source,
        state.tasks[&id].invocation.clone().unwrap()
    );
    assert_eq!(h.seen.borrow().len(), 1);
    *h.hook.borrow_mut() = None;
    assert_eq!(h.deliver(2, 1).id, successor);
    assert_eq!(h.seen.borrow().len(), 2);
    assert_eq!(h.s.deliver_next(&mut h.k, &context(9, 1)).unwrap(), None);
    h.s.dispose(&mut h.k, id).unwrap();
    assert_eq!(h.k.managed_unload(h.actor), Err(Error::Referenced));
    h.s.dispose(&mut h.k, successor).unwrap();
    assert_eq!(h.k.lifecycle_state().references, 0);
}
#[test]
fn real_running_task_cancellation_is_refused_in_callback() {
    // Guest forwards a Cancel payload to queue and reports the ABI refusal as its result.
    let body = format!(
        r#"i32.const 0 local.get $p local.get $n i32.const 32768 i32.const 16384 call $call
        i32.const {} i32.ne if unreachable end
        i32.const 1 local.get $p local.get $n call $forward"#,
        Error::NativeRefusal.abi_code()
    );
    // Match the actual kernel NativeRefusal ABI; no permissive trap expectation.
    let mut h = Host::with(
        TimerLimits::default(),
        Rc::new(Clock::default()),
        &body,
        "",
        vec![],
        true,
    );
    let state = h.s.state().unwrap();
    let expected = TaskId {
        queue: state.queue,
        serial: state.next_serial,
    };
    let id = h.schedule(
        serde_json::to_vec(&Request::Cancel { task: expected }).unwrap(),
        1,
        1,
    );
    assert_eq!(id, expected);
    assert_eq!(h.deliver(1, 1).status, Status::Completed(vec![1]));
    assert_eq!(h.seen.borrow().len(), 1);
}
