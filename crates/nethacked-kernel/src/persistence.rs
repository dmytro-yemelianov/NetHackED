//! D3a local registry records. D4 supplies the bounded D1 envelope and host barrier.
//! These records are requests/evidence, never runtime credentials.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

const MAX_NAMESPACES: usize = 65_536;
const MAX_ROWS: usize = 65_536;
const MAX_EDGES: usize = 262_144;
const MAX_BYTES: usize = 64 * 1024 * 1024;

// Measure without allocating an oversized JSON buffer. This is accounting for
// local records, not an encoding/decoding API or a replacement for D1.
fn bounded_size(value: &impl Serialize) -> Result<usize> {
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .filter(|n| *n <= MAX_BYTES)
                .ok_or_else(|| std::io::Error::other("registry record limit"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter(0);
    serde_json::to_writer(&mut counter, value).map_err(|_| Error::Limit)?;
    Ok(counter.0)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Executable {
    NativeV1,
    WasmAbiV1,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedModule {
    pub id: u64,
    pub manifest: Manifest,
    pub artifact: [u8; 32],
    pub executable: Executable,
    pub dependencies: Vec<u64>,
    /// Original attachment, even when absent from current dispatch.
    pub target: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedGrant {
    pub caller: u64,
    pub slot: usize,
    pub target: u64,
    pub requested: Authority,
    pub active: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedDispatch {
    pub target: u64,
    pub endpoint: String,
    pub extension: u64,
    pub export: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistrySnapshot {
    pub version: u32,
    pub namespaces: BTreeSet<u64>,
    pub limits: Limits,
    pub retention_limits: RetentionLimits,
    pub next_generation: u64,
    pub next_operation: u64,
    pub retention_counters: [u64; 3],
    pub modules: Vec<SavedModule>,
    pub grants: Vec<SavedGrant>,
    pub dispatch: Vec<SavedDispatch>,
}

/// Exact host allowlist material. Native ports MUST be freshly constructed,
/// bounded, effect-free on construction/drop and exclude source service aliases.
/// Those properties are host assumptions, not properties inferred from a digest.
pub enum ArtifactMaterial {
    Native {
        manifest: Manifest,
        identity: Vec<u8>,
        ports: BTreeMap<String, NativePort>,
    },
    Guest {
        manifest: Manifest,
        bytes: Vec<u8>,
    },
}
/// Independent destination policy. Saved IDs and strings carry no authority.
/// Implementations must be pure/bounded and stable through this exclusive call.
pub trait RegistryAuthority {
    fn resolve(&self, module: &SavedModule) -> Result<ArtifactMaterial>;
    /// Checked host binding; arbitrary source strings must be refused.
    fn bind_resource(&self, source: &Resource) -> Result<Resource>;
    /// Approve independently, returning supported restrictions explicitly.
    /// Required authority that cannot be supported must return an error.
    fn approve_grant(
        &self,
        caller: &SavedModule,
        slot: usize,
        target: &SavedModule,
        requested: &Authority,
    ) -> Result<Authority>;
}

impl Kernel {
    /// Exclusive safe boundary; live retention is deliberately refused until D3b.
    pub fn registry_snapshot(&mut self) -> Result<RegistrySnapshot> {
        let (retention_limits, retention_counters) = self.retention.empty_history()?;
        // Bound scratch before copying runtime-owned manifests and authorities.
        let mut scratch = 4096usize;
        for registration in self.registrations.values() {
            scratch = scratch
                .checked_add(bounded_size(&registration.manifest)?)
                .and_then(|n| {
                    registration
                        .dependencies
                        .len()
                        .checked_mul(21)
                        .and_then(|d| n.checked_add(d))
                })
                .and_then(|n| n.checked_add(1024))
                .ok_or(Error::Limit)?;
            if scratch > MAX_BYTES {
                return Err(Error::Limit);
            }
        }
        for grant in self.grants.values() {
            scratch = scratch
                .checked_add(bounded_size(&grant.authority)?)
                .and_then(|n| n.checked_add(256))
                .ok_or(Error::Limit)?;
            if scratch > MAX_BYTES {
                return Err(Error::Limit);
            }
        }
        scratch = self
            .extensions
            .len()
            .checked_mul(512)
            .and_then(|n| scratch.checked_add(n))
            .and_then(|n| {
                self.historical_instances
                    .len()
                    .checked_mul(21)
                    .and_then(|h| n.checked_add(h))
            })
            .ok_or(Error::Limit)?;
        if scratch > MAX_BYTES {
            return Err(Error::Limit);
        }
        let mut namespaces = self.historical_instances.clone();
        namespaces.insert(self.instance);
        let snapshot = RegistrySnapshot {
            version: 1,
            namespaces,
            limits: self.limits.clone(),
            retention_limits,
            next_generation: self.next_generation,
            next_operation: self.next_operation,
            retention_counters,
            modules: self
                .registrations
                .iter()
                .map(|(h, r)| SavedModule {
                    id: h.generation,
                    manifest: r.manifest.clone(),
                    artifact: r.artifact,
                    executable: match r.code {
                        Code::Native(_) => Executable::NativeV1,
                        Code::Guest(_) => Executable::WasmAbiV1,
                    },
                    dependencies: r.dependencies.iter().map(|h| h.generation).collect(),
                    target: r.attachment.as_ref().map(|(h, _, _)| h.generation),
                })
                .collect(),
            grants: self
                .grants
                .iter()
                .map(|((h, slot), g)| SavedGrant {
                    caller: h.generation,
                    slot: *slot,
                    target: g.target.generation,
                    requested: g.authority.clone(),
                    active: g.active,
                })
                .collect(),
            dispatch: self
                .extensions
                .iter()
                .map(|((h, ep), (ext, export))| SavedDispatch {
                    target: h.generation,
                    endpoint: ep.clone(),
                    extension: ext.generation,
                    export: export.clone(),
                })
                .collect(),
        };
        snapshot.preflight(self)?;
        // Legacy unregister can leave dangling incarnation bindings. Never turn
        // those generation numbers into a different installed module on restore.
        for r in self.registrations.values() {
            for h in &r.dependencies {
                self.registration(*h)?;
            }
            if let Some((h, _, _)) = &r.attachment {
                self.registration(*h)?;
            }
        }
        for g in self.grants.values() {
            self.registration(g.target)?;
        }
        Ok(snapshot)
    }

    /// Build completely hidden, then exchange. On refusal the populated
    /// destination and its credentials/counters are unchanged. No code runs.
    pub fn restore_registry(
        &mut self,
        saved: &RegistrySnapshot,
        host: &impl RegistryAuthority,
    ) -> Result<BTreeMap<u64, ModuleHandle>> {
        self.retention.empty_history()?;
        saved.preflight(self)?;
        let mut hidden = Kernel::new(self.limits.clone())?;
        // Works in a new process as well: every source/historical namespace is
        // explicitly excluded, with finite skips and checked allocator exhaustion.
        for _ in 0..=MAX_NAMESPACES {
            if !saved.namespaces.contains(&hidden.instance)
                && hidden.instance != self.instance
                && !self.historical_instances.contains(&hidden.instance)
            {
                break;
            }
            hidden.instance = NEXT_INSTANCE
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
                .map_err(|_| Error::Limit)?;
        }
        if saved.namespaces.contains(&hidden.instance)
            || hidden.instance == self.instance
            || self.historical_instances.contains(&hidden.instance)
        {
            return Err(Error::Limit);
        }
        hidden.historical_instances = saved
            .namespaces
            .union(&self.historical_instances)
            .copied()
            .collect();
        hidden.historical_instances.insert(self.instance);
        if hidden.historical_instances.len() >= MAX_NAMESPACES {
            return Err(Error::Limit);
        }
        let rows: BTreeMap<_, _> = saved.modules.iter().map(|r| (r.id, r)).collect();
        let mut handles = BTreeMap::new();
        for row in &saved.modules {
            hidden.next_generation = row.id;
            let handle = match (row.executable, host.resolve(row)?) {
                (
                    Executable::NativeV1,
                    ArtifactMaterial::Native {
                        manifest,
                        identity,
                        ports,
                    },
                ) if manifest == row.manifest => {
                    hidden.register_native(manifest, &identity, ports)?
                }
                (Executable::WasmAbiV1, ArtifactMaterial::Guest { manifest, bytes })
                    if manifest == row.manifest =>
                {
                    hidden.register_guest(manifest, &bytes)?
                }
                _ => return Err(Error::InvalidModule),
            };
            if hidden.artifact(handle)? != row.artifact {
                return Err(Error::InvalidModule);
            }
            let reg = hidden.registration(handle)?;
            if reg
                .dependencies
                .iter()
                .map(|h| h.generation)
                .collect::<Vec<_>>()
                != row.dependencies
                || reg.attachment.as_ref().map(|(h, _, _)| h.generation) != row.target
            {
                return Err(Error::MissingDependency);
            }
            // Multiple old detached extensions are legal. Rebuild original
            // attachment evidence first; install only saved CURRENT dispatch later.
            hidden.extensions.clear();
            handles.insert(row.id, handle);
        }
        for edge in &saved.dispatch {
            hidden.extensions.insert(
                (handles[&edge.target], edge.endpoint.clone()),
                (handles[&edge.extension], edge.export.clone()),
            );
        }
        for grant in &saved.grants {
            let requested = match &grant.requested.scope {
                Scope::Keys(keys) => {
                    let mapped = keys
                        .iter()
                        .map(|r| host.bind_resource(r))
                        .collect::<Result<BTreeSet<_>>>()?;
                    if mapped.len() != keys.len() {
                        return Err(Error::Authority);
                    }
                    Authority {
                        access: grant.requested.access,
                        scope: Scope::Keys(mapped),
                    }
                }
                Scope::All => return Err(Error::Authority),
            };
            hidden.validate_authority(&requested, true)?;
            let approved = if grant.active {
                host.approve_grant(
                    rows[&grant.caller],
                    grant.slot,
                    rows[&grant.target],
                    &requested,
                )?
            } else {
                requested.clone()
            };
            if !requested.contains(&approved) {
                return Err(Error::Authority);
            }
            hidden.issue_grant(handles[&grant.caller], grant.slot, approved)?;
            if !grant.active {
                hidden.revoke_grant(handles[&grant.caller], grant.slot)?;
            }
        }
        hidden.next_generation = saved.next_generation;
        hidden.next_operation = saved.next_operation;
        hidden.set_retention_limits(saved.retention_limits.clone())?;
        hidden
            .retention
            .restore_empty_history(saved.retention_limits.clone(), saved.retention_counters)?;
        *self = hidden;
        Ok(handles)
    }
}

impl RegistrySnapshot {
    fn preflight(&self, destination: &Kernel) -> Result<()> {
        let (retention_profile, _) = destination.retention.empty_history()?;
        if self.retention_limits.leases > retention_profile.leases
            || self.retention_limits.owners > retention_profile.owners
            || self.retention_limits.nodes > retention_profile.nodes
            || self.retention_limits.slots > retention_profile.slots
            || self.retention_limits.references > retention_profile.references
        {
            return Err(Error::Limit);
        }
        if self.version != 1
            || self.namespaces.is_empty()
            || self.namespaces.contains(&0)
            || self.namespaces.len() >= MAX_NAMESPACES
            || self.next_generation == 0
            || self.next_operation == 0
            || self.retention_counters.contains(&0)
            || self.modules.len() > destination.limits.registrations.min(MAX_ROWS)
            || self.grants.len() > MAX_EDGES
            || self.dispatch.len() > MAX_EDGES
            || serde_json::to_value(&self.limits).map_err(|_| Error::Limit)?
                != serde_json::to_value(&destination.limits).map_err(|_| Error::Limit)?
        {
            return Err(Error::Limit);
        }
        let rows: BTreeMap<_, _> = self.modules.iter().map(|r| (r.id, r)).collect();
        let mut ids = BTreeMap::new();
        let mut previous = 0;
        let mut edges = self
            .grants
            .len()
            .checked_add(self.dispatch.len())
            .ok_or(Error::Limit)?;
        for row in &self.modules {
            if row.id <= previous
                || row.id >= self.next_generation
                || !Kernel::valid_name(&row.manifest.id)
                || row.manifest.exports.is_empty()
                || row.manifest.exports.len() > destination.limits.manifest_entries
                || row.manifest.dependencies.len() > destination.limits.manifest_entries
                || row.manifest.extension_points.len() > destination.limits.manifest_entries
                || row.dependencies.len() != row.manifest.dependencies.len()
            {
                return Err(Error::InvalidManifest);
            }
            previous = row.id;
            if ids.insert(&row.manifest.id, row.id).is_some() {
                return Err(Error::Duplicate);
            }
            for (name, ep) in &row.manifest.exports {
                if !Kernel::valid_name(name)
                    || name == "memory"
                    || name == "alloc"
                    || ep.major == 0
                    || ep.max_request > destination.limits.payload_bytes
                    || ep.max_reply > destination.limits.payload_bytes
                {
                    return Err(Error::InvalidManifest);
                }
            }
            for point in &row.manifest.extension_points {
                if !row.manifest.exports.contains_key(point) {
                    return Err(Error::InvalidManifest);
                }
            }
            for (dep, id) in row.manifest.dependencies.iter().zip(&row.dependencies) {
                if !Kernel::valid_name(&dep.capability.provider)
                    || !Kernel::valid_name(&dep.capability.endpoint)
                {
                    return Err(Error::InvalidManifest);
                }
                let target = rows.get(id).ok_or(Error::MissingDependency)?;
                if *id > row.id
                    || target.manifest.id != dep.capability.provider
                    || ids.get(&dep.capability.provider) != Some(id)
                    || target
                        .manifest
                        .exports
                        .get(&dep.capability.endpoint)
                        .map(|e| e.major)
                        != Some(dep.capability.major)
                {
                    return Err(Error::MissingDependency);
                }
            }
            match (&row.manifest.extension, row.target) {
                (None, None) => (),
                (Some(ext), Some(id)) => {
                    let target = rows.get(&id).ok_or(Error::MissingDependency)?;
                    if id >= row.id
                        || ids.get(&ext.target.provider) != Some(&id)
                        || target.manifest.id != ext.target.provider
                        || !target
                            .manifest
                            .extension_points
                            .contains(&ext.target.endpoint)
                        || target
                            .manifest
                            .exports
                            .get(&ext.target.endpoint)
                            .map(|e| e.major)
                            != Some(ext.target.major)
                        || !Kernel::valid_name(&ext.export)
                        || row.manifest.exports.get(&ext.export)
                            != target.manifest.exports.get(&ext.target.endpoint)
                    {
                        return Err(Error::Signature);
                    }
                }
                _ => return Err(Error::InvalidManifest),
            }
            edges = edges
                .checked_add(row.dependencies.len())
                .and_then(|n| n.checked_add(usize::from(row.target.is_some())))
                .ok_or(Error::Limit)?;
            if edges > MAX_EDGES {
                return Err(Error::Limit);
            }
        }
        let mut dispatch = BTreeSet::new();
        for edge in &self.dispatch {
            let ext = rows.get(&edge.extension).ok_or(Error::MissingDependency)?;
            let attachment = ext
                .manifest
                .extension
                .as_ref()
                .ok_or(Error::InvalidManifest)?;
            if ext.target != Some(edge.target)
                || attachment.target.endpoint != edge.endpoint
                || attachment.export != edge.export
                || !dispatch.insert((edge.target, &edge.endpoint))
            {
                return Err(Error::Duplicate);
            }
        }
        let mut grants = BTreeSet::new();
        for grant in &self.grants {
            let caller = rows.get(&grant.caller).ok_or(Error::MissingDependency)?;
            let dep = caller
                .manifest
                .dependencies
                .get(grant.slot)
                .ok_or(Error::InvalidSlot)?;
            if caller.dependencies[grant.slot] != grant.target
                || grant.requested.access > dep.access
                || !grants.insert((grant.caller, grant.slot))
            {
                return Err(Error::Authority);
            }
            destination.validate_authority(&grant.requested, true)?;
        }
        if self.retention_limits.leases == 0
            || self.retention_limits.owners == 0
            || self.retention_limits.nodes == 0
            || self.retention_limits.slots == 0
            || self.retention_limits.references == 0
        {
            return Err(Error::Limit);
        }
        // Records are local, not a second wire protocol. D4 must bound its parser
        // before allocation; here all constituent collections/strings are bounded
        // before measuring the local aggregate representation.
        bounded_size(self)?;
        Ok(())
    }
}
