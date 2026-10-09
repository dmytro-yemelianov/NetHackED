//! Host-only independent state seams. Bytes are evidence, never credentials.
//!
//! The D4 orchestrator can adapt these local snapshots to the accepted D1 wire.
//! No dependency on that orchestrator, no candidate constructor, no activation.
use super::*;
use serde::{Deserialize, Serialize};
mod execution;
pub use execution::*;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactIdentity {
    pub name: String,
    pub version: u32,
    pub artifact: [u8; 32],
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodecIdentity {
    pub schema: ArtifactIdentity,
    pub codec: ArtifactIdentity,
    pub policy: ArtifactIdentity,
    pub schema_digest: [u8; 32],
}
impl CodecIdentity {
    fn validate(&self) -> Result<()> {
        for id in [&self.schema, &self.codec, &self.policy] {
            if id.name.is_empty() || id.name.len() > 256 || id.version == 0 {
                return Err(Error::Schema);
            }
        }
        Ok(())
    }
}
/// Live edges must be remapped; historical edges remain explicit old origins.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateReference {
    pub owner: OwnerId,
    pub generation: u64,
    pub schema: [u8; 32],
    pub historical: bool,
}
/// Trusted bounded, pure adapter, independent of command preparation.
/// Native alias exclusion, allocation/work bounds and fidelity are host premises.
pub trait PersistenceCodec<T> {
    fn identity(&self) -> CodecIdentity;
    fn encode_state(&self, state: &T, limit: usize) -> Result<Vec<u8>>;
    fn decode_state(&self, bytes: &[u8], limit: usize, remap: &Remap) -> Result<T>;
    fn validate_state(&self, state: &T) -> Result<()>;
    fn references(&self, state: &T) -> Result<Vec<StateReference>>;
    /// Exact host-approved port/dependency/query-policy configuration. A save
    /// cannot remove required dependencies or opt into another query policy.
    fn validate_configuration(
        &self,
        state: &T,
        ports: &BTreeMap<String, Port>,
        policy: &QueryPolicy,
    ) -> Result<()>;
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BackendIdentity {
    wasm: bool,
    slot: usize,
    helpers: Vec<([u8; 32], String)>,
}
fn backend_identity<T>(backend: &Backend<T>) -> Result<BackendIdentity> {
    match backend {
        Backend::Native(_) => Ok(BackendIdentity {
            wasm: false,
            slot: 0,
            helpers: vec![],
        }),
        Backend::Wasm { slot, binding, .. } => Ok(BackendIdentity {
            wasm: true,
            slot: *slot,
            helpers: binding
                .resolution
                .iter()
                .map(|b| (b.artifact, b.endpoint.clone()))
                .collect(),
        }),
        Backend::CommitCallback => Err(Error::Unsupported),
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnerRow {
    old: OwnerId,
    generation: u64,
    revision: u64,
    schema: [u8; 32],
    module_artifact: [u8; 32],
    provider: String,
    live: bool,
    ports: BTreeMap<String, Port>,
    policy: QueryPolicy,
    codec: Option<CodecIdentity>,
    backend: Option<BackendIdentity>,
    payload: Vec<u8>,
    references: Vec<StateReference>,
}
/// Historical evidence cannot be turned into OperationBinding/CapabilityPermit.
/// Original complete receipt encoding is retained as an explicitly historical origin.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoricalCommit {
    pub origin_host: u64,
    pub ticket: u64,
    pub epoch: u64,
    pub policy: [u8; 32],
    pub participants: Vec<StateReference>,
    pub revisions: Vec<u64>,
    pub candidate_serials: Vec<u64>,
    pub event_counts: Vec<usize>,
    pub events: Vec<Vec<u8>>,
    pub reply: Vec<u8>,
    pub origin_encoding: Vec<u8>,
    evidence: evidence::CommitReceipt,
    contributions: Vec<Contribution>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LocalHistory {
    pub(super) next_ticket: u64,
    pub(super) epoch: u64,
    pub(super) records: Vec<HistoricalCommit>,
    pub(super) consumed: Vec<u64>,
    pub(super) events: usize,
    pub(super) bytes: usize,
    pub(super) replies: usize,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    version: u32,
    source_host: u64,
    origins: Vec<u64>,
    next_owner: u64,
    next_candidate: u64,
    owners: Vec<OwnerRow>,
    history: LocalHistory,
}
impl Checkpoint {
    /// Local bounded bytes; the orchestrator owns D1 envelope/table adaptation.
    pub fn to_bytes(&self, limit: usize) -> Result<Vec<u8>> {
        let bytes = serde_json::to_vec(self).map_err(|_| Error::Invalid)?;
        bound(bytes.len(), limit)?;
        Ok(bytes)
    }
    pub fn from_bytes(bytes: &[u8], limit: usize) -> Result<Self> {
        bound(bytes.len(), limit)?;
        let result: Self = serde_json::from_slice(bytes).map_err(|_| Error::Invalid)?;
        // Also refuses duplicate keys, alternative spellings and trailing data.
        if result.to_bytes(limit)? != bytes {
            return Err(Error::Invalid);
        }
        Ok(result)
    }
    pub fn owners(&self) -> Vec<OwnerId> {
        self.owners
            .iter()
            .filter(|r| r.live)
            .map(|r| r.old)
            .collect()
    }
}
/// Checked mappings are assembled exclusively by hidden host construction.
pub struct Remap {
    rows: BTreeMap<OwnerId, (OwnerId, u64, [u8; 32], bool)>,
    source_host: u64,
    target_host: u64,
    origins: BTreeSet<u64>,
}
impl Remap {
    pub fn owner(&self, old: OwnerId) -> Result<OwnerId> {
        let (new, _, _, live) = self.rows.get(&old).ok_or(Error::Stale)?;
        if !live {
            return Err(Error::Stale);
        }
        Ok(*new)
    }
    pub fn reference(&self, old: &StateReference) -> Result<StateReference> {
        let mapped = if old.historical && self.origins.contains(&old.owner.host) {
            self.rows
                .iter()
                .find(|(id, _)| id.serial == old.owner.serial)
                .map(|(_, row)| row)
        } else {
            self.rows.get(&old.owner)
        };
        let (new, generation, schema, live) = mapped.ok_or(Error::Stale)?;
        if old.generation != *generation || old.schema != *schema || (!old.historical && !live) {
            return Err(Error::Stale);
        }
        let mut result = old.clone();
        if !old.historical {
            result.owner = *new;
        }
        Ok(result)
    }
    /// Resource strings are parsed and checked against a declared live port.
    pub fn resource(&self, old: &Resource, checkpoint: &Checkpoint) -> Result<Resource> {
        if old.owner != format!("owners-{}", self.source_host) {
            return Err(Error::Stale);
        }
        let (serial, port) = old.key.split_once('.').ok_or(Error::Invalid)?;
        let serial: u64 = serial.parse().map_err(|_| Error::Invalid)?;
        let id = OwnerId {
            host: self.source_host,
            serial,
        };
        let row = checkpoint
            .owners
            .iter()
            .find(|r| r.old == id)
            .ok_or(Error::Stale)?;
        if !row.live || !row.ports.contains_key(port) || old.key != format!("{serial}.{port}") {
            return Err(Error::Stale);
        }
        Ok(Resource {
            owner: format!("owners-{}", self.target_host),
            key: format!("{}.{}", self.owner(id)?.serial, port),
        })
    }
    fn identity(&self) -> Self {
        Self {
            rows: self
                .rows
                .iter()
                .map(|(old, (_, g, s, live))| (*old, (*old, *g, *s, *live)))
                .collect(),
            source_host: self.source_host,
            target_host: self.source_host,
            origins: self.origins.clone(),
        }
    }
}
/// One capture owns the common gate for all heterogeneous owner encoders.
pub struct Capture {
    host: Host,
    _gate: Gate,
    checkpoint: Checkpoint,
    captured: BTreeSet<OwnerId>,
}
impl Host {
    pub fn capture_state(&self) -> Result<Capture> {
        self.capture_state_mode(false)
    }
    fn capture_state_mode(&self, execution: bool) -> Result<Capture> {
        let gate = self.close_visibility()?;
        let registry = self.0.registry.try_borrow().map_err(|_| Error::Busy)?;
        if !execution && (registry.pending != 0 || self.0.behaviors.get() != 0) {
            return Err(Error::Unsupported);
        }
        let history = self
            .0
            .publication
            .try_borrow()
            .map_err(|_| Error::Busy)?
            .capture_local(self.0.id, &registry, execution)?;
        let mut owners = Vec::new();
        for (id, entry) in &registry.entries {
            let route = entry.route.upgrade();
            let stamp = route
                .as_ref()
                .map_or_else(|| entry.stamp.clone(), |r| r.metadata().current_stamp());
            let live = route.as_ref().is_some_and(|r| r.metadata().live.get());
            owners.push(OwnerRow {
                old: *id,
                generation: stamp.generation,
                revision: stamp.revision,
                schema: stamp.schema,
                module_artifact: stamp.module.artifact,
                provider: route
                    .as_ref()
                    .map_or_else(String::new, |r| r.metadata().provider_name.clone()),
                live,
                ports: if live {
                    route.as_ref().unwrap().metadata().ports.clone()
                } else {
                    BTreeMap::new()
                },
                policy: QueryPolicy::Noncacheable,
                codec: None,
                backend: None,
                payload: vec![],
                references: vec![],
            });
        }
        Ok(Capture {
            host: self.clone(),
            _gate: gate,
            checkpoint: Checkpoint {
                version: 1,
                source_host: self.0.id,
                origins: self
                    .0
                    .persistence_origins
                    .borrow()
                    .iter()
                    .copied()
                    .collect(),
                next_owner: registry.next_owner,
                next_candidate: registry.next_candidate,
                owners,
                history,
            },
            captured: BTreeSet::new(),
        })
    }
    /// Includes restored historical origins and new receipts, without credentials.
    pub fn persistence_history(&self) -> Result<Vec<HistoricalCommit>> {
        let _gate = self.close_visibility()?;
        Ok(self
            .0
            .publication
            .borrow()
            .capture_local(self.0.id, &self.0.registry.borrow(), false)?
            .records)
    }
}
impl Capture {
    pub fn owner<T: 'static>(
        &mut self,
        owner: &Owner<T>,
        codec: &dyn PersistenceCodec<T>,
    ) -> Result<()> {
        self.owner_mode(owner, codec, false)
    }
    fn owner_mode<T: 'static>(
        &mut self,
        owner: &Owner<T>,
        codec: &dyn PersistenceCodec<T>,
        execution: bool,
    ) -> Result<()> {
        if !Rc::ptr_eq(&owner.host.0, &self.host.0) || self.captured.contains(&owner.id()) {
            return Err(Error::Invalid);
        }
        self.host.route(owner.id())?;
        if !execution
            && (owner.compatible.borrow().is_some() || owner.metadata.retained_behaviors.get() != 0)
        {
            return Err(Error::Unsupported);
        }
        #[cfg(feature = "recorded-inputs")]
        if !execution && !owner.recorded.borrow().is_empty() {
            return Err(Error::Unsupported);
        }
        let slot = owner.slot.try_borrow().map_err(|_| Error::Busy)?;
        let identity = codec.identity();
        identity.validate()?;
        if identity.schema_digest != owner.metadata.stamp.schema {
            return Err(Error::Schema);
        }
        codec.validate_state(&slot.state)?;
        codec.validate_configuration(
            &slot.state,
            &owner.metadata.ports,
            &owner.query_policy.borrow(),
        )?;
        let bytes = codec.encode_state(&slot.state, self.host.0.limits.payload_bytes)?;
        bound(bytes.len(), self.host.0.limits.payload_bytes)?;
        let refs = codec.references(&slot.state)?;
        bound(refs.len(), self.host.0.limits.observations)?;
        let remap = remap_for(&self.checkpoint, self.host.0.id);
        for r in &refs {
            remap.reference(r)?;
        }
        let row = self
            .checkpoint
            .owners
            .iter_mut()
            .find(|r| r.old == owner.id())
            .ok_or(Error::Stale)?;
        row.codec = Some(identity);
        row.backend = Some(backend_identity(&owner.backend.borrow())?);
        row.payload = bytes;
        row.references = refs;
        row.policy = owner.query_policy.borrow().clone();
        self.captured.insert(owner.id());
        Ok(())
    }
    pub fn finish(self) -> Result<Checkpoint> {
        if self
            .checkpoint
            .owners
            .iter()
            .any(|r| r.live && !self.captured.contains(&r.old))
        {
            return Err(Error::Unsupported);
        }
        Ok(self.checkpoint)
    }
}
fn remap_for(checkpoint: &Checkpoint, target_host: u64) -> Remap {
    Remap {
        source_host: checkpoint.source_host,
        target_host,
        origins: checkpoint.origins.iter().copied().collect(),
        rows: checkpoint
            .owners
            .iter()
            .map(|r| {
                (
                    r.old,
                    (
                        OwnerId {
                            host: target_host,
                            serial: r.old.serial,
                        },
                        r.generation,
                        r.schema,
                        r.live,
                    ),
                )
            })
            .collect(),
    }
}
/// A hidden fresh namespace. Only finish releases its gate. Refusal cannot affect
/// a source or a nonempty destination, since neither is used as storage here.
pub struct Restore {
    host: Host,
    gate: Option<Gate>,
    checkpoint: Checkpoint,
    remap: Remap,
    restored: BTreeSet<OwnerId>,
}
impl Restore {
    /// Bindings are trusted host resolutions; artifact hashes cannot issue grants.
    pub fn new(
        kernel: &Kernel,
        checkpoint: Checkpoint,
        limits: Limits,
        publication: PublicationLimits,
        bindings: &BTreeMap<[u8; 32], ModuleHandle>,
    ) -> Result<Self> {
        // Admission precedes allocation, including historical/disposed namespaces.
        bound(checkpoint.origins.len(), limits.owners)?;
        if checkpoint.origins.is_empty()
            || checkpoint.origins[0] == 0
            || !checkpoint.origins.contains(&checkpoint.source_host)
            || checkpoint.origins.windows(2).any(|w| w[0] >= w[1])
        {
            return Err(Error::Invalid);
        }
        let excluded = checkpoint.origins.iter().copied().collect();
        let host = Host::new_excluding(limits, &excluded)?;
        host.configure_publication(publication)?;
        let gate = host.close_visibility()?;
        validate_checkpoint(&checkpoint, &host)?;
        Self::from_reserved(kernel, checkpoint, host, gate, bindings)
    }
    fn from_reserved(
        kernel: &Kernel,
        checkpoint: Checkpoint,
        host: Host,
        gate: Gate,
        bindings: &BTreeMap<[u8; 32], ModuleHandle>,
    ) -> Result<Self> {
        let remap = remap_for(&checkpoint, host.0.id);
        for row in &checkpoint.owners {
            let module = *bindings.get(&row.module_artifact).ok_or(Error::Schema)?;
            if kernel.artifact(module)? != row.module_artifact {
                return Err(Error::Schema);
            }
            if row.live && kernel.manifest(module)?.id != row.provider {
                return Err(Error::Schema);
            }
            if !row.live {
                let id = remap.rows[&row.old].0;
                host.0.registry.borrow_mut().entries.insert(
                    id,
                    Entry {
                        stamp: Stamp {
                            owner: id,
                            generation: row.generation,
                            revision: row.revision,
                            schema: row.schema,
                            module: ProviderBinding {
                                handle: module,
                                artifact: row.module_artifact,
                                endpoint: String::new(),
                            },
                        },
                        route: Weak::<Owner<()>>::new(),
                    },
                );
            }
        }
        host.0.registry.borrow_mut().next_owner = checkpoint.next_owner;
        host.0.registry.borrow_mut().next_candidate = checkpoint.next_candidate;
        host.0
            .persistence_origins
            .borrow_mut()
            .extend(checkpoint.origins.iter().copied());
        host.0
            .publication
            .borrow_mut()
            .restore_local(&checkpoint.history)?;
        Ok(Self {
            host,
            gate: Some(gate),
            checkpoint,
            remap,
            restored: BTreeSet::new(),
        })
    }
    pub fn remap(&self) -> &Remap {
        &self.remap
    }
    pub fn owner<T: 'static>(
        &mut self,
        kernel: &Kernel,
        old: OwnerId,
        module: ModuleHandle,
        codec: &dyn PersistenceCodec<T>,
        factory: impl FnOnce() -> Result<Backend<T>>,
    ) -> Result<Rc<Owner<T>>> {
        let row = self.row(old)?.clone();
        self.check_codec(&row, codec)?;
        // First decode/validate in old namespace, before remapping or transformation.
        let old_state = codec.decode_state(
            &row.payload,
            self.host.0.limits.payload_bytes,
            &self.remap.identity(),
        )?;
        self.validate_decoded(&row, codec, &old_state)?;
        let state =
            codec.decode_state(&row.payload, self.host.0.limits.payload_bytes, &self.remap)?;
        codec.validate_state(&state)?;
        let expected = row
            .references
            .iter()
            .map(|r| self.remap.reference(r))
            .collect::<Result<Vec<_>>>()?;
        if codec.references(&state)? != expected {
            return Err(Error::Stale);
        }
        self.install(kernel, row, module, codec, state, factory)
    }
    /// Both exact codecs are resolved by the host. No old validator => no call to
    /// this seam, even when a pure migrator is available. Validation precedes it.
    #[allow(clippy::too_many_arguments)]
    pub fn migrate<Old, T: 'static>(
        &mut self,
        kernel: &Kernel,
        old: OwnerId,
        module: ModuleHandle,
        old_codec: &dyn PersistenceCodec<Old>,
        target: &dyn PersistenceCodec<T>,
        transform: impl FnOnce(Old, &Remap) -> Result<T>,
        factory: impl FnOnce() -> Result<Backend<T>>,
    ) -> Result<Rc<Owner<T>>> {
        let row = self.row(old)?.clone();
        self.check_codec(&row, old_codec)?;
        target.identity().validate()?;
        let state = old_codec.decode_state(
            &row.payload,
            self.host.0.limits.payload_bytes,
            &self.remap.identity(),
        )?;
        self.validate_decoded(&row, old_codec, &state)?;
        let next = transform(state, &self.remap)?;
        target.validate_state(&next)?;
        let expected = row
            .references
            .iter()
            .map(|r| self.remap.reference(r))
            .collect::<Result<Vec<_>>>()?;
        if target.references(&next)? != expected {
            return Err(Error::Stale);
        }
        // Migration of referenced schema identities needs a graph-level D4 plan.
        if target.identity().schema_digest != row.schema
            && self.checkpoint.owners.iter().any(|r| {
                r.references.iter().any(|e| !e.historical && e.owner == old)
                    || r.ports
                        .values()
                        .any(|p| p.dependencies.iter().any(|d| d.owner == old))
            })
        {
            return Err(Error::Unsupported);
        }
        self.install(kernel, row, module, target, next, factory)
    }
    fn row(&self, old: OwnerId) -> Result<&OwnerRow> {
        if self.restored.contains(&old) {
            return Err(Error::Invalid);
        }
        self.checkpoint
            .owners
            .iter()
            .find(|r| r.old == old && r.live)
            .ok_or(Error::Stale)
    }
    fn check_codec<T>(&self, row: &OwnerRow, codec: &dyn PersistenceCodec<T>) -> Result<()> {
        let identity = codec.identity();
        identity.validate()?;
        if row.codec.as_ref() != Some(&identity) {
            return Err(Error::Schema);
        }
        Ok(())
    }
    fn validate_decoded<T>(
        &self,
        row: &OwnerRow,
        codec: &dyn PersistenceCodec<T>,
        state: &T,
    ) -> Result<()> {
        codec.validate_state(state)?;
        codec.validate_configuration(state, &row.ports, &row.policy)?;
        if codec.references(state)? != row.references {
            return Err(Error::Stale);
        }
        for reference in &row.references {
            self.remap.reference(reference)?;
        }
        Ok(())
    }
    fn install<T: 'static>(
        &mut self,
        kernel: &Kernel,
        row: OwnerRow,
        module: ModuleHandle,
        codec: &dyn PersistenceCodec<T>,
        state: T,
        factory: impl FnOnce() -> Result<Backend<T>>,
    ) -> Result<Rc<Owner<T>>> {
        if kernel.artifact(module)? != row.module_artifact
            || kernel.manifest(module)?.id != row.provider
        {
            return Err(Error::Schema);
        }
        let backend = factory()?;
        if Some(backend_identity(&backend)?) != row.backend {
            return Err(Error::Schema);
        }
        let mut ports = row.ports.clone();
        for port in ports.values_mut() {
            for dependency in &mut port.dependencies {
                dependency.owner = self.remap.owner(dependency.owner)?;
            }
        }
        codec.validate_configuration(&state, &ports, &row.policy)?;
        bound(
            codec
                .encode_state(&state, self.host.0.limits.payload_bytes)?
                .len(),
            self.host.0.limits.payload_bytes,
        )?;
        let identity = codec.identity();
        let owner = self.host.enroll_locked(
            kernel,
            OwnerSpec {
                module,
                schema: identity.schema_digest,
                ports,
            },
            state,
            backend,
            Some((self.remap.owner(row.old)?, row.generation, row.revision)),
        )?;
        *owner.query_policy.borrow_mut() = row.policy;
        self.restored.insert(row.old);
        Ok(owner)
    }
    pub fn finish(mut self) -> Result<Host> {
        if self
            .checkpoint
            .owners
            .iter()
            .any(|r| r.live && !self.restored.contains(&r.old))
        {
            return Err(Error::Unsupported);
        }
        for row in self.checkpoint.owners.iter().filter(|r| r.live) {
            self.host.route(self.remap.owner(row.old)?)?;
        }
        for entry in self.host.0.registry.borrow().entries.values() {
            if let Some(route) = entry.route.upgrade() {
                for port in route.metadata().ports.values() {
                    for dependency in &port.dependencies {
                        let target = self.host.route(dependency.owner)?;
                        if target.metadata().stamp.schema != dependency.schema
                            || target
                                .metadata()
                                .ports
                                .get(&dependency.query)
                                .map(|p| p.access)
                                != Some(Access::Read)
                        {
                            return Err(Error::Schema);
                        }
                    }
                }
            }
        }
        drop(self.gate.take());
        Ok(self.host.clone())
    }
}
impl Drop for Restore {
    fn drop(&mut self) {
        // Owners returned for host wiring are unusable if construction refuses.
        // Dropping the gate must never accidentally activate escaped handles.
        if self.gate.is_some() {
            self.host.0.quarantine.set(true);
        }
    }
}
fn validate_checkpoint(c: &Checkpoint, host: &Host) -> Result<()> {
    validate_checkpoint_bounds(c, &host.0.limits, &host.0.publication.borrow().limits)
}
fn validate_checkpoint_bounds(
    c: &Checkpoint,
    limits: &Limits,
    publication: &PublicationLimits,
) -> Result<()> {
    if c.version != 1 || c.source_host == 0 || c.next_owner == 0 || c.next_candidate == 0 {
        return Err(Error::Invalid);
    }
    bound(c.owners.len(), limits.owners)?;
    bound(c.origins.len(), limits.owners)?;
    if !c.origins.contains(&c.source_host)
        || c.origins.first() == Some(&0)
        || c.origins.windows(2).any(|w| w[0] >= w[1])
    {
        return Err(Error::Invalid);
    }
    let mut seen = BTreeSet::new();
    let mut previous = 0;
    for row in &c.owners {
        if row.old.host != c.source_host
            || row.old.serial <= previous
            || row.old.serial >= c.next_owner
            || row.generation != row.old.serial
            || !seen.insert(row.old)
        {
            return Err(Error::Invalid);
        }
        previous = row.old.serial;
        bound(row.payload.len(), limits.payload_bytes)?;
        bound(row.references.len(), limits.observations)?;
        bound(row.ports.len(), limits.ports)?;
        bound(row.provider.len(), 256)?;
        if row.live {
            if row.ports.is_empty() || row.codec.is_none() || row.backend.is_none() {
                return Err(Error::Invalid);
            }
            let identity = row.codec.as_ref().unwrap();
            identity.validate()?;
            if identity.schema_digest != row.schema {
                return Err(Error::Schema);
            }
        } else if row.codec.is_some()
            || row.backend.is_some()
            || !row.payload.is_empty()
            || !row.references.is_empty()
        {
            return Err(Error::Invalid);
        }
        for (name, port) in &row.ports {
            if name.is_empty() || name.len() > 256 {
                return Err(Error::Invalid);
            }
            bound(port.dependencies.len(), limits.observations)?;
            let mut deps = BTreeSet::new();
            for dependency in &port.dependencies {
                if !deps.insert(dependency.owner) {
                    return Err(Error::Invalid);
                }
                let target = c
                    .owners
                    .iter()
                    .find(|r| r.old == dependency.owner && r.live)
                    .ok_or(Error::Stale)?;
                if target.schema != dependency.schema
                    || target.ports.get(&dependency.query).map(|p| p.access) != Some(Access::Read)
                {
                    return Err(Error::Schema);
                }
            }
        }
    }
    let remap = remap_for(c, c.source_host);
    for row in &c.owners {
        for reference in &row.references {
            remap.reference(reference)?;
        }
    }
    c.history.validate(publication, c)?;
    Ok(())
}
impl LocalHistory {
    fn validate(&self, limits: &PublicationLimits, c: &Checkpoint) -> Result<()> {
        bound(
            usize::try_from(self.next_ticket).map_err(|_| Error::Limit)?,
            limits.transactions,
        )?;
        if self.epoch != self.records.len() as u64 || self.epoch > self.next_ticket {
            return Err(Error::Invalid);
        }
        let mut tickets = BTreeSet::new();
        let mut events = 0usize;
        let mut bytes = 0usize;
        let mut replies = 0usize;
        let mut serials = BTreeSet::new();
        let mut lineage = BTreeMap::<u64, u64>::new();
        let mut previous_ticket = 0;
        let mut schemas = BTreeMap::new();
        for (index, record) in self.records.iter().enumerate() {
            if !c.origins.contains(&record.origin_host)
                || record.ticket == 0
                || record.ticket > self.next_ticket
                || !tickets.insert(record.ticket)
                || record.epoch != index as u64 + 1
                || record.participants.is_empty()
                || record.participants.len() != record.revisions.len()
                || record.participants.len() != record.candidate_serials.len()
                || record.participants.len() != record.event_counts.len()
                || !self.consumed.contains(&record.ticket)
            {
                return Err(Error::Invalid);
            }
            record.validate_evidence()?;
            if record.ticket <= previous_ticket {
                return Err(Error::Invalid);
            }
            previous_ticket = record.ticket;
            let event_count = record
                .event_counts
                .iter()
                .try_fold(0usize, |n, e| n.checked_add(*e).ok_or(Error::Limit))?;
            if event_count != record.events.len() {
                return Err(Error::Invalid);
            }
            for serial in &record.candidate_serials {
                if *serial == 0 || *serial >= c.next_candidate || !serials.insert(*serial) {
                    return Err(Error::Invalid);
                }
            }
            bound(record.participants.len(), limits.participants)?;
            let mut owners = BTreeSet::new();
            for (reference, revision) in record.participants.iter().zip(&record.revisions) {
                if !reference.historical
                    || reference.owner.host != record.origin_host
                    || !owners.insert(reference.owner)
                    || *revision == u64::MAX
                {
                    return Err(Error::Invalid);
                }
                // Old historical origins survive multiple restarts. When this is
                // the immediate source namespace, validate against its tombstones.
                let row = c
                    .owners
                    .iter()
                    .find(|r| r.old.serial == reference.owner.serial)
                    .ok_or(Error::Stale)?;
                if let Some(schema) = schemas.insert(reference.owner, reference.schema) {
                    if schema != reference.schema {
                        return Err(Error::Schema);
                    }
                }
                if reference.owner.host == c.source_host && reference.schema != row.schema {
                    return Err(Error::Schema);
                }
                let expected = lineage.entry(reference.owner.serial).or_insert(0);
                if *revision != *expected {
                    return Err(Error::Invalid);
                }
                *expected = expected.checked_add(1).ok_or(Error::Limit)?;
                if reference.generation != row.generation || *revision >= row.revision {
                    return Err(Error::Stale);
                }
            }
            for event in &record.events {
                bytes = bytes.checked_add(event.len()).ok_or(Error::Limit)?;
            }
            bytes = bytes
                .checked_add(record.origin_encoding.len())
                .and_then(|n| n.checked_add(record.reply.len()))
                .ok_or(Error::Limit)?;
            events = events
                .checked_add(record.events.len())
                .ok_or(Error::Limit)?;
            replies = replies
                .checked_add(record.reply.len())
                .ok_or(Error::Limit)?;
        }
        for row in &c.owners {
            if lineage.get(&row.old.serial).copied().unwrap_or(0) != row.revision {
                return Err(Error::Invalid);
            }
        }
        serials.clear();
        for ticket in &self.consumed {
            if *ticket == 0 || *ticket > self.next_ticket || !serials.insert(*ticket) {
                return Err(Error::Invalid);
            }
        }
        if serials.iter().copied().collect::<Vec<_>>() != self.consumed
            || self.consumed.len() as u64 != self.next_ticket
            || events != self.events
            || bytes != self.bytes
            || replies != self.replies
        {
            return Err(Error::Invalid);
        }
        bound(events, limits.history_events)?;
        bound(bytes, limits.history_bytes)?;
        bound(replies, limits.reply_bytes)?;
        Ok(())
    }
}

// Kernel credential interiors have no public serialization or constructors.
// Preserve them as diagnostic leaves only, never decode them into authority.
// The receipt tree and all owner/publication fields remain typed and reconciled.
mod evidence {
    use super::*;
    #[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
    pub(super) struct Diagnostic(String);
    impl std::fmt::Debug for Diagnostic {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(&self.0)
        }
    }
    fn diagnostic(value: &impl std::fmt::Debug) -> Diagnostic {
        Diagnostic(format!("{value:?}"))
    }
    #[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub(super) struct ProviderBinding {
        pub(super) handle: Diagnostic,
        pub(super) artifact: [u8; 32],
        pub(super) endpoint: String,
    }
    impl From<&nethacked_kernel::ProviderBinding> for ProviderBinding {
        fn from(r: &nethacked_kernel::ProviderBinding) -> Self {
            Self {
                handle: diagnostic(&r.handle),
                artifact: r.artifact,
                endpoint: r.endpoint.clone(),
            }
        }
    }
    #[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub(super) struct CapabilityPermit {
        pub(super) capability: Capability,
        pub(super) resolution: Vec<ProviderBinding>,
    }
    impl From<&nethacked_kernel::CapabilityPermit> for CapabilityPermit {
        fn from(r: &nethacked_kernel::CapabilityPermit) -> Self {
            Self {
                capability: r.capability.clone(),
                resolution: r.resolution.iter().map(Into::into).collect(),
            }
        }
    }
    #[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub(super) struct Stamp {
        pub(super) owner: OwnerId,
        pub(super) generation: u64,
        pub(super) revision: u64,
        pub(super) schema: [u8; 32],
        pub(super) module: ProviderBinding,
    }
    impl From<&super::Stamp> for Stamp {
        fn from(r: &super::Stamp) -> Self {
            Self {
                owner: r.owner,
                generation: r.generation,
                revision: r.revision,
                schema: r.schema,
                module: (&r.module).into(),
            }
        }
    }
    #[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub(super) struct Receipt {
        pub(super) serial: u64,
        pub(super) stamp: Stamp,
        pub(super) caller: Diagnostic,
        pub(super) caller_artifact: [u8; 32],
        pub(super) operation: Diagnostic,
        pub(super) command: String,
        pub(super) capability: CapabilityPermit,
        pub(super) behavior: Vec<ProviderBinding>,
        pub(super) next_digest: [u8; 32],
        pub(super) event_count: usize,
        pub(super) request_digest: [u8; 32],
        #[cfg(feature = "recorded-inputs")]
        recorded_inputs: Vec<Diagnostic>,
    }
    impl From<&super::Receipt> for Receipt {
        fn from(r: &super::Receipt) -> Self {
            Self {
                serial: r.serial,
                stamp: (&r.stamp).into(),
                caller: diagnostic(&r.caller),
                caller_artifact: r.caller_artifact,
                operation: diagnostic(&r.operation),
                command: r.command.clone(),
                capability: (&r.capability).into(),
                behavior: r.behavior.iter().map(Into::into).collect(),
                next_digest: r.next_digest,
                event_count: r.event_count,
                request_digest: r.request_digest,
                #[cfg(feature = "recorded-inputs")]
                recorded_inputs: r.recorded_inputs.iter().map(diagnostic).collect(),
            }
        }
    }
    #[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub(super) struct ObservationReceipt {
        pub(super) stamp: Stamp,
        pub(super) query: String,
        pub(super) caller: Diagnostic,
        pub(super) caller_artifact: [u8; 32],
        pub(super) capability: CapabilityPermit,
        pub(super) data_digest: [u8; 32],
        pub(super) dependencies: Vec<ObservationReceipt>,
        #[cfg(feature = "recorded-inputs")]
        recorded_inputs: Vec<Diagnostic>,
    }
    impl From<&super::ObservationReceipt> for ObservationReceipt {
        fn from(r: &super::ObservationReceipt) -> Self {
            Self {
                stamp: (&r.stamp).into(),
                query: r.query.clone(),
                caller: diagnostic(&r.caller),
                caller_artifact: r.caller_artifact,
                capability: (&r.capability).into(),
                data_digest: r.data_digest,
                dependencies: r.dependencies.iter().map(Into::into).collect(),
                #[cfg(feature = "recorded-inputs")]
                recorded_inputs: r.recorded_inputs.iter().map(diagnostic).collect(),
            }
        }
    }
    #[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub(super) struct CommitReceipt {
        pub(super) ticket: u64,
        pub(super) epoch: u64,
        pub(super) caller: Diagnostic,
        pub(super) caller_artifact: [u8; 32],
        pub(super) operation: Diagnostic,
        pub(super) coordinator: CapabilityPermit,
        pub(super) policy: [u8; 32],
        pub(super) participants: Vec<Receipt>,
        pub(super) observations: Vec<ObservationReceipt>,
        pub(super) events: Vec<Vec<u8>>,
        pub(super) reply: Vec<u8>,
        pub(super) history_encoding: Vec<u8>,
    }
    impl From<&super::CommitReceipt> for CommitReceipt {
        fn from(r: &super::CommitReceipt) -> Self {
            Self {
                ticket: r.ticket,
                epoch: r.epoch,
                caller: diagnostic(&r.caller),
                caller_artifact: r.caller_artifact,
                operation: diagnostic(&r.operation),
                coordinator: (&r.coordinator).into(),
                policy: r.policy,
                participants: r.participants.iter().map(Into::into).collect(),
                observations: r.observations.iter().map(Into::into).collect(),
                events: r.events.clone(),
                reply: r.reply.clone(),
                history_encoding: Vec::new(),
            }
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Contribution {
    owner: OwnerId,
    serial: u64,
    events: Vec<Vec<u8>>,
    reply: Vec<u8>,
}
impl HistoricalCommit {
    pub(super) fn from_receipt(
        origin_host: u64,
        r: &CommitReceipt,
        evidence: &[ProposalView],
    ) -> Self {
        Self {
            origin_host,
            ticket: r.ticket,
            epoch: r.epoch,
            policy: r.policy,
            participants: r
                .participants
                .iter()
                .map(|p| StateReference {
                    owner: p.stamp.owner,
                    generation: p.stamp.generation,
                    schema: p.stamp.schema,
                    historical: true,
                })
                .collect(),
            revisions: r.participants.iter().map(|p| p.stamp.revision).collect(),
            candidate_serials: r.participants.iter().map(|p| p.serial).collect(),
            event_counts: r.participants.iter().map(|p| p.event_count).collect(),
            events: r.events.clone(),
            reply: r.reply.clone(),
            origin_encoding: r.history_encoding.clone(),
            evidence: r.into(),
            contributions: r
                .participants
                .iter()
                .map(|p| {
                    let contribution = evidence
                        .iter()
                        .find(|e| {
                            e.receipt.stamp.owner == p.stamp.owner && e.receipt.serial == p.serial
                        })
                        .expect("checked preflight contribution");
                    Contribution {
                        owner: p.stamp.owner,
                        serial: p.serial,
                        events: contribution.events.clone(),
                        reply: contribution.reply.clone(),
                    }
                })
                .collect(),
        }
    }
    fn validate_evidence(&self) -> Result<()> {
        let e = &self.evidence;
        if e.ticket != self.ticket
            || e.epoch != self.epoch
            || e.policy != self.policy
            || e.participants.len() != self.participants.len()
            || self.contributions.len() != self.participants.len()
            || !e.history_encoding.is_empty()
            || format!("owners-history-v1:{e:?}").as_bytes() != self.origin_encoding
        {
            return Err(Error::Invalid);
        }
        let mut events = Vec::new();
        let mut reply = Vec::new();
        reply.extend_from_slice(&e.ticket.to_le_bytes());
        reply.extend_from_slice(&e.epoch.to_le_bytes());
        for (i, (p, contribution)) in e.participants.iter().zip(&self.contributions).enumerate() {
            let reference = &self.participants[i];
            if p.stamp.owner != reference.owner
                || p.stamp.generation != reference.generation
                || p.stamp.schema != reference.schema
                || p.stamp.revision != self.revisions[i]
                || p.serial != self.candidate_serials[i]
                || p.event_count != self.event_counts[i]
                || p.caller != e.caller
                || p.caller_artifact != e.caller_artifact
                || p.operation != e.operation
                || contribution.owner != p.stamp.owner
                || contribution.serial != p.serial
                || contribution.events.len() != p.event_count
            {
                return Err(Error::Invalid);
            }
            events.extend(contribution.events.iter().cloned());
            reply.extend_from_slice(
                &u64::try_from(contribution.reply.len())
                    .map_err(|_| Error::Limit)?
                    .to_le_bytes(),
            );
            reply.extend_from_slice(&contribution.reply);
        }
        if events != self.events || events != e.events || reply != self.reply || reply != e.reply {
            return Err(Error::Invalid);
        }
        Ok(())
    }
}
