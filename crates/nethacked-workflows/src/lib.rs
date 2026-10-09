//! Authoritative in-memory workflows; module state is opaque local scratch.
#![forbid(unsafe_code)]
use nethacked_kernel::{
    Authority, CleanupIdentity, Error as KernelError, Kernel, ModuleHandle, OperationBinding,
    OperationId, PipelineDescriptor, PipelineLease,
};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
#[cfg(feature = "persistence")]
pub mod persistence;

#[derive(Clone, Debug)]
pub struct Limits {
    pub operations: usize,
    pub phases: usize,
    pub edges: usize,
    pub participants: usize,
    pub phase_bytes: usize,
    pub state_bytes: usize,
    pub input_bytes: usize,
    pub value_bytes: usize,
    pub events: usize,
    pub event_bytes: usize,
    pub trace_events: usize,
    pub wire_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            operations: 32,
            phases: 32,
            edges: 128,
            participants: 8,
            phase_bytes: 128,
            state_bytes: 1024,
            input_bytes: 1024,
            value_bytes: 1024,
            events: 16,
            event_bytes: 256,
            trace_events: 256,
            wire_bytes: 16384,
        }
    }
}
/// Trusted finite module program, frozen together with actual code resolution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Program {
    pub provider: ModuleHandle,
    pub endpoint: String,
    pub authority: Authority,
    pub initial: String,
    pub phases: BTreeSet<String>,
    pub allowed: BTreeSet<(String, String)>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Outcome {
    Advance {
        next: String,
        state: Vec<u8>,
        events: Vec<Vec<u8>>,
    },
    Suspend {
        next: String,
        prompt: Vec<u8>,
        state: Vec<u8>,
        events: Vec<Vec<u8>>,
    },
    Finish {
        result: Vec<u8>,
        state: Vec<u8>,
        events: Vec<Vec<u8>>,
    },
    Reject {
        events: Vec<Vec<u8>>,
    },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepRequest {
    pub sequence: u64,
    pub phase: String,
    pub state: Vec<u8>,
    pub input: Vec<u8>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Fault {
    #[serde(with = "fault_error")]
    Invocation(KernelError),
    InvalidOutcome,
    Cleanup,
    Abandoned,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Status {
    Running,
    Suspended(Vec<u8>),
    Finished(Vec<u8>),
    Rejected,
    Faulted(Fault),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Frame {
    pub operation: OperationId,
    pub sequence: u64,
    pub pipeline: PipelineDescriptor,
    pub program: Program,
    pub state: Vec<u8>,
    pub phase: String,
    pub trace: Vec<Vec<u8>>,
    pub status: Status,
    pub cleanup_pending: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ticket {
    pub operation: OperationId,
    pub sequence: u64,
    pub pipeline: PipelineLease,
    pub code: [u8; 32],
    pub phase: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    Unknown,
    Status,
    Ticket,
    Bounds,
    Counter,
    Busy,
    Cleanup,
    Kernel(KernelError),
}
pub type Result<T> = std::result::Result<T, Error>;
impl From<KernelError> for Error {
    fn from(e: KernelError) -> Self {
        Self::Kernel(e)
    }
}
/// Explicit trusted bounded participant; cannot invoke a mutable Kernel/World.
/// Retrying an already completed discard must be safe (idempotent).
pub trait CleanupParticipant {
    fn identity(&self) -> nethacked_kernel::Result<Option<CleanupIdentity>> {
        Ok(None)
    }
    fn discard(&self, binding: &OperationBinding) -> nethacked_kernel::Result<()>;
}
impl CleanupParticipant for nethacked_resources::Service {
    fn identity(&self) -> nethacked_kernel::Result<Option<CleanupIdentity>> {
        self.cleanup_identity().map(Some)
    }
    fn discard(&self, binding: &OperationBinding) -> nethacked_kernel::Result<()> {
        self.discard_operation(binding)
    }
}
struct Owned {
    frame: Frame,
    lease: PipelineLease,
    participants: Vec<Rc<dyn CleanupParticipant>>,
    identities: Vec<CleanupIdentity>,
    cleanup: Option<OperationBinding>,
    cleaned: usize,
    skip_cleaned: bool,
}
struct Inner {
    limits: Limits,
    frames: BTreeMap<OperationId, Owned>,
}
/// Shared handles enable callbacks to attempt reentrancy; try_borrow refuses it.
#[derive(Clone)]
pub struct Workflows(Rc<RefCell<Inner>>);
impl Workflows {
    pub fn new(limits: Limits) -> Result<Self> {
        if [
            limits.operations,
            limits.phases,
            limits.edges,
            limits.participants,
            limits.phase_bytes,
            limits.state_bytes,
            limits.input_bytes,
            limits.value_bytes,
            limits.events,
            limits.event_bytes,
            limits.trace_events,
            limits.wire_bytes,
        ]
        .contains(&0)
        {
            return Err(Error::Bounds);
        }
        limits
            .trace_events
            .checked_mul(limits.event_bytes)
            .ok_or(Error::Bounds)?;
        Ok(Self(Rc::new(RefCell::new(Inner {
            limits,
            frames: BTreeMap::new(),
        }))))
    }
    pub fn start(
        &self,
        kernel: &mut Kernel,
        program: Program,
        state: Vec<u8>,
        participants: Vec<Rc<dyn CleanupParticipant>>,
    ) -> Result<OperationId> {
        let mut inner = self.0.try_borrow_mut().map_err(|_| Error::Busy)?;
        let l = &inner.limits;
        if inner.frames.len() >= l.operations
            || state.len() > l.state_bytes
            || participants.len() > l.participants
            || program.phases.len() > l.phases
            || program.allowed.len() > l.edges
            || !program.phases.contains(&program.initial)
            || program
                .phases
                .iter()
                .any(|p| p.is_empty() || p.len() > l.phase_bytes)
            || program
                .allowed
                .iter()
                .any(|(a, b)| !program.phases.contains(a) || !program.phases.contains(b))
        {
            return Err(Error::Bounds);
        }
        // Request encoding admission happens before acquiring any kernel references.
        encode_request(l, 0, &program.initial, &state, &[])?;
        let identities = participants
            .iter()
            .map(|p| p.identity())
            .collect::<nethacked_kernel::Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        if identities.len() > kernel.operation_cleanup_limit() {
            return Err(Error::Bounds);
        }
        let (operation, lease) = kernel.capture_operation(
            program.provider,
            &program.endpoint,
            program.authority.clone(),
        )?;
        let pipeline = kernel.pipeline_descriptor(lease)?.clone();
        let phase = program.initial.clone();
        // Enroll actual cleanup identities at admission, rather than changing
        // kernel identity history when a safe-boundary checkpoint is captured.
        if let Err(e) = kernel.enroll_operation_cleanup(operation, lease, &identities) {
            kernel.release_pipeline(lease)?;
            return Err(e.into());
        }
        inner.frames.insert(
            operation,
            Owned {
                frame: Frame {
                    operation,
                    sequence: 0,
                    pipeline,
                    program,
                    state,
                    phase,
                    trace: vec![],
                    status: Status::Running,
                    cleanup_pending: false,
                },
                lease,
                participants,
                identities,
                cleanup: None,
                cleaned: 0,
                skip_cleaned: false,
            },
        );
        Ok(operation)
    }
    pub fn inspect(&self, id: OperationId) -> Result<Frame> {
        let inner = self.0.try_borrow().map_err(|_| Error::Busy)?;
        Ok(inner.frames.get(&id).ok_or(Error::Unknown)?.frame.clone())
    }
    pub fn ticket(&self, id: OperationId) -> Result<Ticket> {
        let inner = self.0.try_borrow().map_err(|_| Error::Busy)?;
        let owned = inner.frames.get(&id).ok_or(Error::Unknown)?;
        if !matches!(owned.frame.status, Status::Suspended(_)) {
            return Err(Error::Status);
        }
        Ok(ticket(owned))
    }
    pub fn resume(&self, ticket_value: &Ticket) -> Result<()> {
        let mut inner = self.0.try_borrow_mut().map_err(|_| Error::Busy)?;
        let owned = inner
            .frames
            .get_mut(&ticket_value.operation)
            .ok_or(Error::Unknown)?;
        if !matches!(owned.frame.status, Status::Suspended(_)) {
            return Err(Error::Status);
        }
        if ticket(owned) != *ticket_value {
            return Err(Error::Ticket);
        }
        let next = owned.frame.sequence.checked_add(1).ok_or(Error::Counter)?;
        owned.frame.sequence = next;
        owned.frame.status = Status::Running;
        Ok(())
    }
    pub fn step(&self, kernel: &Kernel, id: OperationId, input: &[u8]) -> Result<Status> {
        let mut inner = self.0.try_borrow_mut().map_err(|_| Error::Busy)?;
        let Inner { limits, frames } = &mut *inner;
        let owned = frames.get_mut(&id).ok_or(Error::Unknown)?;
        if owned.frame.status != Status::Running {
            return Err(Error::Status);
        }
        let next_sequence = owned.frame.sequence.checked_add(1).ok_or(Error::Counter)?;
        if owned
            .frame
            .trace
            .len()
            .checked_add(limits.events)
            .ok_or(Error::Bounds)?
            > limits.trace_events
        {
            return Err(Error::Bounds);
        }
        let request = encode_request(
            limits,
            owned.frame.sequence,
            &owned.frame.phase,
            &owned.frame.state,
            input,
        )?;
        let binding = kernel.operation_binding_with_cleanup(
            id,
            owned.lease,
            owned.frame.sequence,
            &owned.frame.phase,
            &owned.identities,
        )?;
        kernel.preflight_pipeline(owned.lease, &request, &owned.frame.program.authority)?;
        // Reserve bounded publication space BEFORE entering native or guest code.
        owned
            .frame
            .trace
            .try_reserve(limits.events)
            .map_err(|_| Error::Bounds)?;
        let reply =
            kernel.invoke_operation(&binding, &request, owned.frame.program.authority.clone());
        let decoded = match reply {
            Err(e) => Err(Fault::Invocation(e)),
            Ok(bytes) if bytes.len() <= limits.wire_bytes => {
                serde_json::from_slice::<Outcome>(&bytes).map_err(|_| Fault::InvalidOutcome)
            }
            Ok(_) => Err(Fault::InvalidOutcome),
        }
        .and_then(|outcome| {
            validate_outcome(limits, &owned.frame, &outcome)?;
            Ok(outcome)
        });
        owned.cleanup = Some(binding);
        owned.cleaned = 0;
        owned.frame.cleanup_pending = true;
        // Cleanup applies to this invocation epoch, even on trap/invalid result.
        let cleanup = clean(owned);
        if let Err(fault) = decoded {
            owned.frame.status = Status::Faulted(fault);
        } else if cleanup.is_err() {
            owned.frame.status = Status::Faulted(Fault::Cleanup);
        } else if let Ok(outcome) = decoded {
            owned.frame.sequence = next_sequence;
            apply(&mut owned.frame, outcome);
        }
        Ok(owned.frame.status.clone())
    }
    /// Cleanup failure never makes a phase runnable, including after retry.
    pub fn retry_cleanup(&self, id: OperationId) -> Result<()> {
        let mut inner = self.0.try_borrow_mut().map_err(|_| Error::Busy)?;
        let owned = inner.frames.get_mut(&id).ok_or(Error::Unknown)?;
        if !matches!(owned.frame.status, Status::Faulted(_)) {
            return Err(Error::Status);
        }
        clean(owned)
    }
    /// Explicit host retirement for a live operation, including exhausted counters.
    /// No phase is replayed and the last checkpoint/committed prefix is preserved.
    pub fn abandon(&self, id: OperationId) -> Result<()> {
        let mut inner = self.0.try_borrow_mut().map_err(|_| Error::Busy)?;
        let owned = inner.frames.get_mut(&id).ok_or(Error::Unknown)?;
        if !matches!(owned.frame.status, Status::Running | Status::Suspended(_)) {
            return Err(Error::Status);
        }
        owned.frame.status = Status::Faulted(Fault::Abandoned);
        Ok(())
    }
    pub fn dispose(&self, kernel: &mut Kernel, id: OperationId) -> Result<()> {
        let mut inner = self.0.try_borrow_mut().map_err(|_| Error::Busy)?;
        let owned = inner.frames.get_mut(&id).ok_or(Error::Unknown)?;
        if matches!(owned.frame.status, Status::Running | Status::Suspended(_)) {
            return Err(Error::Status);
        }
        if owned.cleanup.is_some() {
            return Err(Error::Cleanup);
        }
        kernel.release_pipeline(owned.lease)?;
        inner.frames.remove(&id);
        Ok(())
    }
}
fn clean(owned: &mut Owned) -> Result<()> {
    if let Some(binding) = &owned.cleanup {
        // Existing live workflows retain their idempotent retry contract.
        // Reconstructed workflows continue the authenticated accepted prefix.
        let first = if owned.skip_cleaned { owned.cleaned } else { 0 };
        for (index, participant) in owned.participants.iter().enumerate().skip(first) {
            participant.discard(binding).map_err(|_| Error::Cleanup)?;
            owned.cleaned = owned.cleaned.max(index + 1);
        }
        owned.cleanup = None;
        owned.frame.cleanup_pending = false;
    }
    Ok(())
}

// Preserve the exact stable kernel refusal without exposing a handle decoder.
mod fault_error {
    use super::KernelError;
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(e: &KernelError, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_i32(e.abi_code())
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> std::result::Result<KernelError, D::Error> {
        use KernelError::*;
        let code = i32::deserialize(d)?;
        [
            InvalidManifest,
            InvalidModule,
            InvalidHandle,
            MissingDependency,
            Duplicate,
            Signature,
            InvalidSlot,
            MissingGrant,
            Revoked,
            Authority,
            Buffer,
            Limit,
            Trap,
            NativeRefusal,
            Referenced,
            Cycle,
        ]
        .into_iter()
        .find(|e| e.abi_code() == code)
        .ok_or_else(|| serde::de::Error::custom("unknown kernel refusal"))
    }
}
fn ticket(o: &Owned) -> Ticket {
    Ticket {
        operation: o.frame.operation,
        sequence: o.frame.sequence,
        pipeline: o.lease,
        code: o.frame.pipeline.hash,
        phase: o.frame.phase.clone(),
    }
}
fn encode_request(
    l: &Limits,
    sequence: u64,
    phase: &str,
    state: &[u8],
    input: &[u8],
) -> Result<Vec<u8>> {
    if input.len() > l.input_bytes {
        return Err(Error::Bounds);
    }
    let request = serde_json::to_vec(&StepRequest {
        sequence,
        phase: phase.into(),
        state: state.into(),
        input: input.into(),
    })
    .map_err(|_| Error::Bounds)?;
    if request.len() > l.wire_bytes {
        return Err(Error::Bounds);
    }
    Ok(request)
}
fn validate_outcome(l: &Limits, f: &Frame, o: &Outcome) -> std::result::Result<(), Fault> {
    let (next, state, value, events) = match o {
        Outcome::Advance {
            next,
            state,
            events,
        } => (Some(next), Some(state), None, events),
        Outcome::Suspend {
            next,
            state,
            prompt,
            events,
        } => (Some(next), Some(state), Some(prompt), events),
        Outcome::Finish {
            state,
            result,
            events,
        } => (None, Some(state), Some(result), events),
        Outcome::Reject { events } => (None, None, None, events),
    };
    if next.is_some_and(|n| !f.program.allowed.contains(&(f.phase.clone(), n.clone())))
        || state.is_some_and(|s| s.len() > l.state_bytes)
        || value.is_some_and(|v| v.len() > l.value_bytes)
        || events.len() > l.events
        || events.iter().any(|e| e.len() > l.event_bytes)
    {
        return Err(Fault::InvalidOutcome);
    }
    Ok(())
}
fn apply(f: &mut Frame, o: Outcome) {
    match o {
        Outcome::Advance {
            next,
            state,
            events,
        } => {
            f.phase = next;
            f.state = state;
            f.trace.extend(events);
        }
        Outcome::Suspend {
            next,
            state,
            prompt,
            events,
        } => {
            f.phase = next;
            f.state = state;
            f.trace.extend(events);
            f.status = Status::Suspended(prompt);
        }
        Outcome::Finish {
            state,
            result,
            events,
        } => {
            f.state = state;
            f.trace.extend(events);
            f.status = Status::Finished(result);
        }
        Outcome::Reject { events } => {
            f.trace.extend(events);
            f.status = Status::Rejected;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nethacked_kernel::{Access, CallerView, Endpoint, Manifest, NativePort, Scope};
    use std::cell::Cell;
    pub(super) fn setup(
        limits: Limits,
        outcome: Outcome,
    ) -> (Kernel, Workflows, OperationId, Rc<Cell<u64>>) {
        let mut k = Kernel::new(nethacked_kernel::Limits::default()).unwrap();
        k.set_retention_limits(nethacked_kernel::RetentionLimits {
            leases: 1,
            ..Default::default()
        })
        .unwrap();
        let calls = Rc::new(Cell::new(0));
        let count = calls.clone();
        let port: NativePort = Rc::new(move |_, _: &CallerView| {
            count.set(count.get() + 1);
            Ok(serde_json::to_vec(&outcome).unwrap())
        });
        let provider = k
            .register_native(
                Manifest {
                    id: "module".into(),
                    exports: BTreeMap::from([(
                        "step".into(),
                        Endpoint {
                            major: 1,
                            access: Access::Write,
                            max_request: 16384,
                            max_reply: 16384,
                        },
                    )]),
                    dependencies: vec![],
                    extension_points: BTreeSet::new(),
                    extension: None,
                },
                b"module",
                BTreeMap::from([("step".into(), port)]),
            )
            .unwrap();
        let w = Workflows::new(limits).unwrap();
        let id = w
            .start(
                &mut k,
                Program {
                    provider,
                    endpoint: "step".into(),
                    authority: Authority {
                        access: Access::Write,
                        scope: Scope::All,
                    },
                    initial: "a".into(),
                    phases: BTreeSet::from(["a".into(), "b".into()]),
                    allowed: BTreeSet::from([("a".into(), "b".into())]),
                },
                vec![],
                vec![],
            )
            .unwrap();
        (k, w, id, calls)
    }
    pub(super) fn done() -> Outcome {
        Outcome::Finish {
            result: vec![],
            state: vec![],
            events: vec![],
        }
    }
    #[test]
    fn overflow_and_trace_capacity_refuse_before_invocation() {
        let (mut k, w, id, calls) = setup(Limits::default(), done());
        w.0.borrow_mut().frames.get_mut(&id).unwrap().frame.sequence = u64::MAX;
        let before = w.inspect(id).unwrap();
        let kernel = k.lifecycle_state();
        assert_eq!(w.step(&k, id, &[]), Err(Error::Counter));
        assert_eq!(calls.get(), 0);
        assert_eq!(w.inspect(id).unwrap(), before);
        assert_eq!(k.lifecycle_state(), kernel);
        w.0.borrow_mut().frames.get_mut(&id).unwrap().frame.status = Status::Suspended(vec![]);
        let t = w.ticket(id).unwrap();
        let before = w.inspect(id).unwrap();
        assert_eq!(w.resume(&t), Err(Error::Counter));
        assert_eq!(w.inspect(id).unwrap(), before);
        w.abandon(id).unwrap();
        assert_eq!(w.step(&k, id, &[]), Err(Error::Status));
        assert_eq!(w.inspect(id).unwrap().sequence, u64::MAX);
        w.dispose(&mut k, id).unwrap();
        assert_eq!(k.lifecycle_state().leases, 0);
        let (k, w, id, calls) = setup(
            Limits {
                events: 2,
                trace_events: 1,
                ..Limits::default()
            },
            done(),
        );
        let before = w.inspect(id).unwrap();
        assert_eq!(w.step(&k, id, &[]), Err(Error::Bounds));
        assert_eq!(calls.get(), 0);
        assert_eq!(w.inspect(id).unwrap(), before);
    }
    #[test]
    fn post_invocation_bounds_fault_without_checkpoint_publication() {
        let l = Limits {
            state_bytes: 1,
            value_bytes: 1,
            events: 1,
            event_bytes: 1,
            wire_bytes: 1024,
            ..Limits::default()
        };
        for o in [
            Outcome::Finish {
                result: vec![0; 2],
                state: vec![],
                events: vec![],
            },
            Outcome::Finish {
                result: vec![],
                state: vec![0; 2],
                events: vec![],
            },
            Outcome::Reject {
                events: vec![vec![], vec![]],
            },
            Outcome::Reject {
                events: vec![vec![0; 2]],
            },
            Outcome::Finish {
                result: vec![0; 2000],
                state: vec![],
                events: vec![],
            },
        ] {
            let (k, w, id, calls) = setup(l.clone(), o);
            let before = w.inspect(id).unwrap();
            assert_eq!(
                w.step(&k, id, &[]),
                Ok(Status::Faulted(Fault::InvalidOutcome))
            );
            assert_eq!(calls.get(), 1);
            let after = w.inspect(id).unwrap();
            assert_eq!(after.state, before.state);
            assert_eq!(after.trace, before.trace);
            assert_eq!(after.phase, before.phase);
            assert_eq!(after.sequence, before.sequence);
            assert_eq!(w.step(&k, id, &[]), Err(Error::Status));
            assert_eq!(calls.get(), 1);
        }
        let (k, w, id, calls) = setup(l, done());
        assert_eq!(w.step(&k, id, &[]), Ok(Status::Finished(vec![])));
        assert_eq!(calls.get(), 1);
    }
    #[test]
    fn encoding_preflight_and_release_failure_preserve_frames() {
        let (mut k, w, id, calls) = setup(Limits::default(), done());
        w.0.borrow_mut().limits.wire_bytes = 1;
        let before = w.inspect(id).unwrap();
        let life = k.lifecycle_state();
        assert_eq!(w.step(&k, id, &[]), Err(Error::Bounds));
        assert_eq!(calls.get(), 0);
        assert_eq!(w.inspect(id).unwrap(), before);
        assert_eq!(k.lifecycle_state(), life);
        w.0.borrow_mut().limits.wire_bytes = 16384;
        w.step(&k, id, &[]).unwrap();
        let lease = w.0.borrow().frames[&id].lease;
        k.release_pipeline(lease).unwrap(); // Simulate trusted host violating lease ownership.
        let before = w.inspect(id).unwrap();
        assert_eq!(
            w.dispose(&mut k, id),
            Err(Error::Kernel(KernelError::InvalidHandle))
        );
        assert_eq!(w.inspect(id).unwrap(), before);
    }
}

#[cfg(test)]
mod admission_tests {
    use super::*;
    #[test]
    fn invalid_program_and_kernel_lease_capacity_do_not_leak_references() {
        let (mut k, w, id, _) = super::tests::setup(Limits::default(), super::tests::done());
        let before = w.inspect(id).unwrap();
        let life = k.lifecycle_state();
        for which in 0..3 {
            let mut p = before.program.clone();
            match which {
                0 => p.initial = "unknown".into(),
                1 => {
                    p.allowed.insert(("a".into(), "unknown".into()));
                }
                _ => {
                    p.phases.insert("x".repeat(129));
                }
            }
            assert_eq!(w.start(&mut k, p, vec![], vec![]), Err(Error::Bounds));
            assert_eq!(k.lifecycle_state(), life);
            assert_eq!(w.inspect(id).unwrap(), before);
        }
        assert_eq!(
            w.start(&mut k, before.program.clone(), vec![], vec![]),
            Err(Error::Kernel(KernelError::Limit))
        );
        assert_eq!(k.lifecycle_state(), life);
        assert_eq!(w.inspect(id).unwrap(), before);
    }
}
