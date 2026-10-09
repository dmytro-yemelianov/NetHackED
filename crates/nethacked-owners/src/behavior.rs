use super::*;

/// Host-held bounded ownership of an exact codec/policy/helper capture. There is
/// no guest constructor, version selector or historical codec registry.
pub struct RetainedBehavior<T> {
    pub(super) inner: Rc<Behavior<T>>,
    pub(crate) scope: Option<Rc<CallbackScope>>,
}
pub(crate) struct CallbackScope {
    pub(crate) host: Host,
    pub(crate) origin: nethacked_kernel::CallbackProvenance,
    #[cfg(feature = "persistence")]
    pub(crate) callback: nethacked_kernel::RetainedCallback,
}
impl Drop for CallbackScope {
    fn drop(&mut self) {
        self.host.0.behaviors.set(self.host.0.behaviors.get() - 1);
    }
}
pub(super) struct Behavior<T> {
    pub(crate) owner: Weak<Owner<T>>,
    pub(crate) host: Host,
    pub(crate) generation: u64,
    pub(crate) schema: [u8; 32],
    pub(crate) caller: ModuleHandle,
    pub(crate) artifact: [u8; 32],
    pub(crate) source: OperationBinding,
    pub(super) codec: Rc<dyn WasmCodec<T>>,
    pub(super) slot: usize,
    pub(super) capture: PureCapture,
    pub(super) revision: u64,
    pub(crate) policy: QueryPolicy,
    pub(crate) target: Resource,
    pub(crate) _invocation: ModulePin,
}
impl<T> Clone for RetainedBehavior<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            scope: self.scope.clone(),
        }
    }
}
impl<T> Drop for Behavior<T> {
    fn drop(&mut self) {
        if let Some(owner) = self.owner.upgrade() {
            owner
                .metadata
                .retained_behaviors
                .set(owner.metadata.retained_behaviors.get() - 1);
        }
        self.host.0.behaviors.set(self.host.0.behaviors.get() - 1);
    }
}
impl<T> RetainedBehavior<T> {
    /// Metadata used by trusted callback retention adapters. Matching this alone
    /// confers no rights: every use validates the actual invocation and grants.
    pub fn source(&self) -> &OperationBinding {
        &self.inner.source
    }
    pub fn caller(&self) -> (ModuleHandle, [u8; 32]) {
        (self.inner.caller, self.inner.artifact)
    }
    pub fn policy(&self) -> &QueryPolicy {
        &self.inner.policy
    }
    pub fn target(&self) -> &Resource {
        &self.inner.target
    }
    pub fn matches_callback(&self, p: &nethacked_kernel::CallbackProvenance) -> bool {
        p.source == self.inner.source && (p.caller.handle, p.caller.artifact) == self.caller()
    }
    pub fn matches_source(&self, view: &CallerView<'_>) -> bool {
        view.operation() == Some(self.source())
            && view.caller() == Some(self.inner.caller)
            && view.caller_artifact() == Some(self.inner.artifact)
    }
    /// Nonexecuting aggregate admission for callback scopes in this host.
    pub fn preflight_callback_scopes(&self, additional: usize) -> Result<()> {
        let _gate = self.inner.host.close_visibility()?;
        let next = self
            .inner
            .host
            .0
            .behaviors
            .get()
            .checked_add(additional)
            .ok_or(Error::Limit)?;
        bound(next, self.inner.host.0.limits.candidates)
    }
    /// Bind only an opaque kernel-issued capture, including its exact service,
    /// capability, slot and callback endpoint. Another callback binding cannot
    /// use this reference even if it originated in the same operation.
    pub fn bind_callback(&self, callback: &nethacked_kernel::RetainedCallback) -> Result<Self> {
        let _gate = self.inner.host.close_visibility()?;
        if !self.matches_callback(callback.provenance()) {
            return Err(Error::Authority);
        }
        let count = self.inner.host.0.behaviors.get();
        if count >= self.inner.host.0.limits.candidates {
            return Err(Error::Limit);
        }
        let next = count.checked_add(1).ok_or(Error::Limit)?;
        let scope = Rc::new(CallbackScope {
            host: self.inner.host.clone(),
            origin: callback.provenance().clone(),
            #[cfg(feature = "persistence")]
            callback: callback.clone(),
        });
        self.inner.host.0.behaviors.set(next);
        Ok(Self {
            inner: self.inner.clone(),
            scope: Some(scope),
        })
    }
    pub fn matches_execution(&self, view: &CallerView<'_>) -> bool {
        self.scope
            .as_ref()
            .is_some_and(|s| view.callback_origin() == Some(&s.origin))
    }
}
impl<T: 'static> RetainedBehavior<T> {
    pub(super) fn validate(&self, owner: &Rc<Owner<T>>, view: &CallerView<'_>) -> Result<()> {
        let original = self.inner.owner.upgrade().ok_or(Error::Stale)?;
        if !Rc::ptr_eq(&original, owner)
            || self.inner.generation != owner.metadata.stamp.generation
            || self.inner.schema != owner.metadata.stamp.schema
        {
            return Err(Error::Stale);
        }
        let operation = view.operation().ok_or(Error::Authority)?;
        let same_work = operation.operation() == self.inner.source.operation()
            && operation.pipeline() == self.inner.source.pipeline()
            && operation.checkpoint() >= self.inner.source.checkpoint()
            && (operation.checkpoint() != self.inner.source.checkpoint()
                || operation.phase() == self.inner.source.phase());
        let execution = if self.scope.is_some() {
            self.matches_execution(view)
        } else {
            same_work
        };
        if !execution
            || view.caller() != Some(self.inner.caller)
            || view.caller_artifact() != Some(self.inner.artifact)
            || !operation.admits_cleanup(owner.host.0.cleanup)
        {
            return Err(Error::Authority);
        }
        // Validates the full captured guest closure against the actual retained
        // routing and current grants. A pin is never an execution capability.
        view.validate_pure(&self.inner.capture)?;
        Ok(())
    }
}
impl Host {
    pub fn retained_behaviors(&self) -> Result<usize> {
        let _gate = self.close_visibility()?;
        Ok(self.0.behaviors.get())
    }
}
impl<T: 'static> Owner<T> {
    /// A successor capture can retain this execution's old behavior without
    /// substituting the current future codec. The new binding is authenticated
    /// here, and subsequent callbacks must match its exact captured origin.
    pub fn retain_from_behavior(
        self: &Rc<Self>,
        previous: &RetainedBehavior<T>,
        name: &str,
        view: &CallerView<'_>,
    ) -> Result<RetainedBehavior<T>> {
        let _gate = self.host.close_visibility()?;
        authorize(
            &self.metadata,
            name,
            view,
            self.metadata.ports.get(name).ok_or(Error::Invalid)?.access,
        )?;
        previous.validate(self, view)?;
        let count = self.host.0.behaviors.get();
        if count >= self.host.0.limits.candidates {
            return Err(Error::Limit);
        }
        let next = count.checked_add(1).ok_or(Error::Limit)?;
        let owner_next = self
            .metadata
            .retained_behaviors
            .get()
            .checked_add(1)
            .ok_or(Error::Limit)?;
        let inner = Rc::new(Behavior {
            owner: Rc::downgrade(self),
            host: self.host.clone(),
            generation: self.metadata.stamp.generation,
            schema: self.metadata.stamp.schema,
            caller: view.caller().ok_or(Error::Authority)?,
            artifact: view.caller_artifact().ok_or(Error::Authority)?,
            source: view.operation().ok_or(Error::Authority)?.clone(),
            codec: previous.inner.codec.clone(),
            slot: previous.inner.slot,
            capture: previous.inner.capture.clone(),
            revision: previous.inner.revision,
            policy: previous.inner.policy.clone(),
            target: resource(&self.metadata, name),
            _invocation: view.pin_invocation()?,
        });
        self.host.0.behaviors.set(next);
        self.metadata.retained_behaviors.set(owner_next);
        Ok(RetainedBehavior { inner, scope: None })
    }
    /// Capture before pausing/scheduling, without querying or preparing state.
    /// Independent captures share the finite candidate bound; clones share one.
    pub fn retain_behavior(
        self: &Rc<Self>,
        name: &str,
        view: &CallerView<'_>,
    ) -> Result<RetainedBehavior<T>> {
        let _gate = self.host.close_visibility()?;
        let access = self.metadata.ports.get(name).ok_or(Error::Invalid)?.access;
        authorize(&self.metadata, name, view, access)?;
        let source = view.operation().ok_or(Error::Authority)?.clone();
        if !source.admits_cleanup(self.host.0.cleanup) {
            return Err(Error::Authority);
        }
        let count = self.host.0.behaviors.get();
        if count >= self.host.0.limits.candidates {
            return Err(Error::Limit);
        }
        let next = count.checked_add(1).ok_or(Error::Limit)?;
        let owner_next = self
            .metadata
            .retained_behaviors
            .get()
            .checked_add(1)
            .ok_or(Error::Limit)?;
        let backend = self.backend.borrow();
        let live = self.compatible.borrow();
        let (codec, slot, binding) = if let Some(live) = live.as_ref() {
            (live.codec.clone(), live.slot, &live.binding)
        } else if let Backend::Wasm {
            codec,
            slot,
            binding,
        } = &*backend
        {
            (codec.clone(), *slot, binding)
        } else {
            return Err(Error::Unsupported);
        };
        let capture = view.capture_pure(slot)?;
        if capture.binding() != binding {
            return Err(Error::Stale);
        }
        let invocation = view.pin_invocation()?;
        let inner = Rc::new(Behavior {
            owner: Rc::downgrade(self),
            host: self.host.clone(),
            generation: self.metadata.stamp.generation,
            schema: self.metadata.stamp.schema,
            caller: view.caller().ok_or(Error::Authority)?,
            artifact: view.caller_artifact().ok_or(Error::Authority)?,
            source,
            codec,
            slot,
            capture,
            revision: self.metadata.behavior_revision.get(),
            policy: self.query_policy.borrow().clone(),
            target: resource(&self.metadata, name),
            _invocation: invocation,
        });
        self.host.0.behaviors.set(next);
        self.metadata.retained_behaviors.set(owner_next);
        Ok(RetainedBehavior { inner, scope: None })
    }
    pub fn query_retained(
        self: &Rc<Self>,
        behavior: &RetainedBehavior<T>,
        name: &str,
        request: &[u8],
        observations: &[Observation],
        view: &CallerView<'_>,
    ) -> Result<Observation> {
        let _gate = self.host.close_visibility()?;
        self.query_with_behavior_gated(
            name,
            request,
            observations,
            view,
            Some(behavior),
            #[cfg(feature = "recorded-inputs")]
            &[],
        )
    }
    pub fn prepare_retained(
        self: &Rc<Self>,
        behavior: &RetainedBehavior<T>,
        name: &str,
        request: &[u8],
        observations: Vec<Observation>,
        view: &CallerView<'_>,
    ) -> Result<Candidate<T>> {
        self.prepare_with_behavior(
            name,
            request,
            observations,
            view,
            Some(behavior),
            #[cfg(feature = "recorded-inputs")]
            &[],
        )
    }
}
