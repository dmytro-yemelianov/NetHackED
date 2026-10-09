//! D4a workflow-local checkpoint. D4e supplies exclusion and atomic HostBundle
//! activation. Pure bounded codecs and effect-free fresh factories are trusted
//! host implementations; incoming names never enroll either of them.
use super::*;
use nethacked_kernel::persistence::{
    ExecutionCaptureIndex, KernelSnapshot, RestoredKernel, SavedOperation,
};

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub name: String,
    pub version: u32,
    pub artifact: [u8; 32],
}
impl Identity {
    fn check(&self) -> Result<()> {
        if self.name.is_empty() || self.name.len() > 256 || self.version == 0 {
            return Err(Error::Bounds);
        }
        Ok(())
    }
}
/// Explicit reference declarations. Opaque nonreference state declares no refs.
/// Codecs must enumerate every embedded runtime reference, including duplicates
/// in their original order. Never scan bytes heuristically for handles.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Reference {
    Module { namespace: u64, id: u64 },
    Cleanup(u64),
    Operation { namespace: u64, id: u64 },
    Lease { namespace: u64, id: u64 },
    Code { lease: u64, hash: [u8; 32] },
    Resource(nethacked_kernel::Resource),
}
/// Pure, bounded old semantic validation precedes transformation. Both source
/// and destination validators/references are checked by the workflow seam.
pub trait StateCodec {
    fn identity(&self) -> Identity;
    fn validate(&self, state: &[u8], limit: usize) -> Result<Vec<Reference>>;
    fn remap(&self, state: &[u8], remap: &LocalRemap<'_>, limit: usize) -> Result<Vec<u8>>;
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramRow {
    pub identity: Identity,
    pub provider: u64,
    pub artifact: [u8; 32],
    pub endpoint: String,
    pub authority: Authority,
    pub initial: String,
    pub phases: BTreeSet<String>,
    pub allowed: BTreeSet<(String, String)>,
    pub codec: Identity,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticipantRow {
    pub id: u64,
    pub build: Identity,
    pub cleanup: Option<u64>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameRow {
    pub source: SavedOperation,
    pub program: ProgramRow,
    pub state: Vec<u8>,
    pub references: Vec<Reference>,
    pub trace: Vec<Vec<u8>>,
    pub status: Status,
    pub participants: Vec<u64>,
    pub cleanup_pending: bool,
    pub cleaned: usize,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    pub version: u32,
    pub namespace: u64,
    pub frames: Vec<FrameRow>,
    pub participants: Vec<ParticipantRow>,
}
struct CaptureProgram {
    operation: OperationId,
    identity: Identity,
    codec: Rc<dyn StateCodec>,
}
struct CaptureParticipant {
    build: Identity,
    value: Rc<dyn CleanupParticipant>,
}
/// Registration is tied to this actual Workflows allocation and its actual
/// participant Rc allocations. Every operation and participant needs one entry.
pub struct CaptureCatalog {
    owner: Workflows,
    programs: Vec<CaptureProgram>,
    participants: Vec<CaptureParticipant>,
}
impl CaptureCatalog {
    pub fn new(owner: &Workflows) -> Self {
        Self {
            owner: owner.clone(),
            programs: vec![],
            participants: vec![],
        }
    }
    pub fn program(
        &mut self,
        operation: OperationId,
        identity: Identity,
        codec: Rc<dyn StateCodec>,
    ) -> Result<()> {
        identity.check()?;
        codec.identity().check()?;
        self.owner.inspect(operation)?;
        if self.programs.iter().any(|p| p.operation == operation) {
            return Err(Error::Status);
        }
        self.programs.push(CaptureProgram {
            operation,
            identity,
            codec,
        });
        Ok(())
    }
    pub fn participant(
        &mut self,
        build: Identity,
        value: Rc<dyn CleanupParticipant>,
    ) -> Result<()> {
        build.check()?;
        if self.participants.len() >= 65_536
            || self
                .participants
                .iter()
                .any(|p| Rc::ptr_eq(&p.value, &value))
        {
            return Err(Error::Bounds);
        }
        let inner = self.owner.0.try_borrow().map_err(|_| Error::Busy)?;
        if !inner
            .frames
            .values()
            .any(|o| o.participants.iter().any(|p| Rc::ptr_eq(p, &value)))
        {
            return Err(Error::Unknown);
        }
        self.participants.push(CaptureParticipant { build, value });
        Ok(())
    }
}
/// Independent frame approval also authenticates sequence/trace/enrollment and
/// pending cleanup cursor. A transport digest or a copied incoming row is not
/// authority. D4e composes this with its independent full context approval.
pub trait FrameAuthority {
    fn approve(
        &self,
        _namespace: u64,
        _frame: &FrameRow,
        _participants: &[ParticipantRow],
    ) -> Result<()> {
        Err(Error::Status)
    }
}
type ParticipantFactory =
    Box<dyn Fn(Option<CleanupIdentity>) -> Result<Rc<dyn CleanupParticipant>>>;
#[derive(Default)]
pub struct RestoreCatalog {
    programs: Vec<(ProgramRow, Rc<dyn StateCodec>)>,
    participants: BTreeMap<Identity, ParticipantFactory>,
}
impl RestoreCatalog {
    pub fn program(&mut self, program: ProgramRow, codec: Rc<dyn StateCodec>) -> Result<()> {
        program.identity.check()?;
        program.codec.check()?;
        if codec.identity() != program.codec
            || self
                .programs
                .iter()
                .any(|(p, _)| p.identity == program.identity)
        {
            return Err(Error::Status);
        }
        self.programs.push((program, codec));
        Ok(())
    }
    pub fn participant(
        &mut self,
        build: Identity,
        factory: impl Fn(Option<CleanupIdentity>) -> Result<Rc<dyn CleanupParticipant>> + 'static,
    ) -> Result<()> {
        build.check()?;
        if self.participants.len() >= 65_536 || self.participants.contains_key(&build) {
            return Err(Error::Bounds);
        }
        self.participants.insert(build, Box::new(factory));
        Ok(())
    }
    fn codec(&self, row: &ProgramRow) -> Result<&dyn StateCodec> {
        self.programs
            .iter()
            .find(|(p, c)| p == row && c.identity() == row.codec)
            .map(|(_, c)| c.as_ref())
            .ok_or(Error::Status)
    }
}
impl Workflows {
    /// Enroll exact bindings in D3's kernel snapshot. Includes terminal and
    /// safe-boundary Running operations; pending cleanup keeps its exact epoch.
    pub fn persistence_enrollment(&self, kernel: &Kernel) -> Result<Vec<OperationBinding>> {
        let inner = self.0.try_borrow_mut().map_err(|_| Error::Busy)?;
        inner
            .frames
            .values()
            .map(|o| match &o.cleanup {
                Some(b) => Ok(b.clone()),
                None => Ok(kernel.operation_binding_with_cleanup(
                    o.frame.operation,
                    o.lease,
                    o.frame.sequence,
                    &o.frame.phase,
                    &o.identities,
                )?),
            })
            .collect()
    }
    pub fn capture_local(
        &self,
        kernel: &Kernel,
        index: &ExecutionCaptureIndex<'_>,
        catalog: &CaptureCatalog,
    ) -> Result<Checkpoint> {
        if !Rc::ptr_eq(&self.0, &catalog.owner.0) {
            return Err(Error::Status);
        }
        let inner = self.0.try_borrow_mut().map_err(|_| Error::Busy)?;
        if catalog.programs.len() != inner.frames.len() {
            return Err(Error::Status);
        }
        let mut participants = vec![];
        let mut used = BTreeSet::new();
        let mut frames = vec![];
        for o in inner.frames.values() {
            let registered = catalog
                .programs
                .iter()
                .find(|p| p.operation == o.frame.operation)
                .ok_or(Error::Status)?;
            let binding = match &o.cleanup {
                Some(b) => b.clone(),
                None => kernel.operation_binding_with_cleanup(
                    o.frame.operation,
                    o.lease,
                    o.frame.sequence,
                    &o.frame.phase,
                    &o.identities,
                )?,
            };
            let source = index.source(&binding)?;
            let p = &o.frame.program;
            let program = ProgramRow {
                identity: registered.identity.clone(),
                provider: index.module(p.provider)?,
                artifact: kernel.artifact(p.provider)?,
                endpoint: p.endpoint.clone(),
                authority: p.authority.clone(),
                initial: p.initial.clone(),
                phases: p.phases.clone(),
                allowed: p.allowed.clone(),
                codec: registered.codec.identity(),
            };
            let mut enrollment = vec![];
            let mut identities = vec![];
            for participant in &o.participants {
                let (i, entry) = catalog
                    .participants
                    .iter()
                    .enumerate()
                    .find(|(_, c)| Rc::ptr_eq(&c.value, participant))
                    .ok_or(Error::Status)?;
                let id = i as u64 + 1;
                let actual = participant.identity()?;
                if let Some(identity) = actual {
                    identities.push(identity);
                }
                let cleanup = actual.map(|c| index.cleanup(c)).transpose()?;
                if used.insert(id) {
                    participants.push(ParticipantRow {
                        id,
                        build: entry.build.clone(),
                        cleanup,
                    });
                }
                enrollment.push(id);
            }
            if identities != o.identities {
                return Err(Error::Status);
            }
            let references = registered
                .codec
                .validate(&o.frame.state, inner.limits.state_bytes)?;
            frames.push(FrameRow {
                source,
                program,
                state: o.frame.state.clone(),
                references,
                trace: o.frame.trace.clone(),
                status: o.frame.status.clone(),
                participants: enrollment,
                cleanup_pending: o.frame.cleanup_pending,
                cleaned: o.cleaned,
            });
        }
        if used.len() != catalog.participants.len() {
            return Err(Error::Status);
        }
        participants.sort_by_key(|p| p.id);
        let checkpoint = Checkpoint {
            version: 1,
            namespace: index.namespace(),
            frames,
            participants,
        };
        checkpoint.bounds(&inner.limits)?;
        Ok(checkpoint)
    }
}
/// Maps reference-bearing local state without giving a codec a Kernel or an
/// executable operation credential. IDs here are explicitly declared transport
/// references; module/cleanup values returned are fresh *serials*, never handles.
pub struct LocalRemap<'a> {
    mapped: &'a RestoredKernel,
    kernel: &'a Kernel,
}
impl LocalRemap<'_> {
    pub fn reference(&self, old: &Reference) -> Result<Reference> {
        Ok(match old {
            Reference::Module { id, .. } => {
                let (namespace, id) = self.mapped.execution_module_identity(*id)?;
                Reference::Module { namespace, id }
            }
            Reference::Cleanup(id) => {
                Reference::Cleanup(self.mapped.execution_cleanup_identity(*id)?)
            }
            Reference::Operation { id, .. } => {
                let (namespace, id) = self.mapped.execution_operation_identity(*id)?;
                Reference::Operation { namespace, id }
            }
            Reference::Lease { id, .. } => {
                let (namespace, id) = self.mapped.execution_lease_identity(*id)?;
                Reference::Lease { namespace, id }
            }
            Reference::Code { lease, .. } => {
                let mapped = *self.mapped.leases().get(lease).ok_or(Error::Unknown)?;
                let (_, lease) = self.mapped.execution_lease_identity(*lease)?;
                Reference::Code {
                    lease,
                    hash: self.kernel.pipeline_descriptor(mapped)?.hash,
                }
            }
            Reference::Resource(r) => {
                Reference::Resource(self.mapped.execution_resource(r)?.clone())
            }
        })
    }
}
/// Sealed approved old semantics; no host handle can escape an incomplete build.
/// One-use local reconstruction does not activate or replace a host session.
pub struct Validated {
    checkpoint: Checkpoint,
    limits: Limits,
}
impl Checkpoint {
    pub fn to_bytes(&self, max_bytes: usize) -> Result<Vec<u8>> {
        let bytes = serde_json::to_vec(&serde_json::to_value(self).map_err(|_| Error::Bounds)?)
            .map_err(|_| Error::Bounds)?;
        if bytes.len() > max_bytes.min(64 * 1024 * 1024) {
            return Err(Error::Bounds);
        }
        Ok(bytes)
    }
    pub fn from_bytes(bytes: &[u8], max_bytes: usize) -> Result<Self> {
        if bytes.len() > max_bytes.min(64 * 1024 * 1024) {
            return Err(Error::Bounds);
        }
        bounded_json(bytes)?;
        let value: Self = serde_json::from_slice(bytes).map_err(|_| Error::Bounds)?;
        if value.to_bytes(max_bytes)? != bytes {
            return Err(Error::Bounds);
        }
        Ok(value)
    }
    fn bounds(&self, l: &Limits) -> Result<()> {
        if self.frames.len() > 65_536
            || self.participants.len() > 65_536
            || self.version != 1
            || self.namespace == 0
            || self.frames.len() > l.operations
            || self.participants.len()
                > l.operations
                    .checked_mul(l.participants)
                    .ok_or(Error::Bounds)?
        {
            return Err(Error::Bounds);
        }
        let mut edges = 0usize;
        let mut operations = BTreeSet::new();
        let mut leases = BTreeSet::new();
        let mut used = BTreeSet::new();
        let mut cleanup_ids = BTreeSet::new();
        if self
            .frames
            .windows(2)
            .any(|f| f[0].source.operation >= f[1].source.operation)
        {
            return Err(Error::Status);
        }
        if self.participants.windows(2).any(|p| p[0].id >= p[1].id) {
            return Err(Error::Status);
        }
        for p in &self.participants {
            p.build.check()?;
            if p.id == 0
                || p.cleanup == Some(0)
                || p.cleanup.is_some_and(|id| !cleanup_ids.insert(id))
            {
                return Err(Error::Status);
            }
        }
        for f in &self.frames {
            edges = edges
                .checked_add(f.references.len())
                .and_then(|n| n.checked_add(f.participants.len()))
                .and_then(|n| n.checked_add(f.program.allowed.len()))
                .ok_or(Error::Bounds)?;
            if edges > 262_144 {
                return Err(Error::Bounds);
            }
            let p = &f.program;
            p.identity.check()?;
            p.codec.check()?;
            if !operations.insert(f.source.operation)
                || !leases.insert(f.source.pipeline)
                || f.source.operation == 0
                || f.source.pipeline == 0
                || p.provider == 0
                || p.endpoint.is_empty()
                || p.endpoint.len() > l.phase_bytes
                || p.phases.len() > l.phases
                || p.allowed.len() > l.edges
                || !p.phases.contains(&p.initial)
                || !p.phases.contains(&f.source.phase)
                || p.phases
                    .iter()
                    .any(|s| s.is_empty() || s.len() > l.phase_bytes)
                || p.allowed
                    .iter()
                    .any(|(a, b)| !p.phases.contains(a) || !p.phases.contains(b))
                || f.state.len() > l.state_bytes
                || f.trace.len() > l.trace_events
                || f.trace.iter().any(|e| e.len() > l.event_bytes)
                || f.references.len() > 262_144
                || f.participants.len() > l.participants
                || f.cleaned > f.participants.len()
            {
                return Err(Error::Bounds);
            }
            if match &f.status {
                Status::Suspended(v) | Status::Finished(v) => v.len() > l.value_bytes,
                _ => false,
            } {
                return Err(Error::Bounds);
            }
            if f.cleanup_pending
                && (!matches!(f.status, Status::Faulted(_)) || f.cleaned == f.participants.len())
                || !f.cleanup_pending && f.cleaned != 0 && f.cleaned != f.participants.len()
            {
                return Err(Error::Status);
            }
            let mut cleanups = vec![];
            for id in &f.participants {
                let participant = self
                    .participants
                    .iter()
                    .find(|p| p.id == *id)
                    .ok_or(Error::Status)?;
                if let Some(c) = participant.cleanup {
                    cleanups.push(c);
                }
                used.insert(*id);
            }
            cleanups.sort_unstable();
            cleanups.dedup();
            if cleanups != f.source.cleanup {
                return Err(Error::Status);
            }
            encode_request(l, f.source.checkpoint, &f.source.phase, &f.state, &[])?;
        }
        if used.len() != self.participants.len() {
            return Err(Error::Status);
        }
        Ok(())
    }
    pub fn validate(
        &self,
        kernel: &KernelSnapshot,
        limits: Limits,
        catalog: &RestoreCatalog,
        authority: &impl FrameAuthority,
    ) -> Result<Validated> {
        Workflows::new(limits.clone())?;
        self.bounds(&limits)?;
        self.to_bytes(64 * 1024 * 1024)?;
        if self.namespace != kernel.source_namespace {
            return Err(Error::Status);
        }
        // Resolve ALL programs/codecs/participants before old validation or any
        // domain factory. Exact D3 source membership is necessary, not authority.
        for f in &self.frames {
            catalog.codec(&f.program)?;
            if !kernel.operations.contains(&f.source)
                || !kernel
                    .registry
                    .modules
                    .iter()
                    .any(|m| m.id == f.program.provider && m.artifact == f.program.artifact)
            {
                return Err(Error::Status);
            }
            for r in &f.references {
                match r {
                    Reference::Module { namespace, id }
                        if *namespace == self.namespace
                            && kernel.registry.modules.iter().any(|m| m.id == *id) => {}
                    Reference::Cleanup(id) if kernel.cleanup_history.contains(id) => (),
                    Reference::Operation { namespace, id }
                        if *namespace == self.namespace
                            && kernel.operations.iter().any(|o| o.operation == *id) => {}
                    Reference::Lease { namespace, id }
                        if *namespace == self.namespace
                            && kernel.leases.iter().any(|l| l.id == *id) => {}
                    Reference::Code { lease, hash }
                        if kernel
                            .leases
                            .iter()
                            .any(|l| l.id == *lease && l.pipeline.hash == *hash) => {}
                    Reference::Resource(r) if f.program.authority.scope.allows(r) => (),
                    _ => return Err(Error::Status),
                }
            }
        }
        for p in &self.participants {
            if !catalog.participants.contains_key(&p.build)
                || p.cleanup
                    .is_some_and(|c| !kernel.cleanup_history.contains(&c))
            {
                return Err(Error::Status);
            }
        }
        for f in &self.frames {
            authority.approve(self.namespace, f, &self.participants)?;
        }
        // Independent approval covers the whole checkpoint before domain code.
        for f in &self.frames {
            let references = catalog
                .codec(&f.program)?
                .validate(&f.state, limits.state_bytes)?;
            if references != f.references {
                return Err(Error::Status);
            }
        }
        Ok(Validated {
            checkpoint: self.clone(),
            limits,
        })
    }
}
impl Validated {
    pub fn rebuild(
        self,
        kernel: &Kernel,
        mapped: &RestoredKernel,
        catalog: &RestoreCatalog,
    ) -> Result<Workflows> {
        let c = &self.checkpoint;
        if c.namespace != mapped.execution_namespace() {
            return Err(Error::Status);
        }
        mapped.execution_live(kernel)?;
        let enrollment = mapped.enroll_workflows(
            &c.frames
                .iter()
                .map(|f| f.source.clone())
                .collect::<Vec<_>>(),
        )?;
        let remap = LocalRemap { mapped, kernel };
        // Complete destination proof for every frame before domain remapping.
        let mut approved = vec![];
        for f in &c.frames {
            let codec = catalog.codec(&f.program)?;
            let binding = mapped.execution_source(&f.source)?;
            let pipeline = kernel.pipeline_descriptor(binding.pipeline())?.clone();
            let provider = *mapped
                .modules()
                .get(&f.program.provider)
                .ok_or(Error::Unknown)?;
            if kernel.artifact(provider)? != f.program.artifact {
                return Err(Error::Status);
            }
            let authority = Authority {
                access: f.program.authority.access,
                scope: match &f.program.authority.scope {
                    nethacked_kernel::Scope::All => nethacked_kernel::Scope::All,
                    nethacked_kernel::Scope::Keys(keys) => nethacked_kernel::Scope::Keys(
                        keys.iter()
                            .map(|r| mapped.execution_resource(r).cloned())
                            .collect::<nethacked_kernel::Result<_>>()?,
                    ),
                },
            };
            if pipeline.ceiling != authority {
                return Err(Error::Status);
            }
            let references = f
                .references
                .iter()
                .map(|r| remap.reference(r))
                .collect::<Result<Vec<_>>>()?;
            let program = Program {
                provider,
                endpoint: f.program.endpoint.clone(),
                authority,
                initial: f.program.initial.clone(),
                phases: f.program.phases.clone(),
                allowed: f.program.allowed.clone(),
            };
            approved.push((binding, pipeline, program, references, codec));
        }
        for p in &c.participants {
            if !catalog.participants.contains_key(&p.build) {
                return Err(Error::Status);
            }
            if p.cleanup
                .is_some_and(|id| !mapped.cleanup().contains_key(&id))
            {
                return Err(Error::Unknown);
            }
        }
        // Exact old validation already succeeded; validate all fresh states
        // before constructing participant allocations.
        let mut frames = vec![];
        for (f, (binding, pipeline, program, references, codec)) in c.frames.iter().zip(approved) {
            let state = codec.remap(&f.state, &remap, self.limits.state_bytes)?;
            if state.len() > self.limits.state_bytes
                || codec.validate(&state, self.limits.state_bytes)? != references
            {
                return Err(Error::Status);
            }
            frames.push((binding, pipeline, state, program));
        }
        let mut participants = BTreeMap::new();
        for p in &c.participants {
            let identity = p
                .cleanup
                .map(|id| mapped.cleanup().get(&id).copied().ok_or(Error::Unknown))
                .transpose()?;
            let value = catalog.participants.get(&p.build).ok_or(Error::Status)?(identity)?;
            if value.identity()? != identity
                || participants.values().any(|other| Rc::ptr_eq(other, &value))
            {
                return Err(Error::Status);
            }
            participants.insert(p.id, value);
        }
        let owner = Workflows::new(self.limits)?;
        {
            let mut inner = owner.0.borrow_mut();
            for (f, (binding, pipeline, state, program)) in c.frames.iter().zip(frames) {
                let enrollment = f
                    .participants
                    .iter()
                    .map(|id| participants.get(id).cloned().ok_or(Error::Unknown))
                    .collect::<Result<Vec<_>>>()?;
                let identities = f
                    .participants
                    .iter()
                    .filter_map(|id| {
                        c.participants
                            .iter()
                            .find(|p| p.id == *id)
                            .and_then(|p| p.cleanup)
                    })
                    .map(|id| mapped.cleanup().get(&id).copied().ok_or(Error::Unknown))
                    .collect::<Result<Vec<_>>>()?;
                let id = binding.operation();
                inner.frames.insert(
                    id,
                    Owned {
                        frame: Frame {
                            operation: id,
                            sequence: binding.checkpoint(),
                            pipeline,
                            program,
                            state,
                            phase: binding.phase().into(),
                            trace: f.trace.clone(),
                            status: f.status.clone(),
                            cleanup_pending: f.cleanup_pending,
                        },
                        lease: binding.pipeline(),
                        participants: enrollment,
                        identities,
                        cleanup: f.cleanup_pending.then_some(binding),
                        cleaned: f.cleaned,
                        skip_cleaned: true,
                    },
                );
            }
        }
        enrollment.finish();
        Ok(owner)
    }
}

// Bound nesting before serde allocation; escaped brackets in strings do not
// affect depth. Canonical typed decoding rejects malformed syntax afterwards.
fn bounded_json(bytes: &[u8]) -> Result<()> {
    let mut depth = 0usize;
    let mut quoted = false;
    let mut escape = false;
    for b in bytes {
        if quoted {
            if escape {
                escape = false;
            } else if *b == b'\\' {
                escape = true;
            } else if *b == b'"' {
                quoted = false;
            }
        } else {
            match b {
                b'"' => quoted = true,
                b'{' | b'[' => {
                    depth += 1;
                    if depth > 32 {
                        return Err(Error::Bounds);
                    }
                }
                b'}' | b']' => depth = depth.checked_sub(1).ok_or(Error::Bounds)?,
                _ => (),
            }
        }
    }
    if depth != 0 || quoted {
        return Err(Error::Bounds);
    }
    Ok(())
}

/// Shared bounded canonical local-checkpoint transport, without runtime authority.
pub fn encode_local_checkpoint<T: Serialize>(value: &T, max_bytes: usize) -> Result<Vec<u8>> {
    let bytes = serde_json::to_vec(&serde_json::to_value(value).map_err(|_| Error::Bounds)?)
        .map_err(|_| Error::Bounds)?;
    if bytes.len() > max_bytes.min(64 * 1024 * 1024) {
        return Err(Error::Bounds);
    }
    bounded_json(&bytes)?;
    Ok(bytes)
}

pub fn decode_local_checkpoint<T: serde::de::DeserializeOwned + Serialize>(
    bytes: &[u8],
    max_bytes: usize,
) -> Result<T> {
    if bytes.len() > max_bytes.min(64 * 1024 * 1024) {
        return Err(Error::Bounds);
    }
    bounded_json(bytes)?;
    let value: T = serde_json::from_slice(bytes).map_err(|_| Error::Bounds)?;
    if encode_local_checkpoint(&value, max_bytes)? != bytes {
        return Err(Error::Bounds);
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn held_mutable_and_shared_borrows_are_busy() {
        let mut kernel = Kernel::new(nethacked_kernel::Limits::default()).unwrap();
        let owner = Workflows::new(Limits::default()).unwrap();
        let catalog = CaptureCatalog::new(&owner);
        let saved = kernel.kernel_snapshot(&Default::default()).unwrap();
        let index = kernel.execution_capture_index(&saved).unwrap();
        let shared = owner.0.borrow();
        assert!(matches!(
            owner.capture_local(&kernel, &index, &catalog),
            Err(Error::Busy)
        ));
        drop(shared);
        let exclusive = owner.0.borrow_mut();
        assert!(matches!(
            owner.persistence_enrollment(&kernel),
            Err(Error::Busy)
        ));
        assert!(matches!(
            owner.capture_local(&kernel, &index, &catalog),
            Err(Error::Busy)
        ));
        drop(exclusive);
        assert!(owner.capture_local(&kernel, &index, &catalog).is_ok());
    }
}
