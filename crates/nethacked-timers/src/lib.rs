//! Optional ephemeral timers. Native ports receive only authenticated CallerView.
#![forbid(unsafe_code)]
use nethacked_kernel::{
    Access, Authority, CallbackProvenance, CallerView, Capability, CleanupIdentity, Endpoint,
    Error, Kernel, Manifest, ModuleHandle, NativePort, OperationBinding, PipelineLease, Resource,
    Result, RetainedCallback, Scope,
};
use nethacked_resources::Stamp;
use nethacked_workflows::CleanupParticipant;
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT_QUEUE: AtomicU64 = AtomicU64::new(1);
const PHASE: &str = "timer";

#[cfg(feature = "persistence")]
pub mod persistence;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub records: usize,
    pub targets: usize,
    pub participants: usize,
    pub field_bytes: usize,
    pub payload_bytes: usize,
    pub context_bytes: usize,
    pub result_bytes: usize,
    pub wire_bytes: u32,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            records: 64,
            targets: 16,
            participants: 16,
            field_bytes: 128,
            payload_bytes: 4096,
            context_bytes: 4096,
            result_bytes: 4096,
            wire_bytes: 16384,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskId {
    pub queue: u64,
    pub serial: u64,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum Request {
    Schedule {
        callback: String,
        payload: Vec<u8>,
        targets: Vec<Stamp>,
        deadline: Vec<u8>,
        residency: Vec<u8>,
        ceiling: Authority,
    },
    Cancel {
        task: TaskId,
    },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pending {
    pub id: TaskId,
    pub deadline: Vec<u8>,
    pub residency: Vec<u8>,
}
/// Side-effect-free, bounded, non-panicking trusted policy. No runtime access.
pub trait Profile {
    fn validate(&self, deadline: &[u8], residency: &[u8]) -> Result<()>;
    fn select(&self, pending: &[Pending], context: &[u8]) -> Result<Option<TaskId>>;
    fn ready(&self, pending: &Pending, context: &[u8]) -> Result<bool>;
}
/// Checks only declared stamps against authoritative owners; never a Kernel/World.
pub trait TargetValidator {
    fn validate(&self, targets: &[Stamp]) -> Result<()>;
}
/// Optional trusted host ownership of codec/behavior references paired with the
/// captured callback. No resource payload is read or mirrored by the queue.
pub trait CallbackRetention {
    /// Nonexecuting bounded admission before kernel capture allocates identities.
    /// Implementations must not perform effects or consume reservations here.
    fn preflight(&self, _view: &CallerView<'_>, _targets: &[Stamp]) -> Result<()> {
        Ok(())
    }
    fn retain(
        &self,
        callback: &RetainedCallback,
        targets: &[Stamp],
    ) -> Result<Rc<dyn CallbackReference>>;
}
/// Ownership only. The callback must independently authenticate every use.
pub trait CallbackReference {
    #[cfg(feature = "persistence")]
    fn persistence_type(&self) -> &dyn std::any::Any;
}
#[cfg(not(feature = "persistence"))]
impl<T> CallbackReference for T {}
#[cfg(feature = "persistence")]
impl<T: 'static> CallbackReference for T {
    fn persistence_type(&self) -> &dyn std::any::Any {
        self
    }
}
/// Optional adapter over the existing bounded resource owner, with no copied authority.
impl TargetValidator for nethacked_resources::Service {
    fn validate(&self, targets: &[Stamp]) -> Result<()> {
        let state = self.state()?;
        if targets.iter().all(|s| {
            state
                .resources
                .get(&s.address)
                .is_some_and(|v| &v.stamp == s)
        }) {
            Ok(())
        } else {
            Err(Error::NativeRefusal)
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum Outcome {
    Completed { result: Vec<u8> },
    Rejected { result: Vec<u8> },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Fault {
    Invocation(Error),
    InvalidOutcome,
    Cleanup,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Status {
    Pending,
    Running,
    Completed(Vec<u8>),
    Rejected(Vec<u8>),
    Faulted(Fault),
    Cancelled,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Task {
    pub id: TaskId,
    pub provenance: CallbackProvenance,
    pub ceiling: Authority,
    pub payload: Vec<u8>,
    pub targets: Vec<Stamp>,
    pub deadline: Vec<u8>,
    pub residency: Vec<u8>,
    pub status: Status,
    pub invocation: Option<OperationBinding>,
    pub cleanup_pending: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct State {
    pub queue: u64,
    pub next_serial: u64,
    pub active: Option<TaskId>,
    pub tasks: BTreeMap<TaskId, Task>,
}
struct Owned {
    task: Task,
    callback: Option<RetainedCallback>,
    lease: Option<PipelineLease>,
    reference: Option<Rc<dyn CallbackReference>>,
    #[cfg(feature = "persistence")]
    history: Option<nethacked_kernel::persistence::TimerHistory>,
}
struct Inner {
    limits: Limits,
    queue: Resource,
    namespace: u64,
    next_serial: u64,
    binding: Option<(ModuleHandle, [u8; 32], Capability)>,
    profile: Rc<dyn Profile>,
    validator: Rc<dyn TargetValidator>,
    validate_on_schedule: bool,
    participants: Vec<Rc<dyn CleanupParticipant>>,
    identities: Vec<CleanupIdentity>,
    #[cfg(feature = "persistence")]
    participant_identities: Vec<Option<CleanupIdentity>>,
    tasks: BTreeMap<TaskId, Owned>,
    active: Option<TaskId>,
    retention: Option<Rc<dyn CallbackRetention>>,
}
/// Shared queue: Running is published and its borrow released before guest execution.
#[derive(Clone)]
pub struct Service(Rc<RefCell<Inner>>);
impl Service {
    pub fn new(
        limits: Limits,
        queue: Resource,
        profile: Rc<dyn Profile>,
        validator: Rc<dyn TargetValidator>,
        validate_on_schedule: bool,
        participants: Vec<Rc<dyn CleanupParticipant>>,
    ) -> Result<Self> {
        if limits.records == 0 || limits.wire_bytes == 0 || participants.len() > limits.participants
        {
            return Err(Error::Limit);
        }
        check_address(&limits, &queue)?;
        let mut identities = Vec::new();
        #[cfg(feature = "persistence")]
        let mut participant_identities = Vec::new();
        for p in &participants {
            let participant_identity = p.identity()?;
            #[cfg(feature = "persistence")]
            participant_identities.push(participant_identity);
            if let Some(id) = participant_identity {
                if identities.contains(&id) {
                    return Err(Error::Duplicate);
                }
                identities.push(id);
            }
        }
        let namespace = issue_namespace(&NEXT_QUEUE)?;
        Ok(Self(Rc::new(RefCell::new(Inner {
            limits,
            queue,
            namespace,
            next_serial: 1,
            binding: None,
            profile,
            validator,
            validate_on_schedule,
            participants,
            identities,
            #[cfg(feature = "persistence")]
            participant_identities,
            tasks: BTreeMap::new(),
            active: None,
            retention: None,
        }))))
    }
    pub fn install(&self, kernel: &mut Kernel, id: &str) -> Result<ModuleHandle> {
        let mut i = self.0.try_borrow_mut().map_err(|_| Error::NativeRefusal)?;
        if i.active.is_some() {
            return Err(Error::NativeRefusal);
        }
        if i.binding.is_some() {
            return Err(Error::Duplicate);
        }
        let service = self.clone();
        let port: NativePort = Rc::new(move |bytes, view| service.dispatch(bytes, view));
        let artifact = encode(&(
            "nethacked-timers-queue-v1",
            env!("CARGO_PKG_VERSION"),
            &i.limits,
            &i.queue,
        ))?;
        let handle = kernel.register_native(
            Manifest {
                id: id.into(),
                exports: BTreeMap::from([(
                    "queue".into(),
                    Endpoint {
                        major: 1,
                        access: Access::Write,
                        max_request: i.limits.wire_bytes,
                        max_reply: i.limits.wire_bytes,
                    },
                )]),
                dependencies: vec![],
                extension_points: BTreeSet::new(),
                extension: None,
            },
            &artifact,
            BTreeMap::from([("queue".into(), port)]),
        )?;
        i.binding = Some((
            handle,
            kernel.artifact(handle)?,
            Capability {
                provider: id.into(),
                endpoint: "queue".into(),
                major: 1,
            },
        ));
        Ok(handle)
    }
    /// Configure before install/first task; failed configuration preserves state.
    pub fn retain_callbacks(&self, retention: Rc<dyn CallbackRetention>) -> Result<()> {
        let mut i = self.0.try_borrow_mut().map_err(|_| Error::NativeRefusal)?;
        if i.binding.is_some() || !i.tasks.is_empty() || i.retention.is_some() {
            return Err(Error::NativeRefusal);
        }
        i.retention = Some(retention);
        Ok(())
    }
    pub fn state(&self) -> Result<State> {
        let i = self.0.try_borrow().map_err(|_| Error::NativeRefusal)?;
        Ok(State {
            queue: i.namespace,
            next_serial: i.next_serial,
            active: i.active,
            tasks: i
                .tasks
                .iter()
                .map(|(id, o)| (*id, o.task.clone()))
                .collect(),
        })
    }
    fn dispatch(&self, bytes: &[u8], view: &CallerView) -> Result<Vec<u8>> {
        let mut i = self.0.try_borrow_mut().map_err(|_| Error::NativeRefusal)?;
        if bytes.len() > i.limits.wire_bytes as usize {
            return Err(Error::Buffer);
        }
        let req: Request = serde_json::from_slice(bytes).map_err(|_| Error::NativeRefusal)?;
        let (caller, artifact) = i.authenticate(view)?;
        match req {
            Request::Schedule {
                callback,
                payload,
                targets,
                deadline,
                residency,
                ceiling,
            } => {
                if i.tasks.len() >= i.limits.records {
                    return Err(Error::Limit);
                }
                let next = i.next_serial.checked_add(1).ok_or(Error::Limit)?;
                if callback.is_empty()
                    || callback.len() > i.limits.field_bytes
                    || payload.len() > i.limits.payload_bytes
                    || deadline.len() > i.limits.field_bytes
                    || residency.len() > i.limits.field_bytes
                {
                    return Err(Error::Buffer);
                }
                i.check_targets(&targets, view.authority())?;
                check_ceiling(&i.limits, &ceiling)?;
                // Capture validates the ceiling against the actual invocation.
                i.check_targets(&targets, &ceiling)?;
                if ceiling.access != Access::Write || !ceiling.scope.allows(&i.queue) {
                    return Err(Error::Authority);
                }
                i.profile.validate(&deadline, &residency)?;
                if i.validate_on_schedule {
                    i.validator.validate(&targets)?;
                }
                let id = TaskId {
                    queue: i.namespace,
                    serial: i.next_serial,
                };
                let reply = bounded_reply(&i.limits, &id, view)?;
                if let Some(retention) = &i.retention {
                    retention.preflight(view, &targets)?;
                }
                // Both opaque code capture and paired host retention complete
                // before any queue record/serial is published.
                let captured = view.capture_callback(&callback, ceiling.clone())?;
                let reference = i
                    .retention
                    .as_ref()
                    .map(|r| r.retain(&captured, &targets))
                    .transpose()?;
                let task = Task {
                    id,
                    provenance: captured.provenance().clone(),
                    ceiling,
                    payload,
                    targets,
                    deadline,
                    residency,
                    status: Status::Pending,
                    invocation: None,
                    cleanup_pending: false,
                };
                i.tasks.insert(
                    id,
                    Owned {
                        task,
                        callback: Some(captured),
                        lease: None,
                        reference,
                        #[cfg(feature = "persistence")]
                        history: None,
                    },
                );
                i.next_serial = next;
                Ok(reply)
            }
            Request::Cancel { task } => {
                let owned = i.tasks.get(&task).ok_or(Error::InvalidHandle)?;
                if owned.task.status != Status::Pending {
                    return Err(Error::NativeRefusal);
                }
                if owned.task.provenance.caller.handle != caller
                    || owned.task.provenance.caller.artifact != artifact
                {
                    return Err(Error::Authority);
                }
                i.check_targets(&owned.task.targets, view.authority())?;
                let reply = bounded_reply(&i.limits, &task, view)?;
                let owned = i.tasks.get_mut(&task).expect("checked task");
                owned.callback = None;
                owned.reference = None;
                owned.task.status = Status::Cancelled;
                Ok(reply)
            }
        }
    }
    /// Trusted host only. Pre-admission errors preserve identities and all task state.
    pub fn deliver_next(&self, kernel: &mut Kernel, context: &[u8]) -> Result<Option<Task>> {
        let (id, binding, authority, payload) = {
            let mut i = self.0.try_borrow_mut().map_err(|_| Error::NativeRefusal)?;
            if i.active.is_some() {
                return Err(Error::NativeRefusal);
            }
            if context.len() > i.limits.context_bytes {
                return Err(Error::Buffer);
            }
            let pending: Vec<_> = i
                .tasks
                .values()
                .filter(|o| o.task.status == Status::Pending)
                .map(|o| Pending {
                    id: o.task.id,
                    deadline: o.task.deadline.clone(),
                    residency: o.task.residency.clone(),
                })
                .collect();
            let Some(id) = i.profile.select(&pending, context)? else {
                return Ok(None);
            };
            let owned = i.tasks.get(&id).ok_or(Error::InvalidHandle)?;
            if owned.task.status != Status::Pending {
                return Err(Error::NativeRefusal);
            }
            let selected = pending
                .iter()
                .find(|p| p.id == id)
                .ok_or(Error::InvalidHandle)?;
            if !i.profile.ready(selected, context)? {
                return Err(Error::NativeRefusal);
            }
            let captured = owned.callback.as_ref().ok_or(Error::InvalidHandle)?;
            let (service, artifact, _) = i.binding.as_ref().ok_or(Error::InvalidHandle)?;
            if kernel.artifact(*service)? != *artifact
                || captured.provenance().service.handle != *service
                || captured.provenance().service.artifact != *artifact
            {
                return Err(Error::InvalidHandle);
            }
            i.validator.validate(&owned.task.targets)?;
            kernel.preflight_callback(
                captured,
                &owned.task.payload,
                &owned.task.ceiling,
                PHASE,
                &i.identities,
            )?;
            let authority = owned.task.ceiling.clone();
            let payload = owned.task.payload.clone();
            let identities = i.identities.clone();
            let (op, lease) = kernel.adopt_callback(captured)?;
            // Once-only commit: publish Running before any invocation or cleanup.
            i.active = Some(id);
            let owned = i.tasks.get_mut(&id).expect("checked task");
            owned.lease = Some(lease);
            owned.callback = None;
            owned.task.status = Status::Running;
            let binding = kernel.operation_binding_with_cleanup(op, lease, 0, PHASE, &identities);
            match binding {
                Ok(binding) => {
                    owned.task.invocation = Some(binding.clone());
                    owned.task.cleanup_pending = true;
                    (id, binding, authority, payload)
                }
                Err(error) => {
                    owned.task.status = Status::Faulted(Fault::Invocation(error));
                    let task = owned.task.clone();
                    i.active = None;
                    return Ok(Some(task));
                }
            }
        };
        // No queue borrow spans execution. Native scheduling uses the same queue.
        let reply = kernel.invoke_operation(&binding, &payload, authority);
        let mut i = self.0.try_borrow_mut().map_err(|_| Error::NativeRefusal)?;
        let decoded = decode_outcome(&i.limits, reply);
        let cleaned = clean(&i, &binding);
        let owned = i.tasks.get_mut(&id).expect("active task cannot be removed");
        owned.task.cleanup_pending = cleaned.is_err();
        if cleaned.is_ok() {
            owned.reference = None;
        }
        owned.task.status = match (decoded, cleaned) {
            (Err(fault), _) => Status::Faulted(fault),
            (_, Err(_)) => Status::Faulted(Fault::Cleanup),
            (Ok(Outcome::Completed { result }), Ok(())) => Status::Completed(result),
            (Ok(Outcome::Rejected { result }), Ok(())) => Status::Rejected(result),
        };
        let task = owned.task.clone();
        i.active = None;
        Ok(Some(task))
    }
    /// Only idempotent discard is retried. Never executes a callback.
    pub fn retry_cleanup(&self, id: TaskId) -> Result<()> {
        let mut i = self.0.try_borrow_mut().map_err(|_| Error::NativeRefusal)?;
        if i.active.is_some() {
            return Err(Error::NativeRefusal);
        }
        let owned = i.tasks.get(&id).ok_or(Error::InvalidHandle)?;
        if !matches!(owned.task.status, Status::Faulted(_)) || !owned.task.cleanup_pending {
            return Err(Error::NativeRefusal);
        }
        clean(
            &i,
            owned.task.invocation.as_ref().ok_or(Error::InvalidHandle)?,
        )?;
        let owned = i.tasks.get_mut(&id).expect("checked task");
        owned.task.cleanup_pending = false;
        owned.reference = None;
        Ok(())
    }
    /// Terminal records occupy finite capacity until the host disposes them.
    /// Failed lease release leaves the record intact and can be retried.
    pub fn dispose(&self, kernel: &mut Kernel, id: TaskId) -> Result<()> {
        let mut i = self.0.try_borrow_mut().map_err(|_| Error::NativeRefusal)?;
        if i.active.is_some() {
            return Err(Error::NativeRefusal);
        }
        let owned = i.tasks.get(&id).ok_or(Error::InvalidHandle)?;
        if matches!(owned.task.status, Status::Pending | Status::Running)
            || owned.task.cleanup_pending
        {
            return Err(Error::NativeRefusal);
        }
        if let Some(lease) = owned.lease {
            kernel.release_pipeline(lease)?;
        } else {
            let (service, artifact, _) = i.binding.as_ref().ok_or(Error::InvalidHandle)?;
            if kernel.artifact(*service)? != *artifact {
                return Err(Error::InvalidHandle);
            }
        }
        i.tasks.remove(&id);
        Ok(())
    }
}
impl Inner {
    fn authenticate(&self, view: &CallerView) -> Result<(ModuleHandle, [u8; 32])> {
        let (provider, artifact, cap) = self.binding.as_ref().ok_or(Error::Authority)?;
        if *provider != view.provider()
            || *artifact != view.provider_artifact()
            || view.operation().is_none()
        {
            return Err(Error::Authority);
        }
        let permit = view
            .permit(cap, &self.queue, Access::Write)
            .ok_or(Error::Authority)?;
        if !permit
            .resolution
            .last()
            .is_some_and(|p| p.handle == *provider && p.artifact == *artifact)
            || view.authority().access != Access::Write
            || !view.authority().scope.allows(&self.queue)
        {
            return Err(Error::Authority);
        }
        Ok((
            view.caller().ok_or(Error::Authority)?,
            view.caller_artifact().ok_or(Error::Authority)?,
        ))
    }
    fn check_targets(&self, targets: &[Stamp], authority: &Authority) -> Result<()> {
        if targets.len() > self.limits.targets {
            return Err(Error::Limit);
        }
        let mut addresses = BTreeSet::new();
        for target in targets {
            check_address(&self.limits, &target.address)?;
            if target.generation == 0 || !addresses.insert(&target.address) {
                return Err(Error::NativeRefusal);
            }
            if authority.access != Access::Write || !authority.scope.allows(&target.address) {
                return Err(Error::Authority);
            }
        }
        Ok(())
    }
}
fn clean(i: &Inner, binding: &OperationBinding) -> Result<()> {
    // Attempt every participant even after failure. Completed discards are idempotent.
    let mut failed = false;
    for p in &i.participants {
        failed |= p.discard(binding).is_err();
    }
    if failed {
        Err(Error::NativeRefusal)
    } else {
        Ok(())
    }
}
fn issue_namespace(counter: &AtomicU64) -> Result<u64> {
    issue_namespace_after(counter, 0)
}
// Reserve the saved allocation history and issue in one atomic operation. A
// failed hidden build burns its allocation just like a dropped ordinary queue.
fn issue_namespace_after(counter: &AtomicU64, retired: u64) -> Result<u64> {
    let floor = retired.checked_add(1).ok_or(Error::Limit)?;
    counter
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
            Some(v.max(floor).saturating_add(1))
        })
        .map_err(|_| Error::Limit)
        .and_then(|v| match v.max(floor) {
            u64::MAX => Err(Error::Limit),
            namespace => Ok(namespace),
        })
}
fn check_address(l: &Limits, r: &Resource) -> Result<()> {
    if r.owner.is_empty()
        || r.key.is_empty()
        || r.owner.len() > l.field_bytes
        || r.key.len() > l.field_bytes
    {
        Err(Error::Buffer)
    } else {
        Ok(())
    }
}
fn check_ceiling(l: &Limits, a: &Authority) -> Result<()> {
    if let Scope::Keys(keys) = &a.scope {
        if keys.len() > l.targets {
            return Err(Error::Limit);
        }
        for key in keys {
            check_address(l, key)?;
        }
    }
    Ok(())
}
fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    serde_json::to_vec(&serde_json::to_value(value).map_err(|_| Error::NativeRefusal)?)
        .map_err(|_| Error::NativeRefusal)
}
fn bounded_reply<T: Serialize>(l: &Limits, value: &T, view: &CallerView) -> Result<Vec<u8>> {
    let reply = encode(value)?;
    if reply.len() > l.wire_bytes as usize || reply.len() > view.max_reply() as usize {
        Err(Error::Buffer)
    } else {
        Ok(reply)
    }
}
fn decode_outcome(l: &Limits, reply: Result<Vec<u8>>) -> std::result::Result<Outcome, Fault> {
    let bytes = reply.map_err(Fault::Invocation)?;
    if bytes.len() > l.wire_bytes as usize {
        return Err(Fault::InvalidOutcome);
    }
    let outcome: Outcome = serde_json::from_slice(&bytes).map_err(|_| Fault::InvalidOutcome)?;
    let result = match &outcome {
        Outcome::Completed { result } | Outcome::Rejected { result } => result,
    };
    if result.len() > l.result_bytes {
        return Err(Fault::InvalidOutcome);
    }
    Ok(outcome)
}

#[cfg(test)]
extern crate self as nethacked_timers;
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
