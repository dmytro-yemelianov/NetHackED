use super::*;
use crate::support::{authority, cap, forward, manifest, replay, resource, sample};
use nethacked_kernel::{persistence::*, Authority, ProviderBinding};
use std::cell::Cell;

const LIMIT: usize = 1_048_576;
struct Approved {
    native_manifest: Manifest,
    native_identity: Vec<u8>,
    port: NativePort,
    root_manifest: Manifest,
    root_bytes: Vec<u8>,
    contexts: Vec<SavedOperationContext>,
    denied: bool,
}
impl RegistryAuthority for Approved {
    fn resolve(&self, m: &SavedModule) -> Result<ArtifactMaterial> {
        match m.manifest.id.as_str() {
            "inputs" if m.manifest == self.native_manifest => Ok(ArtifactMaterial::Native {
                manifest: self.native_manifest.clone(),
                identity: self.native_identity.clone(),
                ports: BTreeMap::from([(ENDPOINT.into(), self.port.clone())]),
            }),
            "root" if m.manifest == self.root_manifest => Ok(ArtifactMaterial::Guest {
                manifest: self.root_manifest.clone(),
                bytes: self.root_bytes.clone(),
            }),
            _ => Err(Error::Authority),
        }
    }
    fn bind_resource(&self, r: &Resource) -> Result<Resource> {
        if *r == resource() {
            Ok(r.clone())
        } else {
            Err(Error::Authority)
        }
    }
    fn approve_grant(
        &self,
        _: &SavedModule,
        _: usize,
        _: &SavedModule,
        a: &Authority,
    ) -> Result<Authority> {
        if !self.denied && *a == authority() {
            Ok(a.clone())
        } else {
            Err(Error::Authority)
        }
    }
}
impl RetainedAuthority for Approved {
    fn approve_cleanup(&self, _: u64) -> Result<()> {
        Err(Error::Authority)
    }
    fn approve_operation_context(&self, c: &SavedOperationContext) -> Result<()> {
        if !self.denied && self.contexts.contains(c) {
            Ok(())
        } else {
            Err(Error::Authority)
        }
    }
}
struct Fixture {
    kernel: Kernel,
    service: Service,
    sampled: Rc<Cell<usize>>,
    observations: Rc<RefCell<Vec<Observation>>>,
    native_manifest: Manifest,
    native_identity: Vec<u8>,
    root_manifest: Manifest,
    root_bytes: Vec<u8>,
}
fn fixture() -> Fixture {
    let sampled = Rc::new(Cell::new(0));
    let calls = sampled.clone();
    let service = Service::new(
        Limits::default(),
        Policy {
            identity: "queue".into(),
            version: 1,
            transition: Rc::new(move |s, r| {
                calls.set(calls.get() + 1);
                assert_eq!(r, b"choice");
                Ok((s[1..].to_vec(), vec![s[0]]))
            }),
        },
        vec![10, 20, 30],
    )
    .unwrap();
    let observations = Rc::new(RefCell::new(vec![]));
    let saved = observations.clone();
    let s = service.clone();
    let port: NativePort = Rc::new(move |bytes, view| {
        let reply = s.dispatch(bytes, view)?;
        let Reply { id, .. } = serde_json::from_slice(&reply).unwrap();
        saved.borrow_mut().push(s.observation(id, b"choice", view)?);
        Ok(reply)
    });
    let mut kernel = Kernel::new(nethacked_kernel::Limits::default()).unwrap();
    let native_manifest = Manifest {
        id: "inputs".into(),
        exports: BTreeMap::from([(
            ENDPOINT.into(),
            Endpoint {
                major: MAJOR,
                access: Access::Write,
                max_request: 16384,
                max_reply: 16384,
            },
        )]),
        dependencies: vec![],
        extension_points: BTreeSet::new(),
        extension: None,
    };
    let native_identity = b"approved-input-test-v1".to_vec();
    let provider = kernel
        .register_native(
            native_manifest.clone(),
            &native_identity,
            BTreeMap::from([(ENDPOINT.into(), port)]),
        )
        .unwrap();
    service.0.borrow_mut().binding = Some((
        provider,
        kernel.artifact(provider).unwrap(),
        cap(),
        resource(),
    ));
    let root_manifest = manifest("root");
    let root_bytes = forward("");
    let root = kernel
        .register_guest(root_manifest.clone(), &root_bytes)
        .unwrap();
    kernel.issue_grant(root, 0, authority()).unwrap();
    let (op, lease) = kernel.capture_operation(root, "step", authority()).unwrap();
    let operation = kernel.operation_binding(op, lease, 3, "choice").unwrap();
    let out = kernel
        .invoke_operation(
            &operation,
            &encode(&sample(b"choice")).unwrap(),
            authority(),
        )
        .unwrap();
    assert_eq!(serde_json::from_slice::<Reply>(&out).unwrap().value, [10]);
    Fixture {
        kernel,
        service,
        sampled,
        observations,
        native_manifest,
        native_identity,
        root_manifest,
        root_bytes,
    }
}
fn capture(f: &mut Fixture) -> (KernelSnapshot, Checkpoint, ObservationId) {
    let operations = f.service.persistence_operations().unwrap();
    let k = f
        .kernel
        .kernel_snapshot(&KernelEnrollment {
            operations: &operations,
            pure_captures: &[],
        })
        .unwrap();
    let index = f.kernel.execution_capture_index(&k).unwrap();
    let obs = f.observations.borrow();
    let (c, ids) = f
        .service
        .capture_persistence(&index, &[&obs[0], &obs[0]])
        .unwrap();
    assert_eq!(ids[0], ids[1]);
    assert_eq!(c.observations.len(), 1);
    (k, c, ids[0])
}
fn catalog(calls: Rc<Cell<usize>>, factories: Rc<Cell<usize>>) -> Catalog {
    let mut c = Catalog::default();
    c.allow("queue".into(), 1, move || {
        factories.set(factories.get() + 1);
        let calls = calls.clone();
        Ok(Policy {
            identity: "queue".into(),
            version: 1,
            transition: Rc::new(move |s, _| {
                calls.set(calls.get() + 1);
                Ok((s[1..].to_vec(), vec![s[0]]))
            }),
        })
    })
    .unwrap();
    c
}
#[test]
fn fresh_ledger_observation_restores_without_sampling_and_rejects_old_authority() {
    let mut f = fixture();
    let old = f.service.inspect().unwrap();
    let old_observation = f.observations.borrow()[0].clone();
    let (kernel_wire, wire, id) = capture(&mut f);
    let mut wire = Checkpoint::from_bytes(&wire.to_bytes(LIMIT).unwrap(), LIMIT).unwrap();
    // Simulate a restarted allocator initially issuing a saved namespace.
    // No global counter reset is needed and other tests can allocate concurrently.
    let soon = NEXT_SERVICE.load(Ordering::Relaxed);
    wire.origins.extend(soon..soon + 32);
    let excluded = wire.origins.clone();
    let calls = Rc::new(Cell::new(0));
    let factories = Rc::new(Cell::new(0));
    let build = Restore::prepare(
        wire,
        Limits::default(),
        &kernel_wire,
        &catalog(calls.clone(), factories.clone()),
    )
    .unwrap();
    let port = build.port();
    let live: Rc<RefCell<Option<Observation>>> = Rc::new(RefCell::new(None));
    let checking = live.clone();
    let old_check = old_observation.clone();
    let authenticated = Rc::new(Cell::new(0));
    let auth_count = authenticated.clone();
    let port: NativePort = Rc::new(move |bytes, view| {
        if let Some(o) = &*checking.borrow() {
            o.validate(view)?;
            assert!(old_check.validate(view).is_err());
            auth_count.set(auth_count.get() + 1);
        }
        port(bytes, view)
    });
    let approved = Approved {
        native_manifest: f.native_manifest.clone(),
        native_identity: f.native_identity.clone(),
        port,
        root_manifest: f.root_manifest.clone(),
        root_bytes: f.root_bytes.clone(),
        contexts: kernel_wire.operation_contexts().unwrap(),
        denied: false,
    };
    let old_calls = f.sampled.clone();
    drop(f);
    let mut kernel = Kernel::new(nethacked_kernel::Limits::default()).unwrap();
    let mapped = kernel.restore_kernel(&kernel_wire, &approved).unwrap();
    let restored = build.finish(&kernel, &mapped).unwrap();
    assert_eq!(factories.get(), 1);
    assert_eq!(calls.get(), 0);
    assert_eq!(old_calls.get(), 1);
    let state = restored.service().inspect().unwrap();
    assert_eq!(state.source, old.source);
    assert_eq!(state.next_serial, old.next_serial);
    assert_eq!(state.records[0].value, old.records[0].value);
    let observation = restored.observation(id).unwrap();
    assert!(!excluded.contains(&observation.identity().record.namespace));
    *live.borrow_mut() = Some(observation.clone());
    assert_ne!(
        observation.identity().record.namespace,
        old_observation.identity().record.namespace
    );
    assert_eq!(observation.bytes(), [10]);
    let op = state.records[0].operation.clone();
    let reply = Reply {
        id: restored.record(1).unwrap(),
        value: vec![10],
    };
    let out = kernel
        .invoke_operation(
            &op,
            &encode(&replay(&reply, b"choice")).unwrap(),
            authority(),
        )
        .unwrap();
    assert_eq!(serde_json::from_slice::<Reply>(&out).unwrap(), reply);
    assert_eq!(calls.get(), 0);
    assert_eq!(authenticated.get(), 1);
    let bad = Reply {
        id: old.records[0].id,
        value: vec![10],
    };
    let out = kernel
        .invoke_operation(&op, &encode(&replay(&bad, b"choice")).unwrap(), authority())
        .unwrap();
    assert!(i32::from_le_bytes(out.try_into().unwrap()) < 0);
    assert_eq!(restored.service().inspect().unwrap(), state);
    assert_eq!(calls.get(), 0);
    // Older checkpoints and a forged phase cannot authenticate a retained record.
    for (checkpoint, phase) in [(2, "choice"), (3, "other")] {
        let stale = kernel
            .operation_binding(op.operation(), op.pipeline(), checkpoint, phase)
            .unwrap();
        let out = kernel
            .invoke_operation(
                &stale,
                &encode(&replay(&reply, b"choice")).unwrap(),
                authority(),
            )
            .unwrap();
        assert_eq!(
            i32::from_le_bytes(out.try_into().unwrap()),
            Error::Authority.abi_code()
        );
    }
    let later = kernel
        .operation_binding(op.operation(), op.pipeline(), 4, "later")
        .unwrap();
    let out = kernel
        .invoke_operation(
            &later,
            &encode(&replay(&reply, b"choice")).unwrap(),
            authority(),
        )
        .unwrap();
    assert_eq!(serde_json::from_slice::<Reply>(&out).unwrap(), reply);
    assert_eq!(calls.get(), 0);
    // Shared observation/service clones retain one source allocation and pin.
    let clone = restored.observation(id).unwrap();
    assert!(Rc::ptr_eq(&clone.service.0, &observation.service.0));
    assert_eq!(clone._code.module_count(), observation._code.module_count());
    drop(restored);
    assert_eq!(clone.bytes(), [10]);
    // Record membership and remapping were not manufactured from owner metadata.
    assert_ne!(state.records[0].caller, old.records[0].caller);
    assert_eq!(observation.identity().permit, state.records[0].permit);
    let _: ProviderBinding = observation.identity().provider.clone();
}

#[test]
fn incomplete_builder_port_remains_quarantined_after_drop_and_wire_is_bounded() {
    let mut f = fixture();
    let (kernel_wire, c, _) = capture(&mut f);
    let encoded = c.to_bytes(LIMIT).unwrap();
    assert_eq!(c.to_bytes(encoded.len()).unwrap(), encoded);
    assert!(c.to_bytes(encoded.len() - 1).is_err());
    assert!(Checkpoint::from_bytes(&encoded, encoded.len() - 1).is_err());
    let mut noncanonical = encoded.clone();
    noncanonical.push(b' ');
    assert!(Checkpoint::from_bytes(&noncanonical, LIMIT).is_err());
    let mut value: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
    value["unknown"] = true.into();
    assert!(Checkpoint::from_bytes(&serde_json::to_vec(&value).unwrap(), LIMIT).is_err());
    let calls = Rc::new(Cell::new(0));
    let factories = Rc::new(Cell::new(0));
    let build = Restore::prepare(
        c,
        Limits::default(),
        &kernel_wire,
        &catalog(calls.clone(), factories),
    )
    .unwrap();
    let weak = Rc::downgrade(&build.service.0);
    let port = build.port();
    drop(build);
    assert!(weak.upgrade().unwrap().borrow().quarantine);
    // Give the leaked factory port a real authenticated invocation. Quarantine
    // precedes dispatch, so neither a fake binding nor ledger can make it active.
    let mut k = Kernel::new(nethacked_kernel::Limits::default()).unwrap();
    k.register_native(
        f.native_manifest.clone(),
        &f.native_identity,
        BTreeMap::from([(ENDPOINT.into(), port)]),
    )
    .unwrap();
    let root = k
        .register_guest(f.root_manifest.clone(), &f.root_bytes)
        .unwrap();
    k.issue_grant(root, 0, authority()).unwrap();
    let (op, lease) = k.capture_operation(root, "step", authority()).unwrap();
    let binding = k.operation_binding(op, lease, 0, "choice").unwrap();
    let out = k
        .invoke_operation(&binding, &encode(&sample(b"choice")).unwrap(), authority())
        .unwrap();
    assert_eq!(
        i32::from_le_bytes(out.try_into().unwrap()),
        Error::Authority.abi_code()
    );
    assert_eq!(calls.get(), 0);
    assert_eq!(f.sampled.get(), 1);
    drop(k);
    assert!(weak.upgrade().is_none());
}
#[test]
fn structural_refusals_precede_factory_and_leave_source_and_destination_unchanged() {
    let mut f = fixture();
    let (kernel_wire, c, _) = capture(&mut f);
    let before = f.service.inspect().unwrap();
    let calls = Rc::new(Cell::new(0));
    let factories = Rc::new(Cell::new(0));
    let catalog = catalog(calls.clone(), factories.clone());
    for case in 0..8 {
        let mut bad = c.clone();
        match case {
            0 => bad.next_serial = 1,
            1 => bad
                .next_ordinals
                .insert(bad.records[0].operation.operation, 9)
                .map(|_| ())
                .unwrap_or(()),
            2 => bad.records[0].caller_artifact = [0; 32],
            3 => bad.records[0].permit.resolution[0].artifact = [0; 32],
            4 => bad.records[0].operation.checkpoint += 1,
            5 => bad.observations[0].pin = u64::MAX,
            6 => bad.source = vec![0; 1025],
            _ => bad.provider.module = u64::MAX,
        }
        assert!(
            Restore::prepare(bad, Limits::default(), &kernel_wire, &catalog).is_err(),
            "case {case}"
        );
    }
    assert!(Restore::prepare(
        c.clone(),
        Limits::default(),
        &kernel_wire,
        &Catalog::default()
    )
    .is_err());
    assert_eq!(factories.get(), 0);
    assert_eq!(calls.get(), 0);
    assert_eq!(f.service.inspect().unwrap(), before);
    assert_eq!(f.sampled.get(), 1);
    let build = Restore::prepare(c, Limits::default(), &kernel_wire, &catalog).unwrap();
    let mut dest = crate::support::Host::new();
    let dest_before = dest.s.inspect().unwrap();
    let approved = Approved {
        native_manifest: f.native_manifest.clone(),
        native_identity: f.native_identity.clone(),
        port: build.port(),
        root_manifest: f.root_manifest.clone(),
        root_bytes: f.root_bytes.clone(),
        contexts: kernel_wire.operation_contexts().unwrap(),
        denied: true,
    };
    assert!(dest.k.restore_kernel(&kernel_wire, &approved).is_err());
    assert_eq!(dest.s.inspect().unwrap(), dest_before);
    assert_eq!(dest.calls.get(), 0);
    assert!(build.service.0.borrow().quarantine);
    assert_eq!(f.service.inspect().unwrap(), before);
}
