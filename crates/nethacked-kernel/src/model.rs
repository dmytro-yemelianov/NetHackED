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
#[derive(Clone, Debug)]
pub struct CallerView {
    pub(crate) caller: Option<ModuleHandle>,
    pub(crate) caller_artifact: Option<[u8; 32]>,
    pub(crate) provider: ModuleHandle,
    pub(crate) provider_artifact: [u8; 32],
    pub(crate) authority: Authority,
    pub(crate) permits: Vec<(CapabilityPermit, Authority)>,
    pub(crate) max_reply: u32,
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
impl CallerView {
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
