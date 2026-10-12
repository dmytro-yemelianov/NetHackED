//! D4b timer-local persistence. The complete barrier and atomic activation belong
//! to D4e. Catalog entries are independent host approval, never incoming metadata.
//! Profile/target validators and cleanup/retention factories must be bounded,
//! pure, non-panicking, faithful and have effect-free drops and private storage.
use super::*;
use nethacked_kernel::persistence::{
    ExecutionCaptureIndex, ExecutionDestination, KernelSnapshot, RestoredKernel, SavedOperation,
    TimerHistory,
};
pub use nethacked_workflows::persistence::Identity;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PersistenceError {
    CaptureBusy,
    Refused(Error),
}
pub type PersistenceResult<T> = std::result::Result<T, PersistenceError>;
impl From<Error> for PersistenceError {
    fn from(e: Error) -> Self {
        Self::Refused(e)
    }
}
fn busy<T>(value: std::result::Result<T, std::cell::BorrowMutError>) -> PersistenceResult<T> {
    value.map_err(|_| PersistenceError::CaptureBusy)
}
fn identity(i: &Identity) -> Result<()> {
    if i.name.is_empty() || i.name.len() > 256 || i.version == 0 {
        Err(Error::Limit)
    } else {
        Ok(())
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum SavedStatus {
    Pending,
    Cancelled,
    Completed(Vec<u8>),
    Rejected(Vec<u8>),
    Invocation(i32),
    InvalidOutcome,
    Cleanup,
}
impl SavedStatus {
    fn capture(s: &Status) -> Result<Self> {
        Ok(match s {
            Status::Pending => Self::Pending,
            Status::Cancelled => Self::Cancelled,
            Status::Completed(v) => Self::Completed(v.clone()),
            Status::Rejected(v) => Self::Rejected(v.clone()),
            Status::Faulted(Fault::Invocation(e)) => Self::Invocation(e.abi_code()),
            Status::Faulted(Fault::InvalidOutcome) => Self::InvalidOutcome,
            Status::Faulted(Fault::Cleanup) => Self::Cleanup,
            Status::Running => return Err(Error::NativeRefusal),
        })
    }
    fn restore(&self) -> Result<Status> {
        Ok(match self {
            Self::Pending => Status::Pending,
            Self::Cancelled => Status::Cancelled,
            Self::Completed(v) => Status::Completed(v.clone()),
            Self::Rejected(v) => Status::Rejected(v.clone()),
            Self::Invocation(code) => Status::Faulted(Fault::Invocation(error_code(*code)?)),
            Self::InvalidOutcome => Status::Faulted(Fault::InvalidOutcome),
            Self::Cleanup => Status::Faulted(Fault::Cleanup),
        })
    }
}
fn error_code(code: i32) -> Result<Error> {
    [
        Error::InvalidManifest,
        Error::InvalidModule,
        Error::InvalidHandle,
        Error::MissingDependency,
        Error::Duplicate,
        Error::Signature,
        Error::InvalidSlot,
        Error::MissingGrant,
        Error::Revoked,
        Error::Authority,
        Error::Buffer,
        Error::Limit,
        Error::Trap,
        Error::NativeRefusal,
        Error::Referenced,
        Error::Cycle,
        Error::Busy,
    ]
    .into_iter()
    .find(|e| e.abi_code() == code)
    .ok_or(Error::InvalidHandle)
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticipantRow {
    pub build: Identity,
    pub cleanup: Option<u64>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskRow {
    pub serial: u64,
    pub provenance: TimerHistory,
    pub ceiling: Authority,
    pub payload: Vec<u8>,
    pub targets: Vec<Stamp>,
    pub deadline: Vec<u8>,
    pub residency: Vec<u8>,
    pub status: SavedStatus,
    pub callback: Option<u64>,
    pub invocation: Option<SavedOperation>,
    pub lease: Option<u64>,
    pub cleanup_pending: bool,
    pub reference: Option<u64>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    pub version: u32,
    pub namespace: u64,
    /// All namespaces issued in the captured process and its restored history,
    /// including disposed queues and unsuccessful hidden builds.
    pub namespace_high_water: u64,
    pub kernel_namespace: u64,
    pub next_serial: u64,
    pub limits: Limits,
    pub queue: Resource,
    pub service: u64,
    pub artifact: [u8; 32],
    pub capability: Capability,
    pub profile: Identity,
    pub targets: Identity,
    pub validate_on_schedule: bool,
    pub retention: Option<Identity>,
    pub participants: Vec<ParticipantRow>,
    pub occurrences: Vec<usize>,
    pub tasks: Vec<TaskRow>,
}
impl Checkpoint {
    pub fn count_references(&self, n: &mut nethacked_kernel::persistence::ReferenceCounter) {
        n.add(4);
        n.add(usize::from(self.retention.is_some())); // queue, service, profile, target codec and retention
        for p in &self.participants {
            n.add(1);
            n.add(usize::from(p.cleanup.is_some()));
        }
        n.add(self.occurrences.len());
        for t in &self.tasks {
            n.add(2);
            n.operation(&t.provenance.source);
            n.authority(&t.ceiling);
            n.add(t.targets.len());
            n.add(usize::from(t.callback.is_some()));
            if let Some(o) = &t.invocation {
                n.operation(o);
            }
            n.add(usize::from(t.lease.is_some()));
            n.add(usize::from(t.reference.is_some()));
        }
    }
    pub fn to_bytes(&self, max: usize) -> Result<Vec<u8>> {
        nethacked_workflows::persistence::encode_local_checkpoint(self, max)
            .map_err(|_| Error::Buffer)
    }
    pub fn from_bytes(bytes: &[u8], max: usize) -> Result<Self> {
        nethacked_workflows::persistence::decode_local_checkpoint(bytes, max)
            .map_err(|_| Error::Buffer)
    }
}
/// Capture enrollment binds actual allocations, including shared cleanup aliases.
pub struct CaptureCatalog {
    owner: Service,
    profile: Identity,
    targets: Identity,
    retention: Option<Identity>,
    participants: Vec<(Identity, Rc<dyn CleanupParticipant>)>,
    references: Vec<(u64, Rc<dyn CallbackReference>)>,
}
impl CaptureCatalog {
    pub fn new(
        owner: &Service,
        profile: Identity,
        targets: Identity,
        retention: Option<Identity>,
    ) -> Result<Self> {
        identity(&profile)?;
        identity(&targets)?;
        if let Some(i) = &retention {
            identity(i)?;
        }
        Ok(Self {
            owner: owner.clone(),
            profile,
            targets,
            retention,
            participants: vec![],
            references: vec![],
        })
    }
    pub fn participant(
        &mut self,
        build: Identity,
        value: Rc<dyn CleanupParticipant>,
    ) -> PersistenceResult<()> {
        identity(&build)?;
        let i = busy(self.owner.0.try_borrow_mut())?;
        if !i.participants.iter().any(|p| Rc::ptr_eq(p, &value))
            || self.participants.iter().any(|(_, p)| Rc::ptr_eq(p, &value))
        {
            return Err(Error::InvalidHandle.into());
        }
        self.participants.push((build, value));
        Ok(())
    }
    pub fn reference(
        &mut self,
        id: u64,
        value: Rc<dyn CallbackReference>,
    ) -> PersistenceResult<()> {
        let i = busy(self.owner.0.try_borrow_mut())?;
        if id == 0
            || !i
                .tasks
                .values()
                .any(|o| o.reference.as_ref().is_some_and(|r| Rc::ptr_eq(r, &value)))
            || self
                .references
                .iter()
                .any(|(n, r)| *n == id || Rc::ptr_eq(r, &value))
        {
            return Err(Error::InvalidHandle.into());
        }
        self.references.push((id, value));
        Ok(())
    }
}
impl Service {
    pub fn persistence_resources(&self) -> PersistenceResult<Vec<Resource>> {
        self.1
            .check_live()
            .map_err(|_| PersistenceError::CaptureBusy)?;
        let i = busy(self.0.try_borrow_mut())?;
        if i.active.is_some() {
            return Err(PersistenceError::CaptureBusy);
        }
        let mut resources = BTreeSet::from([i.queue.clone()]);
        for o in i.tasks.values() {
            resources.extend(o.task.targets.iter().map(|s| s.address.clone()));
            if let Scope::Keys(keys) = &o.task.ceiling.scope {
                resources.extend(keys.iter().cloned());
            }
        }
        if resources.len() > 65536 {
            return Err(Error::Limit.into());
        }
        Ok(resources.into_iter().collect())
    }
    pub fn persistence_cleanup(&self) -> PersistenceResult<Vec<CleanupIdentity>> {
        self.1
            .check_live()
            .map_err(|_| PersistenceError::CaptureBusy)?;
        let i = busy(self.0.try_borrow_mut())?;
        if i.active.is_some() {
            return Err(PersistenceError::CaptureBusy);
        }
        let mut identities: BTreeSet<_> = i.identities.iter().copied().collect();
        for o in i.tasks.values() {
            identities.extend(o.task.provenance.source.cleanup_identities());
        }
        Ok(identities.into_iter().collect())
    }
    pub fn persistence_participants(&self) -> PersistenceResult<Vec<Rc<dyn CleanupParticipant>>> {
        self.1
            .check_live()
            .map_err(|_| PersistenceError::CaptureBusy)?;
        let i = busy(self.0.try_borrow_mut())?;
        if i.active.is_some() {
            return Err(PersistenceError::CaptureBusy);
        }
        Ok(i.participants.clone())
    }
    pub fn persistence_references(
        &self,
    ) -> PersistenceResult<Vec<(TaskId, Rc<dyn CallbackReference>)>> {
        self.1
            .check_live()
            .map_err(|_| PersistenceError::CaptureBusy)?;
        let i = busy(self.0.try_borrow_mut())?;
        if i.active.is_some() {
            return Err(PersistenceError::CaptureBusy);
        }
        Ok(i.tasks
            .iter()
            .filter_map(|(id, o)| o.reference.as_ref().map(|r| (*id, r.clone())))
            .collect())
    }
    pub fn persistence_enrollment(&self) -> PersistenceResult<Vec<OperationBinding>> {
        self.1
            .check_live()
            .map_err(|_| PersistenceError::CaptureBusy)?;
        let i = busy(self.0.try_borrow_mut())?;
        if i.active.is_some() || i.tasks.values().any(|o| o.task.status == Status::Running) {
            return Err(PersistenceError::CaptureBusy);
        }
        Ok(i.tasks
            .values()
            .filter_map(|o| o.task.invocation.clone())
            .collect())
    }
    pub fn capture_local(
        &self,
        index: &ExecutionCaptureIndex<'_>,
        catalog: &CaptureCatalog,
    ) -> PersistenceResult<Checkpoint> {
        self.1
            .capture_ready()
            .map_err(|_| PersistenceError::CaptureBusy)?;
        let i = busy(self.0.try_borrow_mut())?;
        if i.active.is_some() || i.tasks.values().any(|o| o.task.status == Status::Running) {
            return Err(PersistenceError::CaptureBusy);
        }
        if !Rc::ptr_eq(&self.0, &catalog.owner.0)
            || i.retention.is_some() != catalog.retention.is_some()
        {
            return Err(Error::Authority.into());
        }
        let (service, artifact, cap) = i.binding.as_ref().ok_or(Error::InvalidHandle)?;
        let mut participants = vec![];
        let mut allocations: Vec<Rc<dyn CleanupParticipant>> = vec![];
        let mut occurrences = vec![];
        for (position, p) in i.participants.iter().enumerate() {
            if let Some(id) = allocations.iter().position(|v| Rc::ptr_eq(v, p)) {
                occurrences.push(id);
                continue;
            }
            let build = catalog
                .participants
                .iter()
                .find(|(_, v)| Rc::ptr_eq(v, p))
                .ok_or(Error::Authority)?
                .0
                .clone();
            participants.push(ParticipantRow {
                build,
                cleanup: i.participant_identities[position]
                    .map(|id| index.cleanup(id))
                    .transpose()?,
            });
            occurrences.push(allocations.len());
            allocations.push(p.clone());
        }
        let tasks = i
            .tasks
            .values()
            .map(|o| {
                Ok(TaskRow {
                    serial: o.task.id.serial,
                    provenance: match &o.history {
                        Some(h) => h.clone(),
                        None => index.timer_history(&o.task.provenance)?,
                    },
                    ceiling: o.task.ceiling.clone(),
                    payload: o.task.payload.clone(),
                    targets: o.task.targets.clone(),
                    deadline: o.task.deadline.clone(),
                    residency: o.task.residency.clone(),
                    status: SavedStatus::capture(&o.task.status)?,
                    callback: o.callback.as_ref().map(|c| index.callback(c)).transpose()?,
                    invocation: o
                        .task
                        .invocation
                        .as_ref()
                        .map(|b| index.source(b))
                        .transpose()?,
                    lease: o
                        .lease
                        .map(|_| {
                            o.task
                                .invocation
                                .as_ref()
                                .map(|b| index.source(b))
                                .transpose()
                        })
                        .transpose()?
                        .flatten()
                        .map(|b| b.pipeline),
                    cleanup_pending: o.task.cleanup_pending,
                    reference: o
                        .reference
                        .as_ref()
                        .map(|r| {
                            catalog
                                .references
                                .iter()
                                .find(|(_, v)| Rc::ptr_eq(v, r))
                                .map(|(id, _)| *id)
                                .ok_or(Error::Authority)
                        })
                        .transpose()?,
                })
            })
            .collect::<Result<_>>()?;
        Ok(Checkpoint {
            version: 2,
            namespace: i.namespace,
            namespace_high_water: NEXT_QUEUE.load(Ordering::Relaxed) - 1,
            kernel_namespace: index.namespace(),
            next_serial: i.next_serial,
            limits: i.limits.clone(),
            queue: i.queue.clone(),
            service: index.module(*service)?,
            artifact: *artifact,
            capability: cap.clone(),
            profile: catalog.profile.clone(),
            targets: catalog.targets.clone(),
            validate_on_schedule: i.validate_on_schedule,
            retention: catalog.retention.clone(),
            participants,
            occurrences,
            tasks,
        })
    }
}
/// Host-supplied old semantic validation and explicit reference transform. Neither
/// method may schedule, sample, execute callbacks, mutate owners or claim authority.
pub trait TargetCodec {
    fn validate_old(&self, targets: &[Stamp]) -> Result<()>;
    fn remap(&self, targets: &[Stamp], mapped: &RestoredKernel) -> Result<Vec<Stamp>>;
}
/// Independently reconstructed adapter references bind the actual D3 destination.
pub trait RetentionRestore {
    fn destination(&self) -> ExecutionDestination;
    fn check(&self, row: &TaskRow, mapped: &RestoredKernel) -> Result<()>;
    fn reference(&self, id: u64) -> Result<Rc<dyn CallbackReference>>;
    /// Nonexecuting release of hidden adapter staging roots after all timer
    /// records own their references. This must be infallible and effect-free.
    fn finish_rebuild(&self) {}
}
pub trait TimerAuthority {
    /// Approve the complete exact checkpoint from independent host state.
    /// Echoing incoming identities, grants, rows or digests is not approval.
    fn approve(&self, checkpoint: &Checkpoint, kernel: &KernelSnapshot) -> Result<()>;
}
type CleanupFactory = Box<dyn Fn(Option<CleanupIdentity>) -> Result<Rc<dyn CleanupParticipant>>>;
type RetentionEntry = (
    Identity,
    Rc<dyn CallbackRetention>,
    Rc<dyn RetentionRestore>,
);
pub struct RestoreCatalog {
    profile: (Identity, Rc<dyn Profile>),
    targets: (Identity, Rc<dyn TargetValidator>, Rc<dyn TargetCodec>),
    retention: Option<RetentionEntry>,
    participants: BTreeMap<Identity, CleanupFactory>,
    port: Rc<RefCell<Option<Service>>>,
}
impl RestoreCatalog {
    pub fn missing_dependency(&self, c: &Checkpoint) -> Option<String> {
        if self.profile.0 != c.profile {
            return Some(format!("profile policy: {:?}", c.profile));
        }
        if self.targets.0 != c.targets {
            return Some(format!("target codec: {:?}", c.targets));
        }
        if self.retention.as_ref().map(|r| &r.0) != c.retention.as_ref() {
            return Some(format!("retention artifact: {:?}", c.retention));
        }
        c.participants
            .iter()
            .find(|p| !self.participants.contains_key(&p.build))
            .map(|p| format!("cleanup artifact: {:?}", p.build))
    }
    pub fn new(
        profile: Identity,
        policy: Rc<dyn Profile>,
        targets: Identity,
        validator: Rc<dyn TargetValidator>,
        codec: Rc<dyn TargetCodec>,
    ) -> Result<Self> {
        identity(&profile)?;
        identity(&targets)?;
        Ok(Self {
            profile: (profile, policy),
            targets: (targets, validator, codec),
            retention: None,
            participants: BTreeMap::new(),
            port: Rc::new(RefCell::new(None)),
        })
    }
    pub fn retention(
        &mut self,
        build: Identity,
        retention: Rc<dyn CallbackRetention>,
        restored: Rc<dyn RetentionRestore>,
    ) -> Result<()> {
        identity(&build)?;
        if self.retention.is_some() {
            return Err(Error::Duplicate);
        }
        self.retention = Some((build, retention, restored));
        Ok(())
    }
    pub fn participant(
        &mut self,
        build: Identity,
        factory: impl Fn(Option<CleanupIdentity>) -> Result<Rc<dyn CleanupParticipant>> + 'static,
    ) -> Result<()> {
        identity(&build)?;
        if self.participants.contains_key(&build) || self.participants.len() >= 65536 {
            return Err(Error::Duplicate);
        }
        self.participants.insert(build, Box::new(factory));
        Ok(())
    }
    /// Enroll this port in the independent native artifact factory before D3 load.
    /// Until rebuild completes, it refuses every call without consuming work.
    pub fn native_port(&self) -> NativePort {
        let slot = self.port.clone();
        Rc::new(move |bytes, view| {
            let service = slot
                .try_borrow()
                .map_err(|_| Error::NativeRefusal)?
                .clone()
                .ok_or(Error::NativeRefusal)?;
            service.dispatch(bytes, view)
        })
    }
    fn matches(&self, c: &Checkpoint) -> Result<()> {
        if self.profile.0 != c.profile
            || self.targets.0 != c.targets
            || self.retention.as_ref().map(|r| &r.0) != c.retention.as_ref()
            || c.participants
                .iter()
                .any(|p| !self.participants.contains_key(&p.build))
        {
            return Err(Error::Authority);
        }
        Ok(())
    }
}
pub struct Validated {
    checkpoint: Checkpoint,
}
impl Checkpoint {
    pub fn validate(
        &self,
        kernel: &KernelSnapshot,
        catalog: &RestoreCatalog,
        authority: &dyn TimerAuthority,
        limits: &Limits,
    ) -> Result<Validated> {
        self.structure(kernel, limits)?;
        catalog.matches(self)?;
        authority.approve(self, kernel)?;
        // Whole graph/catalog/context approval precedes all avoidable domain calls.
        for row in &self.tasks {
            catalog.profile.1.validate(&row.deadline, &row.residency)?;
            catalog.targets.2.validate_old(&row.targets)?;
        }
        Ok(Validated {
            checkpoint: self.clone(),
        })
    }
    fn structure(&self, k: &KernelSnapshot, l: &Limits) -> Result<()> {
        if self.version != 2
            || self.namespace == 0
            || self.namespace_high_water < self.namespace
            || self.namespace_high_water == u64::MAX
            || self.kernel_namespace != k.source_namespace
            || self.next_serial == 0
            || self.limits.records == 0
            || self.limits.wire_bytes == 0
            || self.tasks.len() > self.limits.records.min(l.records).min(65536)
            || self.occurrences.len() > self.limits.participants.min(l.participants).min(65536)
            || self.participants.len() > self.occurrences.len()
            || self
                .occurrences
                .iter()
                .any(|id| *id >= self.participants.len())
            || (0..self.participants.len()).any(|id| !self.occurrences.contains(&id))
            || self.tasks.windows(2).any(|w| w[0].serial >= w[1].serial)
        {
            return Err(Error::Limit);
        }
        identity(&self.profile)?;
        identity(&self.targets)?;
        if let Some(id) = &self.retention {
            identity(id)?;
        }
        if self.limits.records > l.records
            || self.limits.targets > l.targets
            || self.limits.participants > l.participants
            || self.limits.field_bytes > l.field_bytes
            || self.limits.payload_bytes > l.payload_bytes
            || self.limits.context_bytes > l.context_bytes
            || self.limits.result_bytes > l.result_bytes
            || self.limits.wire_bytes > l.wire_bytes
        {
            return Err(Error::Limit);
        }
        check_address(l, &self.queue)?;
        // Registry admission independently hashes/approves the exact native
        // artifact; TimerAuthority independently approves its configuration.
        let module = k
            .registry
            .modules
            .iter()
            .find(|m| m.id == self.service && m.artifact == self.artifact)
            .ok_or(Error::InvalidHandle)?;
        let manifest = Manifest {
            id: self.capability.provider.clone(),
            exports: BTreeMap::from([(
                "queue".into(),
                Endpoint {
                    major: 1,
                    access: Access::Write,
                    max_request: self.limits.wire_bytes,
                    max_reply: self.limits.wire_bytes,
                },
            )]),
            dependencies: vec![],
            extension_points: BTreeSet::new(),
            extension: None,
        };
        if module.executable != nethacked_kernel::persistence::Executable::NativeV1
            || module.manifest != manifest
            || self.capability.endpoint != "queue"
            || self.capability.major != 1
        {
            return Err(Error::Authority);
        }
        let mut callbacks = BTreeSet::new();
        let mut operations = BTreeSet::new();
        let mut edges = self.occurrences.len();
        let mut cleanup_ids = BTreeSet::new();
        for p in &self.participants {
            identity(&p.build)?;
            if p.cleanup
                .is_some_and(|id| !k.cleanup_history.contains(&id) || !cleanup_ids.insert(id))
            {
                return Err(Error::Authority);
            }
        }
        for r in &self.tasks {
            let keys = match &r.ceiling.scope {
                Scope::All => 0,
                Scope::Keys(keys) => keys.len(),
            };
            edges = edges
                .checked_add(r.targets.len())
                .and_then(|v| v.checked_add(keys))
                .and_then(|v| v.checked_add(4))
                .ok_or(Error::Limit)?;
            r.provenance.check(k)?;
            r.status.restore()?;
            if r.serial == 0
                || r.serial >= self.next_serial
                || r.provenance.service.module != self.service
                || r.provenance.service.artifact != self.artifact
                || r.provenance.capability != self.capability
                || r.provenance.service.endpoint != "queue"
                || r.payload.len() > self.limits.payload_bytes.min(l.payload_bytes)
                || r.targets.len() > self.limits.targets.min(l.targets)
                || r.deadline.len() > self.limits.field_bytes.min(l.field_bytes)
                || r.residency.len() > self.limits.field_bytes.min(l.field_bytes)
                || r.reference == Some(0)
                || r.reference.is_some() && self.retention.is_none()
            {
                return Err(Error::Limit);
            }
            check_ceiling(l, &r.ceiling)?;
            if r.ceiling.access != Access::Write || !r.ceiling.scope.allows(&self.queue) {
                return Err(Error::Authority);
            }
            let mut targets = BTreeSet::new();
            for target in &r.targets {
                check_address(l, &target.address)?;
                if target.generation == 0
                    || !targets.insert(&target.address)
                    || !r.ceiling.scope.allows(&target.address)
                {
                    return Err(Error::Authority);
                }
            }
            if let SavedStatus::Completed(v) | SavedStatus::Rejected(v) = &r.status {
                if v.len() > self.limits.result_bytes.min(l.result_bytes) {
                    return Err(Error::Buffer);
                }
            }
            match &r.status {
                SavedStatus::Pending => {
                    let id = r.callback.ok_or(Error::InvalidHandle)?;
                    let p = &k
                        .callbacks
                        .iter()
                        .find(|c| c.id == id)
                        .ok_or(Error::InvalidHandle)?
                        .pipeline;
                    if !callbacks.insert(id)
                        || r.lease.is_some()
                        || r.invocation.is_some()
                        || r.cleanup_pending
                        || p.ceiling != r.ceiling
                        || p.origin
                            .as_ref()
                            .is_none_or(|o| !history_origin(&r.provenance, o))
                        || self.retention.is_some() != r.reference.is_some()
                    {
                        return Err(Error::Authority);
                    }
                }
                SavedStatus::Cancelled => {
                    if r.callback.is_some()
                        || r.lease.is_some()
                        || r.invocation.is_some()
                        || r.cleanup_pending
                        || r.reference.is_some()
                    {
                        return Err(Error::Authority);
                    }
                }
                _ => {
                    if r.callback.is_some()
                        || r.lease.is_none()
                        || r.invocation.is_none()
                        || r.cleanup_pending
                            && !matches!(
                                r.status,
                                SavedStatus::Invocation(_)
                                    | SavedStatus::InvalidOutcome
                                    | SavedStatus::Cleanup
                            )
                        || r.reference.is_some() && !r.cleanup_pending
                    {
                        return Err(Error::Authority);
                    }
                    if r.cleanup_pending && self.retention.is_some() != r.reference.is_some() {
                        return Err(Error::Authority);
                    }
                    let op = r.invocation.as_ref().ok_or(Error::InvalidHandle)?;
                    let p = &k
                        .leases
                        .iter()
                        .find(|v| Some(v.id) == r.lease)
                        .ok_or(Error::InvalidHandle)?
                        .pipeline;
                    let cleanups: BTreeSet<_> =
                        self.participants.iter().filter_map(|p| p.cleanup).collect();
                    if !k.operations.contains(op)
                        || !operations.insert(op.operation)
                        || Some(op.pipeline) != r.lease
                        || op.phase != PHASE
                        || op.checkpoint != 0
                        || op.cleanup.iter().copied().collect::<BTreeSet<_>>() != cleanups
                        || p.ceiling != r.ceiling
                        || p.origin
                            .as_ref()
                            .is_none_or(|o| !history_origin(&r.provenance, o))
                    {
                        return Err(Error::Authority);
                    }
                }
            }
        }
        if edges > 262144 {
            return Err(Error::Limit);
        }
        self.to_bytes(64 * 1024 * 1024)?;
        Ok(())
    }
}
fn history_origin(h: &TimerHistory, o: &nethacked_kernel::persistence::SavedOrigin) -> bool {
    h.caller == o.caller
        && h.service == o.service
        && h.capability == o.capability
        && h.slot == o.slot
        && h.source == o.source
}
fn remap_authority(a: &Authority, m: &RestoredKernel) -> Result<Authority> {
    Ok(Authority {
        access: a.access,
        scope: match &a.scope {
            Scope::All => Scope::All,
            Scope::Keys(keys) => Scope::Keys(
                keys.iter()
                    .map(|r| m.execution_resource(r).cloned())
                    .collect::<Result<_>>()?,
            ),
        },
    })
}
impl Validated {
    pub fn rebuild(
        self,
        kernel: &Kernel,
        mapped: &RestoredKernel,
        catalog: &RestoreCatalog,
    ) -> Result<Service> {
        let c = &self.checkpoint;
        catalog.matches(c)?;
        if mapped.execution_namespace() != c.kernel_namespace
            || catalog
                .port
                .try_borrow()
                .map_err(|_| Error::NativeRefusal)?
                .is_some()
        {
            return Err(Error::Authority);
        }
        mapped.execution_live(kernel)?;
        let claim = mapped.enroll_timers(c.service)?;
        let module = *mapped
            .modules()
            .get(&c.service)
            .ok_or(Error::InvalidHandle)?;
        if kernel.artifact(module)? != c.artifact {
            return Err(Error::Authority);
        }
        if let Some((_, _, retention)) = &catalog.retention {
            if !retention
                .destination()
                .same_destination(&mapped.execution_destination())
            {
                return Err(Error::Authority);
            }
            for r in &c.tasks {
                retention.check(r, mapped)?;
            }
        }
        // Validate every destination credential before remapping or factories.
        for r in &c.tasks {
            if let Some(id) = r.callback {
                let callback = mapped.callbacks().get(&id).ok_or(Error::InvalidHandle)?;
                check_provenance(&r.provenance, callback.provenance(), mapped)?;
                kernel.preflight_callback(
                    callback,
                    &[],
                    &remap_authority(&r.ceiling, mapped)?,
                    PHASE,
                    &c.participants
                        .iter()
                        .filter_map(|p| p.cleanup)
                        .map(|id| mapped.cleanup().get(&id).copied().ok_or(Error::Authority))
                        .collect::<Result<Vec<_>>>()?,
                )?;
            }
            if let Some(op) = &r.invocation {
                let binding = mapped.execution_source(op)?;
                check_provenance(
                    &r.provenance,
                    kernel
                        .callback_provenance(binding.pipeline())?
                        .ok_or(Error::Authority)?,
                    mapped,
                )?;
            }
        }
        let queue = mapped.execution_resource(&c.queue)?.clone();
        let mut tasks = BTreeMap::new();
        let namespace = issue_namespace_after(&NEXT_QUEUE, c.namespace_high_water)?;
        for r in &c.tasks {
            let targets = catalog.targets.2.remap(&r.targets, mapped)?;
            let expected = r
                .targets
                .iter()
                .map(|s| {
                    Ok(Stamp {
                        address: mapped.execution_resource(&s.address)?.clone(),
                        generation: s.generation,
                        revision: s.revision,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            // Target codecs may remap owner generations, but must preserve order,
            // revision and the explicitly mapped resource reference.
            if targets.len() != expected.len()
                || targets.iter().zip(&expected).any(|(a, b)| {
                    a.address != b.address || a.revision != b.revision || a.generation == 0
                })
            {
                return Err(Error::Authority);
            }
            let callback = r
                .callback
                .map(|id| {
                    mapped
                        .callbacks()
                        .get(&id)
                        .cloned()
                        .ok_or(Error::InvalidHandle)
                })
                .transpose()?;
            let lease = r
                .lease
                .map(|id| {
                    mapped
                        .leases()
                        .get(&id)
                        .copied()
                        .ok_or(Error::InvalidHandle)
                })
                .transpose()?;
            let provenance = if let Some(cb) = &callback {
                cb.provenance().clone()
            } else if let Some(lease) = lease {
                kernel
                    .callback_provenance(lease)?
                    .cloned()
                    .ok_or(Error::Authority)?
            } else {
                mapped.terminal_timer_history(&r.provenance)?
            };
            let invocation = r
                .invocation
                .as_ref()
                .map(|op| mapped.execution_source(op))
                .transpose()?;
            let reference = r
                .reference
                .map(|id| {
                    catalog
                        .retention
                        .as_ref()
                        .ok_or(Error::Authority)?
                        .2
                        .reference(id)
                })
                .transpose()?;
            let id = TaskId {
                queue: namespace,
                serial: r.serial,
            };
            let history = if matches!(r.status, SavedStatus::Cancelled) {
                let mut h = r.provenance.clone();
                h.source.cleanup = h
                    .source
                    .cleanup
                    .iter()
                    .map(|id| mapped.execution_cleanup_identity(*id))
                    .collect::<Result<_>>()?;
                Some(h)
            } else {
                None
            };
            tasks.insert(
                id,
                Owned {
                    task: Task {
                        id,
                        provenance,
                        ceiling: remap_authority(&r.ceiling, mapped)?,
                        payload: r.payload.clone(),
                        targets,
                        deadline: r.deadline.clone(),
                        residency: r.residency.clone(),
                        status: r.status.restore()?,
                        invocation,
                        cleanup_pending: r.cleanup_pending,
                    },
                    callback,
                    lease,
                    reference,
                    history,
                },
            );
        }
        let mut allocations = vec![];
        for p in &c.participants {
            let cleanup = p
                .cleanup
                .map(|id| mapped.cleanup().get(&id).copied().ok_or(Error::Authority))
                .transpose()?;
            let participant = catalog.participants.get(&p.build).ok_or(Error::Authority)?(cleanup)?;
            if participant.identity()? != cleanup {
                return Err(Error::Authority);
            }
            if allocations.iter().any(|v| Rc::ptr_eq(v, &participant)) {
                return Err(Error::Authority);
            }
            allocations.push(participant);
        }
        let participants = c
            .occurrences
            .iter()
            .map(|id| allocations[*id].clone())
            .collect();
        let identities = c
            .participants
            .iter()
            .filter_map(|p| p.cleanup)
            .map(|id| mapped.cleanup().get(&id).copied().ok_or(Error::Authority))
            .collect::<Result<Vec<_>>>()?;
        let participant_identities = c
            .occurrences
            .iter()
            .map(|id| {
                c.participants[*id]
                    .cleanup
                    .map(|id| mapped.cleanup().get(&id).copied().ok_or(Error::Authority))
                    .transpose()
            })
            .collect::<Result<_>>()?;
        let service = Service(
            Rc::new(RefCell::new(Inner {
                limits: c.limits.clone(),
                queue,
                namespace,
                next_serial: c.next_serial,
                binding: Some((module, c.artifact, c.capability.clone())),
                profile: catalog.profile.1.clone(),
                validator: catalog.targets.1.clone(),
                validate_on_schedule: c.validate_on_schedule,
                participants,
                identities,
                participant_identities,
                tasks,
                active: None,
                retention: catalog.retention.as_ref().map(|v| v.1.clone()),
            })),
            Rc::new(Default::default()),
        );
        *catalog
            .port
            .try_borrow_mut()
            .map_err(|_| Error::NativeRefusal)? = Some(service.clone());
        if let Some((_, _, retention)) = &catalog.retention {
            retention.finish_rebuild();
        }
        claim.finish();
        Ok(service)
    }
}

fn check_provenance(h: &TimerHistory, p: &CallbackProvenance, m: &RestoredKernel) -> Result<()> {
    if m.modules().get(&h.caller.module) != Some(&p.caller.handle)
        || m.modules().get(&h.service.module) != Some(&p.service.handle)
        || h.caller.artifact != p.caller.artifact
        || h.caller.endpoint != p.caller.endpoint
        || h.service.artifact != p.service.artifact
        || h.service.endpoint != p.service.endpoint
        || h.capability != p.capability
        || h.slot != p.slot
        || m.execution_source(&h.source)? != p.source
    {
        return Err(Error::Authority);
    }
    Ok(())
}

#[cfg(test)]
#[path = "persistence_tests.rs"]
mod tests;

impl RestoreCatalog {
    pub fn capture_catalog(
        &self,
        checkpoint: &Checkpoint,
        service: &Service,
    ) -> Result<CaptureCatalog> {
        let mut capture = CaptureCatalog::new(
            service,
            checkpoint.profile.clone(),
            checkpoint.targets.clone(),
            checkpoint.retention.clone(),
        )?;
        let inner = service.0.try_borrow().map_err(|_| Error::Busy)?;
        let participants = inner.participants.clone();
        let references = inner
            .tasks
            .iter()
            .filter_map(|(id, owned)| owned.reference.as_ref().map(|r| (id.serial, r.clone())))
            .collect::<Vec<_>>();
        drop(inner);
        for (position, participant) in participants.iter().enumerate() {
            let row = checkpoint
                .participants
                .get(checkpoint.occurrences[position])
                .ok_or(Error::Authority)?;
            capture
                .participant(row.build.clone(), participant.clone())
                .map_err(|_| Error::Authority)?;
        }
        for (id, reference) in &references {
            let row = checkpoint
                .tasks
                .iter()
                .find(|r| r.serial == *id)
                .ok_or(Error::Authority)?;
            let serial = row.reference.ok_or(Error::Authority)?;
            if !capture.references.iter().any(|(n, _)| *n == serial) {
                capture
                    .reference(serial, reference.clone())
                    .map_err(|_| Error::Authority)?;
            }
        }
        Ok(capture)
    }
}

impl Checkpoint {
    pub fn preflight_source(&self, kernel: &KernelSnapshot) -> Result<()> {
        self.structure(kernel, &Limits::default())
    }
}
