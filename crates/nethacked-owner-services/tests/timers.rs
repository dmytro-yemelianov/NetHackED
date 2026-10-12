mod support;
use nethacked_kernel::{Authority, Error, OperationBinding, Scope};
use nethacked_owner_services::{BehaviorHandle, WeakBehavior};
use nethacked_owners::{self as owners, Owner};
use nethacked_timers::{Request, Status, TaskId};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use support::*;

fn capture(h: &Harness, op: &OperationBinding, s: Rc<Owner<u32>>) -> BehaviorHandle<u32> {
    let saved = Rc::new(RefCell::new(None));
    let sink = saved.clone();
    let routes = h.behaviors.clone();
    h.run(op, 1, &[], move |_, _, v| {
        *sink.borrow_mut() = Some(
            routes
                .register(s.retain_behavior("prepare", v).unwrap())
                .unwrap(),
        );
        Ok(vec![])
    });
    let handle = saved.borrow_mut().take().unwrap();
    handle
}

#[test]
fn callback_scope_exhaustion_and_undeclared_owner_refuse_before_capture_ids() {
    for exhausted in [false, true] {
        let mut h = Harness::with_owner_limits(owners::Limits {
            candidates: if exhausted { 1 } else { 8 },
            ..owners::Limits::default()
        });
        let d = device(&mut h);
        let reached = Rc::new(Cell::new(0));
        let s = scalar(&mut h, &d, reached.clone());
        let op = h.operation();
        let handle = capture(&h, &op, s.clone());
        let scalar_target = h.targets.register(&s, "prepare").unwrap();
        let device_target = h.targets.register(&d, "query").unwrap();
        let before = h.timers.state().unwrap();
        let lifecycle = h.kernel.lifecycle_state();
        let owner = s.inspect().unwrap();
        let history = h.host.history().unwrap();
        let request = Request::Schedule {
            callback: "callback".into(),
            payload: vec![],
            targets: if exhausted {
                vec![scalar_target, device_target]
            } else {
                vec![device_target]
            },
            deadline: vec![1],
            residency: vec![1],
            ceiling: Authority {
                access: nethacked_kernel::Access::Write,
                scope: Scope::Keys(h.keys.clone()),
            },
        };
        let result =
            h.kernel
                .invoke_operation(&op, &[&[4], wire(&request).as_slice()].concat(), all());
        assert_eq!(result, Err(Error::Trap));
        assert_eq!(
            *h.retention.preflight.borrow(),
            vec![Err(if exhausted {
                Error::NativeRefusal
            } else {
                Error::Authority
            })]
        );
        assert_eq!(h.retention.retained.get(), 0);
        assert_eq!(h.timers.state().unwrap(), before);
        assert_eq!(h.kernel.lifecycle_state(), lifecycle);
        assert_eq!(s.inspect().unwrap(), owner);
        assert_eq!(h.host.history().unwrap(), history);
        assert_eq!(reached.get(), 0);
        assert_eq!(h.host.retained_behaviors().unwrap(), 1);
        drop(handle);
        assert_eq!(h.host.retained_behaviors().unwrap(), 0);
        h.kernel.release_pipeline(op.pipeline()).unwrap();
        drop(s);
        drop(d);
        assert_eq!(h.kernel.lifecycle_state().references, 0);
    }
}
fn schedule(
    h: &Harness,
    op: &OperationBinding,
    callback: &str,
    targets: Vec<nethacked_resources::Stamp>,
    deadline: u8,
    residency: u8,
) -> TaskId {
    let request = Request::Schedule {
        callback: callback.into(),
        payload: vec![1],
        targets,
        deadline: vec![deadline],
        residency: vec![residency],
        ceiling: Authority {
            access: nethacked_kernel::Access::Write,
            scope: Scope::Keys(h.keys.clone()),
        },
    };
    serde_json::from_slice(&h.run(op, 4, &wire(&request), |_, _, _| {
        panic!("schedule uses actual queue")
    }))
    .unwrap()
}

#[test]
fn old_captured_callback_first_prepares_after_source_disposal_with_busy_cleanup_and_trap_prefix() {
    let mut h = Harness::new();
    let d = device(&mut h);
    let old_calls = Rc::new(Cell::new(0));
    let s = scalar(&mut h, &d, old_calls.clone());
    let source = h.operation();
    let handle = capture(&h, &source, s.clone());
    let weak = handle.downgrade();
    let unbound = Rc::new(RefCell::new(Some(handle.behavior().clone())));
    let target = h.targets.register(&s, "prepare").unwrap();
    let device_target = h.targets.register(&d, "query").unwrap();
    let task = schedule(&h, &source, "callback", vec![target, device_target], 2, 1);
    drop(handle);
    h.kernel.release_pipeline(source.pipeline()).unwrap();
    assert!(weak.is_retained());
    assert_eq!(old_calls.get(), 0);
    assert_eq!(h.host.retained_behaviors().unwrap(), 2);
    let (_, binding) = h.replace();
    let new_calls = Rc::new(Cell::new(0));
    s.admit_compatible_wasm(
        &h.kernel,
        [2; 32],
        Rc::new(ScalarCodec(new_calls.clone())),
        0,
        binding,
    )
    .unwrap();
    // Profile readiness/residency and live grant refusal preserve the entire queue.
    let initial = h.timers.state().unwrap();
    assert_eq!(h.timers.deliver_next(&mut h.kernel, &[1, 1]).unwrap(), None);
    assert_eq!(h.timers.deliver_next(&mut h.kernel, &[2, 9]).unwrap(), None);
    assert_eq!(h.timers.state().unwrap(), initial);
    h.kernel.revoke_grant(h.actor, 4).unwrap();
    let state = h.timers.state().unwrap();
    assert_eq!(
        h.timers.deliver_next(&mut h.kernel, &[2, 1]),
        Err(Error::Revoked)
    );
    assert_eq!(h.timers.state().unwrap(), state);
    assert!(h.host.history().unwrap().is_empty());
    h.grants();
    let host = h.host.clone();
    let source_s = s.clone();
    let device = d.clone();
    let queue = h.timers.clone();
    let pending = Rc::new(RefCell::new(None));
    let pending_sink = pending.clone();
    let held_gate = Rc::new(RefCell::new(None));
    let held = held_gate.clone();
    let calls = Rc::new(Cell::new(0));
    let reached = calls.clone();
    let checks = Rc::new(Cell::new(0));
    let checked = checks.clone();
    *h.action.borrow_mut() = Some(Box::new(move |name, _, view| {
        assert_eq!(name, "effect");
        reached.set(reached.get() + 1);
        assert_ne!(view.operation().unwrap().operation(), source.operation());
        let retained = weak.for_execution(view).unwrap();
        let unbound = unbound.borrow_mut().take().unwrap();
        assert!(matches!(
            source_s.query_retained(&unbound, "derived", &[], &[], view),
            Err(owners::Error::Authority)
        ));
        drop(unbound);
        let observation = device.query("query", &[], &[], view).unwrap();
        let query = source_s
            .query_retained(
                &retained,
                "derived",
                &[],
                std::slice::from_ref(&observation),
                view,
            )
            .unwrap();
        assert_eq!(query.bytes(), 10u32.to_le_bytes());
        let candidate = source_s
            .prepare_retained(&retained, "prepare", &[], vec![observation.clone()], view)
            .unwrap();
        commit(&host, &source_s, candidate, view, checked.clone());
        *pending_sink.borrow_mut() = Some(
            source_s
                .prepare_retained(&retained, "prepare", &[], vec![observation], view)
                .unwrap(),
        );
        let mut foreign =
            nethacked_kernel::Kernel::new(nethacked_kernel::Limits::default()).unwrap();
        let before = queue.state().unwrap();
        assert_eq!(
            queue.deliver_next(&mut foreign, &[9, 1]),
            Err(Error::NativeRefusal)
        );
        assert_eq!(queue.state().unwrap(), before);
        *held.borrow_mut() = Some(host.close_visibility().unwrap());
        Err(Error::Trap)
    }));
    let delivered = h
        .timers
        .deliver_next(&mut h.kernel, &[2, 1])
        .unwrap()
        .unwrap();
    assert_eq!(delivered.id, task);
    assert_eq!(
        delivered.status,
        Status::Faulted(nethacked_timers::Fault::Invocation(Error::Trap))
    );
    assert!(delivered.cleanup_pending);
    assert_eq!(calls.get(), 1);
    assert_eq!(old_calls.get(), 3);
    assert_eq!(new_calls.get(), 0);
    assert!(matches!(s.inspect(), Err(owners::Error::Busy)));
    assert!(h.timers.retry_cleanup(task).is_err());
    assert!(h.timers.dispose(&mut h.kernel, task).is_err());
    held_gate.borrow_mut().take();
    let prefix = h.host.history().unwrap();
    assert_eq!(prefix.len(), 1);
    assert_eq!(s.inspect().unwrap(), 10u32.to_le_bytes());
    assert_eq!(h.host.pending().unwrap(), 1);
    h.timers.retry_cleanup(task).unwrap();
    h.host
        .discard_operation(delivered.invocation.as_ref().unwrap())
        .unwrap();
    h.host
        .discard_operation(delivered.invocation.as_ref().unwrap())
        .unwrap();
    assert_eq!(h.host.pending().unwrap(), 0);
    assert_eq!(h.host.retained_behaviors().unwrap(), 0);
    assert_eq!(h.timers.deliver_next(&mut h.kernel, &[9, 1]).unwrap(), None);
    assert_eq!(h.host.history().unwrap(), prefix);
    assert_eq!(calls.get(), 1);
    pending.borrow_mut().take();
    h.action.borrow_mut().take();
    h.timers.dispose(&mut h.kernel, task).unwrap();
    drop(s);
    drop(d);
    assert_eq!(h.kernel.lifecycle_state().references, 0);
}

#[test]
fn successor_uses_retained_codec_finite_targets_and_profile_ties_without_recursive_delivery() {
    let mut h = Harness::new();
    let d = device(&mut h);
    let old_calls = Rc::new(Cell::new(0));
    let s = scalar(&mut h, &d, old_calls.clone());
    let source = h.operation();
    let handle = capture(&h, &source, s.clone());
    let weak = handle.downgrade();
    let scalar_target = h.targets.register(&s, "prepare").unwrap();
    let device_target = h.targets.register(&d, "query").unwrap();
    let first = schedule(
        &h,
        &source,
        "successor",
        vec![scalar_target, device_target],
        2,
        1,
    );
    drop(handle);
    h.kernel.release_pipeline(source.pipeline()).unwrap();
    let (_, binding) = h.replace();
    s.admit_compatible_wasm(
        &h.kernel,
        [2; 32],
        Rc::new(ScalarCodec(Rc::new(Cell::new(0)))),
        0,
        binding,
    )
    .unwrap();
    let successor = Rc::new(RefCell::new(None::<WeakBehavior<u32>>));
    let successor_handle = Rc::new(RefCell::new(None::<BehaviorHandle<u32>>));
    let successor_id = Rc::new(Cell::new(None));
    let id_sink = successor_id.clone();
    let successor_sink = successor.clone();
    let held = successor_handle.clone();
    let host = h.host.clone();
    let target = s.clone();
    let device = d.clone();
    let registry = h.behaviors.clone();
    let routes = h.targets.clone();
    let keys = h.keys.clone();
    let reached = Rc::new(Cell::new(0));
    let calls = reached.clone();
    *h.action.borrow_mut() = Some(Box::new(move |_, bytes, view| {
        calls.set(calls.get() + 1);
        if let Ok(id) = serde_json::from_slice::<TaskId>(bytes) {
            id_sink.set(Some(id));
            held.borrow_mut().take();
            return Ok(wire(&nethacked_timers::Outcome::Completed {
                result: vec![10],
            }));
        }
        let retained = if successor_sink.borrow().is_some() {
            successor_sink
                .borrow()
                .as_ref()
                .unwrap()
                .for_execution(view)
                .unwrap()
        } else {
            weak.for_execution(view).unwrap()
        };
        let observation = device.query("query", &[], &[], view).unwrap();
        let candidate = target
            .prepare_retained(&retained, "prepare", &[], vec![observation], view)
            .unwrap();
        commit(&host, &target, candidate, view, Rc::new(Cell::new(0)));
        if successor_sink.borrow().is_none() {
            let next = registry
                .register(
                    target
                        .retain_from_behavior(&retained, "prepare", view)
                        .unwrap(),
                )
                .unwrap();
            *successor_sink.borrow_mut() = Some(next.downgrade());
            *held.borrow_mut() = Some(next);
            let targets = vec![
                routes
                    .current(&target.resource("prepare").unwrap())
                    .unwrap(),
                routes.current(&device.resource("query").unwrap()).unwrap(),
            ];
            return Ok(wire(&Request::Schedule {
                callback: "callback".into(),
                payload: vec![2],
                targets,
                deadline: vec![3],
                residency: vec![1],
                ceiling: Authority {
                    access: nethacked_kernel::Access::Write,
                    scope: Scope::Keys(keys.clone()),
                },
            }));
        }
        Ok(wire(&nethacked_timers::Outcome::Completed {
            result: vec![15],
        }))
    }));
    let delivered = h
        .timers
        .deliver_next(&mut h.kernel, &[2, 1])
        .unwrap()
        .unwrap();
    assert_eq!(delivered.id, first);
    assert_eq!(delivered.status, Status::Completed(vec![10]));
    let next = successor_id.get().unwrap();
    assert_eq!(h.host.retained_behaviors().unwrap(), 2);
    assert!(successor.borrow().as_ref().unwrap().is_retained());
    assert_eq!(h.timers.deliver_next(&mut h.kernel, &[2, 1]).unwrap(), None);
    let delivered = h
        .timers
        .deliver_next(&mut h.kernel, &[3, 1])
        .unwrap()
        .unwrap();
    assert_eq!(delivered.id, next);
    assert_eq!(delivered.status, Status::Completed(vec![15]));
    assert_eq!(reached.get(), 3);
    assert_eq!(old_calls.get(), 2);
    assert_eq!(s.inspect().unwrap(), 15u32.to_le_bytes());
    assert_eq!(h.host.history().unwrap().len(), 2);
    assert_eq!(h.host.retained_behaviors().unwrap(), 0);
    assert!(!successor.borrow().as_ref().unwrap().is_retained());
    h.timers.dispose(&mut h.kernel, first).unwrap();
    h.timers.dispose(&mut h.kernel, next).unwrap();
    h.action.borrow_mut().take();
    drop(s);
    drop(d);
    assert_eq!(h.kernel.lifecycle_state().references, 0);
}

#[test]
fn stale_retired_targets_cancel_and_dispose_release_behavior_without_execution() {
    for retired in [false, true] {
        let mut h = Harness::new();
        let d = device(&mut h);
        let calls = Rc::new(Cell::new(0));
        let s = scalar(&mut h, &d, calls.clone());
        let op = h.operation();
        let handle = capture(&h, &op, s.clone());
        let weak = handle.downgrade();
        let target = h.targets.register(&s, "prepare").unwrap();
        let device_target = h.targets.register(&d, "query").unwrap();
        let task = schedule(&h, &op, "callback", vec![target, device_target], 1, 1);
        drop(handle);
        if retired {
            h.host.retire(s.id()).unwrap();
        } else {
            let device = d.clone();
            let host = h.host.clone();
            h.run(&op, 1, &[], move |_, _, v| {
                let candidate = device
                    .prepare("prepare", &5u32.to_le_bytes(), vec![], v)
                    .unwrap();
                commit(&host, &device, candidate, v, Rc::new(Cell::new(0)));
                Ok(vec![])
            });
        }
        let before = h.timers.state().unwrap();
        let history = h.host.history().unwrap();
        assert!(h.timers.deliver_next(&mut h.kernel, &[1, 1]).is_err());
        assert_eq!(h.timers.state().unwrap(), before);
        assert_eq!(h.host.history().unwrap(), history);
        assert_eq!(calls.get(), 0);
        assert!(weak.is_retained());
        h.run(&op, 4, &wire(&Request::Cancel { task }), |_, _, _| {
            panic!("cancel uses actual queue")
        });
        assert!(!weak.is_retained());
        assert_eq!(h.host.retained_behaviors().unwrap(), 0);
        h.timers.dispose(&mut h.kernel, task).unwrap();
        h.kernel.release_pipeline(op.pipeline()).unwrap();
        drop(s);
        drop(d);
        assert_eq!(h.kernel.lifecycle_state().references, 0);
    }
}
