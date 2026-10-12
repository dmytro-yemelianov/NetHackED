use super::*;
use nethacked_workflows::{CleanupParticipant, Fault, Program, Status, Workflows};
fn program(root: ModuleHandle) -> Program {
    Program {
        provider: root,
        endpoint: "workflow".into(),
        authority: Authority {
            access: Access::Write,
            scope: Scope::All,
        },
        initial: "transfer".into(),
        phases: BTreeSet::from(["transfer".into(), "fault".into()]),
        allowed: BTreeSet::from([("transfer".into(), "fault".into())]),
    }
}
struct Marker(Rc<Cell<usize>>);
impl CleanupParticipant for Marker {
    fn discard(&self, _: &OperationBinding) -> nethacked_kernel::Result<()> {
        self.0.set(self.0.get() + 1);
        Ok(())
    }
}
#[test]
fn actual_workflow_phase_trap_cleanup_retry_and_revocation_preserve_prefix() {
    let (mut m, s, d, i) = transfer_host();
    let wf = Workflows::new(nethacked_workflows::Limits::default()).unwrap();
    let cleanup = Rc::new(Cell::new(0));
    let id = wf
        .start(
            &mut m.kernel,
            program(m.root),
            vec![],
            vec![Rc::new(Marker(cleanup.clone())), Rc::new(m.host.clone())],
        )
        .unwrap();
    let staging = Rc::new(RefCell::new(Vec::new()));
    let external = Rc::new(RefCell::new(None));
    let held_gate = Rc::new(RefCell::new(None));
    let trap = Rc::new(Cell::new(false));
    let source = s.clone();
    let dest = d.clone();
    let item = i.clone();
    let host = m.host.clone();
    let proposals = staging.clone();
    let retained = external.clone();
    let held = held_gate.clone();
    let reached = trap.clone();
    let modules = m.modules.clone();
    *m.action.borrow_mut() = Some(Box::new(move |v| {
        let later = v.operation().unwrap().phase() == "fault";
        if v.provider() == modules[0] {
            let c = source
                .prepare("prepare", &[u8::from(later)], vec![], v)
                .unwrap();
            if later {
                *retained.borrow_mut() = Some(c);
            } else {
                proposals.borrow_mut().push(c.erase());
                *retained.borrow_mut() = Some(source.prepare("prepare", &[0], vec![], v).unwrap());
            }
            return Ok(vec![]);
        }
        if v.provider() == modules[1] {
            proposals
                .borrow_mut()
                .push(dest.prepare("prepare", &[1], vec![], v).unwrap().erase());
            return Ok(vec![]);
        }
        if v.provider() == modules[2] {
            proposals
                .borrow_mut()
                .push(item.prepare("prepare", &[1], vec![], v).unwrap().erase());
            return Ok(vec![]);
        }
        assert_eq!(v.provider(), modules[5]);
        if later {
            *held.borrow_mut() = Some(host.close_visibility().unwrap());
            reached.set(true);
            return Err(nethacked_kernel::Error::Trap);
        }
        let batch = host
            .batch(
                std::mem::take(&mut *proposals.borrow_mut()),
                vec![],
                &[source.id(), dest.id(), item.id()],
                vec![item.id(), source.id(), dest.id()],
                [42; 32],
                Rc::new(Transfer {
                    source: source.id(),
                    dest: dest.id(),
                    item: item.id(),
                    reached: Rc::new(Cell::new(0)),
                }),
                named_cap("coordinator", "commit"),
                v,
            )
            .unwrap();
        batch.commit(v).unwrap();
        Ok(br#"{"kind":"advance","next":"fault","state":[],"events":[]}"#.to_vec())
    }));
    assert_eq!(wf.step(&m.kernel, id, &[]).unwrap(), Status::Running);
    assert_eq!(cleanup.get(), 1);
    let prefix = m.host.history().unwrap();
    assert_eq!(prefix.len(), 1);
    assert!(external
        .borrow()
        .as_ref()
        .unwrap()
        .stage
        .prepared
        .borrow()
        .is_none());
    assert_eq!(
        wf.step(&m.kernel, id, &[]).unwrap(),
        Status::Faulted(Fault::Invocation(nethacked_kernel::Error::Trap))
    );
    assert!(trap.get());
    assert_eq!(cleanup.get(), 2);
    assert!(wf.inspect(id).unwrap().cleanup_pending);
    assert_eq!(m.host.pending(), Err(Error::Busy));
    held_gate.borrow_mut().take();
    assert_eq!(m.host.pending().unwrap(), 3);
    assert_eq!(m.host.history().unwrap(), prefix);
    wf.retry_cleanup(id).unwrap();
    wf.retry_cleanup(id).unwrap();
    assert_eq!(cleanup.get(), 3);
    assert!(!wf.inspect(id).unwrap().cleanup_pending);
    assert_eq!(m.host.pending().unwrap(), 0);
    assert!(external
        .borrow()
        .as_ref()
        .unwrap()
        .stage
        .prepared
        .borrow()
        .is_none());
    assert_eq!(m.host.history().unwrap(), prefix);
    assert_eq!(s.inspect().unwrap(), [0, 2]);
    assert_eq!(d.inspect().unwrap(), [1, 2]);
    m.action.borrow_mut().take();
    wf.dispose(&mut m.kernel, id).unwrap();

    let id = wf
        .start(
            &mut m.kernel,
            program(m.root),
            vec![],
            vec![Rc::new(m.host.clone())],
        )
        .unwrap();
    let invoked = Rc::new(Cell::new(0));
    let marker = invoked.clone();
    *m.action.borrow_mut() = Some(Box::new(move |_| {
        marker.set(marker.get() + 1);
        panic!("revoked prepare must never dispatch")
    }));
    m.kernel.revoke_grant(m.root, 1).unwrap();
    assert_eq!(
        wf.step(&m.kernel, id, &[]).unwrap(),
        Status::Faulted(Fault::Invocation(nethacked_kernel::Error::Trap))
    );
    assert_eq!(invoked.get(), 0);
    assert_eq!(m.host.history().unwrap(), prefix);
    assert_eq!(m.host.pending().unwrap(), 0);
    assert!(!wf.inspect(id).unwrap().cleanup_pending);
    wf.dispose(&mut m.kernel, id).unwrap();
}

#[test]
fn workflow_cancel_discards_external_candidates_without_replaying_commands() {
    let (mut m, s, d, i) = transfer_host();
    let wf = Workflows::new(nethacked_workflows::Limits::default()).unwrap();
    let id = wf
        .start(
            &mut m.kernel,
            program(m.root),
            vec![],
            vec![Rc::new(m.host.clone())],
        )
        .unwrap();
    let proposals = Rc::new(RefCell::new(Vec::new()));
    let saved = proposals.clone();
    let modules = m.modules.clone();
    let source = s.clone();
    let dest = d.clone();
    let item = i.clone();
    let calls = Rc::new(Cell::new(0));
    let reached = calls.clone();
    *m.action.borrow_mut() = Some(Box::new(move |v| {
        reached.set(reached.get() + 1);
        if v.provider() == modules[0] {
            saved
                .borrow_mut()
                .push(source.prepare("prepare", &[0], vec![], v).unwrap().erase());
        } else if v.provider() == modules[1] {
            saved
                .borrow_mut()
                .push(dest.prepare("prepare", &[1], vec![], v).unwrap().erase());
        } else if v.provider() == modules[2] {
            saved
                .borrow_mut()
                .push(item.prepare("prepare", &[1], vec![], v).unwrap().erase());
        } else {
            assert_eq!(v.provider(), modules[5]);
            return Ok(br#"{"kind":"reject","events":[]}"#.to_vec());
        }
        Ok(vec![])
    }));
    assert_eq!(wf.step(&m.kernel, id, &[]).unwrap(), Status::Rejected);
    assert_eq!(calls.get(), 4);
    assert_eq!(m.host.pending().unwrap(), 0);
    assert_eq!(m.host.history().unwrap(), vec![]);
    assert_eq!(s.inspect().unwrap(), [1, 2]);
    assert_eq!(d.inspect().unwrap(), [0, 2]);
    assert_eq!(i.inspect().unwrap(), [0, 1]);
    for proposal in proposals.borrow().iter() {
        let holder = proposal.stage.clone();
        let source = s.clone();
        m.run(1, move |v| {
            assert!(matches!(holder.evidence(v), Err(Error::Stale)));
            assert_eq!(source.inspect().unwrap(), [1, 2]);
            Ok(vec![])
        });
    }
    assert_eq!(
        wf.step(&m.kernel, id, &[]),
        Err(nethacked_workflows::Error::Status)
    );
    assert_eq!(calls.get(), 4);
    wf.dispose(&mut m.kernel, id).unwrap();
    m.action.borrow_mut().take();
    m.kernel.release_pipeline(m.lease).unwrap();
    // Held opaque candidates retain no staging code after typed cleanup.
    m.kernel.unregister(m.root).unwrap();
    assert_eq!(proposals.borrow().len(), 3);
}
