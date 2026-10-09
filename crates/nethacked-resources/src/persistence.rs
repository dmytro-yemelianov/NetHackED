//! Optional D4d local persistence. This is one private resource service, not a
//! HostBundle exchange or a gameplay owner. Codec purity, faithful bounded
//! encoding, fresh private closure storage and effect-free construction/drop
//! are explicit native premises. No exactly-once external I/O is promised.
use super::*;
use nethacked_kernel::persistence::{
    ExecutionCaptureIndex, ExecutionDestination, KernelSnapshot, ResourceContext, ResourcePermit,
    RestoredKernel, SavedBinding,
};
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodecIdentity {
    pub name: String,
    pub version: u32,
    pub artifact: [u8; 32],
}
const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_ROWS: usize = 65_536;
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PersistenceError {
    CaptureBusy,
    Refused(Error),
}
pub type PersistenceResult<T> = std::result::Result<T, PersistenceError>;
impl From<Error> for PersistenceError {
    fn from(value: Error) -> Self {
        Self::Refused(value)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Role {
    Owner,
    Accounting,
}
/// Host-approved codec, distinct from invoking the predicate. Encoders read
/// immutable capture data; validators must never be invoked during capture or
/// reconstruction. Decoders create fresh closures using only these bytes/maps.
pub trait PolicyCodec {
    fn identity(&self) -> CodecIdentity;
    fn role(&self) -> Role;
    fn encode(&self, predicate: &Validator, limit: usize) -> Result<Vec<u8>>;
    fn validate(&self, bytes: &[u8], limit: usize) -> Result<()>;
    fn decode(&self, bytes: &[u8], mapped: &RestoredKernel, limit: usize) -> Result<Validator>;
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyRow {
    pub codec: CodecIdentity,
    pub role: Role,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerRow {
    pub owner: String,
    pub capability: Capability,
    pub provider: SavedBinding,
    pub policy: PolicyRow,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingRow {
    pub serial: u64,
    pub plan: Plan,
    pub context: ResourceContext,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    pub version: u32,
    pub namespace: u64,
    pub namespace_high_water: u64,
    pub kernel_namespace: u64,
    pub cleanup: u64,
    pub limits: Limits,
    pub binding: Option<SavedBinding>,
    pub owners: Vec<OwnerRow>,
    pub accounting: PolicyRow,
    pub cells: Vec<Snapshot>,
    pub history: Vec<Vec<u8>>,
    pub pending: Vec<PendingRow>,
    pub next_serial: u64,
    pub next_generation: u64,
    pub commits: u64,
}
impl Checkpoint {
    pub fn to_bytes(&self, limit: usize) -> Result<Vec<u8>> {
        // Bound the serialized size before canonical Value allocation.
        struct Counter(usize, usize);
        impl std::io::Write for Counter {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0 = self
                    .0
                    .checked_add(b.len())
                    .filter(|n| *n <= self.1)
                    .ok_or_else(|| std::io::Error::other("resource checkpoint limit"))?;
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        serde_json::to_writer(&mut Counter(0, limit.min(MAX_BYTES)), self)
            .map_err(|_| Error::Limit)?;
        let bytes = serde_json::to_vec(&serde_json::to_value(self).map_err(|_| Error::Buffer)?)
            .map_err(|_| Error::Buffer)?;
        bounded_json(&bytes)?;
        Ok(bytes)
    }
    pub fn from_bytes(bytes: &[u8], limit: usize) -> Result<Self> {
        if bytes.len() > limit.min(MAX_BYTES) {
            return Err(Error::Limit);
        }
        bounded_json(bytes)?;
        let c: Self = serde_json::from_slice(bytes).map_err(|_| Error::Buffer)?;
        if c.to_bytes(limit)? != bytes {
            return Err(Error::Buffer);
        }
        Ok(c)
    }
}
struct Enrollment {
    predicate: Validator,
    codec: Rc<dyn PolicyCodec>,
}
pub struct CaptureCatalog {
    service: Service,
    owners: BTreeMap<String, Enrollment>,
    accounting: Option<Enrollment>,
}
impl CaptureCatalog {
    pub fn new(service: &Service) -> Self {
        Self {
            service: service.clone(),
            owners: BTreeMap::new(),
            accounting: None,
        }
    }
    pub fn owner(&mut self, owner: &str, codec: Rc<dyn PolicyCodec>) -> Result<()> {
        check_identity(&codec.identity())?;
        if codec.role() != Role::Owner || self.owners.contains_key(owner) {
            return Err(Error::Duplicate);
        }
        let inner = self
            .service
            .0
            .try_borrow()
            .map_err(|_| Error::NativeRefusal)?;
        let predicate = inner
            .owners
            .get(owner)
            .ok_or(Error::Authority)?
            .validate
            .clone();
        self.owners
            .insert(owner.into(), Enrollment { predicate, codec });
        Ok(())
    }
    pub fn accounting(&mut self, codec: Rc<dyn PolicyCodec>) -> Result<()> {
        check_identity(&codec.identity())?;
        if codec.role() != Role::Accounting || self.accounting.is_some() {
            return Err(Error::Duplicate);
        }
        let predicate = self
            .service
            .0
            .try_borrow()
            .map_err(|_| Error::NativeRefusal)?
            .accounting
            .clone();
        self.accounting = Some(Enrollment { predicate, codec });
        Ok(())
    }
}
#[derive(Default)]
pub struct RestoreCatalog {
    codecs: BTreeMap<CodecIdentity, Rc<dyn PolicyCodec>>,
}
impl RestoreCatalog {
    pub fn codec(&mut self, codec: Rc<dyn PolicyCodec>) -> Result<()> {
        let id = codec.identity();
        check_identity(&id)?;
        if self.codecs.len() >= MAX_ROWS || self.codecs.contains_key(&id) {
            return Err(Error::Duplicate);
        }
        self.codecs.insert(id, codec);
        Ok(())
    }
    fn get(&self, row: &PolicyRow) -> Result<&dyn PolicyCodec> {
        let codec = self.codecs.get(&row.codec).ok_or(Error::Authority)?;
        if codec.identity() != row.codec || codec.role() != row.role {
            return Err(Error::Authority);
        }
        Ok(codec.as_ref())
    }
}
/// Independent approval of immutable policy data and accepted provenance. The
/// incoming checkpoint is a request, not an authority catalog. Default deny.
pub trait ResourceAuthority {
    fn approve(&self, _checkpoint: &Checkpoint, _kernel: &KernelSnapshot) -> Result<()> {
        Err(Error::Authority)
    }
}
fn check_identity(id: &CodecIdentity) -> Result<()> {
    if id.name.is_empty() || id.name.len() > 256 || id.version == 0 {
        Err(Error::Limit)
    } else {
        Ok(())
    }
}
fn encode_policy(e: &Enrollment, role: Role, limit: usize) -> Result<PolicyRow> {
    let bytes = e.codec.encode(&e.predicate, limit)?;
    if bytes.len() > limit {
        return Err(Error::Limit);
    }
    Ok(PolicyRow {
        codec: e.codec.identity(),
        role,
        bytes,
    })
}
impl Service {
    /// Cleanup and address roots for the kernel's snapshot-only service roots.
    /// Includes stale/deleted pending addresses; no authority is manufactured.
    pub fn persistence_roots(&self) -> PersistenceResult<(CleanupIdentity, Vec<Resource>)> {
        let inner = self
            .0
            .try_borrow_mut()
            .map_err(|_| PersistenceError::CaptureBusy)?;
        if inner.quarantine {
            return Err(Error::Authority.into());
        }
        let resources = inner
            .state
            .resources
            .keys()
            .cloned()
            .chain(inner.state.pending.values().flat_map(|p| {
                p.plan
                    .readonly
                    .iter()
                    .chain(p.plan.writes.iter().map(|w| &w.stamp))
                    .map(|s| s.address.clone())
            }))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        Ok((inner.cleanup_identity, resources))
    }
    /// Exact live bound enrollments; legacy unbound and disposed provenance are
    /// retained as data. No operation lifecycle, predicates or publication.
    pub fn persistence_enrollment(
        &self,
        kernel: &Kernel,
    ) -> PersistenceResult<Vec<OperationBinding>> {
        let inner = self
            .0
            .try_borrow_mut()
            .map_err(|_| PersistenceError::CaptureBusy)?;
        if inner.quarantine {
            return Err(Error::Authority.into());
        }
        let mut bindings = Vec::new();
        for p in inner.state.pending.values() {
            if let Some(op) = &p.operation {
                if kernel.resource_source_live(op)? && !bindings.contains(op) {
                    bindings.push(op.clone());
                }
            }
        }
        Ok(bindings)
    }
    pub fn persistence_history(&self) -> PersistenceResult<Vec<OperationBinding>> {
        let inner = self
            .0
            .try_borrow_mut()
            .map_err(|_| PersistenceError::CaptureBusy)?;
        if inner.quarantine {
            return Err(Error::Authority.into());
        }
        Ok(inner
            .state
            .pending
            .values()
            .filter_map(|p| p.operation.clone())
            .collect())
    }
    pub fn capture_local(
        &self,
        index: &ExecutionCaptureIndex<'_>,
        kernel: &KernelSnapshot,
        catalog: &CaptureCatalog,
    ) -> PersistenceResult<Checkpoint> {
        let inner = self
            .0
            .try_borrow_mut()
            .map_err(|_| PersistenceError::CaptureBusy)?;
        if inner.quarantine || !Rc::ptr_eq(&self.0, &catalog.service.0) {
            return Err(Error::Authority.into());
        }
        if catalog.owners.len() != inner.owners.len() {
            return Err(Error::Authority.into());
        }
        let accounting = catalog
            .accounting
            .as_ref()
            .filter(|e| Rc::ptr_eq(&e.predicate, &inner.accounting))
            .ok_or(Error::Authority)?;
        let mut owners = Vec::new();
        for (owner, p) in &inner.owners {
            let e = catalog
                .owners
                .get(owner)
                .filter(|e| Rc::ptr_eq(&e.predicate, &p.validate))
                .ok_or(Error::Authority)?;
            owners.push(OwnerRow {
                owner: owner.clone(),
                capability: p.capability.clone(),
                provider: index.resource_binding(p.provider, p.artifact, &p.capability.endpoint)?,
                policy: encode_policy(e, Role::Owner, inner.limits.payload)?,
            });
        }
        let mut pending = Vec::new();
        for (serial, p) in &inner.state.pending {
            let stamps = p
                .plan
                .readonly
                .iter()
                .map(|s| (s, Access::Read))
                .chain(p.plan.writes.iter().map(|w| (&w.stamp, Access::Write)));
            let mut permits = Vec::new();
            for ((stamp, access), permit) in stamps.zip(&p.permits) {
                permits.push(ResourcePermit {
                    capability: permit.capability.clone(),
                    resolution: permit
                        .resolution
                        .iter()
                        .map(|b| index.resource_binding(b.handle, b.artifact, &b.endpoint))
                        .collect::<Result<_>>()?,
                    target: stamp.address.clone(),
                    access,
                });
            }
            let operation = p
                .operation
                .as_ref()
                .map(|op| index.resource_operation(op))
                .transpose()?;
            let missing = |id| !kernel.registry.modules.iter().any(|m| m.id == id);
            let disposed = operation
                .as_ref()
                .is_some_and(|op| !kernel.leases.iter().any(|l| l.id == op.pipeline));
            let caller = index.resource_binding(p.caller, p.artifact, "prepare")?;
            let provider = index.resource_binding(p.provider, p.provider_artifact, "prepare")?;
            let historical = disposed
                || missing(caller.module)
                || missing(provider.module)
                || permits
                    .iter()
                    .flat_map(|p| &p.resolution)
                    .any(|b| missing(b.module));
            pending.push(PendingRow {
                serial: *serial,
                plan: p.plan.clone(),
                context: ResourceContext {
                    operation,
                    caller,
                    provider,
                    permits,
                    historical,
                },
            });
        }
        let c = Checkpoint {
            version: 1,
            namespace: inner.namespace,
            namespace_high_water: NEXT_SERVICE.load(Ordering::Relaxed) - 1,
            kernel_namespace: index.namespace(),
            cleanup: index.cleanup(inner.cleanup_identity)?,
            limits: inner.limits.clone(),
            binding: inner
                .binding
                .map(|(h, a)| index.resource_binding(h, a, "prepare"))
                .transpose()?,
            owners,
            accounting: encode_policy(accounting, Role::Accounting, inner.limits.payload)?,
            cells: inner.state.resources.values().cloned().collect(),
            history: inner.state.history.clone(),
            pending,
            next_serial: inner.state.next_serial,
            next_generation: inner.state.next_generation,
            commits: inner.state.commits,
        };
        c.structural(&inner.limits, kernel)?;
        Ok(c)
    }
}
fn limits_valid(l: &Limits) -> bool {
    l.owners > 0
        && l.resources > 0
        && l.participants > 0
        && l.pending > 0
        && l.payload > 0
        && l.wire > 0
        && l.wire <= i32::MAX as u32
        && [
            l.owners,
            l.resources,
            l.participants,
            l.pending,
            l.events,
            l.history,
        ]
        .into_iter()
        .all(|n| n <= MAX_ROWS)
        && l.payload <= 16 * 1024 * 1024
}
impl Checkpoint {
    fn structural(&self, admitted: &Limits, kernel: &KernelSnapshot) -> Result<()> {
        self.to_bytes(MAX_BYTES)?;
        kernel.validate_execution_metadata()?;
        let l = &self.limits;
        if self.version != 1
            || self.namespace == 0
            || self.namespace > self.namespace_high_water
            || self.namespace_high_water == u64::MAX
            || self.kernel_namespace != kernel.source_namespace
            || self.cleanup == 0
            || !kernel.service_cleanup.contains(&self.cleanup)
            || !limits_valid(l)
            || !limits_valid(admitted)
            || l.owners > admitted.owners
            || l.resources > admitted.resources
            || l.participants > admitted.participants
            || l.pending > admitted.pending
            || l.events > admitted.events
            || l.history > admitted.history
            || l.payload > admitted.payload
            || l.wire > admitted.wire
            || self.owners.len() > l.owners
            || self.cells.len() > l.resources
            || self.pending.len() > l.pending
            || self.history.len() > l.history
            || self.history.iter().any(|e| e.len() > l.payload)
            || self.next_serial == 0
            || self.next_generation == 0
            || self.commits >= self.next_serial
            || self.accounting.role != Role::Accounting
        {
            return Err(Error::Limit);
        }
        let mut previous = "";
        let module = |b: &SavedBinding| -> Result<()> {
            if b.module == 0
                || b.module >= kernel.registry.next_generation
                || kernel
                    .registry
                    .modules
                    .iter()
                    .any(|m| m.id == b.module && m.artifact != b.artifact)
            {
                return Err(Error::Authority);
            }
            Ok(())
        };
        if let Some(b) = &self.binding {
            module(b)?;
            if b.endpoint != "prepare" {
                return Err(Error::Authority);
            }
            if let Some(m) = kernel.registry.modules.iter().find(|m| m.id == b.module) {
                if !matches!(
                    m.executable,
                    nethacked_kernel::persistence::Executable::NativeV1
                ) || m.manifest.exports.len() != 3
                    || !m.manifest.dependencies.is_empty()
                    || [
                        ("read", Access::Read),
                        ("prepare", Access::Write),
                        ("commit", Access::Write),
                    ]
                    .iter()
                    .any(|(ep, a)| {
                        !m.manifest.exports.get(*ep).is_some_and(|e| {
                            e.major == 1
                                && e.access == *a
                                && e.max_request == l.wire
                                && e.max_reply == l.wire
                        })
                    })
                {
                    return Err(Error::Authority);
                }
            }
        }
        for o in &self.owners {
            if !name(&o.owner)
                || o.owner.as_str() <= previous
                || !name(&o.capability.provider)
                || !name(&o.capability.endpoint)
                || o.capability.major == 0
                || o.policy.role != Role::Owner
                || o.provider.endpoint != o.capability.endpoint
            {
                return Err(Error::Authority);
            }
            previous = &o.owner;
            module(&o.provider)?;
            check_identity(&o.policy.codec)?;
            if let Some(m) = kernel
                .registry
                .modules
                .iter()
                .find(|m| m.id == o.provider.module)
            {
                if m.manifest.id != o.capability.provider
                    || m.manifest
                        .exports
                        .get(&o.capability.endpoint)
                        .is_none_or(|e| e.major != o.capability.major)
                {
                    return Err(Error::Authority);
                }
            }
        }
        check_identity(&self.accounting.codec)?;
        let address = |r: &Resource| -> Result<()> {
            if !name(&r.key)
                || !self.owners.iter().any(|o| o.owner == r.owner)
                || !kernel.execution_resource_declared(r)?
            {
                return Err(Error::Authority);
            }
            Ok(())
        };
        let stamp = |s: &Stamp| -> Result<()> {
            address(&s.address)?;
            if s.generation == 0 || s.generation >= self.next_generation {
                return Err(Error::Authority);
            }
            Ok(())
        };
        let mut last = None;
        let mut generations = BTreeMap::new();
        for cell in &self.cells {
            stamp(&cell.stamp)?;
            if cell.value.len() > l.payload
                || last.as_ref().is_some_and(|r| r >= &cell.stamp.address)
            {
                return Err(Error::Limit);
            }
            if generations
                .insert(cell.stamp.generation, cell.stamp.address.clone())
                .is_some()
            {
                return Err(Error::Duplicate);
            }
            last = Some(cell.stamp.address.clone());
        }
        let mut serial = 0;
        let mut edges = 0usize;
        for p in &self.pending {
            let count = p
                .plan
                .readonly
                .len()
                .checked_add(p.plan.writes.len())
                .ok_or(Error::Limit)?;
            if p.serial <= serial
                || p.serial >= self.next_serial
                || count > l.participants
                || p.context.permits.len() != count
                || p.plan.events.len() > l.events
                || p.plan.events.iter().any(|e| e.len() > l.payload)
                || p.plan
                    .writes
                    .iter()
                    .any(|w| w.replacement.as_ref().is_some_and(|v| v.len() > l.payload))
                || self.binding.as_ref() != Some(&p.context.provider)
            {
                return Err(Error::Authority);
            }
            serial = p.serial;
            if p.context
                .operation
                .as_ref()
                .is_some_and(|o| !o.cleanup.contains(&self.cleanup))
            {
                return Err(Error::Authority);
            }
            let mut addresses = BTreeSet::new();
            for ((s, a), permit) in p
                .plan
                .readonly
                .iter()
                .map(|s| (s, Access::Read))
                .chain(p.plan.writes.iter().map(|w| (&w.stamp, Access::Write)))
                .zip(&p.context.permits)
            {
                stamp(s)?;
                if !addresses.insert(&s.address) || permit.target != s.address || permit.access != a
                {
                    return Err(Error::Authority);
                }
                let owner = self
                    .owners
                    .iter()
                    .find(|o| o.owner == s.address.owner)
                    .ok_or(Error::Authority)?;
                if permit.capability != owner.capability
                    || permit.resolution.first() != Some(&owner.provider)
                {
                    return Err(Error::Authority);
                }
                if generations
                    .get(&s.generation)
                    .is_some_and(|r| r != &s.address)
                {
                    return Err(Error::Authority);
                }
                generations.insert(s.generation, s.address.clone());
                edges = edges
                    .checked_add(permit.resolution.len())
                    .ok_or(Error::Limit)?;
                if edges > 262_144 {
                    return Err(Error::Limit);
                }
            }
            p.context.check(kernel)?;
        }
        if self.owners.iter().any(|o| o.policy.bytes.len() > l.payload)
            || self.accounting.bytes.len() > l.payload
        {
            return Err(Error::Limit);
        }
        Ok(())
    }
    pub fn validate(
        self,
        limits: &Limits,
        kernel: &KernelSnapshot,
        catalog: RestoreCatalog,
        authority: &impl ResourceAuthority,
    ) -> Result<Validated> {
        self.structural(limits, kernel)?;
        // All catalog and contextual admission precedes avoidable native codecs.
        catalog.get(&self.accounting)?;
        for o in &self.owners {
            catalog.get(&o.policy)?;
        }
        authority.approve(&self, kernel)?;
        catalog
            .get(&self.accounting)?
            .validate(&self.accounting.bytes, self.limits.payload)?;
        for o in &self.owners {
            catalog
                .get(&o.policy)?
                .validate(&o.policy.bytes, self.limits.payload)?;
        }
        Ok(Validated {
            checkpoint: self,
            catalog,
            cleanup_history: kernel.clone(),
        })
    }
}
pub struct Validated {
    checkpoint: Checkpoint,
    catalog: RestoreCatalog,
    cleanup_history: KernelSnapshot,
}
pub struct Staged {
    checkpoint: Checkpoint,
    catalog: RestoreCatalog,
    approved_kernel: KernelSnapshot,
    service: Service,
    ports: BTreeMap<String, NativePort>,
}
pub struct Restored {
    destination: ExecutionDestination,
    checkpoint: Checkpoint,
    service: Service,
    ids: BTreeMap<u64, Identity>,
}
impl Validated {
    /// Allocate exactly one hidden service. The kernel artifact factory must use
    /// ports() from this allocation; the workflow factory uses the same service.
    pub fn stage(self) -> Result<Staged> {
        self.cleanup_history.reserve_cleanup_history()?;
        let service = Service::new_after(
            self.checkpoint.limits.clone(),
            Rc::new(|_| false),
            self.checkpoint.namespace_high_water,
        )?;
        service.0.borrow_mut().quarantine = true;
        let ports = ["read", "prepare", "commit"]
            .into_iter()
            .map(|ep| {
                let value = service.clone();
                (
                    ep.into(),
                    Rc::new(move |bytes: &[u8], view: &CallerView| value.dispatch(ep, bytes, view))
                        as NativePort,
                )
            })
            .collect();
        Ok(Staged {
            checkpoint: self.checkpoint,
            catalog: self.catalog,
            approved_kernel: self.cleanup_history,
            service,
            ports,
        })
    }
}
impl Staged {
    pub fn ports(&self) -> BTreeMap<String, NativePort> {
        self.ports.clone()
    }
    /// Restore only the independently approved source pair. Check before
    /// delegating to kernel admission, factories or destination allocation.
    /// Hosts using separate kernel restoration still must pass finish's sealed
    /// provenance check; this helper also preserves a nonempty destination on
    /// source substitution before kernel reconstruction begins.
    pub fn restore_kernel(
        &self,
        kernel: &mut Kernel,
        saved: &KernelSnapshot,
        host: &impl nethacked_kernel::persistence::RetainedAuthority,
    ) -> Result<RestoredKernel> {
        saved.resource_snapshot_matches(&self.approved_kernel)?;
        kernel.restore_kernel(saved, host)
    }
    pub fn finish(self, kernel: &Kernel, mapped: &RestoredKernel) -> Result<Restored> {
        mapped.resource_snapshot_matches(&self.approved_kernel)?;
        if mapped.execution_namespace() != self.checkpoint.kernel_namespace {
            return Err(Error::Authority);
        }
        let c = &self.checkpoint;
        if let Some(b) = &c.binding {
            if let Some(h) = mapped.modules().get(&b.module) {
                kernel.resource_ports_match(*h, &self.ports)?;
            }
        }
        let claim = mapped.enroll_resources(c.cleanup)?;
        for p in &c.pending {
            mapped.resource_context_live(kernel, &p.context)?;
        }
        mapped.execution_live(kernel)?;
        let mut inner = self
            .service
            .0
            .try_borrow_mut()
            .map_err(|_| Error::NativeRefusal)?;
        let handle = |b: &SavedBinding, inert: bool| -> Result<ModuleHandle> {
            if inert && !mapped.modules().contains_key(&b.module) {
                Ok(ModuleHandle::inert_history(b.module))
            } else {
                let h = *mapped.modules().get(&b.module).ok_or(Error::Authority)?;
                if kernel.artifact(h)? != b.artifact {
                    return Err(Error::Authority);
                }
                Ok(h)
            }
        };
        let binding = c
            .binding
            .as_ref()
            .map(|b| {
                Ok((
                    handle(b, !mapped.modules().contains_key(&b.module))?,
                    b.artifact,
                ))
            })
            .transpose()?;
        let mut owner_names = BTreeMap::new();
        let mut resources = BTreeMap::new();
        // Address mapping is the actual admitted destination map, never an
        // independently supplied remapper. Enforce owner/key graph consistency.
        for r in c
            .cells
            .iter()
            .map(|s| &s.stamp.address)
            .chain(c.pending.iter().flat_map(|p| {
                p.plan
                    .readonly
                    .iter()
                    .chain(p.plan.writes.iter().map(|w| &w.stamp))
                    .map(|s| &s.address)
            }))
        {
            let fresh = mapped.execution_resource(r)?.clone();
            if !name(&fresh.owner) || !name(&fresh.key) {
                return Err(Error::Authority);
            }
            if owner_names
                .insert(r.owner.clone(), fresh.owner.clone())
                .is_some_and(|old| old != fresh.owner)
            {
                return Err(Error::Authority);
            }
            resources.insert(r.clone(), fresh);
        }
        for o in &c.owners {
            owner_names
                .entry(o.owner.clone())
                .or_insert(o.owner.clone());
        }
        if owner_names.values().collect::<BTreeSet<_>>().len() != owner_names.len() {
            return Err(Error::Authority);
        }
        let remap_stamp = |s: &Stamp| -> Result<Stamp> {
            Ok(Stamp {
                address: resources.get(&s.address).cloned().ok_or(Error::Authority)?,
                generation: s.generation,
                revision: s.revision,
            })
        };
        let mut state = State {
            resources: BTreeMap::new(),
            history: c.history.clone(),
            pending: BTreeMap::new(),
            next_serial: c.next_serial,
            next_generation: c.next_generation,
            commits: c.commits,
        };
        for cell in &c.cells {
            let stamp = remap_stamp(&cell.stamp)?;
            if state
                .resources
                .insert(
                    stamp.address.clone(),
                    Snapshot {
                        stamp,
                        value: cell.value.clone(),
                    },
                )
                .is_some()
            {
                return Err(Error::Duplicate);
            }
        }
        let cleanup = *mapped.cleanup().get(&c.cleanup).ok_or(Error::Authority)?;
        let mut inert = BTreeSet::new();
        let mut ids = BTreeMap::new();
        for p in &c.pending {
            let cx = &p.context;
            let operation = cx
                .operation
                .as_ref()
                .map(|op| {
                    if cx.historical {
                        mapped.terminal_input_source(&nethacked_kernel::persistence::InputHistory {
                            source: op.clone(),
                            caller: cx.caller.module,
                            caller_artifact: cx.caller.artifact,
                            resolution: vec![cx.provider.clone()],
                        })
                    } else {
                        mapped.execution_source(op)
                    }
                })
                .transpose()?;
            let permits = cx
                .permits
                .iter()
                .map(|p| {
                    Ok(CapabilityPermit {
                        capability: p.capability.clone(),
                        resolution: p
                            .resolution
                            .iter()
                            .map(|b| {
                                Ok(nethacked_kernel::ProviderBinding {
                                    handle: handle(b, cx.historical)?,
                                    artifact: b.artifact,
                                    endpoint: b.endpoint.clone(),
                                })
                            })
                            .collect::<Result<_>>()?,
                    })
                })
                .collect::<Result<_>>()?;
            let plan = Plan {
                readonly: p
                    .plan
                    .readonly
                    .iter()
                    .map(remap_stamp)
                    .collect::<Result<_>>()?,
                writes: p
                    .plan
                    .writes
                    .iter()
                    .map(|w| {
                        Ok(Write {
                            stamp: remap_stamp(&w.stamp)?,
                            replacement: w.replacement.clone(),
                        })
                    })
                    .collect::<Result<_>>()?,
                events: p.plan.events.clone(),
            };
            state.pending.insert(
                p.serial,
                Prepared {
                    operation,
                    caller: handle(&cx.caller, cx.historical)?,
                    artifact: cx.caller.artifact,
                    provider: handle(&cx.provider, cx.historical)?,
                    provider_artifact: cx.provider.artifact,
                    plan,
                    permits,
                },
            );
            if cx.historical {
                inert.insert(p.serial);
            } else {
                ids.insert(
                    p.serial,
                    Identity {
                        service: inner.namespace,
                        serial: p.serial,
                    },
                );
            }
        }
        // Everything structural and destination-bound above precedes decoding.
        let mut owners = BTreeMap::new();
        for o in &c.owners {
            let provider = handle(
                &o.provider,
                !mapped.modules().contains_key(&o.provider.module),
            )?;
            owners.insert(
                owner_names[&o.owner].clone(),
                OwnerPolicy {
                    capability: o.capability.clone(),
                    provider,
                    artifact: o.provider.artifact,
                    validate: self.catalog.get(&o.policy)?.decode(
                        &o.policy.bytes,
                        mapped,
                        c.limits.payload,
                    )?,
                },
            );
        }
        let accounting = self.catalog.get(&c.accounting)?.decode(
            &c.accounting.bytes,
            mapped,
            c.limits.payload,
        )?;
        inner.owners = owners;
        inner.accounting = accounting;
        inner.state = state;
        inner.binding = binding;
        inner.cleanup_identity = cleanup;
        inner.inert = inert;
        inner.quarantine = false;
        drop(inner);
        claim.finish();
        Ok(Restored {
            destination: mapped.execution_destination(),
            checkpoint: self.checkpoint,
            service: self.service,
            ids,
        })
    }
}
impl Restored {
    pub fn service(&self) -> &Service {
        &self.service
    }
    /// Only executable plans receive fresh trusted transaction IDs.
    pub fn transaction(&self, old_serial: u64) -> Result<Identity> {
        self.ids.get(&old_serial).cloned().ok_or(Error::Authority)
    }
    pub fn matches_destination(&self, other: &ExecutionDestination) -> bool {
        self.destination.same_destination(other)
    }
    pub fn matches_checkpoint(&self, c: &Checkpoint) -> Result<bool> {
        Ok(self.checkpoint.to_bytes(MAX_BYTES)? == c.to_bytes(MAX_BYTES)?)
    }
}

fn bounded_json(bytes: &[u8]) -> Result<()> {
    let mut depth = 0usize;
    let mut quoted = false;
    let mut escape = false;
    for b in bytes {
        if quoted {
            if escape {
                escape = false;
            } else if *b == b'\\' {
                escape = true;
            } else if *b == b'"' {
                quoted = false;
            }
        } else {
            match b {
                b'"' => quoted = true,
                b'{' | b'[' => {
                    depth += 1;
                    if depth > 32 {
                        return Err(Error::Buffer);
                    }
                }
                b'}' | b']' => depth = depth.checked_sub(1).ok_or(Error::Buffer)?,
                _ => (),
            }
        }
    }
    if depth != 0 || quoted {
        return Err(Error::Buffer);
    }
    Ok(())
}

#[cfg(test)]
mod borrow_tests {
    use super::*;
    #[test]
    fn pure_capture_refuses_both_borrows() {
        let service = Service::new(Limits::default(), Rc::new(|_| true)).unwrap();
        let mut kernel = Kernel::new(Default::default()).unwrap();
        let (cleanup, addresses) = service.persistence_roots().unwrap();
        let saved = kernel
            .kernel_snapshot_with_service_roots(&Default::default(), &[cleanup], &addresses)
            .unwrap();
        let index = kernel.execution_capture_index(&saved).unwrap();
        let catalog = CaptureCatalog::new(&service);
        let shared = service.0.borrow();
        assert!(matches!(
            service.capture_local(&index, &saved, &catalog),
            Err(PersistenceError::CaptureBusy)
        ));
        drop(shared);
        let exclusive = service.0.borrow_mut();
        assert!(matches!(
            service.persistence_roots(),
            Err(PersistenceError::CaptureBusy)
        ));
        assert!(matches!(
            service.persistence_enrollment(&kernel),
            Err(PersistenceError::CaptureBusy)
        ));
        drop(exclusive);
    }
    #[test]
    fn ordinary_allocation_advances_past_holes_and_exhaustion() {
        let counter = AtomicU64::new(1);
        // Test the same checked arithmetic without poisoning process globals.
        let minimum = (u64::MAX - 1).checked_add(1).unwrap();
        assert!(counter
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n
                .max(minimum)
                .checked_add(1))
            .is_err());
        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }
}
