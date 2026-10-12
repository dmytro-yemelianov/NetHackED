#![cfg(feature = "persistence")]
mod support;
use nethacked_kernel::{persistence::*, *};
use nethacked_timers::{persistence::*, Limits as TimerLimits, Request, Service, TaskId};
use nethacked_workflows::CleanupParticipant;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};
use support::{guest, queue_key, Clock};
const MAX: usize = 1 << 20;
fn id(s: &str) -> Identity {
    Identity {
        name: s.into(),
        version: 1,
        artifact: [5; 32],
    }
}
fn all() -> Authority {
    Authority {
        access: Access::Write,
        scope: Scope::Keys([queue_key()].into()),
    }
}
struct EmptyTargets {
    checks: Rc<Cell<usize>>,
}
impl nethacked_timers::TargetValidator for EmptyTargets {
    fn validate(&self, t: &[nethacked_resources::Stamp]) -> Result<()> {
        if t.is_empty() {
            Ok(())
        } else {
            Err(Error::InvalidHandle)
        }
    }
}
impl TargetCodec for EmptyTargets {
    fn validate_old(&self, t: &[nethacked_resources::Stamp]) -> Result<()> {
        self.checks.set(self.checks.get() + 1);
        nethacked_timers::TargetValidator::validate(self, t)
    }
    fn remap(
        &self,
        t: &[nethacked_resources::Stamp],
        _: &RestoredKernel,
    ) -> Result<Vec<nethacked_resources::Stamp>> {
        nethacked_timers::TargetValidator::validate(self, t)?;
        Ok(vec![])
    }
}
struct Cleanup {
    identity: Option<CleanupIdentity>,
    busy: Rc<Cell<bool>>,
    calls: Rc<Cell<usize>>,
}
impl CleanupParticipant for Cleanup {
    fn identity(&self) -> Result<Option<CleanupIdentity>> {
        Ok(self.identity)
    }
    fn discard(&self, _: &OperationBinding) -> Result<()> {
        self.calls.set(self.calls.get() + 1);
        if self.busy.get() {
            Err(Error::NativeRefusal)
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Material {
    manifest: Manifest,
    bytes: Vec<u8>,
    guest: bool,
}
struct Admission {
    materials: BTreeMap<[u8; 32], Material>,
    contexts: Vec<SavedOperationContext>,
    timer: NativePort,
    effects: Rc<Cell<usize>>,
    deny: bool,
    grants: Vec<SavedGrant>,
    cleanups: std::collections::BTreeSet<u64>,
}
impl RegistryAuthority for Admission {
    fn resolve(&self, m: &SavedModule) -> Result<ArtifactMaterial> {
        let a = self
            .materials
            .get(&m.artifact)
            .ok_or(Error::InvalidModule)?;
        if m.manifest != a.manifest || self.deny {
            return Err(Error::Authority);
        }
        if a.guest {
            return Ok(ArtifactMaterial::Guest {
                manifest: a.manifest.clone(),
                bytes: a.bytes.clone(),
            });
        }
        let port: NativePort = if a.manifest.id == "timers" {
            self.timer.clone()
        } else {
            let calls = self.effects.clone();
            Rc::new(move |_, _| {
                calls.set(calls.get() + 1);
                Ok(br#"{"kind":"Completed","result":[9]}"#.to_vec())
            })
        };
        Ok(ArtifactMaterial::Native {
            manifest: a.manifest.clone(),
            identity: a.bytes.clone(),
            ports: a
                .manifest
                .exports
                .keys()
                .map(|n| (n.clone(), port.clone()))
                .collect(),
        })
    }
    fn bind_resource(&self, r: &Resource) -> Result<Resource> {
        if r == &queue_key() {
            Ok(r.clone())
        } else {
            Err(Error::Authority)
        }
    }
    fn approve_grant(
        &self,
        caller: &SavedModule,
        slot: usize,
        target: &SavedModule,
        a: &Authority,
    ) -> Result<Authority> {
        if self.grants.iter().any(|g| {
            g.caller == caller.id && g.slot == slot && g.target == target.id && &g.requested == a
        }) {
            Ok(a.clone())
        } else {
            Err(Error::Authority)
        }
    }
}
impl RetainedAuthority for Admission {
    fn approve_cleanup(&self, id: u64) -> Result<()> {
        if self.cleanups.contains(&id) {
            Ok(())
        } else {
            Err(Error::Authority)
        }
    }
    fn approve_operation_context(&self, c: &SavedOperationContext) -> Result<()> {
        if self.contexts.contains(c) {
            Ok(())
        } else {
            Err(Error::Authority)
        }
    }
}
struct Approved(Checkpoint);
impl TimerAuthority for Approved {
    fn approve(&self, c: &Checkpoint, _: &KernelSnapshot) -> Result<()> {
        if &self.0 == c {
            Ok(())
        } else {
            Err(Error::Authority)
        }
    }
}
type Hook = Rc<RefCell<Option<Box<dyn Fn()>>>>;
struct Fixture {
    kernel: Kernel,
    service: Service,
    actor: ModuleHandle,
    materials: BTreeMap<[u8; 32], Material>,
    busy: Rc<Cell<bool>>,
    effects: Rc<Cell<usize>>,
    cleanup_calls: Rc<Cell<usize>>,
    hook: Hook,
    source_cleanup: CleanupIdentity,
}
impl Fixture {
    fn new() -> Self {
        let mut kernel = Kernel::new(nethacked_kernel::Limits::default()).unwrap();
        let busy = Rc::new(Cell::new(false));
        let effects = Rc::new(Cell::new(0));
        let cleanup_calls = Rc::new(Cell::new(0));
        let hook: Hook = Rc::new(RefCell::new(None));
        let calls = effects.clone();
        let callback = hook.clone();
        let m = support::manifest("effect", &["run"]);
        let bytes = b"timer-effect-test".to_vec();
        let effect = kernel
            .register_native(
                m.clone(),
                &bytes,
                BTreeMap::from([(
                    "run".into(),
                    Rc::new(move |_: &[u8], _: &CallerView| {
                        calls.set(calls.get() + 1);
                        if let Some(f) = callback.borrow().as_ref() {
                            f();
                        }
                        Ok(br#"{"kind":"Completed","result":[9]}"#.to_vec())
                    }) as NativePort,
                )]),
            )
            .unwrap();
        let mut materials = BTreeMap::from([(
            kernel.artifact(effect).unwrap(),
            Material {
                manifest: m,
                bytes,
                guest: false,
            },
        )]);
        let participant: Rc<dyn CleanupParticipant> = Rc::new(Cleanup {
            identity: Some(CleanupIdentity::issue().unwrap()),
            busy: busy.clone(),
            calls: cleanup_calls.clone(),
        });
        let service = Service::new(
            TimerLimits::default(),
            queue_key(),
            Rc::new(Clock::default()),
            Rc::new(EmptyTargets {
                checks: Rc::new(Cell::new(0)),
            }),
            true,
            vec![participant],
        )
        .unwrap();
        let timer = service.install(&mut kernel, "timers").unwrap();
        let empty = kernel.kernel_snapshot(&Default::default()).unwrap();
        let m = empty
            .registry
            .modules
            .iter()
            .find(|m| m.manifest.id == "timers")
            .unwrap()
            .manifest
            .clone();
        let bytes = serde_json::to_vec(
            &serde_json::to_value((
                "nethacked-timers-queue-v1",
                env!("CARGO_PKG_VERSION"),
                TimerLimits::default(),
                queue_key(),
            ))
            .unwrap(),
        )
        .unwrap();
        materials.insert(
            kernel.artifact(timer).unwrap(),
            Material {
                manifest: m,
                bytes,
                guest: false,
            },
        );
        let mut m = support::manifest("actor", &["run", "callback"]);
        m.exports.values_mut().for_each(|e| {
            e.max_request = 16384;
            e.max_reply = 16384;
        });
        m.dependencies = vec![
            support::dep("timers", "queue"),
            support::dep("effect", "run"),
        ];
        let bytes = guest("", "");
        let actor = kernel.register_guest(m.clone(), &bytes).unwrap();
        materials.insert(
            kernel.artifact(actor).unwrap(),
            Material {
                manifest: m,
                bytes,
                guest: true,
            },
        );
        kernel.issue_grant(actor, 0, all()).unwrap();
        kernel.issue_grant(actor, 1, all()).unwrap();
        Self {
            kernel,
            service,
            actor,
            materials,
            busy,
            effects,
            cleanup_calls,
            hook,
            source_cleanup: CleanupIdentity::issue().unwrap(),
        }
    }
    fn request(&mut self, req: Request) -> Vec<u8> {
        let (op, lease) = self
            .kernel
            .capture_operation(self.actor, "run", all())
            .unwrap();
        let binding = self
            .kernel
            .operation_binding_with_cleanup(op, lease, 0, "source", &[self.source_cleanup])
            .unwrap();
        let bytes = self
            .kernel
            .invoke_operation(&binding, &serde_json::to_vec(&req).unwrap(), all())
            .unwrap();
        self.kernel.release_pipeline(lease).unwrap();
        bytes
    }
    fn schedule(&mut self, deadline: u8) -> TaskId {
        serde_json::from_slice(&self.request(Request::Schedule {
            callback: "callback".into(),
            payload: vec![deadline],
            targets: vec![],
            deadline: vec![deadline],
            residency: vec![1],
            ceiling: all(),
        }))
        .unwrap()
    }
    fn catalog(&self) -> CaptureCatalog {
        let mut c = CaptureCatalog::new(&self.service, id("clock"), id("empty"), None).unwrap();
        for p in self.service.persistence_participants().unwrap() {
            c.participant(id("cleanup"), p).unwrap();
        }
        c
    }
    fn save(&mut self) -> (KernelSnapshot, Checkpoint) {
        let ops = self.service.persistence_enrollment().unwrap();
        let cleanup = self.service.persistence_cleanup().unwrap();
        let saved = self
            .kernel
            .kernel_snapshot_with_service_roots(
                &KernelEnrollment {
                    operations: &ops,
                    pure_captures: &[],
                },
                &cleanup,
                &self.service.persistence_resources().unwrap(),
            )
            .unwrap();
        let index = self.kernel.execution_capture_index(&saved).unwrap();
        let checkpoint = self.service.capture_local(&index, &self.catalog()).unwrap();
        (saved, checkpoint)
    }
}
fn restore_catalog(
    checks: Rc<Cell<usize>>,
    busy: Rc<Cell<bool>>,
    calls: Rc<Cell<usize>>,
) -> RestoreCatalog {
    let target = Rc::new(EmptyTargets { checks });
    let mut c = RestoreCatalog::new(
        id("clock"),
        Rc::new(Clock::default()),
        id("empty"),
        target.clone(),
        target,
    )
    .unwrap();
    c.participant(id("cleanup"), move |identity| {
        Ok(Rc::new(Cleanup {
            identity,
            busy: busy.clone(),
            calls: calls.clone(),
        }))
    })
    .unwrap();
    c
}
fn wire(k: &KernelSnapshot) -> KernelSnapshot {
    let bytes = nethacked_workflows::persistence::encode_local_checkpoint(k, MAX).unwrap();
    nethacked_workflows::persistence::decode_local_checkpoint(&bytes, MAX).unwrap()
}
#[test]
fn actual_bytes_source_disposed_ties_deferred_cancelled_terminal_nonreuse_and_repeat() {
    let mut f = Fixture::new();
    let first = f.schedule(1);
    let second = f.schedule(1);
    let deferred = f.schedule(4);
    let cancelled = f.schedule(2);
    f.request(Request::Cancel { task: cancelled });
    let (ks, c) = f.save();
    let approved = Approved(c.clone());
    let materials = f.materials.clone();
    let contexts = ks.operation_contexts().unwrap();
    let known_grants = ks.registry.grants.clone();
    let known_cleanups = ks.cleanup_history.clone();
    let before = f.service.state().unwrap();
    assert_eq!(f.effects.get(), 0);
    drop(f);
    let ks = wire(&ks);
    let c = Checkpoint::from_bytes(&c.to_bytes(MAX).unwrap(), MAX).unwrap();
    let checks = Rc::new(Cell::new(0));
    let effects = Rc::new(Cell::new(0));
    let catalog = restore_catalog(checks, Rc::new(Cell::new(false)), Rc::new(Cell::new(0)));
    let stage = c
        .validate(&ks, &catalog, &approved, &TimerLimits::default())
        .unwrap();
    let host = Admission {
        materials,
        contexts,
        timer: catalog.native_port(),
        effects: effects.clone(),
        deny: false,
        grants: known_grants.clone(),
        cleanups: known_cleanups.clone(),
    };
    let mut kernel = Kernel::new(nethacked_kernel::Limits::default()).unwrap();
    let mapped = kernel.restore_kernel(&ks, &host).unwrap();
    let service = stage.rebuild(&kernel, &mapped, &catalog).unwrap();
    assert_eq!(effects.get(), 0);
    assert_ne!(service.state().unwrap().queue, before.queue);
    assert!(service.retry_cleanup(first).is_err());
    assert_eq!(
        service
            .deliver_next(&mut kernel, &[1, 1])
            .unwrap()
            .unwrap()
            .id
            .serial,
        first.serial
    );
    assert_eq!(
        service
            .deliver_next(&mut kernel, &[1, 1])
            .unwrap()
            .unwrap()
            .id
            .serial,
        second.serial
    );
    assert!(service
        .deliver_next(&mut kernel, &[3, 1])
        .unwrap()
        .is_none());
    assert_eq!(effects.get(), 2);
    let terminal = service
        .state()
        .unwrap()
        .tasks
        .values()
        .find(|t| t.id.serial == cancelled.serial)
        .unwrap()
        .id;
    assert!(matches!(
        service.state().unwrap().tasks[&terminal].status,
        nethacked_timers::Status::Cancelled
    ));
    let ops = service.persistence_enrollment().unwrap();
    let cleanup = service.persistence_cleanup().unwrap();
    let saved = kernel
        .kernel_snapshot_with_service_roots(
            &KernelEnrollment {
                operations: &ops,
                pure_captures: &[],
            },
            &cleanup,
            &service.persistence_resources().unwrap(),
        )
        .unwrap();
    let index = kernel.execution_capture_index(&saved).unwrap();
    let mut capture = CaptureCatalog::new(&service, id("clock"), id("empty"), None).unwrap();
    for p in service.persistence_participants().unwrap() {
        capture.participant(id("cleanup"), p).unwrap();
    }
    let second_save = service.capture_local(&index, &capture).unwrap();
    assert_eq!(second_save.next_serial, c.next_serial);
    assert_eq!(second_save.tasks.len(), 4);
    let approved2 = Approved(second_save.clone());
    let materials = host.materials.clone();
    let contexts = saved.operation_contexts().unwrap();
    let known_grants = saved.registry.grants.clone();
    let known_cleanups = saved.cleanup_history.clone();
    let wire2 = second_save.to_bytes(MAX).unwrap();
    let ks2 = wire(&saved);
    drop(capture);
    drop(service);
    drop(mapped);
    drop(kernel);
    drop(host);
    drop(catalog);
    let second_save = Checkpoint::from_bytes(&wire2, MAX).unwrap();
    let effects = Rc::new(Cell::new(0));
    let catalog = restore_catalog(
        Rc::new(Cell::new(0)),
        Rc::new(Cell::new(false)),
        Rc::new(Cell::new(0)),
    );
    let host = Admission {
        materials,
        contexts,
        timer: catalog.native_port(),
        effects: effects.clone(),
        deny: false,
        grants: known_grants.clone(),
        cleanups: known_cleanups.clone(),
    };
    let mut kernel = Kernel::new(nethacked_kernel::Limits::default()).unwrap();
    let mapped = kernel.restore_kernel(&ks2, &host).unwrap();
    let service = second_save
        .validate(&ks2, &catalog, &approved2, &TimerLimits::default())
        .unwrap()
        .rebuild(&kernel, &mapped, &catalog)
        .unwrap();
    assert_eq!(effects.get(), 0);
    assert_eq!(
        service
            .deliver_next(&mut kernel, &[4, 1])
            .unwrap()
            .unwrap()
            .id
            .serial,
        deferred.serial
    );
    assert_eq!(effects.get(), 1);
    // A repeated load is a one-use refusal before codecs/factories or delivery.
    assert!(second_save
        .validate(&ks2, &catalog, &approved2, &TimerLimits::default())
        .unwrap()
        .rebuild(&kernel, &mapped, &catalog)
        .is_err());
}
#[test]
fn terminal_busy_retries_discard_only_and_capture_never_executes() {
    let mut f = Fixture::new();
    let id = f.schedule(1);
    f.busy.set(true);
    let delivered = f
        .service
        .deliver_next(&mut f.kernel, &[1, 1])
        .unwrap()
        .unwrap();
    assert!(delivered.cleanup_pending);
    assert_eq!(f.effects.get(), 1);
    let calls = f.cleanup_calls.get();
    let (ks, c) = f.save();
    assert_eq!(f.cleanup_calls.get(), calls);
    let materials = f.materials.clone();
    let contexts = ks.operation_contexts().unwrap();
    let known_grants = ks.registry.grants.clone();
    let known_cleanups = ks.cleanup_history.clone();
    let approved = Approved(c.clone());
    drop(f);
    let c = Checkpoint::from_bytes(&c.to_bytes(MAX).unwrap(), MAX).unwrap();
    let ks = wire(&ks);
    let busy = Rc::new(Cell::new(true));
    let calls = Rc::new(Cell::new(0));
    let effects = Rc::new(Cell::new(0));
    let catalog = restore_catalog(Rc::new(Cell::new(0)), busy.clone(), calls.clone());
    let stage = c
        .validate(&ks, &catalog, &approved, &TimerLimits::default())
        .unwrap();
    let host = Admission {
        materials,
        contexts,
        timer: catalog.native_port(),
        effects: effects.clone(),
        deny: false,
        grants: known_grants.clone(),
        cleanups: known_cleanups.clone(),
    };
    let mut kernel = Kernel::new(nethacked_kernel::Limits::default()).unwrap();
    let mapped = kernel.restore_kernel(&ks, &host).unwrap();
    let service = stage.rebuild(&kernel, &mapped, &catalog).unwrap();
    assert_eq!(calls.get(), 0);
    assert_eq!(effects.get(), 0);
    let fresh = *service.state().unwrap().tasks.keys().next().unwrap();
    assert_ne!(fresh, id);
    assert!(service.retry_cleanup(fresh).is_err());
    busy.set(false);
    service.retry_cleanup(fresh).unwrap();
    assert_eq!(effects.get(), 0);
    assert!(service
        .deliver_next(&mut kernel, &[9, 1])
        .unwrap()
        .is_none());
    service.dispose(&mut kernel, fresh).unwrap();
}
#[test]
fn malformed_cross_rows_catalog_and_authority_refuse_before_policy() {
    let mut f = Fixture::new();
    f.schedule(1);
    f.schedule(2);
    let (ks, c) = f.save();
    let approved = Approved(c.clone());
    let checks = Rc::new(Cell::new(0));
    let catalog = restore_catalog(
        checks.clone(),
        Rc::new(Cell::new(false)),
        Rc::new(Cell::new(0)),
    );
    for n in 0..7 {
        let mut bad = c.clone();
        match n {
            0 => bad.tasks.swap(0, 1),
            1 => {
                let a = bad.tasks[0].callback;
                bad.tasks[0].callback = bad.tasks[1].callback;
                bad.tasks[1].callback = a;
            }
            2 => bad.profile = id("missing"),
            3 => bad.tasks[0].ceiling.access = Access::Read,
            4 => bad.tasks[0].deadline = vec![9],
            5 => bad.participants[0].cleanup = Some(0),
            _ => {
                bad.tasks[0].targets = vec![nethacked_resources::Stamp {
                    address: queue_key(),
                    generation: 0,
                    revision: 0,
                }]
            }
        };
        assert!(bad
            .validate(&ks, &catalog, &approved, &TimerLimits::default())
            .is_err());
        assert_eq!(checks.get(), 0);
    }
    let mut bytes = c.to_bytes(MAX).unwrap();
    bytes.push(b' ');
    assert!(Checkpoint::from_bytes(&bytes, MAX).is_err());
    assert!(c
        .validate(&ks, &catalog, &approved, &TimerLimits::default())
        .is_ok());
    assert_eq!(checks.get(), 2);
}
#[test]
fn actual_running_callback_reentrant_capture_is_busy() {
    let mut f = Fixture::new();
    f.schedule(1);
    let service = f.service.clone();
    let catalog = Rc::new(f.catalog());
    let seen = Rc::new(Cell::new(false));
    let flag = seen.clone();
    *f.hook.borrow_mut() = Some(Box::new(move || {
        let mut unrelated = Kernel::new(nethacked_kernel::Limits::default()).unwrap();
        let saved = unrelated.kernel_snapshot(&Default::default()).unwrap();
        let index = unrelated.execution_capture_index(&saved).unwrap();
        assert!(matches!(
            service.capture_local(&index, &catalog),
            Err(PersistenceError::CaptureBusy)
        ));
        flag.set(true);
    }));
    f.service.deliver_next(&mut f.kernel, &[1, 1]).unwrap();
    assert!(seen.get());
    assert_eq!(f.effects.get(), 1);
}

#[test]
fn failed_nonempty_destination_missing_artifact_and_revoked_stage_preserve_retry() {
    let mut f = Fixture::new();
    f.schedule(1);
    let (ks, c) = f.save();
    let approved = Approved(c.clone());
    let checks = Rc::new(Cell::new(0));
    let effects = Rc::new(Cell::new(0));
    let catalog = restore_catalog(
        checks.clone(),
        Rc::new(Cell::new(false)),
        Rc::new(Cell::new(0)),
    );
    let contexts = ks.operation_contexts().unwrap();
    let known_grants = ks.registry.grants.clone();
    let known_cleanups = ks.cleanup_history.clone();
    let mut materials = f.materials.clone();
    let missing = ks
        .registry
        .modules
        .iter()
        .find(|m| m.manifest.id == "actor")
        .unwrap()
        .artifact;
    let material = materials.remove(&missing).unwrap();
    drop(f);
    let mut host = Admission {
        materials,
        contexts,
        timer: catalog.native_port(),
        effects: effects.clone(),
        deny: false,
        grants: known_grants.clone(),
        cleanups: known_cleanups.clone(),
    };
    let mut kernel = Kernel::new(nethacked_kernel::Limits::default()).unwrap();
    kernel
        .register_native(
            support::manifest("populated", &["run"]),
            b"unrelated",
            BTreeMap::from([(
                "run".into(),
                Rc::new(|_: &[u8], _: &CallerView| Ok(b"preserved".to_vec())) as NativePort,
            )]),
        )
        .unwrap();
    let before = kernel.lifecycle_state();
    assert!(kernel.restore_kernel(&ks, &host).is_err());
    assert_eq!(kernel.lifecycle_state(), before);
    assert_eq!(effects.get(), 0);
    host.materials.insert(missing, material);
    let mapped = kernel.restore_kernel(&ks, &host).unwrap();
    let actor = mapped.modules()[&ks
        .registry
        .modules
        .iter()
        .find(|m| m.manifest.id == "actor")
        .unwrap()
        .id];
    let stage = c
        .validate(&ks, &catalog, &approved, &TimerLimits::default())
        .unwrap();
    checks.set(0);
    kernel.revoke_grant(actor, 0).unwrap();
    let before = kernel.lifecycle_state();
    assert!(stage.rebuild(&kernel, &mapped, &catalog).is_err());
    assert_eq!(kernel.lifecycle_state(), before);
    assert_eq!(checks.get(), 0);
    assert_eq!(effects.get(), 0);
    kernel
        .issue_grant(
            actor,
            0,
            Authority {
                access: Access::Write,
                scope: Scope::Keys(Default::default()),
            },
        )
        .unwrap();
    let narrowed = c
        .validate(&ks, &catalog, &approved, &TimerLimits::default())
        .unwrap();
    checks.set(0);
    let before = kernel.lifecycle_state();
    assert!(narrowed.rebuild(&kernel, &mapped, &catalog).is_err());
    assert_eq!(kernel.lifecycle_state(), before);
    assert_eq!(checks.get(), 0);
    assert_eq!(effects.get(), 0);
    kernel.issue_grant(actor, 0, all()).unwrap();
    let service = c
        .validate(&ks, &catalog, &approved, &TimerLimits::default())
        .unwrap()
        .rebuild(&kernel, &mapped, &catalog)
        .unwrap();
    assert_eq!(effects.get(), 0);
    service.deliver_next(&mut kernel, &[1, 1]).unwrap();
    assert_eq!(effects.get(), 1);
}

#[test]
fn cancelled_without_live_source_or_grants_and_empty_queue_roundtrip() {
    for cancelled in [false, true] {
        let mut f = Fixture::new();
        if cancelled {
            let task = f.schedule(1);
            f.request(Request::Cancel { task });
        }
        f.kernel.unregister(f.actor).unwrap();
        let (ks, c) = f.save();
        let trusted = Approved(c.clone());
        let materials = f.materials.clone();
        let contexts = ks.operation_contexts().unwrap();
        let grants = ks.registry.grants.clone();
        let cleanups = ks.cleanup_history.clone();
        drop(f);
        assert!(ks.callbacks.is_empty());
        assert!(ks.operations.is_empty());
        let ks = wire(&ks);
        let c = Checkpoint::from_bytes(&c.to_bytes(MAX).unwrap(), MAX).unwrap();
        let effects = Rc::new(Cell::new(0));
        let catalog = restore_catalog(
            Rc::new(Cell::new(0)),
            Rc::new(Cell::new(false)),
            Rc::new(Cell::new(0)),
        );
        let host = Admission {
            materials,
            contexts,
            timer: catalog.native_port(),
            effects: effects.clone(),
            deny: false,
            grants,
            cleanups,
        };
        let mut kernel = Kernel::new(nethacked_kernel::Limits::default()).unwrap();
        let mapped = kernel.restore_kernel(&ks, &host).unwrap();
        let service = c
            .validate(&ks, &catalog, &trusted, &TimerLimits::default())
            .unwrap()
            .rebuild(&kernel, &mapped, &catalog)
            .unwrap();
        assert_eq!(service.state().unwrap().next_serial, c.next_serial);
        assert_eq!(service.state().unwrap().tasks.len(), usize::from(cancelled));
        assert!(service
            .deliver_next(&mut kernel, &[9, 1])
            .unwrap()
            .is_none());
        assert_eq!(effects.get(), 0);
    }
}

// Adapted from the archived review probes/tests/timers.rs fresh-process fixture.
// Each worker starts without any timer allocation, reads actual checkpoint bytes,
// and receives artifact material through a separate host-catalog file.
fn process_save(dir: &std::path::Path, f: &mut Fixture, held: &[TaskId]) {
    let (kernel, timer) = f.save();
    std::fs::write(
        dir.join("kernel.json"),
        nethacked_workflows::persistence::encode_local_checkpoint(&kernel, MAX).unwrap(),
    )
    .unwrap();
    std::fs::write(dir.join("timer.json"), timer.to_bytes(MAX).unwrap()).unwrap();
    std::fs::write(
        dir.join("host.json"),
        serde_json::to_vec(&f.materials.iter().collect::<Vec<_>>()).unwrap(),
    )
    .unwrap();
    std::fs::write(dir.join("held.json"), serde_json::to_vec(held).unwrap()).unwrap();
}

#[test]
fn fresh_process_generations_reject_all_retired_timer_ids() {
    let dir = std::env::temp_dir().join(format!("timer-history-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for generation in 0..5 {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "fresh_process_generation_worker",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("TIMER_HISTORY_DIR", &dir)
            .env("TIMER_HISTORY_GENERATION", generation.to_string())
            .status()
            .unwrap();
        assert!(status.success(), "generation {generation} failed");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

fn fresh_request(f: &mut Fixture, req: Request) -> Result<Vec<u8>> {
    let (op, lease) = f.kernel.capture_operation(f.actor, "run", all()).unwrap();
    let binding = f
        .kernel
        .operation_binding(op, lease, 0, "fresh-authorized-request")
        .unwrap();
    let result = f
        .kernel
        .invoke_operation(&binding, &serde_json::to_vec(&req).unwrap(), all());
    f.kernel.release_pipeline(lease).unwrap();
    result
}

#[test]
#[ignore = "invoked in a fresh OS process by the generation regression"]
fn fresh_process_generation_worker() {
    let dir = std::path::PathBuf::from(std::env::var_os("TIMER_HISTORY_DIR").unwrap());
    let generation: usize = std::env::var("TIMER_HISTORY_GENERATION")
        .unwrap()
        .parse()
        .unwrap();
    if generation == 0 {
        let mut f = Fixture::new();
        let held = f.schedule(1);
        assert_eq!(held.queue, 1);
        let mut ordinary = Fixture::new();
        let ordinary_id = ordinary.schedule(1);
        assert_eq!(ordinary_id.queue, 2);
        drop(ordinary);
        process_save(&dir, &mut f, &[held, ordinary_id]);
        return;
    }
    let ks: KernelSnapshot = nethacked_workflows::persistence::decode_local_checkpoint(
        &std::fs::read(dir.join("kernel.json")).unwrap(),
        MAX,
    )
    .unwrap();
    let c = Checkpoint::from_bytes(&std::fs::read(dir.join("timer.json")).unwrap(), MAX).unwrap();
    let materials: Vec<([u8; 32], Material)> =
        serde_json::from_slice(&std::fs::read(dir.join("host.json")).unwrap()).unwrap();
    let mut held: Vec<TaskId> =
        serde_json::from_slice(&std::fs::read(dir.join("held.json")).unwrap()).unwrap();
    assert_eq!(held.len(), generation * 2);
    let effects = Rc::new(Cell::new(0));
    let busy = Rc::new(Cell::new(false));
    let cleanup_calls = Rc::new(Cell::new(0));
    let catalog = restore_catalog(Rc::new(Cell::new(0)), busy.clone(), cleanup_calls.clone());
    let host = Admission {
        materials: materials.into_iter().collect(),
        contexts: ks.operation_contexts().unwrap(),
        timer: catalog.native_port(),
        effects: effects.clone(),
        deny: false,
        grants: ks.registry.grants.clone(),
        cleanups: ks.cleanup_history.clone(),
    };
    let mut kernel = Kernel::new(nethacked_kernel::Limits::default()).unwrap();
    let mapped = kernel.restore_kernel(&ks, &host).unwrap();
    let service = c
        .validate(&ks, &catalog, &Approved(c.clone()), &TimerLimits::default())
        .unwrap()
        .rebuild(&kernel, &mapped, &catalog)
        .unwrap();
    let state = service.state().unwrap();
    assert!(state.queue > c.namespace_high_water);
    assert_eq!(state.tasks.len(), 1);
    let current = *state.tasks.keys().next().unwrap();
    assert_eq!(current.serial, 1);
    let actor_id = ks
        .registry
        .modules
        .iter()
        .find(|m| m.manifest.id == "actor")
        .unwrap()
        .id;
    let actor = mapped.modules()[&actor_id];
    let mut f = Fixture {
        kernel,
        service,
        actor,
        materials: host.materials,
        busy,
        effects,
        cleanup_calls,
        hook: Rc::new(RefCell::new(None)),
        source_cleanup: CleanupIdentity::issue().unwrap(),
    };
    drop(mapped);
    for old in &held {
        assert_ne!(current.queue, old.queue);
        assert_eq!(old.serial, current.serial);
        assert_eq!(
            support::abi(&fresh_request(&mut f, Request::Cancel { task: *old }).unwrap()),
            Error::InvalidHandle.abi_code()
        );
        assert_eq!(f.service.state().unwrap(), state);
    }
    assert_eq!(f.effects.get(), 0);
    // Prove the same current authority can cancel a fresh ID. Keep serial 1
    // pending so every earlier held ID collides in serial at every generation.
    let cancellable = f.schedule(2);
    let reply = fresh_request(&mut f, Request::Cancel { task: cancellable }).unwrap();
    assert_eq!(
        serde_json::from_slice::<TaskId>(&reply).unwrap(),
        cancellable
    );
    assert_eq!(
        f.service.state().unwrap().tasks[&cancellable].status,
        nethacked_timers::Status::Cancelled
    );
    f.service.dispose(&mut f.kernel, cancellable).unwrap();
    assert_eq!(
        f.service.state().unwrap(),
        nethacked_timers::State {
            next_serial: cancellable.serial + 1,
            ..state
        }
    );
    let mut ordinary = Fixture::new();
    let ordinary_id = ordinary.schedule(1);
    assert!(ordinary_id.queue > current.queue);
    for old in &held {
        assert_ne!(ordinary_id.queue, old.queue);
        let before = ordinary.service.state().unwrap();
        assert_eq!(
            support::abi(&fresh_request(&mut ordinary, Request::Cancel { task: *old }).unwrap()),
            Error::InvalidHandle.abi_code()
        );
        assert_eq!(ordinary.service.state().unwrap(), before);
    }
    held.extend([current, ordinary_id]);
    drop(ordinary);
    process_save(&dir, &mut f, &held);
    let saved =
        Checkpoint::from_bytes(&std::fs::read(dir.join("timer.json")).unwrap(), MAX).unwrap();
    assert!(saved.namespace_high_water >= ordinary_id.queue);
}

struct StagingTargets(Rc<Cell<bool>>);
impl TargetCodec for StagingTargets {
    fn validate_old(&self, targets: &[nethacked_resources::Stamp]) -> Result<()> {
        assert!(targets.is_empty());
        Ok(())
    }
    fn remap(
        &self,
        targets: &[nethacked_resources::Stamp],
        _: &RestoredKernel,
    ) -> Result<Vec<nethacked_resources::Stamp>> {
        assert!(targets.is_empty());
        if self.0.get() {
            Err(Error::NativeRefusal)
        } else {
            Ok(vec![])
        }
    }
}

#[test]
fn fresh_process_staging_and_exhaustion() {
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "fresh_process_staging_worker",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
#[ignore = "isolates namespace exhaustion and exact allocation accounting"]
fn fresh_process_staging_worker() {
    let mut f = Fixture::new();
    f.schedule(1);
    let (ks, c) = f.save();
    assert_eq!(c.namespace_high_water, 1);
    let fail = Rc::new(Cell::new(true));
    let target = Rc::new(EmptyTargets {
        checks: Rc::new(Cell::new(0)),
    });
    let mut catalog = RestoreCatalog::new(
        id("clock"),
        Rc::new(Clock::default()),
        id("empty"),
        target,
        Rc::new(StagingTargets(fail.clone())),
    )
    .unwrap();
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    catalog
        .participant(id("cleanup"), move |identity| {
            count.set(count.get() + 1);
            Ok(Rc::new(Cleanup {
                identity,
                busy: Rc::new(Cell::new(false)),
                calls: Rc::new(Cell::new(0)),
            }))
        })
        .unwrap();
    let host = Admission {
        materials: f.materials.clone(),
        contexts: ks.operation_contexts().unwrap(),
        timer: catalog.native_port(),
        effects: Rc::new(Cell::new(0)),
        deny: false,
        grants: ks.registry.grants.clone(),
        cleanups: ks.cleanup_history.clone(),
    };
    drop(f);
    let mut kernel = Kernel::new(nethacked_kernel::Limits::default()).unwrap();
    let mapped = kernel.restore_kernel(&wire(&ks), &host).unwrap();
    for _ in 0..2 {
        let stage = c
            .validate(&ks, &catalog, &Approved(c.clone()), &TimerLimits::default())
            .unwrap();
        assert!(matches!(
            stage.rebuild(&kernel, &mapped, &catalog),
            Err(Error::NativeRefusal)
        ));
        assert_eq!(calls.get(), 0);
        let actor_id = ks
            .registry
            .modules
            .iter()
            .find(|m| m.manifest.id == "actor")
            .unwrap()
            .id;
        let (op, lease) = kernel
            .capture_operation(mapped.modules()[&actor_id], "run", all())
            .unwrap();
        let binding = kernel
            .operation_binding(op, lease, 0, "unpublished-port")
            .unwrap();
        let reply = kernel
            .invoke_operation(
                &binding,
                &serde_json::to_vec(&Request::Cancel {
                    task: TaskId {
                        queue: c.namespace,
                        serial: 1,
                    },
                })
                .unwrap(),
                all(),
            )
            .unwrap();
        assert_eq!(support::abi(&reply), Error::NativeRefusal.abi_code());
        kernel.release_pipeline(lease).unwrap();
        // No factory or publication follows the target-remap refusal; the next
        // build must receive a later namespace and the one-use claim is reusable.
    }
    fail.set(false);
    let service = c
        .validate(&ks, &catalog, &Approved(c.clone()), &TimerLimits::default())
        .unwrap()
        .rebuild(&kernel, &mapped, &catalog)
        .unwrap();
    assert_eq!(service.state().unwrap().queue, 4);
    assert_eq!(calls.get(), 1);
    assert!(c
        .validate(&ks, &catalog, &Approved(c.clone()), &TimerLimits::default())
        .unwrap()
        .rebuild(&kernel, &mapped, &catalog)
        .is_err());
    let ordinary = Service::new(
        TimerLimits::default(),
        queue_key(),
        Rc::new(Clock::default()),
        Rc::new(EmptyTargets {
            checks: Rc::new(Cell::new(0)),
        }),
        true,
        vec![],
    )
    .unwrap();
    assert_eq!(ordinary.state().unwrap().queue, 5);
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
    let mut capture = CaptureCatalog::new(&service, id("clock"), id("empty"), None).unwrap();
    for p in service.persistence_participants().unwrap() {
        capture.participant(id("cleanup"), p).unwrap();
    }
    assert_eq!(
        service
            .capture_local(&index, &capture)
            .unwrap()
            .namespace_high_water,
        5
    );
    // Malformed or old-format histories refuse before any target semantic call.
    for (version, high) in [(1, c.namespace_high_water), (2, 0), (2, u64::MAX)] {
        let mut bad = c.clone();
        bad.version = version;
        bad.namespace_high_water = high;
        assert!(bad
            .validate(
                &ks,
                &catalog,
                &Approved(bad.clone()),
                &TimerLimits::default()
            )
            .is_err());
    }
    let mut missing = serde_json::to_value(&c).unwrap();
    missing
        .as_object_mut()
        .unwrap()
        .remove("namespace_high_water");
    assert!(Checkpoint::from_bytes(&serde_json::to_vec(&missing).unwrap(), MAX).is_err());
    // Restore into another destination with an exhausted but valid history.
    let catalog = restore_catalog(
        Rc::new(Cell::new(0)),
        Rc::new(Cell::new(false)),
        Rc::new(Cell::new(0)),
    );
    let host = Admission {
        timer: catalog.native_port(),
        ..host
    };
    let mut kernel = Kernel::new(nethacked_kernel::Limits::default()).unwrap();
    let mapped = kernel.restore_kernel(&ks, &host).unwrap();
    let mut exhausted = c.clone();
    exhausted.namespace_high_water = u64::MAX - 1;
    for _ in 0..2 {
        assert!(matches!(
            exhausted
                .validate(
                    &ks,
                    &catalog,
                    &Approved(exhausted.clone()),
                    &TimerLimits::default()
                )
                .unwrap()
                .rebuild(&kernel, &mapped, &catalog),
            Err(Error::Limit)
        ));
    }
    assert!(matches!(
        Service::new(
            TimerLimits::default(),
            queue_key(),
            Rc::new(Clock::default()),
            Rc::new(EmptyTargets {
                checks: Rc::new(Cell::new(0))
            }),
            true,
            vec![]
        ),
        Err(Error::Limit)
    ));
}
