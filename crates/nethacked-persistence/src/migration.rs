//! Host-approved, typed, one-step state migrations. This registry is never
//! deserialized from a save and does not migrate executable continuations.

/// Exact host-resolved schema and codec. Digests identify approved code; they
/// do not themselves confer authority to register or execute it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormatIdentity {
    pub schema: [u8; 32],
    pub codec_artifact: [u8; 32],
    pub codec_version: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MigrationBinding {
    pub source: FormatIdentity,
    pub target: FormatIdentity,
    pub transform_artifact: [u8; 32],
    pub transform_version: u32,
}

impl MigrationBinding {
    fn valid(self) -> bool {
        [self.source, self.target]
            .iter()
            .all(|f| f.schema != [0; 32] && f.codec_artifact != [0; 32] && f.codec_version != 0)
            && self.transform_artifact != [0; 32]
            && self.transform_version != 0
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum MigrationError<E> {
    InvalidBinding,
    Duplicate,
    Unavailable,
    InputLimit,
    Source(E),
    Transform(E),
    Target(E),
}

/// Trusted host functions must be pure with respect to externally visible
/// state. Owned input/output make mutations local to the detached migration.
/// Source decoding includes legacy syntax policy; the registry imposes no
/// canonical JSON, unknown-field or duplicate-field policy of its own.
pub struct Migration<S, T, C, E> {
    pub binding: MigrationBinding,
    pub decode: fn(&[u8], &C) -> Result<S, E>,
    pub validate_source: fn(&S, &C) -> Result<(), E>,
    pub transform: fn(S, &C) -> Result<T, E>,
    pub validate_target: fn(&T, &C) -> Result<(), E>,
}

pub struct MigrationRegistry<S, T, C, E> {
    entries: Vec<Migration<S, T, C, E>>,
}

impl<S, T, C, E> Default for MigrationRegistry<S, T, C, E> {
    fn default() -> Self {
        Self { entries: vec![] }
    }
}

impl<S, T, C, E> MigrationRegistry<S, T, C, E> {
    pub fn register(&mut self, entry: Migration<S, T, C, E>) -> Result<(), MigrationError<E>> {
        if !entry.binding.valid() {
            return Err(MigrationError::InvalidBinding);
        }
        if self.entries.iter().any(|e| e.binding == entry.binding) {
            return Err(MigrationError::Duplicate);
        }
        self.entries.push(entry);
        Ok(())
    }

    /// Resolve the entire approved tuple before parsing. There is no implicit
    /// fallback, migration chaining, or selection based on guest format labels.
    /// No transformation runs if decoding or source validation fails.
    pub fn migrate(
        &self,
        binding: MigrationBinding,
        bytes: &[u8],
        byte_limit: usize,
        context: &C,
    ) -> Result<T, MigrationError<E>> {
        if bytes.len() > byte_limit {
            return Err(MigrationError::InputLimit);
        }
        let entry = self
            .entries
            .iter()
            .find(|e| e.binding == binding)
            .ok_or(MigrationError::Unavailable)?;
        let source = (entry.decode)(bytes, context).map_err(MigrationError::Source)?;
        Self::apply(entry, source, context)
    }

    /// Apply to state already decoded by an owner codec. The caller retains
    /// responsibility for matching that codec and validating/remapping the
    /// complete reference graph (for example, `owners::Restore::migrate`).
    /// This entry still runs both registered semantic validators.
    pub fn transform(
        &self,
        binding: MigrationBinding,
        source: S,
        context: &C,
    ) -> Result<T, MigrationError<E>> {
        let entry = self
            .entries
            .iter()
            .find(|e| e.binding == binding)
            .ok_or(MigrationError::Unavailable)?;
        Self::apply(entry, source, context)
    }

    fn apply(
        entry: &Migration<S, T, C, E>,
        source: S,
        context: &C,
    ) -> Result<T, MigrationError<E>> {
        (entry.validate_source)(&source, context).map_err(MigrationError::Source)?;
        let target = (entry.transform)(source, context).map_err(MigrationError::Transform)?;
        (entry.validate_target)(&target, context).map_err(MigrationError::Target)?;
        Ok(target)
    }
}
