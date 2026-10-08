//! Private routing and reference ownership; publication follows complete preflight.
use super::*;
use std::collections::BTreeSet;

#[cfg(test)]
#[path = "callbacks_tests.rs"]
mod callbacks_tests;

#[path = "persistence/retained.rs"]
pub mod persistence;

type RouteKey = (ModuleHandle, String);

#[derive(Debug)]
pub(crate) struct RetainedSlot {
    pub target: ModuleHandle,
    pub ceiling: Option<Authority>,
}
#[derive(Debug)]
pub(crate) struct Routing {
    routes: BTreeMap<RouteKey, Vec<ProviderBinding>>,
    pub slots: BTreeMap<(ModuleHandle, usize), RetainedSlot>,
}
#[derive(Clone, Debug)]
struct Pipeline {
    descriptor: PipelineDescriptor,
    routing: Rc<Routing>,
    references: BTreeSet<ModuleHandle>,
    origin: Option<Rc<CallbackOrigin>>,
}
#[derive(Debug)]
struct PipelineWitness {
    descriptor: PipelineDescriptor,
    routing: Rc<Routing>,
    references: BTreeSet<ModuleHandle>,
}
#[derive(Debug)]
struct CallbackOrigin {
    caller: ModuleHandle,
    slot: usize,
    target: ModuleHandle,
    ceiling: Authority,
    provenance: CallbackProvenance,
    service_route: Vec<ProviderBinding>,
    source_pipeline: PipelineWitness,
}
#[derive(Debug)]
pub(crate) struct CallbackData {
    host: u64,
    pipeline: Pipeline,
    pub(crate) provenance: CallbackProvenance,
}
struct Callbacks {
    next: u64,
    live: BTreeMap<u64, std::rc::Weak<CallbackData>>,
}
#[derive(Debug)]
pub(crate) struct PinData {
    references: BTreeSet<ModuleHandle>,
}
impl ModulePin {
    pub fn module_count(&self) -> usize {
        self.0.references.len()
    }
}
struct Pins {
    next: u64,
    live: BTreeMap<u64, std::rc::Weak<PinData>>,
}
pub(super) struct Retention {
    limits: RetentionLimits,
    next_serial: u64,
    pipelines: BTreeMap<PipelineLease, Pipeline>,
    owners: BTreeMap<OwnerReference, BTreeSet<ModuleHandle>>,
    callbacks: RefCell<Callbacks>,
    pins: RefCell<Pins>,
    cleanup_history: RefCell<BTreeSet<u64>>,
}
impl Default for Retention {
    fn default() -> Self {
        Self {
            limits: RetentionLimits::default(),
            next_serial: 1,
            cleanup_history: RefCell::new(BTreeSet::new()),
            pipelines: BTreeMap::new(),
            owners: BTreeMap::new(),
            callbacks: RefCell::new(Callbacks {
                next: 1,
                live: BTreeMap::new(),
            }),
            pins: RefCell::new(Pins {
                next: 1,
                live: BTreeMap::new(),
            }),
        }
    }
}

impl Retention {
    pub(super) fn empty_history(&self) -> Result<(RetentionLimits, [u64; 3])> {
        if !self.pipelines.is_empty()
            || !self.owners.is_empty()
            || self
                .callbacks
                .borrow()
                .live
                .values()
                .any(|v| v.strong_count() != 0)
            || self
                .pins
                .borrow()
                .live
                .values()
                .any(|v| v.strong_count() != 0)
        {
            return Err(Error::Referenced);
        }
        Ok(self.history())
    }

    pub(super) fn history(&self) -> (RetentionLimits, [u64; 3]) {
        (
            self.limits.clone(),
            [
                self.next_serial,
                self.callbacks.borrow().next,
                self.pins.borrow().next,
            ],
        )
    }

    pub(super) fn restore_empty_history(
        &mut self,
        limits: RetentionLimits,
        counters: [u64; 3],
    ) -> Result<()> {
        if counters.contains(&0) {
            return Err(Error::Limit);
        }
        self.empty_history()?;
        self.limits = limits;
        self.next_serial = counters[0];
        self.callbacks.borrow_mut().next = counters[1];
        self.pins.borrow_mut().next = counters[2];
        Ok(())
    }
}

struct Capture<'a> {
    kernel: &'a Kernel,
    ceiling: &'a Authority,
    routing: Routing,
    visiting: BTreeSet<RouteKey>,
    references: BTreeSet<ModuleHandle>,
    nodes: usize,
    source: Option<&'a Routing>,
}

pub(super) struct CallbackFactory<'a> {
    pub kernel: &'a Kernel,
    pub caller: Option<ModuleHandle>,
    pub service: ProviderBinding,
    pub path: Option<usize>,
    pub authority: &'a Authority,
    pub operation: Option<&'a OperationBinding>,
    pub routing: Option<&'a Routing>,
}
impl std::fmt::Debug for CallbackFactory<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CallbackCapture")
    }
}
impl CallbackCapture for CallbackFactory<'_> {
    fn capture(&self, endpoint: &str, ceiling: Authority) -> Result<RetainedCallback> {
        let k = self.kernel;
        let caller = self.caller.ok_or(Error::Authority)?;
        let slot = self.path.ok_or(Error::Authority)?;
        let source = self.operation.ok_or(Error::Authority)?;
        if endpoint.is_empty() || endpoint.len() > k.limits.payload_bytes as usize {
            return Err(Error::Buffer);
        }
        k.validate_authority(&ceiling, false)?;
        if !self.authority.contains(&ceiling) {
            return Err(Error::Authority);
        }
        let dep = k
            .registration(caller)?
            .manifest
            .dependencies
            .get(slot)
            .ok_or(Error::InvalidSlot)?;
        let (target, _) = k.capability(&dep.capability)?;
        let route = k.resolution(target, &dep.capability.endpoint, self.routing)?;
        if route.last() != Some(&self.service) {
            return Err(Error::Authority);
        }
        let provenance = CallbackProvenance {
            caller: ProviderBinding {
                handle: caller,
                artifact: k.artifact(caller)?,
                endpoint: endpoint.into(),
            },
            service: self.service.clone(),
            capability: dep.capability.clone(),
            slot,
            source: source.clone(),
        };
        let source_pipeline = k
            .retention
            .pipelines
            .get(&source.pipeline)
            .ok_or(Error::InvalidHandle)?;
        let source_pipeline = PipelineWitness {
            descriptor: source_pipeline.descriptor.clone(),
            routing: source_pipeline.routing.clone(),
            references: source_pipeline
                .routing
                .routes
                .values()
                .flatten()
                .map(|b| b.handle)
                .collect(),
        };
        let origin = Rc::new(CallbackOrigin {
            caller,
            slot,
            target,
            ceiling: ceiling.clone(),
            provenance: provenance.clone(),
            service_route: route.clone(),
            source_pipeline,
        });
        k.validate_callback_origin(&origin)?;
        {
            let callbacks = k
                .retention
                .callbacks
                .try_borrow()
                .map_err(|_| Error::NativeRefusal)?;
            if callbacks
                .live
                .values()
                .filter(|w| w.strong_count() != 0)
                .count()
                >= k.retention.limits.owners
            {
                return Err(Error::Limit);
            }
            callbacks.next.checked_add(1).ok_or(Error::Limit)?;
        }
        let mut pipeline = k.build_pipeline(caller, endpoint, ceiling, self.routing)?;
        pipeline.references.insert(caller);
        pipeline
            .references
            .extend(origin.source_pipeline.references.iter().copied());
        for binding in route {
            pipeline.references.insert(binding.handle);
        }
        if pipeline.references.len() > k.retention.limits.nodes {
            return Err(Error::Limit);
        }
        k.reference_capacity(pipeline.references.len())?;
        pipeline.origin = Some(origin);
        let data = Rc::new(CallbackData {
            host: k.instance,
            pipeline,
            provenance,
        });
        let mut callbacks = k
            .retention
            .callbacks
            .try_borrow_mut()
            .map_err(|_| Error::NativeRefusal)?;
        let serial = callbacks.next;
        let next = serial.checked_add(1).ok_or(Error::Limit)?;
        callbacks.live.retain(|_, w| w.strong_count() != 0);
        callbacks.live.insert(serial, Rc::downgrade(&data));
        callbacks.next = next;
        Ok(RetainedCallback(data))
    }
}
impl Capture<'_> {
    fn visit(&mut self, key: RouteKey, depth: u32) -> Result<()> {
        if self.visiting.contains(&key) {
            return Err(Error::Cycle);
        }
        if self.routing.routes.contains_key(&key) {
            return Ok(());
        }
        if depth > self.kernel.limits.depth {
            return Err(Error::Limit);
        }
        // Imports already resolved by this invocation keep that executable chain.
        // A separate public callback export absent from the source closure resolves now.
        let source = self.source.filter(|r| r.routes.contains_key(&key));
        let chain = self.kernel.resolution(key.0, &key.1, source)?;
        self.nodes = self.nodes.checked_add(chain.len()).ok_or(Error::Limit)?;
        if self.nodes > self.kernel.retention.limits.nodes {
            return Err(Error::Limit);
        }
        self.visiting.insert(key.clone());
        for binding in &chain {
            self.references.insert(binding.handle);
            let reg = self.kernel.registration(binding.handle)?;
            for (slot, dep) in reg.manifest.dependencies.iter().enumerate() {
                let (target, _) = self.kernel.capability(&dep.capability)?;
                let mut ceiling = match self.kernel.grants.get(&(binding.handle, slot)) {
                    Some(g) if g.active => {
                        if g.target != target {
                            return Err(Error::InvalidHandle);
                        }
                        Some(self.ceiling.intersect(&g.authority).intersect(&Authority {
                            access: dep.access,
                            scope: Scope::All,
                        }))
                    }
                    _ => None,
                };
                if let Some(source) = self.source {
                    if let Some(old) = source.slots.get(&(binding.handle, slot)) {
                        if old.target != target {
                            return Err(Error::InvalidHandle);
                        }
                        ceiling = ceiling
                            .zip(old.ceiling.as_ref())
                            .map(|(live, old)| live.intersect(old));
                    }
                }
                self.routing
                    .slots
                    .insert((binding.handle, slot), RetainedSlot { target, ceiling });
                if self.routing.slots.len() > self.kernel.retention.limits.slots {
                    return Err(Error::Limit);
                }
                // Includes permission-only Read slots to Write owner ports, and
                // absent grants. The latter retain code but confer no authority.
                self.visit((target, dep.capability.endpoint.clone()), depth + 1)?;
            }
        }
        self.visiting.remove(&key);
        self.routing.routes.insert(key, chain);
        Ok(())
    }
}

impl Kernel {
    fn validate_callback_origin(&self, origin: &CallbackOrigin) -> Result<()> {
        let dep = self
            .registration(origin.caller)?
            .manifest
            .dependencies
            .get(origin.slot)
            .ok_or(Error::InvalidSlot)?;
        let grant = self
            .grants
            .get(&(origin.caller, origin.slot))
            .ok_or(Error::MissingGrant)?;
        if !grant.active {
            return Err(Error::Revoked);
        }
        let (target, _) = self.capability(&dep.capability)?;
        if target != origin.target || target != grant.target {
            return Err(Error::InvalidHandle);
        }
        let live = grant.authority.intersect(&Authority {
            access: dep.access,
            scope: Scope::All,
        });
        if !live.contains(&origin.ceiling) {
            return Err(Error::Authority);
        }
        Ok(())
    }

    /// Trusted host safe boundary: fresh identities over the captured route, without dispatch recapture.
    pub fn adopt_callback(
        &mut self,
        callback: &RetainedCallback,
    ) -> Result<(OperationId, PipelineLease)> {
        let data = &callback.0;
        if data.host != self.instance {
            return Err(Error::InvalidHandle);
        }
        self.validate_callback_origin(data.pipeline.origin.as_ref().ok_or(Error::Authority)?)?;
        for &handle in &data.pipeline.references {
            self.registration(handle)?;
        }
        let next_operation = self.next_operation.checked_add(1).ok_or(Error::Limit)?;
        let next_serial = self
            .retention
            .next_serial
            .checked_add(1)
            .ok_or(Error::Limit)?;
        if self.retention.pipelines.len() >= self.retention.limits.leases {
            return Err(Error::Limit);
        }
        self.reference_capacity(data.pipeline.references.len())?;
        let lease = PipelineLease {
            instance: self.instance,
            serial: self.retention.next_serial,
        };
        let operation = OperationId {
            host: self.instance,
            serial: self.next_operation,
        };
        self.retention
            .pipelines
            .insert(lease, data.pipeline.clone());
        self.retention.next_serial = next_serial;
        self.next_operation = next_operation;
        Ok((operation, lease))
    }

    pub(super) fn resolution(
        &self,
        target: ModuleHandle,
        endpoint: &str,
        routing: Option<&Routing>,
    ) -> Result<Vec<ProviderBinding>> {
        if let Some(routing) = routing {
            let route = routing
                .routes
                .get(&(target, endpoint.to_owned()))
                .ok_or(Error::MissingDependency)?;
            for binding in route {
                if self.artifact(binding.handle)? != binding.artifact {
                    return Err(Error::InvalidHandle);
                }
            }
            return Ok(route.clone());
        }
        let mut target = target;
        let mut endpoint = endpoint.to_owned();
        let mut route = Vec::new();
        loop {
            if route.len() > self.limits.depth as usize {
                return Err(Error::Limit);
            }
            let reg = self.registration(target)?;
            if !reg.manifest.exports.contains_key(&endpoint) {
                return Err(Error::MissingDependency);
            }
            route.push(ProviderBinding {
                handle: target,
                artifact: reg.artifact,
                endpoint: endpoint.clone(),
            });
            let Some((next, export)) = self.extensions.get(&(target, endpoint)) else {
                break;
            };
            target = *next;
            endpoint = export.clone();
        }
        Ok(route)
    }

    /// Configure explicit bounds only while no retained references exist.
    pub fn set_retention_limits(&mut self, limits: RetentionLimits) -> Result<()> {
        if limits.leases == 0
            || limits.owners == 0
            || limits.nodes == 0
            || limits.slots == 0
            || limits.references == 0
        {
            return Err(Error::Limit);
        }
        if self.lifecycle_state().references != 0 {
            return Err(Error::Referenced);
        }
        self.retention.limits = limits;
        Ok(())
    }

    pub fn lifecycle_state(&self) -> LifecycleState {
        let callbacks = self.retention.callbacks.borrow();
        let live: Vec<_> = callbacks
            .live
            .values()
            .filter_map(std::rc::Weak::upgrade)
            .collect();
        LifecycleState {
            registrations: self.registrations.len(),
            attachments: self.extensions.len(),
            grants: self.grants.len(),
            leases: self.retention.pipelines.len(),
            owners: self.retention.owners.len()
                + self
                    .retention
                    .pins
                    .borrow()
                    .live
                    .values()
                    .filter(|p| p.strong_count() > 0)
                    .count(),
            references: self
                .retention
                .pipelines
                .values()
                .map(|p| p.references.len())
                .sum::<usize>()
                + self
                    .retention
                    .owners
                    .values()
                    .map(BTreeSet::len)
                    .sum::<usize>()
                + live
                    .iter()
                    .map(|c| c.pipeline.references.len())
                    .sum::<usize>()
                + self
                    .retention
                    .pins
                    .borrow()
                    .live
                    .values()
                    .filter_map(std::rc::Weak::upgrade)
                    .map(|p| p.references.len())
                    .sum::<usize>(),
            next_generation: self.next_generation,
            next_retention: self.retention.next_serial,
            callbacks: live.len(),
            next_callback: callbacks.next,
        }
    }

    fn reference_capacity(&self, additional: usize) -> Result<()> {
        if self
            .lifecycle_state()
            .references
            .checked_add(additional)
            .ok_or(Error::Limit)?
            > self.retention.limits.references
        {
            return Err(Error::Limit);
        }
        Ok(())
    }

    /// Capture the complete declared execution/inspection closure without running code.
    pub fn capture_pipeline(
        &mut self,
        provider: ModuleHandle,
        endpoint: &str,
        ceiling: Authority,
    ) -> Result<PipelineLease> {
        self.validate_authority(&ceiling, false)?;
        let ep = self
            .registration(provider)?
            .manifest
            .exports
            .get(endpoint)
            .ok_or(Error::MissingDependency)?;
        if ceiling.access < ep.access {
            return Err(Error::Authority);
        }
        if self.retention.pipelines.len() >= self.retention.limits.leases {
            return Err(Error::Limit);
        }
        let next = self
            .retention
            .next_serial
            .checked_add(1)
            .ok_or(Error::Limit)?;
        let pipeline = self.build_pipeline(provider, endpoint, ceiling, None)?;
        self.reference_capacity(pipeline.references.len())?;
        let lease = PipelineLease {
            instance: self.instance,
            serial: self.retention.next_serial,
        };
        self.retention.pipelines.insert(lease, pipeline);
        self.retention.next_serial = next;
        Ok(lease)
    }

    fn build_pipeline(
        &self,
        provider: ModuleHandle,
        endpoint: &str,
        ceiling: Authority,
        source: Option<&Routing>,
    ) -> Result<Pipeline> {
        self.validate_authority(&ceiling, false)?;
        let ep = self
            .registration(provider)?
            .manifest
            .exports
            .get(endpoint)
            .ok_or(Error::MissingDependency)?;
        if ceiling.access < ep.access {
            return Err(Error::Authority);
        }
        let mut capture = Capture {
            kernel: self,
            ceiling: &ceiling,
            routing: Routing {
                routes: BTreeMap::new(),
                slots: BTreeMap::new(),
            },
            visiting: BTreeSet::new(),
            references: BTreeSet::new(),
            nodes: 0,
            source,
        };
        capture.visit((provider, endpoint.to_owned()), 1)?;

        let root = ProviderBinding {
            handle: provider,
            artifact: self.artifact(provider)?,
            endpoint: endpoint.to_owned(),
        };
        let routes: Vec<_> = capture.routing.routes.values().cloned().collect();
        // Canonical ordered runtime-local descriptor includes slot ceilings and
        // root identity, not just executable bytes. Identities are not a codec.
        let hash = pipeline_digest(&root, &ceiling, &routes, &capture.routing)?;
        let descriptor = PipelineDescriptor {
            root,
            ceiling: ceiling.clone(),
            routes,
            hash,
        };
        Ok(Pipeline {
            descriptor,
            routing: Rc::new(capture.routing),
            references: capture.references,
            origin: None,
        })
    }

    pub fn pipeline_descriptor(&self, lease: PipelineLease) -> Result<&PipelineDescriptor> {
        Ok(&self
            .retention
            .pipelines
            .get(&lease)
            .ok_or(Error::InvalidHandle)?
            .descriptor)
    }

    /// Immutable creation provenance survives release of every capture handle.
    pub fn callback_provenance(&self, lease: PipelineLease) -> Result<Option<&CallbackProvenance>> {
        let pipeline = self
            .retention
            .pipelines
            .get(&lease)
            .ok_or(Error::InvalidHandle)?;
        Ok(pipeline.origin.as_ref().map(|origin| &origin.provenance))
    }

    pub fn invoke_pipeline(
        &self,
        lease: PipelineLease,
        request: &[u8],
        authority: Authority,
    ) -> Result<Vec<u8>> {
        self.invoke_pipeline_context(lease, request, authority, None)
    }

    fn invoke_pipeline_context(
        &self,
        lease: PipelineLease,
        request: &[u8],
        authority: Authority,
        operation: Option<OperationBinding>,
    ) -> Result<Vec<u8>> {
        self.preflight_pipeline(lease, request, &authority)?;
        let pipeline = self
            .retention
            .pipelines
            .get(&lease)
            .ok_or(Error::InvalidHandle)?;
        if !pipeline.descriptor.ceiling.contains(&authority) {
            return Err(Error::Authority);
        }
        let budget = Rc::new(RefCell::new(Budget {
            fuel: self.limits.fuel,
            calls: 0,
        }));
        self.charge(&budget, 1)?;
        self.dispatch_inner(
            pipeline.descriptor.root.handle,
            &pipeline.descriptor.root.endpoint,
            request,
            Invocation {
                caller: None,
                authority,
                depth: 1,
                routing: Some(pipeline.routing.clone()),
                operation,
                path: None,
                pure: false,
            },
            &budget,
        )
    }

    pub fn release_pipeline(&mut self, lease: PipelineLease) -> Result<()> {
        self.retention
            .pipelines
            .remove(&lease)
            .ok_or(Error::InvalidHandle)?;
        Ok(())
    }

    /// Bounded host reference ownership; no schema/service type is embedded here.
    pub fn retain_owner(&mut self, handles: &[ModuleHandle]) -> Result<OwnerReference> {
        if handles.is_empty()
            || handles.len() > self.retention.limits.nodes
            || self.lifecycle_state().owners >= self.retention.limits.owners
        {
            return Err(Error::Limit);
        }
        let next = self
            .retention
            .next_serial
            .checked_add(1)
            .ok_or(Error::Limit)?;
        let mut references = BTreeSet::new();
        for &handle in handles {
            self.registration(handle)?;
            references.insert(handle);
        }
        self.reference_capacity(references.len())?;
        let owner = OwnerReference {
            instance: self.instance,
            serial: self.retention.next_serial,
        };
        self.retention.owners.insert(owner, references);
        self.retention.next_serial = next;
        Ok(owner)
    }

    pub fn release_owner(&mut self, owner: OwnerReference) -> Result<()> {
        self.retention
            .owners
            .remove(&owner)
            .ok_or(Error::InvalidHandle)?;
        Ok(())
    }

    /// RAII module references for optional owner-local storage and prepared work.
    /// This pins code only; it confers no invocation authority.
    pub fn pin_modules(&self, handles: &[ModuleHandle]) -> Result<ModulePin> {
        if handles.is_empty() || handles.len() > self.retention.limits.nodes {
            return Err(Error::Limit);
        }
        let references: BTreeSet<_> = handles.iter().copied().collect();
        for handle in &references {
            self.registration(*handle)?;
        }
        if self.lifecycle_state().owners >= self.retention.limits.owners {
            return Err(Error::Limit);
        }
        self.reference_capacity(references.len())?;
        let mut pins = self.retention.pins.borrow_mut();
        let next = pins.next.checked_add(1).ok_or(Error::Limit)?;
        let pin = Rc::new(PinData { references });
        pins.live.retain(|_, p| p.strong_count() > 0);
        let serial = pins.next;
        pins.live.insert(serial, Rc::downgrade(&pin));
        pins.next = next;
        Ok(ModulePin(pin))
    }

    pub(super) fn pin_route(
        &self,
        target: ModuleHandle,
        endpoint: &str,
        ceiling: Authority,
        routing: Option<&Routing>,
    ) -> Result<ModulePin> {
        let pipeline = self.build_pipeline(target, endpoint, ceiling, routing)?;
        self.pin_modules(&pipeline.references.into_iter().collect::<Vec<_>>())
    }
    pub(super) fn pin_invocation_route(
        &self,
        caller: Option<ModuleHandle>,
        frame: &Frame,
        endpoint: &str,
    ) -> Result<ModulePin> {
        let mut references = self
            .build_pipeline(
                frame.provider,
                endpoint,
                frame.authority.clone(),
                frame.routing.as_deref(),
            )?
            .references;
        references.extend(caller);
        if let Some(routing) = &frame.routing {
            for route in routing.routes.values() {
                references.extend(route.iter().map(|b| b.handle));
            }
        }
        self.pin_modules(&references.into_iter().collect::<Vec<_>>())
    }

    pub(super) fn unreferenced(&self, handle: ModuleHandle) -> Result<()> {
        if self
            .retention
            .pipelines
            .values()
            .any(|p| p.references.contains(&handle))
            || self.retention.owners.values().any(|r| r.contains(&handle))
            || self
                .retention
                .pins
                .borrow()
                .live
                .values()
                .filter_map(std::rc::Weak::upgrade)
                .any(|p| p.references.contains(&handle))
            || self
                .retention
                .callbacks
                .borrow()
                .live
                .values()
                .filter_map(std::rc::Weak::upgrade)
                .any(|c| c.pipeline.references.contains(&handle))
        {
            return Err(Error::Referenced);
        }
        Ok(())
    }

    /// Detach affects future calls only. Retained chains continue to own code.
    pub fn managed_detach(&mut self, extension: ModuleHandle) -> Result<()> {
        self.registration(extension)?;
        if !self.extensions.values().any(|(h, _)| *h == extension) {
            return Err(Error::MissingDependency);
        }
        self.extensions.retain(|_, (h, _)| *h != extension);
        Ok(())
    }

    /// Reattach only to the original, still installed target incarnation.
    pub fn managed_attach(&mut self, extension: ModuleHandle) -> Result<()> {
        let reg = self.registration(extension)?;
        let (target, endpoint, export) = reg.attachment.clone().ok_or(Error::InvalidManifest)?;
        self.registration(target)?;
        let key = (target, endpoint);
        if self.extensions.contains_key(&key) {
            return Err(Error::Duplicate);
        }
        self.extensions.insert(key, (extension, export));
        Ok(())
    }

    /// Session hosts use this stricter removal boundary. Legacy unregister keeps
    /// its unpinned behavior, but cannot remove any retained code.
    pub fn managed_unload(&mut self, handle: ModuleHandle) -> Result<()> {
        self.registration(handle)?;
        self.unreferenced(handle)?;
        if self
            .extensions
            .iter()
            .any(|((h, _), (e, _))| *h == handle || *e == handle)
            || self.registrations.iter().any(|(h, r)| {
                *h != handle
                    && (r.dependencies.contains(&handle)
                        || r.manifest
                            .dependencies
                            .iter()
                            .any(|d| self.ids.get(&d.capability.provider) == Some(&handle))
                        || r.attachment
                            .as_ref()
                            .is_some_and(|(target, _, _)| *target == handle))
            })
        {
            return Err(Error::Referenced);
        }
        self.unregister(handle)
    }
}

impl Kernel {
    pub fn issue_operation(&mut self) -> Result<OperationId> {
        let next = self.next_operation.checked_add(1).ok_or(Error::Limit)?;
        let id = OperationId {
            host: self.instance,
            serial: self.next_operation,
        };
        self.next_operation = next;
        Ok(id)
    }
    pub fn operation_cleanup_limit(&self) -> usize {
        self.limits.scope_keys
    }
    pub fn operation_binding(
        &self,
        operation: OperationId,
        pipeline: PipelineLease,
        checkpoint: u64,
        phase: &str,
    ) -> Result<OperationBinding> {
        self.operation_binding_with_cleanup(operation, pipeline, checkpoint, phase, &[])
    }
    pub fn operation_binding_with_cleanup(
        &self,
        operation: OperationId,
        pipeline: PipelineLease,
        checkpoint: u64,
        phase: &str,
        participants: &[CleanupIdentity],
    ) -> Result<OperationBinding> {
        if operation.host != self.instance
            || operation.serial == 0
            || operation.serial >= self.next_operation
        {
            return Err(Error::Authority);
        }
        self.pipeline_descriptor(pipeline)?;
        if phase.len() > self.limits.payload_bytes as usize {
            return Err(Error::Buffer);
        }
        if participants.len() > self.limits.scope_keys {
            return Err(Error::Limit);
        }
        let mut cleanup = participants.to_vec();
        cleanup.sort();
        cleanup.dedup();
        {
            let mut history = self.retention.cleanup_history.borrow_mut();
            let new = cleanup
                .iter()
                .filter(|id| !history.contains(&id.raw()))
                .count();
            if history.len().checked_add(new).ok_or(Error::Limit)? > persistence::MAX_CLEANUPS {
                return Err(Error::Limit);
            }
            history.extend(cleanup.iter().map(|id| id.raw()));
        }
        Ok(OperationBinding {
            operation,
            pipeline,
            checkpoint,
            phase: phase.into(),
            cleanup,
        })
    }
    /// All root admission checks that can refuse before code is invoked.
    pub fn preflight_pipeline(
        &self,
        lease: PipelineLease,
        request: &[u8],
        authority: &Authority,
    ) -> Result<()> {
        self.validate_authority(authority, false)?;
        let p = self
            .retention
            .pipelines
            .get(&lease)
            .ok_or(Error::InvalidHandle)?;
        self.preflight_retained(p, request, authority)
    }

    /// Read-only callback admission, including the future operation context bounds.
    /// Does not acquire identities, leases, or references.
    pub fn preflight_callback(
        &self,
        callback: &RetainedCallback,
        request: &[u8],
        authority: &Authority,
        phase: &str,
        participants: &[CleanupIdentity],
    ) -> Result<()> {
        if callback.0.host != self.instance {
            return Err(Error::InvalidHandle);
        }
        if phase.len() > self.limits.payload_bytes as usize {
            return Err(Error::Buffer);
        }
        if participants.len() > self.limits.scope_keys {
            return Err(Error::Limit);
        }
        self.validate_authority(authority, false)?;
        self.preflight_retained(&callback.0.pipeline, request, authority)
    }

    fn preflight_retained(
        &self,
        p: &Pipeline,
        request: &[u8],
        authority: &Authority,
    ) -> Result<()> {
        if let Some(origin) = &p.origin {
            self.validate_callback_origin(origin)?;
        }
        let d = &p.descriptor;
        if !d.ceiling.contains(authority) {
            return Err(Error::Authority);
        }
        let resolution = self.resolution(d.root.handle, &d.root.endpoint, Some(&p.routing))?;
        for p in resolution {
            let e = self
                .registration(p.handle)?
                .manifest
                .exports
                .get(&p.endpoint)
                .ok_or(Error::MissingDependency)?;
            if request.len() > e.max_request as usize {
                return Err(Error::Buffer);
            }
            if authority.access < e.access {
                return Err(Error::Authority);
            }
        }
        Ok(())
    }
    pub fn invoke_operation(
        &self,
        binding: &OperationBinding,
        request: &[u8],
        authority: Authority,
    ) -> Result<Vec<u8>> {
        let authenticated = self.operation_binding_with_cleanup(
            binding.operation,
            binding.pipeline,
            binding.checkpoint,
            &binding.phase,
            &binding.cleanup,
        )?;
        self.invoke_pipeline_context(binding.pipeline, request, authority, Some(authenticated))
    }
}

impl Kernel {
    /// Acquire both identities with no fallible work after pipeline publication.
    pub fn capture_operation(
        &mut self,
        provider: ModuleHandle,
        endpoint: &str,
        authority: Authority,
    ) -> Result<(OperationId, PipelineLease)> {
        let next = self.next_operation.checked_add(1).ok_or(Error::Limit)?;
        let lease = self.capture_pipeline(provider, endpoint, authority)?;
        let operation = OperationId {
            host: self.instance,
            serial: self.next_operation,
        };
        self.next_operation = next;
        Ok((operation, lease))
    }
}

#[cfg(test)]
mod operation_counter_tests {
    use super::*;
    #[test]
    fn exhausted_operation_counter_does_not_acquire_a_lease() {
        let mut k = Kernel::new(Limits::default()).unwrap();
        k.next_operation = u64::MAX;
        let before = k.lifecycle_state();
        let invalid = ModuleHandle {
            instance: k.instance,
            generation: 999,
        };
        assert_eq!(
            k.capture_operation(
                invalid,
                "step",
                Authority {
                    access: Access::Write,
                    scope: Scope::All
                }
            ),
            Err(Error::Limit)
        );
        assert_eq!(k.issue_operation(), Err(Error::Limit));
        assert_eq!(k.next_operation, u64::MAX);
        assert_eq!(k.lifecycle_state(), before);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn install(k: &mut Kernel, id: &str) -> ModuleHandle {
        k.register_native(
            Manifest {
                id: id.into(),
                exports: BTreeMap::from([(
                    "run".into(),
                    Endpoint {
                        major: 1,
                        access: Access::Read,
                        max_request: 0,
                        max_reply: 0,
                    },
                )]),
                dependencies: vec![],
                extension_points: BTreeSet::new(),
                extension: None,
            },
            b"control",
            BTreeMap::from([(
                "run".into(),
                Rc::new(|_: &[u8], _: &CallerView| Ok(vec![])) as NativePort,
            )]),
        )
        .unwrap()
    }
    fn authority() -> Authority {
        Authority {
            access: Access::Read,
            scope: Scope::Keys(BTreeSet::new()),
        }
    }
    #[test]
    fn last_serial_overflow_and_fabricated_identities_are_atomic() {
        let mut k = Kernel::new(Limits::default()).unwrap();
        let h = install(&mut k, "control");
        k.retention.next_serial = u64::MAX - 1;
        let p = k.capture_pipeline(h, "run", authority()).unwrap();
        assert_eq!(k.invoke_pipeline(p, &[], authority()), Ok(vec![]));
        let state = k.lifecycle_state();
        let fabricated = PipelineLease {
            instance: k.instance,
            serial: 0,
        };
        assert_eq!(k.release_pipeline(fabricated), Err(Error::InvalidHandle));
        assert_eq!(
            k.invoke_pipeline(fabricated, &[], authority()),
            Err(Error::InvalidHandle)
        );
        assert_eq!(k.capture_pipeline(h, "run", authority()), Err(Error::Limit));
        assert_eq!(k.retain_owner(&[h]), Err(Error::Limit));
        assert_eq!(k.lifecycle_state(), state);
        assert_eq!(k.invoke_pipeline(p, &[], authority()), Ok(vec![]));
        k.release_pipeline(p).unwrap();
        assert_eq!(k.retention.next_serial, u64::MAX);
        let mut owner_kernel = Kernel::new(Limits::default()).unwrap();
        let oh = install(&mut owner_kernel, "control");
        owner_kernel.retention.next_serial = u64::MAX - 1;
        let o = owner_kernel.retain_owner(&[oh]).unwrap();
        let state = owner_kernel.lifecycle_state();
        assert_eq!(
            owner_kernel.release_owner(OwnerReference {
                instance: owner_kernel.instance,
                serial: 0
            }),
            Err(Error::InvalidHandle)
        );
        assert_eq!(owner_kernel.retain_owner(&[oh]), Err(Error::Limit));
        assert_eq!(owner_kernel.lifecycle_state(), state);
        assert_eq!(owner_kernel.unregister(oh), Err(Error::Referenced));
        owner_kernel.release_owner(o).unwrap();
        owner_kernel.managed_unload(oh).unwrap();
    }
    #[test]
    fn generation_overflow_does_not_publish_registration_or_dispatch() {
        let mut k = Kernel::new(Limits::default()).unwrap();
        let h = install(&mut k, "control");
        k.next_generation = u64::MAX;
        let state = k.lifecycle_state();
        let mut m = k.manifest(h).unwrap().clone();
        m.id = "new".into();
        assert_eq!(
            k.register_native(
                m,
                b"new",
                BTreeMap::from([(
                    "run".into(),
                    Rc::new(|_: &[u8], _: &CallerView| Ok(vec![])) as NativePort
                )])
            ),
            Err(Error::Limit)
        );
        assert_eq!(k.lifecycle_state(), state);
        assert_eq!(k.invoke(h, "run", &[], authority()), Ok(vec![]));
        let p = k.capture_pipeline(h, "run", authority()).unwrap();
        assert_eq!(k.invoke_pipeline(p, &[], authority()), Ok(vec![]));
    }
}

fn pipeline_digest(
    root: &ProviderBinding,
    ceiling: &Authority,
    routes: &[Vec<ProviderBinding>],
    routing: &Routing,
) -> Result<[u8; 32]> {
    let mut digest = Sha256::new();
    digest.update(b"nethacked-retained-v1");
    fn bind(digest: &mut Sha256, b: &ProviderBinding) {
        digest.update(b.handle.instance.to_le_bytes());
        digest.update(b.handle.generation.to_le_bytes());
        digest.update(b.artifact);
        digest.update((b.endpoint.len() as u64).to_le_bytes());
        digest.update(b.endpoint.as_bytes());
    }
    bind(&mut digest, root);
    digest.update((routes.len() as u64).to_le_bytes());
    for route in routes {
        digest.update((route.len() as u64).to_le_bytes());
        for binding in route {
            bind(&mut digest, binding);
        }
    }
    digest.update(
        serde_json::to_vec(&serde_json::to_value(ceiling).map_err(|_| Error::InvalidManifest)?)
            .map_err(|_| Error::InvalidManifest)?,
    );
    for ((caller, slot), retained) in &routing.slots {
        digest.update(caller.instance.to_le_bytes());
        digest.update(caller.generation.to_le_bytes());
        digest.update((*slot as u64).to_le_bytes());
        digest.update(retained.target.instance.to_le_bytes());
        digest.update(retained.target.generation.to_le_bytes());
        digest.update(
            serde_json::to_vec(
                &serde_json::to_value(&retained.ceiling).map_err(|_| Error::InvalidManifest)?,
            )
            .map_err(|_| Error::InvalidManifest)?,
        );
    }
    Ok(digest.finalize().into())
}
