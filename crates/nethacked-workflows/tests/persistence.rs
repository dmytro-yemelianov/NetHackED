#![cfg(feature = "persistence")]
#[path = "support/persistence.rs"]
mod support;
use nethacked_kernel::persistence::KernelEnrollment;
use nethacked_kernel::*;
use nethacked_workflows::{persistence::*, Error as WError, *};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use support::*;

#[test]
fn actual_bytes_source_free_old_guest_import_native_and_new_dispatch() {
    let mut f = Fixture::new();
    let old = f.start(
        "one",
        b"opaque checkpoint".to_vec(),
        vec![],
        Rc::new(Opaque),
    );
    assert_eq!(
        f.workflows.step(&f.kernel, old, &[]).unwrap(),
        Status::Running
    );
    assert!(matches!(
        f.workflows.step(&f.kernel, old, &[]).unwrap(),
        Status::Suspended(_)
    ));
    let old_ticket = f.workflows.ticket(old).unwrap();
    f.replacement();
    let (ks, c) = f.save();
    let trusted = Approved(c.clone());
    let catalog = catalog(&c, Rc::new(Opaque));
    let host = Host::fresh(f.host.materials.clone(), ks.operation_contexts().unwrap());
    drop(f);
    let stage = c
        .validate(
            &ks,
            nethacked_workflows::Limits::default(),
            &catalog,
            &trusted,
        )
        .unwrap();
    let mut k = fresh_kernel();
    let mapped = k.restore_kernel(&ks, &host).unwrap();
    let w = stage.rebuild(&k, &mapped, &catalog).unwrap();
    assert_eq!(host.effects.calls.get(), 0);
    assert_eq!(host.factories.get(), ks.registry.modules.len());
    let replay = c
        .validate(
            &ks,
            nethacked_workflows::Limits::default(),
            &catalog,
            &trusted,
        )
        .unwrap();
    assert!(replay.rebuild(&k, &mapped, &catalog).is_err());
    let id = mapped.operations()[0].operation();
    assert!(w.resume(&old_ticket).is_err());
    let fresh_ticket = w.ticket(id).unwrap();
    assert_ne!(fresh_ticket, old_ticket);
    assert_ne!(fresh_ticket.code, old_ticket.code);
    w.resume(&fresh_ticket).unwrap();
    assert!(w.resume(&fresh_ticket).is_err());
    assert_eq!(
        w.step(&k, id, &[]).unwrap(),
        Status::Finished(b"old".to_vec())
    );
    assert_eq!(host.effects.requests.borrow()[0].1.phase, "three");
    assert_eq!(
        w.inspect(id).unwrap().trace,
        [
            b"old:one".to_vec(),
            b"old:two".to_vec(),
            b"old:three".to_vec()
        ]
    );
    let provider = mapped.modules()[&c.frames[0].program.provider];
    let mut p = w.inspect(id).unwrap().program;
    p.provider = provider;
    p.initial = "three".into();
    let new = w.start(&mut k, p, vec![], vec![]).unwrap();
    assert_eq!(
        w.step(&k, new, &[]).unwrap(),
        Status::Finished(b"new".to_vec())
    );
    assert_eq!(host.effects.calls.get(), 2);
    assert!(w.step(&k, id, &[]).is_err());
    w.dispose(&mut k, id).unwrap();
    assert!(w.dispose(&mut k, id).is_err());
}
#[test]
fn running_boundary_and_every_terminal_status_roundtrip_without_calls() {
    for initial in ["one", "three", "reject", "trap", "invalid", "abandoned"] {
        let mut f = Fixture::new();
        let id = f.start(
            if initial == "abandoned" {
                "one"
            } else {
                initial
            },
            vec![1, 2],
            vec![],
            Rc::new(Opaque),
        );
        if initial == "abandoned" {
            f.workflows.abandon(id).unwrap();
        } else if initial != "one" {
            f.workflows.step(&f.kernel, id, &[]).unwrap();
        }
        let before = f.workflows.inspect(id).unwrap();
        let effects = f.host.effects.calls.get();
        let (ks, c) = f.save();
        assert_eq!(f.host.effects.calls.get(), effects);
        let catalog = catalog(&c, Rc::new(Opaque));
        let stage = c
            .validate(
                &ks,
                nethacked_workflows::Limits::default(),
                &catalog,
                &Approved(c.clone()),
            )
            .unwrap();
        let host = Host::fresh(f.host.materials.clone(), ks.operation_contexts().unwrap());
        drop(f);
        let mut k = fresh_kernel();
        let map = k.restore_kernel(&ks, &host).unwrap();
        let w = stage.rebuild(&k, &map, &catalog).unwrap();
        let id = map.operations()[0].operation();
        let after = w.inspect(id).unwrap();
        assert_eq!(after.sequence, before.sequence);
        assert_eq!(after.phase, before.phase);
        assert_eq!(after.state, before.state);
        assert_eq!(after.trace, before.trace);
        assert_eq!(after.status, before.status);
        assert_eq!(host.effects.calls.get(), 0);
        if initial == "one" {
            assert_eq!(w.step(&k, id, &[]).unwrap(), Status::Running);
            assert_eq!(host.effects.requests.borrow()[0].1.phase, "one");
        } else {
            assert!(w.step(&k, id, &[]).is_err());
            w.dispose(&mut k, id).unwrap();
        }
    }
}
#[test]
fn busy_cleanup_ordered_identityless_and_shared_participants_retry_prefix_once() {
    let mut f = Fixture::new();
    let first = participant(None, false);
    let second = participant(Some(CleanupIdentity::issue().unwrap()), false);
    let id = f.start(
        "one",
        vec![],
        vec![first.clone(), second.clone()],
        Rc::new(Opaque),
    );
    f.catalog
        .participant(identity("first"), first.clone())
        .unwrap();
    f.catalog
        .participant(identity("second"), second.clone())
        .unwrap();
    f.workflows.step(&f.kernel, id, &[]).unwrap(); // accepted prefix
    second.fail.set(true);
    assert_eq!(
        f.workflows.step(&f.kernel, id, &[]).unwrap(),
        Status::Faulted(Fault::Cleanup)
    );
    assert_eq!(first.calls.get(), 2);
    assert_eq!(second.calls.get(), 2);
    let other = f.start("three", vec![], vec![second.clone()], Rc::new(Opaque));
    f.workflows.abandon(other).unwrap();
    let (ks, c) = f.save();
    assert_eq!(c.participants.len(), 2);
    assert_eq!(c.participants[0].cleanup, None);
    assert_eq!(c.frames[0].cleaned, 1);
    let fresh = Rc::new(RefCell::new(vec![]));
    let mut catalog = catalog(&c, Rc::new(Opaque));
    for row in &c.participants {
        let fresh = fresh.clone();
        catalog
            .participant(row.build.clone(), move |identity| {
                let p = participant(identity, false);
                fresh.borrow_mut().push(p.clone());
                Ok(p)
            })
            .unwrap();
    }
    let stage = c
        .validate(
            &ks,
            nethacked_workflows::Limits::default(),
            &catalog,
            &Approved(c.clone()),
        )
        .unwrap();
    let host = Host::fresh(f.host.materials.clone(), ks.operation_contexts().unwrap());
    drop(f);
    let mut k = fresh_kernel();
    let map = k.restore_kernel(&ks, &host).unwrap();
    let w = stage.rebuild(&k, &map, &catalog).unwrap();
    let id = map.operations()[0].operation();
    assert_eq!(fresh.borrow().len(), 2);
    assert_eq!(host.effects.calls.get(), 0);
    assert_eq!(w.inspect(id).unwrap().trace, vec![b"old:one".to_vec()]);
    assert_eq!(w.dispose(&mut k, id), Err(WError::Cleanup));
    w.retry_cleanup(id).unwrap();
    w.retry_cleanup(id).unwrap();
    assert_eq!(fresh.borrow()[0].calls.get(), 0);
    assert_eq!(fresh.borrow()[1].calls.get(), 1);
    let b = fresh.borrow()[1].bindings.borrow()[0].clone();
    assert_eq!(b.operation(), id);
    assert_eq!(b.checkpoint(), 1);
    assert_eq!(b.phase(), "two");
    assert!(b.admits_cleanup(fresh.borrow()[1].id.unwrap()));
    assert!(!w.inspect(id).unwrap().cleanup_pending);
    w.dispose(&mut k, id).unwrap();
    assert_eq!(host.effects.calls.get(), 0);
}
#[test]
fn declared_reference_bytes_use_fresh_namespace_and_actual_cleanup_id() {
    let mut f = Fixture::new();
    let (baseline, _) = f.save();
    let module = baseline
        .registry
        .modules
        .iter()
        .find(|m| m.manifest.id == "actor")
        .unwrap()
        .id;
    let p = participant(Some(CleanupIdentity::issue().unwrap()), false);
    let first_state = serde_json::to_vec(&vec![Reference::Module {
        namespace: baseline.source_namespace,
        id: module,
    }])
    .unwrap();
    let first = f.start("one", first_state, vec![p.clone()], Rc::new(RefCodec));
    f.catalog
        .participant(identity("ref-participant"), p.clone())
        .unwrap();
    let (first_kernel, first_save) = f.save();
    let source = &first_save.frames[0].source;
    let lease = first_kernel
        .leases
        .iter()
        .find(|l| l.id == source.pipeline)
        .unwrap();
    let oldrefs = vec![
        Reference::Module {
            namespace: baseline.source_namespace,
            id: module,
        },
        Reference::Cleanup(first_save.participants[0].cleanup.unwrap()),
        Reference::Resource(Resource {
            owner: "workflow".into(),
            key: "state".into(),
        }),
        Reference::Operation {
            namespace: baseline.source_namespace,
            id: first.serial(),
        },
        Reference::Lease {
            namespace: baseline.source_namespace,
            id: source.pipeline,
        },
        Reference::Code {
            lease: source.pipeline,
            hash: lease.pipeline.hash,
        },
    ];
    let state = serde_json::to_vec(&oldrefs).unwrap();
    let old = f.start("one", state, vec![p], Rc::new(RefCodec));
    let (ks, c) = f.save();
    let mut catalog = catalog(&c, Rc::new(RefCodec));
    catalog
        .participant(identity("ref-participant"), |id| Ok(participant(id, false)))
        .unwrap();
    let host = Host::fresh(f.host.materials.clone(), ks.operation_contexts().unwrap());
    let stage = c
        .validate(
            &ks,
            nethacked_workflows::Limits::default(),
            &catalog,
            &Approved(c.clone()),
        )
        .unwrap();
    drop(f);
    let mut k = fresh_kernel();
    let map = k.restore_kernel(&ks, &host).unwrap();
    let w = stage.rebuild(&k, &map, &catalog).unwrap();
    let new = map.operations()[1].operation();
    assert_ne!(new, old);
    let refs: Vec<Reference> = serde_json::from_slice(&w.inspect(new).unwrap().state).unwrap();
    let (namespace, id) = map.execution_module_identity(module).unwrap();
    let (operation_namespace, operation_id) =
        map.execution_operation_identity(first.serial()).unwrap();
    let (lease_namespace, lease_id) = map.execution_lease_identity(source.pipeline).unwrap();
    let cleanup = map
        .execution_cleanup_identity(first_save.participants[0].cleanup.unwrap())
        .unwrap();
    let hash = k
        .pipeline_descriptor(map.operations()[0].pipeline())
        .unwrap()
        .hash;
    assert_eq!(
        refs,
        vec![
            Reference::Module { namespace, id },
            Reference::Cleanup(cleanup),
            Reference::Resource(Resource {
                owner: "fresh-workflow".into(),
                key: "state".into()
            }),
            Reference::Operation {
                namespace: operation_namespace,
                id: operation_id
            },
            Reference::Lease {
                namespace: lease_namespace,
                id: lease_id
            },
            Reference::Code {
                lease: lease_id,
                hash
            }
        ]
    );
    assert_ne!(namespace, ks.source_namespace);
    assert_ne!(hash, lease.pipeline.hash);
    assert_ne!(refs, oldrefs);
    assert_ne!(w.inspect(new).unwrap().state, c.frames[1].state);
    assert_eq!(w.step(&k, new, &[]).unwrap(), Status::Running);
}
#[test]
fn callback_capture_while_active_is_busy_and_incomplete_enrollment_refuses() {
    let mut f = Fixture::new();
    let id = f.start("one", vec![], vec![], Rc::new(Opaque));
    let w = f.workflows.clone();
    let attempts = Rc::new(Cell::new(0));
    let seen = attempts.clone();
    let (ks, _) = f.save();
    let catalog = Rc::new(f.catalog);
    let kernel = Rc::new(f.kernel);
    let callback_kernel = kernel.clone();
    *f.host.effects.callback.borrow_mut() = Some(Box::new(move || {
        assert_eq!(w.inspect(id), Err(WError::Busy));
        assert!(matches!(
            w.persistence_enrollment(&callback_kernel),
            Err(WError::Busy)
        ));
        let index = callback_kernel.execution_capture_index(&ks).unwrap();
        assert!(matches!(
            w.capture_local(&callback_kernel, &index, &catalog),
            Err(WError::Busy)
        ));
        seen.set(seen.get() + 1);
    }));
    assert_eq!(f.workflows.step(&kernel, id, &[]).unwrap(), Status::Running);
    assert_eq!(attempts.get(), 1);
    let mut incomplete = CaptureCatalog::new(&f.workflows);
    incomplete
        .program(id, identity("program"), Rc::new(Opaque))
        .unwrap();
    assert!(incomplete
        .program(id, identity("duplicate"), Rc::new(Opaque))
        .is_err());
}
#[test]
fn forged_local_rows_refuse_before_factories_and_preserve_source() {
    let mut f = Fixture::new();
    let p = participant(None, false);
    let id = f.start("one", vec![], vec![p.clone()], Rc::new(Opaque));
    f.catalog.participant(identity("p"), p).unwrap();
    let (ks, c) = f.save();
    let trusted = Approved(c.clone());
    let calls = Rc::new(Cell::new(0));
    let mut catalog = catalog(&c, Rc::new(Opaque));
    let factory_calls = calls.clone();
    catalog
        .participant(identity("p"), move |id| {
            factory_calls.set(factory_calls.get() + 1);
            Ok(participant(id, false))
        })
        .unwrap();
    let before = f.workflows.inspect(id).unwrap();
    let lifecycle = f.kernel.lifecycle_state();
    let mut bads = vec![];
    macro_rules! bad {
        ($body:expr) => {{
            let mut bad = c.clone();
            ($body)(&mut bad);
            bads.push(bad);
        }};
    }
    bad!(|b: &mut Checkpoint| b.frames[0].source.checkpoint += 1);
    bad!(|b: &mut Checkpoint| b.frames[0].source.phase = "three".into());
    bad!(|b: &mut Checkpoint| b.frames[0].trace.push(b"forged".to_vec()));
    bad!(|b: &mut Checkpoint| b.frames[0].program.allowed.clear());
    bad!(|b: &mut Checkpoint| b.frames[0].program.codec.artifact[0] ^= 1);
    bad!(|b: &mut Checkpoint| b.participants[0].build.artifact[0] ^= 1);
    bad!(|b: &mut Checkpoint| b.frames[0].participants.clear());
    bad!(|b: &mut Checkpoint| b.frames[0].participants.push(1));
    bad!(|b: &mut Checkpoint| b.frames[0].source.pipeline += 1);
    bad!(|b: &mut Checkpoint| b.frames[0].state.resize(1025, 0));
    bad!(|b: &mut Checkpoint| b.frames.push(b.frames[0].clone()));
    bad!(|b: &mut Checkpoint| b.namespace += 1);
    for bad in bads {
        assert!(bad
            .validate(
                &ks,
                nethacked_workflows::Limits::default(),
                &catalog,
                &trusted
            )
            .is_err());
    }
    assert_eq!(calls.get(), 0);
    assert_eq!(f.workflows.inspect(id).unwrap(), before);
    assert_eq!(f.kernel.lifecycle_state(), lifecycle);
    struct Deny;
    impl FrameAuthority for Deny {}
    assert!(c
        .validate(&ks, nethacked_workflows::Limits::default(), &catalog, &Deny)
        .is_err());
    let bytes = c.to_bytes(1 << 20).unwrap();
    assert!(Checkpoint::from_bytes(&bytes, bytes.len() - 1).is_err());
    let mut trailing = bytes.clone();
    trailing.push(b' ');
    assert!(Checkpoint::from_bytes(&trailing, 1 << 20).is_err());
    assert!(Checkpoint::from_bytes(&bytes[..bytes.len() - 1], 1 << 20).is_err());
}
#[test]
fn foreign_maps_changed_grants_and_failed_factory_publish_no_workflow() {
    let mut f = Fixture::new();
    let p = participant(None, false);
    f.start("one", vec![], vec![p.clone()], Rc::new(Opaque));
    f.catalog.participant(identity("p"), p).unwrap();
    let (ks, c) = f.save();
    let mut catalog = catalog(&c, Rc::new(Opaque));
    catalog
        .participant(identity("p"), |_| Err(WError::Cleanup))
        .unwrap();
    let host = Host::fresh(f.host.materials.clone(), ks.operation_contexts().unwrap());
    let mut k = fresh_kernel();
    let mapped = k.restore_kernel(&ks, &host).unwrap();
    let before = k.lifecycle_state();
    let stage = c
        .validate(
            &ks,
            nethacked_workflows::Limits::default(),
            &catalog,
            &Approved(c.clone()),
        )
        .unwrap();
    assert!(stage.rebuild(&k, &mapped, &catalog).is_err());
    assert_eq!(k.lifecycle_state(), before);
    assert_eq!(host.effects.calls.get(), 0);
    let foreign = fresh_kernel();
    let before = foreign.lifecycle_state();
    let stage = c
        .validate(
            &ks,
            nethacked_workflows::Limits::default(),
            &catalog,
            &Approved(c.clone()),
        )
        .unwrap();
    assert!(stage.rebuild(&foreign, &mapped, &catalog).is_err());
    assert_eq!(foreign.lifecycle_state(), before);
    let actor = mapped.modules()[&c.frames[0].program.provider];
    k.revoke_grant(actor, 0).unwrap();
    let stage = c
        .validate(
            &ks,
            nethacked_workflows::Limits::default(),
            &catalog,
            &Approved(c.clone()),
        )
        .unwrap();
    assert!(stage.rebuild(&k, &mapped, &catalog).is_err());
    assert_eq!(host.effects.calls.get(), 0);
    let mut denied = Host::fresh(host.materials.clone(), host.contexts.clone());
    denied.deny = true;
    let mut untouched = fresh_kernel();
    let before = untouched.lifecycle_state();
    assert!(untouched.restore_kernel(&ks, &denied).is_err());
    assert_eq!(untouched.lifecycle_state(), before);
}

#[test]
fn missing_catalogs_and_default_deny_kernel_contexts_call_no_factory() {
    let mut f = Fixture::new();
    let p = participant(None, false);
    let id = f.start("one", vec![], vec![p.clone()], Rc::new(Opaque));
    f.catalog.participant(identity("p"), p).unwrap();
    let (ks, c) = f.save();
    let trusted = Approved(c.clone());
    assert!(c
        .validate(
            &ks,
            nethacked_workflows::Limits::default(),
            &RestoreCatalog::default(),
            &trusted
        )
        .is_err());
    let missing_participant = catalog(&c, Rc::new(Opaque));
    assert!(c
        .validate(
            &ks,
            nethacked_workflows::Limits::default(),
            &missing_participant,
            &trusted
        )
        .is_err());
    let ops = f.workflows.persistence_enrollment(&f.kernel).unwrap();
    let index = f.kernel.execution_capture_index(&ks).unwrap();
    let missing_capture = CaptureCatalog::new(&f.workflows);
    assert!(f
        .workflows
        .capture_local(&f.kernel, &index, &missing_capture)
        .is_err());
    let mut missing_participant = CaptureCatalog::new(&f.workflows);
    missing_participant
        .program(id, identity("p"), Rc::new(Opaque))
        .unwrap();
    assert!(f
        .workflows
        .capture_local(&f.kernel, &index, &missing_participant)
        .is_err());
    assert_eq!(ops, f.workflows.persistence_enrollment(&f.kernel).unwrap());
    let host = Host::fresh(f.host.materials.clone(), vec![]);
    let mut k = fresh_kernel();
    let existing = k
        .register_native(
            manifest("existing", None),
            b"existing",
            host.ports("existing".into()),
        )
        .unwrap();
    let before = k.lifecycle_state();
    assert!(k.restore_kernel(&ks, &host).is_err());
    assert_eq!(host.factories.get(), 0);
    assert_eq!(host.effects.calls.get(), 0);
    assert_eq!(k.lifecycle_state(), before);
    assert_eq!(k.manifest(existing).unwrap().id, "existing");
}
#[test]
fn old_validation_precedes_transform_and_fresh_reference_validation_refuses_stale_bytes() {
    struct Checked {
        transforms: Rc<Cell<usize>>,
        reject: bool,
    }
    impl StateCodec for Checked {
        fn identity(&self) -> Identity {
            identity("refs")
        }
        fn validate(&self, b: &[u8], l: usize) -> nethacked_workflows::Result<Vec<Reference>> {
            if self.reject {
                Err(WError::Status)
            } else {
                RefCodec.validate(b, l)
            }
        }
        fn remap(
            &self,
            b: &[u8],
            _: &LocalRemap<'_>,
            _: usize,
        ) -> nethacked_workflows::Result<Vec<u8>> {
            self.transforms.set(self.transforms.get() + 1);
            Ok(b.to_vec())
        }
    }
    let mut f = Fixture::new();
    let id = f.start("one", vec![], vec![], Rc::new(Opaque));
    let (baseline, _) = f.save();
    let module = baseline.registry.modules[0].id;
    f.workflows.abandon(id).unwrap();
    f.workflows.dispose(&mut f.kernel, id).unwrap();
    f.catalog = CaptureCatalog::new(&f.workflows);
    let bytes = serde_json::to_vec(&vec![Reference::Module {
        namespace: baseline.source_namespace,
        id: module,
    }])
    .unwrap();
    f.start("one", bytes, vec![], Rc::new(RefCodec));
    let (ks, c) = f.save();
    let count = Rc::new(Cell::new(0));
    let invalid = catalog(
        &c,
        Rc::new(Checked {
            transforms: count.clone(),
            reject: true,
        }),
    );
    assert!(c
        .validate(
            &ks,
            nethacked_workflows::Limits::default(),
            &invalid,
            &Approved(c.clone())
        )
        .is_err());
    assert_eq!(count.get(), 0);
    let stale = catalog(
        &c,
        Rc::new(Checked {
            transforms: count.clone(),
            reject: false,
        }),
    );
    let stage = c
        .validate(
            &ks,
            nethacked_workflows::Limits::default(),
            &stale,
            &Approved(c.clone()),
        )
        .unwrap();
    let host = Host::fresh(f.host.materials.clone(), ks.operation_contexts().unwrap());
    let mut k = fresh_kernel();
    let mapped = k.restore_kernel(&ks, &host).unwrap();
    let before = k.lifecycle_state();
    assert!(stage.rebuild(&k, &mapped, &stale).is_err());
    assert_eq!(count.get(), 1);
    assert_eq!(k.lifecycle_state(), before);
    assert_eq!(host.effects.calls.get(), 0);
    // Failed validation did not consume enrollment: corrected approved codec
    // can rebuild the same destination once.
    let correct = catalog(&c, Rc::new(RefCodec));
    c.validate(
        &ks,
        nethacked_workflows::Limits::default(),
        &correct,
        &Approved(c.clone()),
    )
    .unwrap()
    .rebuild(&k, &mapped, &correct)
    .unwrap();
}
#[test]
fn failed_hidden_build_exposes_no_operation_and_does_not_consume_enrollment() {
    let mut f = Fixture::new();
    let a = participant(None, false);
    let b = participant(None, false);
    f.start("one", vec![], vec![a.clone(), b.clone()], Rc::new(Opaque));
    f.catalog.participant(identity("a"), a).unwrap();
    f.catalog.participant(identity("b"), b).unwrap();
    let (ks, c) = f.save();
    let escaped = Rc::new(RefCell::new(None));
    let mut failing = catalog(&c, Rc::new(Opaque));
    let leaked = escaped.clone();
    failing
        .participant(identity("a"), move |id| {
            let p = participant(id, false);
            *leaked.borrow_mut() = Some(p.clone());
            Ok(p)
        })
        .unwrap();
    failing
        .participant(identity("b"), |_| Err(WError::Cleanup))
        .unwrap();
    let host = Host::fresh(f.host.materials.clone(), ks.operation_contexts().unwrap());
    let mut k = fresh_kernel();
    let map = k.restore_kernel(&ks, &host).unwrap();
    let before = k.lifecycle_state();
    let source = f.workflows.inspect(map.operations()[0].operation());
    assert!(source.is_err());
    assert!(c
        .validate(
            &ks,
            nethacked_workflows::Limits::default(),
            &failing,
            &Approved(c.clone())
        )
        .unwrap()
        .rebuild(&k, &map, &failing)
        .is_err());
    assert_eq!(escaped.borrow().as_ref().unwrap().calls.get(), 0);
    assert_eq!(k.lifecycle_state(), before);
    assert_eq!(host.effects.calls.get(), 0);
    let mut corrected = catalog(&c, Rc::new(Opaque));
    corrected
        .participant(identity("a"), |id| Ok(participant(id, false)))
        .unwrap();
    corrected
        .participant(identity("b"), |id| Ok(participant(id, false)))
        .unwrap();
    let w = c
        .validate(
            &ks,
            nethacked_workflows::Limits::default(),
            &corrected,
            &Approved(c.clone()),
        )
        .unwrap()
        .rebuild(&k, &map, &corrected)
        .unwrap();
    assert_eq!(
        w.inspect(map.operations()[0].operation()).unwrap().status,
        Status::Running
    );
    assert_eq!(escaped.borrow().as_ref().unwrap().calls.get(), 0);
}

#[test]
fn exact_old_kernel_graph_cleanup_and_native_build_are_independently_admitted() {
    let mut f = Fixture::new();
    let p = participant(Some(CleanupIdentity::issue().unwrap()), false);
    f.start("one", vec![], vec![p.clone()], Rc::new(Opaque));
    f.catalog.participant(identity("p"), p).unwrap();
    let (ks, _) = f.save();
    let host = Host::fresh(f.host.materials.clone(), ks.operation_contexts().unwrap());
    let mut altered = ks.clone();
    altered.leases[0].pipeline.routing.routes[0].chain[0].endpoint = "forged".into();
    let mut k = fresh_kernel();
    let before = k.lifecycle_state();
    assert!(k.restore_kernel(&altered, &host).is_err());
    assert_eq!(host.factories.get(), 0);
    assert_eq!(k.lifecycle_state(), before);
    let mut altered = ks.clone();
    altered.operations[0].cleanup.clear();
    assert!(k.restore_kernel(&altered, &host).is_err());
    assert_eq!(host.factories.get(), 0);
    // Exact manifest + native artifact is the approved build; an independently
    // missing old factory cannot be replaced by a current native implementation.
    let mut missing = Host::fresh(host.materials.clone(), host.contexts.clone());
    let native = ks
        .registry
        .modules
        .iter()
        .find(|m| m.manifest.id == "codec")
        .unwrap();
    missing.materials.remove(&native.artifact);
    assert!(k.restore_kernel(&ks, &missing).is_err());
    assert_eq!(k.lifecycle_state(), before);
    assert_eq!(missing.effects.calls.get(), 0);
}

fn checkpoint(f: &mut Fixture) -> nethacked_workflows::Result<Checkpoint> {
    let ops = f.workflows.persistence_enrollment(&f.kernel)?;
    let ks = f.kernel.kernel_snapshot(&KernelEnrollment {
        operations: &ops,
        pure_captures: &[],
    })?;
    let index = f.kernel.execution_capture_index(&ks)?;
    f.workflows.capture_local(&f.kernel, &index, &f.catalog)
}

#[test]
fn duplicate_identityless_enrollment_is_a_legal_live_boundary() {
    let mut f = Fixture::new();
    let p = participant(None, false);
    let id = f.start("one", vec![], vec![p.clone(), p.clone()], Rc::new(Opaque));
    f.catalog.participant(identity("p"), p.clone()).unwrap();
    assert_eq!(
        f.workflows.step(&f.kernel, id, &[]).unwrap(),
        Status::Running
    );
    assert_eq!(p.calls.get(), 2);
    let before = f.workflows.inspect(id).unwrap();
    let result = checkpoint(&mut f);
    assert_eq!(f.workflows.inspect(id).unwrap(), before);
    println!("legal duplicate identityless capture result: {result:?}");
    assert!(
        result.is_ok(),
        "safe Running boundary with legal repeated participant must capture"
    );
    repeated_alias_roundtrip(f, p);
}

#[test]
fn duplicate_identified_enrollment_is_a_legal_live_boundary() {
    let mut f = Fixture::new();
    let p = participant(Some(CleanupIdentity::issue().unwrap()), false);
    let id = f.start("one", vec![], vec![p.clone(), p.clone()], Rc::new(Opaque));
    f.catalog.participant(identity("p"), p.clone()).unwrap();
    assert_eq!(
        f.workflows.step(&f.kernel, id, &[]).unwrap(),
        Status::Running
    );
    assert_eq!(p.calls.get(), 2);
    let result = checkpoint(&mut f);
    println!("legal duplicate identified capture result: {result:?}");
    assert!(
        result.is_ok(),
        "kernel deduplicates identities; ordered enrollment must preserve duplicates"
    );
    repeated_alias_roundtrip(f, p);
}

struct Counted(Rc<Cell<usize>>);
impl StateCodec for Counted {
    fn identity(&self) -> Identity {
        Opaque.identity()
    }
    fn validate(&self, b: &[u8], l: usize) -> nethacked_workflows::Result<Vec<Reference>> {
        self.0.set(self.0.get() + 1);
        Opaque.validate(b, l)
    }
    fn remap(
        &self,
        b: &[u8],
        r: &LocalRemap<'_>,
        l: usize,
    ) -> nethacked_workflows::Result<Vec<u8>> {
        Opaque.remap(b, r, l)
    }
}

#[test]
fn forged_later_frame_refuses_before_any_domain_codec() {
    let mut f = Fixture::new();
    f.start("one", vec![], vec![], Rc::new(Opaque));
    f.start("three", vec![], vec![], Rc::new(Opaque));
    let (ks, trusted) = f.save();
    let mut incoming = trusted.clone();
    incoming.frames[1]
        .trace
        .push(b"forged later frame".to_vec());
    let calls = Rc::new(Cell::new(0));
    let cat = catalog(&trusted, Rc::new(Counted(calls.clone())));
    let result = incoming.validate(
        &ks,
        nethacked_workflows::Limits::default(),
        &cat,
        &Approved(trusted),
    );
    assert!(result.is_err());
    println!(
        "forged later frame refused after {} old codec call(s)",
        calls.get()
    );
    assert_eq!(
        calls.get(),
        0,
        "complete independent graph approval must precede semantic codecs"
    );
}

#[test]
fn running_after_resume_capture_is_observational_and_continues_next_step() {
    let mut f = Fixture::new();
    let p = participant(Some(CleanupIdentity::issue().unwrap()), false);
    let id = f.start("one", b"state".to_vec(), vec![p.clone()], Rc::new(Opaque));
    f.catalog.participant(identity("p"), p.clone()).unwrap();
    f.workflows.step(&f.kernel, id, &[]).unwrap();
    f.workflows.step(&f.kernel, id, &[]).unwrap();
    let t = f.workflows.ticket(id).unwrap();
    f.workflows.resume(&t).unwrap();
    let before_frame = f.workflows.inspect(id).unwrap();
    let before = serde_json::to_vec(
        &f.kernel
            .kernel_snapshot(&KernelEnrollment::default())
            .unwrap(),
    )
    .unwrap();
    let (ks, c) = f.save();
    let after = serde_json::to_vec(
        &f.kernel
            .kernel_snapshot(&KernelEnrollment::default())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(before, after);
    assert_eq!(before_frame, f.workflows.inspect(id).unwrap());
    assert_eq!(c.frames[0].source.checkpoint, 3);
    assert_eq!(c.frames[0].source.phase, "three");
    let mut cat = catalog(&c, Rc::new(Opaque));
    cat.participant(identity("p"), |id| Ok(participant(id, false)))
        .unwrap();
    let stage = c
        .validate(
            &ks,
            nethacked_workflows::Limits::default(),
            &cat,
            &Approved(c.clone()),
        )
        .unwrap();
    let host = Host::fresh(f.host.materials.clone(), ks.operation_contexts().unwrap());
    drop(f);
    let mut k = fresh_kernel();
    let mapped = k.restore_kernel(&ks, &host).unwrap();
    let w = stage.rebuild(&k, &mapped, &cat).unwrap();
    assert_eq!(host.effects.calls.get(), 0);
    let id = mapped.operations()[0].operation();
    assert!(w.resume(&t).is_err());
    assert_eq!(
        w.step(&k, id, &[]).unwrap(),
        Status::Finished(b"old".to_vec())
    );
    assert_eq!(host.effects.requests.borrow()[0].1.sequence, 3);
    assert_eq!(host.effects.requests.borrow()[0].1.phase, "three");
    assert!(w.step(&k, id, &[]).is_err());
}

#[test]
fn cleanup_cursor_and_crossrow_substitution_refuse_before_codecs() {
    let mut f = Fixture::new();
    let a = participant(None, false);
    let b = participant(None, true);
    f.start("one", vec![], vec![a.clone(), b.clone()], Rc::new(Opaque));
    f.start("three", vec![], vec![], Rc::new(Opaque));
    f.catalog.participant(identity("a"), a).unwrap();
    f.catalog.participant(identity("b"), b).unwrap();
    let id = f.workflows.persistence_enrollment(&f.kernel).unwrap()[0].operation();
    assert_eq!(
        f.workflows.step(&f.kernel, id, &[]).unwrap(),
        Status::Faulted(Fault::Cleanup)
    );
    let (ks, c) = f.save();
    let calls = Rc::new(Cell::new(0));
    let mut cat = catalog(&c, Rc::new(Counted(calls.clone())));
    for p in &c.participants {
        cat.participant(p.build.clone(), |id| Ok(participant(id, false)))
            .unwrap();
    }
    let mut bad = c.clone();
    bad.frames[0].cleaned = 0;
    assert!(bad
        .validate(
            &ks,
            nethacked_workflows::Limits::default(),
            &cat,
            &Approved(c.clone())
        )
        .is_err());
    assert_eq!(calls.get(), 0);
    let mut bad = c.clone();
    bad.frames[0].source = c.frames[1].source.clone();
    assert!(bad
        .validate(
            &ks,
            nethacked_workflows::Limits::default(),
            &cat,
            &Approved(c.clone())
        )
        .is_err());
    assert_eq!(calls.get(), 0);
    let mut bad = c.clone();
    bad.frames[0].cleaned = 2;
    assert!(bad
        .validate(
            &ks,
            nethacked_workflows::Limits::default(),
            &cat,
            &Approved(c.clone())
        )
        .is_err());
    assert_eq!(calls.get(), 0);
}

#[test]
fn dropped_enrollment_rolls_back_finished_enrollment_stays_consumed() {
    let mut f = Fixture::new();
    f.start("one", vec![], vec![], Rc::new(Opaque));
    let (ks, c) = f.save();
    let host = Host::fresh(f.host.materials.clone(), ks.operation_contexts().unwrap());
    let mut k = fresh_kernel();
    let mapped = k.restore_kernel(&ks, &host).unwrap();
    let sources: Vec<_> = c.frames.iter().map(|f| f.source.clone()).collect();
    let guard = mapped.enroll_workflows(&sources).unwrap();
    assert!(mapped.enroll_workflows(&sources).is_err());
    drop(guard);
    mapped.enroll_workflows(&sources).unwrap().finish();
    assert!(mapped.enroll_workflows(&sources).is_err());
}

fn repeated_alias_roundtrip(mut f: Fixture, source: Rc<Participant>) {
    let (ks, c) = f.save();
    assert_eq!(c.participants.len(), 1);
    assert_eq!(c.frames[0].participants, vec![c.participants[0].id; 2]);
    assert_eq!(
        c.frames[0].source.cleanup.len(),
        usize::from(source.id.is_some())
    );
    let mut cat = catalog(&c, Rc::new(Opaque));
    let built = Rc::new(RefCell::new(Vec::new()));
    let allocations = built.clone();
    cat.participant(identity("p"), move |id| {
        let p = participant(id, false);
        allocations.borrow_mut().push(p.clone());
        Ok(p)
    })
    .unwrap();
    let mut wrong = c.clone();
    wrong.frames[0].participants.pop();
    assert!(wrong
        .validate(
            &ks,
            nethacked_workflows::Limits::default(),
            &cat,
            &Approved(c.clone())
        )
        .is_err());
    let stage = c
        .validate(
            &ks,
            nethacked_workflows::Limits::default(),
            &cat,
            &Approved(c.clone()),
        )
        .unwrap();
    let host = Host::fresh(f.host.materials.clone(), ks.operation_contexts().unwrap());
    let old_identity = source.id;
    drop(f);
    drop(source);
    let mut k = fresh_kernel();
    let mapped = k.restore_kernel(&ks, &host).unwrap();
    let w = stage.rebuild(&k, &mapped, &cat).unwrap();
    assert_eq!(built.borrow().len(), 1);
    let restored = built.borrow()[0].clone();
    assert_eq!(restored.id.is_some(), old_identity.is_some());
    if old_identity.is_some() {
        assert_ne!(restored.id, old_identity);
    }
    assert_eq!(restored.calls.get(), 0);
    let id = mapped.operations()[0].operation();
    assert!(matches!(w.step(&k, id, &[]).unwrap(), Status::Suspended(_)));
    let ticket = w.ticket(id).unwrap();
    w.resume(&ticket).unwrap();
    assert!(matches!(w.step(&k, id, &[]).unwrap(), Status::Finished(_)));
    assert_eq!(restored.calls.get(), 4);
    assert_eq!(restored.bindings.borrow()[0], restored.bindings.borrow()[1]);
    assert_eq!(restored.bindings.borrow()[2], restored.bindings.borrow()[3]);
}

#[test]
fn independently_approved_two_frames_validate_and_continue() {
    let mut f = Fixture::new();
    f.start("one", vec![], vec![], Rc::new(Opaque));
    f.start("three", vec![], vec![], Rc::new(Opaque));
    let (ks, c) = f.save();
    let calls = Rc::new(Cell::new(0));
    let cat = catalog(&c, Rc::new(Counted(calls.clone())));
    let stage = c
        .validate(
            &ks,
            nethacked_workflows::Limits::default(),
            &cat,
            &Approved(c.clone()),
        )
        .unwrap();
    assert_eq!(calls.get(), 2);
    let host = Host::fresh(f.host.materials.clone(), ks.operation_contexts().unwrap());
    drop(f);
    let mut k = fresh_kernel();
    let mapped = k.restore_kernel(&ks, &host).unwrap();
    let w = stage.rebuild(&k, &mapped, &cat).unwrap();
    assert_eq!(host.effects.calls.get(), 0);
    assert_eq!(
        w.step(&k, mapped.operations()[0].operation(), &[]).unwrap(),
        Status::Running
    );
    assert!(matches!(
        w.step(&k, mapped.operations()[1].operation(), &[]).unwrap(),
        Status::Finished(_)
    ));
}

#[test]
fn later_rebuild_catalog_refusal_precedes_remapping_and_rolls_back_enrollment() {
    struct DomainCalls(Rc<Cell<usize>>);
    impl StateCodec for DomainCalls {
        fn identity(&self) -> Identity {
            Opaque.identity()
        }
        fn validate(&self, b: &[u8], l: usize) -> nethacked_workflows::Result<Vec<Reference>> {
            self.0.set(self.0.get() + 1);
            Opaque.validate(b, l)
        }
        fn remap(
            &self,
            b: &[u8],
            r: &LocalRemap<'_>,
            l: usize,
        ) -> nethacked_workflows::Result<Vec<u8>> {
            self.0.set(self.0.get() + 1);
            Opaque.remap(b, r, l)
        }
    }
    let mut f = Fixture::new();
    f.start("one", vec![], vec![], Rc::new(Opaque));
    f.start("three", vec![], vec![], Rc::new(Opaque));
    let (ks, c) = f.save();
    let calls = Rc::new(Cell::new(0));
    let complete = catalog(&c, Rc::new(DomainCalls(calls.clone())));
    let stage = c
        .validate(
            &ks,
            nethacked_workflows::Limits::default(),
            &complete,
            &Approved(c.clone()),
        )
        .unwrap();
    assert_eq!(calls.get(), 2);
    let host = Host::fresh(f.host.materials.clone(), ks.operation_contexts().unwrap());
    drop(f);
    let mut k = fresh_kernel();
    let mapped = k.restore_kernel(&ks, &host).unwrap();
    let mut missing = RestoreCatalog::default();
    missing
        .program(
            c.frames[0].program.clone(),
            Rc::new(DomainCalls(calls.clone())),
        )
        .unwrap();
    calls.set(0);
    assert!(stage.rebuild(&k, &mapped, &missing).is_err());
    assert_eq!(calls.get(), 0);
    assert_eq!(host.effects.calls.get(), 0);
    let sources: Vec<_> = c.frames.iter().map(|f| f.source.clone()).collect();
    let guard = mapped.enroll_workflows(&sources).unwrap();
    drop(guard);
    let stage = c
        .validate(
            &ks,
            nethacked_workflows::Limits::default(),
            &complete,
            &Approved(c.clone()),
        )
        .unwrap();
    assert!(stage.rebuild(&k, &mapped, &complete).is_ok());
}

#[test]
fn shared_live_catalog_enrollment_captures_actual_frames_and_refuses_duplicates() {
    let mut f = Fixture::new();
    let cleanup = participant(None, false);
    let program = f.program("one");
    let id = f
        .workflows
        .start(
            &mut f.kernel,
            program,
            b"live".to_vec(),
            vec![cleanup.clone()],
        )
        .unwrap();
    // The session exposes this catalog through an immutable service handle.
    let catalog = &f.catalog;
    catalog
        .enroll_program(id, identity("live-program"), Rc::new(Opaque))
        .unwrap();
    catalog
        .enroll_participant(identity("live-cleanup"), cleanup.clone())
        .unwrap();
    let (_, before) = f.save();
    assert_eq!(before.frames.len(), 1);
    assert_eq!(before.participants.len(), 1);
    assert_eq!(before.frames[0].state, b"live");
    assert_eq!(
        f.catalog
            .enroll_program(id, identity("duplicate"), Rc::new(Opaque)),
        Err(WError::Status)
    );
    assert_eq!(
        f.catalog.enroll_participant(identity("duplicate"), cleanup),
        Err(WError::Bounds)
    );
    let (_, after) = f.save();
    assert_eq!(after, before);
    let other = Workflows::new(nethacked_workflows::Limits::default()).unwrap();
    let alien = CaptureCatalog::new(&other);
    assert_eq!(
        alien.enroll_program(id, identity("alien"), Rc::new(Opaque)),
        Err(WError::Unknown)
    );
}
