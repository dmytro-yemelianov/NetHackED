mod support;
use nethacked_kernel::*;
use nethacked_owner_services::BehaviorHandle;
use nethacked_owners::{self as owners, Owner};
use nethacked_timers::{CallbackRetention, Request, TaskId};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use support::*;

fn capture(h: &Harness, source: &OperationBinding, owner: Rc<Owner<u32>>) -> BehaviorHandle<u32> {
    let saved = Rc::new(RefCell::new(None));
    let sink = saved.clone();
    let routes = h.behaviors.clone();
    h.run(source, 1, &[], move |_, _, view| {
        *sink.borrow_mut() = Some(
            routes
                .register(owner.retain_behavior("prepare", view).unwrap())
                .unwrap(),
        );
        Ok(vec![])
    });
    let result = saved.borrow_mut().take().unwrap();
    result
}

#[test]
fn admission_positive_and_pre_admission_masking_control() {
    let mut h = Harness::with_owner_limits(owners::Limits {
        candidates: 2,
        ..owners::Limits::default()
    });
    let d = device(&mut h);
    let calls = Rc::new(Cell::new(0));
    let s = scalar(&mut h, &d, calls.clone());
    let source = h.operation();
    let handle = capture(&h, &source, s.clone());
    let targets = vec![
        h.targets.register(&s, "prepare").unwrap(),
        h.targets.register(&d, "query").unwrap(),
    ];
    let request = Request::Schedule {
        callback: "callback".into(),
        payload: vec![],
        targets,
        deadline: vec![1],
        residency: vec![1],
        ceiling: Authority {
            access: Access::Write,
            scope: Scope::Keys(h.keys.clone()),
        },
    };
    let before = h.timers.state().unwrap();
    let lifecycle = h.kernel.lifecycle_state();
    let state = s.inspect().unwrap();
    let history = h.host.history().unwrap();
    // Identical outer Trap, but malformed wire fails before retention admission.
    assert_eq!(
        h.kernel.invoke_operation(&source, &[4, b'{'], all()),
        Err(Error::Trap)
    );
    assert!(h.retention.preflight.borrow().is_empty());
    assert_eq!(h.retention.retained.get(), 0);
    assert_eq!(h.timers.state().unwrap(), before);
    assert_eq!(h.kernel.lifecycle_state(), lifecycle);
    assert_eq!(s.inspect().unwrap(), state);
    assert_eq!(h.host.history().unwrap(), history);
    assert_eq!(h.host.retained_behaviors().unwrap(), 1);
    assert_eq!(calls.get(), 0);
    let task: TaskId = serde_json::from_slice(&h.run(&source, 4, &wire(&request), |_, _, _| {
        panic!("actual queue")
    }))
    .unwrap();
    assert_eq!(*h.retention.preflight.borrow(), vec![Ok(())]);
    assert_eq!(h.retention.retained.get(), 1);
    assert_eq!(h.host.retained_behaviors().unwrap(), 2);
    let after = h.timers.state().unwrap();
    assert_eq!(after.next_serial, before.next_serial + 1);
    assert_eq!(after.tasks.len(), 1);
    assert_eq!(after.active, before.active);
    assert_eq!(
        h.kernel.lifecycle_state().next_callback,
        lifecycle.next_callback + 1
    );
    assert_eq!(
        h.kernel.lifecycle_state().callbacks,
        lifecycle.callbacks + 1
    );
    assert_eq!(s.inspect().unwrap(), state);
    assert_eq!(h.host.history().unwrap(), history);
    assert_eq!(calls.get(), 0);
    h.run(&source, 4, &wire(&Request::Cancel { task }), |_, _, _| {
        panic!("actual queue")
    });
    h.timers.dispose(&mut h.kernel, task).unwrap();
    drop(handle);
    assert_eq!(h.host.retained_behaviors().unwrap(), 0);
    h.kernel.release_pipeline(source.pipeline()).unwrap();
    drop(s);
    drop(d);
    assert_eq!(h.kernel.lifecycle_state().references, 0);
}

fn adopt(h: &mut Harness, callback: &RetainedCallback) -> OperationBinding {
    let (operation, lease) = h.kernel.adopt_callback(callback).unwrap();
    h.kernel
        .operation_binding_with_cleanup(
            operation,
            lease,
            0,
            "compose",
            &[h.host.cleanup_identity().unwrap()],
        )
        .unwrap()
}

#[test]
fn same_source_scoped_reference_refuses_endpoint_service_and_slot() {
    let mut h = Harness::for_binding_review();
    let d = device(&mut h);
    let old_calls = Rc::new(Cell::new(0));
    let s = scalar(&mut h, &d, old_calls.clone());
    let source = h.operation();
    let handle = capture(&h, &source, s.clone());
    let weak = Rc::new(handle.downgrade());
    let targets = vec![
        h.targets.register(&s, "prepare").unwrap(),
        h.targets.register(&d, "query").unwrap(),
    ];
    let captures = Rc::new(RefCell::new(vec![]));
    for (slot, endpoint) in [
        (5, "review-route"),
        (5, "review-endpoint"),
        (6, "review-route"),
        (7, "review-route"),
    ] {
        let sink = captures.clone();
        let retention = h.retention.clone();
        let targets = targets.clone();
        h.run(&source, slot, &[], move |_, _, view| {
            retention.preflight(view, &targets).unwrap();
            let callback = view
                .capture_callback(endpoint, view.authority().clone())
                .unwrap();
            let reference = retention.retain(&callback, &targets).unwrap();
            sink.borrow_mut().push((callback, reference));
            Ok(vec![])
        });
    }
    let captures = captures.borrow_mut().drain(..).collect::<Vec<_>>();
    let correct = captures[0].0.provenance().clone();
    for (index, (callback, _)) in captures.iter().enumerate().skip(1) {
        let other = callback.provenance();
        assert_eq!(other.source, correct.source);
        assert_eq!(
            (other.caller.handle, other.caller.artifact),
            (correct.caller.handle, correct.caller.artifact)
        );
        match index {
            1 => {
                assert_ne!(other.caller.endpoint, correct.caller.endpoint);
                assert_eq!(other.service, correct.service);
                assert_eq!(other.capability, correct.capability);
                assert_eq!(other.slot, correct.slot);
            }
            2 => {
                assert_eq!(other.caller, correct.caller);
                assert_eq!(other.service, correct.service);
                assert_eq!(other.capability, correct.capability);
                assert_ne!(other.slot, correct.slot);
            }
            3 => {
                assert_eq!(other.caller, correct.caller);
                assert_ne!(other.service, correct.service);
                assert_ne!(other.capability, correct.capability);
            }
            _ => unreachable!(),
        }
    }
    let operations = captures
        .iter()
        .map(|(cb, _)| adopt(&mut h, cb))
        .collect::<Vec<_>>();
    drop(handle);
    h.kernel.release_pipeline(source.pipeline()).unwrap();
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
    let saved = Rc::new(RefCell::new(None));
    let scoped = saved.clone();
    let owner = s.clone();
    let device = d.clone();
    let host = h.host.clone();
    let valid = weak.clone();
    h.run_scoped(&operations[0], 5, move |_, _, view| {
        let retained = valid.for_execution(view).unwrap();
        assert!(retained.matches_execution(view));
        let observation = device.query("query", &[], &[], view).unwrap();
        let candidate = owner
            .prepare_retained(&retained, "prepare", &[], vec![observation], view)
            .unwrap();
        commit(&host, &owner, candidate, view, Rc::new(Cell::new(0)));
        *scoped.borrow_mut() = Some(retained);
        Ok(vec![])
    });
    assert_eq!(s.inspect().unwrap(), 10u32.to_le_bytes());
    assert_eq!(old_calls.get(), 1);
    assert_eq!(new_calls.get(), 0);
    let scoped = saved.borrow_mut().take().unwrap();
    let reached = Rc::new(Cell::new(0));
    for (index, operation) in operations.iter().enumerate().skip(1) {
        let before = h.timers.state().unwrap();
        let lifecycle = h.kernel.lifecycle_state();
        let state = s.inspect().unwrap();
        let history = h.host.history().unwrap();
        let references = h.host.retained_behaviors().unwrap();
        let owner = s.clone();
        let device = d.clone();
        let reference = scoped.clone();
        let expected = correct.clone();
        let actual_origin = captures[index].0.provenance().clone();
        let owner_provider = h.module;
        let count = reached.clone();
        // Keep each capture's foreign provenance, but query through the real owner
        // port so an unrelated provider mismatch cannot explain the refusal.
        h.run_scoped(operation, 5, move |_, _, view| {
            count.set(count.get() + 1);
            let origin = view.callback_origin().unwrap();
            assert_eq!(origin, &actual_origin);
            assert_eq!(view.provider(), owner_provider);
            assert_eq!(origin.source, expected.source);
            assert_eq!(view.caller(), Some(expected.caller.handle));
            assert_eq!(view.caller_artifact(), Some(expected.caller.artifact));
            let observation = device.query("query", &[], &[], view).unwrap();
            assert!(matches!(
                owner.prepare_retained(&reference, "prepare", &[], vec![observation.clone()], view),
                Err(owners::Error::Authority)
            ));
            assert!(matches!(
                owner.query_retained(&reference, "derived", &[], &[observation], view),
                Err(owners::Error::Authority)
            ));
            Ok(vec![])
        });
        assert_eq!(reached.get(), index);
        assert_eq!(h.timers.state().unwrap(), before);
        assert_eq!(h.kernel.lifecycle_state(), lifecycle);
        assert_eq!(s.inspect().unwrap(), state);
        assert_eq!(h.host.history().unwrap(), history);
        assert_eq!(h.host.retained_behaviors().unwrap(), references);
        assert_eq!(old_calls.get(), 1);
        assert_eq!(new_calls.get(), 0);
    }
    // A freshly retrieved correct reference and the original scoped reference both survive refusals.
    let owner = s.clone();
    let device = d.clone();
    let host = h.host.clone();
    let valid = weak.clone();
    h.run_scoped(&operations[0], 5, move |_, _, view| {
        let fresh = valid.for_execution(view).unwrap();
        let observation = device.query("query", &[], &[], view).unwrap();
        assert_eq!(
            owner
                .query_retained(
                    &scoped,
                    "derived",
                    &[],
                    std::slice::from_ref(&observation),
                    view
                )
                .unwrap()
                .bytes(),
            15u32.to_le_bytes()
        );
        let candidate = owner
            .prepare_retained(&fresh, "prepare", &[], vec![observation], view)
            .unwrap();
        commit(&host, &owner, candidate, view, Rc::new(Cell::new(0)));
        Ok(vec![])
    });
    assert_eq!(s.inspect().unwrap(), 15u32.to_le_bytes());
    assert_eq!(h.host.history().unwrap().len(), 2);
    assert_eq!(old_calls.get(), 3);
    assert_eq!(new_calls.get(), 0);
    for operation in operations {
        h.kernel.release_pipeline(operation.pipeline()).unwrap();
    }
    drop(captures);
    assert!(!weak.is_retained());
    assert_eq!(h.host.retained_behaviors().unwrap(), 0);
    drop(s);
    drop(d);
    assert_eq!(h.kernel.lifecycle_state().references, 0);
    assert_eq!(h.kernel.lifecycle_state().callbacks, 0);
}
