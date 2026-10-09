//! D3b bounded local graph records beside the private retained state.
//! D4 owns wire parsing and the complete host barrier; these are not credentials.
use super::*;
use crate::kernel::persistence::{
    bounded_size, ArtifactMaterial, RegistryAuthority, RegistrySnapshot, SavedModule, MAX_BYTES,
    MAX_EDGES, MAX_ROWS,
};
use serde::{Deserialize, Serialize};

pub(super) const MAX_CLEANUPS: usize = MAX_ROWS;

/// Externally held values must be explicitly enrolled by the exclusive host.
/// Kernel-owned leases, owner references, live pins and callbacks are automatic.
#[derive(Default)]
pub struct KernelEnrollment<'a> {
    pub operations: &'a [OperationBinding],
    pub pure_captures: &'a [PureCapture],
}

/// Cleanup participants are approved by the host; their fresh identities are
/// issued by the runtime, never returned by an untrusted record or factory.
pub trait RetainedAuthority: RegistryAuthority {
    fn approve_cleanup(&self, saved_identity: u64) -> Result<()>;
    /// Independently authenticate the COMPLETE context against trusted host
    /// state, including historical/disposed sources. Default is refusal.
    /// The request is untrusted: echoing it, checking its digest or comparing
    /// copies from the save is not approval. Policy must be bounded, pure and
    /// stable for this exclusive restore. D4 composes it with service admission.
    fn approve_operation_context(&self, _context: &SavedOperationContext) -> Result<()> {
        Err(Error::Authority)
    }
}

/// Exact source identity and execution context submitted to independent host
/// policy BEFORE artifact construction. Resources and cleanup IDs here are in
/// the source namespace; one shared remap is applied after approval.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SavedOperationContext {
    pub namespace: u64,
    pub operation: SavedOperation,
    pub pipeline: SavedWitness,
    /// For capture provenance: caller/service artifacts, capability, slot,
    /// source context and retained service/source route identity in full.
    pub origin: Option<SavedOrigin>,
    /// The captured destination route as well as its historical source route.
    pub retained: Option<SavedWitness>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedBinding {
    pub module: u64,
    pub artifact: [u8; 32],
    pub endpoint: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedRoute {
    pub module: u64,
    pub endpoint: String,
    pub chain: Vec<SavedBinding>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedSlot {
    pub caller: u64,
    pub slot: usize,
    pub target: u64,
    pub ceiling: Option<Authority>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedRouting {
    pub routes: Vec<SavedRoute>,
    pub slots: Vec<SavedSlot>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedOperation {
    pub operation: u64,
    pub pipeline: u64,
    pub checkpoint: u64,
    pub phase: String,
    pub cleanup: Vec<u64>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedWitness {
    pub root: SavedBinding,
    pub ceiling: Authority,
    pub hash: [u8; 32],
    pub routing: SavedRouting,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedOrigin {
    pub caller: SavedBinding,
    pub service: SavedBinding,
    pub capability: Capability,
    pub slot: usize,
    pub target: u64,
    pub ceiling: Authority,
    /// Historical binding; the source lease may already have been disposed.
    pub source: SavedOperation,
    pub service_route: Vec<SavedBinding>,
    pub source_pipeline: SavedWitness,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedPipeline {
    pub root: SavedBinding,
    pub ceiling: Authority,
    pub hash: [u8; 32],
    pub routing: SavedRouting,
    pub origin: Option<SavedOrigin>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedLease {
    pub id: u64,
    pub pipeline: SavedPipeline,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedReferences {
    pub id: u64,
    pub modules: Vec<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedCallback {
    pub id: u64,
    pub pipeline: SavedPipeline,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedPermit {
    pub capability: Capability,
    pub resolution: Vec<SavedBinding>,
    pub ceiling: Authority,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedPureCapture {
    pub provider: u64,
    pub slot: usize,
    pub ceiling: Authority,
    pub closure: Vec<SavedPermit>,
    pub pin: u64,
    pub routing: SavedRouting,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KernelSnapshot {
    pub version: u32,
    pub source_namespace: u64,
    pub registry: RegistrySnapshot,
    pub leases: Vec<SavedLease>,
    pub owners: Vec<SavedReferences>,
    pub pins: Vec<SavedReferences>,
    pub callbacks: Vec<SavedCallback>,
    /// In enrollment order; multiple phases/checkpoints of an operation are legal.
    pub operations: Vec<SavedOperation>,
    pub pure_captures: Vec<SavedPureCapture>,
    pub cleanup_history: BTreeSet<u64>,
    /// Authentic host service roots whose cleanup has not yet been invoked.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub service_cleanup: BTreeSet<u64>,
    /// Metadata-only roots such as an empty timer queue's address. They grant
    /// no authority; the destination resolver must independently admit them.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub service_resources: BTreeSet<Resource>,
    /// Explicit disposed source lease IDs, disjoint from every live lease/owner.
    pub disposed_sources: BTreeSet<u64>,
}

/// Sealed credentials published only after complete graph admission. Clone a pin
/// or callback from this map to share ONE reservation. Drop this map's RAII
/// handles when adapters have taken ownership, to release its last references.
pub struct RestoredKernel {
    destination: ExecutionDestination,
    modules: BTreeMap<u64, ModuleHandle>,
    leases: BTreeMap<u64, PipelineLease>,
    owners: BTreeMap<u64, OwnerReference>,
    pins: BTreeMap<u64, ModulePin>,
    callbacks: BTreeMap<u64, RetainedCallback>,
    operations: Vec<OperationBinding>,
    pure_captures: Vec<PureCapture>,
    cleanup: BTreeMap<u64, CleanupIdentity>,
    historical_sources: BTreeMap<u64, PipelineLease>,
    source_bindings: Vec<(SavedOperation, OperationBinding)>,
    execution_namespace: u64,
    execution_contexts: Vec<SavedOperationContext>,
    execution_captures: Vec<SavedPureCapture>,
    execution_resources: BTreeMap<Resource, Resource>,
    workflow_enrollment: Rc<RefCell<BTreeSet<u64>>>,
    timer_enrollment: Rc<RefCell<BTreeSet<u64>>>,
    adapter_enrollment: Rc<RefCell<BTreeSet<u64>>>,
}

/// One-use service reconstruction reservation. A failed hidden build releases
/// the reservation; a successful build permanently consumes its enrollment.
pub struct WorkflowEnrollment {
    occupied: Rc<RefCell<BTreeSet<u64>>>,
    operations: Vec<u64>,
    complete: bool,
}
impl WorkflowEnrollment {
    pub fn finish(mut self) {
        self.complete = true;
    }
}
impl Drop for WorkflowEnrollment {
    fn drop(&mut self) {
        if !self.complete {
            let mut occupied = self.occupied.borrow_mut();
            for id in &self.operations {
                occupied.remove(id);
            }
        }
    }
}
/// Unforgeable correlation of credentials admitted by one kernel restore.
/// This identity is never serialized and is not chosen by a caller.
#[derive(Clone)]
pub struct ExecutionDestination(Rc<()>);
impl ExecutionDestination {
    pub fn same_destination(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
impl RestoredKernel {
    /// Reserve only independently admitted exact sources. No raw caller map is
    /// accepted, and overlap/replay refuses before local factories run.
    pub fn enroll_workflows(&self, sources: &[SavedOperation]) -> Result<WorkflowEnrollment> {
        let mut operations = BTreeSet::new();
        for source in sources {
            self.execution_source(source)?;
            if !operations.insert(source.operation) {
                return Err(Error::Duplicate);
            }
        }
        let mut occupied = self
            .workflow_enrollment
            .try_borrow_mut()
            .map_err(|_| Error::NativeRefusal)?;
        if operations.iter().any(|id| occupied.contains(id)) {
            return Err(Error::Duplicate);
        }
        occupied.extend(&operations);
        Ok(WorkflowEnrollment {
            occupied: self.workflow_enrollment.clone(),
            operations: operations.into_iter().collect(),
            complete: false,
        })
    }
    /// Explicit local-state reference transform. Include the fresh namespace:
    /// restored module generations may retain their kernel-owned serials.
    pub fn execution_module_identity(&self, id: u64) -> Result<(u64, u64)> {
        let module = self.modules.get(&id).ok_or(Error::InvalidHandle)?;
        Ok((module.instance, module.generation))
    }
    pub fn execution_cleanup_identity(&self, id: u64) -> Result<u64> {
        Ok(self.cleanup.get(&id).ok_or(Error::InvalidHandle)?.raw())
    }
    pub fn execution_operation_identity(&self, id: u64) -> Result<(u64, u64)> {
        let (_, binding) = self
            .source_bindings
            .iter()
            .find(|(s, _)| s.operation == id)
            .ok_or(Error::InvalidHandle)?;
        Ok((binding.operation.host, binding.operation.serial))
    }
    pub fn execution_lease_identity(&self, id: u64) -> Result<(u64, u64)> {
        let lease = self.leases.get(&id).ok_or(Error::InvalidHandle)?;
        Ok((lease.instance, lease.serial))
    }
    pub fn execution_destination(&self) -> ExecutionDestination {
        self.destination.clone()
    }
    pub fn modules(&self) -> &BTreeMap<u64, ModuleHandle> {
        &self.modules
    }
    pub fn leases(&self) -> &BTreeMap<u64, PipelineLease> {
        &self.leases
    }
    pub fn owners(&self) -> &BTreeMap<u64, OwnerReference> {
        &self.owners
    }
    pub fn pins(&self) -> &BTreeMap<u64, ModulePin> {
        &self.pins
    }
    pub fn callbacks(&self) -> &BTreeMap<u64, RetainedCallback> {
        &self.callbacks
    }
    pub fn operations(&self) -> &[OperationBinding] {
        &self.operations
    }
    pub fn pure_captures(&self) -> &[PureCapture] {
        &self.pure_captures
    }
    pub fn cleanup(&self) -> &BTreeMap<u64, CleanupIdentity> {
        &self.cleanup
    }
    /// Metadata for future adapters, never an executable live lease.
    pub fn historical_sources(&self) -> &BTreeMap<u64, PipelineLease> {
        &self.historical_sources
    }
}

fn binding(b: &ProviderBinding) -> SavedBinding {
    SavedBinding {
        module: b.handle.generation,
        artifact: b.artifact,
        endpoint: b.endpoint.clone(),
    }
}
fn operation(b: &OperationBinding) -> SavedOperation {
    SavedOperation {
        operation: b.operation.serial,
        pipeline: b.pipeline.serial,
        checkpoint: b.checkpoint,
        phase: b.phase.clone(),
        cleanup: b.cleanup.iter().map(|i| i.raw()).collect(),
    }
}
fn routing(r: &Routing) -> SavedRouting {
    SavedRouting {
        routes: r
            .routes
            .iter()
            .map(|((h, ep), route)| SavedRoute {
                module: h.generation,
                endpoint: ep.clone(),
                chain: route.iter().map(binding).collect(),
            })
            .collect(),
        slots: r
            .slots
            .iter()
            .map(|((h, slot), s)| SavedSlot {
                caller: h.generation,
                slot: *slot,
                target: s.target.generation,
                ceiling: s.ceiling.clone(),
            })
            .collect(),
    }
}
fn pipeline(p: &Pipeline) -> SavedPipeline {
    SavedPipeline {
        root: binding(&p.descriptor.root),
        ceiling: p.descriptor.ceiling.clone(),
        hash: p.descriptor.hash,
        routing: routing(&p.routing),
        origin: p.origin.as_ref().map(|o| SavedOrigin {
            caller: binding(&o.provenance.caller),
            service: binding(&o.provenance.service),
            capability: o.provenance.capability.clone(),
            slot: o.slot,
            target: o.target.generation,
            ceiling: o.ceiling.clone(),
            source: operation(&o.provenance.source),
            service_route: o.service_route.iter().map(binding).collect(),
            source_pipeline: SavedWitness {
                root: binding(&o.source_pipeline.descriptor.root),
                ceiling: o.source_pipeline.descriptor.ceiling.clone(),
                hash: o.source_pipeline.descriptor.hash,
                routing: routing(&o.source_pipeline.routing),
            },
        }),
    }
}
fn references(id: u64, r: &BTreeSet<ModuleHandle>) -> SavedReferences {
    SavedReferences {
        id,
        modules: r.iter().map(|h| h.generation).collect(),
    }
}

impl Kernel {
    /// Freeze a pure closure's routing without invoking or scheduling code.
    pub(in crate::kernel) fn capture_pure_route(
        &self,
        frame: &Frame,
        slot: usize,
        target: ModuleHandle,
        endpoint: &str,
        authority: Authority,
    ) -> Result<(Rc<Routing>, ModulePin)> {
        let mut p = self.build_pipeline(
            target,
            endpoint,
            authority.clone(),
            frame.routing.as_deref(),
        )?;
        let mut r = Rc::try_unwrap(p.routing).map_err(|_| Error::Authority)?;
        r.slots.insert(
            (frame.provider, slot),
            RetainedSlot {
                target,
                ceiling: Some(authority),
            },
        );
        p.references.insert(frame.provider);
        let pin = self.pin_modules(&p.references.into_iter().collect::<Vec<_>>())?;
        Ok((Rc::new(r), pin))
    }

    /// Exclusive host safe boundary. The enrollment contract covers external
    /// values; all kernel-owned retained edges are captured automatically.
    pub fn kernel_snapshot(&mut self, enrollment: &KernelEnrollment<'_>) -> Result<KernelSnapshot> {
        if enrollment.operations.len() > MAX_ROWS || enrollment.pure_captures.len() > MAX_ROWS {
            return Err(Error::Limit);
        }
        for op in enrollment.operations {
            validate_live_binding(self, op)?;
        }
        let (limits, counters) = self.retention.history();
        // Bound allocation from live containers before copying graph metadata.
        let callbacks = self
            .retention
            .callbacks
            .try_borrow()
            .map_err(|_| Error::NativeRefusal)?;
        let pins = self
            .retention
            .pins
            .try_borrow()
            .map_err(|_| Error::NativeRefusal)?;
        let mut edges = 0usize;
        let mut scratch = 4096usize;
        for p in self.retention.pipelines.values() {
            account_pipeline(p, &mut edges, &mut scratch)?;
        }
        for c in callbacks.live.values().filter_map(|w| w.upgrade()) {
            account_pipeline(&c.pipeline, &mut edges, &mut scratch)?;
        }
        for r in self.retention.owners.values() {
            edges = edges.checked_add(r.len()).ok_or(Error::Limit)?;
        }
        for p in pins.live.values().filter_map(|w| w.upgrade()) {
            edges = edges.checked_add(p.references.len()).ok_or(Error::Limit)?;
        }
        for c in enrollment.pure_captures {
            account_live_routing(&c.routing, &mut edges, &mut scratch)?;
            for (permit, ceiling) in &c.closure {
                scratch = scratch
                    .checked_add(bounded_size(ceiling)?)
                    .and_then(|n| n.checked_add(permit.resolution.len().checked_mul(512)?))
                    .ok_or(Error::Limit)?;
            }
            edges = edges
                .checked_add(checked_sum(
                    c.closure.iter().map(|(p, _)| p.resolution.len()),
                )?)
                .ok_or(Error::Limit)?;
            scratch = scratch
                .checked_add(bounded_size(&c.ceiling)?)
                .ok_or(Error::Limit)?;
        }
        if edges > MAX_EDGES
            || scratch > MAX_BYTES
            || enrollment.operations.len() > MAX_ROWS
            || enrollment.pure_captures.len() > MAX_ROWS
        {
            return Err(Error::Limit);
        }
        let registry = self.capture_registry(limits, counters)?;
        let mut saved = KernelSnapshot {
            version: 1,
            source_namespace: self.instance,
            registry,
            leases: self
                .retention
                .pipelines
                .iter()
                .map(|(id, p)| SavedLease {
                    id: id.serial,
                    pipeline: pipeline(p),
                })
                .collect(),
            owners: self
                .retention
                .owners
                .iter()
                .map(|(id, r)| references(id.serial, r))
                .collect(),
            pins: pins
                .live
                .iter()
                .filter_map(|(id, p)| p.upgrade().map(|p| references(*id, &p.references)))
                .collect(),
            callbacks: callbacks
                .live
                .iter()
                .filter_map(|(id, c)| {
                    c.upgrade().map(|c| SavedCallback {
                        id: *id,
                        pipeline: pipeline(&c.pipeline),
                    })
                })
                .collect(),
            operations: Vec::new(),
            pure_captures: Vec::new(),
            cleanup_history: self.retention.cleanup_history.borrow().clone(),
            service_cleanup: BTreeSet::new(),
            service_resources: BTreeSet::new(),
            disposed_sources: BTreeSet::new(),
        };
        for b in enrollment.operations {
            // Authenticate the opaque enrollment against this kernel, without
            // issuing identities or modifying cleanup history during capture.
            validate_live_binding(self, b)?;
            let row = operation(b);
            if saved.operations.contains(&row) {
                return Err(Error::Duplicate);
            }
            saved.operations.push(row);
        }
        for c in enrollment.pure_captures {
            self.registration(c.provider)?;
            let pin = pins
                .live
                .iter()
                .find_map(|(id, p)| p.upgrade().filter(|p| Rc::ptr_eq(p, &c.pin.0)).map(|_| *id))
                .ok_or(Error::InvalidHandle)?;
            let frame = Frame {
                provider: c.provider,
                authority: c.ceiling.clone(),
                depth: 0,
                routing: Some(c.routing.clone()),
                operation: None,
                pure: true,
            };
            if self.inspect_pure_closure(&frame, c.slot)? != c.closure {
                return Err(Error::Authority);
            }
            saved.pure_captures.push(SavedPureCapture {
                provider: c.provider.generation,
                slot: c.slot,
                ceiling: c.ceiling.clone(),
                pin,
                routing: routing(&c.routing),
                closure: c
                    .closure
                    .iter()
                    .map(|(p, a)| SavedPermit {
                        capability: p.capability.clone(),
                        resolution: p.resolution.iter().map(binding).collect(),
                        ceiling: a.clone(),
                    })
                    .collect(),
            });
        }
        for p in saved
            .leases
            .iter()
            .map(|r| &r.pipeline)
            .chain(saved.callbacks.iter().map(|r| &r.pipeline))
        {
            if let Some(o) = &p.origin {
                if !self
                    .retention
                    .pipelines
                    .contains_key(&o_source(self.instance, &o.source))
                {
                    saved.disposed_sources.insert(o.source.pipeline);
                }
            }
        }
        saved.preflight(self)?;
        Ok(saved)
    }

    /// Complete admission into a hidden kernel, followed by one exchange. No
    /// callbacks, pure calls, preparation, lifecycle cleanup or sampling runs.
    pub fn restore_kernel(
        &mut self,
        saved: &KernelSnapshot,
        host: &impl RetainedAuthority,
    ) -> Result<RestoredKernel> {
        saved.preflight(self)?;
        saved.structural_preflight(self)?;
        for context in saved.operation_contexts()? {
            host.approve_operation_context(&context)?;
        }
        for cleanup in &saved.service_cleanup {
            host.approve_cleanup(*cleanup)?;
        }
        // Resolve each resource once; forbid cross-row aliases and unstable maps.
        let resources = saved.resources()?;
        let mut mapped = BTreeMap::new();
        let mut destinations = BTreeSet::new();
        for r in resources {
            let target = host.bind_resource(&r)?;
            if !destinations.insert(target.clone()) {
                return Err(Error::Authority);
            }
            mapped.insert(r, target);
        }
        let adapter = BoundAuthority {
            host,
            resources: &mapped,
        };
        let (mut hidden, modules) = self.build_registry(&saved.registry, &adapter)?;
        let mut excluded = saved.cleanup_history.clone();
        excluded.extend(self.retention.cleanup_history.borrow().iter().copied());
        if excluded.len() > MAX_CLEANUPS {
            return Err(Error::Limit);
        }
        let mut cleanup = BTreeMap::new();
        let mut needed_cleanup: BTreeSet<_> = saved
            .operations
            .iter()
            .chain(
                saved
                    .leases
                    .iter()
                    .filter_map(|r| r.pipeline.origin.as_ref().map(|o| &o.source)),
            )
            .chain(
                saved
                    .callbacks
                    .iter()
                    .filter_map(|r| r.pipeline.origin.as_ref().map(|o| &o.source)),
            )
            .flat_map(|o| o.cleanup.iter().copied())
            .collect();
        if !saved.service_cleanup.is_subset(&saved.cleanup_history) {
            return Err(Error::Authority);
        }
        needed_cleanup.extend(&saved.service_cleanup);
        for old in needed_cleanup {
            host.approve_cleanup(old)?;
            let fresh = CleanupIdentity::fresh_excluding(&excluded)?;
            if excluded.len() >= MAX_CLEANUPS {
                return Err(Error::Limit);
            }
            excluded.insert(fresh.raw());
            cleanup.insert(old, fresh);
        }
        hidden.retention.cleanup_history = RefCell::new(excluded);
        let remap = Remap {
            saved,
            modules: &modules,
            resources: &mapped,
            cleanup: &cleanup,
            instance: hidden.instance,
            structural: false,
        };
        let mut leases = BTreeMap::new();
        for row in &saved.leases {
            let lease = PipelineLease {
                instance: hidden.instance,
                serial: row.id,
            };
            let p = remap.pipeline(&hidden, &row.pipeline)?;
            hidden.reference_capacity(p.references.len())?;
            hidden.retention.pipelines.insert(lease, p);
            leases.insert(row.id, lease);
        }
        let mut owners = BTreeMap::new();
        for row in &saved.owners {
            let owner = OwnerReference {
                instance: hidden.instance,
                serial: row.id,
            };
            let r = remap.references(&row.modules)?;
            hidden.reference_capacity(r.len())?;
            hidden.retention.owners.insert(owner, r);
            owners.insert(row.id, owner);
        }
        let mut pins = BTreeMap::new();
        for row in &saved.pins {
            let references = remap.references(&row.modules)?;
            hidden.reference_capacity(references.len())?;
            let pin = ModulePin(Rc::new(PinData { references }));
            hidden
                .retention
                .pins
                .borrow_mut()
                .live
                .insert(row.id, Rc::downgrade(&pin.0));
            pins.insert(row.id, pin);
        }
        let mut callbacks = BTreeMap::new();
        for row in &saved.callbacks {
            let p = remap.pipeline(&hidden, &row.pipeline)?;
            hidden.reference_capacity(p.references.len())?;
            let provenance = p
                .origin
                .as_ref()
                .ok_or(Error::Authority)?
                .provenance
                .clone();
            let c = RetainedCallback(Rc::new(CallbackData {
                host: hidden.instance,
                pipeline: p,
                provenance,
            }));
            hidden
                .retention
                .callbacks
                .borrow_mut()
                .live
                .insert(row.id, Rc::downgrade(&c.0));
            callbacks.insert(row.id, c);
        }
        if hidden.lifecycle_state().owners > hidden.retention.limits.owners {
            return Err(Error::Limit);
        }
        let operations = saved
            .operations
            .iter()
            .map(|b| remap.operation(b))
            .collect::<Result<Vec<_>>>()?;
        let mut pure_captures = Vec::new();
        for row in &saved.pure_captures {
            let provider = remap.module(row.provider)?;
            let ceiling = remap.authority(&row.ceiling)?;
            let routing = Rc::new(remap.routing(&hidden, &row.routing)?);
            let closure = row
                .closure
                .iter()
                .map(|p| {
                    Ok((
                        CapabilityPermit {
                            capability: p.capability.clone(),
                            resolution: p
                                .resolution
                                .iter()
                                .map(|b| remap.binding(&hidden, b))
                                .collect::<Result<_>>()?,
                        },
                        remap.authority(&p.ceiling)?,
                    ))
                })
                .collect::<Result<Vec<_>>>()?;
            let frame = Frame {
                provider,
                authority: ceiling.clone(),
                depth: 0,
                routing: Some(routing.clone()),
                operation: None,
                pure: true,
            };
            if hidden.inspect_pure_closure(&frame, row.slot)? != closure {
                return Err(Error::Authority);
            }
            let pin = pins.get(&row.pin).ok_or(Error::InvalidHandle)?.clone();
            let expected = pure_references(&hidden, &frame, row.slot, &routing)?;
            if pin.0.references != expected {
                return Err(Error::Authority);
            }
            pure_captures.push(PureCapture {
                provider,
                slot: row.slot,
                ceiling,
                closure,
                pin,
                routing,
            });
        }
        let historical_sources = saved
            .disposed_sources
            .iter()
            .map(|&id| {
                (
                    id,
                    PipelineLease {
                        instance: hidden.instance,
                        serial: id,
                    },
                )
            })
            .collect();
        let mut source_bindings = Vec::new();
        for source in saved.operations.iter().chain(
            saved
                .leases
                .iter()
                .map(|r| &r.pipeline)
                .chain(saved.callbacks.iter().map(|r| &r.pipeline))
                .filter_map(|p| p.origin.as_ref().map(|o| &o.source)),
        ) {
            if !source_bindings.iter().any(|(s, _)| s == source) {
                source_bindings.push((source.clone(), remap.operation(source)?));
            }
        }
        let result = RestoredKernel {
            destination: ExecutionDestination(Rc::new(())),
            modules,
            leases,
            owners,
            pins,
            callbacks,
            operations,
            pure_captures,
            cleanup,
            historical_sources,
            source_bindings,
            execution_namespace: saved.source_namespace,
            workflow_enrollment: Rc::new(RefCell::new(BTreeSet::new())),
            timer_enrollment: Rc::new(RefCell::new(BTreeSet::new())),
            adapter_enrollment: Rc::new(RefCell::new(BTreeSet::new())),
            execution_contexts: saved.operation_contexts()?,
            execution_captures: saved.pure_captures.clone(),
            execution_resources: mapped,
        };
        *self = hidden;
        Ok(result)
    }
}

fn o_source(instance: u64, s: &SavedOperation) -> PipelineLease {
    PipelineLease {
        instance,
        serial: s.pipeline,
    }
}
fn validate_live_binding(k: &Kernel, b: &OperationBinding) -> Result<()> {
    if b.operation.host != k.instance
        || b.operation.serial == 0
        || b.operation.serial >= k.next_operation
        || b.pipeline.instance != k.instance
        || b.phase.len() > 256
        || b.cleanup.len() > k.limits.scope_keys
        || b.cleanup.windows(2).any(|w| w[0] >= w[1])
    {
        return Err(Error::Authority);
    }
    k.pipeline_descriptor(b.pipeline)?;
    if b.cleanup
        .iter()
        .any(|id| !k.retention.cleanup_history.borrow().contains(&id.raw()))
    {
        return Err(Error::Authority);
    }
    Ok(())
}

// Stable resource bindings shared by registry and retained reconstruction.
struct BoundAuthority<'a, H> {
    host: &'a H,
    resources: &'a BTreeMap<Resource, Resource>,
}
impl<H: RegistryAuthority> RegistryAuthority for BoundAuthority<'_, H> {
    fn resolve(&self, row: &SavedModule) -> Result<ArtifactMaterial> {
        self.host.resolve(row)
    }
    fn bind_resource(&self, r: &Resource) -> Result<Resource> {
        self.resources.get(r).cloned().ok_or(Error::Authority)
    }
    fn approve_grant(
        &self,
        c: &SavedModule,
        slot: usize,
        t: &SavedModule,
        a: &Authority,
    ) -> Result<Authority> {
        self.host.approve_grant(c, slot, t, a)
    }
}

struct Remap<'a> {
    // Temporary metadata validation cannot issue cleanup credentials.
    structural: bool,
    saved: &'a KernelSnapshot,
    modules: &'a BTreeMap<u64, ModuleHandle>,
    resources: &'a BTreeMap<Resource, Resource>,
    cleanup: &'a BTreeMap<u64, CleanupIdentity>,
    instance: u64,
}
impl Remap<'_> {
    fn module(&self, id: u64) -> Result<ModuleHandle> {
        self.modules.get(&id).copied().ok_or(Error::InvalidHandle)
    }
    fn binding(&self, k: &Kernel, b: &SavedBinding) -> Result<ProviderBinding> {
        let handle = self.module(b.module)?;
        if k.artifact(handle)? != b.artifact
            || !Kernel::valid_name(&b.endpoint)
            || !k
                .registration(handle)?
                .manifest
                .exports
                .contains_key(&b.endpoint)
        {
            return Err(Error::InvalidModule);
        }
        Ok(ProviderBinding {
            handle,
            artifact: b.artifact,
            endpoint: b.endpoint.clone(),
        })
    }
    fn authority(&self, a: &Authority) -> Result<Authority> {
        Ok(Authority {
            access: a.access,
            scope: match &a.scope {
                Scope::All => Scope::All,
                Scope::Keys(keys) => Scope::Keys(
                    keys.iter()
                        .map(|r| self.resources.get(r).cloned().ok_or(Error::Authority))
                        .collect::<Result<_>>()?,
                ),
            },
        })
    }
    fn operation(&self, b: &SavedOperation) -> Result<OperationBinding> {
        Ok(OperationBinding {
            operation: OperationId {
                host: self.instance,
                serial: b.operation,
            },
            pipeline: PipelineLease {
                instance: self.instance,
                serial: b.pipeline,
            },
            checkpoint: b.checkpoint,
            phase: b.phase.clone(),
            cleanup: if self.structural {
                Vec::new()
            } else {
                b.cleanup
                    .iter()
                    .map(|id| self.cleanup.get(id).copied().ok_or(Error::Authority))
                    .collect::<Result<BTreeSet<_>>>()?
                    .into_iter()
                    .collect()
            },
        })
    }
    fn references(&self, ids: &[u64]) -> Result<BTreeSet<ModuleHandle>> {
        if ids.is_empty() || ids.windows(2).any(|w| w[0] >= w[1]) {
            return Err(Error::InvalidHandle);
        }
        ids.iter().map(|&id| self.module(id)).collect()
    }
    fn chain(&self, k: &Kernel, chain: &[SavedBinding]) -> Result<Vec<ProviderBinding>> {
        if chain.is_empty()
            || chain.len() > k.limits.depth as usize
            || chain.iter().any(|b| !Kernel::valid_name(&b.endpoint))
        {
            return Err(Error::Limit);
        }
        let result = chain
            .iter()
            .map(|b| self.binding(k, b))
            .collect::<Result<Vec<_>>>()?;
        if result.is_empty() || result.len() > k.limits.depth as usize {
            return Err(Error::Limit);
        }
        let mut seen = BTreeSet::new();
        for b in &result {
            if !seen.insert((b.handle, &b.endpoint)) {
                return Err(Error::Cycle);
            }
        }
        for pair in result.windows(2) {
            let a = &pair[0];
            let b = &pair[1];
            if k.registration(b.handle)?.attachment.as_ref()
                != Some(&(a.handle, a.endpoint.clone(), b.endpoint.clone()))
            {
                return Err(Error::Signature);
            }
        }
        Ok(result)
    }
    fn routing(&self, k: &Kernel, saved: &SavedRouting) -> Result<Routing> {
        let mut routes = BTreeMap::new();
        let mut previous = None;
        for row in &saved.routes {
            let key = (self.module(row.module)?, row.endpoint.clone());
            if previous.as_ref().is_some_and(|p| p >= &key) {
                return Err(Error::Duplicate);
            }
            let chain = self.chain(k, &row.chain)?;
            if chain[0].handle != key.0 || chain[0].endpoint != key.1 {
                return Err(Error::Authority);
            }
            previous = Some(key.clone());
            routes.insert(key, chain);
        }
        let mut slots = BTreeMap::new();
        let mut previous = None;
        for row in &saved.slots {
            let caller = self.module(row.caller)?;
            let key = (caller, row.slot);
            if previous.is_some_and(|p| p >= key) {
                return Err(Error::Duplicate);
            }
            let target = self.module(row.target)?;
            let dep = k
                .registration(caller)?
                .manifest
                .dependencies
                .get(row.slot)
                .ok_or(Error::InvalidSlot)?;
            if k.registration(caller)?.dependencies.get(row.slot) != Some(&target)
                || !routes.contains_key(&(target, dep.capability.endpoint.clone()))
            {
                return Err(Error::MissingDependency);
            }
            let ceiling = row
                .ceiling
                .as_ref()
                .map(|a| self.authority(a))
                .transpose()?;
            if let Some(a) = &ceiling {
                k.validate_authority(a, false)?;
                let grant = k.grants.get(&key).ok_or(Error::MissingGrant)?;
                if !grant.active {
                    return Err(Error::Revoked);
                }
                if grant.target != target || !grant.authority.contains(a) || a.access > dep.access {
                    return Err(Error::Authority);
                }
            }
            previous = Some(key);
            slots.insert(key, RetainedSlot { target, ceiling });
        }
        Ok(Routing { routes, slots })
    }
    fn pipeline(&self, k: &Kernel, row: &SavedPipeline) -> Result<Pipeline> {
        let root = self.binding(k, &row.root)?;
        let ceiling = self.authority(&row.ceiling)?;
        let routing = self.routing(k, &row.routing)?;
        // Recompute the source digest using the exact saved namespace and scopes.
        let old = old_routing(self.saved.source_namespace, &row.routing);
        let old_root = old_binding(self.saved.source_namespace, &row.root);
        let old_routes = old.routes.values().cloned().collect::<Vec<_>>();
        if pipeline_digest(&old_root, &row.ceiling, &old_routes, &old)? != row.hash {
            return Err(Error::InvalidModule);
        }
        // Every route is supplied by saved evidence. Validate strict closure before
        // using build_pipeline; its fallback to current dispatch is unreachable.
        complete_routing(k, &root, &routing)?;
        let mut p =
            k.build_pipeline(root.handle, &root.endpoint, ceiling.clone(), Some(&routing))?;
        if routing_evidence(&p.routing) != routing_evidence(&routing) {
            return Err(Error::Authority);
        }
        if let Some(o) = &row.origin {
            let caller = self.binding(k, &o.caller)?;
            let service = self.binding(k, &o.service)?;
            if !matches!(k.registration(service.handle)?.code, Code::Native(_)) {
                return Err(Error::Authority);
            }
            let target = self.module(o.target)?;
            let service_route = self.chain(k, &o.service_route)?;
            let dep = k
                .registration(caller.handle)?
                .manifest
                .dependencies
                .get(o.slot)
                .ok_or(Error::InvalidSlot)?;
            if caller != p.descriptor.root
                || dep.capability != o.capability
                || k.registration(caller.handle)?.dependencies.get(o.slot) != Some(&target)
                || service_route.first().map(|b| (b.handle, &b.endpoint))
                    != Some((target, &dep.capability.endpoint))
                || service_route.last() != Some(&service)
            {
                return Err(Error::Authority);
            }
            let witness_row = SavedPipeline {
                root: o.source_pipeline.root.clone(),
                ceiling: o.source_pipeline.ceiling.clone(),
                hash: o.source_pipeline.hash,
                routing: o.source_pipeline.routing.clone(),
                origin: None,
            };
            let witness = self.pipeline(k, &witness_row)?;
            // The capture service must be exactly the end of the source import,
            // and the caller must occur in the authenticated source execution.
            if !witness
                .routing
                .routes
                .values()
                .any(|route| route.iter().any(|b| b.handle == caller.handle))
                || witness
                    .routing
                    .routes
                    .get(&(target, dep.capability.endpoint.clone()))
                    != Some(&service_route)
            {
                return Err(Error::Authority);
            }
            if let Some(live) = self.saved.leases.iter().find(|r| r.id == o.source.pipeline) {
                if live.pipeline.hash != o.source_pipeline.hash {
                    return Err(Error::Authority);
                }
            }
            let origin_ceiling = self.authority(&o.ceiling)?;
            let source_slot = witness
                .routing
                .slots
                .get(&(caller.handle, o.slot))
                .ok_or(Error::Authority)?;
            if !witness.descriptor.ceiling.contains(&origin_ceiling)
                || !source_slot
                    .ceiling
                    .as_ref()
                    .is_some_and(|a| a.contains(&origin_ceiling))
            {
                return Err(Error::Authority);
            }
            let source = self.operation(&o.source)?;
            let provenance = CallbackProvenance {
                caller: caller.clone(),
                service,
                capability: o.capability.clone(),
                slot: o.slot,
                source,
            };
            let origin = Rc::new(CallbackOrigin {
                caller: caller.handle,
                slot: o.slot,
                target,
                ceiling: self.authority(&o.ceiling)?,
                provenance,
                service_route: service_route.clone(),
                source_pipeline: PipelineWitness {
                    descriptor: witness.descriptor,
                    routing: witness.routing,
                    references: witness.references,
                },
            });
            if origin.ceiling != p.descriptor.ceiling {
                return Err(Error::Authority);
            }
            k.validate_callback_origin(&origin)?;
            p.references.extend(service_route.iter().map(|b| b.handle));
            p.references
                .extend(origin.source_pipeline.references.iter().copied());
            if p.references.len() > k.retention.limits.nodes {
                return Err(Error::Limit);
            }
            p.origin = Some(origin);
        }
        Ok(p)
    }
}
fn old_binding(instance: u64, b: &SavedBinding) -> ProviderBinding {
    ProviderBinding {
        handle: ModuleHandle {
            instance,
            generation: b.module,
        },
        artifact: b.artifact,
        endpoint: b.endpoint.clone(),
    }
}
fn old_routing(instance: u64, r: &SavedRouting) -> Routing {
    Routing {
        routes: r
            .routes
            .iter()
            .map(|r| {
                (
                    (
                        ModuleHandle {
                            instance,
                            generation: r.module,
                        },
                        r.endpoint.clone(),
                    ),
                    r.chain.iter().map(|b| old_binding(instance, b)).collect(),
                )
            })
            .collect(),
        slots: r
            .slots
            .iter()
            .map(|s| {
                (
                    (
                        ModuleHandle {
                            instance,
                            generation: s.caller,
                        },
                        s.slot,
                    ),
                    RetainedSlot {
                        target: ModuleHandle {
                            instance,
                            generation: s.target,
                        },
                        ceiling: s.ceiling.clone(),
                    },
                )
            })
            .collect(),
    }
}
fn routing_evidence(r: &Routing) -> Vec<((ModuleHandle, usize), ModuleHandle, Option<Authority>)> {
    r.slots
        .iter()
        .map(|(key, s)| (*key, s.target, s.ceiling.clone()))
        .collect()
}
fn complete_routing(k: &Kernel, root: &ProviderBinding, r: &Routing) -> Result<()> {
    fn visit(
        k: &Kernel,
        key: RouteKey,
        r: &Routing,
        visiting: &mut BTreeSet<RouteKey>,
        seen: &mut BTreeSet<RouteKey>,
        slots: &mut BTreeSet<(ModuleHandle, usize)>,
        depth: u32,
    ) -> Result<()> {
        if depth > k.limits.depth {
            return Err(Error::Limit);
        }
        if visiting.contains(&key) {
            return Err(Error::Cycle);
        }
        if seen.contains(&key) {
            return Ok(());
        }
        visiting.insert(key.clone());
        let chain = r.routes.get(&key).ok_or(Error::MissingDependency)?;
        for b in chain {
            for (slot, dep) in k
                .registration(b.handle)?
                .manifest
                .dependencies
                .iter()
                .enumerate()
            {
                slots.insert((b.handle, slot));
                let retained = r
                    .slots
                    .get(&(b.handle, slot))
                    .ok_or(Error::MissingDependency)?;
                visit(
                    k,
                    (retained.target, dep.capability.endpoint.clone()),
                    r,
                    visiting,
                    seen,
                    slots,
                    depth.checked_add(1).ok_or(Error::Limit)?,
                )?;
            }
        }
        visiting.remove(&key);
        seen.insert(key);
        Ok(())
    }
    let mut seen = BTreeSet::new();
    let mut slots = BTreeSet::new();
    visit(
        k,
        (root.handle, root.endpoint.clone()),
        r,
        &mut BTreeSet::new(),
        &mut seen,
        &mut slots,
        1,
    )?;
    if seen.len() != r.routes.len() || slots.len() != r.slots.len() {
        return Err(Error::Authority);
    }
    Ok(())
}
fn pure_references(
    k: &Kernel,
    f: &Frame,
    slot: usize,
    r: &Routing,
) -> Result<BTreeSet<ModuleHandle>> {
    let (target, endpoint, authority, _) = k.slot_target(f, slot)?;
    let root = ProviderBinding {
        handle: target,
        artifact: k.artifact(target)?,
        endpoint: endpoint.clone(),
    };
    // The only additional edge is the enrolled provider's selected slot.
    let core = Routing {
        routes: r.routes.clone(),
        slots: r
            .slots
            .iter()
            .filter(|(key, _)| **key != (f.provider, slot))
            .map(|(key, s)| {
                (
                    *key,
                    RetainedSlot {
                        target: s.target,
                        ceiling: s.ceiling.clone(),
                    },
                )
            })
            .collect(),
    };
    complete_routing(k, &root, &core)?;
    let p = k.build_pipeline(target, &endpoint, authority, Some(&core))?;
    if routing_evidence(&p.routing) != routing_evidence(&core) {
        return Err(Error::Authority);
    }
    let mut refs = p.references;
    refs.insert(f.provider);
    Ok(refs)
}

fn account_pipeline(p: &Pipeline, edges: &mut usize, scratch: &mut usize) -> Result<()> {
    *edges = edges
        .checked_add(checked_sum(p.routing.routes.values().map(Vec::len))?)
        .and_then(|n| n.checked_add(p.routing.slots.len()))
        .ok_or(Error::Limit)?;
    *scratch = scratch
        .checked_add(bounded_size(&p.descriptor.ceiling)?)
        .ok_or(Error::Limit)?;
    for ((_, ep), chain) in &p.routing.routes {
        *scratch = scratch
            .checked_add(ep.len())
            .and_then(|n| n.checked_add(chain.len().checked_mul(512)?))
            .ok_or(Error::Limit)?;
    }
    for s in p.routing.slots.values() {
        if let Some(a) = &s.ceiling {
            *scratch = scratch.checked_add(bounded_size(a)?).ok_or(Error::Limit)?;
        }
    }
    if let Some(o) = &p.origin {
        *scratch = scratch
            .checked_add(o.provenance.source.phase.len())
            .and_then(|n| n.checked_add(o.service_route.len().checked_mul(512)?))
            .ok_or(Error::Limit)?;
        account_live_routing(&o.source_pipeline.routing, edges, scratch)?;
        *scratch = scratch
            .checked_add(bounded_size(&o.source_pipeline.descriptor.ceiling)?)
            .ok_or(Error::Limit)?;
    }
    if *edges > MAX_EDGES || *scratch > MAX_BYTES {
        return Err(Error::Limit);
    }
    Ok(())
}

impl KernelSnapshot {
    /// Untrusted approval requests, NOT an authority catalog. Hosts must obtain
    /// their expected contexts independently, before accepting an incoming save.
    pub fn operation_contexts(&self) -> Result<Vec<SavedOperationContext>> {
        let witness = |p: &SavedPipeline| SavedWitness {
            root: p.root.clone(),
            ceiling: p.ceiling.clone(),
            hash: p.hash,
            routing: p.routing.clone(),
        };
        let mut contexts = Vec::new();
        for operation in &self.operations {
            let p = &self
                .leases
                .iter()
                .find(|r| r.id == operation.pipeline)
                .ok_or(Error::InvalidHandle)?
                .pipeline;
            contexts.push(SavedOperationContext {
                namespace: self.source_namespace,
                operation: operation.clone(),
                pipeline: witness(p),
                origin: p.origin.clone(),
                retained: None,
            });
        }
        for p in self
            .leases
            .iter()
            .map(|r| &r.pipeline)
            .chain(self.callbacks.iter().map(|r| &r.pipeline))
        {
            if let Some(o) = &p.origin {
                contexts.push(SavedOperationContext {
                    namespace: self.source_namespace,
                    operation: o.source.clone(),
                    pipeline: o.source_pipeline.clone(),
                    origin: Some(o.clone()),
                    retained: Some(witness(p)),
                });
            }
        }
        Ok(contexts)
    }

    fn structural_preflight(&self, destination: &Kernel) -> Result<()> {
        // Saved metadata gets the same routing, digest, reference and pure
        // closure validation as constructed artifacts, with no host factories.
        let (k, modules) = self.registry.structural_kernel(self.source_namespace)?;
        let resources = self
            .resources()?
            .into_iter()
            .map(|r| (r.clone(), r))
            .collect();
        let remap = Remap {
            saved: self,
            modules: &modules,
            resources: &resources,
            cleanup: &BTreeMap::new(),
            instance: self.source_namespace,
            structural: true,
        };
        let rows: BTreeMap<_, _> = self.registry.modules.iter().map(|r| (r.id, r)).collect();
        let mut references = checked_sum(
            self.owners
                .iter()
                .chain(&self.pins)
                .map(|r| r.modules.len()),
        )?;
        for p in self
            .leases
            .iter()
            .map(|r| &r.pipeline)
            .chain(self.callbacks.iter().map(|r| &r.pipeline))
        {
            if let Some(o) = &p.origin {
                if rows
                    .get(&o.service.module)
                    .ok_or(Error::InvalidHandle)?
                    .executable
                    != crate::kernel::persistence::Executable::NativeV1
                {
                    return Err(Error::Authority);
                }
                // A live source is the exact witness, not just another wire hash.
                if let Some(live) = self.leases.iter().find(|r| r.id == o.source.pipeline) {
                    let source = &live.pipeline;
                    if source.root != o.source_pipeline.root
                        || source.ceiling != o.source_pipeline.ceiling
                        || source.hash != o.source_pipeline.hash
                        || source.routing != o.source_pipeline.routing
                    {
                        return Err(Error::Authority);
                    }
                }
            }
            let pipeline = remap.pipeline(&k, p)?;
            references = references
                .checked_add(pipeline.references.len())
                .ok_or(Error::Limit)?;
            if references > self.registry.retention_limits.references {
                return Err(Error::Limit);
            }
        }
        if references > self.registry.retention_limits.references {
            return Err(Error::Limit);
        }
        for c in &self.pure_captures {
            let provider = remap.module(c.provider)?;
            let routing = Rc::new(remap.routing(&k, &c.routing)?);
            let frame = Frame {
                provider,
                authority: c.ceiling.clone(),
                depth: 0,
                routing: Some(routing.clone()),
                operation: None,
                pure: true,
            };
            let closure = c
                .closure
                .iter()
                .map(|p| {
                    Ok((
                        CapabilityPermit {
                            capability: p.capability.clone(),
                            resolution: remap.chain(&k, &p.resolution)?,
                        },
                        p.ceiling.clone(),
                    ))
                })
                .collect::<Result<Vec<_>>>()?;
            let expected = k.inspect_pure_closure_with(&frame, c.slot, &|h| {
                Ok(rows
                    .get(&h.generation)
                    .ok_or(Error::InvalidHandle)?
                    .executable
                    == crate::kernel::persistence::Executable::WasmAbiV1)
            })?;
            if closure != expected {
                return Err(Error::Authority);
            }
            let pin = self
                .pins
                .iter()
                .find(|p| p.id == c.pin)
                .ok_or(Error::InvalidHandle)?;
            if remap.references(&pin.modules)? != pure_references(&k, &frame, c.slot, &routing)? {
                return Err(Error::Authority);
            }
        }
        let cleanup: BTreeSet<_> = self
            .cleanup_history
            .union(&destination.retention.cleanup_history.borrow())
            .copied()
            .collect();
        let mut needed: BTreeSet<_> = self
            .operation_contexts()?
            .iter()
            .flat_map(|c| c.operation.cleanup.iter().copied())
            .collect();
        needed.extend(&self.service_cleanup);
        if cleanup
            .len()
            .checked_add(needed.len())
            .ok_or(Error::Limit)?
            > MAX_CLEANUPS
        {
            return Err(Error::Limit);
        }
        Ok(())
    }

    fn preflight(&self, destination: &Kernel) -> Result<()> {
        if self.version != 1
            || self.source_namespace == 0
            || !self.registry.namespaces.contains(&self.source_namespace)
            || self.cleanup_history.len() > MAX_CLEANUPS
            || self.cleanup_history.contains(&0)
            || !self.service_cleanup.is_subset(&self.cleanup_history)
            || self.disposed_sources.len() > MAX_ROWS
            || self.operations.len() > MAX_ROWS
            || self.pure_captures.len() > MAX_ROWS
            || self.leases.len() > self.registry.retention_limits.leases.min(MAX_ROWS)
            || self
                .owners
                .len()
                .checked_add(self.pins.len())
                .ok_or(Error::Limit)?
                > self.registry.retention_limits.owners.min(MAX_ROWS)
        {
            return Err(Error::Limit);
        }
        if self.callbacks.len() > self.registry.retention_limits.owners.min(MAX_ROWS) {
            return Err(Error::Limit);
        }
        bounded_size(self)?;
        self.registry.preflight(destination)?;
        let mut serials = BTreeSet::new();
        check_ids(
            self.leases.iter().map(|r| r.id),
            self.registry.retention_counters[0],
            &mut serials,
        )?;
        check_ids(
            self.owners.iter().map(|r| r.id),
            self.registry.retention_counters[0],
            &mut serials,
        )?;
        check_ids(
            self.pins.iter().map(|r| r.id),
            self.registry.retention_counters[2],
            &mut BTreeSet::new(),
        )?;
        check_ids(
            self.callbacks.iter().map(|r| r.id),
            self.registry.retention_counters[1],
            &mut BTreeSet::new(),
        )?;
        if self.disposed_sources.iter().any(|id| {
            *id == 0 || *id >= self.registry.retention_counters[0] || serials.contains(id)
        }) {
            return Err(Error::InvalidHandle);
        }
        let live: BTreeSet<_> = self.leases.iter().map(|r| r.id).collect();
        let modules: BTreeSet<_> = self.registry.modules.iter().map(|r| r.id).collect();
        let mut referenced_disposed = BTreeSet::new();
        let validate_op = |b: &SavedOperation| -> Result<()> {
            if b.operation == 0
                || b.operation >= self.registry.next_operation
                || b.pipeline == 0
                || b.pipeline >= self.registry.retention_counters[0]
                || b.phase.len() > 256
                || b.phase.len() > destination.limits.payload_bytes as usize
                || b.cleanup.len() > destination.limits.scope_keys
                || b.cleanup.windows(2).any(|w| w[0] >= w[1])
                || b.cleanup
                    .iter()
                    .any(|id| !self.cleanup_history.contains(id))
            {
                return Err(Error::Authority);
            }
            Ok(())
        };
        for (i, b) in self.operations.iter().enumerate() {
            validate_op(b)?;
            if !live.contains(&b.pipeline) || self.operations[..i].contains(b) {
                return Err(Error::InvalidHandle);
            }
        }
        let mut edges = self
            .registry
            .grants
            .len()
            .checked_add(self.registry.dispatch.len())
            .ok_or(Error::Limit)?;
        for m in &self.registry.modules {
            edges = edges
                .checked_add(m.dependencies.len())
                .and_then(|n| n.checked_add(usize::from(m.target.is_some())))
                .ok_or(Error::Limit)?;
        }
        for r in self.owners.iter().chain(&self.pins) {
            if r.modules.is_empty()
                || r.modules.len() > self.registry.retention_limits.nodes
                || r.modules.windows(2).any(|w| w[0] >= w[1])
                || r.modules.iter().any(|id| !modules.contains(id))
            {
                return Err(Error::InvalidHandle);
            }
            edges = edges.checked_add(r.modules.len()).ok_or(Error::Limit)?;
        }
        for p in self
            .leases
            .iter()
            .map(|r| &r.pipeline)
            .chain(self.callbacks.iter().map(|r| &r.pipeline))
        {
            validate_record_authority(destination, &p.ceiling, &p.routing)?;
            account_routing(&p.routing, &self.registry.retention_limits, &mut edges)?;
            if let Some(o) = &p.origin {
                validate_op(&o.source)?;
                destination.validate_authority(&o.ceiling, false)?;
                validate_capability(&o.capability)?;
                validate_record_authority(
                    destination,
                    &o.source_pipeline.ceiling,
                    &o.source_pipeline.routing,
                )?;
                if o.service_route.len() > destination.limits.depth as usize
                    || o.service_route
                        .iter()
                        .any(|b| !Kernel::valid_name(&b.endpoint))
                {
                    return Err(Error::Limit);
                }
                account_routing(
                    &o.source_pipeline.routing,
                    &self.registry.retention_limits,
                    &mut edges,
                )?;
                if !live.contains(&o.source.pipeline) {
                    if !self.disposed_sources.contains(&o.source.pipeline) {
                        return Err(Error::InvalidHandle);
                    }
                    referenced_disposed.insert(o.source.pipeline);
                }
                edges = edges
                    .checked_add(o.service_route.len())
                    .and_then(|n| n.checked_add(o.source.cleanup.len()))
                    .ok_or(Error::Limit)?;
            }
        }
        if self.callbacks.iter().any(|r| r.pipeline.origin.is_none())
            || referenced_disposed != self.disposed_sources
        {
            return Err(Error::Authority);
        }
        for c in &self.pure_captures {
            if c.ceiling.access != Access::Read {
                return Err(Error::Authority);
            }
            validate_record_authority(destination, &c.ceiling, &c.routing)?;
            account_routing(&c.routing, &self.registry.retention_limits, &mut edges)?;
            if c.closure.is_empty() || c.closure.len() > destination.limits.calls as usize {
                return Err(Error::Limit);
            }
            for p in &c.closure {
                validate_capability(&p.capability)?;
                destination.validate_authority(&p.ceiling, false)?;
                if p.resolution.len() > destination.limits.depth as usize
                    || p.resolution
                        .iter()
                        .any(|b| !Kernel::valid_name(&b.endpoint))
                {
                    return Err(Error::Limit);
                }
                edges = edges.checked_add(p.resolution.len()).ok_or(Error::Limit)?;
            }
        }
        edges = edges
            .checked_add(checked_sum(
                self.operations.iter().map(|o| o.cleanup.len()),
            )?)
            .ok_or(Error::Limit)?;
        if edges > MAX_EDGES {
            return Err(Error::Limit);
        }
        Ok(())
    }
    fn resources(&self) -> Result<BTreeSet<Resource>> {
        if self.service_resources.len() > MAX_ROWS {
            return Err(Error::Limit);
        }
        for r in &self.service_resources {
            if r.owner.is_empty() || r.key.is_empty() || r.owner.len() > 256 || r.key.len() > 256 {
                return Err(Error::Limit);
            }
        }
        let mut resources = self.service_resources.clone();
        let mut add = |a: &Authority| {
            if let Scope::Keys(keys) = &a.scope {
                resources.extend(keys.iter().cloned());
            }
        };
        for g in &self.registry.grants {
            add(&g.requested);
        }
        for p in self
            .leases
            .iter()
            .map(|r| &r.pipeline)
            .chain(self.callbacks.iter().map(|r| &r.pipeline))
        {
            add(&p.ceiling);
            for s in &p.routing.slots {
                if let Some(a) = &s.ceiling {
                    add(a);
                }
            }
            if let Some(o) = &p.origin {
                add(&o.ceiling);
                add(&o.source_pipeline.ceiling);
                for s in &o.source_pipeline.routing.slots {
                    if let Some(a) = &s.ceiling {
                        add(a);
                    }
                }
            }
        }
        for c in &self.pure_captures {
            add(&c.ceiling);
            for p in &c.closure {
                add(&p.ceiling);
            }
            for s in &c.routing.slots {
                if let Some(a) = &s.ceiling {
                    add(a);
                }
            }
        }
        if resources.len() > MAX_EDGES {
            return Err(Error::Limit);
        }
        Ok(resources)
    }
}
fn check_ids(
    ids: impl Iterator<Item = u64>,
    highwater: u64,
    seen: &mut BTreeSet<u64>,
) -> Result<()> {
    let mut previous = 0;
    for id in ids {
        if id == 0 || id <= previous || id >= highwater || !seen.insert(id) {
            return Err(Error::InvalidHandle);
        }
        previous = id;
    }
    Ok(())
}
fn account_routing(r: &SavedRouting, limits: &RetentionLimits, edges: &mut usize) -> Result<()> {
    if r.routes.len() > limits.nodes || r.slots.len() > limits.slots {
        return Err(Error::Limit);
    }
    let mut nodes = 0usize;
    for route in &r.routes {
        if route.endpoint.len() > 256 || route.chain.iter().any(|b| b.endpoint.len() > 256) {
            return Err(Error::Limit);
        }
        nodes = nodes.checked_add(route.chain.len()).ok_or(Error::Limit)?;
    }
    if nodes > limits.nodes {
        return Err(Error::Limit);
    }
    *edges = edges
        .checked_add(nodes)
        .and_then(|n| n.checked_add(r.slots.len()))
        .ok_or(Error::Limit)?;
    if *edges > MAX_EDGES {
        return Err(Error::Limit);
    }
    Ok(())
}

fn account_live_routing(r: &Routing, edges: &mut usize, scratch: &mut usize) -> Result<()> {
    *edges = edges.checked_add(r.slots.len()).ok_or(Error::Limit)?;
    for ((_, ep), chain) in &r.routes {
        *edges = edges.checked_add(chain.len()).ok_or(Error::Limit)?;
        *scratch = scratch
            .checked_add(ep.len())
            .and_then(|n| n.checked_add(chain.len().checked_mul(512)?))
            .ok_or(Error::Limit)?;
    }
    for s in r.slots.values() {
        if let Some(a) = &s.ceiling {
            *scratch = scratch.checked_add(bounded_size(a)?).ok_or(Error::Limit)?;
        }
    }
    if *edges > MAX_EDGES || *scratch > MAX_BYTES {
        return Err(Error::Limit);
    }
    Ok(())
}

fn validate_capability(c: &Capability) -> Result<()> {
    if !Kernel::valid_name(&c.provider) || !Kernel::valid_name(&c.endpoint) || c.major == 0 {
        return Err(Error::InvalidManifest);
    }
    Ok(())
}
fn validate_record_authority(k: &Kernel, a: &Authority, r: &SavedRouting) -> Result<()> {
    k.validate_authority(a, false)?;
    for s in &r.slots {
        if let Some(a) = &s.ceiling {
            k.validate_authority(a, false)?;
        }
    }
    Ok(())
}

fn checked_sum(mut values: impl Iterator<Item = usize>) -> Result<usize> {
    values.try_fold(0usize, |n, v| n.checked_add(v).ok_or(Error::Limit))
}

/// A borrowed, sealed adapter over an admitted capture. No raw credential parsing.
pub struct ExecutionCaptureIndex<'a> {
    kernel: &'a Kernel,
    saved: &'a KernelSnapshot,
}
impl Kernel {
    pub fn execution_capture_index<'a>(
        &'a self,
        saved: &'a KernelSnapshot,
    ) -> Result<ExecutionCaptureIndex<'a>> {
        if saved.source_namespace != self.instance {
            return Err(Error::InvalidHandle);
        }
        Ok(ExecutionCaptureIndex {
            kernel: self,
            saved,
        })
    }
    /// Historical sources are enrolled through their authentic callback witnesses,
    /// not as live operations. No execution or new reservation occurs here.
    pub fn execution_source_live(&self, source: &OperationBinding) -> Result<bool> {
        if validate_live_binding(self, source).is_ok() {
            return Ok(true);
        }
        let historical = self
            .retention
            .callbacks
            .borrow()
            .live
            .values()
            .filter_map(|w| w.upgrade())
            .any(|c| &c.provenance.source == source)
            || self.retention.pipelines.values().any(|p| {
                p.origin
                    .as_ref()
                    .is_some_and(|o| &o.provenance.source == source)
            });
        if historical {
            Ok(false)
        } else {
            Err(Error::InvalidHandle)
        }
    }
}
impl ExecutionCaptureIndex<'_> {
    pub fn namespace(&self) -> u64 {
        self.saved.source_namespace
    }
    pub fn module(&self, handle: ModuleHandle) -> Result<u64> {
        let artifact = self.kernel.artifact(handle)?;
        if !self
            .saved
            .registry
            .modules
            .iter()
            .any(|m| m.id == handle.generation && m.artifact == artifact)
        {
            return Err(Error::InvalidHandle);
        }
        Ok(handle.generation)
    }
    pub fn cleanup(&self, identity: CleanupIdentity) -> Result<u64> {
        if !self.saved.cleanup_history.contains(&identity.raw()) {
            return Err(Error::Authority);
        }
        Ok(identity.raw())
    }
    pub fn source(&self, source: &OperationBinding) -> Result<SavedOperation> {
        let row = operation(source);
        if self.kernel.execution_source_live(source)? {
            if !self.saved.operations.contains(&row) {
                return Err(Error::Authority);
            }
        } else if !self
            .saved
            .callbacks
            .iter()
            .map(|r| &r.pipeline)
            .chain(self.saved.leases.iter().map(|r| &r.pipeline))
            .any(|p| p.origin.as_ref().is_some_and(|o| o.source == row))
        {
            return Err(Error::Authority);
        }
        Ok(row)
    }
    pub fn pin(&self, pin: &ModulePin) -> Result<u64> {
        let pins = self.kernel.retention.pins.borrow();
        let id = pins
            .live
            .iter()
            .find_map(|(id, w)| w.upgrade().filter(|p| Rc::ptr_eq(p, &pin.0)).map(|_| *id))
            .ok_or(Error::InvalidHandle)?;
        if !self.saved.pins.iter().any(|p| p.id == id) {
            return Err(Error::Authority);
        }
        Ok(id)
    }
    pub fn pure(&self, capture: &PureCapture) -> Result<usize> {
        let pin = self.pin(&capture.pin)?;
        let row = SavedPureCapture {
            provider: self.module(capture.provider)?,
            slot: capture.slot,
            ceiling: capture.ceiling.clone(),
            pin,
            routing: routing(&capture.routing),
            closure: capture
                .closure
                .iter()
                .map(|(p, a)| SavedPermit {
                    capability: p.capability.clone(),
                    resolution: p.resolution.iter().map(binding).collect(),
                    ceiling: a.clone(),
                })
                .collect(),
        };
        let matches: Vec<_> = self
            .saved
            .pure_captures
            .iter()
            .enumerate()
            .filter(|(_, c)| **c == row)
            .collect();
        if matches.len() != 1 {
            return Err(Error::Authority);
        }
        Ok(matches[0].0)
    }
    pub fn callback(&self, callback: &RetainedCallback) -> Result<u64> {
        let callbacks = self.kernel.retention.callbacks.borrow();
        let id = callbacks
            .live
            .iter()
            .find_map(|(id, w)| {
                w.upgrade()
                    .filter(|c| Rc::ptr_eq(c, &callback.0))
                    .map(|_| *id)
            })
            .ok_or(Error::InvalidHandle)?;
        if !self.saved.callbacks.iter().any(|c| c.id == id) {
            return Err(Error::Authority);
        }
        Ok(id)
    }
}
impl RestoredKernel {
    /// Only sources admitted by complete D3b operation-context approval exist here.
    pub fn execution_source(&self, source: &SavedOperation) -> Result<OperationBinding> {
        self.source_bindings
            .iter()
            .find(|(s, _)| s == source)
            .map(|(_, b)| b.clone())
            .ok_or(Error::Authority)
    }
}

impl RestoredKernel {
    pub fn execution_pin(&self, id: u64, modules: &[ModuleHandle]) -> Result<ModulePin> {
        let pin = self.pins.get(&id).ok_or(Error::InvalidHandle)?;
        if pin.0.references != modules.iter().copied().collect() {
            return Err(Error::Authority);
        }
        Ok(pin.clone())
    }
}

impl RestoredKernel {
    pub fn execution_namespace(&self) -> u64 {
        self.execution_namespace
    }
    pub fn execution_resource(&self, saved: &Resource) -> Result<&Resource> {
        self.execution_resources.get(saved).ok_or(Error::Authority)
    }
    /// Correlate owner edges with the independently admitted complete source
    /// witness, not merely a capability name or a digest copied from the save.
    #[allow(clippy::too_many_arguments)]
    pub fn execution_behavior(
        &self,
        capture: usize,
        source: &SavedOperation,
        provider: u64,
        caller: u64,
        slot: usize,
        target: &Resource,
        invocation_pin: u64,
    ) -> Result<()> {
        let pure = self
            .execution_captures
            .get(capture)
            .ok_or(Error::Authority)?;
        if pure.provider != provider || pure.slot != slot {
            return Err(Error::Authority);
        }
        let contexts = self
            .execution_contexts
            .iter()
            .filter(|c| &c.operation == source);
        let context = contexts
            .into_iter()
            .find(|c| {
                pure.routing
                    .routes
                    .iter()
                    .all(|r| c.pipeline.routing.routes.contains(r))
                    && pure.routing.slots.iter().all(|s| {
                        c.pipeline.routing.slots.iter().any(|original| {
                            original.caller == s.caller
                                && original.slot == s.slot
                                && original.target == s.target
                                && match (&original.ceiling, &s.ceiling) {
                                    (Some(a), Some(b)) => a.contains(b),
                                    (None, None) => true,
                                    _ => false,
                                }
                        })
                    })
            })
            .ok_or(Error::Authority)?;
        if !context.pipeline.routing.slots.iter().any(|s| {
            s.caller == caller
                && s.ceiling.as_ref().is_some_and(|a| a.scope.allows(target))
                && context
                    .pipeline
                    .routing
                    .routes
                    .iter()
                    .any(|r| r.module == s.target && r.chain.iter().any(|b| b.module == provider))
        }) {
            return Err(Error::Authority);
        }
        self.execution_resource(target)?;
        let scoped = context.pipeline.ceiling.scope.allows(target);
        if !scoped {
            return Err(Error::Authority);
        }
        let pin = self.pins.get(&invocation_pin).ok_or(Error::InvalidHandle)?;
        if !pin
            .0
            .references
            .contains(self.modules.get(&provider).ok_or(Error::InvalidHandle)?)
            || !pin
                .0
                .references
                .contains(self.modules.get(&caller).ok_or(Error::InvalidHandle)?)
        {
            return Err(Error::Authority);
        }
        Ok(())
    }
}

/// Identity comparison for enrollment deduplication; no address is serialized.
pub fn execution_capture_alias(a: &PureCapture, b: &PureCapture) -> bool {
    Rc::ptr_eq(&a.pin.0, &b.pin.0) && Rc::ptr_eq(&a.routing, &b.routing)
}

// Local input ledgers require exact admitted write routes, not owner metadata or
// ambient current grants. This performs no invocation, sampling or reservation.
fn record_context<'a>(
    contexts: &'a [SavedOperationContext],
    source: &SavedOperation,
    caller: u64,
    capability: &Capability,
    chain: &[SavedBinding],
    target: &Resource,
) -> Result<&'a SavedOperationContext> {
    let context = contexts
        .iter()
        .find(|c| &c.operation == source)
        .ok_or(Error::Authority)?;
    if chain.is_empty()
        || context.pipeline.ceiling.access != Access::Write
        || !context.pipeline.ceiling.scope.allows(target)
        || !context.pipeline.routing.slots.iter().any(|slot| {
            slot.caller == caller
                && slot
                    .ceiling
                    .as_ref()
                    .is_some_and(|a| a.access == Access::Write && a.scope.allows(target))
                && context.pipeline.routing.routes.iter().any(|route| {
                    route.module == slot.target
                        && route.endpoint == capability.endpoint
                        && route.chain == chain
                })
        })
    {
        return Err(Error::Authority);
    }
    Ok(context)
}
impl KernelSnapshot {
    /// Structural correlation before an input factory is called. Full admission
    /// remains default-deny through RetainedAuthority's context approval.
    #[allow(clippy::too_many_arguments)]
    pub fn execution_record(
        &self,
        source: &SavedOperation,
        caller: u64,
        capability: &Capability,
        chain: &[SavedBinding],
        target: &Resource,
        pin: Option<u64>,
    ) -> Result<()> {
        let contexts = self.operation_contexts()?;
        let context = record_context(&contexts, source, caller, capability, chain, target)?;
        let first = chain.first().ok_or(Error::Authority)?;
        let module = self
            .registry
            .modules
            .iter()
            .find(|m| m.id == first.module)
            .ok_or(Error::InvalidHandle)?;
        let endpoint = module
            .manifest
            .exports
            .get(&capability.endpoint)
            .ok_or(Error::Authority)?;
        if module.manifest.id != capability.provider
            || endpoint.major != capability.major
            || endpoint.access != Access::Write
        {
            return Err(Error::Authority);
        }
        if let Some(id) = pin {
            let required: BTreeSet<_> = context
                .pipeline
                .routing
                .routes
                .iter()
                .flat_map(|r| r.chain.iter().map(|b| b.module))
                .chain([context.pipeline.root.module, caller])
                .collect();
            let pin = self
                .pins
                .iter()
                .find(|p| p.id == id)
                .ok_or(Error::Authority)?;
            if pin.modules.iter().copied().collect::<BTreeSet<_>>() != required {
                return Err(Error::Authority);
            }
        }
        Ok(())
    }
}
impl RestoredKernel {
    /// Recheck the independently approved execution witnesses against the live
    /// destination. This performs no invocation or domain construction.
    pub fn execution_live(&self, kernel: &Kernel) -> Result<()> {
        for context in &self.execution_contexts {
            self.execution_source(&context.operation)?;
            for witness in std::iter::once(&context.pipeline)
                .chain(context.retained.iter())
                .chain(context.origin.iter().map(|o| &o.source_pipeline))
            {
                for binding in std::iter::once(&witness.root)
                    .chain(witness.routing.routes.iter().flat_map(|r| r.chain.iter()))
                {
                    let handle = *self.modules.get(&binding.module).ok_or(Error::Authority)?;
                    if kernel.artifact(handle)? != binding.artifact
                        || !kernel
                            .registration(handle)?
                            .manifest
                            .exports
                            .contains_key(&binding.endpoint)
                    {
                        return Err(Error::Authority);
                    }
                }
                for slot in &witness.routing.slots {
                    let caller = *self.modules.get(&slot.caller).ok_or(Error::Authority)?;
                    let target = *self.modules.get(&slot.target).ok_or(Error::Authority)?;
                    let registration = kernel.registration(caller)?;
                    let dependency = registration
                        .manifest
                        .dependencies
                        .get(slot.slot)
                        .ok_or(Error::InvalidSlot)?;
                    if registration.dependencies.get(slot.slot) != Some(&target) {
                        return Err(Error::MissingDependency);
                    }
                    if let Some(saved) = &slot.ceiling {
                        let authority = Authority {
                            access: saved.access,
                            scope: match &saved.scope {
                                Scope::All => Scope::All,
                                Scope::Keys(keys) => Scope::Keys(
                                    keys.iter()
                                        .map(|r| self.execution_resource(r).cloned())
                                        .collect::<Result<_>>()?,
                                ),
                            },
                        };
                        let grant = kernel
                            .grants
                            .get(&(caller, slot.slot))
                            .ok_or(Error::MissingGrant)?;
                        if !grant.active {
                            return Err(Error::Revoked);
                        }
                        if grant.target != target
                            || !grant.authority.contains(&authority)
                            || authority.access > dependency.access
                        {
                            return Err(Error::Authority);
                        }
                    }
                }
            }
        }
        Ok(())
    }
    /// Same correlation against the independently approved contexts and fresh
    /// routes. Incoming IDs never confer authority to a restored observation.
    #[allow(clippy::too_many_arguments)]
    pub fn execution_record(
        &self,
        source: &SavedOperation,
        caller: u64,
        capability: &Capability,
        chain: &[ProviderBinding],
        target: &Resource,
        pin: Option<u64>,
    ) -> Result<()> {
        let saved_chain = chain
            .iter()
            .map(|b| {
                let module = self
                    .modules
                    .iter()
                    .find_map(|(id, m)| (*m == b.handle).then_some(*id))
                    .ok_or(Error::Authority)?;
                Ok(SavedBinding {
                    module,
                    artifact: b.artifact,
                    endpoint: b.endpoint.clone(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let context = record_context(
            &self.execution_contexts,
            source,
            caller,
            capability,
            &saved_chain,
            target,
        )?;
        self.execution_resource(target)?;
        self.execution_source(source)?;
        if let Some(id) = pin {
            let required = context
                .pipeline
                .routing
                .routes
                .iter()
                .flat_map(|r| r.chain.iter().map(|b| b.module))
                .chain([context.pipeline.root.module, caller])
                .map(|id| self.modules.get(&id).copied().ok_or(Error::Authority))
                .collect::<Result<BTreeSet<_>>>()?;
            if self.pins.get(&id).ok_or(Error::Authority)?.0.references != required {
                return Err(Error::Authority);
            }
        }
        Ok(())
    }
}
impl KernelSnapshot {
    /// Owner cross-table preflight before any native artifact factory. These are
    /// untrusted metadata checks; D3b still requires independent context approval.
    #[allow(clippy::too_many_arguments)]
    pub fn execution_behavior(
        &self,
        capture: usize,
        source: &SavedOperation,
        provider: u64,
        caller: u64,
        slot: usize,
        target: &Resource,
        invocation_pin: u64,
    ) -> Result<()> {
        let pure = self.pure_captures.get(capture).ok_or(Error::Authority)?;
        if pure.provider != provider || pure.slot != slot {
            return Err(Error::Authority);
        }
        let contexts = self.operation_contexts()?;
        let context = contexts
            .iter()
            .find(|c| {
                &c.operation == source
                    && pure
                        .routing
                        .routes
                        .iter()
                        .all(|r| c.pipeline.routing.routes.contains(r))
                    && pure.routing.slots.iter().all(|s| {
                        c.pipeline.routing.slots.iter().any(|original| {
                            original.caller == s.caller
                                && original.slot == s.slot
                                && original.target == s.target
                                && match (&original.ceiling, &s.ceiling) {
                                    (Some(a), Some(b)) => a.contains(b),
                                    (None, None) => true,
                                    _ => false,
                                }
                        })
                    })
            })
            .ok_or(Error::Authority)?;
        if !context.pipeline.routing.slots.iter().any(|s| {
            s.caller == caller
                && s.ceiling.as_ref().is_some_and(|a| a.scope.allows(target))
                && context
                    .pipeline
                    .routing
                    .routes
                    .iter()
                    .any(|r| r.module == s.target && r.chain.iter().any(|b| b.module == provider))
        }) || !context.pipeline.ceiling.scope.allows(target)
        {
            return Err(Error::Authority);
        }
        let required: BTreeSet<_> = context
            .pipeline
            .routing
            .routes
            .iter()
            .flat_map(|r| r.chain.iter().map(|b| b.module))
            .chain([provider, caller])
            .collect();
        let pin = self
            .pins
            .iter()
            .find(|p| p.id == invocation_pin)
            .ok_or(Error::InvalidHandle)?;
        if pin.modules.iter().copied().collect::<BTreeSet<_>>() != required {
            return Err(Error::Authority);
        }
        Ok(())
    }
}

impl KernelSnapshot {
    /// Owner read/write correlation before domain codecs or factories.
    #[allow(clippy::too_many_arguments)]
    pub fn execution_owner(
        &self,
        source: &SavedOperation,
        caller: u64,
        capability: &Capability,
        chain: &[SavedBinding],
        target: &Resource,
        pin: Option<u64>,
        access: Access,
    ) -> Result<()> {
        let contexts = self.operation_contexts()?;
        let context = owner_context(&contexts, source, caller, capability, chain, target, access)?;
        let first = chain.first().ok_or(Error::Authority)?;
        let module = self
            .registry
            .modules
            .iter()
            .find(|m| m.id == first.module)
            .ok_or(Error::InvalidHandle)?;
        let endpoint = module
            .manifest
            .exports
            .get(&capability.endpoint)
            .ok_or(Error::Authority)?;
        if module.manifest.id != capability.provider
            || endpoint.major != capability.major
            || endpoint.access != access
        {
            return Err(Error::Authority);
        }
        if let Some(id) = pin {
            let required: BTreeSet<_> = context
                .pipeline
                .routing
                .routes
                .iter()
                .flat_map(|r| r.chain.iter().map(|b| b.module))
                .chain([context.pipeline.root.module, caller])
                .collect();
            let pin = self
                .pins
                .iter()
                .find(|p| p.id == id)
                .ok_or(Error::Authority)?;
            if pin.modules.iter().copied().collect::<BTreeSet<_>>() != required {
                return Err(Error::Authority);
            }
        }
        Ok(())
    }
}

fn owner_context<'a>(
    contexts: &'a [SavedOperationContext],
    source: &SavedOperation,
    caller: u64,
    capability: &Capability,
    chain: &[SavedBinding],
    target: &Resource,
    access: Access,
) -> Result<&'a SavedOperationContext> {
    let context = contexts
        .iter()
        .find(|c| &c.operation == source)
        .ok_or(Error::Authority)?;
    if chain.is_empty()
        || (context.pipeline.ceiling.access != Access::Write && access == Access::Write)
        || !context.pipeline.ceiling.scope.allows(target)
        || !context.pipeline.routing.slots.iter().any(|slot| {
            slot.caller == caller
                && slot.ceiling.as_ref().is_some_and(|a| {
                    (a.access == Access::Write || access == Access::Read) && a.scope.allows(target)
                })
                && context.pipeline.routing.routes.iter().any(|route| {
                    route.module == slot.target
                        && route.endpoint == capability.endpoint
                        && route.chain == chain
                })
        })
    {
        return Err(Error::Authority);
    }
    Ok(context)
}
impl RestoredKernel {
    /// Correlate read/write owner evidence against admitted fresh contexts.
    #[allow(clippy::too_many_arguments)]
    pub fn execution_owner(
        &self,
        source: &SavedOperation,
        caller: u64,
        capability: &Capability,
        chain: &[ProviderBinding],
        target: &Resource,
        pin: Option<u64>,
        access: Access,
    ) -> Result<()> {
        let saved_chain = chain
            .iter()
            .map(|b| {
                let module = self
                    .modules
                    .iter()
                    .find_map(|(id, m)| (*m == b.handle).then_some(*id))
                    .ok_or(Error::Authority)?;
                Ok(SavedBinding {
                    module,
                    artifact: b.artifact,
                    endpoint: b.endpoint.clone(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let context = owner_context(
            &self.execution_contexts,
            source,
            caller,
            capability,
            &saved_chain,
            target,
            access,
        )?;
        self.execution_resource(target)?;
        self.execution_source(source)?;
        if let Some(id) = pin {
            let required = context
                .pipeline
                .routing
                .routes
                .iter()
                .flat_map(|r| r.chain.iter().map(|b| b.module))
                .chain([context.pipeline.root.module, caller])
                .map(|id| self.modules.get(&id).copied().ok_or(Error::Authority))
                .collect::<Result<BTreeSet<_>>>()?;
            if self.pins.get(&id).ok_or(Error::Authority)?.0.references != required {
                return Err(Error::Authority);
            }
        }
        Ok(())
    }
}

/// Timer-owned provenance history. This record supplies no executable credential.
/// Cancelled records can outlive their source, code registration and callback.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimerHistory {
    pub caller: SavedBinding,
    pub service: SavedBinding,
    pub capability: Capability,
    pub slot: usize,
    pub source: SavedOperation,
}
impl ExecutionCaptureIndex<'_> {
    pub fn timer_history(&self, p: &CallbackProvenance) -> Result<TimerHistory> {
        if p.caller.handle.instance != self.namespace()
            || p.service.handle.instance != self.namespace()
            || p.source.operation.host != self.namespace()
            || p.source.pipeline.instance != self.namespace()
        {
            return Err(Error::InvalidHandle);
        }
        let row = TimerHistory {
            caller: SavedBinding {
                module: p.caller.handle.generation,
                artifact: p.caller.artifact,
                endpoint: p.caller.endpoint.clone(),
            },
            service: SavedBinding {
                module: p.service.handle.generation,
                artifact: p.service.artifact,
                endpoint: p.service.endpoint.clone(),
            },
            capability: p.capability.clone(),
            slot: p.slot,
            source: operation(&p.source),
        };
        row.check(self.saved)?;
        Ok(row)
    }
}
impl TimerHistory {
    pub fn check(&self, saved: &KernelSnapshot) -> Result<()> {
        if self.caller.module == 0
            || self.caller.module >= saved.registry.next_generation
            || self.service.module == 0
            || self.service.module >= saved.registry.next_generation
            || self.source.operation == 0
            || self.source.operation >= saved.registry.next_operation
            || self.source.pipeline == 0
            || self.source.pipeline >= saved.registry.retention_counters[0]
            || !Kernel::valid_name(&self.caller.endpoint)
            || !Kernel::valid_name(&self.service.endpoint)
            || self.source.phase.len() > 256
            || self
                .source
                .cleanup
                .iter()
                .any(|id| !saved.cleanup_history.contains(id))
            || self.source.cleanup.windows(2).any(|w| w[0] >= w[1])
        {
            return Err(Error::InvalidHandle);
        }
        Ok(())
    }
}
impl RestoredKernel {
    /// Cancelled diagnostic provenance deliberately uses namespace zero, which
    /// no Kernel ever issues. It cannot authorize invocation, capture or release.
    /// Live tasks instead obtain provenance from admitted callbacks/leases.
    pub fn terminal_timer_history(&self, history: &TimerHistory) -> Result<CallbackProvenance> {
        let binding = |b: &SavedBinding| ProviderBinding {
            handle: ModuleHandle {
                instance: 0,
                generation: b.module,
            },
            artifact: b.artifact,
            endpoint: b.endpoint.clone(),
        };
        Ok(CallbackProvenance {
            caller: binding(&history.caller),
            service: binding(&history.service),
            capability: history.capability.clone(),
            slot: history.slot,
            source: OperationBinding {
                operation: OperationId {
                    host: 0,
                    serial: history.source.operation,
                },
                pipeline: PipelineLease {
                    instance: 0,
                    serial: history.source.pipeline,
                },
                checkpoint: history.source.checkpoint,
                phase: history.source.phase.clone(),
                cleanup: history
                    .source
                    .cleanup
                    .iter()
                    .map(|id| self.cleanup.get(id).copied().ok_or(Error::Authority))
                    .collect::<Result<_>>()?,
            },
        })
    }
    /// One-use enrollment for an entire timer allocation, including an empty
    /// queue and cancelled tombstones. Failure releases the hidden reservation.
    pub fn enroll_timers(&self, service: u64) -> Result<WorkflowEnrollment> {
        if !self.modules.contains_key(&service) {
            return Err(Error::InvalidHandle);
        }
        let mut occupied = self
            .timer_enrollment
            .try_borrow_mut()
            .map_err(|_| Error::NativeRefusal)?;
        if !occupied.insert(service) {
            return Err(Error::Duplicate);
        }
        Ok(WorkflowEnrollment {
            occupied: self.timer_enrollment.clone(),
            operations: vec![service],
            complete: false,
        })
    }
}

impl Kernel {
    /// Additional authentic host service cleanup identities are snapshot-only
    /// roots. Capture does not enroll or mutate the runtime's cleanup history.
    pub fn kernel_snapshot_with_cleanup(
        &mut self,
        enrollment: &KernelEnrollment<'_>,
        cleanup: &[CleanupIdentity],
    ) -> Result<KernelSnapshot> {
        if cleanup.len() > MAX_ROWS {
            return Err(Error::Limit);
        }
        let mut saved = self.kernel_snapshot(enrollment)?;
        saved
            .cleanup_history
            .extend(cleanup.iter().map(|id| id.raw()));
        saved
            .service_cleanup
            .extend(cleanup.iter().map(|id| id.raw()));
        if saved.cleanup_history.len() > MAX_ROWS {
            return Err(Error::Limit);
        }
        Ok(saved)
    }
}
impl RestoredKernel {
    pub fn enroll_owner_adapter(&self, service: u64) -> Result<WorkflowEnrollment> {
        if !self.modules.contains_key(&service) {
            return Err(Error::InvalidHandle);
        }
        let mut occupied = self
            .adapter_enrollment
            .try_borrow_mut()
            .map_err(|_| Error::NativeRefusal)?;
        if !occupied.insert(service) {
            return Err(Error::Duplicate);
        }
        Ok(WorkflowEnrollment {
            occupied: self.adapter_enrollment.clone(),
            operations: vec![service],
            complete: false,
        })
    }
}

impl Kernel {
    /// Snapshot-only local service metadata roots, admitted by the independent
    /// destination resolver. No grants or source runtime history are created.
    pub fn kernel_snapshot_with_service_roots(
        &mut self,
        enrollment: &KernelEnrollment<'_>,
        cleanup: &[CleanupIdentity],
        resources: &[Resource],
    ) -> Result<KernelSnapshot> {
        if resources.len() > MAX_ROWS {
            return Err(Error::Limit);
        }
        let mut saved = self.kernel_snapshot_with_cleanup(enrollment, cleanup)?;
        saved.service_resources.extend(resources.iter().cloned());
        saved.resources()?;
        if crate::kernel::persistence::bounded_size(&saved)? > MAX_BYTES {
            return Err(Error::Limit);
        }
        Ok(saved)
    }
}
