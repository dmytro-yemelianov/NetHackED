#![cfg(feature = "persistence")]
mod persistence_support;
use nethacked_kernel::{persistence::*, *};
use nethacked_resources::persistence::*;
use nethacked_workflows::{persistence as wp, *};
use persistence_support::*;
use std::{cell::Cell, collections::BTreeSet, rc::Rc};

fn identity(name: &str) -> wp::Identity {
    wp::Identity {
        name: name.into(),
        version: 1,
        artifact: [7; 32],
    }
}
struct Opaque;
impl wp::StateCodec for Opaque {
    fn identity(&self) -> wp::Identity {
        identity("opaque")
    }
    fn validate(
        &self,
        state: &[u8],
        limit: usize,
    ) -> nethacked_workflows::Result<Vec<wp::Reference>> {
        if state.len() > limit {
            return Err(nethacked_workflows::Error::Bounds);
        }
        Ok(vec![])
    }
    fn remap(
        &self,
        state: &[u8],
        _: &wp::LocalRemap<'_>,
        limit: usize,
    ) -> nethacked_workflows::Result<Vec<u8>> {
        self.validate(state, limit)?;
        Ok(state.to_vec())
    }
}
struct ApprovedWorkflow(wp::Checkpoint);
impl wp::FrameAuthority for ApprovedWorkflow {
    fn approve(
        &self,
        namespace: u64,
        frame: &wp::FrameRow,
        participants: &[wp::ParticipantRow],
    ) -> nethacked_workflows::Result<()> {
        if namespace == self.0.namespace
            && self.0.frames.contains(frame)
            && participants == self.0.participants
        {
            Ok(())
        } else {
            Err(nethacked_workflows::Error::Status)
        }
    }
}
struct BusyService {
    service: nethacked_resources::Service,
    busy: Cell<bool>,
    calls: Cell<usize>,
}
impl CleanupParticipant for BusyService {
    fn identity(&self) -> nethacked_kernel::Result<Option<CleanupIdentity>> {
        self.service.cleanup_identity().map(Some)
    }
    fn discard(&self, binding: &OperationBinding) -> nethacked_kernel::Result<()> {
        self.calls.set(self.calls.get() + 1);
        if self.busy.get() {
            return Err(nethacked_kernel::Error::NativeRefusal);
        }
        self.service.discard_operation(binding)
    }
}
fn escaped(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("\\{b:02x}")).collect()
}
// Real guest calls prepare twice, commits the first, then yields. The second
// plan is legally conflicted and cleanup alone may discard it.
fn workflow_guest(plan: &nethacked_resources::Plan) -> Vec<u8> {
    let bytes = serde_json::to_vec(plan).unwrap();
    let outcome = serde_json::to_vec(&Outcome::Suspend {
        next: "paused".into(),
        prompt: b"wait".to_vec(),
        state: b"accepted".to_vec(),
        events: vec![b"phase".to_vec()],
    })
    .unwrap();
    let finish = serde_json::to_vec(&Outcome::Finish {
        result: b"resumed".to_vec(),
        state: b"done".to_vec(),
        events: vec![b"resumed".to_vec()],
    })
    .unwrap();
    let phase_offset = br#"{"sequence":1,"phase":""#.len();
    let request = StepRequest {
        sequence: 1,
        phase: "paused".into(),
        state: b"accepted".to_vec(),
        input: vec![],
    };
    let encoded = serde_json::to_vec(&request).unwrap();
    assert_eq!(encoded[phase_offset], b'p');
    wat::parse_str(format!(r#"(module
      (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 2)
      (data (i32.const 1000) "{}") (data (i32.const 48000) "{}")
      (data (i32.const 50000) "{}")
      (func (export "alloc") (param i32) (result i32) i32.const 0)
      (func (export "step") (param $p i32) (param $n i32) (result i64) (local $first i32) (local $r i32)
        local.get $p i32.load8_u i32.const 123 i32.eq
        if (result i64)
          local.get $p i32.const {phase_offset} i32.add i32.load8_u i32.const 112 i32.eq
          if (result i64)
            i64.const {} i64.const {} i64.or
          else
          i32.const 0 i32.const 1000 i32.const {} i32.const 32768 i32.const {WIRE} call $call local.set $first
          i32.const 0 i32.const 1000 i32.const {} i32.const 40000 i32.const {WIRE} call $call drop
          i32.const 1 i32.const 32768 local.get $first i32.const 60000 i32.const {WIRE} call $call drop
          i64.const {} i64.const {} i64.or
          end
        else
          local.get $p i32.load8_u i32.const 67 i32.eq
          local.get $p i32.const 1 i32.add local.get $n i32.const 1 i32.sub i32.const 32768 i32.const {WIRE} call $call local.set $r
          local.get $r i32.const 0 i32.lt_s if i32.const 32768 local.get $r i32.store i32.const 4 local.set $r end
          i64.const 140737488355328 local.get $r i64.extend_i32_u i64.or
        end))"#, escaped(&bytes), escaped(&outcome), escaped(&finish), (50000u64 << 32), finish.len(), bytes.len(), bytes.len(), (48000u64 << 32), outcome.len())).unwrap()
}

fn resource_checkpoint(
    kernel: &mut Kernel,
    restored: &Restored,
    effects: &Rc<Effects>,
) -> Checkpoint {
    let service = restored.service();
    let mut capture = CaptureCatalog::new(service);
    for owner in service
        .state()
        .unwrap()
        .resources
        .keys()
        .map(|r| r.owner.clone())
        .collect::<BTreeSet<_>>()
    {
        capture.owner(&owner, codec(Role::Owner, effects)).unwrap();
    }
    capture
        .accounting(codec(Role::Accounting, effects))
        .unwrap();
    let ops = service.persistence_enrollment(kernel).unwrap();
    let (cleanup, roots) = service.persistence_roots().unwrap();
    let k = kernel
        .kernel_snapshot_with_resource_roots(
            &KernelEnrollment {
                operations: &ops,
                pure_captures: &[],
            },
            &[cleanup],
            &roots,
            &service.persistence_history().unwrap(),
        )
        .unwrap();
    service
        .capture_local(&kernel.execution_capture_index(&k).unwrap(), &k, &capture)
        .unwrap()
}

#[test]
fn paused_and_terminal_busy_use_same_reconstructed_service_and_exact_cleanup() {
    for (busy, abandon) in [(false, false), (false, true), (true, false)] {
        let mut f = Fixture::new();
        let m = f.kernel.manifest(f.actor).unwrap().clone();
        f.kernel.unregister(f.actor).unwrap();
        let bytes = workflow_guest(&f.plan());
        f.actor = f.kernel.register_guest(m.clone(), &bytes).unwrap();
        f.materials.insert(
            f.kernel.artifact(f.actor).unwrap(),
            Material::Guest(m, bytes),
        );
        for slot in 0..3 {
            f.kernel.issue_grant(f.actor, slot, f.auth()).unwrap();
        }
        let unrelated = f.binding();
        let unrelated_id = f.prepare(&f.plan(), Some(&unrelated));
        let unbound_id = f.prepare(&f.plan(), None);
        let workflows = Workflows::new(Default::default()).unwrap();
        let participant = Rc::new(BusyService {
            service: f.service.clone(),
            busy: Cell::new(busy),
            calls: Cell::new(0),
        });
        let program = Program {
            provider: f.actor,
            endpoint: "step".into(),
            authority: f.auth(),
            initial: "charge".into(),
            phases: BTreeSet::from(["charge".into(), "paused".into()]),
            allowed: BTreeSet::from([("charge".into(), "paused".into())]),
        };
        let id = workflows
            .start(&mut f.kernel, program, vec![], vec![participant.clone()])
            .unwrap();
        let status = workflows.step(&f.kernel, id, &[]).unwrap();
        if busy {
            assert_eq!(status, Status::Faulted(Fault::Cleanup));
        } else {
            assert!(matches!(status, Status::Suspended(_)));
        }
        let prefix = f.service.state().unwrap();
        assert_eq!(prefix.commits, 1);
        assert_eq!(prefix.history, [b"one".to_vec(), b"two".to_vec()]);
        let mut wcapture = wp::CaptureCatalog::new(&workflows);
        wcapture
            .program(id, identity("program"), Rc::new(Opaque))
            .unwrap();
        wcapture
            .participant(identity("resource-cleanup"), participant.clone())
            .unwrap();
        let mut operations = workflows.persistence_enrollment(&f.kernel).unwrap();
        for op in f.service.persistence_enrollment(&f.kernel).unwrap() {
            if !operations.contains(&op) {
                operations.push(op);
            }
        }
        let (cleanup, roots) = f.service.persistence_roots().unwrap();
        let k = f
            .kernel
            .kernel_snapshot_with_resource_roots(
                &KernelEnrollment {
                    operations: &operations,
                    pure_captures: &[],
                },
                &[cleanup],
                &roots,
                &f.service.persistence_history().unwrap(),
            )
            .unwrap();
        let index = f.kernel.execution_capture_index(&k).unwrap();
        let c = f.service.capture_local(&index, &k, &f.capture).unwrap();
        assert_eq!(c.pending.len(), if busy { 3 } else { 2 });
        let w = workflows
            .capture_local(&f.kernel, &index, &wcapture)
            .unwrap();
        let k: KernelSnapshot = serde_json::from_slice(&serde_json::to_vec(&k).unwrap()).unwrap();
        let c = Checkpoint::from_bytes(&c.to_bytes(1 << 20).unwrap(), 1 << 20).unwrap();
        let w = wp::Checkpoint::from_bytes(&w.to_bytes(1 << 20).unwrap(), 1 << 20).unwrap();
        let materials = f.materials.clone();
        drop(wcapture);
        drop(participant);
        drop(workflows);
        drop(f);
        let effects = Rc::new(Effects::default());
        let approved = Approved::new(&k, &c);
        let staged = c
            .clone()
            .validate(&limits(), &k, catalog(&effects), &approved)
            .unwrap()
            .stage()
            .unwrap();
        let host = Host::fresh(materials, &k, staged.ports());
        let mut kernel = fresh_kernel();
        let mapped = kernel.restore_kernel(&k, &host).unwrap();
        let restored = staged.finish(&kernel, &mapped).unwrap();
        let shared = Rc::new(BusyService {
            service: restored.service().clone(),
            busy: Cell::new(false),
            calls: Cell::new(0),
        });
        let mut wcatalog = wp::RestoreCatalog::default();
        wcatalog
            .program(w.frames[0].program.clone(), Rc::new(Opaque))
            .unwrap();
        let exact = shared.clone();
        wcatalog
            .participant(identity("resource-cleanup"), move |identity| {
                if identity != exact.identity().unwrap() {
                    return Err(nethacked_workflows::Error::Status);
                }
                Ok(exact.clone())
            })
            .unwrap();
        let validated = w
            .validate(
                &k,
                Default::default(),
                &wcatalog,
                &ApprovedWorkflow(w.clone()),
            )
            .unwrap();
        let rebuilt = validated.rebuild(&kernel, &mapped, &wcatalog).unwrap();
        let fresh_id = mapped
            .execution_source(&w.frames[0].source)
            .unwrap()
            .operation();
        assert_eq!(effects.predicates.get(), 0);
        assert_eq!(shared.calls.get(), 0); // load executes no cleanup
        let before = restored.service().state().unwrap();
        assert_eq!(
            resource_checkpoint(&mut kernel, &restored, &effects)
                .pending
                .len(),
            c.pending.len()
        );
        assert_eq!(before.history, prefix.history);
        assert_eq!(before.next_serial, prefix.next_serial);
        if busy {
            assert!(rebuilt.dispose(&mut kernel, fresh_id).is_err());
            rebuilt.retry_cleanup(fresh_id).unwrap();
            rebuilt.retry_cleanup(fresh_id).unwrap();
            assert_eq!(shared.calls.get(), 1);
        } else {
            assert!(matches!(
                rebuilt.inspect(fresh_id).unwrap().status,
                Status::Suspended(_)
            ));
            if abandon {
                rebuilt.abandon(fresh_id).unwrap();
            } else {
                let ticket = rebuilt.ticket(fresh_id).unwrap();
                rebuilt.resume(&ticket).unwrap();
                assert_eq!(
                    rebuilt.step(&kernel, fresh_id, &[]).unwrap(),
                    Status::Finished(b"resumed".to_vec())
                );
                let frame = rebuilt.inspect(fresh_id).unwrap();
                assert_eq!(frame.sequence, 3); // resume consumes its ticket epoch
                assert_eq!(frame.trace, [b"phase".to_vec(), b"resumed".to_vec()]);
            }
        }
        rebuilt.dispose(&mut kernel, fresh_id).unwrap();
        let after = restored.service().state().unwrap();
        let after_checkpoint = resource_checkpoint(&mut kernel, &restored, &effects);
        assert_eq!(after_checkpoint.pending.len(), 2);
        assert!(after_checkpoint
            .pending
            .iter()
            .any(|p| p.serial == unrelated_id.serial));
        assert!(after_checkpoint
            .pending
            .iter()
            .any(|p| p.serial == unbound_id.serial));
        assert_eq!(after.resources, before.resources);
        assert_eq!(after.history, before.history);
        assert_eq!(after.commits, before.commits);
        assert_eq!(after.next_serial, before.next_serial);
        assert_eq!(after.next_generation, before.next_generation);
        assert_eq!(effects.predicates.get(), 0);
        // The shared participant changed the service behind the installed ports.
        assert!(restored.matches_destination(&mapped.execution_destination()));
    }
}

#[test]
fn actual_resource_borrow_busy_is_terminal_and_retries_only_shared_service_discard() {
    let mut f = Fixture::new();
    let accepted = f.prepare(&f.plan(), None);
    abi(f
        .kernel
        .invoke(f.actor, "step", &command(b'C', &accepted), f.auth())
        .unwrap())
    .unwrap();
    let m = f.kernel.manifest(f.actor).unwrap().clone();
    f.kernel.unregister(f.actor).unwrap();
    let bytes = workflow_guest(&f.plan());
    f.actor = f.kernel.register_guest(m.clone(), &bytes).unwrap();
    f.materials.insert(
        f.kernel.artifact(f.actor).unwrap(),
        Material::Guest(m, bytes),
    );
    for slot in 0..3 {
        f.kernel.issue_grant(f.actor, slot, f.auth()).unwrap();
    }
    let workflows = Workflows::new(Default::default()).unwrap();
    let shared: Rc<dyn CleanupParticipant> = Rc::new(f.service.clone());
    let program = Program {
        provider: f.actor,
        endpoint: "step".into(),
        authority: f.auth(),
        initial: "paused".into(),
        phases: BTreeSet::from(["paused".into()]),
        allowed: BTreeSet::new(),
    };
    let id = workflows
        .start(&mut f.kernel, program, vec![], vec![shared.clone()])
        .unwrap();
    let binding = workflows.persistence_enrollment(&f.kernel).unwrap()[0].clone();
    let plan = f.plan();
    let authority = f.auth();
    let kernel = Rc::new(f.kernel);
    let nested_kernel = kernel.clone();
    let nested_workflows = workflows.clone();
    let once = Rc::new(Cell::new(false));
    let invoked = once.clone();
    *f.effects.callback.borrow_mut() = Some(Box::new(move || {
        if !invoked.replace(true) {
            // The outer real prepare holds the service borrow during its policy.
            // The nested phase is pure Finish; the actual Service participant's
            // discard then refuses its borrowed state, rather than a fake wrapper.
            assert_eq!(
                nested_workflows.step(&nested_kernel, id, &[]).unwrap(),
                Status::Faulted(Fault::Cleanup)
            );
        }
    }));
    let reply = kernel
        .invoke_operation(&binding, &command(b'P', &plan), authority)
        .unwrap();
    let pending: nethacked_resources::Identity =
        serde_json::from_slice(&abi(reply).unwrap()).unwrap();
    *f.effects.callback.borrow_mut() = None;
    assert!(once.get());
    let prefix = f.service.state().unwrap();
    assert_eq!(prefix.commits, 1);
    assert_eq!(prefix.history, [b"one".to_vec(), b"two".to_vec()]);
    assert!(workflows.inspect(id).unwrap().cleanup_pending);
    let mut kernel = Rc::try_unwrap(kernel).ok().unwrap();
    let mut capture = wp::CaptureCatalog::new(&workflows);
    capture
        .program(id, identity("busy-program"), Rc::new(Opaque))
        .unwrap();
    capture
        .participant(identity("actual-service"), shared.clone())
        .unwrap();
    let operations = workflows.persistence_enrollment(&kernel).unwrap();
    let (cleanup, roots) = f.service.persistence_roots().unwrap();
    let k = kernel
        .kernel_snapshot_with_resource_roots(
            &KernelEnrollment {
                operations: &operations,
                pure_captures: &[],
            },
            &[cleanup],
            &roots,
            &f.service.persistence_history().unwrap(),
        )
        .unwrap();
    let index = kernel.execution_capture_index(&k).unwrap();
    let c = f.service.capture_local(&index, &k, &f.capture).unwrap();
    assert_eq!(c.pending.len(), 1);
    assert_eq!(c.pending[0].serial, pending.serial);
    let w = workflows.capture_local(&kernel, &index, &capture).unwrap();
    let k: KernelSnapshot = serde_json::from_slice(&serde_json::to_vec(&k).unwrap()).unwrap();
    let c = Checkpoint::from_bytes(&c.to_bytes(1 << 20).unwrap(), 1 << 20).unwrap();
    let w = wp::Checkpoint::from_bytes(&w.to_bytes(1 << 20).unwrap(), 1 << 20).unwrap();
    let materials = f.materials;
    drop(capture);
    drop(workflows);
    drop(shared);
    drop(kernel);
    drop(f.capture);
    drop(f.service);
    drop(f.effects);
    let effects = Rc::new(Effects::default());
    let approved = Approved::new(&k, &c);
    let stage = c
        .clone()
        .validate(&limits(), &k, catalog(&effects), &approved)
        .unwrap()
        .stage()
        .unwrap();
    let host = Host::fresh(materials, &k, stage.ports());
    let mut kernel = fresh_kernel();
    let mapped = kernel.restore_kernel(&k, &host).unwrap();
    let restored = stage.finish(&kernel, &mapped).unwrap();
    let exact = restored.service().clone();
    let mut catalog = wp::RestoreCatalog::default();
    catalog
        .program(w.frames[0].program.clone(), Rc::new(Opaque))
        .unwrap();
    catalog
        .participant(identity("actual-service"), move |cleanup| {
            if cleanup != Some(exact.cleanup_identity().unwrap()) {
                return Err(nethacked_workflows::Error::Status);
            }
            Ok(Rc::new(exact.clone()))
        })
        .unwrap();
    let validated = w
        .validate(
            &k,
            Default::default(),
            &catalog,
            &ApprovedWorkflow(w.clone()),
        )
        .unwrap();
    let rebuilt = validated.rebuild(&kernel, &mapped, &catalog).unwrap();
    let fresh = mapped
        .execution_source(&w.frames[0].source)
        .unwrap()
        .operation();
    let before = restored.service().state().unwrap();
    assert_eq!(
        resource_checkpoint(&mut kernel, &restored, &effects)
            .pending
            .len(),
        1
    );
    assert_eq!(before.history, prefix.history);
    assert_eq!(effects.predicates.get(), 0);
    assert!(rebuilt.step(&kernel, fresh, &[]).is_err());
    assert!(rebuilt.dispose(&mut kernel, fresh).is_err());
    rebuilt.retry_cleanup(fresh).unwrap();
    rebuilt.retry_cleanup(fresh).unwrap();
    rebuilt.dispose(&mut kernel, fresh).unwrap();
    assert!(resource_checkpoint(&mut kernel, &restored, &effects)
        .pending
        .is_empty());
    let after = restored.service().state().unwrap();
    assert_eq!(after.resources, before.resources);
    assert_eq!(after.history, before.history);
    assert_eq!(after.commits, before.commits);
    assert_eq!(after.next_serial, before.next_serial);
    assert_eq!(after.next_generation, before.next_generation);
    assert_eq!(effects.predicates.get(), 0);
}
