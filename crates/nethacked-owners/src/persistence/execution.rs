//! PASS1 owner-local execution identities. D4 owns whole-bundle activation.
use super::*;
mod prepared;
use crate::behavior::{Behavior, CallbackScope};
use nethacked_kernel::persistence::{
    execution_capture_alias, ExecutionCaptureIndex, KernelEnrollment, KernelSnapshot,
    RestoredKernel, SavedOperation,
};
pub use prepared::*;

/// Trusted capture registration binds a native build to its actual allocation.
/// Codec fidelity, purity, bounds, fresh factories and no payload aliases are
/// explicit host premises. No pointer or closure enters the checkpoint.
pub struct CaptureCatalog<T> {
    entries: Vec<(CodecIdentity, QueryPolicy, Backend<T>)>,
}
impl<T> Default for CaptureCatalog<T> {
    fn default() -> Self {
        Self { entries: vec![] }
    }
}
impl<T> CaptureCatalog<T> {
    pub fn register(
        &mut self,
        identity: CodecIdentity,
        policy: QueryPolicy,
        backend: Backend<T>,
    ) -> Result<()> {
        identity.validate()?;
        if self.entries.len() >= 65_536 {
            return Err(Error::Limit);
        }
        if self
            .entries
            .iter()
            .any(|(i, p, b)| i == &identity && p == &policy && same_backend(b, &backend))
        {
            return Err(Error::Invalid);
        }
        backend.schema()?;
        self.entries.push((identity, policy, backend));
        Ok(())
    }
    fn identify(
        &self,
        backend: &Backend<T>,
        policy: &QueryPolicy,
        schema: [u8; 32],
    ) -> Result<CodecIdentity> {
        let mut found = self.entries.iter().filter(|(i, p, b)| {
            i.schema_digest == schema && p == policy && same_backend(b, backend)
        });
        let id = found.next().ok_or(Error::Schema)?.0.clone();
        if found.next().is_some() {
            return Err(Error::Invalid);
        }
        Ok(id)
    }
}
fn same_backend<T>(a: &Backend<T>, b: &Backend<T>) -> bool {
    match (a, b) {
        (Backend::Native(a), Backend::Native(b)) => Rc::ptr_eq(a, b),
        (Backend::Wasm { codec: a, .. }, Backend::Wasm { codec: b, .. }) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
type Factory<T> = Box<dyn Fn(&ExecutionRemap<'_>) -> Result<Backend<T>>>;
/// Independently supplied allowlist, never inferred from incoming metadata.
pub struct RestoreCatalog<T> {
    entries: Vec<(CodecIdentity, QueryPolicy, Factory<T>)>,
}
impl<T> Default for RestoreCatalog<T> {
    fn default() -> Self {
        Self { entries: vec![] }
    }
}
impl<T> RestoreCatalog<T> {
    pub fn allow(
        &mut self,
        identity: CodecIdentity,
        policy: QueryPolicy,
        factory: impl Fn(&ExecutionRemap<'_>) -> Result<Backend<T>> + 'static,
    ) -> Result<()> {
        identity.validate()?;
        if self.entries.len() >= 65_536 {
            return Err(Error::Limit);
        }
        if self
            .entries
            .iter()
            .any(|(i, p, _)| i == &identity && p == &policy)
        {
            return Err(Error::Invalid);
        }
        self.entries.push((identity, policy, Box::new(factory)));
        Ok(())
    }
    fn resolve(&self, identity: &CodecIdentity, policy: &QueryPolicy) -> Result<&Factory<T>> {
        self.entries
            .iter()
            .find(|(i, p, _)| i == identity && p == policy)
            .map(|(_, _, f)| f)
            .ok_or(Error::Schema)
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BindingRow {
    module: u64,
    artifact: [u8; 32],
    endpoint: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PermitRow {
    capability: Capability,
    resolution: Vec<BindingRow>,
}
fn save_binding(b: &ProviderBinding, index: &ExecutionCaptureIndex<'_>) -> Result<BindingRow> {
    Ok(BindingRow {
        module: index.module(b.handle)?,
        artifact: b.artifact,
        endpoint: b.endpoint.clone(),
    })
}
fn save_permit(p: &CapabilityPermit, index: &ExecutionCaptureIndex<'_>) -> Result<PermitRow> {
    Ok(PermitRow {
        capability: p.capability.clone(),
        resolution: p
            .resolution
            .iter()
            .map(|b| save_binding(b, index))
            .collect::<Result<_>>()?,
    })
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CurrentRow {
    owner: OwnerId,
    module: u64,
    build: CodecIdentity,
    behavior_revision: u64,
    retained_count: usize,
    pin: u64,
    enrollment_pin: Option<u64>,
    compatible_pin: Option<u64>,
    permit: Option<PermitRow>,
    #[cfg(feature = "recorded-inputs")]
    recorded: BTreeMap<String, Vec<DeclarationRow>>,
}
#[cfg(feature = "recorded-inputs")]
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeclarationRow {
    policy: String,
    version: u32,
    provider: BindingRow,
    request: Vec<u8>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BehaviorRow {
    id: u64,
    owner: OwnerId,
    generation: u64,
    schema: [u8; 32],
    caller: u64,
    artifact: [u8; 32],
    source: SavedOperation,
    build: CodecIdentity,
    slot: usize,
    capture: usize,
    revision: u64,
    policy: QueryPolicy,
    target: Resource,
    pin: u64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScopeRow {
    id: u64,
    behavior: u64,
    callback: u64,
}
/// IDs refer to shared rows; copying an ID never creates another reservation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BehaviorId {
    pub behavior: u64,
    pub scope: Option<u64>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionCheckpoint {
    version: u32,
    kernel_namespace: u64,
    state: Checkpoint,
    cleanup: u64,
    current: Vec<CurrentRow>,
    behaviors: Vec<BehaviorRow>,
    scopes: Vec<ScopeRow>,
    #[serde(default)]
    pending: PendingCheckpoint,
}
impl ExecutionCheckpoint {
    pub fn to_bytes(&self, limit: usize) -> Result<Vec<u8>> {
        let b = serde_json::to_vec(&serde_json::to_value(self).map_err(|_| Error::Invalid)?)
            .map_err(|_| Error::Invalid)?;
        bound(b.len(), limit.min(64 * 1024 * 1024))?;
        Ok(b)
    }
    pub fn from_bytes(bytes: &[u8], limit: usize) -> Result<Self> {
        bound(bytes.len(), limit.min(64 * 1024 * 1024))?;
        let value: Self = serde_json::from_slice(bytes).map_err(|_| Error::Invalid)?;
        let supplied: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|_| Error::Invalid)?;
        let mut canonical = serde_json::to_value(&value).map_err(|_| Error::Invalid)?;
        // PASS1 V1 had no pending table. Its strict canonical spelling loads as
        // an empty pending graph; new captures emit complete retirement ranges.
        if supplied
            .as_object()
            .is_some_and(|o| !o.contains_key("pending"))
        {
            canonical
                .as_object_mut()
                .ok_or(Error::Invalid)?
                .remove("pending");
        }
        if serde_json::to_vec(&canonical).map_err(|_| Error::Invalid)? != bytes {
            return Err(Error::Invalid);
        }
        Ok(value)
    }
    pub fn owners(&self) -> Vec<OwnerId> {
        self.state.owners()
    }
}
/// Explicitly enroll all externally held owner roots. Shared behavior clones
/// contribute one pure capture; shared operation contexts contribute one row.
#[derive(Default)]
pub struct ExecutionRoots {
    allocations: BTreeSet<usize>,
    operations: Vec<OperationBinding>,
    captures: Vec<PureCapture>,
}
impl ExecutionRoots {
    pub fn behavior<T>(&mut self, behavior: &RetainedBehavior<T>) -> Result<()> {
        if self.allocations.len() >= 65_536 {
            return Err(Error::Limit);
        }
        if self
            .allocations
            .insert(Rc::as_ptr(&behavior.inner) as usize)
        {
            if !self.operations.contains(behavior.source()) {
                self.operations.push(behavior.source().clone());
            }
            if !self
                .captures
                .iter()
                .any(|c| execution_capture_alias(c, &behavior.inner.capture))
            {
                self.captures.push(behavior.inner.capture.clone());
            }
        }
        Ok(())
    }
    pub fn live_operations(&self, kernel: &Kernel) -> Result<Vec<OperationBinding>> {
        let mut operations = vec![];
        for op in &self.operations {
            if kernel.execution_source_live(op)? {
                operations.push(op.clone());
            }
        }
        Ok(operations)
    }
    pub fn enrollment<'a>(&'a self, live: &'a [OperationBinding]) -> KernelEnrollment<'a> {
        KernelEnrollment {
            operations: live,
            pure_captures: &self.captures,
        }
    }
}
/// Metadata only; payloads remain inside their separately allocated owners.
pub struct ExecutionCapture<'a> {
    state: Capture,
    index: &'a ExecutionCaptureIndex<'a>,
    current: Vec<CurrentRow>,
    behaviors: Vec<BehaviorRow>,
    scopes: Vec<ScopeRow>,
    allocations: BTreeMap<usize, u64>,
    scope_allocations: BTreeMap<usize, u64>,
    cleanup: u64,
    pending: PendingCapture,
}
impl Host {
    pub fn capture_execution<'a>(
        &self,
        index: &'a ExecutionCaptureIndex<'a>,
    ) -> Result<ExecutionCapture<'a>> {
        let state = self.capture_state_mode(true)?;
        let cleanup = index.cleanup(self.0.cleanup)?;
        Ok(ExecutionCapture {
            state,
            index,
            current: vec![],
            behaviors: vec![],
            scopes: vec![],
            allocations: BTreeMap::new(),
            scope_allocations: BTreeMap::new(),
            cleanup,
            pending: PendingCapture::default(),
        })
    }
}
impl ExecutionCapture<'_> {
    pub fn owner<T: 'static>(
        &mut self,
        owner: &Rc<Owner<T>>,
        codec: &dyn PersistenceCodec<T>,
        catalog: &CaptureCatalog<T>,
        behaviors: &[&RetainedBehavior<T>],
    ) -> Result<Vec<BehaviorId>> {
        if self.state.captured.contains(&owner.id()) {
            return Err(Error::Invalid);
        }
        let policy = owner.query_policy.borrow().clone();
        let build = catalog.identify(
            &owner.backend.borrow(),
            &policy,
            owner.metadata.stamp.schema,
        )?;
        let current = CurrentRow {
            owner: owner.id(),
            module: self.index.module(owner.metadata.stamp.module.handle)?,
            build,
            behavior_revision: owner.metadata.behavior_revision.get(),
            retained_count: owner.metadata.retained_behaviors.get(),
            pin: self.index.pin(&owner.pin)?,
            enrollment_pin: owner
                .enrollment_code
                .borrow()
                .as_ref()
                .map(|p| self.index.pin(p))
                .transpose()?,
            compatible_pin: owner
                .compatible
                .borrow()
                .as_ref()
                .map(|c| self.index.pin(&c._pin))
                .transpose()?,
            permit: match &*owner.backend.borrow() {
                Backend::Wasm { binding, .. } => Some(save_permit(binding, self.index)?),
                _ => None,
            },
            #[cfg(feature = "recorded-inputs")]
            recorded: owner
                .recorded
                .borrow()
                .iter()
                .map(|(n, ds)| {
                    Ok((
                        n.clone(),
                        ds.iter()
                            .map(|d| {
                                Ok(DeclarationRow {
                                    policy: d.policy.clone(),
                                    version: d.version,
                                    provider: save_binding(&d.provider, self.index)?,
                                    request: d.request.clone(),
                                })
                            })
                            .collect::<Result<_>>()?,
                    ))
                })
                .collect::<Result<_>>()?,
        };
        let mut seen = BTreeSet::new();
        let mut ids = vec![];
        let mut unique = BTreeSet::new();
        for b in behaviors {
            let ptr = Rc::as_ptr(&b.inner) as usize;
            let scope_ptr = b.scope.as_ref().map(|s| Rc::as_ptr(s) as usize);
            if !seen.insert((ptr, scope_ptr)) {
                return Err(Error::Invalid);
            }
            let inner = &b.inner;
            if !inner.owner.upgrade().is_some_and(|o| Rc::ptr_eq(&o, owner))
                || !Rc::ptr_eq(&inner.host.0, &owner.host.0)
                || inner.generation != owner.metadata.stamp.generation
                || inner.schema != owner.metadata.stamp.schema
            {
                return Err(Error::Stale);
            }
            unique.insert(ptr);
            let id = if let Some(id) = self.allocations.get(&ptr) {
                *id
            } else {
                let id = u64::try_from(self.behaviors.len())
                    .map_err(|_| Error::Limit)?
                    .checked_add(1)
                    .ok_or(Error::Limit)?;
                let backend = Backend::Wasm {
                    codec: inner.codec.clone(),
                    slot: inner.slot,
                    binding: inner.capture.binding().clone(),
                };
                let build = catalog.identify(&backend, &inner.policy, inner.schema)?;
                self.behaviors.push(BehaviorRow {
                    id,
                    owner: owner.id(),
                    generation: inner.generation,
                    schema: inner.schema,
                    caller: self.index.module(inner.caller)?,
                    artifact: inner.artifact,
                    source: self.index.source(&inner.source)?,
                    build,
                    slot: inner.slot,
                    capture: self.index.pure(&inner.capture)?,
                    revision: inner.revision,
                    policy: inner.policy.clone(),
                    target: inner.target.clone(),
                    pin: self.index.pin(&inner._invocation)?,
                });
                self.allocations.insert(ptr, id);
                id
            };
            let scope = if let Some(s) = &b.scope {
                let ptr = scope_ptr.unwrap();
                if !Rc::ptr_eq(&s.host.0, &owner.host.0) || !b.matches_callback(&s.origin) {
                    return Err(Error::Authority);
                }
                let sid = if let Some(id) = self.scope_allocations.get(&ptr) {
                    *id
                } else {
                    let sid = u64::try_from(self.scopes.len())
                        .map_err(|_| Error::Limit)?
                        .checked_add(1)
                        .ok_or(Error::Limit)?;
                    self.scopes.push(ScopeRow {
                        id: sid,
                        behavior: id,
                        callback: self.index.callback(&s.callback)?,
                    });
                    self.scope_allocations.insert(ptr, sid);
                    sid
                };
                Some(sid)
            } else {
                None
            };
            ids.push(BehaviorId {
                behavior: id,
                scope,
            });
        }
        if unique.len() != owner.metadata.retained_behaviors.get() {
            return Err(Error::Unsupported);
        }
        self.state.owner_mode(owner, codec, true)?;
        self.current.push(current);
        Ok(ids)
    }
    pub fn finish(mut self) -> Result<ExecutionCheckpoint> {
        let count = self
            .behaviors
            .len()
            .checked_add(self.scopes.len())
            .ok_or(Error::Limit)?;
        bound(count, self.state.host.0.limits.candidates)?;
        if count != self.state.host.0.behaviors.get() {
            return Err(Error::Unsupported);
        }
        self.pending.validate_capture(&self.state.host)?;
        self.current.sort_by_key(|r| r.owner);
        Ok(ExecutionCheckpoint {
            version: 1,
            kernel_namespace: self.index.namespace(),
            state: self.state.finish()?,
            cleanup: self.cleanup,
            current: self.current,
            behaviors: self.behaviors,
            scopes: self.scopes,
            pending: self.pending.finish()?,
        })
    }
}
/// Hidden, sealed, one-use namespace allocation BEFORE kernel resource admission.
/// No active host, chosen namespace, cleanup constructor or owner handle escapes.
pub struct OwnerIdentityReservation {
    id: u64,
    limits: Limits,
    publication: PublicationLimits,
    checkpoint: Option<ExecutionCheckpoint>,
    remap: Remap,
}
impl OwnerIdentityReservation {
    pub fn new(
        checkpoint: ExecutionCheckpoint,
        kernel: &KernelSnapshot,
        limits: Limits,
        publication: PublicationLimits,
    ) -> Result<Self> {
        bound(checkpoint.state.origins.len(), limits.owners)?;
        let excluded = checkpoint.state.origins.iter().copied().collect();
        Host::validate_limits(&limits)?;
        validate_checkpoint_bounds(&checkpoint.state, &limits, &publication)?;
        validate_execution(&checkpoint, &limits)?;
        checkpoint
            .pending
            .validate(&checkpoint, &limits, &publication)?;
        validate_execution_kernel(&checkpoint, kernel)?;
        checkpoint.pending.kernel(&checkpoint, kernel)?;
        let id = Host::reserve_namespace(&excluded)?;
        let remap = remap_for(&checkpoint.state, id);
        Ok(Self {
            id,
            limits,
            publication,
            checkpoint: Some(checkpoint),
            remap,
        })
    }
    pub fn resource(&self, old: &Resource) -> Result<Resource> {
        let c = &self.checkpoint.as_ref().ok_or(Error::Stale)?.state;
        if old.owner == format!("owners-{}", c.source_host) && old.key == "publication" {
            return Ok(Resource {
                owner: format!("owners-{}", self.id),
                key: old.key.clone(),
            });
        }
        self.remap.resource(old, c)
    }
    pub fn bind(mut self, kernel: &Kernel, mapped: &RestoredKernel) -> Result<ExecutionRestore> {
        let c = self.checkpoint.as_ref().ok_or(Error::Stale)?;
        preflight_mappings(c, kernel, mapped, &self.remap)?;
        c.pending.preflight(c, kernel, mapped, &self.remap)?;
        let cleanup = *mapped.cleanup().get(&c.cleanup).ok_or(Error::Authority)?;
        let mut bindings = BTreeMap::new();
        for current in &c.current {
            let module = *mapped.modules().get(&current.module).ok_or(Error::Schema)?;
            let row = c
                .state
                .owners
                .iter()
                .find(|r| r.old == current.owner)
                .ok_or(Error::Invalid)?;
            if bindings
                .insert(row.module_artifact, module)
                .is_some_and(|old| old != module)
            {
                return Err(Error::Schema);
            }
        }
        // Tombstones also need approved module bindings, without live factories.
        for row in &c.state.owners {
            if let std::collections::btree_map::Entry::Vacant(entry) =
                bindings.entry(row.module_artifact)
            {
                let modules = mapped
                    .modules()
                    .values()
                    .filter(|m| kernel.artifact(**m).ok() == Some(row.module_artifact))
                    .copied()
                    .collect::<Vec<_>>();
                if modules.len() != 1 {
                    return Err(Error::Schema);
                }
                entry.insert(modules[0]);
            }
        }
        let checkpoint = self.checkpoint.take().ok_or(Error::Stale)?;
        let host = Host::with_identity(self.limits, self.id, cleanup)?;
        host.configure_publication(self.publication)?;
        let gate = host.close_visibility()?;
        let state =
            Restore::from_reserved(kernel, checkpoint.state.clone(), host, gate, &bindings)?;
        Ok(ExecutionRestore {
            state,
            checkpoint,
            behaviors: BTreeMap::new(),
            scopes: BTreeMap::new(),
            pending: PendingRestore::new(mapped),
        })
    }
}
pub struct ExecutionRemap<'a> {
    kernel: &'a Kernel,
    mapped: &'a RestoredKernel,
}
impl ExecutionRemap<'_> {
    pub fn module(&self, old: u64) -> Result<ModuleHandle> {
        self.mapped
            .modules()
            .get(&old)
            .copied()
            .ok_or(Error::Schema)
    }
    pub fn binding(&self, old: &ProviderBinding, old_module: u64) -> Result<ProviderBinding> {
        let handle = self.module(old_module)?;
        if self.kernel.artifact(handle)? != old.artifact {
            return Err(Error::Schema);
        }
        Ok(ProviderBinding {
            handle,
            artifact: old.artifact,
            endpoint: old.endpoint.clone(),
        })
    }
    fn row_binding(&self, b: &BindingRow) -> Result<ProviderBinding> {
        let handle = self.module(b.module)?;
        if self.kernel.artifact(handle)? != b.artifact
            || (!b.endpoint.is_empty()
                && !self
                    .kernel
                    .manifest(handle)?
                    .exports
                    .contains_key(&b.endpoint))
        {
            return Err(Error::Schema);
        }
        Ok(ProviderBinding {
            handle,
            artifact: b.artifact,
            endpoint: b.endpoint.clone(),
        })
    }
    fn permit(&self, p: &PermitRow) -> Result<CapabilityPermit> {
        Ok(CapabilityPermit {
            capability: p.capability.clone(),
            resolution: p
                .resolution
                .iter()
                .map(|b| self.row_binding(b))
                .collect::<Result<_>>()?,
        })
    }
}
trait ErasedBehavior {
    fn as_any(&self) -> &dyn std::any::Any;
}
impl<T: 'static> ErasedBehavior for Rc<Behavior<T>> {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
/// Owner handles and behavior adapters remain quarantined until finish succeeds.
pub struct ExecutionRestore {
    state: Restore,
    checkpoint: ExecutionCheckpoint,
    behaviors: BTreeMap<u64, Box<dyn ErasedBehavior>>,
    scopes: BTreeMap<u64, Rc<CallbackScope>>,
    pending: PendingRestore,
}
impl ExecutionRestore {
    pub fn remap(&self) -> &Remap {
        self.state.remap()
    }
    pub fn owner<T: 'static>(
        &mut self,
        kernel: &Kernel,
        mapped: &RestoredKernel,
        old: OwnerId,
        codec: &dyn PersistenceCodec<T>,
        catalog: &RestoreCatalog<T>,
    ) -> Result<Rc<Owner<T>>> {
        self.pending_approved()?;
        self.bound_mappings(kernel, mapped)?;
        let row = self.state.row(old)?.clone();
        self.state.check_codec(&row, codec)?;
        let current = self
            .checkpoint
            .current
            .iter()
            .find(|c| c.owner == old)
            .ok_or(Error::Invalid)?
            .clone();
        let retained = self
            .checkpoint
            .behaviors
            .iter()
            .filter(|b| b.owner == old)
            .cloned()
            .collect::<Vec<_>>();
        // Resolve ALL exact old/current factories before any domain call.
        let factory = catalog.resolve(&current.build, &row.policy)?;
        for b in &retained {
            catalog.resolve(&b.build, &b.policy)?;
        }
        for candidate in self
            .checkpoint
            .pending
            .candidates
            .iter()
            .filter(|c| c.stamp.owner == old)
        {
            catalog.resolve(&candidate.build, &candidate.policy)?;
        }
        let remap = ExecutionRemap { kernel, mapped };
        let module = remap.module(current.module)?;
        let pin = mapped.execution_pin(current.pin, &[module])?;
        let enrollment_pin = current
            .enrollment_pin
            .map(|id| {
                let modules = current
                    .permit
                    .as_ref()
                    .ok_or(Error::Schema)?
                    .resolution
                    .iter()
                    .map(|b| remap.module(b.module))
                    .collect::<Result<Vec<_>>>()?;
                mapped.execution_pin(id, &modules).map_err(Error::from)
            })
            .transpose()?;
        let state = codec.decode_state(
            &row.payload,
            self.state.host.0.limits.payload_bytes,
            &self.state.remap.identity(),
        )?;
        self.state.validate_decoded(&row, codec, &state)?;
        let state = codec.decode_state(
            &row.payload,
            self.state.host.0.limits.payload_bytes,
            &self.state.remap,
        )?;
        codec.validate_state(&state)?;
        let expected = row
            .references
            .iter()
            .map(|r| self.state.remap.reference(r))
            .collect::<Result<Vec<_>>>()?;
        if codec.references(&state)? != expected {
            return Err(Error::Stale);
        }
        let mut ports = row.ports.clone();
        for p in ports.values_mut() {
            for d in &mut p.dependencies {
                d.owner = self.state.remap.owner(d.owner)?;
            }
        }
        codec.validate_configuration(&state, &ports, &row.policy)?;
        let backend = factory(&remap)?;
        validate_backend(&backend, &row.backend, &current.permit, &remap)?;
        let mut restored_codecs = vec![];
        for b in &retained {
            let backend = catalog.resolve(&b.build, &b.policy)?(&remap)?;
            let Backend::Wasm {
                codec,
                slot,
                binding,
            } = backend
            else {
                return Err(Error::Schema);
            };
            let capture = mapped
                .pure_captures()
                .get(b.capture)
                .ok_or(Error::Authority)?;
            if slot != b.slot || capture.binding() != &binding {
                return Err(Error::Schema);
            }
            codec.validate(&state)?;
            bound(
                codec.encode(&state)?.len(),
                self.state.host.0.limits.payload_bytes,
            )?;
            restored_codecs.push(codec);
        }
        let compatible_pin = current
            .compatible_pin
            .map(|id| mapped.pins().get(&id).cloned().ok_or(Error::Authority))
            .transpose()?;
        let compatible = match (&backend, compatible_pin) {
            (
                Backend::Wasm {
                    codec,
                    slot,
                    binding,
                },
                Some(pin),
            ) => Some(Compatible {
                codec: codec.clone(),
                slot: *slot,
                binding: binding.clone(),
                _pin: pin,
            }),
            (_, None) => None,
            _ => return Err(Error::Schema),
        };
        let owner = self.state.host.enroll_sealed(
            kernel,
            OwnerSpec {
                module,
                schema: row.schema,
                ports,
            },
            state,
            backend,
            Some((self.state.remap.owner(old)?, row.generation, row.revision)),
            Some((pin, enrollment_pin)),
        )?;
        owner
            .metadata
            .behavior_revision
            .set(current.behavior_revision);
        *owner.query_policy.borrow_mut() = row.policy;
        *owner.compatible.borrow_mut() = compatible;
        #[cfg(feature = "recorded-inputs")]
        for (name, ds) in &current.recorded {
            owner.recorded.borrow_mut().insert(
                name.clone(),
                ds.iter()
                    .map(|d| {
                        Ok(RecordedDeclaration {
                            policy: d.policy.clone(),
                            version: d.version,
                            provider: remap.row_binding(&d.provider)?,
                            request: d.request.clone(),
                        })
                    })
                    .collect::<Result<_>>()?,
            );
        }
        for (b, codec) in retained.iter().zip(restored_codecs) {
            let inner = Rc::new(Behavior {
                owner: Rc::downgrade(&owner),
                host: self.state.host.clone(),
                generation: b.generation,
                schema: b.schema,
                caller: remap.module(b.caller)?,
                artifact: b.artifact,
                source: mapped.execution_source(&b.source)?,
                codec,
                slot: b.slot,
                capture: mapped.pure_captures()[b.capture].clone(),
                revision: b.revision,
                policy: b.policy.clone(),
                target: self
                    .state
                    .remap
                    .resource(&b.target, &self.checkpoint.state)?,
                _invocation: mapped.pins().get(&b.pin).cloned().ok_or(Error::Authority)?,
            });
            self.state
                .host
                .0
                .behaviors
                .set(self.state.host.0.behaviors.get() + 1);
            owner
                .metadata
                .retained_behaviors
                .set(owner.metadata.retained_behaviors.get() + 1);
            self.behaviors.insert(b.id, Box::new(inner));
        }
        for s in self
            .checkpoint
            .scopes
            .iter()
            .filter(|s| retained.iter().any(|b| b.id == s.behavior))
        {
            let callback = mapped
                .callbacks()
                .get(&s.callback)
                .ok_or(Error::Authority)?;
            let scope = Rc::new(CallbackScope {
                host: self.state.host.clone(),
                origin: callback.provenance().clone(),
                callback: callback.clone(),
            });
            self.state
                .host
                .0
                .behaviors
                .set(self.state.host.0.behaviors.get() + 1);
            self.scopes.insert(s.id, scope);
        }
        self.state.restored.insert(old);
        Ok(owner)
    }
    pub fn behavior<T: 'static>(&self, id: BehaviorId) -> Result<RetainedBehavior<T>> {
        let inner = self
            .behaviors
            .get(&id.behavior)
            .ok_or(Error::Stale)?
            .as_any()
            .downcast_ref::<Rc<Behavior<T>>>()
            .ok_or(Error::Schema)?
            .clone();
        let scope = if let Some(sid) = id.scope {
            if !self
                .checkpoint
                .scopes
                .iter()
                .any(|s| s.id == sid && s.behavior == id.behavior)
            {
                return Err(Error::Authority);
            }
            Some(self.scopes.get(&sid).ok_or(Error::Stale)?.clone())
        } else {
            None
        };
        Ok(RetainedBehavior { inner, scope })
    }
    pub fn finish(self) -> Result<Host> {
        self.pending.validate_finish(&self.checkpoint.pending)?;
        self.state.finish()
    }
}
fn validate_backend<T>(
    backend: &Backend<T>,
    identity: &Option<BackendIdentity>,
    permit: &Option<PermitRow>,
    remap: &ExecutionRemap<'_>,
) -> Result<()> {
    if Some(backend_identity(backend)?) != *identity {
        return Err(Error::Schema);
    }
    match (backend, permit) {
        (Backend::Native(_), None) => Ok(()),
        (Backend::Wasm { binding, .. }, Some(p)) if binding == &remap.permit(p)? => Ok(()),
        _ => Err(Error::Schema),
    }
}
fn validate_execution(c: &ExecutionCheckpoint, limits: &Limits) -> Result<()> {
    if c.version != 1 || c.kernel_namespace == 0 || c.cleanup == 0 {
        return Err(Error::Invalid);
    }
    bound(c.current.len(), limits.owners)?;
    if c.current.windows(2).any(|w| w[0].owner >= w[1].owner) {
        return Err(Error::Invalid);
    }
    bound(
        c.behaviors
            .len()
            .checked_add(c.scopes.len())
            .ok_or(Error::Limit)?,
        limits.candidates,
    )?;
    let live = c
        .state
        .owners
        .iter()
        .filter(|r| r.live)
        .map(|r| r.old)
        .collect::<BTreeSet<_>>();
    let mut current = BTreeSet::new();
    let mut edges = 0usize;
    for row in &c.state.owners {
        edges = edges
            .checked_add(row.references.len())
            .ok_or(Error::Limit)?;
        for p in row.ports.values() {
            edges = edges
                .checked_add(p.dependencies.len())
                .ok_or(Error::Limit)?;
        }
    }
    for row in &c.current {
        if !live.contains(&row.owner)
            || !current.insert(row.owner)
            || row.module == 0
            || row.pin == 0
        {
            return Err(Error::Invalid);
        }
        row.build.validate()?;
        let owner = c
            .state
            .owners
            .iter()
            .find(|o| o.old == row.owner)
            .ok_or(Error::Stale)?;
        if row.build.schema_digest != owner.schema
            || row.compatible_pin.is_some()
                && (row.permit.is_none() || row.enrollment_pin.is_some())
        {
            return Err(Error::Schema);
        }
        if let Some(p) = &row.permit {
            validate_permit(p, limits, &mut edges)?;
        }
        #[cfg(feature = "recorded-inputs")]
        {
            bound(row.recorded.len(), limits.ports)?;
            for (name, ds) in &row.recorded {
                let port = owner.ports.get(name).ok_or(Error::Schema)?;
                bound(ds.len(), limits.observations)?;
                if !ds.is_empty()
                    && port.access == Access::Read
                    && owner.policy != QueryPolicy::Noncacheable
                {
                    return Err(Error::Schema);
                }
                let mut bytes = 0usize;
                for d in ds {
                    if d.policy.is_empty() || d.provider.endpoint != nethacked_inputs::ENDPOINT {
                        return Err(Error::Schema);
                    }
                    validate_binding(&d.provider)?;
                    bytes = bytes
                        .checked_add(d.request.len())
                        .and_then(|n| n.checked_add(d.policy.len()))
                        .ok_or(Error::Limit)?;
                    edges = edges.checked_add(1).ok_or(Error::Limit)?;
                }
                bound(bytes, limits.observation_bytes)?;
            }
        }
    }
    if current != live {
        return Err(Error::Unsupported);
    }
    let mut behaviors = BTreeSet::new();
    let mut previous = 0;
    for b in &c.behaviors {
        if b.id <= previous
            || !behaviors.insert(b.id)
            || !live.contains(&b.owner)
            || b.caller == 0
            || b.pin == 0
        {
            return Err(Error::Invalid);
        }
        previous = b.id;
        b.build.validate()?;
        bound(b.source.phase.len(), 256)?;
        bound(b.source.cleanup.len(), 65_536)?;
        if b.source.operation == 0
            || b.source.pipeline == 0
            || b.source.cleanup.contains(&0)
            || b.source.cleanup.windows(2).any(|w| w[0] >= w[1])
        {
            return Err(Error::Invalid);
        }
        edges = edges
            .checked_add(b.source.cleanup.len())
            .ok_or(Error::Limit)?;
        let row = c
            .state
            .owners
            .iter()
            .find(|r| r.old == b.owner)
            .ok_or(Error::Stale)?;
        let current = c
            .current
            .iter()
            .find(|r| r.owner == b.owner)
            .ok_or(Error::Stale)?;
        let remap = remap_for(&c.state, c.state.source_host);
        if b.generation != row.generation
            || b.schema != row.schema
            || b.build.schema_digest != row.schema
            || b.revision > current.behavior_revision
            || remap.resource(&b.target, &c.state)? != b.target
            || !b.target.key.starts_with(&format!("{}.", b.owner.serial))
            || !b.source.cleanup.contains(&c.cleanup)
        {
            return Err(Error::Stale);
        }
        edges = edges.checked_add(8).ok_or(Error::Limit)?;
    }
    for row in &c.current {
        if row.retained_count != c.behaviors.iter().filter(|b| b.owner == row.owner).count() {
            return Err(Error::Invalid);
        }
    }
    let mut previous = 0;
    for s in &c.scopes {
        if s.id <= previous || !behaviors.contains(&s.behavior) || s.callback == 0 {
            return Err(Error::Invalid);
        }
        previous = s.id;
        edges = edges.checked_add(2).ok_or(Error::Limit)?;
    }
    bound(edges, 262_144)
}
fn validate_binding(b: &BindingRow) -> Result<()> {
    if b.module == 0 || b.endpoint.len() > 256 {
        return Err(Error::Schema);
    }
    Ok(())
}
fn validate_permit(p: &PermitRow, limits: &Limits, edges: &mut usize) -> Result<()> {
    if p.resolution.is_empty()
        || p.capability.provider.len() > 256
        || p.capability.endpoint.len() > 256
    {
        return Err(Error::Schema);
    }
    bound(p.resolution.len(), limits.observations)?;
    for b in &p.resolution {
        validate_binding(b)?;
        *edges = edges.checked_add(1).ok_or(Error::Limit)?;
    }
    Ok(())
}
fn preflight_mappings(
    c: &ExecutionCheckpoint,
    kernel: &Kernel,
    mapped: &RestoredKernel,
    owner_remap: &Remap,
) -> Result<()> {
    if c.kernel_namespace != mapped.execution_namespace() {
        return Err(Error::Authority);
    }
    let remap = ExecutionRemap { kernel, mapped };
    let cleanup = *mapped.cleanup().get(&c.cleanup).ok_or(Error::Authority)?;
    for row in &c.current {
        let module = remap.module(row.module)?;
        let owner = c
            .state
            .owners
            .iter()
            .find(|o| o.old == row.owner)
            .ok_or(Error::Stale)?;
        if kernel.artifact(module)? != owner.module_artifact
            || kernel.manifest(module)?.id != owner.provider
        {
            return Err(Error::Schema);
        }
        mapped.execution_pin(row.pin, &[module])?;
        if let Some(p) = &row.permit {
            remap.permit(p)?;
            let modules = p
                .resolution
                .iter()
                .map(|b| remap.module(b.module))
                .collect::<Result<Vec<_>>>()?;
            if row.enrollment_pin.is_none() && row.compatible_pin.is_none() {
                return Err(Error::Authority);
            }
            for id in [row.enrollment_pin, row.compatible_pin]
                .into_iter()
                .flatten()
            {
                mapped.execution_pin(id, &modules)?;
            }
        } else if row.enrollment_pin.is_some() || row.compatible_pin.is_some() {
            return Err(Error::Schema);
        }
        #[cfg(feature = "recorded-inputs")]
        for ds in row.recorded.values() {
            for d in ds {
                remap.row_binding(&d.provider)?;
            }
        }
    }
    for b in &c.behaviors {
        let caller = remap.module(b.caller)?;
        if kernel.artifact(caller)? != b.artifact {
            return Err(Error::Schema);
        }
        let source = mapped.execution_source(&b.source)?;
        if !source.admits_cleanup(cleanup) {
            return Err(Error::Authority);
        }
        let capture = mapped
            .pure_captures()
            .get(b.capture)
            .ok_or(Error::Authority)?;
        let provider = c
            .current
            .iter()
            .find(|r| r.owner == b.owner)
            .ok_or(Error::Stale)?
            .module;
        mapped.execution_behavior(
            b.capture, &b.source, provider, b.caller, b.slot, &b.target, b.pin,
        )?;
        if mapped.execution_resource(&b.target)? != &owner_remap.resource(&b.target, &c.state)? {
            return Err(Error::Authority);
        }
        let _ = capture;
        if !mapped.pins().contains_key(&b.pin) {
            return Err(Error::Authority);
        }
        owner_remap.resource(&b.target, &c.state)?;
    }
    for s in &c.scopes {
        let b = c
            .behaviors
            .iter()
            .find(|b| b.id == s.behavior)
            .ok_or(Error::Stale)?;
        let p = mapped
            .callbacks()
            .get(&s.callback)
            .ok_or(Error::Authority)?
            .provenance();
        if p.source != mapped.execution_source(&b.source)?
            || p.caller.handle != remap.module(b.caller)?
            || p.caller.artifact != b.artifact
        {
            return Err(Error::Authority);
        }
    }
    Ok(())
}

fn validate_execution_kernel(c: &ExecutionCheckpoint, k: &KernelSnapshot) -> Result<()> {
    if c.kernel_namespace != k.source_namespace || !k.cleanup_history.contains(&c.cleanup) {
        return Err(Error::Authority);
    }
    let modules = k
        .registry
        .modules
        .iter()
        .map(|m| (m.id, m))
        .collect::<BTreeMap<_, _>>();
    let binding = |b: &BindingRow| -> Result<()> {
        let m = modules.get(&b.module).ok_or(Error::Schema)?;
        if m.artifact != b.artifact || !m.manifest.exports.contains_key(&b.endpoint) {
            return Err(Error::Schema);
        }
        Ok(())
    };
    let pin = |id: u64, expected: BTreeSet<u64>| -> Result<()> {
        let row = k.pins.iter().find(|p| p.id == id).ok_or(Error::Authority)?;
        if row.modules.iter().copied().collect::<BTreeSet<_>>() != expected {
            return Err(Error::Authority);
        }
        Ok(())
    };
    for row in &c.current {
        let m = modules.get(&row.module).ok_or(Error::Schema)?;
        let owner = c
            .state
            .owners
            .iter()
            .find(|o| o.old == row.owner)
            .ok_or(Error::Stale)?;
        if m.artifact != owner.module_artifact || m.manifest.id != owner.provider {
            return Err(Error::Schema);
        }
        pin(row.pin, [row.module].into())?;
        match (&row.permit, &owner.backend) {
            (Some(p), Some(backend)) if backend.wasm => {
                if m.manifest
                    .dependencies
                    .get(backend.slot)
                    .map(|d| &d.capability)
                    != Some(&p.capability)
                    || p.resolution
                        .iter()
                        .map(|b| (b.artifact, b.endpoint.clone()))
                        .collect::<Vec<_>>()
                        != backend.helpers
                {
                    return Err(Error::Schema);
                }
                for b in &p.resolution {
                    binding(b)?;
                }
                let expected = p
                    .resolution
                    .iter()
                    .map(|b| b.module)
                    .collect::<BTreeSet<_>>();
                if row.enrollment_pin.is_none() && row.compatible_pin.is_none() {
                    return Err(Error::Authority);
                }
                for id in [row.enrollment_pin, row.compatible_pin]
                    .into_iter()
                    .flatten()
                {
                    pin(id, expected.clone())?;
                }
            }
            (None, Some(backend))
                if !backend.wasm
                    && row.enrollment_pin.is_none()
                    && row.compatible_pin.is_none() => {}
            _ => return Err(Error::Schema),
        }
        #[cfg(feature = "recorded-inputs")]
        for ds in row.recorded.values() {
            for d in ds {
                binding(&d.provider)?;
            }
        }
    }
    for b in &c.behaviors {
        let current = c
            .current
            .iter()
            .find(|r| r.owner == b.owner)
            .ok_or(Error::Stale)?;
        if modules.get(&b.caller).ok_or(Error::Schema)?.artifact != b.artifact {
            return Err(Error::Schema);
        }
        k.execution_behavior(
            b.capture,
            &b.source,
            current.module,
            b.caller,
            b.slot,
            &b.target,
            b.pin,
        )?;
    }
    for s in &c.scopes {
        let b = c
            .behaviors
            .iter()
            .find(|b| b.id == s.behavior)
            .ok_or(Error::Stale)?;
        let callback = k
            .callbacks
            .iter()
            .find(|r| r.id == s.callback)
            .ok_or(Error::Authority)?;
        let origin = callback.pipeline.origin.as_ref().ok_or(Error::Authority)?;
        if origin.source != b.source
            || origin.caller.module != b.caller
            || origin.caller.artifact != b.artifact
        {
            return Err(Error::Authority);
        }
    }
    Ok(())
}

impl ExecutionCheckpoint {
    /// Exact adapter edge check against the owner-local behavior/scope table.
    pub fn adapter_behavior(
        &self,
        id: BehaviorId,
        target: &Resource,
        callback: Option<u64>,
    ) -> Result<()> {
        let row = self
            .behaviors
            .iter()
            .find(|r| r.id == id.behavior)
            .ok_or(Error::Stale)?;
        if &row.target != target {
            return Err(Error::Authority);
        }
        match (id.scope, callback) {
            (None, None) => Ok(()),
            (Some(scope), Some(callback))
                if self.scopes.iter().any(|s| {
                    s.id == scope && s.behavior == id.behavior && s.callback == callback
                }) =>
            {
                Ok(())
            }
            _ => Err(Error::Authority),
        }
    }
}
impl ExecutionRestore {
    pub fn adapter_destination(
        &self,
        kernel: &Kernel,
        mapped: &RestoredKernel,
    ) -> Result<nethacked_kernel::persistence::ExecutionDestination> {
        self.bound_mappings(kernel, mapped)?;
        Ok(mapped.execution_destination())
    }
    pub fn adapter_behavior<T: 'static>(
        &self,
        id: BehaviorId,
        kernel: &Kernel,
        mapped: &RestoredKernel,
    ) -> Result<RetainedBehavior<T>> {
        self.bound_mappings(kernel, mapped)?;
        self.behavior(id)
    }
}

/// Sealed weak target enrollment from an actual hidden D3 owner allocation.
/// It carries metadata only; current() uses the ordinary owner visibility gate.
pub struct AdapterTarget<T> {
    owner: std::rc::Weak<Owner<T>>,
    address: Resource,
    stamp: Stamp,
}
impl<T: 'static> AdapterTarget<T> {
    pub fn address(&self) -> &Resource {
        &self.address
    }
    pub fn stamp(&self) -> &Stamp {
        &self.stamp
    }
    pub fn current(&self) -> Result<Stamp> {
        let owner = self.owner.upgrade().ok_or(Error::Stale)?;
        let now = owner.stamp()?;
        if now.owner != self.stamp.owner
            || now.generation != self.stamp.generation
            || now.schema != self.stamp.schema
            || now.module != self.stamp.module
        {
            return Err(Error::Stale);
        }
        Ok(now)
    }
}
impl ExecutionRestore {
    pub fn adapter_target<T: 'static>(
        &self,
        owner: &Rc<Owner<T>>,
        port: &str,
        kernel: &Kernel,
        mapped: &RestoredKernel,
    ) -> Result<AdapterTarget<T>> {
        self.bound_mappings(kernel, mapped)?;
        if !Rc::ptr_eq(&owner.host.0, &self.state.host.0)
            || !self.state.restored.iter().any(|old| {
                self.state
                    .remap
                    .owner(*old)
                    .is_ok_and(|new| new == owner.id())
            })
        {
            return Err(Error::Authority);
        }
        self.state.host.route(owner.id())?;
        Ok(AdapterTarget {
            owner: Rc::downgrade(owner),
            address: owner.resource(port)?,
            stamp: owner.metadata.current_stamp(),
        })
    }
}

impl ExecutionCapture<'_> {
    pub fn adapter_target<T: 'static>(
        &self,
        owner: &Rc<Owner<T>>,
        port: &str,
    ) -> Result<AdapterTarget<T>> {
        if !Rc::ptr_eq(&owner.host.0, &self.state.host.0) {
            return Err(Error::Authority);
        }
        self.state.host.route(owner.id())?;
        Ok(AdapterTarget {
            owner: Rc::downgrade(owner),
            address: owner.resource(port)?,
            stamp: owner.metadata.current_stamp(),
        })
    }
}

/// Allocation alias test for typed adapter capture; neither value is a wire ID.
pub fn adapter_behavior_alias<T>(a: &RetainedBehavior<T>, b: &RetainedBehavior<T>) -> bool {
    Rc::ptr_eq(&a.inner, &b.inner)
        && match (&a.scope, &b.scope) {
            (None, None) => true,
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
}
