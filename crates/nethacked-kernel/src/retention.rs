//! Private routing and reference ownership; publication follows complete preflight.
use super::*;
use std::collections::BTreeSet;

type RouteKey = (ModuleHandle, String);

pub(crate) struct RetainedSlot {
    pub target: ModuleHandle,
    pub ceiling: Option<Authority>,
}
pub(crate) struct Routing {
    routes: BTreeMap<RouteKey, Vec<ProviderBinding>>,
    pub slots: BTreeMap<(ModuleHandle, usize), RetainedSlot>,
}
struct Pipeline {
    descriptor: PipelineDescriptor,
    routing: Rc<Routing>,
    references: BTreeSet<ModuleHandle>,
}
pub(super) struct Retention {
    limits: RetentionLimits,
    next_serial: u64,
    pipelines: BTreeMap<PipelineLease, Pipeline>,
    owners: BTreeMap<OwnerReference, BTreeSet<ModuleHandle>>,
}
impl Default for Retention {
    fn default() -> Self {
        Self {
            limits: RetentionLimits::default(),
            next_serial: 1,
            pipelines: BTreeMap::new(),
            owners: BTreeMap::new(),
        }
    }
}

struct Capture<'a> {
    kernel: &'a Kernel,
    ceiling: &'a Authority,
    routing: Routing,
    visiting: BTreeSet<RouteKey>,
    references: BTreeSet<ModuleHandle>,
    nodes: usize,
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
        let chain = self.kernel.resolution(key.0, &key.1, None)?;
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
                let ceiling = match self.kernel.grants.get(&(binding.handle, slot)) {
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
        if !self.retention.pipelines.is_empty() || !self.retention.owners.is_empty() {
            return Err(Error::Referenced);
        }
        self.retention.limits = limits;
        Ok(())
    }

    pub fn lifecycle_state(&self) -> LifecycleState {
        LifecycleState {
            registrations: self.registrations.len(),
            attachments: self.extensions.len(),
            grants: self.grants.len(),
            leases: self.retention.pipelines.len(),
            owners: self.retention.owners.len(),
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
                    .sum::<usize>(),
            next_generation: self.next_generation,
            next_retention: self.retention.next_serial,
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
        };
        capture.visit((provider, endpoint.to_owned()), 1)?;
        self.reference_capacity(capture.references.len())?;
        let root = ProviderBinding {
            handle: provider,
            artifact: self.artifact(provider)?,
            endpoint: endpoint.to_owned(),
        };
        let routes: Vec<_> = capture.routing.routes.values().cloned().collect();
        // Canonical ordered runtime-local descriptor includes slot ceilings and
        // root identity, not just executable bytes. Identities are not a codec.
        let mut digest = Sha256::new();
        digest.update(b"nethacked-retained-v1");
        fn bind(digest: &mut Sha256, b: &ProviderBinding) {
            digest.update(b.handle.instance.to_le_bytes());
            digest.update(b.handle.generation.to_le_bytes());
            digest.update(b.artifact);
            digest.update((b.endpoint.len() as u64).to_le_bytes());
            digest.update(b.endpoint.as_bytes());
        }
        bind(&mut digest, &root);
        digest.update((routes.len() as u64).to_le_bytes());
        for route in &routes {
            digest.update((route.len() as u64).to_le_bytes());
            for binding in route {
                bind(&mut digest, binding);
            }
        }
        digest.update(
            serde_json::to_vec(
                &serde_json::to_value(&ceiling).map_err(|_| Error::InvalidManifest)?,
            )
            .map_err(|_| Error::InvalidManifest)?,
        );
        for ((caller, slot), retained) in &capture.routing.slots {
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
        let descriptor = PipelineDescriptor {
            root,
            ceiling: ceiling.clone(),
            routes,
            hash: digest.finalize().into(),
        };
        let pipeline = Pipeline {
            descriptor,
            routing: Rc::new(capture.routing),
            references: capture.references,
        };
        let lease = PipelineLease {
            instance: self.instance,
            serial: self.retention.next_serial,
        };
        self.retention.pipelines.insert(lease, pipeline);
        self.retention.next_serial = next;
        Ok(lease)
    }

    pub fn pipeline_descriptor(&self, lease: PipelineLease) -> Result<&PipelineDescriptor> {
        Ok(&self
            .retention
            .pipelines
            .get(&lease)
            .ok_or(Error::InvalidHandle)?
            .descriptor)
    }

    pub fn invoke_pipeline(
        &self,
        lease: PipelineLease,
        request: &[u8],
        authority: Authority,
    ) -> Result<Vec<u8>> {
        self.validate_authority(&authority, false)?;
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
            || self.retention.owners.len() >= self.retention.limits.owners
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

    pub(super) fn unreferenced(&self, handle: ModuleHandle) -> Result<()> {
        if self
            .retention
            .pipelines
            .values()
            .any(|p| p.references.contains(&handle))
            || self.retention.owners.values().any(|r| r.contains(&handle))
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
