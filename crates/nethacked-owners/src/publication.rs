use super::*;
use std::{any::Any, cell::RefMut};

#[derive(Clone, Debug)]
pub struct PublicationLimits {
    pub transactions: usize,
    pub participants: usize,
    pub history_events: usize,
    pub history_bytes: usize,
    pub reply_bytes: usize,
    pub retained_bytes: usize,
}
impl Default for PublicationLimits {
    fn default() -> Self {
        Self {
            transactions: 128,
            participants: 64,
            history_events: 4096,
            history_bytes: 1_048_576,
            reply_bytes: 1_048_576,
            retained_bytes: 1_048_576,
        }
    }
}
#[derive(Default)]
pub(super) struct Publication {
    pub(super) limits: PublicationLimits,
    next_ticket: u64,
    pub(super) epoch: u64,
    history: Vec<CommitReceipt>,
    #[cfg(feature = "persistence")]
    local_history: Vec<persistence::HistoricalCommit>,
    events: usize,
    bytes: usize,
    replies: usize,
    #[cfg(feature = "persistence")]
    archived: Vec<persistence::HistoricalCommit>,
}
#[cfg(test)]
impl Publication {
    pub(super) fn counters(&self) -> (u64, u64, usize, usize, usize) {
        (
            self.next_ticket,
            self.epoch,
            self.events,
            self.bytes,
            self.replies,
        )
    }
}
/// Detached preflight evidence, never an authoritative payload representation.
pub struct ProposalView {
    pub receipt: Receipt,
    pub before: Vec<u8>,
    pub proposed: Vec<u8>,
    pub events: Vec<Vec<u8>>,
    pub reply: Vec<u8>,
}
/// Trusted pure cross-owner validator. It runs only during preflight, cannot
/// supply states or implement publication. The declared write set is mandatory.
pub trait Accounting {
    fn validate(&self, proposals: &[ProposalView], readonly: &[Observation]) -> Result<()>;
}
/// Sealed mixed-type proposal; external implementations cannot join publication.
pub struct ErasedCandidate {
    pub(super) stage: Rc<dyn Staged>,
}
pub struct Batch {
    host: Host,
    ticket: u64,
    state: Rc<TicketState>,
    coordinator: CapabilityPermit,
    provider: ProviderBinding,
    caller: ModuleHandle,
    artifact: [u8; 32],
    operation: OperationBinding,
    candidates: Vec<ErasedCandidate>,
    writes: Vec<OwnerId>,
    order: Vec<OwnerId>,
    policy: [u8; 32],
    accounting: Rc<dyn Accounting>,
}
pub(super) struct TicketState {
    operation: OperationBinding,
    consumed: Cell<bool>,
    readonly: RefCell<Vec<Observation>>,
    code: RefCell<Option<ModulePin>>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitReceipt {
    pub ticket: u64,
    pub epoch: u64,
    pub caller: ModuleHandle,
    pub caller_artifact: [u8; 32],
    pub operation: OperationBinding,
    pub coordinator: CapabilityPermit,
    pub policy: [u8; 32],
    /// Profile order, independent of canonical guard acquisition order.
    pub participants: Vec<Receipt>,
    pub observations: Vec<ObservationReceipt>,
    pub events: Vec<Vec<u8>>,
    /// Fully encoded length-prefixed participant replies in profile order.
    pub reply: Vec<u8>,
    /// Bounded diagnostic encoding, not an M4d restoration format.
    pub history_encoding: Vec<u8>,
}
/// Immutable dependency proof metadata. Contains digests, never live payloads,
/// routing pointers, mutable aliases or executable authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservationReceipt {
    pub stamp: Stamp,
    pub query: String,
    pub caller: ModuleHandle,
    pub caller_artifact: [u8; 32],
    pub capability: CapabilityPermit,
    pub data_digest: [u8; 32],
    pub dependencies: Vec<ObservationReceipt>,
    #[cfg(feature = "recorded-inputs")]
    pub recorded_inputs: Vec<nethacked_inputs::ObservationIdentity>,
}
impl From<&Observation> for ObservationReceipt {
    fn from(o: &Observation) -> Self {
        Self {
            stamp: o.stamp.clone(),
            query: o.name.clone(),
            caller: o.caller,
            caller_artifact: o.caller_artifact,
            capability: o.permit.clone(),
            data_digest: Sha256::digest(&o.data).into(),
            dependencies: o.dependencies.iter().map(Self::from).collect(),
            #[cfg(feature = "recorded-inputs")]
            recorded_inputs: o.recorded.iter().map(|r| r.identity().clone()).collect(),
        }
    }
}
pub(super) trait Staged {
    fn host(&self) -> &Host;
    fn receipt(&self) -> &Receipt;
    fn evidence(&self, view: &CallerView<'_>) -> Result<ProposalView>;
    fn with_guard(&self, action: &mut dyn FnMut(&mut dyn Locked) -> Result<()>) -> Result<()>;
    fn release(&self, registry: &mut Registry);
    fn take(&self) -> Result<Box<dyn Any>>;
    fn available(&self) -> Result<()>;
    fn retained_bytes(&self) -> usize;
    fn dependencies(
        &self,
        view: &CallerView<'_>,
        count: &mut usize,
        bytes: &mut usize,
    ) -> Result<()>;
    fn observation_receipts(&self) -> Result<Vec<ObservationReceipt>>;
}
pub(super) trait Locked {
    fn swap(&mut self);
}
struct TypedGuard<'a, T> {
    slot: RefMut<'a, StateSlot<T>>,
    next: &'a mut T,
    metadata: &'a Metadata,
    active: &'a Cell<bool>,
    revision: u64,
}
impl<T> Locked for TypedGuard<'_, T> {
    fn swap(&mut self) {
        std::mem::swap(&mut self.slot.state, self.next);
        self.slot.revision = self.revision;
        self.metadata.revision.set(self.revision);
        self.metadata.pending.set(self.metadata.pending.get() - 1);
        self.active.set(false);
        #[cfg(test)]
        probe_swap();
    }
}
impl<T: 'static> Staged for Stage<T> {
    fn host(&self) -> &Host {
        &self.host
    }
    fn receipt(&self) -> &Receipt {
        &self.receipt
    }
    fn evidence(&self, view: &CallerView<'_>) -> Result<ProposalView> {
        let prepared = self.prepared.try_borrow().map_err(|_| Error::Busy)?;
        self.preflight(prepared.as_ref().ok_or(Error::Stale)?, view)
    }
    fn with_guard(&self, action: &mut dyn FnMut(&mut dyn Locked) -> Result<()>) -> Result<()> {
        if !self.active.get() {
            return Err(Error::Stale);
        }
        let mut prepared = self.prepared.try_borrow_mut().map_err(|_| Error::Busy)?;
        let p = prepared.as_mut().ok_or(Error::Stale)?;
        let slot = p.owner.slot.try_borrow_mut().map_err(|_| Error::Busy)?;
        let revision = slot.revision.checked_add(1).ok_or(Error::Limit)?;
        let mut guard = TypedGuard {
            slot,
            next: &mut p.next,
            metadata: &p.owner.metadata,
            active: &self.active,
            revision,
        };
        action(&mut guard)
    }
    fn release(&self, registry: &mut Registry) {
        self.release_pending(registry);
    }
    fn take(&self) -> Result<Box<dyn Any>> {
        Ok(Box::new(
            self.prepared
                .try_borrow_mut()
                .map_err(|_| Error::Busy)?
                .take(),
        ))
    }
    fn available(&self) -> Result<()> {
        drop(self.prepared.try_borrow_mut().map_err(|_| Error::Busy)?);
        Ok(())
    }
    fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }
    fn dependencies(
        &self,
        view: &CallerView<'_>,
        count: &mut usize,
        bytes: &mut usize,
    ) -> Result<()> {
        let prepared = self.prepared.try_borrow().map_err(|_| Error::Busy)?;
        #[cfg(feature = "recorded-inputs")]
        for input in &prepared.as_ref().ok_or(Error::Stale)?.recorded {
            input.validate(view)?;
            *count = count.checked_add(1).ok_or(Error::Limit)?;
            *bytes = bytes
                .checked_add(recorded_size(input)?)
                .ok_or(Error::Limit)?;
            bound(*count, self.host.0.limits.observations)?;
            bound(*bytes, self.host.0.limits.observation_bytes)?;
        }
        for o in &prepared.as_ref().ok_or(Error::Stale)?.dependencies {
            validate_observation(
                &self.host,
                o,
                view,
                count,
                bytes,
                ObservationMode::Retained(&self.receipt.operation),
            )?;
        }
        Ok(())
    }
    fn observation_receipts(&self) -> Result<Vec<ObservationReceipt>> {
        let prepared = self.prepared.try_borrow().map_err(|_| Error::Busy)?;
        Ok(prepared
            .as_ref()
            .ok_or(Error::Stale)?
            .dependencies
            .iter()
            .map(ObservationReceipt::from)
            .collect())
    }
}

fn participant_permit(m: &Metadata, name: &str, view: &CallerView<'_>) -> Result<CapabilityPermit> {
    if !m.live.get() {
        return Err(Error::Stale);
    }
    let permit = view
        .permit(&capability(m, name)?, &resource(m, name), Access::Write)
        .ok_or(Error::Authority)?;
    let expected = ProviderBinding {
        handle: m.stamp.module.handle,
        artifact: m.stamp.module.artifact,
        endpoint: name.into(),
    };
    if permit.resolution.last() != Some(&expected) {
        return Err(Error::Authority);
    }
    Ok(permit)
}
impl Host {
    fn route_without_registry(&self, registry: &Registry, id: OwnerId) -> Result<()> {
        let route = registry
            .entries
            .get(&id)
            .and_then(|e| e.route.upgrade())
            .ok_or(Error::Stale)?;
        if !route.metadata().live.get() || route.metadata().pending.get() == 0 {
            return Err(Error::Stale);
        }
        Ok(())
    }
    pub fn publication_resource(&self) -> Resource {
        Resource {
            owner: format!("owners-{}", self.0.id),
            key: "publication".into(),
        }
    }
    pub fn cleanup_identity(&self) -> Result<CleanupIdentity> {
        let _gate = self.close_visibility()?;
        Ok(self.0.cleanup)
    }
    /// Enrollment-time bounds. Refuses after the first ticket is issued.
    pub fn configure_publication(&self, limits: PublicationLimits) -> Result<()> {
        let _gate = self.close_visibility()?;
        if [
            limits.transactions,
            limits.participants,
            limits.history_events,
            limits.history_bytes,
            limits.reply_bytes,
            limits.retained_bytes,
        ]
        .contains(&0)
            || limits.participants > self.0.limits.candidates
        {
            return Err(Error::Limit);
        }
        let mut p = self.0.publication.borrow_mut();
        if p.next_ticket != 0 || self.0.registry.borrow().pending != 0 {
            return Err(Error::Referenced);
        }
        p.limits = limits;
        Ok(())
    }
    /// Seals exact participant and profile order declarations. `writes` comes
    /// from the host's accounting policy, so omission is checked before validation.
    #[allow(clippy::too_many_arguments)]
    pub fn batch(
        &self,
        candidates: Vec<ErasedCandidate>,
        readonly: Vec<Observation>,
        writes: &[OwnerId],
        order: Vec<OwnerId>,
        policy: [u8; 32],
        accounting: Rc<dyn Accounting>,
        coordinator: Capability,
        view: &CallerView<'_>,
    ) -> Result<Batch> {
        let _gate = self.close_visibility()?;
        let code = view.pin_invocation()?;
        let permit = coordinator_permit(self, &coordinator, view)?;
        let operation = view.operation().ok_or(Error::Authority)?.clone();
        if !operation.admits_cleanup(self.0.cleanup) {
            return Err(Error::Authority);
        }
        let mut candidates = candidates;
        candidates.sort_by_key(|c| c.stage.receipt().stamp.owner);
        let mut declared = writes.to_vec();
        declared.sort();
        let caller = view.caller().ok_or(Error::Authority)?;
        let artifact = view.caller_artifact().ok_or(Error::Authority)?;
        let mut p = self.0.publication.borrow_mut();
        bound(candidates.len(), p.limits.participants)?;
        bound(readonly.len(), self.0.limits.observations)?;
        // Seal current behavior, preserving the existing deferred refusal timing
        // for state/schema/authority. Commit still revalidates every dependency.
        for observation in &readonly {
            admit_observation_behavior(self, observation, view)?;
        }
        let ticket = p.next_ticket.checked_add(1).ok_or(Error::Limit)?;
        bound(
            usize::try_from(ticket).map_err(|_| Error::Limit)?,
            p.limits.transactions,
        )?;
        p.next_ticket = ticket;
        let state = Rc::new(TicketState {
            operation: operation.clone(),
            consumed: Cell::new(false),
            readonly: RefCell::new(readonly),
            code: RefCell::new(Some(code)),
        });
        self.0
            .registry
            .borrow_mut()
            .tickets
            .insert(ticket, Rc::downgrade(&state));
        Ok(Batch {
            host: self.clone(),
            ticket,
            state,
            coordinator: permit,
            provider: ProviderBinding {
                handle: view.provider(),
                artifact: view.provider_artifact(),
                endpoint: coordinator.endpoint,
            },
            caller,
            artifact,
            operation,
            candidates,
            writes: declared,
            order,
            policy,
            accounting,
        })
    }
    pub fn history(&self) -> Result<Vec<CommitReceipt>> {
        let _gate = self.close_visibility()?;
        Ok(self.0.publication.borrow().history.clone())
    }
    /// Exact checkpoint cleanup; never changes committed history or live slots.
    pub fn discard_operation(&self, binding: &OperationBinding) -> Result<()> {
        let gate = self.close_visibility()?;
        let stages: Vec<_> = self
            .0
            .registry
            .borrow()
            .staging
            .values()
            .filter_map(Weak::upgrade)
            .filter(|s| &s.receipt().operation == binding)
            .collect();
        for stage in &stages {
            stage.available()?;
        }
        let tickets: Vec<_> = self
            .0
            .registry
            .borrow()
            .tickets
            .values()
            .filter_map(Weak::upgrade)
            .filter(|t| &t.operation == binding)
            .collect();
        for ticket in &tickets {
            drop(ticket.readonly.try_borrow_mut().map_err(|_| Error::Busy)?);
            drop(ticket.code.try_borrow_mut().map_err(|_| Error::Busy)?);
        }
        let mut retained = Vec::with_capacity(stages.len());
        let mut ticket_refs = Vec::with_capacity(tickets.len());
        for ticket in tickets {
            ticket.consumed.set(true);
            ticket_refs.push((
                std::mem::take(&mut *ticket.readonly.borrow_mut()),
                ticket.code.borrow_mut().take(),
            ));
        }
        for stage in &stages {
            retained.push(stage.take()?);
        }
        let mut registry = self.0.registry.borrow_mut();
        for stage in &stages {
            stage.release(&mut registry);
        }
        drop(registry);
        drop(gate);
        self.release_retained((retained, ticket_refs));
        Ok(())
    }
    fn release_retained<T>(&self, retained: T) {
        if let Err(panic) =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(retained)))
        {
            self.0.quarantine.set(true);
            std::panic::resume_unwind(panic);
        }
    }
    /// Fail-stop diagnostic: committed prefixes remain available in quarantine.
    /// This cannot inspect live payloads or resume execution.
    pub fn quarantine_history(&self) -> Result<Vec<CommitReceipt>> {
        if self.0.gate.get() {
            return Err(Error::Busy);
        }
        if !self.0.quarantine.get() {
            return Err(Error::Invalid);
        }
        Ok(self.0.publication.borrow().history.clone())
    }
}
fn coordinator_permit(
    host: &Host,
    cap: &Capability,
    view: &CallerView<'_>,
) -> Result<CapabilityPermit> {
    let permit = view
        .permit(cap, &host.publication_resource(), Access::Write)
        .ok_or(Error::Authority)?;
    if permit.resolution.last()
        != Some(&ProviderBinding {
            handle: view.provider(),
            artifact: view.provider_artifact(),
            endpoint: cap.endpoint.clone(),
        })
    {
        return Err(Error::Authority);
    }
    Ok(permit)
}

impl Batch {
    #[cfg(test)]
    pub(crate) fn consumed_for_test(&self) -> bool {
        self.state.consumed.get()
    }
    pub fn ticket(&self) -> u64 {
        self.ticket
    }
    /// All fallible work precedes the sealed swaps. On refusal this batch and
    /// every candidate remain retryable. Successful tickets can never replay.
    pub fn commit(&self, view: &CallerView<'_>) -> Result<CommitReceipt> {
        let gate = self.host.close_visibility()?;
        if self.state.consumed.get() {
            return Err(Error::Stale);
        }
        let _pin = view.pin_invocation()?;
        let ids: Vec<_> = self
            .candidates
            .iter()
            .map(|c| c.stage.receipt().stamp.owner)
            .collect();
        let mut ordered = self.order.clone();
        ordered.sort();
        if ids.is_empty()
            || ids.windows(2).any(|w| w[0] == w[1])
            || ids != self.writes
            || ids != ordered
        {
            return Err(Error::Invalid);
        }
        for c in &self.candidates {
            let r = c.stage.receipt();
            if !Rc::ptr_eq(&c.stage.host().0, &self.host.0) {
                return Err(Error::Stale);
            }
            if r.operation != self.operation
                || r.caller != self.caller
                || r.caller_artifact != self.artifact
            {
                return Err(Error::Authority);
            }
        }
        if view.caller() != Some(self.caller)
            || view.caller_artifact() != Some(self.artifact)
            || view.operation() != Some(&self.operation)
            || view.provider() != self.provider.handle
            || view.provider_artifact() != self.provider.artifact
            || coordinator_permit(&self.host, &self.coordinator.capability, view)?
                != self.coordinator
        {
            return Err(Error::Authority);
        }
        let mut count = 0;
        let mut bytes = 0;
        let readonly = self.state.readonly.try_borrow().map_err(|_| Error::Busy)?;
        for observation in &*readonly {
            validate_observation(
                &self.host,
                observation,
                view,
                &mut count,
                &mut bytes,
                ObservationMode::Retained(&self.operation),
            )?;
        }
        for candidate in &self.candidates {
            candidate.stage.dependencies(view, &mut count, &mut bytes)?;
        }
        let evidence = self
            .candidates
            .iter()
            .map(|c| c.stage.evidence(view))
            .collect::<Result<Vec<_>>>()?;
        let mut observations: Vec<_> = readonly.iter().map(ObservationReceipt::from).collect();
        for candidate in &self.candidates {
            observations.extend(candidate.stage.observation_receipts()?);
        }
        self.accounting.validate(&evidence, &readonly)?;
        let mut p = self
            .host
            .0
            .publication
            .try_borrow_mut()
            .map_err(|_| Error::Busy)?;
        let epoch = p.epoch.checked_add(1).ok_or(Error::Limit)?;
        let mut participants = Vec::with_capacity(evidence.len());
        let mut events = Vec::new();
        let mut reply = Vec::new();
        reply.extend_from_slice(&self.ticket.to_le_bytes());
        reply.extend_from_slice(&epoch.to_le_bytes());
        let mut retained = 0usize;
        for id in &self.order {
            let e = evidence
                .iter()
                .find(|e| e.receipt.stamp.owner == *id)
                .ok_or(Error::Invalid)?;
            retained = retained
                .checked_add(e.before.len())
                .and_then(|n| n.checked_add(e.proposed.len()))
                .ok_or(Error::Limit)?;
            participants.push(e.receipt.clone());
            events.extend(e.events.clone());
            let len = u64::try_from(e.reply.len()).map_err(|_| Error::Limit)?;
            reply.extend_from_slice(&len.to_le_bytes());
            reply.extend_from_slice(&e.reply);
        }
        bound(retained, p.limits.retained_bytes)?;
        bound(events.len(), self.host.0.limits.events)?;
        let event_bytes = events
            .iter()
            .try_fold(0usize, |n, e| n.checked_add(e.len()).ok_or(Error::Limit))?;
        bound(event_bytes, self.host.0.limits.event_bytes)?;
        bound(
            reply.len(),
            self.host
                .0
                .limits
                .reply_bytes
                .min(view.max_reply() as usize),
        )?;
        let total_events = p.events.checked_add(events.len()).ok_or(Error::Limit)?;
        let total_replies = p.replies.checked_add(reply.len()).ok_or(Error::Limit)?;
        bound(total_events, p.limits.history_events)?;
        bound(total_replies, p.limits.reply_bytes)?;
        bound(
            usize::try_from(epoch).map_err(|_| Error::Limit)?,
            p.limits.transactions,
        )?;
        let mut receipt = CommitReceipt {
            ticket: self.ticket,
            epoch,
            caller: self.caller,
            caller_artifact: self.artifact,
            operation: self.operation.clone(),
            coordinator: self.coordinator.clone(),
            policy: self.policy,
            participants,
            observations,
            events,
            reply,
            history_encoding: Vec::new(),
        };
        receipt.history_encoding = format!("owners-history-v1:{receipt:?}").into_bytes();
        let storage_bytes = receipt
            .history_encoding
            .len()
            .checked_add(event_bytes)
            .and_then(|n| n.checked_add(receipt.reply.len()))
            .ok_or(Error::Limit)?;
        let total_bytes = p.bytes.checked_add(storage_bytes).ok_or(Error::Limit)?;
        bound(total_bytes, p.limits.history_bytes)?;
        #[cfg(feature = "persistence")]
        let mut local = Some(persistence::HistoricalCommit::from_receipt(
            self.host.0.id,
            &receipt,
            &evidence,
        ));
        #[cfg(feature = "persistence")]
        p.local_history.try_reserve(1).map_err(|_| Error::Limit)?;
        let mut stored = Some(receipt.clone());
        p.history.try_reserve(1).map_err(|_| Error::Limit)?;
        let mut registry = self
            .host
            .0
            .registry
            .try_borrow_mut()
            .map_err(|_| Error::Busy)?;
        let staged_bytes = self.candidates.iter().try_fold(0usize, |n, c| {
            n.checked_add(c.stage.retained_bytes()).ok_or(Error::Limit)
        })?;
        bound(
            registry
                .staging_bytes
                .checked_add(retained)
                .ok_or(Error::Limit)?,
            p.limits.retained_bytes,
        )?;
        if registry.pending < self.candidates.len()
            || registry.staging_bytes < staged_bytes
            || self.candidates.iter().any(|c| {
                self.host
                    .route_without_registry(&registry, c.stage.receipt().stamp.owner)
                    .is_err()
            })
        {
            return Err(Error::Stale);
        }
        // Recursion acquires ALL typed guards before entering this leaf. The
        // linked stack contains only sealed runtime implementations.
        acquire(&self.candidates, &mut End, &mut |chain| {
            let stored = stored.take().ok_or(Error::Invalid)?;
            #[cfg(test)]
            probe_begin();
            chain.swap_all();
            for c in &self.candidates {
                if let Some(entry) = registry.entries.get_mut(&c.stage.receipt().stamp.owner) {
                    entry.stamp.revision = c.stage.receipt().stamp.revision + 1;
                }
            }
            registry.pending -= self.candidates.len();
            registry.staging_bytes -= staged_bytes;
            p.history.push(stored);
            #[cfg(feature = "persistence")]
            p.local_history
                .push(local.take().expect("preallocated history evidence"));
            p.epoch = epoch;
            p.events = total_events;
            p.bytes = total_bytes;
            p.replies = total_replies;
            self.state.consumed.set(true);
            Ok(())
        })?;
        // No payload, observation or code token is dropped before gate opening.
        drop(registry);
        drop(p);
        drop(readonly);
        drop(gate);
        // Meter guard release and gate opening too, before post-commit cleanup.
        #[cfg(test)]
        probe_end();
        let mut old = Vec::with_capacity(self.candidates.len());
        for candidate in &self.candidates {
            old.push(candidate.stage.take().expect("publication guards released"));
            candidate
                .stage
                .release(&mut self.host.0.registry.borrow_mut());
        }
        let references = (
            std::mem::take(&mut *self.state.readonly.borrow_mut()),
            self.state.code.borrow_mut().take(),
        );
        self.host.release_retained((old, references));
        Ok(receipt)
    }
    pub fn discard(&self) -> Result<()> {
        let gate = self.host.close_visibility()?;
        let local: Vec<_> = self
            .candidates
            .iter()
            .filter(|c| Rc::ptr_eq(&c.stage.host().0, &self.host.0))
            .collect();
        for c in &local {
            c.stage.available()?;
        }
        let mut retained = Vec::with_capacity(self.candidates.len());
        for c in &local {
            retained.push(c.stage.take()?);
        }
        for c in &local {
            c.stage.release(&mut self.host.0.registry.borrow_mut());
        }
        self.state.consumed.set(true);
        let references = (
            std::mem::take(&mut *self.state.readonly.borrow_mut()),
            self.state.code.borrow_mut().take(),
        );
        drop(gate);
        self.host.release_retained((retained, references));
        Ok(())
    }
}

#[cfg(test)]
thread_local! { pub(crate) static SWAP_PROBE: RefCell<Option<Box<dyn FnMut()>>> = RefCell::new(None); }
#[cfg(test)]
fn probe_swap() {
    SWAP_PROBE.with(|p| {
        if let Some(p) = p.borrow_mut().as_mut() {
            p();
        }
    });
}
#[cfg(test)]
fn probe_begin() {
    crate::probes::begin();
}
#[cfg(test)]
fn probe_end() {
    crate::probes::end();
}
trait Chain {
    fn swap_all(&mut self);
}
struct End;
impl Chain for End {
    fn swap_all(&mut self) {}
}
struct Link<'a> {
    guard: &'a mut dyn Locked,
    previous: &'a mut dyn Chain,
}
impl Chain for Link<'_> {
    fn swap_all(&mut self) {
        self.previous.swap_all();
        self.guard.swap();
    }
}
fn acquire(
    stages: &[ErasedCandidate],
    chain: &mut dyn Chain,
    action: &mut dyn FnMut(&mut dyn Chain) -> Result<()>,
) -> Result<()> {
    match stages.split_first() {
        None => action(chain),
        Some((head, tail)) => head.stage.with_guard(&mut |guard| {
            acquire(
                tail,
                &mut Link {
                    guard,
                    previous: chain,
                },
                action,
            )
        }),
    }
}

impl<T> Stage<T> {
    fn release_pending(&self, registry: &mut Registry) {
        if self.active.replace(false) {
            registry.pending -= 1;
            registry.staging_bytes -= self.retained_bytes;
            if let Some(owner) = self.owner.upgrade() {
                owner.metadata.pending.set(owner.metadata.pending.get() - 1);
            }
        }
        registry.staging.remove(&self.receipt.serial);
    }
}
impl<T> Drop for Stage<T> {
    fn drop(&mut self) {
        self.release_pending(&mut self.host.0.registry.borrow_mut());
    }
}
impl<T: 'static> Stage<T> {
    pub(super) fn preflight(&self, p: &Prepared<T>, view: &CallerView<'_>) -> Result<ProposalView> {
        if !self.active.get() {
            return Err(Error::Stale);
        }
        let owner = &p.owner;
        let r = &self.receipt;
        if owner.metadata.current_stamp() != r.stamp
            || owner.slot.try_borrow().map_err(|_| Error::Busy)?.revision != r.stamp.revision
        {
            return Err(Error::Stale);
        }
        if view.caller() != Some(r.caller)
            || view.caller_artifact() != Some(r.caller_artifact)
            || view.operation() != Some(&r.operation)
        {
            return Err(Error::Authority);
        }
        if participant_permit(&owner.metadata, &r.command, view)? != r.capability {
            return Err(Error::Authority);
        }
        owner.observations_with_mode(
            &r.command,
            &p.dependencies,
            view,
            ObservationMode::Retained(&r.operation),
        )?;
        #[cfg(feature = "recorded-inputs")]
        owner.validate_recorded_inputs(&r.command, &p.dependencies, &p.recorded, view)?;
        let code_count = match &*owner.backend.borrow() {
            Backend::Native(_) => 2,
            Backend::Wasm { .. } => 3,
            Backend::CommitCallback => return Err(Error::Unsupported),
        };
        if p.code.len() != code_count || p.code.iter().any(|pin| pin.module_count() == 0) {
            return Err(Error::Stale);
        }
        if let Some(helper) = &p.helper {
            view.validate_pure(helper)?;
        }
        let backend = owner.backend.borrow();
        let schema = p
            .codec
            .as_ref()
            .map(|c| c.as_ref() as &dyn Schema<T>)
            .unwrap_or(backend.schema()?);
        schema.validate(&p.next)?;
        let proposed = schema.encode(&p.next)?;
        let before = schema.encode(&owner.slot.try_borrow().map_err(|_| Error::Busy)?.state)?;
        bound(proposed.len(), self.host.0.limits.payload_bytes)?;
        bound(before.len(), self.host.0.limits.payload_bytes)?;
        if <[u8; 32]>::from(Sha256::digest(&proposed)) != r.next_digest {
            return Err(Error::Invariant);
        }
        Ok(ProposalView {
            receipt: r.clone(),
            before,
            proposed,
            events: p.events.clone(),
            reply: p.reply.clone(),
        })
    }
}

#[cfg(feature = "persistence")]
impl Publication {
    pub(super) fn capture_local(
        &self,
        _origin_host: u64,
        registry: &Registry,
    ) -> Result<persistence::LocalHistory> {
        if registry
            .tickets
            .values()
            .filter_map(Weak::upgrade)
            .any(|t| !t.consumed.get())
        {
            return Err(Error::Unsupported);
        }
        let mut records = self.archived.clone();
        records.extend(self.local_history.iter().cloned());
        // Every issued ticket is permanently unavailable once its batch is gone;
        // live unconsumed batches refused above belong to the D3 pending seam.
        let consumed = (1..=self.next_ticket).collect();
        Ok(persistence::LocalHistory {
            next_ticket: self.next_ticket,
            epoch: self.epoch,
            records,
            consumed,
            events: self.events,
            bytes: self.bytes,
            replies: self.replies,
        })
    }
    pub(super) fn restore_local(&mut self, history: &persistence::LocalHistory) -> Result<()> {
        if self.next_ticket != 0 || !self.history.is_empty() || !self.archived.is_empty() {
            return Err(Error::Invalid);
        }
        self.next_ticket = history.next_ticket;
        self.epoch = history.epoch;
        self.events = history.events;
        self.bytes = history.bytes;
        self.replies = history.replies;
        self.archived = history.records.clone();
        Ok(())
    }
}
