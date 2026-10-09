//! D4b typed adapter enrollment. Metadata contains only routes and D3 behavior/
//! scope IDs. Pure/fidelity/bounds/private allocation and effect-free-drop
//! premises of the approved D3 codecs/factories remain trusted host obligations.
use super::*;
use nethacked_kernel::persistence::{ExecutionDestination, KernelSnapshot, RestoredKernel};
use nethacked_kernel::Kernel;
use nethacked_owners::persistence::{
    BehaviorId, CaptureCatalog as OwnerCatalog, ExecutionCapture, ExecutionCheckpoint,
    ExecutionRestore, ExecutionRoots, PersistenceCodec,
};
use nethacked_timers::persistence::{Identity, RetentionRestore, TargetCodec, TaskRow};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BehaviorKey {
    pub behavior: u64,
    pub scope: Option<u64>,
}
impl From<BehaviorId> for BehaviorKey {
    fn from(id: BehaviorId) -> Self {
        Self {
            behavior: id.behavior,
            scope: id.scope,
        }
    }
}
impl BehaviorKey {
    fn id(&self) -> BehaviorId {
        BehaviorId {
            behavior: self.behavior,
            scope: self.scope,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteRow {
    pub id: u64,
    pub target: Resource,
    pub behavior: BehaviorKey,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleRow {
    pub id: u64,
    pub routes: Vec<u64>,
    pub scopes: Vec<BehaviorKey>,
    pub callback: u64,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    pub version: u32,
    pub kernel_namespace: u64,
    pub service: u64,
    pub identity: Identity,
    pub target_limit: usize,
    pub behavior_limit: usize,
    pub targets: Vec<Stamp>,
    pub routes: Vec<RouteRow>,
    pub bundles: Vec<BundleRow>,
}
impl Checkpoint {
    pub fn to_bytes(&self, max: usize) -> Result<Vec<u8>> {
        nethacked_workflows::persistence::encode_local_checkpoint(self, max)
            .map_err(|_| Error::Buffer)
    }
    pub fn from_bytes(bytes: &[u8], max: usize) -> Result<Self> {
        nethacked_workflows::persistence::decode_local_checkpoint(bytes, max)
            .map_err(|_| Error::Buffer)
    }
}
impl<T: 'static> BehaviorHandle<T> {
    pub fn persistence_roots(&self, roots: &mut ExecutionRoots) -> Result<()> {
        roots.behavior(&self.route.behavior).map_err(owner_error)?;
        for b in self
            .route
            .bindings
            .try_borrow()
            .map_err(|_| Error::NativeRefusal)?
            .iter()
            .filter_map(Weak::upgrade)
        {
            roots.behavior(&b).map_err(owner_error)?;
        }
        Ok(())
    }
}
impl<T: 'static> WeakBehavior<T> {
    pub fn persistence_handle(&self) -> Result<BehaviorHandle<T>> {
        Ok(BehaviorHandle {
            route: self.route.upgrade().ok_or(Error::InvalidHandle)?,
        })
    }
}
struct CapturedRoute {
    route: Rc<dyn BehaviorRoute>,
    row: RouteRow,
    scopes: Vec<(Rc<dyn CallbackReference>, BehaviorKey)>,
}
pub struct Capture {
    targets: Rc<Targets>,
    callbacks: Rc<CallbackBehaviors>,
    routes: Vec<CapturedRoute>,
    enrolled: BTreeMap<Resource, Stamp>,
    bundles: Vec<(Rc<dyn CallbackReference>, BundleRow)>,
    identity: Identity,
}
impl Capture {
    pub fn new(
        targets: Rc<Targets>,
        callbacks: Rc<CallbackBehaviors>,
        identity: Identity,
    ) -> Result<Self> {
        check_identity(&identity)?;
        Ok(Self {
            targets,
            callbacks,
            routes: vec![],
            enrolled: BTreeMap::new(),
            bundles: vec![],
            identity,
        })
    }
    /// Enroll one separately stored owner and all its retained routes/scopes in
    /// D3. No owner payload is stored in adapter metadata.
    pub fn owner<T: 'static>(
        &mut self,
        owner: &Rc<Owner<T>>,
        port: &str,
        handles: &[&BehaviorHandle<T>],
        execution: &mut ExecutionCapture<'_>,
        codec: &dyn PersistenceCodec<T>,
        catalog: &OwnerCatalog<T>,
    ) -> Result<()> {
        let address = owner.resource(port).map_err(owner_error)?;
        let proof = execution.adapter_target(owner, port).map_err(owner_error)?;
        let stamp = proof.stamp();
        let registered = self
            .targets
            .routes
            .try_borrow_mut()
            .map_err(|_| Error::NativeRefusal)?;
        let original = &registered
            .get(&address)
            .ok_or(Error::InvalidHandle)?
            .original;
        if original.owner != stamp.owner
            || original.generation != stamp.generation
            || original.schema != stamp.schema
            || original.module != stamp.module
            || self.enrolled.contains_key(&address)
        {
            return Err(Error::InvalidHandle);
        }
        drop(registered);
        let target = Stamp {
            address: address.clone(),
            generation: stamp.generation,
            revision: stamp.revision,
        };
        let live: Vec<_> = self
            .callbacks
            .routes
            .try_borrow()
            .map_err(|_| Error::NativeRefusal)?
            .iter()
            .filter_map(Weak::upgrade)
            .collect();
        let mut retained = vec![];
        let mut pieces = vec![];
        for handle in handles {
            let erased: Rc<dyn BehaviorRoute> = handle.route.clone();
            if !live.iter().any(|r| Rc::ptr_eq(r, &erased))
                || self.routes.iter().any(|r| Rc::ptr_eq(&r.route, &erased))
                || handle.behavior().target() != &address
            {
                return Err(Error::Authority);
            }
            let bindings: Vec<_> = handle
                .route
                .bindings
                .try_borrow()
                .map_err(|_| Error::NativeRefusal)?
                .iter()
                .filter_map(Weak::upgrade)
                .collect();
            retained.push(handle.behavior().clone());
            for b in &bindings {
                retained.push((**b).clone());
            }
            pieces.push((erased, bindings));
        }
        let mut unique = Vec::new();
        let mut aliases = Vec::new();
        for b in &retained {
            let id = if let Some(id) = unique
                .iter()
                .position(|u| nethacked_owners::persistence::adapter_behavior_alias(u, b))
            {
                id
            } else {
                unique.push(b.clone());
                unique.len() - 1
            };
            aliases.push(id);
        }
        let refs: Vec<_> = unique.iter().collect();
        let saved = execution
            .owner(owner, codec, catalog, &refs)
            .map_err(owner_error)?;
        let ids: Vec<_> = aliases.iter().map(|id| saved[*id]).collect();
        let mut cursor = 0;
        for (route, bindings) in pieces {
            let route_id = live
                .iter()
                .position(|r| Rc::ptr_eq(r, &route))
                .ok_or(Error::InvalidHandle)? as u64
                + 1;
            let row = RouteRow {
                id: route_id,
                target: address.clone(),
                behavior: ids[cursor].into(),
            };
            cursor += 1;
            let mut scopes = vec![];
            for b in bindings {
                let reference: Rc<dyn CallbackReference> = b;
                scopes.push((reference, ids[cursor].into()));
                cursor += 1;
            }
            self.routes.push(CapturedRoute { route, row, scopes });
        }
        self.enrolled.insert(address, target);
        Ok(())
    }
    /// IDs name actual shared Bundle allocations, never trait-object addresses
    /// in the wire. The references are the exact scoped D3 reservations.
    pub fn bundle(&mut self, reference: Rc<dyn CallbackReference>, callback: u64) -> Result<u64> {
        if let Some((_, row)) = self.bundles.iter().find(|(r, _)| Rc::ptr_eq(r, &reference)) {
            if row.callback != callback {
                return Err(Error::Authority);
            }
            return Ok(row.id);
        }
        let bundle = reference
            .as_ref()
            .persistence_type()
            .downcast_ref::<Bundle>()
            .ok_or(Error::InvalidHandle)?;
        if callback == 0
            || bundle._routes.len() != bundle._references.len()
            || bundle._routes.is_empty()
        {
            return Err(Error::Authority);
        }
        let mut routes = vec![];
        let mut scopes = vec![];
        for (route, binding) in bundle._routes.iter().zip(&bundle._references) {
            let captured = self
                .routes
                .iter()
                .find(|r| Rc::ptr_eq(&r.route, route))
                .ok_or(Error::Authority)?;
            let scope = captured
                .scopes
                .iter()
                .find(|(r, _)| Rc::ptr_eq(r, binding))
                .ok_or(Error::Authority)?
                .1
                .clone();
            routes.push(captured.row.id);
            scopes.push(scope);
        }
        let id = self.bundles.len() as u64 + 1;
        self.bundles.push((
            reference,
            BundleRow {
                id,
                routes,
                scopes,
                callback,
            },
        ));
        Ok(id)
    }
    pub fn finish(mut self, kernel_namespace: u64, service: u64) -> Result<Checkpoint> {
        let addresses: Vec<_> = self
            .targets
            .routes
            .try_borrow()
            .map_err(|_| Error::NativeRefusal)?
            .keys()
            .cloned()
            .collect();
        if addresses.iter().any(|a| !self.enrolled.contains_key(a)) {
            return Err(Error::Authority);
        }
        let live: Vec<_> = self
            .callbacks
            .routes
            .try_borrow_mut()
            .map_err(|_| Error::NativeRefusal)?
            .iter()
            .filter_map(Weak::upgrade)
            .collect();
        if live.len() != self.routes.len()
            || live
                .iter()
                .any(|r| !self.routes.iter().any(|c| Rc::ptr_eq(r, &c.route)))
        {
            return Err(Error::Authority);
        }
        self.routes.sort_by_key(|r| r.row.id);
        let targets = addresses
            .iter()
            .map(|a| self.enrolled.get(a).cloned().ok_or(Error::InvalidHandle))
            .collect::<Result<_>>()?;
        Ok(Checkpoint {
            version: 1,
            kernel_namespace,
            service,
            identity: self.identity,
            target_limit: self.targets.limit,
            behavior_limit: self.callbacks.limit,
            targets,
            routes: self.routes.into_iter().map(|r| r.row).collect(),
            bundles: self.bundles.into_iter().map(|(_, r)| r).collect(),
        })
    }
}
fn check_identity(i: &Identity) -> Result<()> {
    if i.name.is_empty() || i.name.len() > 256 || i.version == 0 {
        Err(Error::Limit)
    } else {
        Ok(())
    }
}
pub trait AdapterAuthority {
    fn approve(
        &self,
        checkpoint: &Checkpoint,
        kernel: &KernelSnapshot,
        execution: &ExecutionCheckpoint,
    ) -> Result<()>;
}
pub struct Validated {
    checkpoint: Checkpoint,
}
impl Checkpoint {
    pub fn validate(
        &self,
        kernel: &KernelSnapshot,
        execution: &ExecutionCheckpoint,
        approved: &Identity,
        authority: &dyn AdapterAuthority,
    ) -> Result<Validated> {
        check_identity(&self.identity)?;
        if self.version != 1
            || self.kernel_namespace != kernel.source_namespace
            || &self.identity != approved
            || self.service == 0
            || !kernel.registry.modules.iter().any(|m| m.id == self.service)
            || self.target_limit == 0
            || self.behavior_limit == 0
            || self.targets.len() > self.target_limit.min(65536)
            || self.routes.len() > self.behavior_limit.min(65536)
            || self.bundles.len() > 65536
            || self
                .targets
                .windows(2)
                .any(|w| w[0].address >= w[1].address)
        {
            return Err(Error::Limit);
        }
        let mut edges = 0usize;
        for t in &self.targets {
            if t.generation == 0
                || t.address.owner.is_empty()
                || t.address.owner.len() > 256
                || t.address.key.is_empty()
                || t.address.key.len() > 256
            {
                return Err(Error::InvalidHandle);
            }
        }
        for (n, r) in self.routes.iter().enumerate() {
            if r.id != n as u64 + 1
                || r.behavior.scope.is_some()
                || !self.targets.iter().any(|t| t.address == r.target)
            {
                return Err(Error::Authority);
            }
            execution
                .adapter_behavior(r.behavior.id(), &r.target, None)
                .map_err(owner_error)?;
        }
        for (n, b) in self.bundles.iter().enumerate() {
            if b.id != n as u64 + 1
                || b.routes.is_empty()
                || b.routes.len() != b.scopes.len()
                || b.routes.len() > self.behavior_limit
                || !kernel.callbacks.iter().any(|c| c.id == b.callback)
            {
                return Err(Error::Authority);
            }
            let mut seen = BTreeSet::new();
            for (route, scope) in b.routes.iter().zip(&b.scopes) {
                let r = self
                    .routes
                    .iter()
                    .find(|r| r.id == *route)
                    .ok_or(Error::InvalidHandle)?;
                if !seen.insert(route) || scope.behavior != r.behavior.behavior {
                    return Err(Error::Authority);
                }
                execution
                    .adapter_behavior(scope.id(), &r.target, Some(b.callback))
                    .map_err(owner_error)?;
            }
            edges = edges.checked_add(b.routes.len()).ok_or(Error::Limit)?;
        }
        if edges > 262144 {
            return Err(Error::Limit);
        }
        authority.approve(self, kernel, execution)?;
        Ok(Validated {
            checkpoint: self.clone(),
        })
    }
}
/// Typed hidden reconstruction; no arbitrary erased factory or payload mirror.
pub struct Restore {
    checkpoint: Checkpoint,
    destination: ExecutionDestination,
    targets: Rc<Targets>,
    callbacks: Rc<CallbackBehaviors>,
    routes: BTreeMap<u64, Rc<dyn BehaviorRoute>>,
    scopes: BTreeMap<(u64, u64), Rc<dyn CallbackReference>>,
    stamps: BTreeMap<Resource, Stamp>,
    claim: nethacked_kernel::persistence::WorkflowEnrollment,
}
impl Validated {
    pub fn rebuild(
        self,
        kernel: &Kernel,
        mapped: &RestoredKernel,
        execution: &ExecutionRestore,
    ) -> Result<Restore> {
        if self.checkpoint.kernel_namespace != mapped.execution_namespace() {
            return Err(Error::Authority);
        }
        mapped.execution_live(kernel)?;
        let destination = execution
            .adapter_destination(kernel, mapped)
            .map_err(owner_error)?;
        let claim = mapped.enroll_owner_adapter(self.checkpoint.service)?;
        Ok(Restore {
            targets: Rc::new(Targets::new(self.checkpoint.target_limit)?),
            callbacks: Rc::new(CallbackBehaviors::new(self.checkpoint.behavior_limit)?),
            checkpoint: self.checkpoint,
            destination,
            routes: BTreeMap::new(),
            scopes: BTreeMap::new(),
            stamps: BTreeMap::new(),
            claim,
        })
    }
}
impl Restore {
    pub fn target<T: 'static>(
        &mut self,
        old: &Resource,
        owner: &Rc<Owner<T>>,
        port: &str,
        mapped: &RestoredKernel,
        execution: &ExecutionRestore,
        kernel: &Kernel,
    ) -> Result<()> {
        if !execution
            .adapter_destination(kernel, mapped)
            .map_err(owner_error)?
            .same_destination(&self.destination)
            || self.stamps.contains_key(old)
        {
            return Err(Error::Authority);
        }
        let previous = self
            .checkpoint
            .targets
            .iter()
            .find(|t| &t.address == old)
            .ok_or(Error::InvalidHandle)?;
        let address = mapped.execution_resource(old)?;
        let proof = execution
            .adapter_target(owner, port, kernel, mapped)
            .map_err(owner_error)?;
        if proof.address() != address || proof.stamp().revision != previous.revision {
            return Err(Error::Authority);
        }
        let fresh = Stamp {
            address: address.clone(),
            generation: proof.stamp().generation,
            revision: proof.stamp().revision,
        };
        let route_address = address.clone();
        let original = proof.stamp().clone();
        self.targets
            .routes
            .try_borrow_mut()
            .map_err(|_| Error::NativeRefusal)?
            .insert(
                address.clone(),
                Route {
                    original,
                    current: Rc::new(move || {
                        let now = proof.current().map_err(owner_error)?;
                        Ok(Stamp {
                            address: route_address.clone(),
                            generation: now.generation,
                            revision: now.revision,
                        })
                    }),
                },
            );
        self.stamps.insert(old.clone(), fresh);
        Ok(())
    }
    pub fn behavior<T: 'static>(
        &mut self,
        id: u64,
        execution: &ExecutionRestore,
        kernel: &Kernel,
        mapped: &RestoredKernel,
    ) -> Result<BehaviorHandle<T>> {
        if !execution
            .adapter_destination(kernel, mapped)
            .map_err(owner_error)?
            .same_destination(&self.destination)
            || self.routes.contains_key(&id)
        {
            return Err(Error::Authority);
        }
        let row = self
            .checkpoint
            .routes
            .iter()
            .find(|r| r.id == id)
            .ok_or(Error::InvalidHandle)?;
        let behavior = execution
            .adapter_behavior::<T>(row.behavior.id(), kernel, mapped)
            .map_err(owner_error)?;
        if behavior.target() != mapped.execution_resource(&row.target)? {
            return Err(Error::Authority);
        }
        let handle = self.callbacks.register(behavior)?;
        for b in &self.checkpoint.bundles {
            for (rid, scope) in b.routes.iter().zip(&b.scopes) {
                if *rid != id {
                    continue;
                }
                let key = (scope.behavior, scope.scope.ok_or(Error::Authority)?);
                if self.scopes.contains_key(&key) {
                    continue;
                }
                let scoped = Rc::new(
                    execution
                        .adapter_behavior::<T>(scope.id(), kernel, mapped)
                        .map_err(owner_error)?,
                );
                let callback = mapped
                    .callbacks()
                    .get(&b.callback)
                    .ok_or(Error::InvalidHandle)?;
                if !scoped.matches_callback(callback.provenance()) {
                    return Err(Error::Authority);
                }
                handle
                    .route
                    .bindings
                    .borrow_mut()
                    .push(Rc::downgrade(&scoped));
                self.scopes.insert(key, scoped);
            }
        }
        let erased: Rc<dyn BehaviorRoute> = handle.route.clone();
        self.routes.insert(id, erased);
        Ok(handle)
    }
    pub fn finish(self) -> Result<Rc<Restored>> {
        if self.routes.len() != self.checkpoint.routes.len()
            || self.stamps.len() != self.checkpoint.targets.len()
        {
            return Err(Error::Authority);
        }
        let mut references = BTreeMap::new();
        *self
            .callbacks
            .routes
            .try_borrow_mut()
            .map_err(|_| Error::NativeRefusal)? = self
            .checkpoint
            .routes
            .iter()
            .map(|r| Rc::downgrade(&self.routes[&r.id]))
            .collect();
        for b in &self.checkpoint.bundles {
            let routes = b
                .routes
                .iter()
                .map(|id| self.routes.get(id).cloned().ok_or(Error::InvalidHandle))
                .collect::<Result<_>>()?;
            let refs = b
                .scopes
                .iter()
                .map(|s| {
                    self.scopes
                        .get(&(s.behavior, s.scope.ok_or(Error::Authority)?))
                        .cloned()
                        .ok_or(Error::InvalidHandle)
                })
                .collect::<Result<_>>()?;
            let reference: Rc<dyn CallbackReference> = Rc::new(Bundle {
                _routes: routes,
                _references: refs,
            });
            references.insert(b.id, reference);
        }
        self.claim.finish();
        let weak = references
            .iter()
            .map(|(id, r)| (*id, Rc::downgrade(r)))
            .collect();
        Ok(Rc::new(Restored {
            checkpoint: self.checkpoint,
            destination: self.destination,
            targets: self.targets,
            callbacks: self.callbacks,
            references: RefCell::new(references),
            weak,
            stamps: self.stamps,
        }))
    }
}
pub struct Restored {
    checkpoint: Checkpoint,
    destination: ExecutionDestination,
    targets: Rc<Targets>,
    callbacks: Rc<CallbackBehaviors>,
    references: RefCell<BTreeMap<u64, Rc<dyn CallbackReference>>>,
    weak: BTreeMap<u64, Weak<dyn CallbackReference>>,
    stamps: BTreeMap<Resource, Stamp>,
}
impl Restored {
    pub fn targets(&self) -> Rc<Targets> {
        self.targets.clone()
    }
    pub fn callbacks(&self) -> Rc<CallbackBehaviors> {
        self.callbacks.clone()
    }
}
impl RetentionRestore for Restored {
    fn destination(&self) -> ExecutionDestination {
        self.destination.clone()
    }
    fn check(&self, row: &TaskRow, mapped: &RestoredKernel) -> Result<()> {
        if !self
            .destination
            .same_destination(&mapped.execution_destination())
            || row.provenance.service.module != self.checkpoint.service
        {
            return Err(Error::Authority);
        }
        if let Some(id) = row.reference {
            let bundle = self
                .checkpoint
                .bundles
                .iter()
                .find(|b| b.id == id)
                .ok_or(Error::InvalidHandle)?;
            if let Some(callback) = row.callback {
                if callback != bundle.callback {
                    return Err(Error::Authority);
                }
            } else if row.invocation.is_none() || !row.cleanup_pending {
                return Err(Error::Authority);
            }
            let callback = mapped
                .callbacks()
                .get(&bundle.callback)
                .ok_or(Error::InvalidHandle)?;
            if callback.provenance().source != mapped.execution_source(&row.provenance.source)?
                || callback.provenance().caller.artifact != row.provenance.caller.artifact
                || callback.provenance().capability != row.provenance.capability
                || callback.provenance().slot != row.provenance.slot
            {
                return Err(Error::Authority);
            }
            for route in &bundle.routes {
                let target = &self
                    .checkpoint
                    .routes
                    .iter()
                    .find(|r| r.id == *route)
                    .ok_or(Error::InvalidHandle)?
                    .target;
                if !row.targets.iter().any(|t| &t.address == target) {
                    return Err(Error::Authority);
                }
            }
        }
        Ok(())
    }
    fn reference(&self, id: u64) -> Result<Rc<dyn CallbackReference>> {
        self.references
            .try_borrow()
            .map_err(|_| Error::NativeRefusal)?
            .get(&id)
            .cloned()
            .or_else(|| self.weak.get(&id).and_then(Weak::upgrade))
            .ok_or(Error::InvalidHandle)
    }
    fn finish_rebuild(&self) {
        self.references.borrow_mut().clear();
    }
}
impl TargetCodec for Restored {
    fn validate_old(&self, targets: &[Stamp]) -> Result<()> {
        if targets.iter().all(|t| {
            self.checkpoint.targets.iter().any(|current| {
                current.address == t.address
                    && current.generation == t.generation
                    && t.revision <= current.revision
            })
        }) {
            Ok(())
        } else {
            Err(Error::InvalidHandle)
        }
    }
    fn remap(&self, targets: &[Stamp], mapped: &RestoredKernel) -> Result<Vec<Stamp>> {
        if !self
            .destination
            .same_destination(&mapped.execution_destination())
        {
            return Err(Error::Authority);
        }
        targets
            .iter()
            .map(|old| {
                let mut fresh = self
                    .stamps
                    .get(&old.address)
                    .cloned()
                    .ok_or(Error::InvalidHandle)?;
                fresh.revision = old.revision;
                Ok(fresh)
            })
            .collect()
    }
}
