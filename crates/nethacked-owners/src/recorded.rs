use super::*;

/// Host-declared immutable input protocol, independent of the eventual record ID.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordedDeclaration {
    pub policy: String,
    pub version: u32,
    pub provider: ProviderBinding,
    pub request: Vec<u8>,
}
pub(super) fn recorded_size(input: &nethacked_inputs::Observation) -> Result<usize> {
    input
        .bytes()
        .len()
        .checked_add(input.identity().request.len())
        .and_then(|n| n.checked_add(input.identity().policy.len()))
        .ok_or(Error::Limit)
}
impl<T: 'static> Owner<T> {
    /// Enrollment-time declaration. Variable external inputs always refuse the
    /// deterministic cache API; recorded query computation itself remains pure.
    pub fn declare_recorded_inputs(
        &self,
        name: &str,
        declarations: Vec<RecordedDeclaration>,
    ) -> Result<()> {
        let _gate = self.host.close_visibility()?;
        self.host.route(self.id())?;
        if self.metadata.retained_behaviors.get() != 0 {
            return Err(Error::Referenced);
        }
        if !self.metadata.ports.contains_key(name)
            || self.metadata.revision.get() != 0
            || self.metadata.pending.get() != 0
            || self.recorded.borrow().contains_key(name)
        {
            return Err(Error::Invalid);
        }
        bound(declarations.len(), self.host.0.limits.observations)?;
        let mut size = 0usize;
        for d in &declarations {
            if d.policy.is_empty() || d.provider.endpoint != nethacked_inputs::ENDPOINT {
                return Err(Error::Invalid);
            }
            size = size
                .checked_add(d.request.len())
                .and_then(|n| n.checked_add(d.policy.len()))
                .ok_or(Error::Limit)?;
        }
        bound(size, self.host.0.limits.observation_bytes)?;
        self.host.0.session.changed().map_err(|_| Error::Limit)?;
        if !declarations.is_empty() && self.metadata.ports[name].access == Access::Read {
            *self.query_policy.borrow_mut() = QueryPolicy::Noncacheable;
        }
        self.recorded.borrow_mut().insert(name.into(), declarations);
        self.clear_cache_gated();
        Ok(())
    }
    pub(super) fn validate_recorded_inputs(
        &self,
        name: &str,
        observations: &[Observation],
        recorded: &[nethacked_inputs::Observation],
        view: &CallerView<'_>,
    ) -> Result<()> {
        let declarations = self.recorded.borrow();
        let expected = declarations.get(name).map_or(&[][..], Vec::as_slice);
        if expected.len() != recorded.len() {
            return Err(Error::Invalid);
        }
        if recorded.is_empty() {
            return Ok(());
        }
        let mut count = 0usize;
        let mut bytes = 0usize;
        // Reuses recursive gated validation for complete aggregate reservations.
        for o in observations {
            validate_observation(
                &self.host,
                o,
                view,
                &mut count,
                &mut bytes,
                ObservationMode::Retained(view.operation().ok_or(Error::Authority)?),
            )?;
        }
        for (expected, input) in expected.iter().zip(recorded) {
            let identity = input.identity();
            if expected.policy != identity.policy
                || expected.version != identity.version
                || expected.provider != identity.provider
                || expected.request != identity.request
            {
                return Err(Error::Schema);
            }
            input.validate(view)?;
            count = count.checked_add(1).ok_or(Error::Limit)?;
            bytes = bytes
                .checked_add(recorded_size(input)?)
                .ok_or(Error::Limit)?;
        }
        bound(count, self.host.0.limits.observations)?;
        bound(bytes, self.host.0.limits.observation_bytes)
    }
    pub fn prepare_recorded(
        self: &Rc<Self>,
        behavior: Option<&RetainedBehavior<T>>,
        name: &str,
        request: &[u8],
        observations: Vec<Observation>,
        recorded: &[nethacked_inputs::Observation],
        view: &CallerView<'_>,
    ) -> Result<Candidate<T>> {
        self.prepare_with_behavior(name, request, observations, view, behavior, recorded)
    }
    pub fn query_recorded(
        self: &Rc<Self>,
        behavior: Option<&RetainedBehavior<T>>,
        name: &str,
        request: &[u8],
        observations: &[Observation],
        recorded: &[nethacked_inputs::Observation],
        view: &CallerView<'_>,
    ) -> Result<Observation> {
        let _gate = self.host.close_visibility()?;
        // External query inputs require an actual admitted guest helper.
        if behavior.is_none() && self.compatible.borrow().is_none() {
            return Err(Error::Unsupported);
        }
        self.query_with_behavior_gated(name, request, observations, view, behavior, recorded)
    }
}
