//! Sealed transport of already prepared allocations. No preparation during load.
use super::*;
use crate::publication::{Accounting, Batch, TicketState};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StampRow {
    pub(super) owner: OwnerId,
    generation: u64,
    revision: u64,
    schema: [u8; 32],
    module: BindingRow,
}
impl StampRow {
    fn capture(s: &Stamp, i: &ExecutionCaptureIndex<'_>) -> Result<Self> {
        Ok(Self {
            owner: s.owner,
            generation: s.generation,
            revision: s.revision,
            schema: s.schema,
            module: save_binding(&s.module, i)?,
        })
    }
    fn restore(&self, r: &Remap, e: &ExecutionRemap<'_>) -> Result<Stamp> {
        Ok(Stamp {
            owner: r.owner(self.owner)?,
            generation: self.generation,
            revision: self.revision,
            schema: self.schema,
            module: e.row_binding(&self.module)?,
        })
    }
    fn validate(&self, c: &ExecutionCheckpoint) -> Result<()> {
        let o = c
            .state
            .owners
            .iter()
            .find(|o| o.old == self.owner && o.live)
            .ok_or(Error::Stale)?;
        let current = c
            .current
            .iter()
            .find(|o| o.owner == self.owner)
            .ok_or(Error::Stale)?;
        if self.module.module != current.module {
            return Err(Error::Schema);
        }
        if self.generation != o.generation
            || self.schema != o.schema
            || self.revision > o.revision
            || self.module.artifact != o.module_artifact
        {
            return Err(Error::Stale);
        }
        validate_binding(&self.module)
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservationRow {
    stamp: StampRow,
    name: String,
    data: Vec<u8>,
    caller: u64,
    artifact: [u8; 32],
    permit: PermitRow,
    dependencies: Vec<ObservationRow>,
    pin: u64,
    revision: u64,
    helper: Option<usize>,
    source: Option<SavedOperation>,
    retained_execution: bool,
    #[cfg(feature = "recorded-inputs")]
    recorded: Vec<usize>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CandidateRow {
    serial: u64,
    pub(super) stamp: StampRow,
    caller: u64,
    artifact: [u8; 32],
    source: SavedOperation,
    command: String,
    capability: PermitRow,
    behavior: Vec<BindingRow>,
    next_digest: [u8; 32],
    request_digest: [u8; 32],
    pub(super) build: CodecIdentity,
    state_codec: CodecIdentity,
    backend: BackendIdentity,
    pub(super) policy: QueryPolicy,
    permit: Option<PermitRow>,
    payload: Vec<u8>,
    references: Vec<StateReference>,
    events: Vec<Vec<u8>>,
    reply: Vec<u8>,
    dependencies: Vec<ObservationRow>,
    pins: Vec<u64>,
    helper: Option<usize>,
    retained_bytes: usize,
    #[cfg(feature = "recorded-inputs")]
    recorded: Vec<usize>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BatchRow {
    ticket: u64,
    coordinator: PermitRow,
    provider: BindingRow,
    caller: u64,
    artifact: [u8; 32],
    source: SavedOperation,
    candidates: Vec<u64>,
    writes: Vec<OwnerId>,
    order: Vec<OwnerId>,
    policy: [u8; 32],
    accounting: ArtifactIdentity,
    readonly: Vec<ObservationRow>,
    pin: u64,
}
/// Pointer-bound capture registration. Accounting purity, fidelity, bounded work,
/// no aliases and effect-free construction/drop are explicit host premises.
#[derive(Default)]
pub struct AccountingCaptureCatalog {
    entries: Vec<(ArtifactIdentity, [u8; 32], Rc<dyn Accounting>)>,
}
impl AccountingCaptureCatalog {
    pub fn register(
        &mut self,
        id: ArtifactIdentity,
        profile: [u8; 32],
        value: Rc<dyn Accounting>,
    ) -> Result<()> {
        validate_identity(&id)?;
        bound(self.entries.len() + 1, 65_536)?;
        if self
            .entries
            .iter()
            .any(|(i, p, a)| i == &id || (*p == profile && Rc::ptr_eq(a, &value)))
        {
            return Err(Error::Invalid);
        }
        self.entries.push((id, profile, value));
        Ok(())
    }
}
type AccountingFactory = Box<dyn Fn() -> Result<Rc<dyn Accounting>>>;
/// Independent allowlist. Factories must allocate fresh pure accounting objects.
#[derive(Default)]
pub struct AccountingRestoreCatalog {
    entries: Vec<(ArtifactIdentity, [u8; 32], AccountingFactory)>,
}
impl AccountingRestoreCatalog {
    pub fn allow(
        &mut self,
        id: ArtifactIdentity,
        profile: [u8; 32],
        factory: impl Fn() -> Result<Rc<dyn Accounting>> + 'static,
    ) -> Result<()> {
        validate_identity(&id)?;
        bound(self.entries.len() + 1, 65_536)?;
        if self.entries.iter().any(|(i, _, _)| i == &id) {
            return Err(Error::Invalid);
        }
        self.entries.push((id, profile, Box::new(factory)));
        Ok(())
    }
    fn resolve(&self, row: &BatchRow) -> Result<&AccountingFactory> {
        self.entries
            .iter()
            .find(|(i, p, _)| i == &row.accounting && p == &row.policy)
            .map(|(_, _, f)| f)
            .ok_or(Error::Authority)
    }
}
fn validate_identity(i: &ArtifactIdentity) -> Result<()> {
    if i.name.is_empty() || i.name.len() > 256 || i.version == 0 {
        return Err(Error::Schema);
    }
    Ok(())
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PendingCheckpoint {
    pub(super) candidates: Vec<CandidateRow>,
    #[serde(default)]
    retired_candidates: Option<Vec<(u64, u64)>>,
    #[serde(default)]
    retired_tickets: Option<Vec<(u64, u64)>>,
    batches: Vec<BatchRow>,
    #[cfg(feature = "recorded-inputs")]
    inputs: Option<nethacked_inputs::persistence::Checkpoint>,
    #[cfg(feature = "recorded-inputs")]
    observations: Vec<nethacked_inputs::persistence::ObservationId>,
}
#[derive(Default)]
pub(super) struct PendingCapture {
    checkpoint: PendingCheckpoint,
    #[cfg(feature = "recorded-inputs")]
    recorded: Vec<nethacked_inputs::Observation>,
}
impl PendingCapture {
    #[cfg(feature = "recorded-inputs")]
    fn recorded(&mut self, values: &[nethacked_inputs::Observation]) -> Vec<usize> {
        values
            .iter()
            .map(|v| {
                // Authenticity is checked by the input-owned capture before finish.
                let id = self.recorded.len();
                self.recorded.push(v.clone());
                id
            })
            .collect()
    }
    fn observation(
        &mut self,
        o: &Observation,
        i: &ExecutionCaptureIndex<'_>,
    ) -> Result<ObservationRow> {
        Ok(ObservationRow {
            stamp: StampRow::capture(&o.stamp, i)?,
            name: o.name.clone(),
            data: o.data.clone(),
            caller: i.module(o.caller)?,
            artifact: o.caller_artifact,
            permit: save_permit(&o.permit, i)?,
            dependencies: o
                .dependencies
                .iter()
                .map(|d| self.observation(d, i))
                .collect::<Result<_>>()?,
            pin: i.pin(&o.code)?,
            revision: o.behavior_revision,
            helper: o.helper.as_ref().map(|h| i.pure(h)).transpose()?,
            source: o
                .persistence_execution
                .as_ref()
                .map(|s| i.source(s))
                .transpose()?,
            retained_execution: o.execution.is_some(),
            #[cfg(feature = "recorded-inputs")]
            recorded: self.recorded(&o.recorded),
        })
    }
    pub(super) fn validate_capture(&mut self, host: &Host) -> Result<()> {
        let r = host.0.registry.borrow();
        let stages = r
            .staging
            .iter()
            .filter_map(|(id, w)| w.upgrade().map(|_| *id))
            .collect::<BTreeSet<_>>();
        let saved = self
            .checkpoint
            .candidates
            .iter()
            .map(|c| c.serial)
            .collect::<BTreeSet<_>>();
        let tickets = r
            .tickets
            .iter()
            .filter_map(|(id, w)| w.upgrade().filter(|t| !t.consumed.get()).map(|_| *id))
            .collect::<BTreeSet<_>>();
        if stages != saved
            || tickets != self.checkpoint.batches.iter().map(|b| b.ticket).collect()
            || r.pending != saved.len()
        {
            return Err(Error::Unsupported);
        }
        self.checkpoint.retired_candidates = Some(retired_ranges(r.next_candidate, &saved)?);
        self.checkpoint.retired_tickets = Some(retired_ranges(
            host.0
                .publication
                .borrow()
                .next_ticket
                .checked_add(1)
                .ok_or(Error::Limit)?,
            &tickets,
        )?);
        Ok(())
    }
    pub(super) fn finish(mut self) -> Result<PendingCheckpoint> {
        #[cfg(feature = "recorded-inputs")]
        if self.recorded.len() != self.checkpoint.observations.len() {
            return Err(Error::Unsupported);
        }
        self.checkpoint.candidates.sort_by_key(|c| c.serial);
        self.checkpoint.batches.sort_by_key(|b| b.ticket);
        Ok(self.checkpoint)
    }
}
impl ExecutionRoots {
    #[cfg(feature = "recorded-inputs")]
    pub fn inputs(&mut self, service: &nethacked_inputs::Service) -> Result<()> {
        for op in service.persistence_operations()? {
            self.pending_operation(&op);
        }
        Ok(())
    }
    fn pending_operation(&mut self, op: &OperationBinding) {
        if !self.operations.contains(op) {
            self.operations.push(op.clone());
        }
    }
    fn pending_helper(&mut self, helper: &Option<PureCapture>) {
        if let Some(h) = helper {
            if !self.captures.iter().any(|c| execution_capture_alias(c, h)) {
                self.captures.push(h.clone());
            }
        }
    }
    pub fn observation(&mut self, observation: &Observation) -> Result<()> {
        bound(self.operations.len() + self.captures.len(), 65_536)?;
        if let Some(s) = &observation.execution {
            self.pending_operation(s);
        }
        self.pending_helper(&observation.helper);
        for d in &observation.dependencies {
            self.observation(d)?;
        }
        Ok(())
    }
    pub fn candidate<T>(&mut self, candidate: &Candidate<T>) -> Result<()> {
        self.stage(&candidate.stage)
    }
    pub(crate) fn stage<T>(&mut self, stage: &Stage<T>) -> Result<()> {
        let p = stage.prepared.try_borrow().map_err(|_| Error::Busy)?;
        let p = p.as_ref().ok_or(Error::Stale)?;
        self.pending_operation(&stage.receipt.operation);
        self.pending_helper(&p.helper);
        for d in &p.dependencies {
            self.observation(d)?;
        }
        Ok(())
    }
    pub fn batch(&mut self, batch: &Batch) -> Result<()> {
        self.pending_operation(&batch.operation);
        for d in &*batch.state.readonly.try_borrow().map_err(|_| Error::Busy)? {
            self.observation(d)?;
        }
        for c in &batch.candidates {
            c.stage.persistence_enroll(self)?;
        }
        Ok(())
    }
}
impl ExecutionCapture<'_> {
    pub fn candidate<T: 'static>(
        &mut self,
        candidate: &Candidate<T>,
        codec: &dyn PersistenceCodec<T>,
        catalog: &CaptureCatalog<T>,
    ) -> Result<u64> {
        self.capture_stage(&candidate.stage, codec, catalog)
    }
    pub fn batch_candidate<T: 'static>(
        &mut self,
        batch: &Batch,
        serial: u64,
        codec: &dyn PersistenceCodec<T>,
        catalog: &CaptureCatalog<T>,
    ) -> Result<u64> {
        if !Rc::ptr_eq(&batch.host.0, &self.state.host.0) {
            return Err(Error::Invalid);
        }
        let stage = batch
            .candidates
            .iter()
            .find(|c| c.stage.receipt().serial == serial)
            .ok_or(Error::Stale)?
            .stage
            .as_any()
            .downcast_ref::<Stage<T>>()
            .ok_or(Error::Schema)?;
        self.capture_stage(stage, codec, catalog)
    }
    fn capture_stage<T: 'static>(
        &mut self,
        s: &Stage<T>,
        codec: &dyn PersistenceCodec<T>,
        catalog: &CaptureCatalog<T>,
    ) -> Result<u64> {
        if !Rc::ptr_eq(&s.host.0, &self.state.host.0)
            || !s.active.get()
            || self
                .pending
                .checkpoint
                .candidates
                .iter()
                .any(|c| c.serial == s.receipt.serial)
        {
            return Err(Error::Invalid);
        }
        let borrow = s.prepared.try_borrow().map_err(|_| Error::Busy)?;
        let p = borrow.as_ref().ok_or(Error::Stale)?;
        let r = &s.receipt;
        let identity = codec.identity();
        identity.validate()?;
        if identity.schema_digest != r.stamp.schema {
            return Err(Error::Schema);
        }
        let build = catalog.identify(
            &p.persistence_backend,
            &p.persistence_policy,
            r.stamp.schema,
        )?;
        codec.validate_state(&p.next)?;
        codec.validate_configuration(&p.next, &p.owner.metadata.ports, &p.persistence_policy)?;
        let payload = codec.encode_state(&p.next, self.state.host.0.limits.payload_bytes)?;
        bound(payload.len(), self.state.host.0.limits.payload_bytes)?;
        let references = codec.references(&p.next)?;
        for reference in &references {
            remap_for(&self.state.checkpoint, self.state.host.0.id).reference(reference)?;
        }
        let permit = match &p.persistence_backend {
            Backend::Wasm { binding, .. } => Some(save_permit(binding, self.index)?),
            _ => None,
        };
        let row = CandidateRow {
            serial: r.serial,
            stamp: StampRow::capture(&r.stamp, self.index)?,
            caller: self.index.module(r.caller)?,
            artifact: r.caller_artifact,
            source: self.index.source(&r.operation)?,
            command: r.command.clone(),
            capability: save_permit(&r.capability, self.index)?,
            behavior: r
                .behavior
                .iter()
                .map(|b| save_binding(b, self.index))
                .collect::<Result<_>>()?,
            next_digest: r.next_digest,
            request_digest: r.request_digest,
            build,
            state_codec: identity,
            backend: backend_identity(&p.persistence_backend)?,
            policy: p.persistence_policy.clone(),
            permit,
            payload,
            references,
            events: p.events.clone(),
            reply: p.reply.clone(),
            dependencies: p
                .dependencies
                .iter()
                .map(|d| self.pending.observation(d, self.index))
                .collect::<Result<_>>()?,
            pins: p
                .code
                .iter()
                .map(|pin| self.index.pin(pin).map_err(Error::from))
                .collect::<Result<_>>()?,
            helper: p.helper.as_ref().map(|h| self.index.pure(h)).transpose()?,
            retained_bytes: s.retained_bytes,
            #[cfg(feature = "recorded-inputs")]
            recorded: self.pending.recorded(&p.recorded),
        };
        self.pending.checkpoint.candidates.push(row);
        Ok(r.serial)
    }
    pub fn batch(&mut self, batch: &Batch, catalog: &AccountingCaptureCatalog) -> Result<u64> {
        if !Rc::ptr_eq(&batch.host.0, &self.state.host.0)
            || batch.state.consumed.get()
            || self
                .pending
                .checkpoint
                .batches
                .iter()
                .any(|b| b.ticket == batch.ticket)
        {
            return Err(Error::Invalid);
        }
        let mut entries = catalog
            .entries
            .iter()
            .filter(|(_, p, a)| *p == batch.policy && Rc::ptr_eq(a, &batch.accounting));
        let id = entries.next().ok_or(Error::Authority)?.0.clone();
        if entries.next().is_some() {
            return Err(Error::Invalid);
        }
        let readonly = batch.state.readonly.try_borrow().map_err(|_| Error::Busy)?;
        let code = batch.state.code.try_borrow().map_err(|_| Error::Busy)?;
        let row = BatchRow {
            ticket: batch.ticket,
            coordinator: save_permit(&batch.coordinator, self.index)?,
            provider: save_binding(&batch.provider, self.index)?,
            caller: self.index.module(batch.caller)?,
            artifact: batch.artifact,
            source: self.index.source(&batch.operation)?,
            candidates: batch
                .candidates
                .iter()
                .map(|c| c.stage.receipt().serial)
                .collect(),
            writes: batch.writes.clone(),
            order: batch.order.clone(),
            policy: batch.policy,
            accounting: id,
            readonly: readonly
                .iter()
                .map(|d| self.pending.observation(d, self.index))
                .collect::<Result<_>>()?,
            pin: self.index.pin(code.as_ref().ok_or(Error::Stale)?)?,
        };
        self.pending.checkpoint.batches.push(row);
        Ok(batch.ticket)
    }
    /// Capture authenticated observations through the actual ledger, after all
    /// candidates/batches have been enrolled. No sampling or policy transition.
    #[cfg(feature = "recorded-inputs")]
    pub fn inputs(&mut self, service: &nethacked_inputs::Service) -> Result<()> {
        if self.pending.checkpoint.inputs.is_some() {
            return Err(Error::Invalid);
        }
        let refs = self.pending.recorded.iter().collect::<Vec<_>>();
        let (checkpoint, ids) = service.capture_persistence(self.index, &refs)?;
        self.pending.checkpoint.inputs = Some(checkpoint);
        self.pending.checkpoint.observations = ids;
        Ok(())
    }
}
fn target(c: &ExecutionCheckpoint, stamp: &StampRow, name: &str) -> Resource {
    Resource {
        owner: format!("owners-{}", c.state.source_host),
        key: format!("{}.{}", stamp.owner.serial, name),
    }
}
fn publication_target(c: &ExecutionCheckpoint) -> Resource {
    Resource {
        owner: format!("owners-{}", c.state.source_host),
        key: "publication".into(),
    }
}
fn saved_chain(p: &PermitRow) -> Vec<nethacked_kernel::persistence::SavedBinding> {
    p.resolution
        .iter()
        .map(|b| nethacked_kernel::persistence::SavedBinding {
            module: b.module,
            artifact: b.artifact,
            endpoint: b.endpoint.clone(),
        })
        .collect()
}
fn validate_source(s: &SavedOperation, c: &ExecutionCheckpoint) -> Result<()> {
    if s.operation == 0
        || s.pipeline == 0
        || s.phase.len() > 256
        || !s.cleanup.contains(&c.cleanup)
        || s.cleanup.windows(2).any(|w| w[0] >= w[1])
    {
        return Err(Error::Authority);
    }
    Ok(())
}
#[cfg(feature = "recorded-inputs")]
#[allow(clippy::too_many_arguments)]
fn validate_recorded(
    ids: &[usize],
    c: &ExecutionCheckpoint,
    owner: OwnerId,
    name: &str,
    source: &SavedOperation,
    caller: u64,
    artifact: [u8; 32],
    count: &mut usize,
    bytes: &mut usize,
    l: &Limits,
) -> Result<()> {
    let current = c
        .current
        .iter()
        .find(|r| r.owner == owner)
        .ok_or(Error::Stale)?;
    let declarations = current.recorded.get(name).map_or(&[][..], Vec::as_slice);
    if declarations.len() != ids.len() {
        return Err(Error::Authority);
    }
    for (id, d) in ids.iter().zip(declarations) {
        let index = *c.pending.observations.get(*id).ok_or(Error::Authority)?;
        let input = c.pending.inputs.as_ref().ok_or(Error::Authority)?;
        let size = input.observation_binding(
            index,
            &nethacked_inputs::persistence::ObservationBinding {
                policy: &d.policy,
                version: d.version,
                provider: d.provider.module,
                provider_artifact: d.provider.artifact,
                caller,
                caller_artifact: artifact,
                operation: source,
                request: &d.request,
            },
        )?;
        *count = count.checked_add(1).ok_or(Error::Limit)?;
        *bytes = bytes.checked_add(size).ok_or(Error::Limit)?;
        bound(*count, l.observations)?;
        bound(*bytes, l.observation_bytes)?;
    }
    Ok(())
}
impl ObservationRow {
    fn validate(
        &self,
        c: &ExecutionCheckpoint,
        l: &Limits,
        count: &mut usize,
        bytes: &mut usize,
    ) -> Result<()> {
        *count = count.checked_add(1).ok_or(Error::Limit)?;
        *bytes = bytes.checked_add(self.data.len()).ok_or(Error::Limit)?;
        bound(*count, l.observations)?;
        bound(*bytes, l.observation_bytes)?;
        self.stamp.validate(c)?;
        let owner = c
            .state
            .owners
            .iter()
            .find(|o| o.old == self.stamp.owner)
            .ok_or(Error::Stale)?;
        let current = c
            .current
            .iter()
            .find(|o| o.owner == self.stamp.owner)
            .ok_or(Error::Stale)?;
        if owner.ports.get(&self.name).map(|p| p.access) != Some(Access::Read)
            || self.revision > current.behavior_revision
            || self.caller == 0
            || self.pin == 0
        {
            return Err(Error::Schema);
        }
        let mut edges = 0;
        validate_permit(&self.permit, l, &mut edges)?;
        if let Some(s) = &self.source {
            validate_source(s, c)?;
        }
        if self.helper.is_some() && self.source.is_none() {
            return Err(Error::Authority);
        }
        #[cfg(feature = "recorded-inputs")]
        validate_recorded(
            &self.recorded,
            c,
            self.stamp.owner,
            &self.name,
            self.source.as_ref().ok_or(Error::Authority)?,
            self.caller,
            self.artifact,
            count,
            bytes,
            l,
        )?;
        validate_dependency_edges(&owner.ports[&self.name], &self.dependencies)?;
        for d in &self.dependencies {
            d.validate(c, l, count, bytes)?;
        }
        Ok(())
    }
    fn kernel(&self, c: &ExecutionCheckpoint, k: &KernelSnapshot) -> Result<()> {
        let s = self.source.as_ref().ok_or(Error::Authority)?;
        k.execution_owner(
            s,
            self.caller,
            &self.permit.capability,
            &saved_chain(&self.permit),
            &target(c, &self.stamp, &self.name),
            Some(self.pin),
            Access::Read,
        )?;
        if let Some(h) = self.helper {
            let pure = k.pure_captures.get(h).ok_or(Error::Authority)?;
            k.execution_behavior(
                h,
                s,
                self.stamp.module.module,
                self.caller,
                pure.slot,
                &target(c, &self.stamp, &self.name),
                self.pin,
            )?;
        }
        for d in &self.dependencies {
            d.kernel(c, k)?;
        }
        Ok(())
    }
    fn preflight(&self, c: &ExecutionCheckpoint, e: &ExecutionRemap<'_>) -> Result<()> {
        let caller = e.module(self.caller)?;
        if e.kernel.artifact(caller)? != self.artifact {
            return Err(Error::Schema);
        }
        let s = self.source.as_ref().ok_or(Error::Authority)?;
        let p = e.permit(&self.permit)?;
        e.mapped.execution_owner(
            s,
            self.caller,
            &p.capability,
            &p.resolution,
            &target(c, &self.stamp, &self.name),
            Some(self.pin),
            Access::Read,
        )?;
        e.row_binding(&self.stamp.module)?;
        if let Some(h) = self.helper {
            e.mapped.pure_captures().get(h).ok_or(Error::Authority)?;
        }
        for d in &self.dependencies {
            d.preflight(c, e)?;
        }
        Ok(())
    }
    fn restore(&self, r: &ExecutionRestore, e: &ExecutionRemap<'_>) -> Result<Observation> {
        Ok(Observation {
            stamp: self.stamp.restore(&r.state.remap, e)?,
            name: self.name.clone(),
            data: self.data.clone(),
            caller: e.module(self.caller)?,
            caller_artifact: self.artifact,
            permit: e.permit(&self.permit)?,
            route: r.state.host.route(r.state.remap.owner(self.stamp.owner)?)?,
            dependencies: self
                .dependencies
                .iter()
                .map(|d| d.restore(r, e))
                .collect::<Result<_>>()?,
            code: e
                .mapped
                .pins()
                .get(&self.pin)
                .cloned()
                .ok_or(Error::Authority)?,
            behavior_revision: self.revision,
            helper: self
                .helper
                .map(|h| {
                    e.mapped
                        .pure_captures()
                        .get(h)
                        .cloned()
                        .ok_or(Error::Authority)
                })
                .transpose()?,
            execution: if self.retained_execution {
                self.source
                    .as_ref()
                    .map(|s| e.mapped.execution_source(s))
                    .transpose()?
            } else {
                None
            },
            persistence_execution: self
                .source
                .as_ref()
                .map(|s| e.mapped.execution_source(s))
                .transpose()?,
            #[cfg(feature = "recorded-inputs")]
            recorded: r.pending.recorded(&self.recorded, &r.checkpoint.pending)?,
        })
    }
}
// Match the ordered declaration, including edges absent from incoming evidence.
// Recursive rows remain bounded by the shared observation count and byte limits.
fn validate_dependency_edges(port: &Port, rows: &[ObservationRow]) -> Result<()> {
    if port.dependencies.len() != rows.len() {
        return Err(Error::Schema);
    }
    let mut seen = BTreeSet::new();
    for (declared, row) in port.dependencies.iter().zip(rows) {
        if row.stamp.owner != declared.owner
            || row.name != declared.query
            || row.stamp.schema != declared.schema
            || !seen.insert((row.stamp.owner, &row.name))
        {
            return Err(Error::Schema);
        }
    }
    Ok(())
}
impl PendingCheckpoint {
    pub(super) fn validate(
        &self,
        c: &ExecutionCheckpoint,
        l: &Limits,
        p: &PublicationLimits,
    ) -> Result<()> {
        bound(self.candidates.len(), l.candidates)?;
        bound(self.batches.len(), p.transactions)?;
        let mut serials = BTreeSet::new();
        let mut previous = 0;
        let mut retained = 0usize;
        for row in &self.candidates {
            if row.serial <= previous
                || row.serial >= c.state.next_candidate
                || !serials.insert(row.serial)
            {
                return Err(Error::Invalid);
            }
            previous = row.serial;
            row.stamp.validate(c)?;
            validate_source(&row.source, c)?;
            row.build.validate()?;
            row.state_codec.validate()?;
            if row.build.schema_digest != row.stamp.schema
                || row.state_codec.schema_digest != row.stamp.schema
                || row.caller == 0
                || row.pins.len() != if row.backend.wasm { 3 } else { 2 }
                || row.pins.contains(&0)
            {
                return Err(Error::Schema);
            }
            if row.backend.wasm != row.permit.is_some() || row.helper.is_some() && !row.backend.wasm
            {
                return Err(Error::Schema);
            }
            let owner = c
                .state
                .owners
                .iter()
                .find(|o| o.old == row.stamp.owner)
                .ok_or(Error::Stale)?;
            if owner.ports.get(&row.command).map(|p| p.access) != Some(Access::Write) {
                return Err(Error::Schema);
            }
            validate_dependency_edges(&owner.ports[&row.command], &row.dependencies)?;
            bound(row.payload.len(), l.payload_bytes)?;
            bound(row.reply.len(), l.reply_bytes)?;
            bound(row.events.len(), l.events)?;
            let event_bytes = row
                .events
                .iter()
                .try_fold(0usize, |n, e| n.checked_add(e.len()).ok_or(Error::Limit))?;
            bound(event_bytes, l.event_bytes)?;
            bound(row.references.len(), l.observations)?;
            for reference in &row.references {
                remap_for(&c.state, c.state.source_host).reference(reference)?;
            }
            let mut edges = 0;
            validate_permit(&row.capability, l, &mut edges)?;
            if let Some(permit) = &row.permit {
                validate_permit(permit, l, &mut edges)?;
                if permit.resolution != row.behavior
                    || permit
                        .resolution
                        .iter()
                        .map(|b| (b.artifact, b.endpoint.clone()))
                        .collect::<Vec<_>>()
                        != row.backend.helpers
                {
                    return Err(Error::Schema);
                }
            } else if row.behavior != row.capability.resolution {
                return Err(Error::Schema);
            }
            let mut count = 0;
            let mut bytes = 0;
            for o in &row.dependencies {
                o.validate(c, l, &mut count, &mut bytes)?;
            }
            #[cfg(feature = "recorded-inputs")]
            validate_recorded(
                &row.recorded,
                c,
                row.stamp.owner,
                &row.command,
                &row.source,
                row.caller,
                row.artifact,
                &mut count,
                &mut bytes,
                l,
            )?;
            retained = retained
                .checked_add(row.retained_bytes)
                .ok_or(Error::Limit)?;
        }
        bound(retained, p.retained_bytes)?;
        validate_retired(&self.retired_candidates, c.state.next_candidate, &serials)?;
        let mut tickets = BTreeSet::new();
        let mut previous = 0;
        for row in &self.batches {
            if row.ticket <= previous
                || row.ticket > c.state.history.next_ticket
                || !tickets.insert(row.ticket)
                || c.state
                    .history
                    .records
                    .iter()
                    .any(|h| h.ticket == row.ticket)
            {
                return Err(Error::Invalid);
            }
            previous = row.ticket;
            validate_identity(&row.accounting)?;
            validate_source(&row.source, c)?;
            bound(row.candidates.len(), p.participants)?;
            let candidates = row
                .candidates
                .iter()
                .map(|id| {
                    self.candidates
                        .iter()
                        .find(|s| s.serial == *id)
                        .ok_or(Error::Stale)
                })
                .collect::<Result<Vec<_>>>()?;
            let owners = candidates.iter().map(|s| s.stamp.owner).collect::<Vec<_>>();
            let mut order = row.order.clone();
            order.sort();
            if owners.is_empty()
                || owners.windows(2).any(|w| w[0] >= w[1])
                || owners != row.writes
                || owners != order
                || candidates.iter().any(|s| {
                    s.source != row.source || s.caller != row.caller || s.artifact != row.artifact
                })
            {
                return Err(Error::Invalid);
            }
            if row.caller == 0 || row.pin == 0 {
                return Err(Error::Authority);
            }
            validate_binding(&row.provider)?;
            let mut edges = 0;
            validate_permit(&row.coordinator, l, &mut edges)?;
            let mut count = 0;
            let mut bytes = 0;
            for o in &row.readonly {
                o.validate(c, l, &mut count, &mut bytes)?;
            }
        }
        validate_retired(
            &self.retired_tickets,
            c.state
                .history
                .next_ticket
                .checked_add(1)
                .ok_or(Error::Limit)?,
            &tickets,
        )?;
        #[cfg(feature = "recorded-inputs")]
        {
            bound(self.observations.len(), 65_536)?;
            for id in &self.observations {
                if !self
                    .inputs
                    .as_ref()
                    .ok_or(Error::Authority)?
                    .contains_observation(*id)
                {
                    return Err(Error::Authority);
                }
            }
        }
        Ok(())
    }
    pub(super) fn kernel(&self, c: &ExecutionCheckpoint, k: &KernelSnapshot) -> Result<()> {
        #[cfg(feature = "recorded-inputs")]
        if let Some(input) = &self.inputs {
            input.validate_checkpoint(&nethacked_inputs::Limits::default(), k)?;
        }
        for row in &self.candidates {
            k.execution_owner(
                &row.source,
                row.caller,
                &row.capability.capability,
                &saved_chain(&row.capability),
                &target(c, &row.stamp, &row.command),
                row.pins.last().copied(),
                Access::Write,
            )?;
            let current = c
                .current
                .iter()
                .find(|o| o.owner == row.stamp.owner)
                .ok_or(Error::Stale)?;
            if let Some(h) = row.helper {
                k.execution_behavior(
                    h,
                    &row.source,
                    current.module,
                    row.caller,
                    row.backend.slot,
                    &target(c, &row.stamp, &row.command),
                    *row.pins.last().ok_or(Error::Authority)?,
                )?;
            }
            for id in &row.pins {
                k.pins
                    .iter()
                    .find(|p| p.id == *id)
                    .ok_or(Error::Authority)?;
            }
            for o in &row.dependencies {
                o.kernel(c, k)?;
            }
        }
        for row in &self.batches {
            k.execution_owner(
                &row.source,
                row.caller,
                &row.coordinator.capability,
                &saved_chain(&row.coordinator),
                &publication_target(c),
                Some(row.pin),
                Access::Write,
            )?;
            for o in &row.readonly {
                o.kernel(c, k)?;
            }
        }
        Ok(())
    }
    pub(super) fn preflight(
        &self,
        c: &ExecutionCheckpoint,
        kernel: &Kernel,
        mapped: &RestoredKernel,
        _r: &Remap,
    ) -> Result<()> {
        mapped.execution_live(kernel)?;
        let e = ExecutionRemap { kernel, mapped };
        for row in &self.candidates {
            let caller = e.module(row.caller)?;
            if kernel.artifact(caller)? != row.artifact {
                return Err(Error::Schema);
            }
            let permit = e.permit(&row.capability)?;
            mapped.execution_record(
                &row.source,
                row.caller,
                &permit.capability,
                &permit.resolution,
                &target(c, &row.stamp, &row.command),
                row.pins.last().copied(),
            )?;
            for b in &row.behavior {
                e.row_binding(b)?;
            }
            for id in &row.pins {
                mapped.pins().get(id).ok_or(Error::Authority)?;
            }
            if let Some(h) = row.helper {
                let current = c
                    .current
                    .iter()
                    .find(|o| o.owner == row.stamp.owner)
                    .ok_or(Error::Stale)?;
                mapped.execution_behavior(
                    h,
                    &row.source,
                    current.module,
                    row.caller,
                    row.backend.slot,
                    &target(c, &row.stamp, &row.command),
                    *row.pins.last().ok_or(Error::Authority)?,
                )?;
            }
            for o in &row.dependencies {
                o.preflight(c, &e)?;
            }
        }
        for row in &self.batches {
            if kernel.artifact(e.module(row.caller)?)? != row.artifact {
                return Err(Error::Schema);
            }
            let permit = e.permit(&row.coordinator)?;
            e.row_binding(&row.provider)?;
            mapped.execution_record(
                &row.source,
                row.caller,
                &permit.capability,
                &permit.resolution,
                &publication_target(c),
                Some(row.pin),
            )?;
            for o in &row.readonly {
                o.preflight(c, &e)?;
            }
        }
        Ok(())
    }
}
#[derive(Default)]
pub(super) struct PendingRestore {
    #[cfg(feature = "recorded-inputs")]
    pub(super) destination: Option<nethacked_kernel::persistence::ExecutionDestination>,
    stages: BTreeMap<u64, Weak<dyn Staged>>,
    accounting_approved: bool,
    accounting: Vec<(ArtifactIdentity, Rc<dyn Accounting>)>,
    tickets: BTreeMap<u64, Weak<TicketState>>,
    #[cfg(feature = "recorded-inputs")]
    inputs: Option<nethacked_inputs::persistence::Restored>,
}
impl PendingRestore {
    pub(super) fn new(_mapped: &RestoredKernel) -> Self {
        Self {
            #[cfg(feature = "recorded-inputs")]
            destination: Some(_mapped.execution_destination()),
            ..Self::default()
        }
    }
    #[cfg(feature = "recorded-inputs")]
    fn recorded(
        &self,
        ids: &[usize],
        c: &PendingCheckpoint,
    ) -> Result<Vec<nethacked_inputs::Observation>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let input = self.inputs.as_ref().ok_or(Error::Authority)?;
        ids.iter()
            .map(|id| {
                input
                    .observation(*c.observations.get(*id).ok_or(Error::Authority)?)
                    .map_err(Error::from)
            })
            .collect()
    }
    pub(super) fn validate_finish(&self, c: &PendingCheckpoint) -> Result<()> {
        if self.stages.keys().copied().collect::<BTreeSet<_>>()
            != c.candidates.iter().map(|r| r.serial).collect()
            || self.tickets.keys().copied().collect::<BTreeSet<_>>()
                != c.batches.iter().map(|r| r.ticket).collect()
            || self.stages.values().any(|w| w.upgrade().is_none())
            || self.tickets.values().any(|w| w.upgrade().is_none())
        {
            return Err(Error::Unsupported);
        }
        Ok(())
    }
}
impl ExecutionCheckpoint {
    pub fn candidate_ids(&self) -> Vec<u64> {
        self.pending.candidates.iter().map(|r| r.serial).collect()
    }
    pub fn batch_ids(&self) -> Vec<u64> {
        self.pending.batches.iter().map(|r| r.ticket).collect()
    }
    #[cfg(feature = "recorded-inputs")]
    pub fn input_checkpoint(&self) -> Option<&nethacked_inputs::persistence::Checkpoint> {
        self.pending.inputs.as_ref()
    }
}
impl ExecutionRestore {
    /// Resolve every saved profile/accounting identity before any domain decode
    /// or factory. Approval never invokes the factories.
    pub fn approve_accounting(&mut self, catalog: &AccountingRestoreCatalog) -> Result<()> {
        for row in &self.checkpoint.pending.batches {
            catalog.resolve(row)?;
        }
        self.pending.accounting_approved = true;
        Ok(())
    }
    pub(super) fn bound_mappings(&self, kernel: &Kernel, mapped: &RestoredKernel) -> Result<()> {
        if mapped.execution_namespace() != self.checkpoint.kernel_namespace
            || mapped.cleanup().get(&self.checkpoint.cleanup) != Some(&self.state.host.0.cleanup)
        {
            return Err(Error::Authority);
        }
        self.checkpoint
            .pending
            .preflight(&self.checkpoint, kernel, mapped, &self.state.remap)
    }
    pub(super) fn pending_approved(&self) -> Result<()> {
        if !self.checkpoint.pending.batches.is_empty() && !self.pending.accounting_approved {
            return Err(Error::Authority);
        }
        Ok(())
    }
    #[cfg(feature = "recorded-inputs")]
    pub fn inputs(&mut self, input: nethacked_inputs::persistence::Restored) -> Result<()> {
        if self.pending.inputs.is_some()
            || !input
                .matches_destination(self.pending.destination.as_ref().ok_or(Error::Authority)?)
            || !input.matches_checkpoint(
                self.checkpoint
                    .pending
                    .inputs
                    .as_ref()
                    .ok_or(Error::Authority)?,
            )?
        {
            return Err(Error::Authority);
        }
        for id in &self.checkpoint.pending.observations {
            input.observation(*id)?;
        }
        self.pending.inputs = Some(input);
        Ok(())
    }
    /// Decode an enrolled row into a private owner-local allocation. The factory
    /// is independently approved; no command/query/publish callback is invoked.
    #[allow(clippy::too_many_arguments)]
    pub fn candidate<T: 'static>(
        &mut self,
        kernel: &Kernel,
        mapped: &RestoredKernel,
        serial: u64,
        owner: &Rc<Owner<T>>,
        codec: &dyn PersistenceCodec<T>,
        catalog: &RestoreCatalog<T>,
    ) -> Result<Candidate<T>> {
        self.pending_approved()?;
        if self.pending.stages.contains_key(&serial)
            || !Rc::ptr_eq(&owner.host.0, &self.state.host.0)
        {
            return Err(Error::Invalid);
        }
        let row = self
            .checkpoint
            .pending
            .candidates
            .iter()
            .find(|r| r.serial == serial)
            .ok_or(Error::Stale)?
            .clone();
        if owner.id() != self.state.remap.owner(row.stamp.owner)?
            || codec.identity() != row.state_codec
        {
            return Err(Error::Schema);
        }
        let factory = catalog.resolve(&row.build, &row.policy)?;
        let e = ExecutionRemap { kernel, mapped };
        // All mapped metadata and ledger credentials before codecs/factories.
        self.bound_mappings(kernel, mapped)?;
        #[cfg(feature = "recorded-inputs")]
        let recorded = self
            .pending
            .recorded(&row.recorded, &self.checkpoint.pending)?;
        let dependencies = row
            .dependencies
            .iter()
            .map(|o| o.restore(self, &e))
            .collect::<Result<Vec<_>>>()?;
        // Authenticate the exact saved prepared allocation in its original
        // namespace. The approved old backend supplies its original schema;
        // current code and a newly prepared proposal cannot substitute for it.
        let saved_remap = remap_for(&self.checkpoint.state, self.checkpoint.state.source_host);
        let saved = codec.decode_state(
            &row.payload,
            self.state.host.0.limits.payload_bytes,
            &saved_remap,
        )?;
        codec.validate_state(&saved)?;
        if codec.encode_state(&saved, self.state.host.0.limits.payload_bytes)? != row.payload {
            return Err(Error::Invariant);
        }
        if codec.references(&saved)? != row.references {
            return Err(Error::Stale);
        }
        let saved_owner = self
            .checkpoint
            .state
            .owners
            .iter()
            .find(|o| o.old == row.stamp.owner && o.live)
            .ok_or(Error::Stale)?;
        codec.validate_configuration(&saved, &saved_owner.ports, &row.policy)?;
        let backend = factory(&e)?;
        validate_backend(&backend, &Some(row.backend.clone()), &row.permit, &e)?;
        let schema = backend.schema()?;
        schema.validate(&saved)?;
        let saved_encoded = schema.encode(&saved)?;
        bound(saved_encoded.len(), self.state.host.0.limits.payload_bytes)?;
        if <[u8; 32]>::from(Sha256::digest(&saved_encoded)) != row.next_digest {
            return Err(Error::Invariant);
        }
        let next = codec.decode_state(
            &row.payload,
            self.state.host.0.limits.payload_bytes,
            &self.state.remap,
        )?;
        codec.validate_state(&next)?;
        if codec.references(&next)?
            != row
                .references
                .iter()
                .map(|r| self.state.remap.reference(r))
                .collect::<Result<Vec<_>>>()?
        {
            return Err(Error::Stale);
        }
        codec.validate_configuration(&next, &owner.metadata.ports, &row.policy)?;
        schema.validate(&next)?;
        let encoded = schema.encode(&next)?;
        bound(encoded.len(), self.state.host.0.limits.payload_bytes)?;
        let next_digest = <[u8; 32]>::from(Sha256::digest(&encoded));
        let event_bytes = row
            .events
            .iter()
            .try_fold(0usize, |n, e| n.checked_add(e.len()).ok_or(Error::Limit))?;
        let retained = encoded
            .len()
            .checked_add(event_bytes)
            .and_then(|n| n.checked_add(row.reply.len()))
            .and_then(|n| n.checked_add(self.state.host.0.limits.observation_bytes))
            .ok_or(Error::Limit)?;
        let saved_retained = saved_encoded
            .len()
            .checked_add(event_bytes)
            .and_then(|n| n.checked_add(row.reply.len()))
            .and_then(|n| n.checked_add(self.state.host.0.limits.observation_bytes))
            .ok_or(Error::Limit)?;
        if saved_retained != row.retained_bytes {
            return Err(Error::Invariant);
        }
        let capability = e.permit(&row.capability)?;
        let receipt = Receipt {
            serial,
            stamp: row.stamp.restore(&self.state.remap, &e)?,
            caller: e.module(row.caller)?,
            caller_artifact: row.artifact,
            operation: mapped.execution_source(&row.source)?,
            command: row.command.clone(),
            capability,
            behavior: row
                .behavior
                .iter()
                .map(|b| e.row_binding(b))
                .collect::<Result<_>>()?,
            next_digest,
            event_count: row.events.len(),
            request_digest: row.request_digest,
            #[cfg(feature = "recorded-inputs")]
            recorded_inputs: recorded.iter().map(|o| o.identity().clone()).collect(),
        };
        let code = row
            .pins
            .iter()
            .map(|id| mapped.pins().get(id).cloned().ok_or(Error::Authority))
            .collect::<Result<_>>()?;
        let helper = row
            .helper
            .map(|id| {
                mapped
                    .pure_captures()
                    .get(id)
                    .cloned()
                    .ok_or(Error::Authority)
            })
            .transpose()?;
        let wasm = match &backend {
            Backend::Wasm { codec, .. } => Some(codec.clone()),
            _ => None,
        };
        let stage = Rc::new(Stage {
            host: self.state.host.clone(),
            owner: Rc::downgrade(owner),
            receipt,
            prepared: RefCell::new(Some(Prepared {
                owner: owner.clone(),
                next,
                events: row.events,
                reply: row.reply,
                dependencies,
                code,
                codec: wasm,
                helper,
                persistence_backend: backend,
                persistence_policy: row.policy,
                #[cfg(feature = "recorded-inputs")]
                recorded,
            })),
            active: Cell::new(false),
            retained_bytes: retained,
        });
        let erased: Rc<dyn Staged> = stage.clone();
        {
            let mut r = self.state.host.0.registry.borrow_mut();
            let total = r.staging_bytes.checked_add(retained).ok_or(Error::Limit)?;
            bound(
                total,
                self.state.host.0.publication.borrow().limits.retained_bytes,
            )?;
            bound(r.pending + 1, self.state.host.0.limits.candidates)?;
            r.pending += 1;
            r.staging_bytes = total;
            owner.metadata.pending.set(owner.metadata.pending.get() + 1);
            stage.active.set(true);
            r.staging.insert(serial, Rc::downgrade(&erased));
        }
        self.pending.stages.insert(serial, Rc::downgrade(&erased));
        Ok(Candidate { stage })
    }
    pub fn batch(
        &mut self,
        kernel: &Kernel,
        mapped: &RestoredKernel,
        ticket: u64,
        catalog: &AccountingRestoreCatalog,
    ) -> Result<Batch> {
        self.pending_approved()?;
        if self.pending.tickets.contains_key(&ticket) {
            return Err(Error::Invalid);
        }
        let row = self
            .checkpoint
            .pending
            .batches
            .iter()
            .find(|r| r.ticket == ticket)
            .ok_or(Error::Stale)?
            .clone();
        let factory = catalog.resolve(&row)?;
        self.bound_mappings(kernel, mapped)?;
        let e = ExecutionRemap { kernel, mapped };
        let candidates = row
            .candidates
            .iter()
            .map(|id| {
                self.pending
                    .stages
                    .get(id)
                    .and_then(Weak::upgrade)
                    .map(|stage| ErasedCandidate { stage })
                    .ok_or(Error::Stale)
            })
            .collect::<Result<_>>()?;
        let readonly = row
            .readonly
            .iter()
            .map(|o| o.restore(self, &e))
            .collect::<Result<_>>()?;
        let operation = mapped.execution_source(&row.source)?;
        let code = mapped
            .pins()
            .get(&row.pin)
            .cloned()
            .ok_or(Error::Authority)?;
        let coordinator = e.permit(&row.coordinator)?;
        let provider = e.row_binding(&row.provider)?;
        let caller = e.module(row.caller)?;
        let writes = row
            .writes
            .iter()
            .map(|o| self.state.remap.owner(*o))
            .collect::<Result<_>>()?;
        let order = row
            .order
            .iter()
            .map(|o| self.state.remap.owner(*o))
            .collect::<Result<_>>()?;
        let accounting = if let Some((_, value)) = self
            .pending
            .accounting
            .iter()
            .find(|(id, _)| id == &row.accounting)
        {
            value.clone()
        } else {
            let value = factory()?;
            self.pending
                .accounting
                .push((row.accounting.clone(), value.clone()));
            value
        };
        let state = Rc::new(TicketState {
            operation: operation.clone(),
            consumed: Cell::new(false),
            readonly: RefCell::new(readonly),
            code: RefCell::new(Some(code)),
        });
        self.state
            .host
            .0
            .registry
            .borrow_mut()
            .tickets
            .insert(ticket, Rc::downgrade(&state));
        self.pending.tickets.insert(ticket, Rc::downgrade(&state));
        Ok(Batch {
            host: self.state.host.clone(),
            ticket,
            state,
            coordinator,
            provider,
            caller,
            artifact: row.artifact,
            operation,
            candidates,
            writes,
            order,
            policy: row.policy,
            accounting,
        })
    }
}

// Half-open ranges retain every consumed/abandoned hole without unbounded per-ID
// allocation. The live IDs and retirement ranges form a complete partition.
fn retired_ranges(end: u64, live: &BTreeSet<u64>) -> Result<Vec<(u64, u64)>> {
    if end == 0 {
        return Err(Error::Invalid);
    }
    let mut cursor = 1;
    let mut rows = vec![];
    for id in live {
        if *id < cursor || *id >= end {
            return Err(Error::Invalid);
        }
        if cursor < *id {
            rows.push((cursor, *id));
        }
        cursor = id.checked_add(1).ok_or(Error::Limit)?;
    }
    if cursor < end {
        rows.push((cursor, end));
    }
    Ok(rows)
}
fn validate_retired(rows: &Option<Vec<(u64, u64)>>, end: u64, live: &BTreeSet<u64>) -> Result<()> {
    match rows {
        Some(rows) if rows == &retired_ranges(end, live)? => Ok(()),
        None if live.is_empty() => Ok(()),
        _ => Err(Error::Stale),
    }
}
