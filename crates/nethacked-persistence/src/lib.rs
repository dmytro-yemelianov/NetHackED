//! D1 wire transport plus separate host-only serial runtime orchestration.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
pub mod runtime;
#[cfg(feature = "session")]
pub mod session;
mod syntax;

pub const MAGIC: &[u8; 8] = b"NHOWNR01";
pub const HEADER_LEN: usize = 52;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaveLimits {
    pub wire_bytes: usize,
    pub payload_bytes: usize,
    pub rows_per_table: usize,
    pub aggregate_edges: usize,
    pub name_bytes: usize,
    pub depth: usize,
}
impl Default for SaveLimits {
    fn default() -> Self {
        Self {
            wire_bytes: 64 * 1024 * 1024,
            payload_bytes: 16 * 1024 * 1024,
            rows_per_table: 65_536,
            aggregate_edges: 262_144,
            name_bytes: 256,
            depth: 32,
        }
    }
}
impl SaveLimits {
    fn checked(self) -> Result<Self, Error> {
        let hard = Self::default();
        if self.wire_bytes > hard.wire_bytes
            || self.payload_bytes > hard.payload_bytes
            || self.rows_per_table > hard.rows_per_table
            || self.aggregate_edges > hard.aggregate_edges
            || self.name_bytes > hard.name_bytes
            || self.depth > hard.depth
        {
            return Err(Error::Limits);
        }
        Ok(self)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Limits,
    Truncated,
    Magic,
    Version,
    Length,
    Digest,
    Json,
    Noncanonical,
    Depth,
    String,
    Payload,
    Rows,
    Edges,
    Hex,
    Id,
    Counter,
    Reference,
    Schema,
    Allocation,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

macro_rules! record {
    ($name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        pub struct $name { $(#[serde(deserialize_with = "required")] pub $field: $ty),* }
    };
}
fn required<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdKind {
    Module,
    Owner,
    Operation,
    Lease,
    Callback,
    Pin,
    Candidate,
    Batch,
    Cleanup,
    InputService,
    InputRecord,
    Timer,
    Reservation,
    Workflow,
    Publication,
}
record!(SaveId {
    kind: IdKind,
    serial: u64
});
impl PartialOrd for SaveId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for SaveId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.kind, self.serial).cmp(&(other.kind, other.serial))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceMode {
    Live,
    Historical,
}
record!(Reference {
    target: SaveId,
    mode: ReferenceMode
});
record!(Identity {
    name: String,
    version: u64,
    artifact: Hex
});
/// Lowercase hexadecimal bytes. Digest fields additionally require exactly 32 bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Hex(pub String);
impl Hex {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        const HEX: &[u8] = b"0123456789abcdef";
        let mut s = String::with_capacity(bytes.len().saturating_mul(2));
        for b in bytes {
            s.push(HEX[(b >> 4) as usize] as char);
            s.push(HEX[(b & 15) as usize] as char);
        }
        Self(s)
    }
    pub fn bytes(&self, limits: SaveLimits) -> Result<Vec<u8>, Error> {
        limits.checked()?;
        check_hex(self, limits.payload_bytes)?;
        let mut out = Vec::new();
        out.try_reserve_exact(self.0.len() / 2)
            .map_err(|_| Error::Allocation)?;
        for pair in self.0.as_bytes().chunks_exact(2) {
            let digit = |b| if b <= b'9' { b - b'0' } else { b - b'a' + 10 };
            out.push(digit(pair[0]) * 16 + digit(pair[1]));
        }
        Ok(out)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutableKind {
    Native,
    Wasm,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    Read,
    Write,
}
record!(Capability {
    provider: Reference,
    endpoint: String,
    major: u32
});
record!(Resource {
    owner: Reference,
    key: String
});
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Scope {
    All,
    Keys { resources: Vec<Resource> },
}
record!(GrantRequest {
    capability: Capability,
    access: Access,
    scope: Scope
});
record!(ImportSlot {
    name: String,
    module: Reference,
    port: String,
    capability: Capability,
    access: Access
});
record!(OwnerDependency {
    owner: Reference,
    query: String,
    schema: Identity
});
record!(Port { name: String, schema: Identity, access: Access, major: u32, dependencies: Vec<OwnerDependency> });
record!(Endpoint {
    name: String,
    major: u32,
    access: Access,
    max_request: u32,
    max_reply: u32
});
record!(Extension {
    target: Capability,
    export: String
});
record!(ConfigEntry {
    name: String,
    payload: Hex
});
record!(Manifest { name: String, version: u64, ports: Vec<Port>, configuration: Vec<ConfigEntry>, exports: Vec<Endpoint>, extension_points: Vec<String>, extension: Option<Extension> });
record!(ModuleRow { id: SaveId, artifact: Hex, executable: ExecutableKind, abi: Identity,
    manifest: Manifest, guest_bytes: Option<Hex>, extension_target: Option<Reference>,
    imports: Vec<ImportSlot>, requested_grants: Vec<GrantRequest> });
record!(Route {
    name: String,
    module: Reference,
    port: String,
    major: u32,
    access: Access
});
record!(DispatchRow { owner: Reference, routes: Vec<Route> });
record!(OwnerRow { id: SaveId, module: Reference, incarnation: u64, generation: u64, revision: u64,
    schema: Identity, backend: Identity, codec: Identity, policy: Identity,
    ports: Vec<Port>, dependencies: Vec<OwnerDependency>, payload: Hex });
record!(Stamp {
    owner: Reference,
    module: Reference,
    incarnation: u64,
    generation: u64,
    revision: u64,
    schema: Identity
});
record!(Event {
    source: Reference,
    sequence: u64,
    schema: Identity,
    payload: Hex
});
// Detached native declarations: structural evidence only, never restored permits.
record!(NamedCapability {
    provider_name: String,
    endpoint: String,
    major: u32
});
record!(NamedResource {
    owner_name: String,
    key: String
});
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EffectiveScope {
    All,
    Keys { resources: Vec<NamedResource> },
}
record!(Authority {
    access: Access,
    scope: EffectiveScope
});
record!(ProviderBinding {
    module: Reference,
    artifact: Hex,
    endpoint: String
});
record!(Permit { capability: NamedCapability, resolution: Vec<ProviderBinding>, effective: Authority });
record!(OperationBinding { operation: Reference, lease: Reference, checkpoint: u64,
    phase: String, cleanup_identities: Vec<u64> });
record!(ReceiptStamp {
    owner: Reference,
    module: ProviderBinding,
    generation: u64,
    revision: u64,
    schema_digest: Hex
});
record!(HelperBinding {
    permit: Permit,
    authority: Authority
});
record!(HelperCapture { provider: Reference, slot: u64, ceiling: Authority,
    closure: Vec<HelperBinding>, code: Reference });
record!(RecordedInputIdentity {
    policy_name: String,
    version: u32,
    record: Reference,
    provider: ProviderBinding,
    request: Hex,
    binding: OperationBinding,
    caller: Reference,
    caller_artifact: Hex,
    permit: Permit
});
record!(ObservationReceipt { stamp: ReceiptStamp, query: String, caller: Reference,
    caller_artifact: Hex, permit: Permit, data_digest: Hex,
    dependencies: Vec<ObservationReceipt>, recorded_inputs: Vec<RecordedInputIdentity> });
record!(ObservationMetadata { receipt: ObservationReceipt, data: Hex,
    dependencies: Vec<ObservationMetadata>, code: Reference, behavior_revision: u64,
    helper: Option<HelperCapture>, execution: Option<OperationBinding> });
record!(ParticipantReceipt { serial: u64, stamp: ReceiptStamp, caller: Reference,
    caller_artifact: Hex, binding: OperationBinding, command: String,
    permit: Permit, behavior: Vec<ProviderBinding>, next_digest: Hex,
    event_count: u64, request_digest: Hex, recorded_inputs: Vec<RecordedInputIdentity> });
record!(PreparedMetadata { dependencies: Vec<ObservationMetadata>, code: Vec<Reference>,
    helper: Option<HelperCapture>, recorded_inputs: Vec<RecordedInputIdentity> });
record!(BatchDeclaration { caller: Reference, caller_artifact: Hex,
    binding: OperationBinding, coordinator: Permit, provider: ProviderBinding,
    required_writes: Vec<Reference>, effect_order: Vec<Reference>,
    readonly: Vec<ObservationMetadata>, retained_code: Reference, policy: Identity });
record!(PublicationReceipt { issuing_host: u64, ticket: u64, epoch: u64,
    caller: Reference, caller_artifact: Hex, binding: OperationBinding,
    coordinator: Permit, policy: Identity, participants: Vec<ParticipantReceipt>,
    observations: Vec<ObservationReceipt>, events: Vec<Hex>, reply: Hex,
    history_encoding: Hex });
record!(PublicationRow { id: SaveId, epoch: u64, operation: Reference, batch: Option<Reference>,
    accepted_events: Vec<Event>, delivery_cursor: u64, receipt: PublicationReceipt,
    declaration: BatchDeclaration, accounting: Identity, accounting_payload: Hex });
record!(CandidateRow { id: SaveId, owner: Reference, operation: Reference, stamp: Stamp,
    codec: Identity, policy: Identity, retained_modules: Vec<Reference>, readonly: Vec<Stamp>,
    observations: Vec<Reference>, next_payload: Hex, events: Vec<Event>, reply: Hex,
    receipt: ParticipantReceipt, prepared: PreparedMetadata });
record!(BatchRow { id: SaveId, operation: Reference, checkpoint: u64, ticket: u64,
    issuing_host: u64, declaration: BatchDeclaration,
    candidates: Vec<Reference>, accounting: Identity, accounting_payload: Hex });
record!(PublicationDomain {
    issuing_host: u64,
    high_water: u64
});
record!(OperationRow {
    id: SaveId,
    owner: Reference,
    sequence: u64,
    checkpoint: u64
});
record!(LeaseRow { id: SaveId, operation: Reference, modules: Vec<Reference> });
record!(PinRow {
    id: SaveId,
    module: Reference,
    origin: Reference
});
record!(CallbackRow { id: SaveId, source: Reference, targets: Vec<Stamp>, modules: Vec<Reference>,
    codec: Identity, policy: Identity, reservation: Reference, payload: Hex });
record!(ReservationRow { id: SaveId, modules: Vec<Reference>, owners: Vec<Reference>,
    codec: Identity, policy: Identity, scope: Vec<GrantRequest>, payload: Hex });
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Paused,
    Running,
    Completed,
    Failed,
    Cancelled,
    CleanupBusy,
}
record!(WorkflowTicket {
    operation: Reference,
    sequence: u64,
    lease: Reference,
    pipeline_digest: Hex,
    phase: String
});
record!(WorkflowRow { id: SaveId, operation: Reference, lease: Reference, program: Identity,
    program_payload: Hex, phase: String, sequence: u64, checkpoint: u64,
    pipeline_digest: Hex, status: Status, local_payload: Hex,
    trace: Vec<Hex>, ticket: Option<WorkflowTicket>, accepted_publications: Vec<Reference>,
    accepted_inputs: Vec<Reference>, cleanup: Vec<Reference> });
record!(CleanupRow {
    id: SaveId,
    operation: Reference,
    lease: Reference,
    participant: Identity,
    reservation: Reference,
    outstanding: bool,
    outcome: Hex,
    payload: Hex
});
record!(InputServiceRow { id: SaveId, source: Identity, policy: Identity, source_payload: Hex,
    ledger: Vec<Reference>, ordinals: Vec<Ordinal> });
record!(Ordinal {
    operation: Reference,
    high_water: u64
});
record!(InputRecordRow { id: SaveId, service: Reference, operation: Reference, ordinal: u64,
    request: Hex, value: Hex, provenance: Identity, observations: Vec<Stamp> });
record!(TimerRow { id: SaveId, task: Identity, profile: Identity, operation: Reference,
    lease: Reference, callback: Reference, deadline: u64, tie: u64, residency: String,
    status: Status, delivery_cursor: u64, payload: Hex, cleanup: Vec<Reference> });
record!(RetentionEdge {
    holder: Reference,
    retained: Reference
});
record!(Counter {
    kind: IdKind,
    high_water: u64
});
record!(Tombstone {
    id: SaveId,
    disposed_epoch: u64,
    last_sequence: u64,
    last_checkpoint: u64
});
record!(ConsumedTicket {
    issuing_host: u64,
    operation: Reference,
    ticket: u64,
    checkpoint: u64
});
record!(Migration {
    owner: Reference,
    source_schema: Identity,
    target_schema: Identity,
    source_codec: Identity,
    target_codec: Identity,
    migrator: Identity,
    epoch: u64
});
record!(SaveCatalog0 { modules: Vec<ModuleRow>, current_dispatch: Vec<DispatchRow>, owners: Vec<OwnerRow>,
    publications: Vec<PublicationRow>, candidates: Vec<CandidateRow>, batches: Vec<BatchRow>,
    operations: Vec<OperationRow>, leases: Vec<LeaseRow>, callbacks: Vec<CallbackRow>, pins: Vec<PinRow>,
    reservations: Vec<ReservationRow>, workflows: Vec<WorkflowRow>, cleanup: Vec<CleanupRow>,
    input_services: Vec<InputServiceRow>, input_records: Vec<InputRecordRow>, timers: Vec<TimerRow>,
    retention: Vec<RetentionEdge>, counters: Vec<Counter>, tombstones: Vec<Tombstone>,
    retired: Vec<SaveId>, consumed_tickets: Vec<ConsumedTicket>, publication_domains: Vec<PublicationDomain>, migrations: Vec<Migration> });
record!(OwnerSaveV1 {
    format_version: u64,
    profile_version: u64,
    capture_epoch: u64,
    catalog: SaveCatalog0
});

fn check_hex(hex: &Hex, max: usize) -> Result<(), Error> {
    if hex.0.len() % 2 != 0
        || !hex
            .0
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Error::Hex);
    }
    if hex.0.len() / 2 > max {
        return Err(Error::Payload);
    }
    Ok(())
}

struct BoundedWriter {
    bytes: Vec<u8>,
    maximum: usize,
}
impl std::io::Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let end = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .filter(|n| *n <= self.maximum)
            .ok_or_else(|| std::io::Error::other("wire limit"))?;
        self.bytes
            .try_reserve(end - self.bytes.len())
            .map_err(std::io::Error::other)?;
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
/// Encode a declarative graph; does not invoke private codecs or grant authority.
pub fn encode(save: &OwnerSaveV1, limits: SaveLimits) -> Result<Vec<u8>, Error> {
    let limits = limits.checked()?;
    let maximum = limits
        .wire_bytes
        .checked_sub(HEADER_LEN)
        .ok_or(Error::Limits)?;
    let mut raw = BoundedWriter {
        bytes: Vec::new(),
        maximum,
    };
    serde_json::to_writer(&mut raw, save).map_err(|_| Error::Limits)?;
    syntax::check(&raw.bytes, limits, false)?;
    // Only bounded, already typed encoder input enters this temporary sorting map.
    let value: serde_json::Value = serde_json::from_slice(&raw.bytes).map_err(|_| Error::Json)?;
    validate(save, &value, limits)?;
    let mut canonical = BoundedWriter {
        bytes: Vec::new(),
        maximum,
    };
    serde_json::to_writer(&mut canonical, &value).map_err(|_| Error::Limits)?;
    syntax::check(&canonical.bytes, limits, true)?;
    let length = u64::try_from(canonical.bytes.len()).map_err(|_| Error::Length)?;
    let total = HEADER_LEN
        .checked_add(canonical.bytes.len())
        .ok_or(Error::Length)?;
    let mut wire = Vec::new();
    wire.try_reserve_exact(total)
        .map_err(|_| Error::Allocation)?;
    wire.extend_from_slice(MAGIC);
    wire.extend_from_slice(&1u32.to_le_bytes());
    wire.extend_from_slice(&length.to_le_bytes());
    wire.extend_from_slice(&Sha256::digest(&canonical.bytes));
    wire.extend_from_slice(&canonical.bytes);
    Ok(wire)
}
/// Check envelope and bounded canonical syntax before any table/payload allocation.
pub fn decode(wire: &[u8], limits: SaveLimits) -> Result<OwnerSaveV1, Error> {
    let limits = limits.checked()?;
    if wire.len() > limits.wire_bytes {
        return Err(Error::Limits);
    }
    if wire.len() < HEADER_LEN {
        return Err(Error::Truncated);
    }
    if &wire[..8] != MAGIC {
        return Err(Error::Magic);
    }
    if wire[8..12] != 1u32.to_le_bytes() {
        return Err(Error::Version);
    }
    let length = u64::from_le_bytes(wire[12..20].try_into().map_err(|_| Error::Length)?);
    let length = usize::try_from(length).map_err(|_| Error::Length)?;
    let end = HEADER_LEN.checked_add(length).ok_or(Error::Length)?;
    if end != wire.len() {
        return Err(Error::Length);
    }
    let body = wire.get(HEADER_LEN..end).ok_or(Error::Length)?;
    if Sha256::digest(body).as_slice() != &wire[20..52] {
        return Err(Error::Digest);
    }
    syntax::check(body, limits, true)?;
    let save: OwnerSaveV1 = serde_json::from_slice(body).map_err(|_| Error::Schema)?;
    // This map is bounded by the preceding syntax pass, never an unbounded parse.
    let value: serde_json::Value = serde_json::from_slice(body).map_err(|_| Error::Json)?;
    validate(&save, &value, limits)?;
    // Includes required explicit null option fields and freezes typed fidelity.
    if encode(&save, limits)? != wire {
        return Err(Error::Noncanonical);
    }
    Ok(save)
}
fn validate(
    save: &OwnerSaveV1,
    value: &serde_json::Value,
    limits: SaveLimits,
) -> Result<(), Error> {
    if save.format_version != 1 || save.profile_version != 1 {
        return Err(Error::Version);
    }
    let c = &save.catalog;
    let mut ids = BTreeMap::new();
    macro_rules! table {
        ($field:ident, $kind:ident) => {{
            let mut previous = None;
            for row in &c.$field {
                if row.id.kind != IdKind::$kind
                    || row.id.serial == 0
                    || previous.as_ref().is_some_and(|p| p >= &row.id)
                    || ids.insert(row.id.clone(), false).is_some()
                {
                    return Err(Error::Id);
                }
                previous = Some(row.id.clone());
            }
        }};
    }
    table!(modules, Module);
    table!(owners, Owner);
    table!(publications, Publication);
    table!(operations, Operation);
    table!(leases, Lease);
    table!(callbacks, Callback);
    table!(pins, Pin);
    table!(candidates, Candidate);
    table!(batches, Batch);
    table!(cleanup, Cleanup);
    table!(input_services, InputService);
    table!(input_records, InputRecord);
    table!(timers, Timer);
    table!(reservations, Reservation);
    table!(workflows, Workflow);
    let mut previous = None;
    for row in &c.tombstones {
        if row.id.serial == 0
            || row.disposed_epoch > save.capture_epoch
            || previous.as_ref().is_some_and(|p| p >= &row.id)
            || ids.insert(row.id.clone(), true).is_some()
        {
            return Err(Error::Id);
        }
        previous = Some(row.id.clone());
    }
    let mut previous = None;
    for id in &c.retired {
        if id.serial == 0 || previous.as_ref().is_some_and(|p| p >= id) {
            return Err(Error::Id);
        }
        if ids.get(id) == Some(&false) {
            return Err(Error::Id);
        }
        previous = Some(id.clone());
    }
    let mut counters = BTreeMap::new();
    let mut previous = None;
    for counter in &c.counters {
        if previous.is_some_and(|p| p >= counter.kind)
            || counters.insert(counter.kind, counter.high_water).is_some()
        {
            return Err(Error::Counter);
        }
        previous = Some(counter.kind);
    }
    for id in ids.keys().chain(c.retired.iter()) {
        if counters.get(&id.kind).is_none_or(|n| *n < id.serial) {
            return Err(Error::Counter);
        }
    }
    let mut edges = 0usize;
    walk_references(value, "", false, &ids, &mut edges, limits.aggregate_edges)?;
    let mut dispatched = BTreeSet::new();
    for row in &c.current_dispatch {
        if row.owner.mode != ReferenceMode::Live || !dispatched.insert(row.owner.target.clone()) {
            return Err(Error::Reference);
        }
    }
    if c.current_dispatch
        .windows(2)
        .any(|w| w[0].owner.target >= w[1].owner.target)
    {
        return Err(Error::Id);
    }
    if dispatched.len() != c.owners.len() {
        return Err(Error::Reference);
    }
    let operations: BTreeMap<_, _> = c.operations.iter().map(|r| (&r.id, r)).collect();
    let tombstones: BTreeMap<_, _> = c.tombstones.iter().map(|r| (&r.id, r)).collect();
    let records: BTreeMap<_, _> = c.input_records.iter().map(|r| (&r.id, r)).collect();
    let services: BTreeMap<_, _> = c.input_services.iter().map(|r| (&r.id, r)).collect();
    let leases: BTreeMap<_, _> = c.leases.iter().map(|r| (&r.id, r)).collect();
    let candidates: BTreeMap<_, _> = c.candidates.iter().map(|r| (&r.id, r)).collect();
    let mut ordinal_keys = BTreeSet::new();
    for record in &c.input_records {
        if !ordinal_keys.insert((
            &record.service.target,
            &record.operation.target,
            record.ordinal,
        )) {
            return Err(Error::Counter);
        }
    }
    let mut domains = BTreeMap::new();
    let mut previous = None;
    for domain in &c.publication_domains {
        if domain.issuing_host == 0
            || domain.high_water == u64::MAX
            || previous.is_some_and(|p| p >= domain.issuing_host)
            || domains
                .insert(domain.issuing_host, domain.high_water)
                .is_some()
        {
            return Err(Error::Counter);
        }
        previous = Some(domain.issuing_host);
    }
    let issued = |host, ticket| {
        if ticket == 0 || ticket == u64::MAX || domains.get(&host).is_none_or(|high| ticket > *high)
        {
            Err(Error::Counter)
        } else {
            Ok(())
        }
    };
    let checkpoint = |operation: &Reference, checkpoint| {
        let high = if let Some(op) = operations.get(&operation.target) {
            op.checkpoint
        } else {
            tombstones
                .get(&operation.target)
                .ok_or(Error::Reference)?
                .last_checkpoint
        };
        if checkpoint > high {
            Err(Error::Counter)
        } else {
            Ok(())
        }
    };
    let mut consumed = BTreeMap::new();
    let mut previous = None;
    for ticket in &c.consumed_tickets {
        issued(ticket.issuing_host, ticket.ticket)?;
        let key = (ticket.issuing_host, ticket.ticket);
        if previous.is_some_and(|p| p >= key) || consumed.insert(key, ticket).is_some() {
            return Err(Error::Counter);
        }
        previous = Some(key);
        checkpoint(&ticket.operation, ticket.checkpoint)?;
    }
    let mut pending = BTreeSet::new();
    for batch in &c.batches {
        issued(batch.issuing_host, batch.ticket)?;
        let key = (batch.issuing_host, batch.ticket);
        if consumed.contains_key(&key) || !pending.insert(key) {
            return Err(Error::Counter);
        }
        checkpoint(&batch.operation, batch.checkpoint)?;
        if batch.declaration.binding.operation != batch.operation
            || batch.declaration.binding.checkpoint != batch.checkpoint
        {
            return Err(Error::Reference);
        }
    }
    for row in &c.workflows {
        let op = operations
            .get(&row.operation.target)
            .ok_or(Error::Reference)?;
        if row.sequence > op.sequence
            || row.checkpoint > op.checkpoint
            || row.checkpoint > row.sequence
        {
            return Err(Error::Counter);
        }
        if leases
            .get(&row.lease.target)
            .is_none_or(|lease| lease.operation != row.operation)
        {
            return Err(Error::Reference);
        }
        if row.status == Status::Paused && row.ticket.is_none() {
            return Err(Error::Counter);
        }
        if let Some(ticket) = &row.ticket {
            if ticket.sequence == 0
                || ticket.sequence > row.sequence
                || ticket.operation != row.operation
                || ticket.lease != row.lease
                || ticket.pipeline_digest != row.pipeline_digest
                || (row.status == Status::Paused
                    && (ticket.sequence != row.sequence || ticket.phase != row.phase))
                || (row.status == Status::Running && ticket.sequence >= row.sequence)
            {
                return Err(Error::Counter);
            }
        }
    }
    let mut history = BTreeSet::new();
    for row in &c.publications {
        let receipt = &row.receipt;
        issued(receipt.issuing_host, receipt.ticket)?;
        let key = (receipt.issuing_host, receipt.ticket);
        let used = consumed.get(&key).ok_or(Error::Counter)?;
        if !history.insert(key)
            || used.operation != row.operation
            || used.checkpoint != receipt.binding.checkpoint
            || receipt.binding.operation != row.operation
            || row.declaration.binding != receipt.binding
            || row.declaration.caller != receipt.caller
            || row.declaration.caller_artifact != receipt.caller_artifact
            || row.declaration.coordinator != receipt.coordinator
            || row.declaration.policy != receipt.policy
            || receipt.epoch != row.epoch
            || row.epoch > save.capture_epoch
            || row.delivery_cursor
                > u64::try_from(row.accepted_events.len()).map_err(|_| Error::Counter)?
        {
            return Err(Error::Counter);
        }
    }
    // Bindings and receipt counters are outer metadata; private bytes never fill gaps.
    validate_outer_metadata(value, &operations, &tombstones)?;
    let mut ledger_members = BTreeSet::new();
    for row in &c.input_services {
        let mut ordinals = BTreeMap::new();
        for ordinal in &row.ordinals {
            if ordinals
                .insert(&ordinal.operation.target, ordinal.high_water)
                .is_some()
            {
                return Err(Error::Counter);
            }
        }
        let mut ledger = BTreeSet::new();
        for reference in &row.ledger {
            ledger_members.insert((&row.id, &reference.target));
            if !ledger.insert(&reference.target) {
                return Err(Error::Id);
            }
            let record = records.get(&reference.target).ok_or(Error::Reference)?;
            if record.service.target != row.id
                || ordinals
                    .get(&record.operation.target)
                    .is_none_or(|n| *n < record.ordinal)
            {
                return Err(Error::Counter);
            }
        }
    }
    for record in &c.input_records {
        if !services.contains_key(&record.service.target)
            || !ledger_members.contains(&(&record.service.target, &record.id))
        {
            return Err(Error::Reference);
        }
    }
    let mut enrolled = BTreeSet::new();
    for batch in &c.batches {
        for reference in &batch.candidates {
            let candidate = candidates.get(&reference.target).ok_or(Error::Reference)?;
            if candidate.operation.target != batch.operation.target
                || !enrolled.insert(&reference.target)
            {
                return Err(Error::Reference);
            }
        }
    }

    Ok(())
}
fn expected_kind(key: &str) -> Option<IdKind> {
    Some(match key {
        "module" | "modules" | "retained_modules" | "extension_target" | "provider" | "target"
        | "caller" => IdKind::Module,
        "owner" | "owners" | "source" | "required_writes" | "effect_order" => IdKind::Owner,
        "operation" | "origin" => IdKind::Operation,
        "lease" => IdKind::Lease,
        "callback" => IdKind::Callback,
        "reservation" => IdKind::Reservation,
        "batch" => IdKind::Batch,
        "candidates" => IdKind::Candidate,
        "cleanup" => IdKind::Cleanup,
        "code" | "retained_code" => IdKind::Pin,
        "service" => IdKind::InputService,
        "ledger" | "observations" | "accepted_inputs" | "record" => IdKind::InputRecord,
        "accepted_publications" => IdKind::Publication,
        _ => return None,
    })
}
fn walk_references(
    value: &serde_json::Value,
    key: &str,
    historical: bool,
    ids: &BTreeMap<SaveId, bool>,
    edges: &mut usize,
    maximum: usize,
) -> Result<(), Error> {
    match value {
        serde_json::Value::Object(map) => {
            if map.contains_key("target") && map.contains_key("mode") {
                let reference: Reference =
                    serde_json::from_value(value.clone()).map_err(|_| Error::Reference)?;
                *edges = edges.checked_add(1).ok_or(Error::Edges)?;
                if *edges > maximum {
                    return Err(Error::Edges);
                }
                if expected_kind(key).is_some_and(|kind| kind != reference.target.kind) {
                    return Err(Error::Reference);
                }
                if reference.mode == ReferenceMode::Historical
                    && !historical
                    && !matches!(
                        key,
                        "source"
                            | "origin"
                            | "operation"
                            | "accepted_publications"
                            | "accepted_inputs"
                            | "holder"
                            | "retained"
                    )
                {
                    return Err(Error::Reference);
                }
                match ids.get(&reference.target) {
                    Some(false) => {}
                    Some(true) if reference.mode == ReferenceMode::Historical => {}
                    _ => return Err(Error::Reference),
                }
            } else {
                for (k, v) in map {
                    walk_references(
                        v,
                        k,
                        historical || k == "publications" || k == "consumed_tickets",
                        ids,
                        edges,
                        maximum,
                    )?;
                }
            }
        }
        serde_json::Value::Array(array) => {
            for v in array {
                walk_references(v, key, historical, ids, edges, maximum)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_outer_metadata(
    value: &serde_json::Value,
    operations: &BTreeMap<&SaveId, &OperationRow>,
    tombstones: &BTreeMap<&SaveId, &Tombstone>,
) -> Result<(), Error> {
    match value {
        serde_json::Value::Object(map) => {
            if map.contains_key("cleanup_identities") {
                let binding: OperationBinding =
                    serde_json::from_value(value.clone()).map_err(|_| Error::Schema)?;
                let high = if let Some(op) = operations.get(&binding.operation.target) {
                    op.checkpoint
                } else {
                    tombstones
                        .get(&binding.operation.target)
                        .ok_or(Error::Reference)?
                        .last_checkpoint
                };
                if binding.checkpoint > high
                    || binding.cleanup_identities.contains(&0)
                    || binding.cleanup_identities.windows(2).any(|w| w[0] >= w[1])
                {
                    return Err(Error::Counter);
                }
            }
            if map.contains_key("event_count")
                && map.get("serial").and_then(|v| v.as_u64()) == Some(0)
            {
                return Err(Error::Counter);
            }
            for v in map.values() {
                validate_outer_metadata(v, operations, tombstones)?;
            }
        }
        serde_json::Value::Array(array) => {
            for v in array {
                validate_outer_metadata(v, operations, tombstones)?;
            }
        }
        _ => {}
    }
    Ok(())
}
