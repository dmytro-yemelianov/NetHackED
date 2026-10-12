use super::*;
use nethacked_kernel::persistence::{
    ArtifactMaterial, KernelEnrollment, RegistryAuthority, RetainedAuthority, SavedModule,
};
use std::cell::Cell;
fn id(name: &str) -> Identity {
    Identity {
        name: name.into(),
        version: 1,
        artifact: [9; 32],
    }
}
struct Policy;
impl Profile for Policy {
    fn validate(&self, _: &[u8], _: &[u8]) -> Result<()> {
        Ok(())
    }
    fn select(&self, _: &[Pending], _: &[u8]) -> Result<Option<TaskId>> {
        panic!("load selected")
    }
    fn ready(&self, _: &Pending, _: &[u8]) -> Result<bool> {
        panic!("load checked readiness")
    }
}
struct Targets;
impl TargetValidator for Targets {
    fn validate(&self, t: &[Stamp]) -> Result<()> {
        if t.is_empty() {
            Ok(())
        } else {
            Err(Error::InvalidHandle)
        }
    }
}
impl TargetCodec for Targets {
    fn validate_old(&self, t: &[Stamp]) -> Result<()> {
        self.validate(t)
    }
    fn remap(&self, t: &[Stamp], _: &RestoredKernel) -> Result<Vec<Stamp>> {
        self.validate(t)?;
        Ok(vec![])
    }
}
struct Cleanup(Option<CleanupIdentity>);
impl CleanupParticipant for Cleanup {
    fn identity(&self) -> Result<Option<CleanupIdentity>> {
        Ok(self.0)
    }
    fn discard(&self, _: &OperationBinding) -> Result<()> {
        panic!("load discarded")
    }
}
struct Approved {
    checkpoint: Checkpoint,
    module: Manifest,
    artifact: Vec<u8>,
    port: NativePort,
    cleanup: CleanupIdentity,
}
impl RegistryAuthority for Approved {
    fn resolve(&self, m: &SavedModule) -> Result<ArtifactMaterial> {
        if m.manifest != self.module {
            return Err(Error::Authority);
        }
        Ok(ArtifactMaterial::Native {
            manifest: self.module.clone(),
            identity: self.artifact.clone(),
            ports: BTreeMap::from([("queue".into(), self.port.clone())]),
        })
    }
    fn bind_resource(&self, r: &Resource) -> Result<Resource> {
        if r == &self.checkpoint.queue {
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
        _: &Authority,
    ) -> Result<Authority> {
        Err(Error::Authority)
    }
}
impl RetainedAuthority for Approved {
    fn approve_cleanup(&self, id: u64) -> Result<()> {
        if self
            .checkpoint
            .participants
            .iter()
            .any(|p| p.cleanup == Some(id))
        {
            Ok(())
        } else {
            Err(Error::Authority)
        }
    }
}
impl TimerAuthority for Approved {
    fn approve(&self, c: &Checkpoint, _: &KernelSnapshot) -> Result<()> {
        if c == &self.checkpoint {
            Ok(())
        } else {
            Err(Error::Authority)
        }
    }
}
#[test]
fn shared_and_exclusive_guards_are_capture_busy_and_ordered_cleanup_aliases_share_one_allocation() {
    let cleanup = CleanupIdentity::issue().unwrap();
    let identified: Rc<dyn CleanupParticipant> = Rc::new(Cleanup(Some(cleanup)));
    let plain: Rc<dyn CleanupParticipant> = Rc::new(Cleanup(None));
    let queue = Resource {
        owner: "timers".into(),
        key: "queue".into(),
    };
    let service = Service::new(
        Limits::default(),
        queue,
        Rc::new(Policy),
        Rc::new(Targets),
        true,
        vec![identified.clone(), plain.clone(), plain.clone()],
    )
    .unwrap();
    // A previously reconstructed identified alias is a legal local state. The
    // ordinary constructor's existing duplicate-identity refusal stays unchanged.
    {
        let mut i = service.0.borrow_mut();
        i.participants.push(identified.clone());
        i.participant_identities.push(Some(cleanup));
    }
    let mut kernel = Kernel::new(nethacked_kernel::Limits::default()).unwrap();
    let module = service.install(&mut kernel, "timers").unwrap();
    let ops = service.persistence_enrollment().unwrap();
    let saved = kernel
        .kernel_snapshot_with_service_roots(
            &KernelEnrollment {
                operations: &ops,
                pure_captures: &[],
            },
            &service.persistence_cleanup().unwrap(),
            &service.persistence_resources().unwrap(),
        )
        .unwrap();
    let index = kernel.execution_capture_index(&saved).unwrap();
    let mut capture = CaptureCatalog::new(&service, id("profile"), id("targets"), None).unwrap();
    capture.participant(id("identified"), identified).unwrap();
    capture.participant(id("plain"), plain).unwrap();
    let shared = service.0.borrow();
    assert!(matches!(
        service.capture_local(&index, &capture),
        Err(PersistenceError::CaptureBusy)
    ));
    assert!(matches!(
        service.persistence_enrollment(),
        Err(PersistenceError::CaptureBusy)
    ));
    drop(shared);
    let exclusive = service.0.borrow_mut();
    assert!(matches!(
        service.capture_local(&index, &capture),
        Err(PersistenceError::CaptureBusy)
    ));
    drop(exclusive);
    let c = service.capture_local(&index, &capture).unwrap();
    assert_eq!(c.occurrences, vec![0, 1, 1, 0]);
    assert_eq!(c.participants.len(), 2);
    let c = Checkpoint::from_bytes(&c.to_bytes(1 << 20).unwrap(), 1 << 20).unwrap();
    let count = Rc::new(Cell::new(0));
    let mut catalog = RestoreCatalog::new(
        id("profile"),
        Rc::new(Policy),
        id("targets"),
        Rc::new(Targets),
        Rc::new(Targets),
    )
    .unwrap();
    for name in ["identified", "plain"] {
        let count = count.clone();
        catalog
            .participant(id(name), move |identity| {
                count.set(count.get() + 1);
                Ok(Rc::new(Cleanup(identity)))
            })
            .unwrap();
    }
    let approved = Approved {
        checkpoint: c.clone(),
        module: kernel.manifest(module).unwrap().clone(),
        artifact: encode(&(
            "nethacked-timers-queue-v1",
            env!("CARGO_PKG_VERSION"),
            Limits::default(),
            c.queue.clone(),
        ))
        .unwrap(),
        port: catalog.native_port(),
        cleanup,
    };
    let source_history = kernel.lifecycle_state();
    let stage = c
        .validate(&saved, &catalog, &approved, &Limits::default())
        .unwrap();
    assert_eq!(kernel.lifecycle_state(), source_history);
    drop(capture);
    drop(service);
    drop(kernel);
    let mut fresh = Kernel::new(nethacked_kernel::Limits::default()).unwrap();
    let mapped = fresh.restore_kernel(&saved, &approved).unwrap();
    let restored = stage.rebuild(&fresh, &mapped, &catalog).unwrap();
    let inner = restored.0.borrow();
    assert_eq!(count.get(), 2);
    assert_eq!(inner.participants.len(), 4);
    assert!(Rc::ptr_eq(&inner.participants[0], &inner.participants[3]));
    assert!(Rc::ptr_eq(&inner.participants[1], &inner.participants[2]));
    assert_eq!(inner.identities.len(), 1);
    assert_ne!(inner.identities[0], approved.cleanup);
}

#[test]
fn namespace_history_allocator_boundaries_and_burned_allocations() {
    let counter = AtomicU64::new(1);
    assert_eq!(issue_namespace_after(&counter, 7).unwrap(), 8);
    assert_eq!(issue_namespace_after(&counter, 7).unwrap(), 9);
    assert_eq!(issue_namespace(&counter).unwrap(), 10);
    assert_eq!(issue_namespace_after(&counter, 3).unwrap(), 11);
    assert_eq!(counter.load(Ordering::Relaxed), 12);
    let counter = AtomicU64::new(1);
    assert_eq!(issue_namespace_after(&counter, u64::MAX), Err(Error::Limit));
    assert_eq!(counter.load(Ordering::Relaxed), 1);
    assert_eq!(
        issue_namespace_after(&counter, u64::MAX - 2).unwrap(),
        u64::MAX - 1
    );
    assert_eq!(issue_namespace(&counter), Err(Error::Limit));
    assert_eq!(issue_namespace_after(&counter, 1), Err(Error::Limit));
    assert_eq!(counter.load(Ordering::Relaxed), u64::MAX);
    let counter = AtomicU64::new(1);
    assert_eq!(
        issue_namespace_after(&counter, u64::MAX - 1),
        Err(Error::Limit)
    );
    assert_eq!(counter.load(Ordering::Relaxed), u64::MAX);
    assert_eq!(issue_namespace(&counter), Err(Error::Limit));
}
