mod support;
use nethacked_owners::{self as owners, *};
use nethacked_workflows::{Status, Workflows};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use support::*;

#[test]
fn paused_recorded_workflow_first_queries_prepares_and_atomically_publishes_old_guests() {
    let mut h = Harness::new();
    let d = device(&mut h);
    let old_scalar = Rc::new(Cell::new(0));
    let old_field = Rc::new(Cell::new(0));
    let s = scalar(&mut h, &d, old_scalar.clone());
    let f = field(&mut h, &d, old_field.clone());
    let provider = nethacked_kernel::ProviderBinding {
        handle: h.input_module,
        artifact: [0; 32],
        endpoint: "input".into(),
    };
    let mut declaration = RecordedDeclaration {
        policy: "recorded-nondeterministic-fixture".into(),
        version: 1,
        provider,
        request: b"choice".to_vec(),
    };
    declaration.provider.artifact = h.kernel.artifact(declaration.provider.handle).unwrap();
    s.declare_recorded_inputs("prepare", vec![declaration.clone()])
        .unwrap();
    s.declare_recorded_inputs("derived", vec![declaration])
        .unwrap();
    assert_eq!(
        s.set_query_policy(QueryPolicy::Deterministic {
            profile: [1; 32],
            input_policy: [2; 32]
        }),
        Err(owners::Error::Unsupported)
    );
    let cache = s.query_cache().unwrap();
    let workflows = Workflows::new(nethacked_workflows::Limits::default()).unwrap();
    let program = h.program("workflow");
    let id = workflows
        .start(
            &mut h.kernel,
            program,
            vec![],
            vec![Rc::new(h.host.clone())],
        )
        .unwrap();
    let held = Rc::new(RefCell::new(None));
    let retained = held.clone();
    let source_s = s.clone();
    let source_f = f.clone();
    let device = d.clone();
    let inputs = h.inputs.clone();
    let branches = Rc::new(Cell::new(0));
    let reached = branches.clone();
    let host = h.host.clone();
    let cached = cache.clone();
    let preflights = Rc::new(Cell::new(0));
    let checks = preflights.clone();
    *h.action.borrow_mut() = Some(Box::new(move |name, bytes, view| {
        assert_eq!(name, "effect");
        reached.set(reached.get() + 1);
        if serde_json::from_slice::<nethacked_workflows::StepRequest>(bytes).is_ok() {
            return Ok(if view.operation().unwrap().phase() == "capture" {
                wire(&nethacked_inputs::Request::Sample {
                    request: b"choice".to_vec(),
                })
            } else {
                let state = retained.borrow();
                let (_, _, _, input): &(
                    RetainedBehavior<u32>,
                    RetainedBehavior<Field>,
                    Observation,
                    nethacked_inputs::Observation,
                ) = state.as_ref().unwrap();
                wire(&nethacked_inputs::Request::Replay {
                    id: input.identity().record,
                    request: b"choice".to_vec(),
                })
            });
        }
        let reply: nethacked_inputs::Reply = serde_json::from_slice(bytes).unwrap();
        let input = inputs.observation(reply.id, b"choice", view).unwrap();
        assert_eq!(input.bytes(), [3]);
        if view.operation().unwrap().phase() == "capture" {
            let obs = device.query("query", &[], &[], view).unwrap();
            *retained.borrow_mut() = Some((
                source_s.retain_behavior("prepare", view).unwrap(),
                source_f.retain_behavior("prepare", view).unwrap(),
                obs,
                input,
            ));
            return Ok(wire(&nethacked_workflows::Outcome::Suspend {
                next: "apply".into(),
                prompt: vec![1],
                state: vec![],
                events: vec![b"choice-prefix".to_vec()],
            }));
        }
        let state = retained.borrow();
        let (scalar_behavior, field_behavior, obs, _) = state.as_ref().unwrap();
        assert!(matches!(
            source_s.query_cached(&cached, "derived", &[], std::slice::from_ref(obs), view),
            Err(owners::Error::Unsupported)
        ));
        let query = source_s
            .query_recorded(
                Some(scalar_behavior),
                "derived",
                &[],
                std::slice::from_ref(obs),
                std::slice::from_ref(&input),
                view,
            )
            .unwrap();
        assert_eq!(query.bytes(), 13u32.to_le_bytes());
        let scalar_candidate = source_s
            .prepare_recorded(
                Some(scalar_behavior),
                "prepare",
                &[],
                vec![obs.clone()],
                std::slice::from_ref(&input),
                view,
            )
            .unwrap();
        let field_query = source_f
            .query_retained(
                field_behavior,
                "derived",
                &[],
                std::slice::from_ref(obs),
                view,
            )
            .unwrap();
        assert_eq!(
            field_query.bytes(),
            [12u32.to_le_bytes(), 11u32.to_le_bytes()].concat()
        );
        let field_candidate = source_f
            .prepare_retained(field_behavior, "prepare", &[], vec![obs.clone()], view)
            .unwrap();
        let writes = [source_s.id(), source_f.id()];
        host.batch(
            vec![scalar_candidate.erase(), field_candidate.erase()],
            vec![query, field_query],
            &writes,
            vec![source_f.id(), source_s.id()],
            [7; 32],
            Rc::new(Accept(checks.clone())),
            cap("owners", "effect"),
            view,
        )
        .unwrap()
        .commit(view)
        .unwrap();
        Ok(wire(&nethacked_workflows::Outcome::Finish {
            result: vec![13],
            state: vec![],
            events: vec![],
        }))
    }));
    assert_eq!(
        workflows.step(&h.kernel, id, &[]).unwrap(),
        Status::Suspended(vec![1])
    );
    assert_eq!(h.samples.get(), 1);
    assert_eq!(old_scalar.get(), 0);
    assert_eq!(old_field.get(), 0);
    assert_eq!(s.inspect().unwrap(), 5u32.to_le_bytes());
    assert!(h.host.history().unwrap().is_empty());
    let ticket = workflows.ticket(id).unwrap();
    let (extension, binding) = h.replace();
    let new_scalar = Rc::new(Cell::new(0));
    let new_field = Rc::new(Cell::new(0));
    s.admit_compatible_wasm(
        &h.kernel,
        [2; 32],
        Rc::new(ScalarCodec(new_scalar.clone())),
        0,
        binding.clone(),
    )
    .unwrap();
    f.admit_compatible_wasm(
        &h.kernel,
        [3; 32],
        Rc::new(FieldCodec(new_field.clone())),
        0,
        binding,
    )
    .unwrap();
    workflows.resume(&ticket).unwrap();
    assert_eq!(
        workflows.step(&h.kernel, id, &[]).unwrap(),
        Status::Finished(vec![13])
    );
    assert_eq!(branches.get(), 4);
    assert_eq!(h.samples.get(), 1);
    assert_eq!(old_scalar.get(), 2);
    assert_eq!(old_field.get(), 2);
    assert!(preflights.get() > 0);
    assert_eq!(new_scalar.get(), 0);
    assert_eq!(new_field.get(), 0);
    assert_eq!(s.inspect().unwrap(), 13u32.to_le_bytes());
    assert_eq!(
        f.inspect().unwrap(),
        [12u32.to_le_bytes(), 11u32.to_le_bytes()].concat()
    );
    assert_eq!(d.inspect().unwrap(), 4u32.to_le_bytes());
    let history = h.host.history().unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(
        history[0].events,
        vec![b"field".to_vec(), b"scalar".to_vec()]
    );
    assert_eq!(
        history[0]
            .participants
            .iter()
            .find(|p| p.stamp.owner == s.id())
            .unwrap()
            .recorded_inputs
            .len(),
        1
    );
    h.action.borrow_mut().take();
    workflows.dispose(&mut h.kernel, id).unwrap();
    held.borrow_mut().take();
    assert_eq!(h.host.retained_behaviors().unwrap(), 0);
    let op = h.operation();
    let reply: nethacked_inputs::Reply = serde_json::from_slice(&h.run(
        &op,
        3,
        &wire(&nethacked_inputs::Request::Sample {
            request: b"choice".to_vec(),
        }),
        |_, _, _| panic!("input must use real service"),
    ))
    .unwrap();
    let obs = observe(&h, &op, d.clone());
    let target = s.clone();
    let service = h.inputs.clone();
    let host = h.host.clone();
    let checks = preflights.clone();
    h.run(&op, 1, &[], move |_, _, view| {
        let input = service.observation(reply.id, b"choice", view).unwrap();
        assert_eq!(input.bytes(), [4]);
        assert!(service.observation(reply.id, b"forged", view).is_err());
        let query = target
            .query_recorded(
                None,
                "derived",
                &[],
                std::slice::from_ref(&obs),
                std::slice::from_ref(&input),
                view,
            )
            .unwrap();
        assert_eq!(query.bytes(), 44u32.to_le_bytes());
        let candidate = target
            .prepare_recorded(
                None,
                "prepare",
                &[],
                vec![obs.clone()],
                std::slice::from_ref(&input),
                view,
            )
            .unwrap();
        commit(&host, &target, candidate, view, checks.clone());
        Ok(vec![])
    });
    assert_eq!(new_scalar.get(), 2);
    assert_eq!(s.inspect().unwrap(), 44u32.to_le_bytes());
    assert_eq!(h.samples.get(), 2);
    h.kernel.release_pipeline(op.pipeline()).unwrap();
    h.kernel.managed_detach(extension).unwrap();
    drop(cache);
    drop(s);
    drop(f);
    drop(d);
    h.kernel.managed_unload(extension).unwrap();
    assert_eq!(h.kernel.lifecycle_state().references, 0);
}

#[test]
fn accepted_choice_prefix_survives_trap_and_cancel_without_resampling() {
    for trap in [false, true] {
        let mut h = Harness::new();
        let workflows = Workflows::new(nethacked_workflows::Limits::default()).unwrap();
        let program = h.program(if trap { "fault" } else { "workflow" });
        let id = workflows
            .start(
                &mut h.kernel,
                program,
                vec![],
                vec![Rc::new(h.host.clone())],
            )
            .unwrap();
        let branches = Rc::new(Cell::new(0));
        let reached = branches.clone();
        let service = h.inputs.clone();
        *h.action.borrow_mut() = Some(Box::new(move |_, bytes, view| {
            reached.set(reached.get() + 1);
            if serde_json::from_slice::<nethacked_workflows::StepRequest>(bytes).is_ok() {
                return Ok(wire(&nethacked_inputs::Request::Sample {
                    request: b"choice".to_vec(),
                }));
            }
            let reply: nethacked_inputs::Reply = serde_json::from_slice(bytes).unwrap();
            let observation = service.observation(reply.id, b"choice", view).unwrap();
            assert_eq!(observation.bytes(), [3]);
            Ok(wire(&nethacked_workflows::Outcome::Suspend {
                next: "apply".into(),
                prompt: vec![],
                state: vec![],
                events: vec![],
            }))
        }));
        let status = workflows.step(&h.kernel, id, &[]).unwrap();
        if trap {
            assert_eq!(
                status,
                Status::Faulted(nethacked_workflows::Fault::Invocation(
                    nethacked_kernel::Error::Trap
                ))
            );
        } else {
            assert_eq!(status, Status::Suspended(vec![]));
            workflows.abandon(id).unwrap();
        }
        let prefix = h.inputs.inspect().unwrap();
        assert_eq!(prefix.source, [4]);
        assert_eq!(prefix.records.len(), 1);
        assert_eq!(h.samples.get(), 1);
        assert_eq!(branches.get(), 2);
        assert!(workflows.step(&h.kernel, id, &[]).is_err());
        assert_eq!(h.inputs.inspect().unwrap(), prefix);
        assert_eq!(h.samples.get(), 1);
        workflows.dispose(&mut h.kernel, id).unwrap();
        h.action.borrow_mut().take();
        assert_eq!(h.kernel.lifecycle_state().references, 0);
    }
}
