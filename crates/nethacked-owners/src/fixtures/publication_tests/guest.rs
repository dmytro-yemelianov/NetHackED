use super::*;
#[test]
fn actual_wasm_proposals_commit_heterogeneous_state_and_retain_old_dispatch() {
    let mut m = Multi::new();
    let inv = m
        .host
        .enroll(
            &m.kernel,
            m.spec(0, 1),
            Inventory {
                amount: 4,
                capacity: 24,
            },
            Backend::Native(Rc::new(InventoryDomain)),
        )
        .unwrap();
    let mut spec = m.spec(4, 4);
    spec.ports.get_mut("prepare").unwrap().dependencies = vec![dep(&inv)];
    let binding = CapabilityPermit {
        capability: named_cap("algorithm", "compute"),
        resolution: vec![ProviderBinding {
            handle: m.helper,
            artifact: m.kernel.artifact(m.helper).unwrap(),
            endpoint: "compute".into(),
        }],
    };
    let blob = m
        .host
        .enroll(
            &m.kernel,
            spec,
            5u32.to_le_bytes().to_vec(),
            Backend::Wasm {
                codec: Rc::new(BlobCodec),
                slot: 0,
                binding,
            },
        )
        .unwrap();
    let holder = Rc::new(RefCell::new(None));
    let out = holder.clone();
    let source = inv.clone();
    m.run(0, move |v| {
        *out.borrow_mut() = Some(source.query("query", &[], &[], v).unwrap());
        Ok(vec![])
    });
    let obs = holder.borrow_mut().take().unwrap();
    let holder = Rc::new(RefCell::new(None));
    let out = holder.clone();
    let device = blob.clone();
    m.run(9, move |v| {
        let c = device
            .prepare("prepare", &[2], vec![obs.clone()], v)
            .unwrap();
        assert_eq!(
            c.stage.prepared.borrow().as_ref().unwrap().next,
            18u32.to_le_bytes()
        );
        *out.borrow_mut() = Some(c.erase());
        Ok(vec![])
    });
    let batch = m.batch(
        vec![
            m.prepare(0, inv.clone(), 2).erase(),
            holder.borrow_mut().take().unwrap(),
        ],
        vec![inv.id(), blob.id()],
        vec![blob.id(), inv.id()],
        Rc::new(Allow),
        vec![],
    );
    let replacement = m
        .kernel
        .register_guest(
            Manifest {
                id: "replacement".into(),
                exports: BTreeMap::from([("compute".into(), endpoint(Access::Read))]),
                dependencies: vec![],
                extension_points: BTreeSet::new(),
                extension: Some(nethacked_kernel::Extension {
                    target: named_cap("algorithm", "compute"),
                    export: "compute".into(),
                }),
            },
            &helper_wasm("i32.const 0 i32.const 99 i32.store i64.const 4"),
        )
        .unwrap();
    let marker = Rc::new(Cell::new(false));
    let seen = marker.clone();
    *m.action.borrow_mut() = Some(Box::new(move |v| {
        let output = v.call_pure(0, &[0; 9])?;
        assert_eq!(output.bytes, 99u32.to_le_bytes());
        assert_eq!(
            output.binding.resolution.last().unwrap().handle,
            replacement
        );
        seen.set(true);
        Ok(output.bytes)
    }));
    m.kernel
        .invoke(
            m.root,
            "run",
            &[9],
            Authority {
                access: Access::Write,
                scope: Scope::All,
            },
        )
        .unwrap();
    m.action.borrow_mut().take();
    assert!(marker.get());
    assert_eq!(
        m.kernel.unregister(m.helper),
        Err(nethacked_kernel::Error::Referenced)
    );
    let b = batch.clone();
    m.run(10, move |v| {
        let r = b.commit(v).unwrap();
        assert_eq!(r.events, vec![b"blob-increment".to_vec(), vec![b'i', 2]]);
        assert_eq!(r.participants[0].behavior.len(), 1);
        assert_eq!(r.observations.len(), 1);
        assert_eq!(r.observations[0].stamp.revision, 0);
        assert_eq!(crate::probes::counts(), [0; 5]);
        Ok(r.reply)
    });
    assert_eq!(blob.inspect().unwrap(), 18u32.to_le_bytes());
    assert_eq!(
        inv.inspect().unwrap(),
        [6u64.to_le_bytes(), 24u64.to_le_bytes()].concat()
    );
    assert_eq!(m.host.history().unwrap().len(), 1);
}

#[test]
fn committed_wasm_call_then_reply_error_or_trap_keeps_exactly_one_prefix() {
    for fault in ["reply", "trap"] {
        let (m, s, d, i) = transfer_host();
        let batch = transfer_batch(&m, &s, &d, &i, Rc::new(Cell::new(0)));
        let reached = Rc::new(Cell::new(false));
        let marker = reached.clone();
        let b = batch.clone();
        let result = m.run_result(10, move |v| {
            let r = b.commit(v).unwrap();
            assert_eq!(r.epoch, 1);
            marker.set(true);
            if fault == "reply" {
                Ok(vec![0; 1025])
            } else {
                Err(nethacked_kernel::Error::Trap)
            }
        });
        assert_eq!(result, Err(nethacked_kernel::Error::Trap));
        assert!(reached.get());
        assert_eq!(m.host.history().unwrap().len(), 1);
        assert_eq!(m.host.pending().unwrap(), 0);
        assert_eq!(s.inspect().unwrap(), [0, 2]);
        assert_eq!(d.inspect().unwrap(), [1, 2]);
        assert_eq!(i.inspect().unwrap(), [1, 1]);
        let b = batch.clone();
        m.run(10, move |v| {
            assert_eq!(b.commit(v), Err(Error::Stale));
            Ok(vec![])
        });
    }
}
