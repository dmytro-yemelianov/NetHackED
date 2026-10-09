//! Host-only serial orchestration, independent of wire syntax and entity types.
//!
//! Native factory fidelity, complete proof enrollment, immutable configuration,
//! private allocations/alias exclusion and effect-free construction/drop are
//! explicit trust premises. This module cannot make arbitrary native code pure.
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Serial,
    /// No cross-thread exclusion proof is implemented.
    Threaded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Unsupported,
    Busy,
    ForeignDomain,
    ForeignAuthority,
    AuthorityChanged,
    StaleStage,
    ConsumedStage,
    Unauthorized,
    MissingBinding,
    MissingFactory,
    DuplicateFactory,
    FactoryRefused,
    FactoryPanicked,
    EncoderPanicked,
    RuntimePanicked,
    FailStop,
    Exhausted,
    Allocation,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

/// Exact trusted artifact/schema/codec/policy/resolver binding; never a pointer.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Binding {
    pub name: String,
    pub version: u64,
    pub artifact: [u8; 32],
}

/// A host-defined exact permission atom. Concrete adapters must enumerate every
/// effective capability/access/resource requirement; names alone confer nothing.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Permission(pub String);

/// Trusted native enrollment, deliberately separate from untrusted save metadata.
/// `build` must validate old/target semantics and the complete graph, construct
/// independent hidden allocations and require every binding/permission through
/// `BuildContext`. It must neither publish, schedule nor invoke effects. No closure
/// is decoded from bytes. There is no final-admission callback during activation.
pub trait TrustedFactory<B>: 'static {
    fn build(&self, context: &mut BuildContext<'_, B>, metadata: &[u8]) -> Result<B, Error>;
}

/// Immutable host-approved configuration. Factories and their captured resolvers
/// must be immutable too; retained mutable aliases violate enrollment premises.
/// Changes are installed only with `HostSlot::replace_configuration`/`rotate_authority`.
/// Each owned factory reference contains destructor unwinding, including cleanup
/// on duplicate enrollment. Caller-retained naked `Rc`s are outside this boundary.
pub struct ApprovedConfiguration<B> {
    permissions: BTreeSet<Permission>,
    bindings: BTreeSet<Binding>,
    factories: BTreeMap<Binding, Held<Rc<dyn TrustedFactory<B>>>>,
}
impl<B> ApprovedConfiguration<B> {
    pub fn new(
        permissions: BTreeSet<Permission>,
        bindings: BTreeSet<Binding>,
        factories: Vec<(Binding, Rc<dyn TrustedFactory<B>>)>,
    ) -> Result<Self, Error> {
        // Protect every supplied reference before validation: duplicate refusal
        // drops displaced, enrolled and still-unprocessed references separately.
        let factories: Vec<_> = factories
            .into_iter()
            .map(|(identity, factory)| (identity, Held(Some(factory))))
            .collect();
        let mut enrolled = BTreeMap::new();
        for (identity, factory) in factories {
            if enrolled.insert(identity, factory).is_some() {
                return Err(Error::DuplicateFactory);
            }
        }
        Ok(Self {
            permissions,
            bindings,
            factories: enrolled,
        })
    }
}

#[derive(Default)]
struct Proofs {
    permissions: BTreeSet<Permission>,
    bindings: BTreeSet<Binding>,
}
/// Only the runtime constructs this context. Rejected requirements remain rejected
/// even if a factory ignores the returned error. It cannot mint a ready stage.
pub struct BuildContext<'a, B> {
    configuration: &'a ApprovedConfiguration<B>,
    proofs: Proofs,
    refused: Option<Error>,
}
impl<B> BuildContext<'_, B> {
    pub fn require_permission(&mut self, permission: &Permission) -> Result<(), Error> {
        if !self.configuration.permissions.contains(permission) {
            self.refused = Some(Error::Unauthorized);
            return Err(Error::Unauthorized);
        }
        self.proofs.permissions.insert(permission.clone());
        Ok(())
    }
    pub fn require_binding(&mut self, binding: &Binding) -> Result<(), Error> {
        if !self.configuration.bindings.contains(binding) {
            self.refused = Some(Error::MissingBinding);
            return Err(Error::MissingBinding);
        }
        self.proofs.bindings.insert(binding.clone());
        Ok(())
    }
}

struct Domain {
    occupied: Cell<bool>,
    failed: Cell<bool>,
}
impl Domain {
    fn enter(&self) -> Result<DomainGuard<'_>, Error> {
        if self.occupied.get() {
            return Err(Error::Busy);
        }
        if self.failed.get() {
            return Err(Error::FailStop);
        }
        self.occupied.set(true);
        Ok(DomainGuard { domain: self })
    }
}
/// A held guard blocks *all* runtime access, capture, staging and authority changes.
/// Dropping it releases exclusion, including during unwinding. Forgetting it
/// intentionally leaves the domain Busy; it cannot open an exclusion hole.
pub struct DomainGuard<'a> {
    domain: &'a Domain,
}
impl Drop for DomainGuard<'_> {
    fn drop(&mut self) {
        self.domain.occupied.set(false);
    }
}
struct AuthorityIdentity;

/// Host administration credential, not serializable or constructible by callers.
/// Authority rotation invalidates it. Ordinary configuration changes preserve it.
pub struct AuthorityKey {
    domain: Rc<Domain>,
    identity: Rc<AuthorityIdentity>,
}
/// Sealed runtime credential; exchange invalidates prior bundle generations and
/// authority changes invalidate prior epochs. Never reconstructed from save IDs.
pub struct RuntimeCredential {
    domain: Rc<Domain>,
    identity: Rc<AuthorityIdentity>,
    epoch: u64,
    generation: u64,
    permissions: BTreeSet<Permission>,
}

// Panic payloads are quarantined (leaked): even their Drop may panic. With unwind
// enabled a single native panic is contained. Abort/OOM/double-unwind are fail-stop.
fn contain<T>(f: impl FnOnce() -> T) -> Result<T, ()> {
    catch_unwind(AssertUnwindSafe(f)).map_err(|payload| {
        std::mem::forget(payload);
    })
}
struct Held<T>(Option<T>);
impl<T> Held<T> {
    fn get(&self) -> &T {
        self.0.as_ref().expect("internal occupied allocation")
    }
    fn get_mut(&mut self) -> &mut T {
        self.0.as_mut().expect("internal occupied allocation")
    }
    fn retire(&mut self) -> bool {
        self.0
            .take()
            .is_none_or(|value| contain(|| drop(value)).is_ok())
    }
}
impl<T> Drop for Held<T> {
    fn drop(&mut self) {
        self.retire();
    }
}

struct State<B> {
    bundle: Held<B>,
    configuration: Rc<Held<ApprovedConfiguration<B>>>,
    identity: Rc<AuthorityIdentity>,
    epoch: u64,
    generation: u64,
}
/// Owns ONE complete typed (or host-erased) heterogeneous bundle. `B` is a tuple
/// or struct of independent owner/service allocations, never a generic mutable
/// domain database or a live World replica. This is a serial, !Send/!Sync executor.
///
/// ```compile_fail
/// use nethacked_persistence::runtime::HostSlot;
/// fn require_send<T: Send>() {}
/// require_send::<HostSlot<()>>();
/// ```
pub struct HostSlot<B> {
    domain: Rc<Domain>,
    state: RefCell<State<B>>,
}
/// Private hidden bundle and precomputed admission proofs; no public constructor,
/// bundle getter, proof constructor, unchecked ready-state or Clone implementation.
///
/// ```compile_fail
/// use nethacked_persistence::runtime::RestoreStage;
/// let forged = RestoreStage::<()> {};
/// ```
pub struct RestoreStage<B> {
    domain: Rc<Domain>,
    identity: Rc<AuthorityIdentity>,
    configuration: Rc<Held<ApprovedConfiguration<B>>>,
    epoch: u64,
    generation: u64,
    factory: Binding,
    proofs: Proofs,
    bundle: Held<B>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Retirement {
    Complete,
    /// Exchange committed; a retirement panic disabled this domain permanently.
    PanickedFailStop,
}
pub struct Activation {
    handles: Vec<RuntimeCredential>,
    pub retirement: Retirement,
}
impl Activation {
    pub fn credential(&self) -> &RuntimeCredential {
        &self.handles[0]
    }
}

impl<B: 'static> HostSlot<B> {
    /// Host enrollment only. All supplied allocations/factories obey the native
    /// premises above. Initial credentials cover the initial approved permissions.
    pub fn new(
        bundle: B,
        configuration: ApprovedConfiguration<B>,
        backend: Backend,
    ) -> Result<(Self, AuthorityKey, RuntimeCredential), Error> {
        let bundle = Held(Some(bundle));
        let configuration = Held(Some(configuration));
        if backend != Backend::Serial {
            return Err(Error::Unsupported);
        }
        let domain = Rc::new(Domain {
            occupied: Cell::new(false),
            failed: Cell::new(false),
        });
        let identity = Rc::new(AuthorityIdentity);
        let key = AuthorityKey {
            domain: domain.clone(),
            identity: identity.clone(),
        };
        let credential = RuntimeCredential {
            domain: domain.clone(),
            identity: identity.clone(),
            epoch: 1,
            generation: 1,
            permissions: configuration.get().permissions.clone(),
        };
        Ok((
            Self {
                domain,
                state: RefCell::new(State {
                    bundle,
                    configuration: Rc::new(configuration),
                    identity,
                    epoch: 1,
                    generation: 1,
                }),
            },
            key,
            credential,
        ))
    }
    fn check_key(&self, key: &AuthorityKey, state: &State<B>) -> Result<(), Error> {
        if !Rc::ptr_eq(&key.domain, &self.domain) {
            return Err(Error::ForeignDomain);
        }
        if !Rc::ptr_eq(&key.identity, &state.identity) {
            return Err(Error::ForeignAuthority);
        }
        Ok(())
    }
    pub fn try_hold(&self, key: &AuthorityKey) -> Result<DomainGuard<'_>, Error> {
        let guard = self.domain.enter()?;
        self.check_key(key, &self.state.borrow())?;
        Ok(guard)
    }
    /// The complete immutable snapshot/encoding runs inside exclusion. `S` must be
    /// detached immutable data; native encoders may not leak service aliases.
    pub fn capture<S>(
        &self,
        key: &AuthorityKey,
        encoder: impl FnOnce(&B) -> Result<S, Error>,
    ) -> Result<S, Error> {
        let _guard = self.try_hold(key)?;
        let state = self.state.borrow();
        contain(|| encoder(state.bundle.get())).map_err(|()| Error::EncoderPanicked)?
    }
    /// Every enrolled invocation/publication/delivery/scheduling/lifecycle/mutation
    /// enters here. A paused task holds no guard. Admission errors precede the
    /// callback. Native commands must preflight their own refusals; returned values
    /// are not interpreted as admission. A mutable panic fail-stops, without rollback.
    pub fn with_runtime<R>(
        &self,
        credential: &RuntimeCredential,
        call: impl FnOnce(&mut B) -> R,
    ) -> Result<R, Error> {
        let _guard = self.domain.enter()?;
        let mut state = self.state.borrow_mut();
        if !Rc::ptr_eq(&credential.domain, &self.domain) {
            return Err(Error::ForeignDomain);
        }
        if !Rc::ptr_eq(&credential.identity, &state.identity) {
            return Err(Error::ForeignAuthority);
        }
        if credential.generation != state.generation {
            return Err(Error::StaleStage);
        }
        if credential.epoch != state.epoch {
            return Err(Error::AuthorityChanged);
        }
        if !credential
            .permissions
            .is_subset(&state.configuration.get().permissions)
        {
            return Err(Error::Unauthorized);
        }
        contain(|| call(state.bundle.get_mut())).map_err(|()| {
            self.domain.failed.set(true);
            Error::RuntimePanicked
        })
    }
    /// Reissue only currently approved permissions under the same exclusion.
    pub fn issue_credential(
        &self,
        key: &AuthorityKey,
        permissions: BTreeSet<Permission>,
    ) -> Result<RuntimeCredential, Error> {
        let _guard = self.try_hold(key)?;
        let state = self.state.borrow();
        if !permissions.is_subset(&state.configuration.get().permissions) {
            return Err(Error::Unauthorized);
        }
        Ok(RuntimeCredential {
            domain: self.domain.clone(),
            identity: state.identity.clone(),
            epoch: state.epoch,
            generation: state.generation,
            permissions,
        })
    }
    /// Grant revoke/narrow, artifact/module, resolver, codec and policy updates all
    /// replace ONE immutable configuration, advancing a trusted checked epoch.
    pub fn replace_configuration(
        &self,
        key: &AuthorityKey,
        configuration: ApprovedConfiguration<B>,
    ) -> Result<(), Error> {
        let configuration = Held(Some(configuration));
        let _guard = self.try_hold(key)?;
        let mut state = self.state.borrow_mut();
        let next = state.epoch.checked_add(1).ok_or(Error::Exhausted)?;
        state.configuration = Rc::new(configuration);
        state.epoch = next;
        Ok(())
    }
    pub fn rotate_authority(
        &self,
        key: &AuthorityKey,
        configuration: ApprovedConfiguration<B>,
    ) -> Result<AuthorityKey, Error> {
        let configuration = Held(Some(configuration));
        let _guard = self.try_hold(key)?;
        let mut state = self.state.borrow_mut();
        let next = state.epoch.checked_add(1).ok_or(Error::Exhausted)?;
        let identity = Rc::new(AuthorityIdentity);
        state.configuration = Rc::new(configuration);
        state.identity = identity.clone();
        state.epoch = next;
        Ok(AuthorityKey {
            domain: self.domain.clone(),
            identity,
        })
    }
    /// Selects an already enrolled exact native factory. Metadata never enrolls
    /// code or supplies authority. Hidden build/proof generation holds exclusion.
    pub fn prepare_restore(
        &self,
        key: &AuthorityKey,
        factory: &Binding,
        metadata: &[u8],
    ) -> Result<RestoreStage<B>, Error> {
        let _guard = self.try_hold(key)?;
        let state = self.state.borrow();
        let configuration = state.configuration.get();
        let enrolled = configuration
            .factories
            .get(factory)
            .ok_or(Error::MissingFactory)?;
        let mut context = BuildContext {
            configuration,
            proofs: Proofs::default(),
            refused: None,
        };
        let mut bundle = Held(Some(
            contain(|| enrolled.get().build(&mut context, metadata))
                .map_err(|()| Error::FactoryPanicked)??,
        ));
        if let Some(error) = context.refused {
            bundle.retire();
            return Err(error);
        }
        Ok(RestoreStage {
            domain: self.domain.clone(),
            identity: state.identity.clone(),
            configuration: state.configuration.clone(),
            epoch: state.epoch,
            generation: state.generation,
            factory: factory.clone(),
            proofs: context.proofs,
            bundle,
        })
    }
    /// Reserve outputs, check all sealed proofs under exclusion, exchange once,
    /// retire the detached old bundle under that guard. No user code can intervene
    /// between admission and exchange. Refusal leaves both slot and stage intact.
    pub fn activate(&self, stage: &mut RestoreStage<B>) -> Result<Activation, Error> {
        self.activate_inner(
            stage,
            #[cfg(test)]
            || {},
        )
    }
    fn activate_inner(
        &self,
        stage: &mut RestoreStage<B>,
        #[cfg(test)] boundary: impl FnOnce(),
    ) -> Result<Activation, Error> {
        let mut handles = Vec::new();
        handles
            .try_reserve_exact(1)
            .map_err(|_| Error::Allocation)?;
        let _guard = self.domain.enter()?;
        let mut state = self.state.borrow_mut();
        if !Rc::ptr_eq(&stage.domain, &self.domain) {
            return Err(Error::ForeignDomain);
        }
        if !Rc::ptr_eq(&stage.identity, &state.identity) {
            return Err(Error::ForeignAuthority);
        }
        if stage.bundle.0.is_none() {
            return Err(Error::ConsumedStage);
        }
        if stage.epoch != state.epoch || !Rc::ptr_eq(&stage.configuration, &state.configuration) {
            return Err(Error::AuthorityChanged);
        }
        if stage.generation != state.generation {
            return Err(Error::StaleStage);
        }
        let configuration = state.configuration.get();
        if !configuration.factories.contains_key(&stage.factory) {
            return Err(Error::MissingFactory);
        }
        if !stage
            .proofs
            .permissions
            .is_subset(&configuration.permissions)
        {
            return Err(Error::Unauthorized);
        }
        if !stage.proofs.bindings.is_subset(&configuration.bindings) {
            return Err(Error::MissingBinding);
        }
        let next = state.generation.checked_add(1).ok_or(Error::Exhausted)?;
        // Move precomputed proof storage into the result: no allocation/callback.
        let credential = RuntimeCredential {
            domain: self.domain.clone(),
            identity: state.identity.clone(),
            epoch: state.epoch,
            generation: next,
            permissions: BTreeSet::new(),
        };
        #[cfg(test)]
        boundary();
        std::mem::swap(&mut state.bundle, &mut stage.bundle);
        state.generation = next;
        handles.push(RuntimeCredential {
            permissions: std::mem::take(&mut stage.proofs.permissions),
            ..credential
        });
        // Remove old state from the stage before opening visibility. Reentry Busy.
        let retired = stage.bundle.retire();
        let retirement = if retired {
            Retirement::Complete
        } else {
            self.domain.failed.set(true);
            Retirement::PanickedFailStop
        };
        Ok(Activation {
            handles,
            retirement,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Factory;
    impl TrustedFactory<(Box<u64>, Box<u64>)> for Factory {
        fn build(
            &self,
            _: &mut BuildContext<'_, (Box<u64>, Box<u64>)>,
            _: &[u8],
        ) -> Result<(Box<u64>, Box<u64>), Error> {
            Ok((Box::new(9), Box::new(9)))
        }
    }
    fn id() -> Binding {
        Binding {
            name: "pair".into(),
            version: 1,
            artifact: [1; 32],
        }
    }
    fn config() -> ApprovedConfiguration<(Box<u64>, Box<u64>)> {
        ApprovedConfiguration::new(
            BTreeSet::new(),
            BTreeSet::new(),
            vec![(id(), Rc::new(Factory))],
        )
        .unwrap()
    }
    fn refused<T>(result: Result<T, Error>, expected: Error) {
        assert_eq!(result.err(), Some(expected));
    }

    #[test]
    fn final_check_exchange_boundary_blocks_updates_and_every_reentrant_operation() {
        let (slot, key, credential) =
            HostSlot::new((Box::new(3), Box::new(3)), config(), Backend::Serial).unwrap();
        let mut stage = slot.prepare_restore(&key, &id(), &[]).unwrap();
        let mut other = slot.prepare_restore(&key, &id(), &[]).unwrap();
        let result = slot
            .activate_inner(&mut stage, || {
                refused(slot.replace_configuration(&key, config()), Error::Busy);
                refused(slot.rotate_authority(&key, config()), Error::Busy);
                refused(slot.capture(&key, |_| Ok(())), Error::Busy);
                refused(slot.with_runtime(&credential, |_| ()), Error::Busy);
                refused(slot.prepare_restore(&key, &id(), &[]), Error::Busy);
                refused(slot.activate(&mut other), Error::Busy);
                refused(slot.issue_credential(&key, BTreeSet::new()), Error::Busy);
            })
            .unwrap();
        assert_eq!(result.retirement, Retirement::Complete);
        assert_eq!(slot.capture(&key, |b| Ok((*b.0, *b.1))).unwrap(), (9, 9));
        slot.with_runtime(result.credential(), |_| ()).unwrap();
        assert!(!slot.domain.occupied.get());
    }

    #[test]
    fn update_before_final_guard_invalidates_activation_and_hook_unwind_releases_guard() {
        let (slot, key, credential) =
            HostSlot::new((Box::new(3), Box::new(3)), config(), Backend::Serial).unwrap();
        let mut stage = slot.prepare_restore(&key, &id(), &[]).unwrap();
        let panic = catch_unwind(AssertUnwindSafe(|| {
            let _ = slot.activate_inner(&mut stage, || panic!("test-only boundary interruption"));
        }));
        assert!(panic.is_err());
        assert!(!slot.domain.occupied.get());
        assert_eq!(slot.capture(&key, |b| Ok((*b.0, *b.1))).unwrap(), (3, 3));
        slot.with_runtime(&credential, |_| ()).unwrap();
        slot.replace_configuration(&key, config()).unwrap();
        refused(slot.activate(&mut stage), Error::AuthorityChanged);
        assert_eq!(slot.capture(&key, |b| Ok((*b.0, *b.1))).unwrap(), (3, 3));
    }

    #[test]
    fn trusted_epoch_and_bundle_generation_exhaustion_never_wrap_or_exchange() {
        let (slot, key, _) =
            HostSlot::new((Box::new(3), Box::new(3)), config(), Backend::Serial).unwrap();
        // Private boundary injection, followed by actual guarded public APIs.
        slot.state.borrow_mut().epoch = u64::MAX - 1;
        slot.replace_configuration(&key, config()).unwrap();
        let credential = slot.issue_credential(&key, BTreeSet::new()).unwrap();
        let mut stage = slot.prepare_restore(&key, &id(), &[]).unwrap();
        refused(slot.replace_configuration(&key, config()), Error::Exhausted);
        refused(slot.rotate_authority(&key, config()), Error::Exhausted);
        assert_eq!(slot.state.borrow().epoch, u64::MAX);
        slot.with_runtime(&credential, |_| ()).unwrap();
        slot.activate(&mut stage).unwrap(); // Epoch stays unchanged at max.
        slot.state.borrow_mut().generation = u64::MAX - 1;
        let mut last = slot.prepare_restore(&key, &id(), &[]).unwrap();
        let result = slot.activate(&mut last).unwrap();
        let mut exhausted = slot.prepare_restore(&key, &id(), &[]).unwrap();
        refused(slot.activate(&mut exhausted), Error::Exhausted);
        assert_eq!(slot.state.borrow().generation, u64::MAX);
        assert!(exhausted.bundle.0.is_some());
        slot.with_runtime(result.credential(), |_| ()).unwrap();
        assert!(!slot.domain.occupied.get());
    }
}
