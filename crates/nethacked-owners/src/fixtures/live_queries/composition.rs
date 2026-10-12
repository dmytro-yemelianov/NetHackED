use super::*;

#[test]
fn cache_and_retained_queries_refuse_at_each_swap_of_a_heterogeneous_batch() {
    let (h, item, owner, calls) = setup(ADD_OBSERVATION);
    let behavior = retain(&h, owner.clone());
    let cache = owner.query_cache().unwrap();
    let observation = observe(&h, item.clone());
    let swaps = Rc::new(Cell::new(0));
    let seen = swaps.clone();
    let target = owner.clone();
    let input_owner = item.clone();
    let host = h.host.clone();
    h.run(1, move |view| {
        let query = target
            .query_cached(
                &cache,
                "derived",
                &[1],
                std::slice::from_ref(&observation),
                view,
            )
            .unwrap();
        assert!(!query.hit);
        let blob = target
            .prepare_retained(&behavior, "prepare", &[1], vec![observation.clone()], view)
            .unwrap();
        let inventory = input_owner.prepare("prepare", &[1], vec![], view).unwrap();
        let writes = [target.id(), input_owner.id()];
        let batch = host
            .batch(
                vec![blob.erase(), inventory.erase()],
                vec![],
                &writes,
                writes.to_vec(),
                [7; 32],
                Rc::new(Accept),
                target.capability("prepare").unwrap(),
                view,
            )
            .unwrap();
        let detached = view.clone();
        let read = target.clone();
        let cached = cache.clone();
        let dependencies = observation.clone();
        let retained = behavior.clone();
        let marks = seen.clone();
        crate::publication::SWAP_PROBE.with(|p| {
            *p.borrow_mut() = Some(Box::new(move || {
                marks.set(marks.get() + 1);
                assert!(matches!(
                    read.query_cached(
                        &cached,
                        "derived",
                        &[1],
                        std::slice::from_ref(&dependencies),
                        &detached
                    ),
                    Err(Error::Busy)
                ));
                assert!(matches!(
                    read.query_retained(
                        &retained,
                        "derived",
                        &[1],
                        std::slice::from_ref(&dependencies),
                        &detached
                    ),
                    Err(Error::Busy)
                ));
                assert_eq!(read.inspect(), Err(Error::Busy));
            }))
        });
        batch.commit(view).unwrap();
        crate::publication::SWAP_PROBE.with(|p| p.borrow_mut().take());
        Ok(vec![])
    });
    assert_eq!(swaps.get(), 2);
    assert_eq!(owner.inspect().unwrap(), 9u32.to_le_bytes());
    assert_eq!(
        item.inspect().unwrap(),
        [5u64.to_le_bytes(), 20u64.to_le_bytes()].concat()
    );
    assert_eq!(calls.get(), 2);
    assert_eq!(crate::probes::counts(), [0; 5]);
}

#[test]
fn behavior_bound_and_revision_exhaustion_preserve_exact_state_history_and_ids() {
    let mut h = Harness::live(
        Limits {
            candidates: 1,
            ..Limits::default()
        },
        KernelLimits::default(),
        ADD_OBSERVATION,
    );
    let item = inventory(&mut h);
    let count = Rc::new(Cell::new(0));
    let codec = Rc::new(CountCodec(count.clone()));
    let owner = h
        .host
        .enroll(
            &h.kernel,
            h.spec(3, vec![dep(&item)]),
            5u32.to_le_bytes().to_vec(),
            Backend::Wasm {
                codec: codec.clone(),
                slot: 0,
                binding: h.binding(),
            },
        )
        .unwrap();
    owner
        .admit_compatible_wasm(&h.kernel, [3; 32], codec, 0, h.binding())
        .unwrap();
    h.admit(&owner);
    h.refresh();
    let behavior = retain(&h, owner.clone());
    let before = owner.inspect().unwrap();
    let ids = (
        h.host.0.registry.borrow().next_candidate,
        h.host.0.registry.borrow().next_owner,
    );
    let target = owner.clone();
    h.run(1, move |view| {
        assert!(matches!(
            target.retain_behavior("prepare", view),
            Err(Error::Limit)
        ));
        Ok(vec![])
    });
    assert_eq!(h.host.retained_behaviors().unwrap(), 1);
    drop(behavior);
    assert_eq!(h.host.retained_behaviors().unwrap(), 0);
    let behavior = retain(&h, owner.clone());
    owner.metadata.behavior_revision.set(u64::MAX);
    let binding = h.binding();
    assert_eq!(
        owner.admit_compatible_wasm(
            &h.kernel,
            [3; 32],
            Rc::new(CountCodec(count.clone())),
            0,
            binding
        ),
        Err(Error::Limit)
    );
    assert_eq!(owner.metadata.behavior_revision.get(), u64::MAX);
    assert_eq!(owner.inspect().unwrap(), before);
    assert!(h.host.history().unwrap().is_empty());
    assert_eq!(count.get(), 0);
    assert_eq!(
        (
            h.host.0.registry.borrow().next_candidate,
            h.host.0.registry.borrow().next_owner
        ),
        ids
    );
    drop(behavior);
}

fn retain(h: &Harness, owner: Rc<Owner<Vec<u8>>>) -> RetainedBehavior<Vec<u8>> {
    let saved = Rc::new(RefCell::new(None));
    let sink = saved.clone();
    h.run(1, move |v| {
        *sink.borrow_mut() = Some(owner.retain_behavior("prepare", v).unwrap());
        Ok(vec![])
    });
    let behavior = saved.borrow_mut().take().unwrap();
    behavior
}

#[test]
fn retained_first_query_prepare_and_publish_use_original_codec_and_guest() {
    let (mut h, item, owner, old_calls) = setup(ADD_OBSERVATION);
    let original_codec = match &*owner.backend.borrow() {
        Backend::Wasm { codec, .. } => Rc::downgrade(codec),
        _ => unreachable!(),
    };
    let retained = retain(&h, owner.clone());
    assert_eq!(h.host.retained_behaviors().unwrap(), 1);
    assert_eq!(old_calls.get(), 0);
    #[cfg(feature = "recorded-inputs")]
    assert_eq!(
        owner.declare_recorded_inputs("derived", vec![]),
        Err(Error::Referenced)
    );
    let extension = h.extension(MULTIPLY);
    let new_calls = Rc::new(Cell::new(0));
    owner
        .admit_compatible_wasm(
            &h.kernel,
            [3; 32],
            Rc::new(CountCodec(new_calls.clone())),
            0,
            h.future_binding(extension),
        )
        .unwrap();
    let input = observe(&h, item.clone());
    let target = owner.clone();
    let behavior = retained.clone();
    h.run(1, move |view| {
        let query = target
            .query_retained(
                &behavior,
                "derived",
                &[1],
                std::slice::from_ref(&input),
                view,
            )
            .unwrap();
        assert_eq!(query.bytes(), 9u32.to_le_bytes());
        let candidate = target
            .prepare_retained(&behavior, "prepare", &[1], vec![input.clone()], view)
            .unwrap();
        candidate.validate(view).unwrap();
        let id = target.id();
        let receipt = target
            .host
            .batch(
                vec![candidate.erase()],
                vec![query],
                &[id],
                vec![id],
                [7; 32],
                Rc::new(Accept),
                target.capability("prepare").unwrap(),
                view,
            )
            .unwrap()
            .commit(view)
            .unwrap();
        assert_eq!(receipt.epoch, 1);
        Ok(vec![])
    });
    assert_eq!(old_calls.get(), 2);
    assert_eq!(new_calls.get(), 0);
    assert_eq!(owner.inspect().unwrap(), 9u32.to_le_bytes());
    let input = observe(&h, item);
    let target = owner.clone();
    h.current(2, move |view| {
        assert_eq!(
            target
                .query("derived", &[1], &[input.clone()], view)
                .unwrap()
                .bytes(),
            36u32.to_le_bytes()
        );
        Ok(vec![])
    });
    assert_eq!(new_calls.get(), 1);
    assert!(original_codec.upgrade().is_some());
    drop(retained);
    assert_eq!(h.host.retained_behaviors().unwrap(), 0);
    assert!(original_codec.upgrade().is_none());
    assert_eq!(h.host.history().unwrap().len(), 1);
}

#[test]
fn retained_behavior_refuses_foreign_owner_operation_and_detached_selection() {
    let (mut h, item, owner, calls) = setup(ADD_OBSERVATION);
    let retained = retain(&h, owner.clone());
    let other = h
        .host
        .enroll(
            &h.kernel,
            h.spec(3, vec![dep(&item)]),
            5u32.to_le_bytes().to_vec(),
            Backend::Wasm {
                codec: Rc::new(CountCodec(calls.clone())),
                slot: 0,
                binding: h.binding(),
            },
        )
        .unwrap();
    h.admit(&other);
    // A new operation has a freshly captured graph but no right to select old behavior.
    h.refresh();
    let input = observe(&h, item);
    let target = owner.clone();
    let behavior = retained.clone();
    h.run(1, move |view| {
        assert!(matches!(
            target.prepare_retained(&behavior, "prepare", &[1], vec![input.clone()], view),
            Err(Error::Authority)
        ));
        assert!(matches!(
            other.prepare_retained(&behavior, "prepare", &[1], vec![input.clone()], view),
            Err(Error::Stale)
        ));
        assert!(target
            .prepare_retained(
                &behavior,
                "prepare",
                &[1],
                vec![input.clone()],
                &view.clone()
            )
            .is_err());
        Ok(vec![])
    });
    assert_eq!(calls.get(), 0);
    assert_eq!(owner.inspect().unwrap(), 5u32.to_le_bytes());
    assert!(h.host.history().unwrap().is_empty());
    drop(retained);
    assert_eq!(h.host.retained_behaviors().unwrap(), 0);
}

#[test]
fn baseline_first_preparation_after_future_admission_refuses_without_mutation() {
    let (mut h, item, owner, count) = setup(ADD_OBSERVATION);
    let observation = observe(&h, item);
    let extension = h.extension(MULTIPLY);
    owner
        .admit_compatible_wasm(
            &h.kernel,
            [3; 32],
            Rc::new(CountCodec(count.clone())),
            0,
            h.future_binding(extension),
        )
        .unwrap();
    let target = owner.clone();
    let query_observation = observation.clone();
    h.run(2, move |view| {
        assert!(matches!(
            target.query(
                "derived",
                &[1],
                std::slice::from_ref(&query_observation),
                view
            ),
            Err(Error::Stale)
        ));
        Ok(vec![])
    });
    let target = owner.clone();
    h.run(1, move |view| {
        assert!(matches!(
            target.prepare("prepare", &[1], vec![observation.clone()], view),
            Err(Error::Stale)
        ));
        Ok(vec![])
    });
    assert_eq!(owner.inspect().unwrap(), 5u32.to_le_bytes());
    assert!(h.host.history().unwrap().is_empty());
    assert_eq!(count.get(), 0);
}
