use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
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
}

impl Error {
    /// Stable ABI v1 refusal codes. A refusal never rolls back prior calls.
    pub fn abi_code(self) -> i32 {
        -(self as i32 + 1)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// Constructed only by a kernel, never accepted from guest bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ModuleHandle {
    pub(crate) instance: u64,
    pub(crate) generation: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum Access {
    Read,
    Write,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct Resource {
    pub owner: String,
    pub key: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Scope {
    All,
    Keys(BTreeSet<Resource>),
}
impl Scope {
    pub fn allows(&self, resource: &Resource) -> bool {
        match self {
            Self::All => true,
            Self::Keys(keys) => keys.contains(resource),
        }
    }
    pub(crate) fn intersect(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::All, scope) | (scope, Self::All) => scope.clone(),
            (Self::Keys(a), Self::Keys(b)) => Self::Keys(a.intersection(b).cloned().collect()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Authority {
    pub access: Access,
    pub scope: Scope,
}
impl Authority {
    pub(crate) fn contains(&self, other: &Self) -> bool {
        other.access <= self.access && self.scope.intersect(&other.scope) == other.scope
    }
    pub(crate) fn intersect(&self, other: &Self) -> Self {
        Self {
            access: self.access.min(other.access),
            scope: self.scope.intersect(&other.scope),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct Capability {
    pub provider: String,
    pub endpoint: String,
    pub major: u32,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Dependency {
    pub capability: Capability,
    pub access: Access,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Endpoint {
    pub major: u32,
    pub access: Access,
    pub max_request: u32,
    pub max_reply: u32,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Extension {
    pub target: Capability,
    pub export: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub exports: BTreeMap<String, Endpoint>,
    /// Slot order is semantic, and is preserved in the canonical encoding.
    pub dependencies: Vec<Dependency>,
    pub extension_points: BTreeSet<String>,
    pub extension: Option<Extension>,
}

/// Immutable authenticated view. Native ports must validate scoped requests
/// before effects; the runtime cannot interpret their opaque byte schema.
#[derive(Debug)]
pub struct CallerView<'a> {
    pub(crate) caller: Option<ModuleHandle>,
    pub(crate) caller_artifact: Option<[u8; 32]>,
    pub(crate) provider: ModuleHandle,
    pub(crate) provider_artifact: [u8; 32],
    pub(crate) authority: Authority,
    pub(crate) permits: Vec<(CapabilityPermit, Authority)>,
    pub(crate) max_reply: u32,
    pub(crate) operation: Option<OperationBinding>,
    pub(crate) callback_capture: Option<&'a dyn CallbackCapture>,
    pub(crate) pure_call: Option<&'a dyn PureCall>,
}
impl Clone for CallerView<'_> {
    fn clone(&self) -> Self {
        CallerView::clone(self)
    }
}

/// A borrowed capability, never a dispatcher or a mutable registry.
pub(crate) trait CallbackCapture: std::fmt::Debug {
    fn capture(&self, endpoint: &str, ceiling: Authority) -> Result<RetainedCallback>;
}

/// A lifetime-bound dispatcher restricted to guest computation.
pub(crate) trait PureCall: std::fmt::Debug {
    fn call(&self, slot: usize, request: &[u8]) -> Result<PureReply>;
    fn pin(&self) -> Result<ModulePin>;
    fn capture(&self, slot: usize) -> Result<PureCapture>;
    fn validate(&self, capture: &PureCapture) -> Result<()>;
}

/// Nonexecuting authenticated helper closure. Private fields prevent host or
/// guest bytes from manufacturing authority. Pins confer no execution rights.
#[derive(Clone, Debug)]
pub struct PureCapture {
    pub(crate) provider: ModuleHandle,
    pub(crate) slot: usize,
    pub(crate) ceiling: Authority,
    pub(crate) closure: Vec<(CapabilityPermit, Authority)>,
    pub(crate) pin: ModulePin,
}
impl PureCapture {
    pub fn binding(&self) -> &CapabilityPermit {
        &self.closure[0].0
    }
}

/// Detached bytes and the exact executable chain retained by this computation.
#[derive(Debug)]
pub struct PureReply {
    pub bytes: Vec<u8>,
    pub binding: CapabilityPermit,
    pub pin: ModulePin,
}

/// Shared bounded code ownership. Last drop releases the pin automatically.
#[derive(Clone, Debug)]
pub struct ModulePin(pub(crate) std::rc::Rc<crate::kernel::retention::PinData>);

/// Opaque host ownership. Clones share one bounded capture; the last drop releases it.
#[derive(Clone, Debug)]
pub struct RetainedCallback(pub(crate) std::rc::Rc<crate::kernel::retention::CallbackData>);
impl RetainedCallback {
    pub fn provenance(&self) -> &CallbackProvenance {
        &self.0.provenance
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallbackProvenance {
    pub caller: ProviderBinding,
    pub service: ProviderBinding,
    pub capability: Capability,
    pub slot: usize,
    pub source: OperationBinding,
}

/// Exact executable resolution of a declared capability, including extensions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderBinding {
    pub handle: ModuleHandle,
    pub artifact: [u8; 32],
    pub endpoint: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityPermit {
    pub capability: Capability,
    pub resolution: Vec<ProviderBinding>,
}

/// Ephemeral trusted-host identity. No wire constructor or serialization.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct PipelineLease {
    pub(crate) instance: u64,
    pub(crate) serial: u64,
}

/// An explicit host-owned reference set for optional services.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct OwnerReference {
    pub(crate) instance: u64,
    pub(crate) serial: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PipelineDescriptor {
    pub root: ProviderBinding,
    pub ceiling: Authority,
    pub routes: Vec<Vec<ProviderBinding>>,
    /// Runtime-local binding digest, not a persistence or machine-code claim.
    pub hash: [u8; 32],
}

#[derive(Clone, Debug)]
pub struct RetentionLimits {
    pub leases: usize,
    pub owners: usize,
    pub nodes: usize,
    pub slots: usize,
    pub references: usize,
}
impl Default for RetentionLimits {
    fn default() -> Self {
        Self {
            leases: 128,
            owners: 128,
            nodes: 1024,
            slots: 1024,
            references: 4096,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LifecycleState {
    pub registrations: usize,
    pub attachments: usize,
    pub grants: usize,
    pub leases: usize,
    pub owners: usize,
    pub references: usize,
    pub next_generation: u64,
    pub next_retention: u64,
    pub callbacks: usize,
    pub next_callback: u64,
}
impl CallerView<'_> {
    /// A detached metadata snapshot, preserving the original owned view API.
    /// The invocation-only capture capability is never cloned into storage.
    #[allow(
        clippy::should_implement_trait,
        reason = "Legacy view.clone() storage needs a detached static snapshot; Clone::Self retains its lifetime"
    )]
    pub fn clone(&self) -> CallerView<'static> {
        CallerView {
            caller: self.caller,
            caller_artifact: self.caller_artifact,
            provider: self.provider,
            provider_artifact: self.provider_artifact,
            authority: self.authority.clone(),
            permits: self.permits.clone(),
            max_reply: self.max_reply,
            operation: self.operation.clone(),
            callback_capture: None,
            pure_call: None,
        }
    }

    /// Retain this authenticated calling module's public callback export.
    pub fn capture_callback(&self, endpoint: &str, ceiling: Authority) -> Result<RetainedCallback> {
        self.callback_capture
            .ok_or(Error::Authority)?
            .capture(endpoint, ceiling)
    }
    /// Invoke this native provider's declared Wasm helper using the invocation's
    /// shared fuel/call/depth budget, retained routing and live grants. All
    /// descendant native dispatches refuse before callbacks, even Read ports.
    /// Detached clones deliberately have no callable bridge.
    pub fn call_pure(&self, slot: usize, request: &[u8]) -> Result<PureReply> {
        self.pure_call.ok_or(Error::Authority)?.call(slot, request)
    }
    /// Same current/retained routing and live grants as call_pure; no guest
    /// execution and no fuel/call budget allocation. Detached views refuse.
    pub fn capture_pure(&self, slot: usize) -> Result<PureCapture> {
        self.pure_call.ok_or(Error::Authority)?.capture(slot)
    }
    /// May inspect a transitive observation's private helper capture. This
    /// authenticates code and grants only, never the observation's owner port.
    pub fn validate_pure(&self, capture: &PureCapture) -> Result<()> {
        self.pure_call.ok_or(Error::Authority)?.validate(capture)
    }
    /// Retain the exact invocation code closure for detached observations/staging.
    /// Detached clones cannot acquire new pins or dispatch calls.
    pub fn pin_invocation(&self) -> Result<ModulePin> {
        self.pure_call.ok_or(Error::Authority)?.pin()
    }
    /// Inspect only the invoking module's live grants, under this call's ceiling.
    /// A permit is a snapshot, not a credential usable in a later invocation.
    pub fn permit(
        &self,
        capability: &Capability,
        resource: &Resource,
        access: Access,
    ) -> Option<CapabilityPermit> {
        self.permits
            .iter()
            .find(|(p, a)| {
                &p.capability == capability && a.access >= access && a.scope.allows(resource)
            })
            .map(|(p, _)| p.clone())
    }
    /// Native services must stage their complete reply before publishing effects.
    pub fn max_reply(&self) -> u32 {
        self.max_reply
    }
    pub fn caller(&self) -> Option<ModuleHandle> {
        self.caller
    }
    pub fn caller_artifact(&self) -> Option<[u8; 32]> {
        self.caller_artifact
    }
    pub fn provider(&self) -> ModuleHandle {
        self.provider
    }
    pub fn provider_artifact(&self) -> [u8; 32] {
        self.provider_artifact
    }
    pub fn authority(&self) -> &Authority {
        &self.authority
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Limits {
    pub module_bytes: usize,
    pub memory_bytes: usize,
    pub table_elements: usize,
    pub stack_bytes: usize,
    pub wasm_recursion: usize,
    pub payload_bytes: u32,
    pub fuel: u64,
    pub calls: u32,
    pub depth: u32,
    pub registrations: usize,
    pub manifest_entries: usize,
    pub scope_keys: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            module_bytes: 1024 * 1024,
            memory_bytes: 256 * 1024,
            table_elements: 128,
            stack_bytes: 4096,
            wasm_recursion: 64,
            payload_bytes: 16 * 1024,
            fuel: 100_000,
            calls: 64,
            depth: 8,
            registrations: 128,
            manifest_entries: 64,
            scope_keys: 64,
        }
    }
}

/// Host-issued, non-reused identity. Never decoded from module payloads.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct OperationId {
    pub(crate) host: u64,
    pub(crate) serial: u64,
}
impl OperationId {
    pub fn namespace(self) -> u64 {
        self.host
    }
    pub fn serial(self) -> u64 {
        self.serial
    }
}
/// Immutable authenticated invocation context, constructed by trusted host APIs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationBinding {
    pub(crate) operation: OperationId,
    pub(crate) pipeline: PipelineLease,
    pub(crate) checkpoint: u64,
    pub(crate) phase: String,
    pub(crate) cleanup: Vec<CleanupIdentity>,
}
impl OperationBinding {
    pub fn admits_cleanup(&self, identity: CleanupIdentity) -> bool {
        self.cleanup.binary_search(&identity).is_ok()
    }
    pub fn operation(&self) -> OperationId {
        self.operation
    }
    pub fn pipeline(&self) -> PipelineLease {
        self.pipeline
    }
    pub fn checkpoint(&self) -> u64 {
        self.checkpoint
    }
    pub fn phase(&self) -> &str {
        &self.phase
    }
}

/// Non-reused typed participant identity. Issue only in trusted host setup;
/// no wire constructor exists and no service lookup is associated with it.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct CleanupIdentity(u64);
impl CleanupIdentity {
    pub fn issue() -> Result<Self> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT_CLEANUP: AtomicU64 = AtomicU64::new(1);
        let id = NEXT_CLEANUP
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| Error::Limit)?;
        Ok(Self(id))
    }
}
impl CallerView<'_> {
    pub fn operation(&self) -> Option<&OperationBinding> {
        self.operation.as_ref()
    }
}
