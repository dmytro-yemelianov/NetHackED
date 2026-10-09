//! Sealed local ledger reconstruction. D4 owns whole-service activation and the
//! authority epoch; this builder alone must never replace a live HostBundle.
use super::*;
use nethacked_kernel::persistence::{
    ExecutionCaptureIndex, InputHistory, KernelSnapshot, RestoredKernel, SavedOperation,
};

const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_ROWS: usize = 65_536;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    module: u64,
    artifact: [u8; 32],
    endpoint: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Permit {
    capability: Capability,
    resolution: Vec<Binding>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    serial: u64,
    #[serde(default, skip_serializing_if = "is_live")]
    historical: bool,
    ordinal: u64,
    operation: SavedOperation,
    caller: u64,
    caller_artifact: [u8; 32],
    permit: Permit,
    request: Vec<u8>,
    value: Vec<u8>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservationRow {
    serial: u64,
    pin: u64,
}
/// Wire rows are lookup keys and requests, never observation credentials.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    version: u32,
    namespace: u64,
    origins: BTreeSet<u64>,
    #[serde(default, skip_serializing_if = "is_zero")]
    namespace_high_water: u64,
    kernel_namespace: u64,
    policy: String,
    policy_version: u32,
    provider: Binding,
    capability: Capability,
    resource: Resource,
    source: Vec<u8>,
    records: Vec<Row>,
    next_serial: u64,
    next_ordinals: BTreeMap<u64, u64>,
    observations: Vec<ObservationRow>,
}
impl Checkpoint {
    pub fn count_references(&self, n: &mut nethacked_kernel::persistence::ReferenceCounter) {
        n.add(3); // provider, resource and policy
        for r in &self.records {
            n.operation(&r.operation);
            n.add(1);
            n.add(r.permit.resolution.len());
        }
        n.add(self.next_ordinals.len());
        for _ in &self.observations {
            n.add(2);
        } // record and pin
    }
    pub fn to_bytes(&self, limit: usize) -> Result<Vec<u8>> {
        // Count before allocating the output. Input-owned data never exceeds the
        // local envelope even if a host supplied permissive limits.
        struct Counter {
            size: usize,
            limit: usize,
        }
        impl std::io::Write for Counter {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.size = self
                    .size
                    .checked_add(b.len())
                    .filter(|n| *n <= self.limit)
                    .ok_or_else(|| std::io::Error::other("input checkpoint limit"))?;
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut counter = Counter {
            size: 0,
            limit: limit.min(MAX_BYTES),
        };
        serde_json::to_writer(&mut counter, self).map_err(|_| Error::Limit)?;
        let value = serde_json::to_value(self).map_err(|_| Error::Buffer)?;
        serde_json::to_vec(&value).map_err(|_| Error::Buffer)
    }
    pub fn from_bytes(bytes: &[u8], limit: usize) -> Result<Self> {
        if bytes.len() > limit.min(MAX_BYTES) {
            return Err(Error::Limit);
        }
        let c: Self = serde_json::from_slice(bytes).map_err(|_| Error::NativeRefusal)?;
        if c.to_bytes(limit)? != bytes {
            return Err(Error::NativeRefusal);
        }
        Ok(c)
    }
    /// Structural preflight for composing sealed local checkpoints.
    pub fn validate_checkpoint(&self, limits: &Limits, kernel: &KernelSnapshot) -> Result<()> {
        self.validate(limits, kernel)
    }
    fn validate(&self, limits: &Limits, kernel: &KernelSnapshot) -> Result<()> {
        self.to_bytes(MAX_BYTES)?;
        kernel.validate_execution_metadata()?;
        if self.version != 1
            || self.namespace == 0
            || self.kernel_namespace == 0
            || self.origins.is_empty()
            || self.origins.len() >= MAX_ROWS
            || self.origins.contains(&0)
            || !self.origins.contains(&self.namespace)
            || self.origins.contains(&u64::MAX)
            || self.namespace_high_water == u64::MAX
            || self.policy.is_empty()
            || self.policy.len() > limits.wire as usize
            || self.provider.endpoint != ENDPOINT
            || self.capability.endpoint != ENDPOINT
            || self.capability.major != MAJOR
            || self.resource.key != "source"
            || self.source.len() > limits.source
            || self.next_serial == 0
            || self.records.len() > limits.records.min(MAX_ROWS)
            || self.next_ordinals.len() > limits.operations.min(MAX_ROWS)
            || self.observations.len() > limits.records.min(MAX_ROWS)
        {
            return Err(Error::Limit);
        }
        if !kernel.execution_resource_declared(&self.resource)? {
            return Err(Error::Authority);
        }
        if kernel.source_namespace != self.kernel_namespace {
            return Err(Error::Authority);
        }
        let provider = kernel
            .registry
            .modules
            .iter()
            .find(|m| m.id == self.provider.module)
            .ok_or(Error::InvalidHandle)?;
        if provider.artifact != self.provider.artifact
            || provider.manifest.id != self.capability.provider
            || provider.manifest.id != self.resource.owner
            || !provider
                .manifest
                .exports
                .get(ENDPOINT)
                .is_some_and(|e| e.major == MAJOR && e.access == Access::Write)
        {
            return Err(Error::Authority);
        }
        let mut previous = 0;
        let mut ordinals = BTreeMap::new();
        for r in &self.records {
            if r.serial <= previous
                || r.serial >= self.next_serial
                || r.request.len() > limits.request
                || r.value.len() > limits.value
                || r.permit.capability != self.capability
                || r.permit.resolution.is_empty()
                || r.permit.resolution.len() > MAX_ROWS
            {
                return Err(Error::NativeRefusal);
            }
            previous = r.serial;
            let ordinal = ordinals.entry(r.operation.operation).or_insert(0u64);
            if r.ordinal != *ordinal {
                return Err(Error::NativeRefusal);
            }
            *ordinal = ordinal.checked_add(1).ok_or(Error::Limit)?;
            if r.historical {
                history(r).check(kernel)?;
                if kernel.leases.iter().any(|l| l.id == r.operation.pipeline)
                    || kernel
                        .operations
                        .iter()
                        .any(|o| o.pipeline == r.operation.pipeline)
                {
                    return Err(Error::Authority);
                }
                let first = &r.permit.resolution[0];
                if first.module != self.provider.module
                    || first.artifact != self.provider.artifact
                    || first.endpoint != ENDPOINT
                {
                    return Err(Error::Authority);
                }
                continue;
            }
            let caller = kernel
                .registry
                .modules
                .iter()
                .find(|m| m.id == r.caller)
                .ok_or(Error::InvalidHandle)?;
            if caller.artifact != r.caller_artifact {
                return Err(Error::Authority);
            }
            let first = &r.permit.resolution[0];
            if first.module != self.provider.module
                || first.artifact != self.provider.artifact
                || first.endpoint != ENDPOINT
            {
                return Err(Error::Authority);
            }
            let chain = r
                .permit
                .resolution
                .iter()
                .map(|b| {
                    let module = kernel
                        .registry
                        .modules
                        .iter()
                        .find(|m| m.id == b.module)
                        .ok_or(Error::InvalidHandle)?;
                    if module.artifact != b.artifact
                        || !module.manifest.exports.contains_key(&b.endpoint)
                    {
                        return Err(Error::Authority);
                    }
                    Ok(nethacked_kernel::persistence::SavedBinding {
                        module: b.module,
                        artifact: b.artifact,
                        endpoint: b.endpoint.clone(),
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            kernel.execution_record(
                &r.operation,
                r.caller,
                &self.capability,
                &chain,
                &self.resource,
                None,
            )?;
        }
        // This service has no record deletion: exact counters, not just lower bounds.
        if self.next_serial != previous.checked_add(1).ok_or(Error::Limit)?
            || self.next_ordinals != ordinals
        {
            return Err(Error::NativeRefusal);
        }
        let mut seen = BTreeSet::new();
        for o in &self.observations {
            if !seen.insert((o.serial, o.pin)) {
                return Err(Error::Duplicate);
            }
            let r = self
                .records
                .iter()
                .find(|r| r.serial == o.serial)
                .ok_or(Error::Authority)?;
            if r.historical {
                return Err(Error::Authority);
            }
            let chain = r
                .permit
                .resolution
                .iter()
                .map(|b| nethacked_kernel::persistence::SavedBinding {
                    module: b.module,
                    artifact: b.artifact,
                    endpoint: b.endpoint.clone(),
                })
                .collect::<Vec<_>>();
            kernel.execution_record(
                &r.operation,
                r.caller,
                &self.capability,
                &chain,
                &self.resource,
                Some(o.pin),
            )?;
        }
        Ok(())
    }
}
/// Index into an input-owned checkpoint, not a raw observation constructor.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ObservationId(usize);

impl Service {
    /// Enroll these exact accepted operation contexts before kernel capture.
    pub fn persistence_operations(&self) -> Result<Vec<OperationBinding>> {
        self.session()?.check_live().map_err(|_| Error::Authority)?;
        let inner = self.0.try_borrow().map_err(|_| Error::NativeRefusal)?;
        if inner.quarantine {
            return Err(Error::Authority);
        }
        if inner.state.records.len() > MAX_ROWS {
            return Err(Error::Limit);
        }
        let mut ops = vec![];
        for r in &inner.state.records {
            if !ops.contains(&r.operation) {
                ops.push(r.operation.clone());
            }
        }
        Ok(ops)
    }
    /// Enroll only surviving sources; disposed receipts remain diagnostic history.
    pub fn persistence_operations_for(&self, kernel: &Kernel) -> Result<Vec<OperationBinding>> {
        self.session()?.check_live().map_err(|_| Error::Authority)?;
        Ok(self
            .persistence_operations()?
            .into_iter()
            .filter(|op| kernel.execution_source_live(op).unwrap_or(false))
            .collect())
    }
    /// Preserve immutable cleanup identity roots even after source disposal.
    pub fn persistence_cleanups(&self) -> Result<Vec<nethacked_kernel::CleanupIdentity>> {
        self.session()?.check_live().map_err(|_| Error::Authority)?;
        let inner = self.0.try_borrow().map_err(|_| Error::NativeRefusal)?;
        if inner.quarantine {
            return Err(Error::Authority);
        }
        Ok(inner
            .state
            .records
            .iter()
            .flat_map(|r| r.operation.cleanup_identities().iter().copied())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect())
    }
    /// Resource roots must also be captured for empty/source-only services.
    pub fn persistence_resources(&self) -> Result<Vec<Resource>> {
        self.session()?.check_live().map_err(|_| Error::Authority)?;
        let inner = self.0.try_borrow().map_err(|_| Error::NativeRefusal)?;
        if inner.quarantine {
            return Err(Error::Authority);
        }
        Ok(vec![inner
            .binding
            .as_ref()
            .ok_or(Error::Authority)?
            .3
            .clone()])
    }
    /// Capture accepted ledger and explicitly enrolled observation reservations.
    pub fn capture_persistence(
        &self,
        index: &ExecutionCaptureIndex<'_>,
        observations: &[&Observation],
    ) -> Result<(Checkpoint, Vec<ObservationId>)> {
        self.session()?.capture_ready().map_err(|_| Error::Busy)?;
        let inner = self.0.try_borrow().map_err(|_| Error::NativeRefusal)?;
        if inner.quarantine {
            return Err(Error::Authority);
        }
        if observations.len() > MAX_ROWS
            || inner.state.records.len() > MAX_ROWS
            || inner.state.next_ordinals.len() > MAX_ROWS
            || inner.persistence_origins.len() > MAX_ROWS
        {
            return Err(Error::Limit);
        }
        let mut size = inner.state.source.len();
        for r in &inner.state.records {
            size = size
                .checked_add(r.request.len())
                .and_then(|n| n.checked_add(r.value.len()))
                .ok_or(Error::Limit)?;
            if r.permit.resolution.len() > MAX_ROWS {
                return Err(Error::Limit);
            }
            for b in &r.permit.resolution {
                size = size.checked_add(b.endpoint.len()).ok_or(Error::Limit)?;
            }
        }
        if size > MAX_BYTES {
            return Err(Error::Limit);
        }
        let (provider, artifact, capability, resource) =
            inner.binding.as_ref().ok_or(Error::Authority)?;
        let mut c = Checkpoint {
            version: 1,
            namespace: inner.namespace,
            kernel_namespace: index.namespace(),
            origins: inner.persistence_origins.clone(),
            namespace_high_water: NEXT_SERVICE.load(Ordering::Relaxed) - 1,
            policy: inner.policy.identity.clone(),
            policy_version: inner.policy.version,
            provider: Binding {
                module: index.module(*provider)?,
                artifact: *artifact,
                endpoint: ENDPOINT.into(),
            },
            capability: capability.clone(),
            resource: resource.clone(),
            source: inner.state.source.clone(),
            records: inner
                .state
                .records
                .iter()
                .map(|r| {
                    let historical = match index.source(&r.operation) {
                        Ok(_) => false,
                        Err(e) if index.input_source_is_live(&r.operation) => return Err(e),
                        Err(_) => true,
                    };
                    let h = index.input_history(
                        &r.operation,
                        r.caller,
                        r.caller_artifact,
                        &r.permit.resolution,
                    )?;
                    Ok(Row {
                        historical,
                        serial: r.id.serial,
                        ordinal: r.ordinal,
                        operation: h.source,
                        caller: h.caller,
                        caller_artifact: r.caller_artifact,
                        permit: Permit {
                            capability: r.permit.capability.clone(),
                            resolution: h
                                .resolution
                                .into_iter()
                                .map(|b| Binding {
                                    module: b.module,
                                    artifact: b.artifact,
                                    endpoint: b.endpoint,
                                })
                                .collect(),
                        },
                        request: r.request.clone(),
                        value: r.value.clone(),
                    })
                })
                .collect::<Result<_>>()?,
            next_serial: inner.state.next_serial,
            next_ordinals: inner
                .state
                .records
                .iter()
                .map(|r| {
                    Ok((
                        index
                            .input_history(
                                &r.operation,
                                r.caller,
                                r.caller_artifact,
                                &r.permit.resolution,
                            )?
                            .source
                            .operation,
                        *inner
                            .state
                            .next_ordinals
                            .get(&r.operation.operation())
                            .ok_or(Error::Authority)?,
                    ))
                })
                .collect::<Result<_>>()?,
            observations: vec![],
        };
        let mut ids = vec![];
        for o in observations {
            if !Rc::ptr_eq(&o.service.0, &self.0) || !inner.state.records.contains(&o.record) {
                return Err(Error::Authority);
            }
            let row = ObservationRow {
                serial: o.record.id.serial,
                pin: index.pin(&o._code)?,
            };
            let id = c
                .observations
                .iter()
                .position(|r| r.serial == row.serial && r.pin == row.pin)
                .unwrap_or_else(|| {
                    c.observations.push(row);
                    c.observations.len() - 1
                });
            ids.push(ObservationId(id));
        }
        c.to_bytes(MAX_BYTES)?;
        Ok((c, ids))
    }
}
type Factory = Box<dyn Fn() -> Result<Policy>>;
/// Policy identity is only a selector into this independently supplied allowlist.
/// Factories must be fresh, pure, bounded, effect-free on drop, and alias-free.
#[derive(Default)]
pub struct Catalog {
    entries: BTreeMap<(String, u32), Factory>,
}
impl Catalog {
    pub fn missing_dependency(&self, c: &Checkpoint) -> Option<String> {
        (!self
            .entries
            .contains_key(&(c.policy.clone(), c.policy_version)))
        .then(|| {
            format!(
                "input policy: {:?}; version: {}",
                c.policy, c.policy_version
            )
        })
    }
    pub fn allow(
        &mut self,
        identity: String,
        version: u32,
        factory: impl Fn() -> Result<Policy> + 'static,
    ) -> Result<()> {
        if identity.is_empty() || identity.len() > MAX_BYTES || self.entries.len() >= MAX_ROWS {
            return Err(Error::Limit);
        }
        let key = (identity, version);
        if self.entries.contains_key(&key) {
            return Err(Error::Duplicate);
        }
        self.entries.insert(key, Box::new(factory));
        Ok(())
    }
}
/// A port may be given to an approved native kernel factory, but remains denied
/// until finish validates every record and observation against admitted mappings.
pub struct Restore {
    service: Service,
    checkpoint: Checkpoint,
}
pub struct Restored {
    destination: nethacked_kernel::persistence::ExecutionDestination,
    checkpoint: Checkpoint,
    service: Service,
    observations: Vec<Observation>,
    records: BTreeMap<u64, RecordId>,
}
impl Restore {
    pub fn prepare(
        c: Checkpoint,
        limits: Limits,
        kernel: &KernelSnapshot,
        catalog: &Catalog,
    ) -> Result<Self> {
        c.validate(&limits, kernel)?;
        let factory = catalog
            .entries
            .get(&(c.policy.clone(), c.policy_version))
            .ok_or(Error::Authority)?;
        let policy = factory()?;
        if policy.identity != c.policy || policy.version != c.policy_version {
            return Err(Error::Authority);
        }
        let retired = c
            .namespace_high_water
            .max(*c.origins.last().ok_or(Error::Limit)?);
        let service = Service::new_after(limits, policy, c.source.clone(), retired)?;
        {
            let mut inner = service.0.borrow_mut();
            inner.persistence_origins = c.origins.clone();
            if inner.persistence_origins.len() >= MAX_ROWS {
                return Err(Error::Limit);
            }
            let namespace = inner.namespace;
            inner.persistence_origins.insert(namespace);
            inner.quarantine = true;
        }
        Ok(Self {
            service,
            checkpoint: c,
        })
    }
    pub fn port(&self) -> NativePort {
        let service = self.service.clone();
        Rc::new(move |bytes, view| service.dispatch(bytes, view))
    }
    pub fn finish(self, kernel: &Kernel, mapped: &RestoredKernel) -> Result<Restored> {
        mapped.execution_live(kernel)?;
        let c = &self.checkpoint;
        if mapped.execution_namespace() != c.kernel_namespace {
            return Err(Error::Authority);
        }
        let module = |id: u64, artifact: [u8; 32]| -> Result<ModuleHandle> {
            let m = *mapped.modules().get(&id).ok_or(Error::InvalidHandle)?;
            if kernel.artifact(m)? != artifact {
                return Err(Error::Authority);
            }
            Ok(m)
        };
        let provider = module(c.provider.module, c.provider.artifact)?;
        let resource = mapped.execution_resource(&c.resource)?.clone();
        let namespace = self.service.0.borrow().namespace;
        let mut state = State {
            source: c.source.clone(),
            records: vec![],
            next_serial: c.next_serial,
            next_ordinals: BTreeMap::new(),
        };
        let mut records = BTreeMap::new();
        for r in &c.records {
            let operation = if r.historical {
                mapped.terminal_input_source(&history(r))?
            } else {
                mapped.execution_source(&r.operation)?
            };
            state.next_ordinals.insert(
                operation.operation(),
                c.next_ordinals[&r.operation.operation],
            );
            let chain = r
                .permit
                .resolution
                .iter()
                .map(|b| {
                    Ok(nethacked_kernel::ProviderBinding {
                        handle: if r.historical {
                            ModuleHandle::inert_history(b.module)
                        } else {
                            module(b.module, b.artifact)?
                        },
                        artifact: b.artifact,
                        endpoint: b.endpoint.clone(),
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            if !r.historical {
                mapped.execution_record(
                    &r.operation,
                    r.caller,
                    &c.capability,
                    &chain,
                    &c.resource,
                    None,
                )?;
            }
            let id = RecordId {
                namespace,
                serial: r.serial,
            };
            records.insert(r.serial, id);
            state.records.push(Record {
                id,
                ordinal: r.ordinal,
                operation,
                caller: if r.historical {
                    ModuleHandle::inert_history(r.caller)
                } else {
                    module(r.caller, r.caller_artifact)?
                },
                caller_artifact: r.caller_artifact,
                provider: if r.historical {
                    ModuleHandle::inert_history(c.provider.module)
                } else {
                    provider
                },
                provider_artifact: c.provider.artifact,
                permit: CapabilityPermit {
                    capability: c.capability.clone(),
                    resolution: chain,
                },
                request: r.request.clone(),
                value: r.value.clone(),
            });
        }
        let mut observations = vec![];
        for o in &c.observations {
            let r = state
                .records
                .iter()
                .find(|r| r.id.serial == o.serial)
                .ok_or(Error::Authority)?;
            let saved = c
                .records
                .iter()
                .find(|r| r.serial == o.serial)
                .ok_or(Error::Authority)?;
            mapped.execution_record(
                &saved.operation,
                saved.caller,
                &c.capability,
                &r.permit.resolution,
                &c.resource,
                Some(o.pin),
            )?;
            let pin = mapped.pins().get(&o.pin).ok_or(Error::Authority)?.clone();
            observations.push(Observation {
                service: self.service.clone(),
                record: r.clone(),
                _code: pin,
                identity: ObservationIdentity {
                    policy: c.policy.clone(),
                    version: c.policy_version,
                    record: r.id,
                    provider: nethacked_kernel::ProviderBinding {
                        handle: provider,
                        artifact: c.provider.artifact,
                        endpoint: ENDPOINT.into(),
                    },
                    request: r.request.clone(),
                    operation: r.operation.clone(),
                    caller: r.caller,
                    caller_artifact: r.caller_artifact,
                    permit: r.permit.clone(),
                },
            });
        }
        {
            let mut inner = self
                .service
                .0
                .try_borrow_mut()
                .map_err(|_| Error::NativeRefusal)?;
            inner.state = state;
            inner.binding = Some((
                provider,
                c.provider.artifact,
                c.capability.clone(),
                resource,
            ));
            inner.quarantine = false;
        }
        Ok(Restored {
            destination: mapped.execution_destination(),
            checkpoint: self.checkpoint.clone(),
            service: self.service,
            observations,
            records,
        })
    }
}
impl Restored {
    /// Finish authenticated the service, records, operations, routes and pins
    /// against this sealed destination. Equal saved bytes cannot replace it.
    pub fn matches_destination(
        &self,
        destination: &nethacked_kernel::persistence::ExecutionDestination,
    ) -> bool {
        self.destination.same_destination(destination)
    }
    /// Sealed correlation for a composing owner restore; no raw credentials.
    pub fn matches_checkpoint(&self, checkpoint: &Checkpoint) -> Result<bool> {
        Ok(self.checkpoint.to_bytes(MAX_BYTES)? == checkpoint.to_bytes(MAX_BYTES)?)
    }
    pub fn service(&self) -> &Service {
        &self.service
    }
    pub fn observation(&self, id: ObservationId) -> Result<Observation> {
        self.observations.get(id.0).cloned().ok_or(Error::Authority)
    }
    pub fn record(&self, old_serial: u64) -> Result<RecordId> {
        self.records
            .get(&old_serial)
            .copied()
            .ok_or(Error::Authority)
    }
}

#[cfg(test)]
#[path = "../tests/support/persistence.rs"]
mod tests;
/// Metadata correlation request, never an observation credential or constructor.
pub struct ObservationBinding<'a> {
    pub policy: &'a str,
    pub version: u32,
    pub provider: u64,
    pub provider_artifact: [u8; 32],
    pub caller: u64,
    pub caller_artifact: [u8; 32],
    pub operation: &'a SavedOperation,
    pub request: &'a [u8],
}
impl Checkpoint {
    pub fn contains_observation(&self, id: ObservationId) -> bool {
        id.0 < self.observations.len()
    }
    /// Check the declaration and exact accepted prefix before composing codecs
    /// or native factories. Returned size is the owner observation reservation.
    pub fn observation_binding(
        &self,
        id: ObservationId,
        expected: &ObservationBinding<'_>,
    ) -> Result<usize> {
        let o = self.observations.get(id.0).ok_or(Error::Authority)?;
        let r = self
            .records
            .iter()
            .find(|r| r.serial == o.serial)
            .ok_or(Error::Authority)?;
        if expected.policy != self.policy
            || expected.version != self.policy_version
            || expected.provider != self.provider.module
            || expected.provider_artifact != self.provider.artifact
            || expected.caller != r.caller
            || expected.caller_artifact != r.caller_artifact
            || expected.request != r.request
            || expected.operation.operation != r.operation.operation
            || expected.operation.pipeline != r.operation.pipeline
            || expected.operation.checkpoint < r.operation.checkpoint
            || (expected.operation.checkpoint == r.operation.checkpoint
                && expected.operation.phase != r.operation.phase)
        {
            return Err(Error::Authority);
        }
        r.value
            .len()
            .checked_add(r.request.len())
            .and_then(|n| n.checked_add(self.policy.len()))
            .ok_or(Error::Limit)
    }
}

fn history(r: &Row) -> InputHistory {
    InputHistory {
        source: r.operation.clone(),
        caller: r.caller,
        caller_artifact: r.caller_artifact,
        resolution: r
            .permit
            .resolution
            .iter()
            .map(|b| nethacked_kernel::persistence::SavedBinding {
                module: b.module,
                artifact: b.artifact,
                endpoint: b.endpoint.clone(),
            })
            .collect(),
    }
}

#[cfg(test)]
#[path = "../tests/support/history.rs"]
mod history_tests;

fn is_live(historical: &bool) -> bool {
    !*historical
}

fn is_zero(value: &u64) -> bool {
    *value == 0
}

impl Checkpoint {
    pub fn native_binding(&self, kernel: &KernelSnapshot) -> Result<(String, String)> {
        let module = kernel
            .registry
            .modules
            .iter()
            .find(|m| m.id == self.provider.module && m.artifact == self.provider.artifact)
            .ok_or(Error::Authority)?;
        Ok((module.manifest.id.clone(), self.provider.endpoint.clone()))
    }
}
