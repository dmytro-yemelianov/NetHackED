mod support;
use nethacked_kernel::*;
use nethacked_resources::{Plan, Write};
use nethacked_timers::{Fault, Limits as TimerLimits, Status};
use nethacked_workflows::CleanupParticipant;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use support::*;

// Actual guest commits a write, observes its prefix, then prepares an abandoned plan.
fn transaction_body(trap: bool) -> String {
    format!(
        r#"
      i32.const 2 local.get $p local.get $n i32.const 32768 i32.const 16384 call $call local.set $r
      local.get $r i32.const 0 i32.lt_s if unreachable end
      i32.const 3 i32.const 32768 local.get $r i32.const 49152 i32.const 16384 call $call
      i32.const 0 i32.lt_s if unreachable end
      i32.const 1 local.get $p local.get $n call $forward drop
      i32.const 2 i32.const 8192 i32.const 39 i32.const 32768 i32.const 16384 call $call
      i32.const 0 i32.lt_s if unreachable end
      i32.const 1 local.get $p local.get $n call $forward {}"#,
        if trap { "drop unreachable" } else { "" }
    )
}
fn plan(h: &Host) -> Vec<u8> {
    serde_json::to_vec(&Plan {
        readonly: vec![],
        writes: vec![Write {
            stamp: h.stamp.clone(),
            replacement: Some(b"committed".to_vec()),
        }],
        events: vec![b"changed".to_vec()],
    })
    .unwrap()
}
#[test]
fn real_committed_prefix_survives_trap_malformed_oversized_and_rejection() {
    for (trap, reply, expected) in [
        (
            true,
            br#"{"kind":"Completed","result":[]}"#.to_vec(),
            Status::Faulted(Fault::Invocation(Error::Trap)),
        ),
        (
            false,
            b"bad".to_vec(),
            Status::Faulted(Fault::InvalidOutcome),
        ),
        (
            false,
            br#"{"kind":"Completed","result":[1,2]}"#.to_vec(),
            Status::Faulted(Fault::InvalidOutcome),
        ),
        (
            false,
            br#"{"kind":"Rejected","result":[7]}"#.to_vec(),
            Status::Rejected(vec![7]),
        ),
        (
            false,
            br#"{"kind":"Completed","result":[7]}"#.to_vec(),
            Status::Completed(vec![7]),
        ),
    ] {
        let mut h = Host::with(
            TimerLimits {
                result_bytes: 1,
                ..TimerLimits::default()
            },
            Rc::new(Clock::default()),
            &transaction_body(trap),
            "",
            vec![],
            true,
        );
        *h.effect_reply.borrow_mut() = reply;
        let before = h.resources.state().unwrap();
        let checkpoints = Rc::new(RefCell::new(vec![]));
        let observed = checkpoints.clone();
        let resources = h.resources.clone();
        *h.hook.borrow_mut() = Some(Rc::new(move |_, _| {
            observed.borrow_mut().push(resources.state()?);
            Ok(vec![])
        }));
        let id = h.schedule(plan(&h), 1, 1);
        let task = h.deliver(1, 1);
        assert_eq!(task.status, expected);
        assert!(!task.cleanup_pending);
        assert_eq!(h.seen.borrow().len(), 2);
        let prefix = &checkpoints.borrow()[0];
        let mut exact = prefix.clone();
        exact.next_serial += 1;
        assert_eq!(h.resources.state().unwrap(), exact); // prepared but uncommitted plan discarded
        assert_eq!(exact.resources[&key("value")].value, b"committed");
        assert_eq!(
            exact.resources[&key("value")].stamp.revision,
            h.stamp.revision + 1
        );
        assert_eq!(
            exact.resources[&key("unrelated")],
            before.resources[&key("unrelated")]
        );
        assert_eq!(exact.history, vec![b"changed".to_vec()]);
        assert_eq!(exact.commits, 1);
        let snapshot = h.snapshot();
        assert_eq!(h.s.deliver_next(&mut h.k, &context(9, 1)).unwrap(), None);
        assert_eq!(h.snapshot(), snapshot);
        assert_eq!(h.s.retry_cleanup(id), Err(Error::NativeRefusal));
        h.s.dispose(&mut h.k, id).unwrap();
        assert_eq!(h.k.lifecycle_state().references, 0);
    }
}
struct Flaky {
    id: CleanupIdentity,
    fail: Cell<bool>,
    discards: Cell<usize>,
    bindings: RefCell<Vec<OperationBinding>>,
}
impl CleanupParticipant for Flaky {
    fn identity(&self) -> Result<Option<CleanupIdentity>> {
        Ok(Some(self.id))
    }
    fn discard(&self, binding: &OperationBinding) -> Result<()> {
        assert!(binding.admits_cleanup(self.id));
        self.discards.set(self.discards.get() + 1);
        self.bindings.borrow_mut().push(binding.clone());
        if self.fail.get() {
            Err(Error::NativeRefusal)
        } else {
            Ok(())
        }
    }
}
#[test]
fn partial_cleanup_failure_retains_shared_code_retry_and_failed_release_are_atomic() {
    let first = Rc::new(Flaky {
        id: CleanupIdentity::issue().unwrap(),
        fail: Cell::new(true),
        discards: Cell::new(0),
        bindings: RefCell::new(vec![]),
    });
    let last = Rc::new(Flaky {
        id: CleanupIdentity::issue().unwrap(),
        fail: Cell::new(false),
        discards: Cell::new(0),
        bindings: RefCell::new(vec![]),
    });
    let mut h = Host::with(
        TimerLimits::default(),
        Rc::new(Clock::default()),
        &transaction_body(false),
        "",
        vec![first.clone(), last.clone()],
        true,
    );
    let prefix = Rc::new(RefCell::new(vec![]));
    let copy = prefix.clone();
    let r = h.resources.clone();
    *h.hook.borrow_mut() = Some(Rc::new(move |_, _| {
        copy.borrow_mut().push(r.state()?);
        Ok(vec![])
    }));
    let id = h.schedule(plan(&h), 1, 1);
    let second = h.schedule(plan(&h), 2, 1);
    let task = h.deliver(1, 1);
    assert_eq!(task.status, Status::Faulted(Fault::Cleanup));
    assert!(task.cleanup_pending);
    assert_eq!(first.discards.get(), 1);
    assert_eq!(last.discards.get(), 1);
    let mut exact = prefix.borrow()[0].clone();
    exact.next_serial += 1;
    assert_eq!(h.resources.state().unwrap(), exact);
    assert_eq!(h.k.lifecycle_state().callbacks, 1);
    assert_eq!(h.k.lifecycle_state().leases, 1);
    let snapshot = h.snapshot();
    assert_eq!(h.s.dispose(&mut h.k, id), Err(Error::NativeRefusal));
    assert_eq!(h.s.retry_cleanup(id), Err(Error::NativeRefusal));
    assert_eq!(h.snapshot(), snapshot); // only trusted discard observations changed
    assert_eq!(first.discards.get(), 2);
    assert_eq!(last.discards.get(), 2);
    assert_eq!(h.k.managed_unload(h.actor), Err(Error::Referenced));
    first.fail.set(false);
    h.s.retry_cleanup(id).unwrap();
    assert_eq!(first.discards.get(), 3);
    assert_eq!(last.discards.get(), 3);
    assert_eq!(h.seen.borrow().len(), 2);
    assert_eq!(h.resources.state().unwrap(), exact);
    assert!(first
        .bindings
        .borrow()
        .iter()
        .all(|b| Some(b) == task.invocation.as_ref()));
    assert_eq!(
        h.s.state().unwrap().tasks[&id].status,
        Status::Faulted(Fault::Cleanup)
    );
    assert!(!h.s.state().unwrap().tasks[&id].cleanup_pending);
    let mut foreign = Kernel::new(Limits::default()).unwrap();
    let snapshot = h.snapshot();
    let other = foreign.lifecycle_state();
    assert_eq!(h.s.dispose(&mut foreign, id), Err(Error::InvalidHandle));
    assert_eq!(h.snapshot(), snapshot);
    assert_eq!(foreign.lifecycle_state(), other);
    h.s.dispose(&mut h.k, id).unwrap();
    assert_eq!(h.k.managed_unload(h.actor), Err(Error::Referenced));
    // Cancel the other pending task through the authenticated real port, then dispose.
    let (lease, binding) = h.source(h.actor);
    let cancelled = h.invoke(
        &binding,
        &nethacked_timers::Request::Cancel { task: second },
    );
    assert_eq!(
        serde_json::from_slice::<nethacked_timers::TaskId>(&cancelled).unwrap(),
        second
    );
    h.k.release_pipeline(lease).unwrap();
    h.s.dispose(&mut h.k, second).unwrap();
    assert_eq!(h.k.lifecycle_state().references, 0);
}
#[test]
fn stored_schedule_survives_source_trap_and_terminal_capacity_requires_disposal() {
    let mut h = Host::with(
        TimerLimits {
            records: 1,
            ..TimerLimits::default()
        },
        Rc::new(Clock::default()),
        "",
        "drop unreachable",
        vec![],
        true,
    );
    let (lease, binding) = h.source(h.actor);
    let req = h.request(vec![1], 1, 1);
    assert_eq!(
        h.k.invoke_operation(&binding, &serde_json::to_vec(&req).unwrap(), auth()),
        Err(Error::Trap)
    );
    h.k.release_pipeline(lease).unwrap();
    let id = *h.s.state().unwrap().tasks.keys().next().unwrap();
    assert_eq!(h.deliver(1, 1).status, Status::Completed(vec![1]));
    let (lease, binding) = h.source(h.actor);
    let snapshot = h.snapshot();
    // Trap remains a committed prefix even when the native schedule refuses capacity.
    assert_eq!(
        h.k.invoke_operation(&binding, &serde_json::to_vec(&req).unwrap(), auth()),
        Err(Error::Trap)
    );
    assert_eq!(h.snapshot(), snapshot);
    h.k.release_pipeline(lease).unwrap();
    h.s.dispose(&mut h.k, id).unwrap();
    let (lease, binding) = h.source(h.actor);
    assert_eq!(
        h.k.invoke_operation(&binding, &serde_json::to_vec(&req).unwrap(), auth()),
        Err(Error::Trap)
    );
    let next = *h.s.state().unwrap().tasks.keys().next().unwrap();
    assert!(next.serial > id.serial);
    h.k.release_pipeline(lease).unwrap();
    assert_eq!(h.deliver(1, 1).id, next);
}

#[test]
fn actual_oversized_guest_root_reply_is_terminal_and_preserves_prefix() {
    let body = format!("{} drop i64.const 16385", transaction_body(false));
    let mut h = Host::with(
        TimerLimits::default(),
        Rc::new(Clock::default()),
        &body,
        "",
        vec![],
        true,
    );
    let prefix = Rc::new(RefCell::new(vec![]));
    let copy = prefix.clone();
    let resources = h.resources.clone();
    *h.hook.borrow_mut() = Some(Rc::new(move |_, _| {
        copy.borrow_mut().push(resources.state()?);
        Ok(vec![])
    }));
    let id = h.schedule(plan(&h), 1, 1);
    let task = h.deliver(1, 1);
    assert_eq!(
        task.status,
        Status::Faulted(Fault::Invocation(Error::Buffer))
    );
    assert!(!task.cleanup_pending);
    assert_eq!(h.seen.borrow().len(), 2);
    let mut exact = prefix.borrow()[0].clone();
    exact.next_serial += 1;
    assert_eq!(h.resources.state().unwrap(), exact);
    let before = h.snapshot();
    assert_eq!(h.s.deliver_next(&mut h.k, &context(2, 1)).unwrap(), None);
    assert_eq!(h.snapshot(), before);
    h.s.dispose(&mut h.k, id).unwrap();
}
