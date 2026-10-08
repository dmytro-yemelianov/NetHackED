//! Optional metadata-only routes. Owners retain their own independent payloads.
#![forbid(unsafe_code)]
use nethacked_kernel::{CallbackProvenance, CallerView, Error, Resource, Result, RetainedCallback};
use nethacked_owners::{Owner, RetainedBehavior};
use nethacked_resources::Stamp;
use nethacked_timers::{CallbackReference, CallbackRetention, TargetValidator};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::{Rc, Weak},
};

type Route = Rc<dyn Fn() -> Result<Stamp>>;
/// Bounded target metadata and weak owner routes; no payload database or World.
pub struct Targets {
    limit: usize,
    routes: RefCell<BTreeMap<Resource, Route>>,
}
impl Targets {
    pub fn new(limit: usize) -> Result<Self> {
        if limit == 0 {
            return Err(Error::Limit);
        }
        Ok(Self {
            limit,
            routes: RefCell::new(BTreeMap::new()),
        })
    }
    /// One address per registered owner port. Schema/provider/incarnation are
    /// sealed into the weak route; task wire stamps alone confer no rights.
    pub fn register<T: 'static>(&self, owner: &Rc<Owner<T>>, port: &str) -> Result<Stamp> {
        let stamp = owner.stamp().map_err(owner_error)?;
        let address = owner.resource(port).map_err(owner_error)?;
        let mut routes = self
            .routes
            .try_borrow_mut()
            .map_err(|_| Error::NativeRefusal)?;
        if routes.contains_key(&address) {
            return Err(Error::Duplicate);
        }
        if routes.len() >= self.limit {
            return Err(Error::Limit);
        }
        let weak = Rc::downgrade(owner);
        let target_address = address.clone();
        let original = stamp.clone();
        routes.insert(
            address.clone(),
            Rc::new(move || {
                let owner = weak.upgrade().ok_or(Error::InvalidHandle)?;
                let now = owner.stamp().map_err(owner_error)?;
                if now.owner != original.owner
                    || now.generation != original.generation
                    || now.schema != original.schema
                    || now.module != original.module
                {
                    return Err(Error::InvalidHandle);
                }
                Ok(Stamp {
                    address: target_address.clone(),
                    generation: now.generation,
                    revision: now.revision,
                })
            }),
        );
        Ok(Stamp {
            address,
            generation: stamp.generation,
            revision: stamp.revision,
        })
    }
    pub fn current(&self, address: &Resource) -> Result<Stamp> {
        let route = self
            .routes
            .try_borrow()
            .map_err(|_| Error::NativeRefusal)?
            .get(address)
            .cloned()
            .ok_or(Error::InvalidHandle)?;
        route()
    }
    pub fn remove(&self, address: &Resource) -> Result<()> {
        self.routes
            .try_borrow_mut()
            .map_err(|_| Error::NativeRefusal)?
            .remove(address)
            .ok_or(Error::InvalidHandle)?;
        Ok(())
    }
}
impl TargetValidator for Targets {
    fn validate(&self, targets: &[Stamp]) -> Result<()> {
        if targets.len() > self.limit {
            return Err(Error::Limit);
        }
        for stamp in targets {
            if self.current(&stamp.address)? != *stamp {
                return Err(Error::InvalidHandle);
            }
        }
        Ok(())
    }
}
fn owner_error(e: nethacked_owners::Error) -> Error {
    match e {
        nethacked_owners::Error::Kernel(e) => e,
        _ => Error::NativeRefusal,
    }
}
/// A host retains one or more heterogeneous owner behaviors for one callback.
/// The registry stores only weak routes; the actual queue owns returned bundles.
trait BehaviorRoute {
    fn matches(&self, provenance: &CallbackProvenance) -> bool;
    fn target(&self) -> &Resource;
    fn bind(&self, callback: &RetainedCallback) -> Result<Rc<dyn CallbackReference>>;
    fn matches_source(&self, view: &CallerView<'_>) -> bool;
    fn preflight(&self, additional: usize) -> Result<()>;
}
struct Behavior<T> {
    behavior: RetainedBehavior<T>,
    bindings: RefCell<Vec<Weak<RetainedBehavior<T>>>>,
    limit: usize,
}
impl<T> Behavior<T> {
    fn for_execution(&self, view: &CallerView<'_>) -> Result<RetainedBehavior<T>> {
        self.bindings
            .try_borrow()
            .map_err(|_| Error::NativeRefusal)?
            .iter()
            .filter_map(Weak::upgrade)
            .find(|b| b.matches_execution(view))
            .map(|b| (*b).clone())
            .ok_or(Error::Authority)
    }
}
impl<T: 'static> BehaviorRoute for Behavior<T> {
    fn matches_source(&self, view: &CallerView<'_>) -> bool {
        self.behavior.matches_source(view)
    }
    fn preflight(&self, additional: usize) -> Result<()> {
        let bindings = self
            .bindings
            .try_borrow()
            .map_err(|_| Error::NativeRefusal)?;
        if bindings.iter().filter(|b| b.strong_count() != 0).count() >= self.limit {
            return Err(Error::Limit);
        }
        self.behavior
            .preflight_callback_scopes(additional)
            .map_err(owner_error)
    }
    fn matches(&self, provenance: &CallbackProvenance) -> bool {
        self.behavior.matches_callback(provenance)
    }
    fn target(&self) -> &Resource {
        self.behavior.target()
    }
    fn bind(&self, callback: &RetainedCallback) -> Result<Rc<dyn CallbackReference>> {
        let mut bindings = self
            .bindings
            .try_borrow_mut()
            .map_err(|_| Error::NativeRefusal)?;
        bindings.retain(|b| b.strong_count() != 0);
        if bindings.len() >= self.limit {
            return Err(Error::Limit);
        }
        let reference = Rc::new(self.behavior.bind_callback(callback).map_err(owner_error)?);
        bindings.push(Rc::downgrade(&reference));
        Ok(reference)
    }
}
pub struct CallbackBehaviors {
    limit: usize,
    routes: RefCell<Vec<Weak<dyn BehaviorRoute>>>,
}
pub struct BehaviorHandle<T> {
    route: Rc<Behavior<T>>,
}
pub struct WeakBehavior<T> {
    route: Weak<Behavior<T>>,
}
impl<T> BehaviorHandle<T> {
    pub fn behavior(&self) -> &RetainedBehavior<T> {
        &self.route.behavior
    }
    pub fn downgrade(&self) -> WeakBehavior<T> {
        WeakBehavior {
            route: Rc::downgrade(&self.route),
        }
    }
}
impl<T> WeakBehavior<T> {
    pub fn for_execution(&self, view: &CallerView<'_>) -> Result<RetainedBehavior<T>> {
        let route = self.route.upgrade().ok_or(Error::InvalidHandle)?;
        route.for_execution(view)
    }
    pub fn is_retained(&self) -> bool {
        self.route.strong_count() != 0
    }
}
impl CallbackBehaviors {
    pub fn new(limit: usize) -> Result<Self> {
        if limit == 0 {
            return Err(Error::Limit);
        }
        Ok(Self {
            limit,
            routes: RefCell::new(Vec::new()),
        })
    }
    pub fn register<T: 'static>(&self, behavior: RetainedBehavior<T>) -> Result<BehaviorHandle<T>> {
        let mut routes = self
            .routes
            .try_borrow_mut()
            .map_err(|_| Error::NativeRefusal)?;
        routes.retain(|route| route.strong_count() != 0);
        if routes.len() >= self.limit {
            return Err(Error::Limit);
        }
        let route = Rc::new(Behavior {
            behavior,
            bindings: RefCell::new(Vec::new()),
            limit: self.limit,
        });
        let erased: Rc<dyn BehaviorRoute> = route.clone();
        routes.push(Rc::downgrade(&erased));
        Ok(BehaviorHandle { route })
    }
    /// Lookup only by authenticated callback provenance, never a guest version ID.
    pub fn for_execution<T: 'static>(
        &self,
        handle: &BehaviorHandle<T>,
        view: &CallerView<'_>,
    ) -> Result<RetainedBehavior<T>> {
        handle.route.for_execution(view)
    }
}
struct Bundle {
    _routes: Vec<Rc<dyn BehaviorRoute>>,
    _references: Vec<Rc<dyn CallbackReference>>,
}
impl CallbackRetention for CallbackBehaviors {
    fn preflight(&self, view: &CallerView<'_>, targets: &[Stamp]) -> Result<()> {
        let routes: Vec<_> = self
            .routes
            .try_borrow()
            .map_err(|_| Error::NativeRefusal)?
            .iter()
            .filter_map(Weak::upgrade)
            .filter(|r| r.matches_source(view))
            .collect();
        if routes.is_empty() {
            return Err(Error::InvalidHandle);
        }
        let mut counts = BTreeMap::new();
        for route in &routes {
            if !targets.iter().any(|t| &t.address == route.target()) {
                return Err(Error::Authority);
            }
            let count = counts.entry(&route.target().owner).or_insert(0usize);
            *count = count.checked_add(1).ok_or(Error::Limit)?;
        }
        for route in &routes {
            route.preflight(counts[&route.target().owner])?;
        }
        Ok(())
    }
    fn retain(
        &self,
        callback: &RetainedCallback,
        targets: &[Stamp],
    ) -> Result<Rc<dyn CallbackReference>> {
        let routes: Vec<_> = self
            .routes
            .try_borrow()
            .map_err(|_| Error::NativeRefusal)?
            .iter()
            .filter_map(Weak::upgrade)
            .filter(|r| r.matches(callback.provenance()))
            .collect();
        if routes.is_empty() {
            return Err(Error::InvalidHandle);
        }
        if routes
            .iter()
            .any(|r| !targets.iter().any(|t| &t.address == r.target()))
        {
            return Err(Error::Authority);
        }
        let references = routes
            .iter()
            .map(|r| r.bind(callback))
            .collect::<Result<Vec<_>>>()?;
        Ok(Rc::new(Bundle {
            _routes: routes,
            _references: references,
        }))
    }
}
