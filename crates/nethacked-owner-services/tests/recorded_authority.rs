mod support;
use nethacked_kernel::{Access, Authority, Error, Scope};
use nethacked_owners::{self as owners, *};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
    rc::Rc,
};
use support::*;

#[test]
fn recorded_observations_are_credentials_only_when_host_issued_and_freshly_authorized() {
    let mut h = Harness::new();
    let d = device(&mut h);
    let calls = Rc::new(Cell::new(0));
    let s = scalar(&mut h, &d, calls.clone());
    let declaration = RecordedDeclaration {
        policy: "recorded-nondeterministic-fixture".into(),
        version: 1,
        provider: nethacked_kernel::ProviderBinding {
            handle: h.input_module,
            artifact: h.kernel.artifact(h.input_module).unwrap(),
            endpoint: "input".into(),
        },
        request: b"choice".to_vec(),
    };
    s.declare_recorded_inputs("prepare", vec![declaration])
        .unwrap();
    let op = h.operation();
    let reply: nethacked_inputs::Reply = serde_json::from_slice(&h.run(
        &op,
        3,
        &wire(&nethacked_inputs::Request::Sample {
            request: b"choice".to_vec(),
        }),
        |_, _, _| panic!("real input service"),
    ))
    .unwrap();
    let observation = observe(&h, &op, d.clone());
    let target = s.clone();
    let service = h.inputs.clone();
    let host = h.host.clone();
    let saved = Rc::new(RefCell::new(None));
    let sealed = saved.clone();
    let receipt = Rc::new(RefCell::new(None));
    let retained_input = receipt.clone();
    h.run(&op, 1, &[], move |_, _, v| {
        let input = service.observation(reply.id, b"choice", v).unwrap();
        let state = service.inspect().unwrap();
        let mut edited = state.clone();
        edited.records[0].value = vec![99];
        assert_eq!(input.bytes(), [3]);
        assert_eq!(service.inspect().unwrap(), state);
        let mut forged = reply.id;
        forged.serial += 1;
        assert!(service.observation(forged, b"choice", v).is_err());
        assert!(service
            .observation(reply.id, b"different-request", v)
            .is_err());
        assert!(input.validate(&v.clone()).is_err());
        let candidate = target
            .prepare_recorded(
                None,
                "prepare",
                &[],
                vec![observation.clone()],
                std::slice::from_ref(&input),
                v,
            )
            .unwrap();
        *retained_input.borrow_mut() = Some(input);
        *sealed.borrow_mut() = Some(
            host.batch(
                vec![candidate.erase()],
                vec![],
                &[target.id()],
                vec![target.id()],
                [7; 32],
                Rc::new(Accept(Rc::new(Cell::new(0)))),
                cap("owners", "effect"),
                v,
            )
            .unwrap(),
        );
        Ok(vec![])
    });
    let prefix = h.inputs.inspect().unwrap();
    let state = s.inspect().unwrap();
    let history = h.host.history().unwrap();
    assert_eq!(calls.get(), 1);
    assert_eq!(h.samples.get(), 1);
    // Changing only the input grant invalidates commit even though owner grants
    // and its original helper capture remain available.
    h.kernel
        .issue_grant(
            h.actor,
            3,
            Authority {
                access: Access::Write,
                scope: Scope::Keys(BTreeSet::new()),
            },
        )
        .unwrap();
    let batch = saved.clone();
    h.run(&op, 5, &[], move |_, _, v| {
        assert_eq!(
            batch.borrow().as_ref().unwrap().commit(v),
            Err(owners::Error::Kernel(Error::Authority))
        );
        Ok(vec![])
    });
    assert_eq!(s.inspect().unwrap(), state);
    assert_eq!(h.host.history().unwrap(), history);
    assert_eq!(h.inputs.inspect().unwrap(), prefix);
    assert_eq!(h.samples.get(), 1);
    assert_eq!(calls.get(), 1);
    h.grants();
    let foreign = h.operation();
    let input = receipt.clone();
    h.run(&foreign, 5, &[], move |_, _, v| {
        assert_eq!(
            input.borrow().as_ref().unwrap().validate(v),
            Err(Error::Authority)
        );
        Ok(vec![])
    });
    // Legacy replay still refuses another operation, even if the request/ID are exact.
    let refused = h.kernel.invoke_operation(
        &foreign,
        &[
            &[3],
            wire(&nethacked_inputs::Request::Replay {
                id: reply.id,
                request: b"choice".to_vec(),
            })
            .as_slice(),
        ]
        .concat(),
        all(),
    );
    assert_eq!(refused, Err(Error::Trap));
    assert_eq!(h.inputs.inspect().unwrap(), prefix);
    assert_eq!(h.samples.get(), 1);
    saved.borrow().as_ref().unwrap().discard().unwrap();
    saved.borrow_mut().take();
    receipt.borrow_mut().take();
    h.kernel.release_pipeline(op.pipeline()).unwrap();
    h.kernel.release_pipeline(foreign.pipeline()).unwrap();
    drop(s);
    drop(d);
    assert_eq!(h.kernel.lifecycle_state().references, 0);
}

#[test]
fn weak_registry_bounds_and_foreign_target_selection_preserve_state_and_release_references() {
    use nethacked_owner_services::{CallbackBehaviors, Targets};
    let mut h = Harness::new();
    let d = device(&mut h);
    let s = scalar(&mut h, &d, Rc::new(Cell::new(0)));
    let op = h.operation();
    let targets = Targets::new(1).unwrap();
    let target = targets.register(&s, "prepare").unwrap();
    assert_eq!(targets.register(&d, "query"), Err(Error::Limit));
    assert_eq!(targets.current(&target.address).unwrap(), target);
    let behaviors = Rc::new(CallbackBehaviors::new(1).unwrap());
    let registry = behaviors.clone();
    let owner = s.clone();
    let held = Rc::new(RefCell::new(None));
    let sink = held.clone();
    h.run(&op, 1, &[], move |_, _, v| {
        let reference = owner.retain_behavior("prepare", v).unwrap();
        *sink.borrow_mut() = Some(registry.register(reference.clone()).unwrap());
        assert!(matches!(registry.register(reference), Err(Error::Limit)));
        Ok(vec![])
    });
    assert_eq!(h.host.retained_behaviors().unwrap(), 1);
    assert_eq!(s.inspect().unwrap(), 5u32.to_le_bytes());
    assert!(h.host.history().unwrap().is_empty());
    held.borrow_mut().take();
    assert_eq!(h.host.retained_behaviors().unwrap(), 0);
    drop(s);
    assert!(targets.current(&target.address).is_err());
    targets.remove(&target.address).unwrap();
    h.kernel.release_pipeline(op.pipeline()).unwrap();
    drop(d);
    assert_eq!(h.kernel.lifecycle_state().references, 0);
}
