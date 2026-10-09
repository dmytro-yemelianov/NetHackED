//! Concrete, byte-only composition of independent kernel/owner/service checkpoints.
//! Native codecs, bounded factories and effect-free drop retain their local trust
//! premises. Admission is composed here; no callback can mint a ready bundle.
use crate::runtime::{self, Binding, BuildContext, TrustedFactory};
use crate::{Hex, SaveLimits};
use nethacked_inputs::persistence as inputs;
use nethacked_kernel::persistence::{KernelSnapshot, RestoredKernel};
use nethacked_kernel::{Kernel, ModuleHandle, NativePort, Resource};
use nethacked_owner_services::{
    persistence as adapters, BehaviorHandle, CallbackBehaviors, Targets,
};
use nethacked_owners::{
    persistence as owners, Batch, Candidate, Host, Owner, OwnerId, RetainedBehavior,
};
use nethacked_resources::persistence as resources;
use nethacked_timers::persistence as timers;
use nethacked_workflows::persistence as workflows;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::any::Any;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

/// Structured details are additive: the established runtime error stays intact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadDiagnostic {
    pub section: String,
    pub dependency: Option<String>,
    pub reason: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadError {
    pub error: runtime::Error,
    pub diagnostics: Vec<LoadDiagnostic>,
}
thread_local! {
    static LOAD_DIAGNOSTICS: RefCell<Option<Rc<RefCell<Vec<LoadDiagnostic>>>>> = const { RefCell::new(None) };
}
fn diagnostic(section: &str, dependency: Option<String>, reason: String) {
    LOAD_DIAGNOSTICS.with(|slot| {
        if let Some(sink) = slot.borrow().as_ref() {
            sink.borrow_mut().push(LoadDiagnostic {
                section: section.into(),
                dependency,
                reason,
            });
        }
    });
}
fn dependency_refused<E: std::fmt::Debug>(
    section: &str,
    dependency: Option<String>,
    e: E,
) -> runtime::Error {
    diagnostic(section, dependency, format!("{e:?}"));
    runtime::Error::FactoryRefused
}
fn refused<E: std::fmt::Debug>(e: E) -> runtime::Error {
    diagnostic("session", None, format!("{e:?}"));
    runtime::Error::FactoryRefused
}
fn require_catalog<T>(
    section: &str,
    checkpoint: Option<&T>,
    catalog_present: bool,
    missing: impl FnOnce(&T) -> Option<String>,
) -> Result<(), runtime::Error> {
    if checkpoint.is_some() == catalog_present {
        return Ok(());
    }
    diagnostic(
        section,
        Some(
            checkpoint
                .and_then(missing)
                .unwrap_or_else(|| format!("{section} catalog")),
        ),
        if catalog_present {
            "catalog supplied without a saved service section"
        } else {
            "required service catalog is missing"
        }
        .into(),
    );
    Err(runtime::Error::MissingBinding)
}
const MAX: usize = 16 * 1024 * 1024;
/// Explicit typed enrollment metadata; contains no state aliases or credentials.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerRow {
    pub key: String,
    pub owner: OwnerId,
    pub target: Option<(String, Resource)>,
    pub behaviors: Vec<owners::BehaviorId>,
    pub routes: Vec<u64>,
    pub candidates: Vec<u64>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Section {
    name: String,
    local_payload: Hex,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    version: u64,
    capture_epoch: u64,
    owners: Vec<OwnerRow>,
    sections: Vec<Section>,
}
fn sections_within_limits(wire: &Wire, limits: SaveLimits) -> Result<Vec<Vec<u8>>, runtime::Error> {
    if wire.owners.len() > limits.rows_per_table || wire.sections.len() > 7 {
        return Err(runtime::Error::FactoryRefused);
    }
    let mut total = 0usize;
    let mut parts = Vec::new();
    for section in &wire.sections {
        total = total
            .checked_add(section.local_payload.0.len() / 2)
            .ok_or(runtime::Error::FactoryRefused)?;
        if total > limits.payload_bytes {
            return Err(runtime::Error::FactoryRefused);
        }
        parts.push(section.local_payload.bytes(limits).map_err(refused)?);
    }
    Ok(parts)
}
/// Uses D1's bounded writer, strict syntax, canonical encoding and digest framing.
/// Version 2 identifies concrete local-checkpoint composition; it cannot be parsed
/// as the declarative D1 catalog, and never implies authorization.
fn encode(wire: &Wire, limits: SaveLimits) -> Result<Vec<u8>, runtime::Error> {
    let limits = limits.checked().map_err(refused)?;
    let parts = sections_within_limits(wire, limits)?;
    let maximum = limits
        .wire_bytes
        .checked_sub(crate::HEADER_LEN)
        .ok_or(runtime::Error::FactoryRefused)?;
    let mut raw = crate::BoundedWriter {
        bytes: Vec::new(),
        maximum,
    };
    serde_json::to_writer(&mut raw, wire).map_err(refused)?;
    crate::syntax::check(&raw.bytes, limits, false).map_err(refused)?;
    let value: serde_json::Value = serde_json::from_slice(&raw.bytes).map_err(refused)?;
    let mut body = crate::BoundedWriter {
        bytes: Vec::new(),
        maximum,
    };
    serde_json::to_writer(&mut body, &value).map_err(refused)?;
    crate::syntax::check(&body.bytes, limits, true).map_err(refused)?;
    crate::syntax::check_many(&body.bytes, parts.iter().map(Vec::as_slice), limits)
        .map_err(refused)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(crate::HEADER_LEN + body.bytes.len())
        .map_err(|_| runtime::Error::Allocation)?;
    bytes.extend_from_slice(crate::MAGIC);
    bytes.extend_from_slice(&2u32.to_le_bytes());
    bytes.extend_from_slice(&(body.bytes.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&Sha256::digest(&body.bytes));
    bytes.extend_from_slice(&body.bytes);
    Ok(bytes)
}
fn decode(bytes: &[u8], limits: SaveLimits) -> Result<Wire, runtime::Error> {
    let limits = limits.checked().map_err(refused)?;
    if bytes.len() < crate::HEADER_LEN
        || bytes.len() > limits.wire_bytes
        || &bytes[..8] != crate::MAGIC
        || bytes[8..12] != 2u32.to_le_bytes()
    {
        return Err(runtime::Error::FactoryRefused);
    }
    let size = u64::from_le_bytes(bytes[12..20].try_into().map_err(refused)?);
    if usize::try_from(size)
        .ok()
        .and_then(|n| n.checked_add(crate::HEADER_LEN))
        != Some(bytes.len())
        || Sha256::digest(&bytes[crate::HEADER_LEN..]).as_slice() != &bytes[20..52]
    {
        return Err(runtime::Error::FactoryRefused);
    }
    crate::syntax::check(&bytes[crate::HEADER_LEN..], limits, true).map_err(refused)?;
    let wire: Wire = serde_json::from_slice(&bytes[crate::HEADER_LEN..]).map_err(refused)?;
    if wire.version != 1 || wire.capture_epoch == 0 || encode(&wire, limits)? != bytes {
        return Err(runtime::Error::FactoryRefused);
    }
    Ok(wire)
}

/// Erases type only at enrollment. Each implementation owns one independent
/// Owner<T> and uses its actual sealed capture/reconstruction APIs.
mod sealed {
    pub trait Allocation {}
    pub trait Factory {}
}
pub trait OwnerAllocation: Any + sealed::Allocation {
    fn as_any(&self) -> &dyn Any;
    fn key(&self) -> &str;
    fn roots(&self, roots: &mut owners::ExecutionRoots) -> Result<(), runtime::Error>;
    fn capture(
        &self,
        execution: &mut owners::ExecutionCapture<'_>,
        adapter: Option<&mut adapters::Capture>,
        batches: &[Rc<Batch>],
    ) -> Result<OwnerRow, runtime::Error>;
}
impl<T: 'static> sealed::Allocation for TypedOwner<T> {}
pub struct TypedOwner<T> {
    pub key: String,
    pub owner: Rc<Owner<T>>,
    pub codec: Rc<dyn owners::PersistenceCodec<T>>,
    pub catalog: owners::CaptureCatalog<T>,
    pub retained: Vec<RetainedBehavior<T>>,
    pub callbacks: Vec<BehaviorHandle<T>>,
    pub target_port: Option<String>,
    pub candidates: Vec<Candidate<T>>,
    /// Typed candidate serials residing in sealed heterogeneous batches.
    pub batch_candidates: Vec<u64>,
}
impl<T: 'static> OwnerAllocation for TypedOwner<T> {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn key(&self) -> &str {
        &self.key
    }
    fn roots(&self, roots: &mut owners::ExecutionRoots) -> Result<(), runtime::Error> {
        roots.owner(&self.owner).map_err(refused)?;
        for behavior in &self.retained {
            roots.behavior(behavior).map_err(refused)?;
        }
        for handle in &self.callbacks {
            handle.persistence_roots(roots).map_err(refused)?;
        }
        for candidate in self
            .candidates
            .iter()
            .filter(|c| c.persistence_is_pending())
        {
            roots.candidate(candidate).map_err(refused)?;
        }
        Ok(())
    }
    fn capture(
        &self,
        execution: &mut owners::ExecutionCapture<'_>,
        adapter: Option<&mut adapters::Capture>,
        batches: &[Rc<Batch>],
    ) -> Result<OwnerRow, runtime::Error> {
        let target = if let Some(port) = &self.target_port {
            Some((port.clone(), self.owner.resource(port).map_err(refused)?))
        } else {
            None
        };
        let behaviors = if let Some(port) = &self.target_port {
            if !self.retained.is_empty() {
                return Err(runtime::Error::Unsupported);
            }
            let handles = self.callbacks.iter().collect::<Vec<_>>();
            adapter
                .ok_or(runtime::Error::MissingBinding)?
                .owner(
                    &self.owner,
                    port,
                    &handles,
                    execution,
                    self.codec.as_ref(),
                    &self.catalog,
                )
                .map_err(refused)?;
            Vec::new()
        } else {
            execution
                .owner(
                    &self.owner,
                    self.codec.as_ref(),
                    &self.catalog,
                    &self.retained.iter().collect::<Vec<_>>(),
                )
                .map_err(refused)?
        };
        let mut candidates = Vec::new();
        for candidate in self
            .candidates
            .iter()
            .filter(|c| c.persistence_is_pending())
        {
            candidates.push(
                execution
                    .candidate(candidate, self.codec.as_ref(), &self.catalog)
                    .map_err(refused)?,
            );
        }
        for serial in &self.batch_candidates {
            let batch = batches
                .iter()
                .find(|b| b.persistence_candidate_serials().contains(serial))
                .ok_or(runtime::Error::FactoryRefused)?;
            if !batch.persistence_is_pending() {
                continue;
            }
            candidates.push(
                execution
                    .batch_candidate::<T>(batch, *serial, self.codec.as_ref(), &self.catalog)
                    .map_err(refused)?,
            );
        }
        Ok(OwnerRow {
            key: self.key.clone(),
            owner: self.owner.id(),
            target,
            behaviors,
            routes: Vec::new(),
            candidates,
        })
    }
}
/// Exact approved typed factories. A factory reconstructs a single owner through
/// local codec/catalog admission, never an unchecked whole-session callback.
pub trait OwnerFactory: sealed::Factory {
    fn admit(
        &self,
        row: &OwnerRow,
        checkpoint: &owners::ExecutionCheckpoint,
    ) -> Result<(), runtime::Error>;
    fn rebuild(
        &self,
        row: &OwnerRow,
        execution: &mut owners::ExecutionRestore,
        kernel: &Kernel,
        mapped: &RestoredKernel,
        adapter: Option<&mut adapters::Restore>,
    ) -> Result<Rc<dyn OwnerAllocation>, runtime::Error>;
}
pub struct TypedFactory<T> {
    pub codec: Rc<dyn owners::PersistenceCodec<T>>,
    pub catalog: owners::RestoreCatalog<T>,
}
impl<T> sealed::Factory for TypedFactory<T> {}
impl<T: 'static> OwnerFactory for TypedFactory<T> {
    fn admit(
        &self,
        row: &OwnerRow,
        checkpoint: &owners::ExecutionCheckpoint,
    ) -> Result<(), runtime::Error> {
        checkpoint
            .approve_owner(row.owner, self.codec.as_ref(), &self.catalog)
            .map_err(|e| {
                dependency_refused(
                    &row.key,
                    checkpoint.missing_owner_dependency(
                        row.owner,
                        self.codec.as_ref(),
                        &self.catalog,
                    ),
                    e,
                )
            })
    }
    fn rebuild(
        &self,
        row: &OwnerRow,
        execution: &mut owners::ExecutionRestore,
        kernel: &Kernel,
        mapped: &RestoredKernel,
        mut adapter: Option<&mut adapters::Restore>,
    ) -> Result<Rc<dyn OwnerAllocation>, runtime::Error> {
        let owner = execution
            .owner(
                kernel,
                mapped,
                row.owner,
                self.codec.as_ref(),
                &self.catalog,
            )
            .map_err(refused)?;
        let retained = row
            .behaviors
            .iter()
            .map(|id| execution.behavior::<T>(*id).map_err(refused))
            .collect::<Result<Vec<_>, _>>()?;
        let mut callbacks = Vec::new();
        if let Some((port, old)) = &row.target {
            let adapter = adapter.as_mut().ok_or(runtime::Error::MissingBinding)?;
            adapter
                .target(old, &owner, port, mapped, execution, kernel)
                .map_err(refused)?;
            for id in &row.routes {
                callbacks.push(
                    adapter
                        .behavior::<T>(*id, execution, kernel, mapped)
                        .map_err(refused)?,
                );
            }
        } else if !row.routes.is_empty() {
            return Err(runtime::Error::FactoryRefused);
        }
        let candidates = row
            .candidates
            .iter()
            .map(|serial| {
                execution
                    .candidate(
                        kernel,
                        mapped,
                        *serial,
                        &owner,
                        self.codec.as_ref(),
                        &self.catalog,
                    )
                    .map_err(refused)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let catalog = execution.capture_catalog(&owner).map_err(refused)?;
        Ok(Rc::new(TypedOwner {
            key: row.key.clone(),
            owner,
            codec: self.codec.clone(),
            catalog,
            retained,
            callbacks,
            target_port: row.target.as_ref().map(|t| t.0.clone()),
            candidates,
            batch_candidates: Vec::new(),
        }))
    }
}

/// Fresh stage-only handle routing for native artifact factories. It stores typed
/// handles/metadata, never payloads. Ports cannot obtain handles before admission.
#[derive(Default)]
pub struct Handles {
    owners: RefCell<BTreeMap<String, Rc<dyn OwnerAllocation>>>,
    host: RefCell<Option<Host>>,
    inputs: RefCell<Option<nethacked_inputs::Service>>,
    resources: RefCell<Option<nethacked_resources::Service>>,
    batches: RefCell<Vec<Rc<Batch>>>,
    cleanup: RefCell<Option<Rc<dyn nethacked_workflows::CleanupParticipant>>>,
}
impl Handles {
    /// One actual owner cleanup allocation shared by the service catalogs in
    /// this hidden reconstruction. Construction is subject to native premises.
    pub fn owner_cleanup(
        &self,
        factory: impl FnOnce(Host) -> Rc<dyn nethacked_workflows::CleanupParticipant>,
    ) -> nethacked_kernel::Result<Rc<dyn nethacked_workflows::CleanupParticipant>> {
        let mut held = self
            .cleanup
            .try_borrow_mut()
            .map_err(|_| nethacked_kernel::Error::Busy)?;
        let host = self.host()?;
        let identity = host
            .cleanup_identity()
            .map_err(|_| nethacked_kernel::Error::Authority)?;
        if held.is_none() {
            let value = factory(host);
            if value.identity()? != Some(identity) {
                return Err(nethacked_kernel::Error::Authority);
            }
            *held = Some(value);
        }
        Ok(held
            .as_ref()
            .ok_or(nethacked_kernel::Error::Authority)?
            .clone())
    }
    pub fn callback_behavior<T: 'static>(
        &self,
        key: &str,
        index: usize,
        view: &nethacked_kernel::CallerView<'_>,
    ) -> nethacked_kernel::Result<RetainedBehavior<T>> {
        let owners = self
            .owners
            .try_borrow()
            .map_err(|_| nethacked_kernel::Error::Busy)?;
        owners
            .get(key)
            .and_then(|o| o.as_any().downcast_ref::<TypedOwner<T>>())
            .and_then(|o| o.callbacks.get(index))
            .ok_or(nethacked_kernel::Error::Authority)?
            .downgrade()
            .for_execution(view)
    }
    pub fn commit_prepared(
        &self,
        view: &nethacked_kernel::CallerView<'_>,
    ) -> Result<nethacked_owners::CommitReceipt, nethacked_owners::Error> {
        let batches = self
            .batches
            .try_borrow()
            .map_err(|_| nethacked_owners::Error::Busy)?;
        if batches.len() != 1 {
            return Err(nethacked_owners::Error::Unsupported);
        }
        batches[0].commit(view)
    }
    pub fn host(&self) -> Result<Host, nethacked_kernel::Error> {
        self.host
            .try_borrow()
            .map_err(|_| nethacked_kernel::Error::Busy)?
            .clone()
            .ok_or(nethacked_kernel::Error::Authority)
    }
    pub fn owner<T: 'static>(&self, key: &str) -> Result<Rc<Owner<T>>, nethacked_kernel::Error> {
        let owners = self
            .owners
            .try_borrow()
            .map_err(|_| nethacked_kernel::Error::Busy)?;
        Ok(owners
            .get(key)
            .and_then(|o| o.as_any().downcast_ref::<TypedOwner<T>>())
            .ok_or(nethacked_kernel::Error::Authority)?
            .owner
            .clone())
    }
}
/// One capture plan for the actual, separately allocated service instances.
#[derive(Default)]
pub struct Services {
    pub workflows: Option<(nethacked_workflows::Workflows, workflows::CaptureCatalog)>,
    pub inputs: Option<nethacked_inputs::Service>,
    pub timers: Option<(nethacked_timers::Service, timers::CaptureCatalog)>,
    pub resources: Option<(nethacked_resources::Service, resources::CaptureCatalog)>,
    pub adapters: Option<(
        Rc<Targets>,
        Rc<CallbackBehaviors>,
        timers::Identity,
        ModuleHandle,
    )>,
}
pub struct HostBundle {
    kernel: Rc<RefCell<Kernel>>,
    host: Host,
    owners: Vec<Rc<dyn OwnerAllocation>>,
    batches: Vec<Rc<Batch>>,
    accounting: owners::AccountingCaptureCatalog,
    services: Services,
    session: nethacked_kernel::session::Participant,
}
impl HostBundle {
    /// Trusted initial host enrollment, with independent payload allocations.
    fn new(
        kernel: Kernel,
        host: Host,
        owners: Vec<Rc<dyn OwnerAllocation>>,
        batches: Vec<Batch>,
        accounting: owners::AccountingCaptureCatalog,
        services: Services,
    ) -> Self {
        Self {
            kernel: Rc::new(RefCell::new(kernel)),
            host,
            owners,
            batches: batches.into_iter().map(Rc::new).collect(),
            accounting,
            services,
            session: Default::default(),
        }
    }
    pub fn kernel(&self) -> Result<std::cell::Ref<'_, Kernel>, runtime::Error> {
        self.session.check_live()?;
        self.kernel.try_borrow().map_err(|_| runtime::Error::Busy)
    }
    /// Admission updates invalidate stages before exposing mutable kernel access.
    /// A replacement kernel is enrolled before this boundary opens again.
    pub fn update_kernel<R>(
        &self,
        update: impl FnOnce(&mut Kernel) -> R,
    ) -> Result<R, runtime::Error> {
        let _guard = self.session.enter()?;
        let mut kernel = self
            .kernel
            .try_borrow_mut()
            .map_err(|_| runtime::Error::Busy)?;
        let token = self
            .session
            .enrollment()?
            .ok_or(runtime::Error::ForeignDomain)?;
        self.session.changed()?;
        let result =
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| update(&mut kernel))) {
                Ok(result) => result,
                Err(panic) => {
                    token.fail();
                    std::panic::resume_unwind(panic);
                }
            };
        if let Err(error) = kernel.enroll_session(&token) {
            token.fail();
            return Err(refused(error));
        }
        Ok(result)
    }
    pub fn host(&self) -> &Host {
        &self.host
    }
    pub fn services(&self) -> &Services {
        &self.services
    }
    pub fn owner<T: 'static>(&self, key: &str) -> Option<&TypedOwner<T>> {
        self.owners
            .iter()
            .find(|o| o.key() == key)?
            .as_any()
            .downcast_ref()
    }
    pub fn batches(&self) -> &[Rc<Batch>] {
        &self.batches
    }
    pub fn enroll(
        &self,
        token: &nethacked_kernel::session::Enrollment,
    ) -> Result<(), runtime::Error> {
        self.session.enroll(token)?;
        self.kernel
            .try_borrow()
            .map_err(|_| runtime::Error::Busy)?
            .enroll_session(token)
            .map_err(refused)?;
        self.host.enroll_session(token).map_err(refused)?;
        if let Some((s, _)) = &self.services.workflows {
            s.enroll_session(token).map_err(refused)?;
        }
        if let Some(s) = &self.services.inputs {
            s.enroll_session(token).map_err(refused)?;
        }
        if let Some((s, _)) = &self.services.timers {
            s.enroll_session(token).map_err(refused)?;
        }
        if let Some((s, _)) = &self.services.resources {
            s.enroll_session(token).map_err(refused)?;
        }
        if let Some((t, c, _, _)) = &self.services.adapters {
            t.enroll_session(token).map_err(refused)?;
            c.enroll_session(token).map_err(refused)?;
        }
        Ok(())
    }
    fn save(&self, epoch: u64, limits: SaveLimits) -> Result<Vec<u8>, runtime::Error> {
        let mut kernel = self
            .kernel
            .try_borrow_mut()
            .map_err(|_| runtime::Error::Busy)?;
        let mut roots = owners::ExecutionRoots::default();
        for owner in &self.owners {
            owner.roots(&mut roots)?;
        }
        for batch in self.batches.iter().filter(|b| b.persistence_is_pending()) {
            roots.batch(batch).map_err(refused)?;
        }
        if let Some(s) = &self.services.inputs {
            roots.inputs(s).map_err(refused)?;
        }
        roots.preflight(&self.host).map_err(refused)?;
        let mut operations = roots.live_operations(&kernel).map_err(refused)?;
        let mut cleanup = vec![self.host.cleanup_identity().map_err(refused)?];
        let mut resources = vec![self.host.publication_resource()];
        if let Some((s, _)) = &self.services.workflows {
            operations.extend(s.persistence_enrollment(&kernel).map_err(refused)?);
        }
        if let Some(s) = &self.services.inputs {
            operations.extend(s.persistence_operations_for(&kernel).map_err(refused)?);
            cleanup.extend(s.persistence_cleanups().map_err(refused)?);
            resources.extend(s.persistence_resources().map_err(refused)?);
        }
        if let Some((s, _)) = &self.services.timers {
            operations.extend(s.persistence_enrollment().map_err(refused)?);
            cleanup.extend(s.persistence_cleanup().map_err(refused)?);
            resources.extend(s.persistence_resources().map_err(refused)?);
        }
        let mut history = Vec::new();
        if let Some((s, _)) = &self.services.resources {
            operations.extend(s.persistence_enrollment(&kernel).map_err(refused)?);
            history.extend(s.persistence_history().map_err(refused)?);
            let (c, r) = s.persistence_roots().map_err(refused)?;
            cleanup.push(c);
            resources.extend(r);
        }
        let mut unique = Vec::new();
        for op in operations {
            if !unique.contains(&op) {
                unique.push(op);
            }
        }
        let snapshot = kernel
            .kernel_snapshot_with_resource_roots(
                &roots.enrollment(&unique),
                &cleanup,
                &resources,
                &history,
            )
            .map_err(refused)?;
        let index = kernel.execution_capture_index(&snapshot).map_err(refused)?;
        let mut execution = self.host.capture_execution(&index).map_err(refused)?;
        let mut adapter = self
            .services
            .adapters
            .as_ref()
            .map(|(t, c, i, _)| adapters::Capture::new(t.clone(), c.clone(), i.clone()))
            .transpose()
            .map_err(refused)?;
        let mut rows = Vec::new();
        for owner in &self.owners {
            rows.push(owner.capture(&mut execution, adapter.as_mut(), &self.batches)?);
        }
        for batch in self.batches.iter().filter(|b| b.persistence_is_pending()) {
            execution.batch(batch, &self.accounting).map_err(refused)?;
        }
        if let Some(s) = &self.services.inputs {
            execution.inputs(s).map_err(refused)?;
        }
        let timer = self
            .services
            .timers
            .as_ref()
            .map(|(s, c)| s.capture_local(&index, c))
            .transpose()
            .map_err(refused)?;
        if let (Some(adapter), Some(timer), Some((service, _))) = (
            adapter.as_mut(),
            timer.as_ref(),
            self.services.timers.as_ref(),
        ) {
            for (task, reference) in service.persistence_references().map_err(refused)? {
                let row = timer
                    .tasks
                    .iter()
                    .find(|r| r.serial == task.serial)
                    .ok_or(runtime::Error::FactoryRefused)?;
                let callback = if let Some(callback) = row.callback {
                    callback
                } else {
                    // Terminal cleanup retains the authentic adapter scopes even
                    // after delivery has released the task's callback handle.
                    let mut matches = snapshot.callbacks.iter().filter(|c| {
                        c.pipeline.origin.as_ref().is_some_and(|o| {
                            o.caller == row.provenance.caller
                                && o.service == row.provenance.service
                                && o.capability == row.provenance.capability
                                && o.slot == row.provenance.slot
                                && o.source == row.provenance.source
                                && o.ceiling == row.ceiling
                        })
                    });
                    let callback = matches.next().ok_or(runtime::Error::MissingBinding)?.id;
                    if matches.next().is_some() {
                        return Err(runtime::Error::FactoryRefused);
                    }
                    callback
                };
                if adapter.bundle(reference, callback).map_err(refused)?
                    != row.reference.ok_or(runtime::Error::FactoryRefused)?
                {
                    return Err(runtime::Error::FactoryRefused);
                }
            }
        }
        let adapter = adapter
            .map(|c| {
                let service = self
                    .services
                    .adapters
                    .as_ref()
                    .ok_or(runtime::Error::MissingBinding)?
                    .3;
                c.finish(
                    snapshot.source_namespace,
                    index.module(service).map_err(refused)?,
                )
                .map_err(refused)
            })
            .transpose()?;
        if let Some(adapter) = &adapter {
            for row in &mut rows {
                if let Some((_, target)) = &row.target {
                    row.routes = adapter
                        .routes
                        .iter()
                        .filter(|r| &r.target == target)
                        .map(|r| r.id)
                        .collect();
                }
            }
        }
        let execution = execution.finish().map_err(refused)?;
        let mut sections = vec![
            section("kernel", canonical(&snapshot)?),
            section("owners", execution.to_bytes(MAX).map_err(refused)?),
        ];
        if let Some(input) = execution.input_checkpoint() {
            sections.push(section("inputs", input.to_bytes(MAX).map_err(refused)?));
        }
        if let Some((s, c)) = &self.services.workflows {
            sections.push(section(
                "workflows",
                s.capture_local(&kernel, &index, c)
                    .map_err(refused)?
                    .to_bytes(MAX)
                    .map_err(refused)?,
            ));
        }
        if let Some(timer) = timer {
            sections.push(section("timers", timer.to_bytes(MAX).map_err(refused)?));
        }
        if let Some(adapter) = adapter {
            sections.push(section("adapters", adapter.to_bytes(MAX).map_err(refused)?));
        }
        if let Some((s, c)) = &self.services.resources {
            sections.push(section(
                "resources",
                s.capture_local(&index, &snapshot, c)
                    .map_err(refused)?
                    .to_bytes(MAX)
                    .map_err(refused)?,
            ));
        }
        sections.sort_by(|a, b| a.name.cmp(&b.name));
        rows.sort_by(|a, b| a.key.cmp(&b.key));
        let bytes = encode(
            &Wire {
                version: 1,
                capture_epoch: epoch,
                owners: rows,
                sections,
            },
            limits,
        )?;
        Parsed::parse(&bytes, limits)?;
        Ok(bytes)
    }
}
fn canonical<T: Serialize>(value: &T) -> Result<Vec<u8>, runtime::Error> {
    serde_json::to_vec(&serde_json::to_value(value).map_err(refused)?).map_err(refused)
}
fn section(name: &str, bytes: Vec<u8>) -> Section {
    Section {
        name: name.into(),
        local_payload: Hex::from_bytes(&bytes),
    }
}

/// Catalog construction is local to a service or artifact. These interfaces do
/// not validate a whole session or receive a stage/slot/exchange capability.
/// Implementations must be immutable, bounded, source-free native factories.
pub trait KernelFactory {
    fn resolver(
        &self,
        handles: Rc<Handles>,
        ports: BTreeMap<(String, String), NativePort>,
    ) -> Result<Rc<dyn nethacked_kernel::persistence::RetainedAuthority>, runtime::Error>;
}
pub trait WorkflowFactory {
    fn catalog(&self, handles: Rc<Handles>) -> Result<workflows::RestoreCatalog, runtime::Error>;
}
pub trait TimerFactory {
    fn catalog(
        &self,
        handles: Rc<Handles>,
        adapters: Rc<AdapterPort>,
    ) -> Result<timers::RestoreCatalog, runtime::Error>;
}
pub type WorkflowApproval = (
    Rc<dyn WorkflowFactory>,
    Rc<dyn workflows::FrameAuthority>,
    nethacked_workflows::Limits,
);
pub type TimerApproval = (
    Rc<dyn TimerFactory>,
    Rc<dyn timers::TimerAuthority>,
    nethacked_timers::Limits,
);
/// The independent immutable approved configuration. Restore proofs bind this
/// complete catalog (including actual resolver/codecs) to HostSlot authority
/// identity+epoch. No Permission string supplies kernel grants or local approval.
pub struct RestoreAuthority {
    pub bindings: BTreeSet<Binding>,
    pub kernel: Rc<dyn KernelFactory>,
    pub owners: BTreeMap<String, Rc<dyn OwnerFactory>>,
    pub accounting: Rc<owners::AccountingRestoreCatalog>,
    pub workflows: Option<WorkflowApproval>,
    pub inputs: Option<(inputs::Catalog, nethacked_inputs::Limits)>,
    pub timers: Option<TimerApproval>,
    pub adapters: Option<(timers::Identity, Rc<dyn adapters::AdapterAuthority>)>,
    pub resources: Option<(
        resources::RestoreCatalog,
        Rc<dyn resources::ResourceAuthority>,
        nethacked_resources::Limits,
    )>,
    pub owner_limits: nethacked_owners::Limits,
    pub publication_limits: nethacked_owners::PublicationLimits,
    pub limits: SaveLimits,
}
/// Fresh, private indirection lets kernel ports exist before targets/scopes are
/// rebuilt. Every call refuses until complete admission. No source alias survives.
pub struct AdapterPort {
    old_targets: Vec<nethacked_resources::Stamp>,
    restored: RefCell<Option<Rc<adapters::Restored>>>,
}
impl AdapterPort {
    fn get(&self) -> nethacked_kernel::Result<Rc<adapters::Restored>> {
        self.restored
            .try_borrow()
            .map_err(|_| nethacked_kernel::Error::Busy)?
            .clone()
            .ok_or(nethacked_kernel::Error::Authority)
    }
}
impl nethacked_timers::TargetValidator for AdapterPort {
    fn validate(&self, targets: &[nethacked_resources::Stamp]) -> nethacked_kernel::Result<()> {
        nethacked_timers::TargetValidator::validate(self.get()?.targets().as_ref(), targets)
    }
}
impl timers::TargetCodec for AdapterPort {
    fn validate_old(&self, targets: &[nethacked_resources::Stamp]) -> nethacked_kernel::Result<()> {
        if targets.iter().all(|t| {
            self.old_targets.iter().any(|a| {
                a.address == t.address && a.generation == t.generation && t.revision <= a.revision
            })
        }) {
            Ok(())
        } else {
            Err(nethacked_kernel::Error::Authority)
        }
    }
    fn remap(
        &self,
        targets: &[nethacked_resources::Stamp],
        mapped: &RestoredKernel,
    ) -> nethacked_kernel::Result<Vec<nethacked_resources::Stamp>> {
        timers::TargetCodec::remap(self.get()?.as_ref(), targets, mapped)
    }
}
impl nethacked_timers::CallbackRetention for AdapterPort {
    fn preflight(
        &self,
        view: &nethacked_kernel::CallerView<'_>,
        targets: &[nethacked_resources::Stamp],
    ) -> nethacked_kernel::Result<()> {
        nethacked_timers::CallbackRetention::preflight(
            self.get()?.callbacks().as_ref(),
            view,
            targets,
        )
    }
    fn retain(
        &self,
        callback: &nethacked_kernel::RetainedCallback,
        targets: &[nethacked_resources::Stamp],
    ) -> nethacked_kernel::Result<Rc<dyn nethacked_timers::CallbackReference>> {
        nethacked_timers::CallbackRetention::retain(
            self.get()?.callbacks().as_ref(),
            callback,
            targets,
        )
    }
}
impl timers::RetentionRestore for AdapterPort {
    fn destination(&self) -> nethacked_kernel::persistence::ExecutionDestination {
        timers::RetentionRestore::destination(
            self.restored
                .borrow()
                .as_ref()
                .expect("private admitted adapter")
                .as_ref(),
        )
    }
    fn check(
        &self,
        row: &timers::TaskRow,
        mapped: &RestoredKernel,
    ) -> nethacked_kernel::Result<()> {
        timers::RetentionRestore::check(self.get()?.as_ref(), row, mapped)
    }
    fn reference(
        &self,
        id: u64,
    ) -> nethacked_kernel::Result<Rc<dyn nethacked_timers::CallbackReference>> {
        timers::RetentionRestore::reference(self.get()?.as_ref(), id)
    }
    fn finish_rebuild(&self) {
        if let Some(restored) = self.restored.borrow().as_ref() {
            timers::RetentionRestore::finish_rebuild(restored.as_ref());
        }
    }
}
struct MappedAuthority<'a> {
    base: &'a dyn nethacked_kernel::persistence::RetainedAuthority,
    owners: &'a owners::OwnerIdentityReservation,
    source: &'a KernelSnapshot,
}
impl nethacked_kernel::persistence::RegistryAuthority for MappedAuthority<'_> {
    fn resolve(
        &self,
        module: &nethacked_kernel::persistence::SavedModule,
    ) -> nethacked_kernel::Result<nethacked_kernel::persistence::ArtifactMaterial> {
        self.base.resolve(module).inspect_err(|e| {
            diagnostic(
                "kernel",
                Some(format!(
                    "artifact: {:?}; module: {:?}; executable: {:?}",
                    module.artifact, module.manifest.id, module.executable
                )),
                format!("{e:?}"),
            );
        })
    }
    fn bind_resource(&self, source: &Resource) -> nethacked_kernel::Result<Resource> {
        self.owners
            .resource(source)
            .or_else(|_| {
                self.base
                    .bind_resource(source)
                    .map_err(nethacked_owners::Error::from)
            })
            .map_err(|_| nethacked_kernel::Error::Authority)
    }
    fn approve_grant(
        &self,
        caller: &nethacked_kernel::persistence::SavedModule,
        slot: usize,
        target: &nethacked_kernel::persistence::SavedModule,
        _requested: &nethacked_kernel::Authority,
    ) -> nethacked_kernel::Result<nethacked_kernel::Authority> {
        use nethacked_kernel::{Authority, Scope};
        let original = self
            .source
            .registry
            .grants
            .iter()
            .find(|g| g.caller == caller.id && g.slot == slot && g.target == target.id)
            .ok_or(nethacked_kernel::Error::Authority)?;
        // Approval uses independent current policy over stable source identities.
        // Only this admitted owner reservation translates scopes to fresh IDs.
        let approved = self
            .base
            .approve_grant(caller, slot, target, &original.requested)?;
        Ok(Authority {
            access: approved.access,
            scope: match approved.scope {
                Scope::All => Scope::All,
                Scope::Keys(keys) => Scope::Keys(
                    keys.iter()
                        .map(|r| self.bind_resource(r))
                        .collect::<nethacked_kernel::Result<_>>()?,
                ),
            },
        })
    }
}
impl nethacked_kernel::persistence::RetainedAuthority for MappedAuthority<'_> {
    fn approve_cleanup(&self, id: u64) -> nethacked_kernel::Result<()> {
        self.base.approve_cleanup(id)
    }
    fn approve_operation_context(
        &self,
        context: &nethacked_kernel::persistence::SavedOperationContext,
    ) -> nethacked_kernel::Result<()> {
        self.base.approve_operation_context(context)
    }
}
pub struct SessionFactory {
    authority: RestoreAuthority,
}
impl SessionFactory {
    pub fn new(authority: RestoreAuthority) -> Self {
        Self { authority }
    }
    pub fn configuration(
        self,
        binding: Binding,
    ) -> Result<runtime::ApprovedConfiguration<HostBundle>, runtime::Error> {
        runtime::ApprovedConfiguration::new(
            BTreeSet::new(),
            self.authority.bindings.clone(),
            vec![(binding, Rc::new(self))],
        )
    }

    /// Pure, bounded inspection for additional host-profile state admission.
    /// Parsing and source pairing do not authorize restoration; `build` still
    /// applies the complete configured catalog and authority checks.
    pub fn inspect_owner_states(
        &self,
        bytes: &[u8],
    ) -> Result<(Vec<OwnerRow>, owners::ExecutionCheckpoint), runtime::Error> {
        let p = Parsed::parse(bytes, self.authority.limits)?;
        p.owners
            .preflight_source(
                &p.kernel,
                &self.authority.owner_limits,
                &self.authority.publication_limits,
            )
            .map_err(refused)?;
        Ok((p.wire.owners, p.owners))
    }
}
struct Parsed {
    wire: Wire,
    kernel: KernelSnapshot,
    owners: owners::ExecutionCheckpoint,
    workflows: Option<workflows::Checkpoint>,
    inputs: Option<inputs::Checkpoint>,
    timers: Option<timers::Checkpoint>,
    adapters: Option<adapters::Checkpoint>,
    resources: Option<resources::Checkpoint>,
}
impl Parsed {
    fn parse(bytes: &[u8], limits: SaveLimits) -> Result<Self, runtime::Error> {
        let wire = decode(bytes, limits)?;
        if wire.sections.windows(2).any(|w| w[0].name >= w[1].name)
            || wire.owners.windows(2).any(|w| w[0].key >= w[1].key)
        {
            return Err(runtime::Error::FactoryRefused);
        }
        let parts = sections_within_limits(&wire, limits)?;
        let mut sections = BTreeMap::new();
        for (section, bytes) in wire.sections.iter().zip(parts) {
            sections.insert(section.name.as_str(), bytes);
        }
        let mut take = |name: &str| sections.remove(name).ok_or(runtime::Error::FactoryRefused);
        let bytes = take("kernel")?;
        let kernel: KernelSnapshot = serde_json::from_slice(&bytes).map_err(refused)?;
        if canonical(&kernel)? != bytes {
            return Err(runtime::Error::FactoryRefused);
        }
        let owners =
            owners::ExecutionCheckpoint::from_bytes(&take("owners")?, MAX).map_err(refused)?;
        macro_rules! local {
            ($name:literal, $ty:ty) => {
                sections
                    .remove($name)
                    .map(|b| <$ty>::from_bytes(&b, MAX))
                    .transpose()
                    .map_err(refused)?
            };
        }
        let workflows = local!("workflows", workflows::Checkpoint);
        let inputs = local!("inputs", inputs::Checkpoint);
        let timers = local!("timers", timers::Checkpoint);
        let adapters = local!("adapters", adapters::Checkpoint);
        let resources = local!("resources", resources::Checkpoint);
        if !sections.is_empty() {
            return Err(runtime::Error::Unsupported);
        }
        let mut links = nethacked_kernel::persistence::ReferenceCounter::new();
        for row in &wire.owners {
            links.add(1); // outer owner root
            links.add(usize::from(row.target.is_some()));
            for b in &row.behaviors {
                links.add(1);
                links.add(usize::from(b.scope.is_some()));
            }
            links.add(row.routes.len());
            links.add(row.candidates.len());
        }
        kernel.count_references(&mut links);
        owners.count_references(&mut links);
        if let Some(c) = &workflows {
            c.count_references(&mut links);
        }
        if let Some(c) = &inputs {
            c.count_references(&mut links);
        }
        if let Some(c) = &timers {
            c.count_references(&mut links);
        }
        if let Some(c) = &adapters {
            c.count_references(&mut links);
        }
        if let Some(c) = &resources {
            c.count_references(&mut links);
        }
        if links.total().is_none_or(|n| n > limits.aggregate_edges) {
            return Err(runtime::Error::FactoryRefused);
        }
        // EXACT shared source checkpoint/typed-reference admission, before codecs,
        // factories, policy construction or fresh service allocation.
        kernel.validate_execution_metadata().map_err(refused)?;
        if let Some(c) = &workflows {
            c.preflight_source(&kernel).map_err(refused)?;
        }
        if let Some(c) = &timers {
            c.preflight_source(&kernel).map_err(refused)?;
            for task in &c.tasks {
                if let Some(reference) = task.reference {
                    let bundle = adapters
                        .as_ref()
                        .and_then(|a| a.bundles.iter().find(|b| b.id == reference))
                        .ok_or(runtime::Error::FactoryRefused)?;
                    if task
                        .callback
                        .is_some_and(|callback| callback != bundle.callback)
                    {
                        return Err(runtime::Error::FactoryRefused);
                    }
                }
            }
        }
        if let Some(c) = &resources {
            c.preflight_source(&kernel).map_err(refused)?;
        }
        if let Some(c) = &adapters {
            if c.kernel_namespace != kernel.source_namespace {
                return Err(runtime::Error::FactoryRefused);
            }
        }
        if let Some(c) = &inputs {
            c.validate_checkpoint(&nethacked_inputs::Limits::default(), &kernel)
                .map_err(refused)?;
            if owners
                .input_checkpoint()
                .ok_or(runtime::Error::FactoryRefused)?
                .to_bytes(MAX)
                .map_err(refused)?
                != c.to_bytes(MAX).map_err(refused)?
            {
                return Err(runtime::Error::FactoryRefused);
            }
        } else if owners.input_checkpoint().is_some() {
            return Err(runtime::Error::FactoryRefused);
        }
        let live = owners.owners().into_iter().collect::<BTreeSet<_>>();
        let enrolled = wire.owners.iter().map(|r| r.owner).collect::<BTreeSet<_>>();
        if enrolled.len() != wire.owners.len() || enrolled != live {
            return Err(runtime::Error::FactoryRefused);
        }
        let mut candidates = BTreeSet::new();
        let mut behavior_references = Vec::new();
        let mut route_references = BTreeSet::new();
        for row in &wire.owners {
            owners
                .preflight_owner_candidates(row.owner, &row.candidates)
                .map_err(refused)?;
            for id in &row.behaviors {
                if owners.reference_owner(*id).map_err(refused)? != row.owner {
                    return Err(runtime::Error::FactoryRefused);
                }
                behavior_references.push(*id);
            }
            if let Some((port, target)) = &row.target {
                owners
                    .preflight_owner_target(row.owner, port, target)
                    .map_err(refused)?;
                if !row.behaviors.is_empty() {
                    return Err(runtime::Error::FactoryRefused);
                }
                let adapter = adapters.as_ref().ok_or(runtime::Error::FactoryRefused)?;
                for id in &row.routes {
                    if !route_references.insert(*id) {
                        return Err(runtime::Error::FactoryRefused);
                    }
                    let route = adapter
                        .routes
                        .iter()
                        .find(|r| r.id == *id && &r.target == target)
                        .ok_or(runtime::Error::FactoryRefused)?;
                    let behavior = owners::BehaviorId {
                        behavior: route.behavior.behavior,
                        scope: route.behavior.scope,
                    };
                    if owners.reference_owner(behavior).map_err(refused)? != row.owner {
                        return Err(runtime::Error::FactoryRefused);
                    }
                    behavior_references.push(behavior);
                }
            } else if !row.routes.is_empty() {
                return Err(runtime::Error::FactoryRefused);
            }
            for id in &row.candidates {
                if !candidates.insert(*id) {
                    return Err(runtime::Error::FactoryRefused);
                }
            }
        }
        if candidates != owners.candidate_ids().into_iter().collect() {
            return Err(runtime::Error::FactoryRefused);
        }
        if let Some(adapter) = &adapters {
            if route_references != adapter.routes.iter().map(|r| r.id).collect() {
                return Err(runtime::Error::FactoryRefused);
            }
            for bundle in &adapter.bundles {
                for id in &bundle.scopes {
                    behavior_references.push(owners::BehaviorId {
                        behavior: id.behavior,
                        scope: id.scope,
                    });
                }
            }
        }
        owners
            .preflight_behavior_references(&behavior_references)
            .map_err(refused)?;
        Ok(Self {
            wire,
            kernel,
            owners,
            workflows,
            inputs,
            timers,
            adapters,
            resources,
        })
    }
}
impl TrustedFactory<HostBundle> for SessionFactory {
    fn build(
        &self,
        context: &mut BuildContext<'_, HostBundle>,
        bytes: &[u8],
    ) -> Result<HostBundle, runtime::Error> {
        let a = &self.authority;
        for binding in &a.bindings {
            context.require_binding(binding).inspect_err(|e| {
                diagnostic(
                    "session",
                    Some(format!("binding: {binding:?}")),
                    format!("{e:?}"),
                );
            })?;
        }
        let p = Parsed::parse(bytes, a.limits)?;
        p.owners
            .preflight_source(&p.kernel, &a.owner_limits, &a.publication_limits)
            .map_err(refused)?;
        require_catalog(
            "workflows",
            p.workflows.as_ref(),
            a.workflows.is_some(),
            |c| workflows::RestoreCatalog::default().missing_dependency(c),
        )?;
        require_catalog("inputs", p.inputs.as_ref(), a.inputs.is_some(), |c| {
            inputs::Catalog::default().missing_dependency(c)
        })?;
        require_catalog("timers", p.timers.as_ref(), a.timers.is_some(), |c| {
            Some(format!(
                "timer profile: {:?}; targets: {:?}; retention: {:?}",
                c.profile, c.targets, c.retention
            ))
        })?;
        require_catalog("adapters", p.adapters.as_ref(), a.adapters.is_some(), |c| {
            Some(format!("adapter artifact: {:?}", c.identity))
        })?;
        require_catalog(
            "resources",
            p.resources.as_ref(),
            a.resources.is_some(),
            |c| resources::RestoreCatalog::default().missing_dependency(c),
        )?;
        for row in &p.wire.owners {
            a.owners
                .get(&row.key)
                .ok_or_else(|| {
                    diagnostic(
                        "owners",
                        Some(format!("owner factory: {:?}", row.key)),
                        "MissingFactory".into(),
                    );
                    runtime::Error::MissingFactory
                })?
                .admit(row, &p.owners)?;
        }
        a.accounting.approve_checkpoint(&p.owners).map_err(|e| {
            dependency_refused("owners", a.accounting.missing_dependency(&p.owners), e)
        })?;
        let reservation = owners::OwnerIdentityReservation::new(
            p.owners.clone(),
            &p.kernel,
            a.owner_limits.clone(),
            a.publication_limits.clone(),
        )
        .map_err(refused)?;
        let handles = Rc::new(Handles::default());
        let adapter = Rc::new(AdapterPort {
            old_targets: p
                .adapters
                .as_ref()
                .map(|c| c.targets.clone())
                .unwrap_or_default(),
            restored: RefCell::new(None),
        });
        let mut ports = BTreeMap::new();
        let adapter_valid =
            if let (Some(c), Some((identity, authority))) = (&p.adapters, &a.adapters) {
                Some(
                    c.validate(&p.kernel, &p.owners, identity, authority.as_ref())
                        .map_err(|e| {
                            dependency_refused(
                                "adapters",
                                Some(format!("adapter artifact: {:?}", c.identity)),
                                e,
                            )
                        })?,
                )
            } else {
                None
            };
        let workflow_catalog = a
            .workflows
            .as_ref()
            .map(|(f, _, _)| f.catalog(handles.clone()))
            .transpose()?;
        let timer_catalog = a
            .timers
            .as_ref()
            .map(|(f, _, _)| f.catalog(handles.clone(), adapter.clone()))
            .transpose()?;
        // Every local graph is admitted before the first executable artifact is built.
        let workflow = if let (Some(c), Some(catalog), Some((_, authority, limits))) =
            (&p.workflows, &workflow_catalog, &a.workflows)
        {
            Some(
                c.validate(&p.kernel, limits.clone(), catalog, authority.as_ref())
                    .map_err(|e| {
                        dependency_refused("workflows", catalog.missing_dependency(c), e)
                    })?,
            )
        } else {
            None
        };
        let timer = if let (Some(c), Some(catalog), Some((_, authority, limits))) =
            (&p.timers, &timer_catalog, &a.timers)
        {
            Some(
                c.validate(&p.kernel, catalog, authority.as_ref(), limits)
                    .map_err(|e| dependency_refused("timers", catalog.missing_dependency(c), e))?,
            )
        } else {
            None
        };
        let resource_stage = if let (Some(c), Some((catalog, authority, limits))) =
            (&p.resources, &a.resources)
        {
            Some(
                c.clone()
                    .validate(limits, &p.kernel, catalog.clone(), authority.as_ref())
                    .map_err(|e| dependency_refused("resources", catalog.missing_dependency(c), e))?
                    .stage()
                    .map_err(refused)?,
            )
        } else {
            None
        };
        if let (Some(c), Some(stage)) = (&p.resources, &resource_stage) {
            if let Some(binding) = &c.binding {
                let name = p
                    .kernel
                    .registry
                    .modules
                    .iter()
                    .find(|m| m.id == binding.module)
                    .ok_or(runtime::Error::FactoryRefused)?
                    .manifest
                    .id
                    .clone();
                for (endpoint, port) in stage.ports() {
                    ports.insert((name.clone(), endpoint), port);
                }
            }
        }
        let input_stage = if let (Some(c), Some((catalog, limits))) = (&p.inputs, &a.inputs) {
            Some(
                inputs::Restore::prepare(c.clone(), limits.clone(), &p.kernel, catalog)
                    .map_err(|e| dependency_refused("inputs", catalog.missing_dependency(c), e))?,
            )
        } else {
            None
        };
        if let (Some(c), Some(stage)) = (&p.inputs, &input_stage) {
            ports.insert(c.native_binding(&p.kernel).map_err(refused)?, stage.port());
        }
        if let (Some(c), Some(catalog)) = (&p.timers, &timer_catalog) {
            let name = p
                .kernel
                .registry
                .modules
                .iter()
                .find(|m| m.id == c.service)
                .ok_or(runtime::Error::FactoryRefused)?
                .manifest
                .id
                .clone();
            ports.insert((name, c.capability.endpoint.clone()), catalog.native_port());
        }
        let resolver = a.kernel.resolver(handles.clone(), ports)?;
        let mut kernel = Kernel::new(p.kernel.registry.limits.clone()).map_err(refused)?;
        let mapped = kernel
            .restore_kernel(
                &p.kernel,
                &MappedAuthority {
                    base: resolver.as_ref(),
                    owners: &reservation,
                    source: &p.kernel,
                },
            )
            .map_err(refused)?;
        let mut execution = reservation.bind(&kernel, &mapped).map_err(refused)?;
        execution
            .approve_accounting(&a.accounting)
            .map_err(refused)?;
        let inputs = input_stage
            .map(|s| s.finish(&kernel, &mapped))
            .transpose()
            .map_err(refused)?;
        if let Some(input) = inputs {
            let service = input.service().clone();
            execution.inputs(input).map_err(refused)?;
            handles.inputs.replace(Some(service));
        }
        let resources = resource_stage
            .map(|s| s.finish(&kernel, &mapped))
            .transpose()
            .map_err(refused)?;
        if let Some(resources) = &resources {
            handles.resources.replace(Some(resources.service().clone()));
        }
        let mut adapter_restore = adapter_valid
            .map(|v| v.rebuild(&kernel, &mapped, &execution))
            .transpose()
            .map_err(refused)?;
        let mut owners = Vec::new();
        for row in &p.wire.owners {
            let owner = a.owners[&row.key].rebuild(
                row,
                &mut execution,
                &kernel,
                &mapped,
                adapter_restore.as_mut(),
            )?;
            handles
                .owners
                .borrow_mut()
                .insert(row.key.clone(), owner.clone());
            owners.push(owner);
        }
        let batches = p
            .owners
            .batch_ids()
            .into_iter()
            .map(|ticket| {
                execution
                    .batch(&kernel, &mapped, ticket, &a.accounting)
                    .map_err(refused)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let host = execution.finish().map_err(refused)?;
        handles.host.replace(Some(host.clone()));
        let adapters = adapter_restore
            .map(|s| s.finish())
            .transpose()
            .map_err(refused)?;
        if let Some(restored) = &adapters {
            adapter.restored.replace(Some(restored.clone()));
        }
        let workflows = workflow
            .map(|s| {
                s.rebuild(
                    &kernel,
                    &mapped,
                    workflow_catalog
                        .as_ref()
                        .ok_or(runtime::Error::MissingBinding)?,
                )
                .map_err(refused)
            })
            .transpose()?;
        let timers = timer
            .map(|s| {
                s.rebuild(
                    &kernel,
                    &mapped,
                    timer_catalog
                        .as_ref()
                        .ok_or(runtime::Error::MissingBinding)?,
                )
                .map_err(refused)
            })
            .transpose()?;
        let workflow_capture = if let (Some(s), Some(c), Some(catalog)) =
            (&workflows, &p.workflows, &workflow_catalog)
        {
            Some(catalog.capture_catalog(c, s, &mapped).map_err(refused)?)
        } else {
            None
        };
        let timer_capture =
            if let (Some(s), Some(c), Some(catalog)) = (&timers, &p.timers, &timer_catalog) {
                Some(catalog.capture_catalog(c, s).map_err(refused)?)
            } else {
                None
            };
        let resource_capture = if let (Some(s), Some(c), Some((catalog, _, _))) =
            (&resources, &p.resources, &a.resources)
        {
            Some(catalog.capture_catalog(c, s.service()).map_err(refused)?)
        } else {
            None
        };
        let services = Services {
            workflows: workflows.zip(workflow_capture),
            inputs: handles.inputs.borrow().clone(),
            timers: timers.zip(timer_capture),
            resources: resources.map(|s| s.service().clone()).zip(resource_capture),
            adapters: adapters
                .map(|s| -> Result<_, runtime::Error> {
                    Ok((
                        s.targets(),
                        s.callbacks(),
                        p.adapters
                            .as_ref()
                            .ok_or(runtime::Error::MissingBinding)?
                            .identity
                            .clone(),
                        *mapped
                            .modules()
                            .get(
                                &p.adapters
                                    .as_ref()
                                    .ok_or(runtime::Error::MissingBinding)?
                                    .service,
                            )
                            .ok_or(runtime::Error::MissingBinding)?,
                    ))
                })
                .transpose()?,
        };
        let accounting = a.accounting.capture_catalog(&batches).map_err(refused)?;
        let bundle = HostBundle::new(kernel, host, owners, batches, accounting, services);
        handles.batches.replace(bundle.batches.clone());
        bundle.enroll(context.enrollment()?)?;
        Ok(bundle)
    }
}
/// Source capture and destination enrollment always use the accepted HostSlot.
pub struct Session {
    slot: runtime::HostSlot<HostBundle>,
    authority_epoch: std::cell::Cell<u64>,
}
impl Session {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        kernel: Kernel,
        host: Host,
        owners: Vec<Rc<dyn OwnerAllocation>>,
        batches: Vec<Batch>,
        accounting: owners::AccountingCaptureCatalog,
        services: Services,
        configuration: runtime::ApprovedConfiguration<HostBundle>,
    ) -> Result<(Self, runtime::AuthorityKey, runtime::RuntimeCredential), runtime::Error> {
        let bundle = HostBundle::new(kernel, host, owners, batches, accounting, services);
        let (slot, key, credential) =
            runtime::HostSlot::new(bundle, configuration, runtime::Backend::Serial)?;
        let token = slot.enrollment(&key)?;
        slot.capture(&key, |bundle| bundle.enroll(&token))?;
        let authority_epoch = std::cell::Cell::new(slot.capture_epoch());
        Ok((
            Self {
                slot,
                authority_epoch,
            },
            key,
            credential,
        ))
    }
    pub fn capture(
        &self,
        key: &runtime::AuthorityKey,
        limits: SaveLimits,
    ) -> Result<Vec<u8>, runtime::Error> {
        self.slot
            .capture(key, |bundle| bundle.save(self.slot.capture_epoch(), limits))
    }
    pub fn with_runtime<R>(
        &self,
        credential: &runtime::RuntimeCredential,
        call: impl FnOnce(&HostBundle) -> R,
    ) -> Result<R, runtime::Error> {
        self.slot.with_runtime(credential, |bundle| call(bundle))
    }
    pub fn issue_credential(
        &self,
        key: &runtime::AuthorityKey,
    ) -> Result<runtime::RuntimeCredential, runtime::Error> {
        self.slot.issue_credential(key, BTreeSet::new())
    }
    pub fn prepare_restore(
        &self,
        key: &runtime::AuthorityKey,
        factory: &Binding,
        bytes: &[u8],
    ) -> Result<runtime::RestoreStage<HostBundle>, runtime::Error> {
        // Raw enrolled admission changes invalidate the immutable authority's
        // enrollment as well as existing stages. Only an explicit independent
        // configuration update can authorize another hidden reconstruction.
        if self.authority_epoch.get() != self.slot.capture_epoch() {
            return Err(runtime::Error::AuthorityChanged);
        }
        self.slot.prepare_restore(key, factory, bytes)
    }
    /// Same admission and established error as `prepare_restore`, with exact
    /// unavailable dependencies and local refusal reasons retained for callers.
    pub fn prepare_restore_diagnosed(
        &self,
        key: &runtime::AuthorityKey,
        factory: &Binding,
        bytes: &[u8],
    ) -> Result<runtime::RestoreStage<HostBundle>, LoadError> {
        struct Scope(Option<Rc<RefCell<Vec<LoadDiagnostic>>>>);
        impl Drop for Scope {
            fn drop(&mut self) {
                LOAD_DIAGNOSTICS.with(|s| {
                    s.replace(self.0.take());
                });
            }
        }
        let sink = Rc::new(RefCell::new(Vec::new()));
        let _scope = Scope(LOAD_DIAGNOSTICS.with(|s| s.replace(Some(sink.clone()))));
        self.prepare_restore(key, factory, bytes)
            .map_err(|error| LoadError {
                diagnostics: {
                    let mut details = sink.borrow().clone();
                    if details.is_empty() {
                        details.push(LoadDiagnostic {
                            section: "session".into(),
                            dependency: None,
                            reason: format!("{error:?}"),
                        });
                    }
                    details
                },
                error,
            })
    }
    pub fn activate(
        &self,
        stage: &mut runtime::RestoreStage<HostBundle>,
    ) -> Result<runtime::Activation, runtime::Error> {
        self.slot.activate(stage)
    }
    pub fn replace_configuration(
        &self,
        key: &runtime::AuthorityKey,
        configuration: runtime::ApprovedConfiguration<HostBundle>,
    ) -> Result<(), runtime::Error> {
        self.slot.replace_configuration(key, configuration)?;
        self.authority_epoch.set(self.slot.capture_epoch());
        Ok(())
    }
}

#[cfg(test)]
mod repair_tests;
