//! Independent owner-local slots and sealed heterogeneous publication.
#![cfg_attr(not(test), forbid(unsafe_code))]

use nethacked_kernel::{
    Access, CallerView, Capability, CapabilityPermit, CleanupIdentity, Kernel, ModuleHandle,
    ModulePin, OperationBinding, ProviderBinding, PureCapture, Resource,
};
use sha2::{Digest, Sha256};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    rc::{Rc, Weak},
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Kernel(nethacked_kernel::Error),
    Limit,
    Authority,
    Stale,
    Schema,
    Busy,
    Invalid,
    Invariant,
    Unsupported,
    Referenced,
    Quarantined,
}
pub type Result<T> = std::result::Result<T, Error>;
impl From<nethacked_kernel::Error> for Error {
    fn from(e: nethacked_kernel::Error) -> Self {
        Self::Kernel(e)
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Debug)]
pub struct Limits {
    /// Includes tombstones; identities are never evicted or reused.
    pub owners: usize,
    pub candidates: usize,
    pub ports: usize,
    pub observations: usize,
    pub payload_bytes: usize,
    pub observation_bytes: usize,
    pub events: usize,
    pub event_bytes: usize,
    pub reply_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            owners: 128,
            candidates: 64,
            ports: 16,
            observations: 32,
            payload_bytes: 16_384,
            observation_bytes: 16_384,
            events: 32,
            event_bytes: 16_384,
            reply_bytes: 16_384,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct OwnerId {
    host: u64,
    serial: u64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Stamp {
    pub owner: OwnerId,
    pub generation: u64,
    pub revision: u64,
    pub schema: [u8; 32],
    pub module: ProviderBinding,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Dependency {
    pub owner: OwnerId,
    pub query: String,
    pub schema: [u8; 32],
}
#[derive(Clone, Debug)]
pub struct Port {
    pub access: Access,
    pub major: u32,
    pub dependencies: Vec<Dependency>,
}
#[derive(Clone, Debug)]
pub struct OwnerSpec {
    pub module: ModuleHandle,
    pub schema: [u8; 32],
    pub ports: BTreeMap<String, Port>,
}

/// Trusted host adapters must exclude interior mutability, shared mutable aliases,
/// external effects and unbounded work, including in validators and codecs.
/// This is a trust obligation, not something Rust trait bounds establish.
pub trait Schema<T> {
    fn validate(&self, state: &T) -> Result<()>;
    fn encode(&self, state: &T) -> Result<Vec<u8>>;
    fn query(
        &self,
        state: &T,
        name: &str,
        request: &[u8],
        observations: &[Observation],
    ) -> Result<Vec<u8>>;
}
pub trait NativeBehavior<T>: Schema<T> {
    #[cfg(feature = "recorded-inputs")]
    fn prepare_recorded(
        &self,
        state: &T,
        name: &str,
        request: &[u8],
        observations: &[Observation],
        recorded: &[nethacked_inputs::Observation],
    ) -> Result<Proposal<T>> {
        if !recorded.is_empty() {
            return Err(Error::Unsupported);
        }
        self.prepare(state, name, request, observations)
    }
    fn prepare(
        &self,
        state: &T,
        name: &str,
        request: &[u8],
        observations: &[Observation],
    ) -> Result<Proposal<T>>;
}
/// Host codec encodes own state + exact declared detached observations + command.
/// The returned guest bytes must decode a fully constructed typed proposal.
pub trait WasmCodec<T>: Schema<T> {
    #[cfg(feature = "recorded-inputs")]
    fn input_recorded(
        &self,
        state: &T,
        name: &str,
        request: &[u8],
        observations: &[Observation],
        recorded: &[nethacked_inputs::Observation],
    ) -> Result<Vec<u8>> {
        if !recorded.is_empty() {
            return Err(Error::Unsupported);
        }
        self.input(state, name, request, observations)
    }
    fn input(
        &self,
        state: &T,
        name: &str,
        request: &[u8],
        observations: &[Observation],
    ) -> Result<Vec<u8>>;
    fn decode(&self, bytes: &[u8]) -> Result<Proposal<T>>;
    /// Trusted query codec; decoded bytes are observations, never credentials.
    fn decode_query(&self, bytes: &[u8]) -> Result<Vec<u8>> {
        Ok(bytes.to_vec())
    }
}
/// Constructible by trusted domain adapters, never accepted from client requests.
pub struct Proposal<T> {
    pub next: T,
    pub events: Vec<Vec<u8>>,
    pub reply: Vec<u8>,
}
pub enum Backend<T> {
    Native(Rc<dyn NativeBehavior<T>>),
    Wasm {
        codec: Rc<dyn WasmCodec<T>>,
        slot: usize,
        binding: CapabilityPermit,
    },
    /// Explicit refusal for external callback-based storage.
    CommitCallback,
}
impl<T> Backend<T> {
    fn schema(&self) -> Result<&dyn Schema<T>> {
        match self {
            Self::Native(b) => Ok(b.as_ref()),
            Self::Wasm { codec, .. } => Ok(codec.as_ref()),
            Self::CommitCallback => Err(Error::Unsupported),
        }
    }
}
// This region is physically owned by one Owner<T>, not by Host/registry.
struct StateSlot<T> {
    state: T,
    revision: u64,
}
struct Metadata {
    stamp: Stamp,
    revision: Cell<u64>,
    provider_name: String,
    ports: BTreeMap<String, Port>,
    live: Cell<bool>,
    pending: Cell<usize>,
    behavior_revision: Cell<u64>,
    retained_behaviors: Cell<usize>,
}
trait Route {
    fn metadata(&self) -> &Metadata;
}
impl Metadata {
    fn current_stamp(&self) -> Stamp {
        let mut stamp = self.stamp.clone();
        stamp.revision = self.revision.get();
        stamp
    }
}
struct Entry {
    stamp: Stamp,
    route: Weak<dyn Route>,
}
struct Registry {
    next_owner: u64,
    next_candidate: u64,
    pending: usize,
    entries: BTreeMap<OwnerId, Entry>,
    staging: BTreeMap<u64, Weak<dyn Staged>>,
    tickets: BTreeMap<u64, Weak<publication::TicketState>>,
    staging_bytes: usize,
}
struct HostInner {
    id: u64,
    limits: Limits,
    gate: Cell<bool>,
    registry: RefCell<Registry>,
    cleanup: CleanupIdentity,
    publication: RefCell<Publication>,
    quarantine: Cell<bool>,
    behaviors: Cell<usize>,
}
#[derive(Clone)]
pub struct Host(Rc<HostInner>);
pub struct Owner<T> {
    host: Host,
    metadata: Metadata,
    slot: RefCell<StateSlot<T>>,
    backend: RefCell<Backend<T>>,
    pin: ModulePin,
    enrollment_code: RefCell<Option<ModulePin>>,
    compatible: RefCell<Option<Compatible<T>>>,
    query_policy: RefCell<QueryPolicy>,
    cache: RefCell<Weak<QueryCache>>,
    #[cfg(feature = "recorded-inputs")]
    recorded: RefCell<BTreeMap<String, Vec<RecordedDeclaration>>>,
}
struct Compatible<T> {
    codec: Rc<dyn WasmCodec<T>>,
    slot: usize,
    binding: CapabilityPermit,
    _pin: ModulePin,
}
/// Trusted host configuration. Deterministic excludes external/time-varying
/// inputs. Declared variable recorded inputs require Noncacheable queries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QueryPolicy {
    Noncacheable,
    Deterministic {
        profile: [u8; 32],
        input_policy: [u8; 32],
    },
}
/// One disposable entry per owner, held by the host, with no owner/cache cycle.
pub struct QueryCache {
    owner: OwnerId,
    entry: RefCell<Option<CachedEntry>>,
}
struct CachedEntry {
    request: Vec<u8>,
    policy: QueryPolicy,
    observation: Observation,
}
pub struct CachedQuery {
    pub observation: Observation,
    pub hit: bool,
}
impl<T> Route for Owner<T> {
    fn metadata(&self) -> &Metadata {
        &self.metadata
    }
}

/// A host gate guard; dropping it opens this serialized executor's visibility.
pub struct Gate {
    host: Host,
}
impl Drop for Gate {
    fn drop(&mut self) {
        self.host.0.gate.set(false);
    }
}
/// Detached immutable query bytes. The private route/code reference prevents
/// unsafe unload; no state pointer or mutable borrow is exposed.
#[derive(Clone)]
pub struct Observation {
    stamp: Stamp,
    name: String,
    data: Vec<u8>,
    caller: ModuleHandle,
    caller_artifact: [u8; 32],
    permit: CapabilityPermit,
    route: Rc<dyn Route>,
    dependencies: Vec<Observation>,
    code: ModulePin,
    behavior_revision: u64,
    helper: Option<PureCapture>,
    execution: Option<OperationBinding>,
    #[cfg(feature = "recorded-inputs")]
    recorded: Vec<nethacked_inputs::Observation>,
}
impl Observation {
    pub fn stamp(&self) -> &Stamp {
        &self.stamp
    }
    pub fn bytes(&self) -> &[u8] {
        &self.data
    }
    pub fn query(&self) -> &str {
        &self.name
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Receipt {
    pub serial: u64,
    pub stamp: Stamp,
    pub caller: ModuleHandle,
    pub caller_artifact: [u8; 32],
    pub operation: OperationBinding,
    pub command: String,
    pub capability: CapabilityPermit,
    pub behavior: Vec<ProviderBinding>,
    pub next_digest: [u8; 32],
    pub event_count: usize,
    pub request_digest: [u8; 32],
    #[cfg(feature = "recorded-inputs")]
    pub recorded_inputs: Vec<nethacked_inputs::ObservationIdentity>,
}
/// Opaque staged ownership. No apply/commit, field setter, next-state borrow or
/// wire constructor is public. Drop discards only staged state and releases bounds.
pub struct Candidate<T> {
    stage: Rc<Stage<T>>,
}
struct Prepared<T> {
    owner: Rc<Owner<T>>,
    next: T,
    events: Vec<Vec<u8>>,
    reply: Vec<u8>,
    dependencies: Vec<Observation>,
    code: Vec<ModulePin>,
    codec: Option<Rc<dyn WasmCodec<T>>>,
    helper: Option<PureCapture>,
    #[cfg(feature = "recorded-inputs")]
    recorded: Vec<nethacked_inputs::Observation>,
}
struct Stage<T> {
    host: Host,
    owner: Weak<Owner<T>>,
    receipt: Receipt,
    prepared: RefCell<Option<Prepared<T>>>,
    active: Cell<bool>,
    retained_bytes: usize,
}
impl<T> Candidate<T> {
    pub fn receipt(&self) -> &Receipt {
        &self.stage.receipt
    }
    pub fn discard(self) {
        drop(self);
    }
}
impl<T: 'static> Candidate<T> {
    /// Owner-provider validation. Coordinator validation is a separate path.
    pub fn validate(&self, view: &CallerView<'_>) -> Result<()> {
        let prepared = self.stage.prepared.try_borrow().map_err(|_| Error::Busy)?;
        let prepared = prepared.as_ref().ok_or(Error::Stale)?;
        let owner = &prepared.owner;
        let _gate = owner.host.close_visibility()?;
        let _invocation_pin = view.pin_invocation()?;
        authorize(
            &owner.metadata,
            &self.stage.receipt.command,
            view,
            Access::Write,
        )?;
        self.stage.preflight(prepared, view).map(|_| ())
    }
    pub fn erase(self) -> ErasedCandidate {
        ErasedCandidate { stage: self.stage }
    }
}

impl Host {
    pub fn new(limits: Limits) -> Result<Self> {
        if [
            limits.owners,
            limits.candidates,
            limits.ports,
            limits.observations,
            limits.payload_bytes,
            limits.observation_bytes,
            limits.events,
            limits.event_bytes,
            limits.reply_bytes,
        ]
        .contains(&0)
        {
            return Err(Error::Limit);
        }
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let id = NEXT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| Error::Limit)?;
        Ok(Self(Rc::new(HostInner {
            id,
            limits,
            gate: Cell::new(false),
            cleanup: CleanupIdentity::issue()?,
            publication: RefCell::new(Publication::default()),
            quarantine: Cell::new(false),
            behaviors: Cell::new(0),
            registry: RefCell::new(Registry {
                next_owner: 1,
                next_candidate: 1,
                pending: 0,
                entries: BTreeMap::new(),
                staging: BTreeMap::new(),
                tickets: BTreeMap::new(),
                staging_bytes: 0,
            }),
        })))
    }
    pub fn close_visibility(&self) -> Result<Gate> {
        if self.0.quarantine.get() {
            return Err(Error::Quarantined);
        }
        if self.0.gate.replace(true) {
            return Err(Error::Busy);
        }
        Ok(Gate { host: self.clone() })
    }
    /// Sorted metadata only, including tombstones. Never exposes payloads/routes.
    pub fn registry(&self) -> Result<Vec<Stamp>> {
        let _gate = self.close_visibility()?;
        Ok(self
            .0
            .registry
            .borrow()
            .entries
            .values()
            .map(|e| {
                e.route
                    .upgrade()
                    .map_or_else(|| e.stamp.clone(), |r| r.metadata().current_stamp())
            })
            .collect())
    }
    pub fn pending(&self) -> Result<usize> {
        let _gate = self.close_visibility()?;
        Ok(self.0.registry.borrow().pending)
    }
    /// Privileged validated initialization; does not provide a restoration API.
    pub fn enroll<T: 'static>(
        &self,
        kernel: &Kernel,
        spec: OwnerSpec,
        initial: T,
        backend: Backend<T>,
    ) -> Result<Rc<Owner<T>>> {
        let _gate = self.close_visibility()?;
        let schema = backend.schema()?;
        let limits = &self.0.limits;
        if spec.ports.is_empty() || spec.ports.len() > limits.ports {
            return Err(Error::Limit);
        }
        let manifest = kernel.manifest(spec.module)?;
        let artifact = kernel.artifact(spec.module)?;
        let mut code = vec![spec.module];
        if let Backend::Wasm { slot, binding, .. } = &backend {
            if manifest.dependencies.get(*slot).map(|d| &d.capability) != Some(&binding.capability)
                || binding.resolution.is_empty()
            {
                return Err(Error::Schema);
            }
            for b in &binding.resolution {
                if kernel.artifact(b.handle)? != b.artifact {
                    return Err(Error::Stale);
                }
                code.push(b.handle);
            }
        }
        for (name, port) in &spec.ports {
            let ep = manifest.exports.get(name).ok_or(Error::Schema)?;
            if ep.major != port.major || ep.access != port.access {
                return Err(Error::Schema);
            }
            if port.dependencies.len() > limits.observations {
                return Err(Error::Limit);
            }
            let mut seen = BTreeSet::new();
            for d in &port.dependencies {
                if !seen.insert(d.owner) {
                    return Err(Error::Invalid);
                }
                let route = self.route(d.owner)?;
                let m = route.metadata();
                if m.stamp.schema != d.schema
                    || m.ports.get(&d.query).map(|p| p.access) != Some(Access::Read)
                {
                    return Err(Error::Schema);
                }
            }
        }
        schema.validate(&initial)?;
        bound(schema.encode(&initial)?.len(), limits.payload_bytes)?;
        let pin = kernel.pin_modules(&[spec.module])?;
        let enrollment_code = if code.len() > 1 {
            Some(kernel.pin_modules(&code[1..])?)
        } else {
            None
        };
        let mut registry = self.0.registry.borrow_mut();
        if registry.entries.len() >= limits.owners {
            return Err(Error::Limit);
        }
        let next = registry.next_owner.checked_add(1).ok_or(Error::Limit)?;
        let id = OwnerId {
            host: self.0.id,
            serial: registry.next_owner,
        };
        let stamp = Stamp {
            owner: id,
            generation: id.serial,
            revision: 0,
            schema: spec.schema,
            module: ProviderBinding {
                handle: spec.module,
                artifact,
                endpoint: String::new(),
            },
        };
        let owner = Rc::new(Owner {
            host: self.clone(),
            metadata: Metadata {
                stamp: stamp.clone(),
                revision: Cell::new(0),
                provider_name: manifest.id.clone(),
                ports: spec.ports,
                live: Cell::new(true),
                pending: Cell::new(0),
                behavior_revision: Cell::new(0),
                retained_behaviors: Cell::new(0),
            },
            slot: RefCell::new(StateSlot {
                state: initial,
                revision: 0,
            }),
            backend: RefCell::new(backend),
            pin,
            enrollment_code: RefCell::new(enrollment_code),
            compatible: RefCell::new(None),
            query_policy: RefCell::new(QueryPolicy::Noncacheable),
            cache: RefCell::new(Weak::new()),
            #[cfg(feature = "recorded-inputs")]
            recorded: RefCell::new(BTreeMap::new()),
        });
        let route: Rc<dyn Route> = owner.clone();
        registry.entries.insert(
            id,
            Entry {
                stamp,
                route: Rc::downgrade(&route),
            },
        );
        registry.next_owner = next;
        Ok(owner)
    }
    fn route(&self, id: OwnerId) -> Result<Rc<dyn Route>> {
        if id.host != self.0.id {
            return Err(Error::Stale);
        }
        let route = self
            .0
            .registry
            .borrow()
            .entries
            .get(&id)
            .and_then(|e| e.route.upgrade())
            .ok_or(Error::Stale)?;
        if !route.metadata().live.get() {
            return Err(Error::Stale);
        }
        Ok(route)
    }
    /// Host lifecycle action, never a guest slot mutation. Tombstones stay bounded.
    pub fn retire(&self, id: OwnerId) -> Result<()> {
        let _gate = self.close_visibility()?;
        let route = self.route(id)?;
        if route.metadata().pending.get() != 0 {
            return Err(Error::Referenced);
        }
        route.metadata().live.set(false);
        Ok(())
    }
}
fn bound(size: usize, limit: usize) -> Result<()> {
    if size > limit {
        Err(Error::Limit)
    } else {
        Ok(())
    }
}
fn resource(m: &Metadata, name: &str) -> Resource {
    Resource {
        owner: format!("owners-{}", m.stamp.owner.host),
        key: format!("{}.{}", m.stamp.owner.serial, name),
    }
}
fn capability(m: &Metadata, name: &str) -> Result<Capability> {
    let port = m.ports.get(name).ok_or(Error::Invalid)?;
    Ok(Capability {
        provider: m.provider_name.clone(),
        endpoint: name.to_owned(),
        major: port.major,
    })
}
fn authorize(
    m: &Metadata,
    name: &str,
    view: &CallerView<'_>,
    access: Access,
) -> Result<CapabilityPermit> {
    if !m.live.get() {
        return Err(Error::Stale);
    }
    let cap = capability(m, name)?;
    let res = resource(m, name);
    if view.caller().is_none()
        || view.caller_artifact().is_none()
        || view.provider() != m.stamp.module.handle
        || view.provider_artifact() != m.stamp.module.artifact
        || view.authority().access < access
        || !view.authority().scope.allows(&res)
    {
        return Err(Error::Authority);
    }
    let permit = view.permit(&cap, &res, access).ok_or(Error::Authority)?;
    let expected = ProviderBinding {
        handle: view.provider(),
        artifact: view.provider_artifact(),
        endpoint: name.to_owned(),
    };
    if permit.resolution.last() != Some(&expected) {
        return Err(Error::Authority);
    }
    Ok(permit)
}
// Only host-sealed work may use retained validation. The opaque binding contains
// the exact issued operation, pipeline lease, checkpoint and phase; equality with
// the invocation is checked before recursively relaxing future behavior identity.
#[derive(Clone, Copy)]
enum ObservationMode<'a> {
    Current,
    Retained(&'a OperationBinding),
}
// Batch creation historically defers state/schema/authority refusals to commit.
// Seal only current behavior identities here; full retained validation remains
// mandatory at commit, including every recursive stamp, permit and helper capture.
fn admit_observation_behavior(
    host: &Host,
    observation: &Observation,
    view: &CallerView<'_>,
) -> Result<()> {
    let route = host.route(observation.stamp.owner)?;
    if observation.behavior_revision != route.metadata().behavior_revision.get() {
        let execution = observation.execution.as_ref().ok_or(Error::Stale)?;
        if view.operation() != Some(execution) || !execution.admits_cleanup(host.0.cleanup) {
            return Err(Error::Authority);
        }
        view.validate_pure(observation.helper.as_ref().ok_or(Error::Stale)?)?;
    }
    for dependency in &observation.dependencies {
        admit_observation_behavior(host, dependency, view)?;
    }
    Ok(())
}
fn validate_observation(
    host: &Host,
    o: &Observation,
    view: &CallerView<'_>,
    count: &mut usize,
    bytes: &mut usize,
    mode: ObservationMode<'_>,
) -> Result<()> {
    if let ObservationMode::Retained(operation) = mode {
        if view.operation() != Some(operation) || !operation.admits_cleanup(host.0.cleanup) {
            return Err(Error::Authority);
        }
    }
    *count = count.checked_add(1).ok_or(Error::Limit)?;
    *bytes = bytes.checked_add(o.data.len()).ok_or(Error::Limit)?;
    bound(*count, host.0.limits.observations)?;
    bound(*bytes, host.0.limits.observation_bytes)?;
    let route = host.route(o.stamp.owner)?;
    let m = route.metadata();
    if o.stamp != m.current_stamp()
        || !Rc::ptr_eq(&route, &o.route)
        || (matches!(mode, ObservationMode::Current)
            && o.behavior_revision != m.behavior_revision.get())
    {
        return Err(Error::Stale);
    }
    if view.caller() != Some(o.caller) || view.caller_artifact() != Some(o.caller_artifact) {
        return Err(Error::Authority);
    }
    let permit = view
        .permit(
            &capability(m, &o.name)?,
            &resource(m, &o.name),
            Access::Read,
        )
        .ok_or(Error::Authority)?;
    if permit != o.permit || o.code.module_count() == 0 {
        return Err(Error::Stale);
    }
    #[cfg(feature = "recorded-inputs")]
    for input in &o.recorded {
        input.validate(view)?;
        *count = count.checked_add(1).ok_or(Error::Limit)?;
        *bytes = bytes
            .checked_add(recorded_size(input)?)
            .ok_or(Error::Limit)?;
        bound(*count, host.0.limits.observations)?;
        bound(*bytes, host.0.limits.observation_bytes)?;
    }
    if let Some(helper) = &o.helper {
        view.validate_pure(helper)?;
    }
    for dep in &o.dependencies {
        validate_observation(host, dep, view, count, bytes, mode)?;
    }
    Ok(())
}
impl<T: 'static> Owner<T> {
    /// Trusted host action at the visibility boundary. Schema identity and
    /// encoding must remain compatible; semantic codec fidelity is trusted.
    /// No migration, state setter, provider replacement, or grant is supplied.
    pub fn admit_compatible_wasm(
        &self,
        kernel: &Kernel,
        schema: [u8; 32],
        codec: Rc<dyn WasmCodec<T>>,
        slot: usize,
        binding: CapabilityPermit,
    ) -> Result<()> {
        let _gate = self.host.close_visibility()?;
        self.host.route(self.id())?;
        let backend = self.backend.borrow();
        let Backend::Wasm {
            slot: old_slot,
            binding: old_binding,
            ..
        } = &*backend
        else {
            return Err(Error::Unsupported);
        };
        if schema != self.metadata.stamp.schema
            || slot != *old_slot
            || binding.capability != old_binding.capability
        {
            return Err(Error::Schema);
        }
        let next = self
            .metadata
            .behavior_revision
            .get()
            .checked_add(1)
            .ok_or(Error::Limit)?;
        let state = &self.slot.borrow().state;
        codec.validate(state)?;
        let encoded = codec.encode(state)?;
        bound(encoded.len(), self.host.0.limits.payload_bytes)?;
        if encoded != backend.schema()?.encode(state)? {
            return Err(Error::Schema);
        }
        let pin = kernel.pin_pure_binding(self.metadata.stamp.module.handle, slot, &binding)?;
        drop(backend);
        *self.backend.borrow_mut() = Backend::Wasm {
            codec: codec.clone(),
            slot,
            binding: binding.clone(),
        };
        *self.compatible.borrow_mut() = Some(Compatible {
            codec,
            slot,
            binding,
            _pin: pin,
        });
        self.enrollment_code.borrow_mut().take();
        self.metadata.behavior_revision.set(next);
        self.clear_cache_gated();
        Ok(())
    }
    fn clear_cache_gated(&self) {
        if let Some(cache) = self.cache.borrow().upgrade() {
            cache.entry.borrow_mut().take();
        }
    }
    pub fn set_query_policy(&self, policy: QueryPolicy) -> Result<()> {
        let _gate = self.host.close_visibility()?;
        self.host.route(self.id())?;
        #[cfg(feature = "recorded-inputs")]
        if policy != QueryPolicy::Noncacheable
            && self.recorded.borrow().iter().any(|(name, inputs)| {
                !inputs.is_empty()
                    && self
                        .metadata
                        .ports
                        .get(name)
                        .is_some_and(|p| p.access == Access::Read)
            })
        {
            return Err(Error::Unsupported);
        }
        *self.query_policy.borrow_mut() = policy;
        self.clear_cache_gated();
        Ok(())
    }
    pub fn query_cache(&self) -> Result<Rc<QueryCache>> {
        let _gate = self.host.close_visibility()?;
        self.host.route(self.id())?;
        if let Some(cache) = self.cache.borrow().upgrade() {
            return Ok(cache);
        }
        let cache = Rc::new(QueryCache {
            owner: self.id(),
            entry: RefCell::new(None),
        });
        *self.cache.borrow_mut() = Rc::downgrade(&cache);
        Ok(cache)
    }
    /// Single gated validation/recomputation. Policy is configured by the
    /// trusted host, never accepted from a request or a guest observation.
    pub fn query_cached(
        self: &Rc<Self>,
        cache: &QueryCache,
        name: &str,
        request: &[u8],
        observations: &[Observation],
        view: &CallerView<'_>,
    ) -> Result<CachedQuery> {
        let _gate = self.host.close_visibility()?;
        if cache.owner != self.id() {
            return Err(Error::Stale);
        }
        authorize(&self.metadata, name, view, Access::Read)?;
        bound(request.len(), self.host.0.limits.payload_bytes)?;
        self.observations(name, observations, view)?;
        #[cfg(feature = "recorded-inputs")]
        if self
            .recorded
            .borrow()
            .get(name)
            .is_some_and(|inputs| !inputs.is_empty())
        {
            return Err(Error::Unsupported);
        }
        let policy = self.query_policy.borrow().clone();
        if policy == QueryPolicy::Noncacheable {
            return Err(Error::Unsupported);
        }
        let mut entry = cache.entry.try_borrow_mut().map_err(|_| Error::Busy)?;
        if let Some(old) = entry.as_ref() {
            bound(
                old.observation.data.len(),
                self.host
                    .0
                    .limits
                    .reply_bytes
                    .min(view.max_reply() as usize),
            )?;
            if old.policy == policy
                && old.request == request
                && old.observation.name == name
                && same_observations(&old.observation.dependencies, observations)
            {
                let mut count = 0;
                let mut bytes = 0;
                match validate_observation(
                    &self.host,
                    &old.observation,
                    view,
                    &mut count,
                    &mut bytes,
                    ObservationMode::Current,
                ) {
                    Ok(()) => {
                        return Ok(CachedQuery {
                            observation: old.observation.clone(),
                            hit: true,
                        })
                    }
                    Err(Error::Stale) => {}
                    Err(error) => return Err(error),
                }
            }
        }
        // Failure cannot leave an apparently valid stale cache behind.
        entry.take();
        let observation = self.query_with_behavior_gated(
            name,
            request,
            observations,
            view,
            None,
            #[cfg(feature = "recorded-inputs")]
            &[],
        )?;
        *entry = Some(CachedEntry {
            request: request.to_vec(),
            policy,
            observation: observation.clone(),
        });
        Ok(CachedQuery {
            observation,
            hit: false,
        })
    }
    pub fn id(&self) -> OwnerId {
        self.metadata.stamp.owner
    }
    pub fn stamp(&self) -> Result<Stamp> {
        let _gate = self.host.close_visibility()?;
        self.host.route(self.id())?;
        Ok(self.metadata.current_stamp())
    }
    pub fn resource(&self, name: &str) -> Result<Resource> {
        capability(&self.metadata, name)?;
        Ok(resource(&self.metadata, name))
    }
    pub fn capability(&self, name: &str) -> Result<Capability> {
        capability(&self.metadata, name)
    }
    fn observations(
        &self,
        name: &str,
        observations: &[Observation],
        view: &CallerView<'_>,
    ) -> Result<()> {
        self.observations_with_mode(name, observations, view, ObservationMode::Current)
    }
    fn observations_with_mode(
        &self,
        name: &str,
        observations: &[Observation],
        view: &CallerView<'_>,
        mode: ObservationMode<'_>,
    ) -> Result<()> {
        let port = self.metadata.ports.get(name).ok_or(Error::Invalid)?;
        if observations.len() != port.dependencies.len() {
            return Err(Error::Invalid);
        }
        let mut total = 0usize;
        let mut count = 0usize;
        for (expected, observation) in port.dependencies.iter().zip(observations) {
            let route = self.host.route(expected.owner)?;
            let m = route.metadata();
            if observation.stamp.owner != expected.owner
                || observation.name != expected.query
                || observation.stamp.schema != expected.schema
            {
                return Err(Error::Schema);
            }
            if observation.stamp != m.current_stamp() || !Rc::ptr_eq(&route, &observation.route) {
                return Err(Error::Stale);
            }
            validate_observation(&self.host, observation, view, &mut count, &mut total, mode)?;
        }
        bound(total, self.host.0.limits.observation_bytes)
    }
    pub fn query(
        self: &Rc<Self>,
        name: &str,
        request: &[u8],
        observations: &[Observation],
        view: &CallerView<'_>,
    ) -> Result<Observation> {
        let _gate = self.host.close_visibility()?;
        self.query_with_behavior_gated(
            name,
            request,
            observations,
            view,
            None,
            #[cfg(feature = "recorded-inputs")]
            &[],
        )
    }
    fn query_with_behavior_gated(
        self: &Rc<Self>,
        name: &str,
        request: &[u8],
        observations: &[Observation],
        view: &CallerView<'_>,
        retained: Option<&RetainedBehavior<T>>,
        #[cfg(feature = "recorded-inputs")] recorded: &[nethacked_inputs::Observation],
    ) -> Result<Observation> {
        let permit = authorize(&self.metadata, name, view, Access::Read)?;
        if self.metadata.ports.get(name).map(|p| p.access) != Some(Access::Read) {
            return Err(Error::Invalid);
        }
        bound(request.len(), self.host.0.limits.payload_bytes)?;
        let mode = if let Some(retained) = retained {
            retained.validate(self, view)?;
            ObservationMode::Retained(view.operation().ok_or(Error::Authority)?)
        } else {
            ObservationMode::Current
        };
        self.observations_with_mode(name, observations, view, mode)?;
        #[cfg(feature = "recorded-inputs")]
        self.validate_recorded_inputs(name, observations, recorded, view)?;
        let code = view.pin_invocation()?;
        let current = self.compatible.borrow();
        let selected = retained.map(|b| Compatible {
            codec: b.inner.codec.clone(),
            slot: b.inner.slot,
            binding: b.inner.capture.binding().clone(),
            _pin: self.pin.clone(),
        });
        let live = selected.as_ref().or(current.as_ref());
        let (data, helper) = if let Some(live) = live {
            let capture = view.capture_pure(live.slot)?;
            if capture.binding() != &live.binding {
                return Err(Error::Stale);
            }
            #[cfg(not(feature = "recorded-inputs"))]
            let input = live
                .codec
                .input(&self.slot.borrow().state, name, request, observations)?;
            #[cfg(feature = "recorded-inputs")]
            let input = live.codec.input_recorded(
                &self.slot.borrow().state,
                name,
                request,
                observations,
                recorded,
            )?;
            bound(input.len(), self.host.0.limits.payload_bytes)?;
            let output = view.call_pure(live.slot, &input)?;
            if output.binding != live.binding {
                return Err(Error::Stale);
            }
            bound(output.bytes.len(), self.host.0.limits.reply_bytes)?;
            (live.codec.decode_query(&output.bytes)?, Some(capture))
        } else {
            (
                self.backend.borrow().schema()?.query(
                    &self.slot.borrow().state,
                    name,
                    request,
                    observations,
                )?,
                None,
            )
        };
        bound(
            data.len(),
            self.host
                .0
                .limits
                .reply_bytes
                .min(view.max_reply() as usize),
        )?;
        let mut count = 1usize;
        let mut bytes = data.len();
        #[cfg(feature = "recorded-inputs")]
        for input in recorded {
            count = count.checked_add(1).ok_or(Error::Limit)?;
            bytes = bytes
                .checked_add(recorded_size(input)?)
                .ok_or(Error::Limit)?;
        }
        for observation in observations {
            validate_observation(&self.host, observation, view, &mut count, &mut bytes, mode)?;
        }
        bound(count, self.host.0.limits.observations)?;
        bound(bytes, self.host.0.limits.observation_bytes)?;
        Ok(Observation {
            stamp: self.metadata.current_stamp(),
            name: name.to_owned(),
            data,
            caller: view.caller().ok_or(Error::Authority)?,
            caller_artifact: view.caller_artifact().ok_or(Error::Authority)?,
            permit,
            route: self.clone(),
            dependencies: observations.to_vec(),
            code,
            behavior_revision: retained
                .map_or(self.metadata.behavior_revision.get(), |b| b.inner.revision),
            execution: retained.map(|_| view.operation().unwrap().clone()),
            helper,
            #[cfg(feature = "recorded-inputs")]
            recorded: recorded.to_vec(),
        })
    }
    /// Bounded detached host inspection, under the same visibility gate.
    pub fn inspect(&self) -> Result<Vec<u8>> {
        let _gate = self.host.close_visibility()?;
        self.host.route(self.id())?;
        let data = self
            .backend
            .borrow()
            .schema()?
            .encode(&self.slot.borrow().state)?;
        bound(data.len(), self.host.0.limits.payload_bytes)?;
        Ok(data)
    }
    pub fn prepare(
        self: &Rc<Self>,
        name: &str,
        request: &[u8],
        observations: Vec<Observation>,
        view: &CallerView<'_>,
    ) -> Result<Candidate<T>> {
        self.prepare_with_behavior(
            name,
            request,
            observations,
            view,
            None,
            #[cfg(feature = "recorded-inputs")]
            &[],
        )
    }
    fn prepare_with_behavior(
        self: &Rc<Self>,
        name: &str,
        request: &[u8],
        observations: Vec<Observation>,
        view: &CallerView<'_>,
        retained: Option<&RetainedBehavior<T>>,
        #[cfg(feature = "recorded-inputs")] recorded: &[nethacked_inputs::Observation],
    ) -> Result<Candidate<T>> {
        let _gate = self.host.close_visibility()?;
        let permit = authorize(&self.metadata, name, view, Access::Write)?;
        if self.metadata.ports.get(name).map(|p| p.access) != Some(Access::Write) {
            return Err(Error::Invalid);
        }
        let operation = view.operation().ok_or(Error::Authority)?.clone();
        if !operation.admits_cleanup(self.host.0.cleanup) {
            return Err(Error::Authority);
        }
        let limits = &self.host.0.limits;
        bound(request.len(), limits.payload_bytes)?;
        let mode = if let Some(retained) = retained {
            retained.validate(self, view)?;
            ObservationMode::Retained(&operation)
        } else {
            ObservationMode::Current
        };
        self.observations_with_mode(name, &observations, view, mode)?;
        // Refuse exhausted counters/reservations before domain callbacks.
        {
            let r = self.host.0.registry.borrow();
            if r.pending >= limits.candidates {
                return Err(Error::Limit);
            }
            r.next_candidate.checked_add(1).ok_or(Error::Limit)?;
            self.slot
                .borrow()
                .revision
                .checked_add(1)
                .ok_or(Error::Limit)?;
            self.metadata
                .pending
                .get()
                .checked_add(1)
                .ok_or(Error::Limit)?;
        }
        #[cfg(feature = "recorded-inputs")]
        self.validate_recorded_inputs(name, &observations, recorded, view)?;
        let invocation_pin = view.pin_invocation()?;
        let backend = self.backend.borrow();
        let current = self.compatible.borrow();
        let selected = retained.map(|b| Compatible {
            codec: b.inner.codec.clone(),
            slot: b.inner.slot,
            binding: b.inner.capture.binding().clone(),
            _pin: self.pin.clone(),
        });
        let live = selected.as_ref().or(current.as_ref());
        let (proposal, behavior, mut code, codec, helper) = if let Some(live) = live {
            let capture = view.capture_pure(live.slot)?;
            if capture.binding() != &live.binding {
                return Err(Error::Stale);
            }
            #[cfg(not(feature = "recorded-inputs"))]
            let input =
                live.codec
                    .input(&self.slot.borrow().state, name, request, &observations)?;
            #[cfg(feature = "recorded-inputs")]
            let input = live.codec.input_recorded(
                &self.slot.borrow().state,
                name,
                request,
                &observations,
                recorded,
            )?;
            bound(input.len(), limits.payload_bytes)?;
            let output = view.call_pure(live.slot, &input)?;
            if output.binding != live.binding {
                return Err(Error::Stale);
            }
            bound(output.bytes.len(), limits.payload_bytes)?;
            (
                live.codec.decode(&output.bytes)?,
                output.binding.resolution,
                vec![self.pin.clone(), output.pin],
                Some(live.codec.clone()),
                Some(capture),
            )
        } else {
            match &*backend {
                Backend::Native(domain) => (
                    {
                        #[cfg(not(feature = "recorded-inputs"))]
                        {
                            domain.prepare(
                                &self.slot.borrow().state,
                                name,
                                request,
                                &observations,
                            )?
                        }
                        #[cfg(feature = "recorded-inputs")]
                        {
                            domain.prepare_recorded(
                                &self.slot.borrow().state,
                                name,
                                request,
                                &observations,
                                recorded,
                            )?
                        }
                    },
                    permit.resolution.clone(),
                    vec![self.pin.clone()],
                    None,
                    None,
                ),
                Backend::Wasm {
                    codec,
                    slot,
                    binding,
                } => {
                    #[cfg(not(feature = "recorded-inputs"))]
                    let input =
                        codec.input(&self.slot.borrow().state, name, request, &observations)?;
                    #[cfg(feature = "recorded-inputs")]
                    let input = codec.input_recorded(
                        &self.slot.borrow().state,
                        name,
                        request,
                        &observations,
                        recorded,
                    )?;
                    bound(input.len(), limits.payload_bytes)?;
                    let output = view.call_pure(*slot, &input)?;
                    if &output.binding != binding {
                        return Err(Error::Stale);
                    }
                    bound(output.bytes.len(), limits.payload_bytes)?;
                    (
                        codec.decode(&output.bytes)?,
                        output.binding.resolution,
                        vec![self.pin.clone(), output.pin],
                        Some(codec.clone()),
                        None,
                    )
                }
                Backend::CommitCallback => return Err(Error::Unsupported),
            }
        };
        code.push(invocation_pin);
        let schema = codec
            .as_ref()
            .map(|c| c.as_ref() as &dyn Schema<T>)
            .unwrap_or(backend.schema()?);
        schema.validate(&proposal.next)?;
        let encoded = schema.encode(&proposal.next)?;
        bound(encoded.len(), limits.payload_bytes)?;
        bound(proposal.events.len(), limits.events)?;
        let bytes = proposal
            .events
            .iter()
            .try_fold(0usize, |n, e| n.checked_add(e.len()).ok_or(Error::Limit))?;
        bound(bytes, limits.event_bytes)?;
        bound(
            proposal.reply.len(),
            limits.reply_bytes.min(view.max_reply() as usize),
        )?;
        let retained_bytes = encoded
            .len()
            .checked_add(bytes)
            .and_then(|n| n.checked_add(proposal.reply.len()))
            .and_then(|n| n.checked_add(limits.observation_bytes))
            .ok_or(Error::Limit)?;
        let staging_bytes = self
            .host
            .0
            .registry
            .borrow()
            .staging_bytes
            .checked_add(retained_bytes)
            .ok_or(Error::Limit)?;
        bound(
            staging_bytes,
            self.host.0.publication.borrow().limits.retained_bytes,
        )?;
        let mut registry = self.host.0.registry.borrow_mut();
        let serial = registry.next_candidate;
        registry.next_candidate = serial.checked_add(1).ok_or(Error::Limit)?;
        registry.pending += 1;
        registry.staging_bytes = staging_bytes;
        self.metadata.pending.set(self.metadata.pending.get() + 1);
        let receipt = Receipt {
            serial,
            stamp: self.metadata.current_stamp(),
            caller: view.caller().ok_or(Error::Authority)?,
            caller_artifact: view.caller_artifact().ok_or(Error::Authority)?,
            operation,
            command: name.to_owned(),
            capability: permit,
            behavior,
            next_digest: Sha256::digest(encoded).into(),
            event_count: proposal.events.len(),
            request_digest: Sha256::digest(request).into(),
            #[cfg(feature = "recorded-inputs")]
            recorded_inputs: recorded.iter().map(|r| r.identity().clone()).collect(),
        };
        let stage = Rc::new(Stage {
            host: self.host.clone(),
            owner: Rc::downgrade(self),
            receipt,
            prepared: RefCell::new(Some(Prepared {
                owner: self.clone(),
                next: proposal.next,
                events: proposal.events,
                reply: proposal.reply,
                dependencies: observations,
                code,
                codec,
                helper,
                #[cfg(feature = "recorded-inputs")]
                recorded: recorded.to_vec(),
            })),
            active: Cell::new(true),
            retained_bytes,
        });
        let erased: Rc<dyn Staged> = stage.clone();
        registry.staging.insert(serial, Rc::downgrade(&erased));
        Ok(Candidate { stage })
    }
}

fn same_observations(a: &[Observation], b: &[Observation]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| {
            a.stamp == b.stamp
                && a.name == b.name
                && a.data == b.data
                && a.permit == b.permit
                && a.caller == b.caller
                && a.caller_artifact == b.caller_artifact
                && a.behavior_revision == b.behavior_revision
                && Rc::ptr_eq(&a.route, &b.route)
                && same_recorded(a, b)
                && same_observations(&a.dependencies, &b.dependencies)
        })
}

#[cfg(test)]
mod fixtures;
#[cfg(test)]
#[path = "fixtures/live_queries.rs"]
mod live_queries;
#[cfg(test)]
mod probes;

mod behavior;
mod publication;
pub use behavior::RetainedBehavior;
pub use publication::{
    Accounting, Batch, CommitReceipt, ErasedCandidate, ObservationReceipt, ProposalView,
    PublicationLimits,
};
use publication::{Publication, Staged};

#[cfg(feature = "workflows")]
impl nethacked_workflows::CleanupParticipant for Host {
    fn identity(&self) -> nethacked_kernel::Result<Option<CleanupIdentity>> {
        self.cleanup_identity()
            .map(Some)
            .map_err(|_| nethacked_kernel::Error::NativeRefusal)
    }
    fn discard(&self, binding: &OperationBinding) -> nethacked_kernel::Result<()> {
        self.discard_operation(binding)
            .map_err(|_| nethacked_kernel::Error::NativeRefusal)
    }
}

#[cfg(not(feature = "recorded-inputs"))]
fn same_recorded(_: &Observation, _: &Observation) -> bool {
    true
}
#[cfg(feature = "recorded-inputs")]
fn same_recorded(a: &Observation, b: &Observation) -> bool {
    a.recorded.len() == b.recorded.len()
        && a.recorded
            .iter()
            .zip(&b.recorded)
            .all(|(a, b)| a.identity() == b.identity() && a.bytes() == b.bytes())
}
#[cfg(feature = "recorded-inputs")]
mod recorded;
#[cfg(feature = "recorded-inputs")]
use recorded::recorded_size;
#[cfg(feature = "recorded-inputs")]
pub use recorded::RecordedDeclaration;
