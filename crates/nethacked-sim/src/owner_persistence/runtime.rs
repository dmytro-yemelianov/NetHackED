//! Current-schema D4 session composition. World exists only in detached native
//! import/export, never in a port, behavior, registry or publication callback.
use super::{partitions::*, wand_ports as wand, NativeImportRegistry, NativePartitions};
use crate::world::{bill_ledger, validate_bill_object, validate_bill_terms, BillError, BillTerms};
use crate::{LoadError, Ruleset, RulesetRef, SimulationWorld};
use nethacked_arena::ItemId;
use nethacked_kernel::{
    self as kernel, persistence as kp, Access, Authority, CallerView, Capability, Endpoint, Kernel,
    Manifest, NativePort, Resource, Scope,
};
use nethacked_owners::{
    self as owners, persistence as op, Accounting, Backend, Dependency, Host, NativeBehavior,
    Observation, OwnerSpec, Port, Proposal, ProposalView, QueryPolicy, Schema,
};
use nethacked_persistence::{runtime as pr, session, SaveLimits};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
    sync::Arc,
};

const PAYLOAD: usize = 4 * 1024 * 1024;
const PROVIDER: &str = "native-owner-bridge-v1";
const CALLER: &str = "native-owner-caller-v1";
const COMMAND: &str = "record_bill_terms";
const NATIVE_ID: &[u8] = b"NetHackED D5 current native owner adapters v1";
const POLICY: [u8; 32] = [75; 32];

#[derive(Debug)]
pub enum OwnerBridgeError {
    Native(LoadError),
    Owner(owners::Error),
    Kernel(kernel::Error),
    Session(pr::Error),
    Admission(session::LoadError),
    Migration(nethacked_persistence::migration::MigrationError<LoadError>),
}
impl From<LoadError> for OwnerBridgeError {
    fn from(e: LoadError) -> Self {
        Self::Native(e)
    }
}
impl From<owners::Error> for OwnerBridgeError {
    fn from(e: owners::Error) -> Self {
        Self::Owner(e)
    }
}
impl From<kernel::Error> for OwnerBridgeError {
    fn from(e: kernel::Error) -> Self {
        Self::Kernel(e)
    }
}
impl From<pr::Error> for OwnerBridgeError {
    fn from(e: pr::Error) -> Self {
        Self::Session(e)
    }
}

/// Host-approved immutable ruleset and current executable/codec configuration.
/// It carries no source owners, World, closures over payloads, or mutable state.
#[derive(Clone)]
pub struct OwnerBridgeConfig {
    pub(super) ruleset: Arc<Ruleset>,
    pub(super) ruleset_ref: RulesetRef,
    wand_ports: bool,
    native_registry: NativeImportRegistry,
    native_binding: nethacked_persistence::migration::MigrationBinding,
}
impl OwnerBridgeConfig {
    pub fn new(ruleset: Arc<Ruleset>, ruleset_ref: RulesetRef) -> Result<Self, OwnerBridgeError> {
        if ruleset_ref.id.is_empty()
            || ruleset_ref.version.is_empty()
            || ruleset_ref.hash.is_empty()
        {
            return Err(owners::Error::Schema.into());
        }
        Ok(Self {
            ruleset,
            ruleset_ref,
            wand_ports: false,
            native_registry: NativeImportRegistry::current(),
            native_binding: super::native_migration_binding(),
        })
    }

    /// Explicitly approve the I1 native collaborator/code/codec/port profile.
    /// Historical registrations retain their original identities and behavior.
    pub fn with_wand_ports(mut self) -> Self {
        self.wand_ports = true;
        self
    }
    fn schema_tag(&self, tag: u8) -> u8 {
        if self.wand_ports {
            tag + 100
        } else {
            tag
        }
    }
    fn codec_identity(&self, name: &str, tag: u8) -> op::CodecIdentity {
        if self.wand_ports {
            identity(&format!("{name}-wand-i1"), tag + 100)
        } else {
            identity(name, tag)
        }
    }
    fn receipt_policy(&self) -> [u8; 32] {
        if self.wand_ports {
            [175; 32]
        } else {
            POLICY
        }
    }
    fn receipt_identity(&self) -> op::ArtifactIdentity {
        if self.wand_ports {
            artifact("native-wand-i1-receipt-accounting", 175)
        } else {
            accounting_identity()
        }
    }
    fn session_binding(&self) -> pr::Binding {
        if self.wand_ports {
            pr::Binding {
                name: "native-owner-wand-i1-session".into(),
                version: 1,
                artifact: [176; 32],
            }
        } else {
            binding()
        }
    }
    /// Explicit host selection; save bytes cannot add registrations or choose
    /// another parser. Missing or changed tuples refuse before decoding.
    pub fn with_native_import(
        mut self,
        registry: NativeImportRegistry,
        binding: nethacked_persistence::migration::MigrationBinding,
    ) -> Self {
        self.native_registry = registry;
        self.native_binding = binding;
        self
    }
}

// Count before allocating JSON values/output. Decoding checks bytes first and
// requires exact canonical re-encoding, including nested fields and defaults.
pub(super) fn encode<T: Serialize>(value: &T, limit: usize) -> owners::Result<Vec<u8>> {
    struct Counter {
        count: usize,
        limit: usize,
    }
    impl std::io::Write for Counter {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.count = self
                .count
                .checked_add(b.len())
                .filter(|n| *n <= self.limit)
                .ok_or_else(|| std::io::Error::other("owner payload limit"))?;
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter {
        count: 0,
        limit: limit.min(PAYLOAD),
    };
    serde_json::to_writer(&mut counter, value).map_err(|_| owners::Error::Limit)?;
    let bytes =
        serde_json::to_vec(&serde_json::to_value(value).map_err(|_| owners::Error::Schema)?)
            .map_err(|_| owners::Error::Schema)?;
    if bytes.len() > counter.limit {
        return Err(owners::Error::Limit);
    }
    Ok(bytes)
}
pub(super) fn decode<T: DeserializeOwned + Serialize>(
    bytes: &[u8],
    limit: usize,
) -> owners::Result<T> {
    if bytes.len() > limit.min(PAYLOAD) {
        return Err(owners::Error::Limit);
    }
    let state: T = serde_json::from_slice(bytes).map_err(|_| owners::Error::Schema)?;
    if encode(&state, limit)? != bytes {
        return Err(owners::Error::Schema);
    }
    Ok(state)
}
fn artifact(name: &str, tag: u8) -> op::ArtifactIdentity {
    op::ArtifactIdentity {
        name: name.into(),
        version: 1,
        artifact: [tag; 32],
    }
}
fn identity(name: &str, tag: u8) -> op::CodecIdentity {
    op::CodecIdentity {
        schema: artifact(&format!("{name}-schema"), tag),
        codec: artifact(&format!("{name}-json"), tag),
        policy: artifact("native-owner-policy", 75),
        schema_digest: [tag; 32],
    }
}
fn port(access: Access, dependencies: Vec<Dependency>) -> Port {
    Port {
        access,
        major: 1,
        dependencies,
    }
}
fn basic_ports(name: &str) -> BTreeMap<String, Port> {
    let _ = name;
    [("query".into(), port(Access::Read, vec![]))].into()
}
fn validate_ports(
    name: &str,
    ports: &BTreeMap<String, Port>,
    policy: &QueryPolicy,
) -> owners::Result<()> {
    if *policy != QueryPolicy::Noncacheable {
        return Err(owners::Error::Schema);
    }
    let expected = basic_ports(name);
    if ports.len() != expected.len()
        || expected.iter().any(|(name, p)| {
            !ports.get(name).is_some_and(|q| {
                q.major == p.major && q.access == p.access && q.dependencies.is_empty()
            })
        })
    {
        return Err(owners::Error::Schema);
    }
    Ok(())
}

// Explicit instantiations are the entire current-schema allowlist. This helper
// never selects a decoder from guest labels or accepts an unknown payload type.
macro_rules! codec {
    ($codec:ident, $state:ident, $name:literal, $tag:literal, $validate:expr) => {
        #[derive(Clone)]
        struct $codec(OwnerBridgeConfig);
        impl Schema<$state> for $codec {
            fn validate(&self, s: &$state) -> owners::Result<()> {
                ($validate)(s, &self.0)
            }
            fn encode(&self, s: &$state) -> owners::Result<Vec<u8>> {
                encode(s, PAYLOAD)
            }
            fn query(
                &self,
                s: &$state,
                name: &str,
                request: &[u8],
                obs: &[Observation],
            ) -> owners::Result<Vec<u8>> {
                if self.0.wand_ports && name.starts_with("wand_") {
                    return wand::query(s, name, request, obs, &self.0);
                }
                if name != "query" || !request.is_empty() || !obs.is_empty() {
                    return Err(owners::Error::Unsupported);
                }
                self.encode(s)
            }
        }
        impl NativeBehavior<$state> for $codec {
            fn prepare(
                &self,
                _: &$state,
                _: &str,
                _: &[u8],
                _: &[Observation],
            ) -> owners::Result<Proposal<$state>> {
                Err(owners::Error::Unsupported)
            }
        }
        impl op::PersistenceCodec<$state> for $codec {
            fn identity(&self) -> op::CodecIdentity {
                self.0.codec_identity($name, $tag)
            }
            fn encode_state(&self, s: &$state, limit: usize) -> owners::Result<Vec<u8>> {
                self.validate(s)?;
                encode(s, limit)
            }
            fn decode_state(
                &self,
                b: &[u8],
                limit: usize,
                _: &op::Remap,
            ) -> owners::Result<$state> {
                let s = decode(b, limit)?;
                self.validate(&s)?;
                Ok(s)
            }
            fn validate_state(&self, s: &$state) -> owners::Result<()> {
                self.validate(s)
            }
            fn references(&self, _: &$state) -> owners::Result<Vec<op::StateReference>> {
                Ok(vec![])
            }
            fn validate_configuration(
                &self,
                _: &$state,
                p: &BTreeMap<String, Port>,
                q: &QueryPolicy,
            ) -> owners::Result<()> {
                if self.0.wand_ports {
                    validate_wand_ports($name, p, q)
                } else {
                    validate_ports($name, p, q)
                }
            }
        }
    };
}
codec!(
    GeographyCodec,
    GeographyState,
    "Geography",
    1,
    |s: &GeographyState, _: &OwnerBridgeConfig| {
        if s.stored_keys.len() != s.stored_levels.len() {
            Err(owners::Error::Invariant)
        } else {
            Ok(())
        }
    }
);
codec!(
    ObjectsCodec,
    ObjectsState,
    "Objects",
    2,
    |_: &ObjectsState, _: &OwnerBridgeConfig| Ok(())
);
codec!(
    ActorsCodec,
    ActorsState,
    "Actors",
    3,
    |_: &ActorsState, _: &OwnerBridgeConfig| Ok(())
);
codec!(
    HeroCodec,
    HeroState,
    "Hero",
    4,
    |_: &HeroState, _: &OwnerBridgeConfig| Ok(())
);
codec!(
    EquipmentCodec,
    EquipmentState,
    "Equipment",
    5,
    |_: &EquipmentState, _: &OwnerBridgeConfig| Ok(())
);
codec!(
    ClockCodec,
    ClockState,
    "Clock",
    7,
    |_: &ClockState, _: &OwnerBridgeConfig| Ok(())
);
codec!(
    RandomCodec,
    RandomState,
    "Random",
    8,
    |_: &RandomState, _: &OwnerBridgeConfig| Ok(())
);
codec!(
    SessionCodec,
    SessionState,
    "Session",
    9,
    |s: &SessionState, c: &OwnerBridgeConfig| {
        if s.ruleset_ref != c.ruleset_ref {
            Err(owners::Error::Schema)
        } else {
            Ok(())
        }
    }
);

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReceiptRequest {
    id: ItemId,
    terms: BillTerms,
}
#[derive(Clone)]
struct BillingCodec(OwnerBridgeConfig);
impl BillingCodec {
    fn admission(
        &self,
        s: &BillingState,
        request: &ReceiptRequest,
        obs: &[Observation],
    ) -> owners::Result<Result<(), BillError>> {
        if obs.len() != 3 {
            return Err(owners::Error::Schema);
        }
        let objects: ObjectsState = decode(obs[0].bytes(), PAYLOAD)?;
        let hero: HeroState = decode(obs[1].bytes(), PAYLOAD)?;
        let session: SessionState = decode(obs[2].bytes(), PAYLOAD)?;
        if session.ruleset_ref != self.0.ruleset_ref {
            return Err(owners::Error::Schema);
        }
        Ok((|| {
            let object = validate_bill_object(
                request.id,
                &objects.items,
                hero.player_id,
                &s.billing,
                &self.0.ruleset,
            )?;
            let (base, existing) = bill_ledger(
                request.id,
                &s.unpaid_items,
                &s.billing,
                s.stored_unpaid_items.iter().flatten(),
                objects
                    .stored_items
                    .iter()
                    .flatten()
                    .any(|(id, _)| *id == request.id),
            )?;
            let base = base.ok_or(BillError::InvalidReceipt)?;
            if existing.is_some() {
                return Err(BillError::ReceiptAlreadyRecorded);
            }
            validate_bill_terms(&request.terms, object.quantity, base)
        })())
    }
}
impl Schema<BillingState> for BillingCodec {
    fn validate(&self, _: &BillingState) -> owners::Result<()> {
        Ok(())
    }
    fn encode(&self, s: &BillingState) -> owners::Result<Vec<u8>> {
        encode(s, PAYLOAD)
    }
    fn query(
        &self,
        s: &BillingState,
        name: &str,
        request: &[u8],
        obs: &[Observation],
    ) -> owners::Result<Vec<u8>> {
        if self.0.wand_ports && name.starts_with("wand_") {
            return wand::query(s, name, request, obs, &self.0);
        }
        if name != "admit" {
            return Err(owners::Error::Unsupported);
        }
        encode(
            &self.admission(s, &decode(request, PAYLOAD)?, obs)?,
            PAYLOAD,
        )
    }
}
impl NativeBehavior<BillingState> for BillingCodec {
    fn prepare(
        &self,
        s: &BillingState,
        name: &str,
        request: &[u8],
        obs: &[Observation],
    ) -> owners::Result<Proposal<BillingState>> {
        if name != COMMAND {
            return Err(owners::Error::Unsupported);
        }
        let request: ReceiptRequest = decode(request, PAYLOAD)?;
        self.admission(s, &request, obs)?
            .map_err(|_| owners::Error::Invariant)?;
        let mut next = s.clone();
        next.billing.live_terms.push((request.id, request.terms));
        Ok(Proposal {
            next,
            events: vec![],
            reply: vec![],
        })
    }
}
impl op::PersistenceCodec<BillingState> for BillingCodec {
    fn identity(&self) -> op::CodecIdentity {
        self.0.codec_identity("Billing", 6)
    }
    fn encode_state(&self, s: &BillingState, limit: usize) -> owners::Result<Vec<u8>> {
        encode(s, limit)
    }
    fn decode_state(&self, b: &[u8], limit: usize, _: &op::Remap) -> owners::Result<BillingState> {
        decode(b, limit)
    }
    fn validate_state(&self, s: &BillingState) -> owners::Result<()> {
        self.validate(s)
    }
    fn references(&self, _: &BillingState) -> owners::Result<Vec<op::StateReference>> {
        Ok(vec![])
    }
    fn validate_configuration(
        &self,
        _: &BillingState,
        p: &BTreeMap<String, Port>,
        q: &QueryPolicy,
    ) -> owners::Result<()> {
        let legacy;
        let p = if self.0.wand_ports {
            validate_wand_ports("Billing", p, q)?;
            legacy = p
                .iter()
                .filter(|(name, _)| !name.starts_with("wand_"))
                .map(|(n, p)| (n.clone(), p.clone()))
                .collect::<BTreeMap<_, _>>();
            &legacy
        } else {
            p
        };
        if *q != QueryPolicy::Noncacheable || p.len() != 2 {
            return Err(owners::Error::Schema);
        }
        for (name, access) in [("admit", Access::Read), (COMMAND, Access::Write)] {
            let p = p.get(name).ok_or(owners::Error::Schema)?;
            if p.access != access
                || p.major != 1
                || p.dependencies.len() != 3
                || p.dependencies
                    .iter()
                    .zip([2, 4, 9])
                    .any(|(d, tag)| d.query != "query" || d.schema != [self.0.schema_tag(tag); 32])
                || p.dependencies
                    .iter()
                    .map(|d| d.owner)
                    .collect::<BTreeSet<_>>()
                    .len()
                    != 3
            {
                return Err(owners::Error::Schema);
            }
        }
        let a = &p["admit"].dependencies;
        let b = &p[COMMAND].dependencies;
        if a != b {
            return Err(owners::Error::Schema);
        }
        Ok(())
    }
}

struct ReceiptAccounting(OwnerBridgeConfig);
impl Accounting for ReceiptAccounting {
    fn validate(&self, p: &[ProposalView], obs: &[Observation]) -> owners::Result<()> {
        if p.len() != 1
            || p[0].receipt.command != COMMAND
            || p[0].receipt.stamp.schema != [self.0.schema_tag(6); 32]
            || !p[0].events.is_empty()
            || !p[0].reply.is_empty()
        {
            return Err(owners::Error::Invariant);
        }
        let mut before: BillingState = decode(&p[0].before, PAYLOAD)?;
        let proposed: BillingState = decode(&p[0].proposed, PAYLOAD)?;
        if proposed.billing.live_terms.len() != before.billing.live_terms.len() + 1 {
            return Err(owners::Error::Invariant);
        }
        let (id, terms) = proposed
            .billing
            .live_terms
            .last()
            .ok_or(owners::Error::Invariant)?
            .clone();
        BillingCodec(self.0.clone())
            .admission(
                &before,
                &ReceiptRequest {
                    id,
                    terms: terms.clone(),
                },
                obs,
            )?
            .map_err(|_| owners::Error::Invariant)?;
        before.billing.live_terms.push((id, terms));
        if encode(&before, PAYLOAD)? != p[0].proposed {
            return Err(owners::Error::Invariant);
        }
        Ok(())
    }
}
fn accounting_identity() -> op::ArtifactIdentity {
    artifact("native-receipt-accounting", 75)
}
fn binding() -> pr::Binding {
    pr::Binding {
        name: "native-owner-session".into(),
        version: 1,
        artifact: [76; 32],
    }
}
fn capability(endpoint: &str) -> Capability {
    Capability {
        provider: PROVIDER.into(),
        endpoint: endpoint.into(),
        major: 1,
    }
}
fn endpoint(access: Access) -> Endpoint {
    Endpoint {
        access,
        major: 1,
        max_request: 8192,
        max_reply: 8192,
    }
}
fn provider_manifest() -> Manifest {
    Manifest {
        id: PROVIDER.into(),
        exports: [
            ("query".into(), endpoint(Access::Read)),
            ("admit".into(), endpoint(Access::Read)),
            (COMMAND.into(), endpoint(Access::Write)),
        ]
        .into(),
        dependencies: vec![],
        extension_points: BTreeSet::new(),
        extension: None,
    }
}
fn caller_manifest() -> Manifest {
    Manifest {
        id: CALLER.into(),
        exports: [
            ("run".into(), endpoint(Access::Write)),
            ("query".into(), endpoint(Access::Read)),
        ]
        .into(),
        dependencies: [
            ("query", Access::Read),
            ("admit", Access::Read),
            (COMMAND, Access::Write),
        ]
        .into_iter()
        .map(|(name, access)| kernel::Dependency {
            capability: capability(name),
            access,
        })
        .collect(),
        extension_points: BTreeSet::new(),
        extension: None,
    }
}
fn caller_bytes() -> kernel::Result<Vec<u8>> {
    // Only forwards the typed public request to the trusted native owner port.
    wat::parse_str(
        r#"(module
      (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (func (export "query") (param i32 i32) (result i64) unreachable)
      (func (export "alloc") (param i32) (result i32) i32.const 0)
      (func (export "run") (param $p i32) (param $n i32) (result i64) (local $r i32)
        i32.const 2 local.get $p local.get $n i32.const 32768 i32.const 8192 call $call local.tee $r
        i32.const 0 i32.lt_s if unreachable end
        i64.const 140737488355328 local.get $r i64.extend_i32_u i64.or))"#,
    )
    .map_err(|_| kernel::Error::InvalidModule)
}
// Both routes contain only handles to actual typed owners, never payloads.
struct SourceRoutes {
    host: Host,
    objects: Rc<owners::Owner<ObjectsState>>,
    hero: Rc<owners::Owner<HeroState>>,
    session: Rc<owners::Owner<SessionState>>,
    billing: Rc<owners::Owner<BillingState>>,
    geography: Rc<owners::Owner<GeographyState>>,
    actors: Rc<owners::Owner<ActorsState>>,
    equipment: Rc<owners::Owner<EquipmentState>>,
    clock: Rc<owners::Owner<ClockState>>,
    random: Rc<owners::Owner<RandomState>>,
}
enum Routes {
    Initial(RefCell<Option<SourceRoutes>>),
    Restored(Rc<session::Handles>),
}
impl Routes {
    fn get(&self) -> kernel::Result<SourceRoutes> {
        match self {
            Self::Initial(slot) => {
                let slot = slot.try_borrow().map_err(|_| kernel::Error::Busy)?;
                let r = slot.as_ref().ok_or(kernel::Error::Authority)?;
                Ok(SourceRoutes {
                    host: r.host.clone(),
                    objects: r.objects.clone(),
                    hero: r.hero.clone(),
                    session: r.session.clone(),
                    billing: r.billing.clone(),
                    geography: r.geography.clone(),
                    actors: r.actors.clone(),
                    equipment: r.equipment.clone(),
                    clock: r.clock.clone(),
                    random: r.random.clone(),
                })
            }
            Self::Restored(h) => Ok(SourceRoutes {
                host: h.host()?,
                objects: h.owner("Objects")?,
                hero: h.owner("Hero")?,
                session: h.owner("Session")?,
                billing: h.owner("Billing")?,
                geography: h.owner("Geography")?,
                actors: h.owner("Actors")?,
                equipment: h.owner("Equipment")?,
                clock: h.owner("Clock")?,
                random: h.owner("Random")?,
            }),
        }
    }
}
fn native_ports(handles: Rc<Routes>, config: OwnerBridgeConfig) -> BTreeMap<String, NativePort> {
    let command: NativePort = Rc::new(move |request, view: &CallerView| {
        let routes = handles.get()?;
        let observations = vec![
            routes
                .objects
                .query("query", &[], &[], view)
                .map_err(native_refusal)?,
            routes
                .hero
                .query("query", &[], &[], view)
                .map_err(native_refusal)?,
            routes
                .session
                .query("query", &[], &[], view)
                .map_err(native_refusal)?,
        ];
        let billing = routes.billing;
        let admission = billing
            .query("admit", request, &observations, view)
            .map_err(native_refusal)?;
        let result: Result<(), BillError> =
            decode(admission.bytes(), PAYLOAD).map_err(native_refusal)?;
        if result.is_ok() {
            let candidate = billing
                .prepare(COMMAND, request, observations.clone(), view)
                .map_err(native_refusal)?;
            let host = routes.host;
            let batch = host
                .batch(
                    vec![candidate.erase()],
                    observations,
                    &[billing.id()],
                    vec![billing.id()],
                    config.receipt_policy(),
                    Rc::new(ReceiptAccounting(config.clone())),
                    profile_capability(&config, COMMAND),
                    view,
                )
                .map_err(native_refusal)?;
            if let Err(e) = batch.commit(view) {
                batch.discard().map_err(native_refusal)?;
                return Err(native_refusal(e));
            }
        }
        encode(&result, PAYLOAD).map_err(native_refusal)
    });
    // These named capabilities are used for owner authorization from the actual
    // command invocation. There is no untyped external state-selection endpoint.
    let refuse: NativePort = Rc::new(|_, _| Err(kernel::Error::NativeRefusal));
    [
        ("query".into(), refuse.clone()),
        ("admit".into(), refuse),
        (COMMAND.into(), command),
    ]
    .into()
}
fn native_refusal(_: owners::Error) -> kernel::Error {
    kernel::Error::NativeRefusal
}

fn bridge_stamps(b: &session::HostBundle) -> owners::Result<Vec<owners::Stamp>> {
    macro_rules! stamp {
        ($state:ident, $key:literal) => {
            b.owner::<$state>($key)
                .ok_or(owners::Error::Schema)?
                .owner
                .stamp()?
        };
    }
    Ok(vec![
        stamp!(GeographyState, "Geography"),
        stamp!(ObjectsState, "Objects"),
        stamp!(ActorsState, "Actors"),
        stamp!(HeroState, "Hero"),
        stamp!(EquipmentState, "Equipment"),
        stamp!(BillingState, "Billing"),
        stamp!(ClockState, "Clock"),
        stamp!(RandomState, "Random"),
        stamp!(SessionState, "Session"),
    ])
}

// I1 deliberately has its own artifact/ABI/schema/factory approvals. Legacy
// constants above continue to describe the exact accepted historical code.
const I1_NATIVE_ID: &[u8] = b"NetHackED M5-I1 typed owner observations v1";
const I1_PROVIDER: &str = "native-owner-wand-i1-provider-v1";
const I1_CALLER: &str = "native-owner-wand-i1-caller-v1";
const I1_CALLER_ID: &[u8] = b"NetHackED M5-I1 native bounded observation caller v1";
const I1_RANDOM: &str = "native-owner-wand-i1-random-v1";
const I1_RANDOM_ID: &[u8] = b"NetHackED M5-I1 native random reservation observations v1";
const WAND_ROUTES: [(&str, &str); 10] = [
    ("Geography", "wand_geography"),
    ("Objects", "wand_objects"),
    ("Actors", "wand_actors"),
    ("Hero", "wand_hero"),
    ("Equipment", "wand_equipment"),
    ("Billing", "wand_billing"),
    ("Billing", "wand_consumed"),
    ("Clock", "wand_clock"),
    ("Random", "wand_random"),
    ("Session", "wand_session"),
];
fn wand_declarations(name: &str) -> Vec<(&'static str, &'static str, u8)> {
    match name {
        "Objects" => vec![
            ("Hero", "wand_hero", 104),
            ("Session", "wand_session", 109),
            ("Geography", "wand_geography", 101),
        ],
        "Actors" => vec![("Geography", "wand_geography", 101)],
        "Equipment" => vec![("Objects", "wand_objects", 102), ("Hero", "wand_hero", 104)],
        "Billing" => vec![
            ("Objects", "wand_objects", 102),
            ("Geography", "wand_geography", 101),
        ],
        "Clock" => vec![("Objects", "wand_objects", 102)],
        _ => vec![],
    }
}
fn add_wand_ports(
    name: &str,
    mut ports: BTreeMap<String, Port>,
    ids: &BTreeMap<&str, owners::OwnerId>,
) -> owners::Result<BTreeMap<String, Port>> {
    for (_, query) in WAND_ROUTES.iter().filter(|(owner, _)| *owner == name) {
        let dependencies = if *query == "wand_consumed" {
            vec![]
        } else {
            wand_declarations(name)
                .into_iter()
                .map(|(key, query, tag)| {
                    Ok(Dependency {
                        owner: *ids.get(key).ok_or(owners::Error::Schema)?,
                        query: query.into(),
                        schema: [tag; 32],
                    })
                })
                .collect::<owners::Result<Vec<_>>>()?
        };
        ports.insert((*query).into(), port(Access::Read, dependencies));
    }
    Ok(ports)
}
fn validate_wand_ports(
    name: &str,
    ports: &BTreeMap<String, Port>,
    policy: &QueryPolicy,
) -> owners::Result<()> {
    if *policy != QueryPolicy::Noncacheable {
        return Err(owners::Error::Schema);
    }
    let expected = WAND_ROUTES
        .iter()
        .filter(|(owner, _)| *owner == name)
        .collect::<Vec<_>>();
    let legacy_count = if name == "Billing" { 2 } else { 1 };
    if ports.len() != expected.len() + legacy_count {
        return Err(owners::Error::Schema);
    }
    for (_, query) in expected {
        let p = ports.get(*query).ok_or(owners::Error::Schema)?;
        let deps = if *query == "wand_consumed" {
            vec![]
        } else {
            wand_declarations(name)
        };
        if p.access != Access::Read
            || p.major != 1
            || p.dependencies.len() != deps.len()
            || p.dependencies
                .iter()
                .zip(&deps)
                .any(|(d, (_, query, tag))| d.query != *query || d.schema != [*tag; 32])
            || p.dependencies
                .iter()
                .map(|d| d.owner)
                .collect::<BTreeSet<_>>()
                .len()
                != deps.len()
        {
            return Err(owners::Error::Schema);
        }
    }
    if name != "Billing" {
        let legacy = ports
            .iter()
            .filter(|(name, _)| !name.starts_with("wand_"))
            .map(|(n, p)| (n.clone(), p.clone()))
            .collect();
        validate_ports(name, &legacy, policy)?;
    }
    Ok(())
}
fn profile_native_id(config: &OwnerBridgeConfig) -> &'static [u8] {
    if config.wand_ports {
        I1_NATIVE_ID
    } else {
        NATIVE_ID
    }
}
fn profile_capability(config: &OwnerBridgeConfig, endpoint: &str) -> Capability {
    Capability {
        provider: if config.wand_ports {
            I1_PROVIDER
        } else {
            PROVIDER
        }
        .into(),
        endpoint: endpoint.into(),
        major: 1,
    }
}
fn profile_provider(config: &OwnerBridgeConfig) -> Manifest {
    if !config.wand_ports {
        return provider_manifest();
    }
    let mut m = provider_manifest();
    m.id = I1_PROVIDER.into();
    for (_, name) in WAND_ROUTES {
        if name != "wand_random" {
            m.exports.insert(
                name.into(),
                Endpoint {
                    max_request: wand::MAX_REQUEST_BYTES as u32,
                    max_reply: wand::MAX_RESPONSE_BYTES as u32,
                    ..endpoint(Access::Read)
                },
            );
        }
    }
    m
}
fn profile_caller(config: &OwnerBridgeConfig) -> Manifest {
    if !config.wand_ports {
        return caller_manifest();
    }
    let mut m = caller_manifest();
    m.id = I1_CALLER.into();
    for dep in &mut m.dependencies {
        dep.capability.provider = I1_PROVIDER.into();
    }
    for (_, name) in WAND_ROUTES {
        m.dependencies.push(kernel::Dependency {
            capability: if name == "wand_random" {
                random_capability()
            } else {
                profile_capability(config, name)
            },
            access: Access::Read,
        });
    }
    m.exports.insert(
        "observe".into(),
        Endpoint {
            max_request: wand::MAX_REQUEST_BYTES as u32,
            max_reply: wand::MAX_RESPONSE_BYTES as u32,
            ..endpoint(Access::Read)
        },
    );
    m
}
fn random_capability() -> Capability {
    Capability {
        provider: I1_RANDOM.into(),
        endpoint: "wand_random".into(),
        major: 1,
    }
}
fn random_manifest() -> Manifest {
    Manifest {
        id: I1_RANDOM.into(),
        exports: [
            ("query".into(), endpoint(Access::Read)),
            (
                "wand_random".into(),
                Endpoint {
                    max_request: wand::MAX_REQUEST_BYTES as u32,
                    max_reply: wand::MAX_RESPONSE_BYTES as u32,
                    ..endpoint(Access::Read)
                },
            ),
        ]
        .into(),
        dependencies: vec![],
        extension_points: BTreeSet::new(),
        extension: None,
    }
}
fn approve_wand_grant(
    caller: &kp::SavedModule,
    slot: usize,
    target: &kp::SavedModule,
    requested: &Authority,
    config: &OwnerBridgeConfig,
) -> kernel::Result<Authority> {
    let manifest = profile_caller(config);
    let is_caller = caller.manifest == manifest
        && manifest.dependencies.get(slot).is_some_and(|dep| {
            target.manifest
                == if dep.capability == random_capability() {
                    random_manifest()
                } else {
                    profile_provider(config)
                }
        });
    if !is_caller {
        return Err(kernel::Error::Authority);
    }
    let Scope::Keys(keys) = &requested.scope else {
        return Err(kernel::Error::Authority);
    };
    let mut expected: BTreeSet<String> = [
        "publication",
        "1.wand_geography",
        "2.query",
        "2.wand_hero",
        "3.query",
        "3.wand_session",
        "4.query",
        "4.wand_objects",
        "5.wand_actors",
        "6.wand_equipment",
        "7.admit",
        "7.record_bill_terms",
        "7.wand_billing",
        "7.wand_consumed",
        "8.wand_clock",
        "9.wand_random",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    if manifest.dependencies[slot].capability == random_capability() {
        expected = ["9.wand_random".to_owned()].into();
    }
    let namespace = &keys.first().ok_or(kernel::Error::Authority)?.owner;
    let access = if slot == 2 {
        Access::Write
    } else {
        Access::Read
    };
    if requested.access != access
        || keys.len() != expected.len()
        || keys.iter().map(|r| r.key.clone()).collect::<BTreeSet<_>>() != expected
        || namespace
            .strip_prefix("owners-")
            .is_none_or(|s| s.parse::<u64>().is_err())
        || keys.iter().any(|r| r.owner != *namespace)
    {
        return Err(kernel::Error::Authority);
    }
    Ok(requested.clone())
}
#[allow(clippy::too_many_arguments)]
fn wand_resources(
    name: &str,
    geography: &Rc<owners::Owner<GeographyState>>,
    objects: &Rc<owners::Owner<ObjectsState>>,
    actors: &Rc<owners::Owner<ActorsState>>,
    hero: &Rc<owners::Owner<HeroState>>,
    equipment: &Rc<owners::Owner<EquipmentState>>,
    billing: &Rc<owners::Owner<BillingState>>,
    clock: &Rc<owners::Owner<ClockState>>,
    random: &Rc<owners::Owner<RandomState>>,
    session: &Rc<owners::Owner<SessionState>>,
) -> owners::Result<Vec<Resource>> {
    WAND_ROUTES
        .iter()
        .filter(|(owner, _)| *owner == name)
        .map(|(_, q)| match name {
            "Geography" => geography.resource(q),
            "Objects" => objects.resource(q),
            "Actors" => actors.resource(q),
            "Hero" => hero.resource(q),
            "Equipment" => equipment.resource(q),
            "Billing" => billing.resource(q),
            "Clock" => clock.resource(q),
            "Random" => random.resource(q),
            "Session" => session.resource(q),
            _ => Err(owners::Error::Schema),
        })
        .collect()
}
impl SourceRoutes {
    fn stamp(&self, name: &str) -> owners::Result<owners::Stamp> {
        match name {
            "Geography" => self.geography.stamp(),
            "Objects" => self.objects.stamp(),
            "Actors" => self.actors.stamp(),
            "Hero" => self.hero.stamp(),
            "Equipment" => self.equipment.stamp(),
            "Billing" => self.billing.stamp(),
            "Clock" => self.clock.stamp(),
            "Random" => self.random.stamp(),
            "Session" => self.session.stamp(),
            _ => Err(owners::Error::Schema),
        }
    }
    fn observe(
        &self,
        q: &wand::WandQuery,
        view: &CallerView,
    ) -> owners::Result<(Observation, Vec<Observation>)> {
        use wand::{ObjectRequest, WandQuery as Q};
        q.validate()?;
        let geo = |residency| {
            Q::Geography(wand::GeographyRequest {
                residency,
                cells: vec![],
                trajectory: None,
            })
        };
        let object = |target| Q::Objects(ObjectRequest::Identity(target));
        let deps = match q {
            Q::Objects(r) => {
                let residency = match r {
                    ObjectRequest::Selection(_) => wand::Residency::Active,
                    ObjectRequest::Identity(t) => t.residency,
                };
                vec![Q::Hero, Q::Session, geo(residency)]
            }
            Q::Equipment(t) => vec![object(*t), Q::Hero],
            Q::Clock(t) => vec![object(*t)],
            Q::Billing { target, .. } => vec![object(*target), geo(target.residency)],
            Q::Actors(r) => vec![geo(r.residency)],
            _ => vec![],
        };
        let mut immediate = Vec::with_capacity(deps.len());
        let mut provenance = Vec::new();
        for dep in deps {
            let (observation, nested) = self.observe(&dep, view)?;
            provenance.extend(nested);
            provenance.push(observation.clone());
            immediate.push(observation);
        }
        let request = encode(q, wand::MAX_REQUEST_BYTES)?;
        let name = q.route().1;
        let observation = match q {
            Q::Objects(_) => self.objects.query(name, &request, &immediate, view),
            Q::Equipment(_) => self.equipment.query(name, &request, &immediate, view),
            Q::Billing { .. } | Q::ConsumedBilling { .. } => {
                self.billing.query(name, &request, &immediate, view)
            }
            Q::Clock(_) => self.clock.query(name, &request, &immediate, view),
            Q::Random { .. } => self.random.query(name, &request, &immediate, view),
            Q::Geography(_) => self.geography.query(name, &request, &immediate, view),
            Q::Actors(_) => self.actors.query(name, &request, &immediate, view),
            Q::Hero => self.hero.query(name, &request, &immediate, view),
            Q::Session => self.session.query(name, &request, &immediate, view),
        }?;
        Ok((observation, provenance))
    }
    fn read(&self, request: &[u8], view: &CallerView) -> kernel::Result<Vec<u8>> {
        let read: wand::WandRead =
            decode(request, wand::MAX_REQUEST_BYTES).map_err(native_refusal)?;
        read.query.validate().map_err(native_refusal)?;
        let stamp = self.stamp(read.query.route().0).map_err(native_refusal)?;
        if read
            .expected_owner
            .as_ref()
            .is_some_and(|e| *e != wand::OwnerWitness::from(&stamp))
        {
            return Err(kernel::Error::NativeRefusal);
        }
        let (observation, deps) = self.observe(&read.query, view).map_err(native_refusal)?;
        let facts =
            decode(observation.bytes(), wand::MAX_RESPONSE_BYTES).map_err(native_refusal)?;
        let wire = wand::WireObservation {
            witness: wand::OwnerWitness::from(observation.stamp()),
            dependencies: deps
                .iter()
                .map(|d| wand::OwnerWitness::from(d.stamp()))
                .collect(),
            facts,
        };
        encode(&wire, wand::MAX_RESPONSE_BYTES).map_err(native_refusal)
    }
}
fn profile_native_ports(
    handles: Rc<Routes>,
    config: OwnerBridgeConfig,
) -> BTreeMap<String, NativePort> {
    let mut ports = native_ports(handles.clone(), config.clone());
    if !config.wand_ports {
        return ports;
    }
    for (_, name) in WAND_ROUTES {
        if name != "wand_random" {
            let routes = handles.clone();
            ports.insert(
                name.into(),
                Rc::new(move |request, view| read_route(&routes, name, request, view)),
            );
        }
    }
    ports
}
fn read_route(
    routes: &Routes,
    endpoint: &str,
    request: &[u8],
    view: &CallerView,
) -> kernel::Result<Vec<u8>> {
    let read: wand::WandRead = decode(request, wand::MAX_REQUEST_BYTES).map_err(native_refusal)?;
    if read.query.route().1 != endpoint {
        return Err(kernel::Error::NativeRefusal);
    }
    routes.get()?.read(request, view)
}
fn random_native_ports(routes: Rc<Routes>) -> BTreeMap<String, NativePort> {
    let query: NativePort = Rc::new(|_, _| Err(kernel::Error::NativeRefusal));
    let random: NativePort =
        Rc::new(move |request, view| read_route(&routes, "wand_random", request, view));
    [("query".into(), query), ("wand_random".into(), random)].into()
}
fn caller_native_ports() -> BTreeMap<String, NativePort> {
    let query: NativePort = Rc::new(|_, _| Err(kernel::Error::NativeRefusal));
    let run: NativePort = Rc::new(|request, view| view.call_dependency(2, request));
    let observe: NativePort = Rc::new(|request, view| {
        let read: wand::WandRead =
            decode(request, wand::MAX_REQUEST_BYTES).map_err(native_refusal)?;
        read.query.validate().map_err(native_refusal)?;
        let slot = WAND_ROUTES
            .iter()
            .position(|route| *route == read.query.route())
            .ok_or(kernel::Error::MissingDependency)?
            + 3;
        view.call_dependency(slot, request)
    });
    [
        ("query".into(), query),
        ("run".into(), run),
        ("observe".into(), observe),
    ]
    .into()
}

struct KernelFactory(OwnerBridgeConfig);
#[cfg(test)]
thread_local! {
    static FACTORY_BUILDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
struct Resolver {
    handles: Rc<session::Handles>,
    config: OwnerBridgeConfig,
}
impl session::KernelFactory for KernelFactory {
    fn resolver(
        &self,
        handles: Rc<session::Handles>,
        ports: BTreeMap<(String, String), NativePort>,
    ) -> Result<Rc<dyn kp::RetainedAuthority>, pr::Error> {
        #[cfg(test)]
        FACTORY_BUILDS.with(|count| count.set(count.get() + 1));
        if !ports.is_empty() {
            return Err(pr::Error::FactoryRefused);
        }
        Ok(Rc::new(Resolver {
            handles,
            config: self.0.clone(),
        }))
    }
}
impl kp::RegistryAuthority for Resolver {
    fn resolve(&self, saved: &kp::SavedModule) -> kernel::Result<kp::ArtifactMaterial> {
        // Both manifests and executable identities are host constants. Registry
        // reconstruction verifies their artifact digests independently.
        if saved.manifest == profile_provider(&self.config)
            && saved.executable == kp::Executable::NativeV1
        {
            Ok(kp::ArtifactMaterial::Native {
                manifest: profile_provider(&self.config),
                identity: profile_native_id(&self.config).to_vec(),
                ports: profile_native_ports(
                    Rc::new(Routes::Restored(self.handles.clone())),
                    self.config.clone(),
                ),
            })
        } else if self.config.wand_ports
            && saved.manifest == random_manifest()
            && saved.executable == kp::Executable::NativeV1
        {
            Ok(kp::ArtifactMaterial::Native {
                manifest: random_manifest(),
                identity: I1_RANDOM_ID.to_vec(),
                ports: random_native_ports(Rc::new(Routes::Restored(self.handles.clone()))),
            })
        } else if self.config.wand_ports
            && saved.manifest == profile_caller(&self.config)
            && saved.executable == kp::Executable::NativeV1
        {
            Ok(kp::ArtifactMaterial::Native {
                manifest: profile_caller(&self.config),
                identity: I1_CALLER_ID.to_vec(),
                ports: caller_native_ports(),
            })
        } else if !self.config.wand_ports
            && saved.manifest == caller_manifest()
            && saved.executable == kp::Executable::WasmAbiV1
        {
            Ok(kp::ArtifactMaterial::Guest {
                manifest: caller_manifest(),
                bytes: caller_bytes()?,
            })
        } else {
            Err(kernel::Error::Authority)
        }
    }
    fn bind_resource(&self, _: &Resource) -> kernel::Result<Resource> {
        Err(kernel::Error::Authority)
    }
    fn approve_grant(
        &self,
        caller: &kp::SavedModule,
        slot: usize,
        target: &kp::SavedModule,
        requested: &Authority,
    ) -> kernel::Result<Authority> {
        if self.config.wand_ports {
            return approve_wand_grant(caller, slot, target, requested, &self.config);
        }
        if caller.manifest != caller_manifest()
            || target.manifest != provider_manifest()
            || slot > 2
        {
            return Err(kernel::Error::Authority);
        }
        let expected_access = if slot == 2 {
            Access::Write
        } else {
            Access::Read
        };
        let Scope::Keys(keys) = &requested.scope else {
            return Err(kernel::Error::Authority);
        };
        let expected: BTreeSet<String> = [
            "2.query",
            "4.query",
            "6.query",
            "7.admit",
            "7.record_bill_terms",
            "publication",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        let namespace = keys.first().ok_or(kernel::Error::Authority)?.owner.clone();
        if requested.access != expected_access
            || keys.len() != expected.len()
            || keys.iter().map(|r| r.key.clone()).collect::<BTreeSet<_>>() != expected
            || namespace
                .strip_prefix("owners-")
                .is_none_or(|s| s.parse::<u64>().is_err())
            || keys.iter().any(|r| r.owner != namespace)
        {
            return Err(kernel::Error::Authority);
        }
        Ok(requested.clone())
    }
}
impl kp::RetainedAuthority for Resolver {
    fn approve_cleanup(&self, id: u64) -> kernel::Result<()> {
        // This profile supports D4's remapped owner-host cleanup root. No live
        // operation context, callback, service or retained work is admitted.
        if id == 0 {
            Err(kernel::Error::Authority)
        } else {
            Ok(())
        }
    }
    fn approve_operation_context(&self, _: &kp::SavedOperationContext) -> kernel::Result<()> {
        Err(kernel::Error::Authority)
    }
}
fn limits() -> owners::Limits {
    owners::Limits {
        payload_bytes: PAYLOAD,
        observation_bytes: 3 * PAYLOAD,
        ..Default::default()
    }
}
fn publication_limits() -> owners::PublicationLimits {
    owners::PublicationLimits {
        retained_bytes: 16 * PAYLOAD,
        ..Default::default()
    }
}
fn restore_authority(
    config: &OwnerBridgeConfig,
) -> Result<session::RestoreAuthority, OwnerBridgeError> {
    let mut factories: BTreeMap<String, Rc<dyn session::OwnerFactory>> = BTreeMap::new();
    macro_rules! factory {
        ($codec:ident, $state:ident, $name:literal, $tag:literal) => {{
            let c = config.clone();
            let mut catalog = op::RestoreCatalog::default();
            catalog.allow(
                config.codec_identity($name, $tag),
                QueryPolicy::Noncacheable,
                move |_| Ok(Backend::Native(Rc::new($codec(c.clone())))),
            )?;
            factories.insert(
                $name.into(),
                Rc::new(session::TypedFactory::<$state> {
                    codec: Rc::new($codec(config.clone())),
                    catalog,
                }),
            );
        }};
    }
    factory!(GeographyCodec, GeographyState, "Geography", 1);
    factory!(ObjectsCodec, ObjectsState, "Objects", 2);
    factory!(ActorsCodec, ActorsState, "Actors", 3);
    factory!(HeroCodec, HeroState, "Hero", 4);
    factory!(EquipmentCodec, EquipmentState, "Equipment", 5);
    factory!(BillingCodec, BillingState, "Billing", 6);
    factory!(ClockCodec, ClockState, "Clock", 7);
    factory!(RandomCodec, RandomState, "Random", 8);
    factory!(SessionCodec, SessionState, "Session", 9);
    let mut accounting = op::AccountingRestoreCatalog::default();
    let c = config.clone();
    accounting.allow(
        config.receipt_identity(),
        config.receipt_policy(),
        move || Ok(Rc::new(ReceiptAccounting(c.clone()))),
    )?;
    Ok(session::RestoreAuthority {
        bindings: [config.session_binding()].into(),
        kernel: Rc::new(KernelFactory(config.clone())),
        owners: factories,
        accounting: Rc::new(accounting),
        workflows: None,
        inputs: None,
        timers: None,
        adapters: None,
        resources: None,
        owner_limits: limits(),
        publication_limits: publication_limits(),
        limits: SaveLimits::default(),
    })
}
/// Application profile validation runs before any executable factory.
struct ProfileFactory {
    inner: session::SessionFactory,
    config: OwnerBridgeConfig,
}
impl pr::TrustedFactory<session::HostBundle> for ProfileFactory {
    fn build(
        &self,
        context: &mut pr::BuildContext<'_, session::HostBundle>,
        bytes: &[u8],
    ) -> Result<session::HostBundle, pr::Error> {
        let (rows, checkpoint) = self.inner.inspect_owner_states(bytes)?;
        self.admit(&rows, &checkpoint)
            .map_err(|_| pr::Error::FactoryRefused)?;
        if self.config.wand_ports {
            // The session parser above has already checked framing, canonical
            // syntax and aggregate bounds. Inspect only its kernel section for
            // the independent public caller; artifact approval remains with
            // Resolver. This is save admission, never a production owner query.
            #[derive(Deserialize)]
            struct Sections<'a> {
                #[serde(borrow)]
                sections: Vec<Section<'a>>,
            }
            #[derive(Deserialize)]
            struct Section<'a> {
                name: &'a str,
                local_payload: &'a str,
            }
            let wire: Sections<'_> =
                serde_json::from_slice(&bytes[nethacked_persistence::HEADER_LEN..])
                    .map_err(|_| pr::Error::FactoryRefused)?;
            let section = wire
                .sections
                .iter()
                .find(|s| s.name == "kernel")
                .ok_or(pr::Error::FactoryRefused)?;
            let payload = nethacked_persistence::Hex(section.local_payload.to_owned())
                .bytes(SaveLimits::default())
                .map_err(|_| pr::Error::FactoryRefused)?;
            let kernel: kp::KernelSnapshot =
                serde_json::from_slice(&payload).map_err(|_| pr::Error::FactoryRefused)?;
            if !kernel.registry.modules.iter().any(|m| {
                m.manifest == profile_caller(&self.config)
                    && m.executable == kp::Executable::NativeV1
            }) {
                return Err(pr::Error::FactoryRefused);
            }
        }
        self.inner.build(context, bytes)
    }
}
impl ProfileFactory {
    fn admit(
        &self,
        rows: &[session::OwnerRow],
        checkpoint: &op::ExecutionCheckpoint,
    ) -> owners::Result<()> {
        if rows.len() != 9 || checkpoint.owners().len() != 9 {
            return Err(owners::Error::Schema);
        }
        let payloads: BTreeMap<_, _> = checkpoint
            .state_payloads()
            .map(|(id, codec, bytes)| (id, (codec, bytes)))
            .collect();
        if payloads.len() != 9 {
            return Err(owners::Error::Schema);
        }
        macro_rules! state {
            ($state:ident, $name:literal, $tag:literal) => {{
                let row = rows
                    .iter()
                    .find(|row| row.key == $name)
                    .ok_or(owners::Error::Schema)?;
                let (codec, bytes) = payloads.get(&row.owner).ok_or(owners::Error::Schema)?;
                if **codec != self.config.codec_identity($name, $tag) {
                    return Err(owners::Error::Schema);
                }
                decode::<$state>(bytes, PAYLOAD)?
            }};
        }
        let parts = NativePartitions {
            geography: state!(GeographyState, "Geography", 1),
            objects: state!(ObjectsState, "Objects", 2),
            actors: state!(ActorsState, "Actors", 3),
            hero: state!(HeroState, "Hero", 4),
            equipment: state!(EquipmentState, "Equipment", 5),
            billing: state!(BillingState, "Billing", 6),
            clock: state!(ClockState, "Clock", 7),
            random: state!(RandomState, "Random", 8),
            session: state!(SessionState, "Session", 9),
        };
        if parts.session.ruleset_ref != self.config.ruleset_ref {
            return Err(owners::Error::Schema);
        }
        parts.admit_references(&self.config.ruleset)?;
        Ok(())
    }
}

fn restore_configuration(
    config: &OwnerBridgeConfig,
) -> Result<pr::ApprovedConfiguration<session::HostBundle>, OwnerBridgeError> {
    let mut profiles = vec![config.clone()];
    if config.wand_ports {
        let mut legacy = config.clone();
        legacy.wand_ports = false;
        profiles.push(legacy);
    }
    let mut entries: Vec<(pr::Binding, Rc<dyn pr::TrustedFactory<session::HostBundle>>)> = vec![];
    for profile in profiles {
        entries.push((
            profile.session_binding(),
            Rc::new(ProfileFactory {
                inner: session::SessionFactory::new(restore_authority(&profile)?),
                config: profile,
            }),
        ));
    }
    Ok(pr::ApprovedConfiguration::new(
        BTreeSet::new(),
        entries.iter().map(|(binding, _)| binding.clone()).collect(),
        entries,
    )?)
}

/// Nine independently allocated typed owners in the accepted serial session.
/// The facade holds only session authority and approved immutable host config.
pub struct OwnerBridge {
    session: session::Session,
    key: pr::AuthorityKey,
    credential: RefCell<pr::RuntimeCredential>,
    config: OwnerBridgeConfig,
}
fn bridge_caller(
    b: &session::HostBundle,
    config: &OwnerBridgeConfig,
) -> Result<kernel::ModuleHandle, OwnerBridgeError> {
    let random = b
        .owner::<RandomState>("Random")
        .ok_or(owners::Error::Schema)?
        .owner
        .stamp()?;
    let k = b.kernel()?;
    // Legacy restoration through an opt-in facade retains the historical caller.
    if k.manifest(random.module.handle)? == &caller_manifest() {
        return Ok(random.module.handle);
    }
    if !config.wand_ports || k.manifest(random.module.handle)? != &random_manifest() {
        return Err(kernel::Error::Authority.into());
    }
    Ok(k.registered_module(&profile_caller(config))?)
}
impl OwnerBridge {
    pub fn import_native(
        json: &str,
        byte_limit: usize,
        config: OwnerBridgeConfig,
    ) -> Result<Self, OwnerBridgeError> {
        let parts = config
            .native_registry
            .prepare(
                config.native_binding,
                json,
                byte_limit,
                config.ruleset.clone(),
                &config.ruleset_ref,
            )
            .map_err(OwnerBridgeError::Migration)?
            .partition()?;
        Self::install(parts, config)
    }
    fn install(
        parts: NativePartitions,
        config: OwnerBridgeConfig,
    ) -> Result<Self, OwnerBridgeError> {
        let NativePartitions {
            geography,
            objects,
            actors,
            hero,
            equipment,
            billing,
            clock,
            random,
            session: session_state,
        } = parts;
        let host = Host::new(limits())?;
        host.configure_publication(publication_limits())?;
        let mut kernel = Kernel::new(kernel::Limits::default())?;
        let handles = Rc::new(Routes::Initial(RefCell::new(None)));
        let provider = kernel.register_native(
            profile_provider(&config),
            profile_native_id(&config),
            profile_native_ports(handles.clone(), config.clone()),
        )?;
        let random_module = if config.wand_ports {
            Some(kernel.register_native(
                random_manifest(),
                I1_RANDOM_ID,
                random_native_ports(handles.clone()),
            )?)
        } else {
            None
        };
        let caller = if config.wand_ports {
            kernel.register_native(profile_caller(&config), I1_CALLER_ID, caller_native_ports())?
        } else {
            kernel.register_guest(caller_manifest(), &caller_bytes()?)?
        };
        let mut allocations: Vec<Rc<dyn session::OwnerAllocation>> = vec![];
        let mut wand_deps = BTreeMap::new();
        macro_rules! enroll {
            ($codec:ident, $name:literal, $tag:literal, $state:expr, $module:expr, $ports:expr) => {{
                let codec = Rc::new($codec(config.clone()));
                let behavior: Rc<dyn NativeBehavior<_>> = codec.clone();
                let owner = host.enroll(
                    &kernel,
                    OwnerSpec {
                        module: $module,
                        schema: [config.schema_tag($tag); 32],
                        ports: if config.wand_ports {
                            add_wand_ports($name, $ports, &wand_deps)?
                        } else {
                            $ports
                        },
                    },
                    $state,
                    Backend::Native(behavior.clone()),
                )?;
                let mut catalog = op::CaptureCatalog::default();
                catalog.register(
                    config.codec_identity($name, $tag),
                    QueryPolicy::Noncacheable,
                    Backend::Native(behavior),
                )?;
                allocations.push(Rc::new(session::TypedOwner {
                    key: $name.into(),
                    owner: owner.clone(),
                    codec,
                    catalog,
                    retained: vec![],
                    callbacks: vec![],
                    target_port: None,
                    candidates: vec![],
                    batch_candidates: vec![],
                }));
                owner
            }};
        }
        let geography_owner = enroll!(
            GeographyCodec,
            "Geography",
            1,
            geography,
            provider,
            basic_ports("Geography")
        );
        // Objects' declared dependencies are enrolled below, so in I1 order
        // Hero and Session precede Objects. Legacy order stays byte-compatible.
        let early_hero = if config.wand_ports {
            Some(enroll!(
                HeroCodec,
                "Hero",
                4,
                hero.clone(),
                provider,
                basic_ports("Hero")
            ))
        } else {
            None
        };
        if let Some(h) = &early_hero {
            wand_deps.insert("Hero", h.id());
        }
        wand_deps.insert("Geography", geography_owner.id());
        let early_session = if config.wand_ports {
            Some(enroll!(
                SessionCodec,
                "Session",
                9,
                session_state.clone(),
                provider,
                basic_ports("Session")
            ))
        } else {
            None
        };
        if let Some(s) = &early_session {
            wand_deps.insert("Session", s.id());
        }
        let objects = enroll!(
            ObjectsCodec,
            "Objects",
            2,
            objects,
            provider,
            basic_ports("Objects")
        );
        wand_deps.insert("Objects", objects.id());
        let actors_owner = enroll!(
            ActorsCodec,
            "Actors",
            3,
            actors,
            provider,
            basic_ports("Actors")
        );
        let hero = match early_hero {
            Some(h) => h,
            None => enroll!(HeroCodec, "Hero", 4, hero, provider, basic_ports("Hero")),
        };
        let equipment_owner = enroll!(
            EquipmentCodec,
            "Equipment",
            5,
            equipment,
            provider,
            basic_ports("Equipment")
        );
        // Readers precede Billing in the host enrollment order.
        let session_owner = match early_session {
            Some(s) => s,
            None => enroll!(
                SessionCodec,
                "Session",
                9,
                session_state,
                provider,
                basic_ports("Session")
            ),
        };
        let deps = vec![
            Dependency {
                owner: objects.id(),
                query: "query".into(),
                schema: [config.schema_tag(2); 32],
            },
            Dependency {
                owner: hero.id(),
                query: "query".into(),
                schema: [config.schema_tag(4); 32],
            },
            Dependency {
                owner: session_owner.id(),
                query: "query".into(),
                schema: [config.schema_tag(9); 32],
            },
        ];
        let billing = enroll!(
            BillingCodec,
            "Billing",
            6,
            billing,
            provider,
            [
                ("admit".into(), port(Access::Read, deps.clone())),
                (COMMAND.into(), port(Access::Write, deps))
            ]
            .into()
        );
        let clock_owner = enroll!(
            ClockCodec,
            "Clock",
            7,
            clock,
            provider,
            basic_ports("Clock")
        );
        // I1 Random has an independently approved native read provider. The
        // historical idle Random owner continues to pin the D5 Wasm caller.
        let random_owner = enroll!(
            RandomCodec,
            "Random",
            8,
            random,
            random_module.unwrap_or(caller),
            basic_ports("Random")
        );
        let keys = BTreeSet::from([
            host.publication_resource(),
            billing.resource("admit")?,
            billing.resource(COMMAND)?,
            objects.resource("query")?,
            hero.resource("query")?,
            session_owner.resource("query")?,
        ]);
        let keys = if config.wand_ports {
            let mut keys = keys;
            for (name, _) in WAND_ROUTES {
                for resource in wand_resources(
                    name,
                    &geography_owner,
                    &objects,
                    &actors_owner,
                    &hero,
                    &equipment_owner,
                    &billing,
                    &clock_owner,
                    &random_owner,
                    &session_owner,
                )? {
                    keys.insert(resource);
                }
            }
            keys
        } else {
            keys
        };
        for slot in 0..profile_caller(&config).dependencies.len() {
            let random_slot = config.wand_ports
                && profile_caller(&config).dependencies[slot].capability == random_capability();
            kernel.issue_grant(
                caller,
                slot,
                Authority {
                    access: if slot == 2 {
                        Access::Write
                    } else {
                        Access::Read
                    },
                    scope: Scope::Keys(if random_slot {
                        [random_owner.resource("wand_random")?].into()
                    } else {
                        keys.clone()
                    }),
                },
            )?;
        }
        if let Routes::Initial(slot) = handles.as_ref() {
            slot.replace(Some(SourceRoutes {
                host: host.clone(),
                objects,
                hero,
                session: session_owner,
                billing,
                geography: geography_owner,
                actors: actors_owner,
                equipment: equipment_owner,
                clock: clock_owner,
                random: random_owner,
            }));
        }
        let mut accounting = op::AccountingCaptureCatalog::default();
        accounting.register(
            config.receipt_identity(),
            config.receipt_policy(),
            Rc::new(ReceiptAccounting(config.clone())),
        )?;
        let configuration = restore_configuration(&config)?;
        let (session, key, credential) = session::Session::new(
            kernel,
            host,
            allocations,
            vec![],
            accounting,
            session::Services::default(),
            configuration,
        )?;
        Ok(Self {
            session,
            key,
            credential: RefCell::new(credential),
            config,
        })
    }
    pub fn capture(&self) -> Result<Vec<u8>, OwnerBridgeError> {
        Ok(self.session.capture(&self.key, SaveLimits::default())?)
    }
    pub fn reconstruct(bytes: &[u8], config: OwnerBridgeConfig) -> Result<Self, OwnerBridgeError> {
        let (session, key, _) = session::Session::new(
            Kernel::new(kernel::Limits::default())?,
            Host::new(limits())?,
            vec![],
            vec![],
            op::AccountingCaptureCatalog::default(),
            session::Services::default(),
            restore_configuration(&config)?,
        )?;
        let credential = session.issue_credential(&key)?;
        let bridge = Self {
            session,
            key,
            credential: RefCell::new(credential),
            config,
        };
        bridge.restore(bytes)?;
        Ok(bridge)
    }
    /// Atomically replace a nonempty facade after complete profile admission.
    /// Failure leaves the current session and its credential usable.
    pub fn restore(&self, bytes: &[u8]) -> Result<(), OwnerBridgeError> {
        let prepared = self.session.prepare_restore_diagnosed(
            &self.key,
            &self.config.session_binding(),
            bytes,
        );
        let mut stage = match prepared {
            Ok(stage) => stage,
            Err(original) if self.config.wand_ports => self
                .session
                .prepare_restore_diagnosed(&self.key, &binding(), bytes)
                .map_err(|_| OwnerBridgeError::Admission(original))?,
            Err(error) => return Err(OwnerBridgeError::Admission(error)),
        };
        self.session.activate(&mut stage)?;
        *self.credential.borrow_mut() = self.session.issue_credential(&self.key)?;
        Ok(())
    }

    /// Validate and migrate a native save on hidden detached state, then
    /// atomically replace this facade through the same owner admission path.
    /// Import failures leave its current owners and credentials usable.
    pub fn restore_native(&self, json: &str, byte_limit: usize) -> Result<(), OwnerBridgeError> {
        let source = Self::import_native(json, byte_limit, self.config.clone())?;
        let bytes = source.capture()?;
        drop(source);
        self.restore(&bytes)
    }
    fn partitions(&self) -> Result<NativePartitions, OwnerBridgeError> {
        self.session
            .with_runtime(&self.credential.borrow(), |b| {
                macro_rules! state {
                    ($state:ident, $name:literal) => {
                        decode::<$state>(
                            &b.owner::<$state>($name)
                                .ok_or(owners::Error::Schema)?
                                .owner
                                .inspect()?,
                            PAYLOAD,
                        )?
                    };
                }
                Ok::<_, owners::Error>(NativePartitions {
                    geography: state!(GeographyState, "Geography"),
                    objects: state!(ObjectsState, "Objects"),
                    actors: state!(ActorsState, "Actors"),
                    hero: state!(HeroState, "Hero"),
                    equipment: state!(EquipmentState, "Equipment"),
                    billing: state!(BillingState, "Billing"),
                    clock: state!(ClockState, "Clock"),
                    random: state!(RandomState, "Random"),
                    session: state!(SessionState, "Session"),
                })
            })?
            .map_err(Into::into)
    }
    /// Detached export, without timer dispatch, migration, or gameplay commands.
    pub fn export_native(&self) -> Result<SimulationWorld, OwnerBridgeError> {
        let parts = self.partitions()?;
        parts.admit_references(&self.config.ruleset)?;
        let world = parts.into_native(self.config.ruleset.clone(), &self.config.ruleset_ref)?;
        world
            .validate_billing_snapshot()
            .map_err(|_| owners::Error::Invariant)?;
        world
            .validate_object_timers()
            .map_err(|_| owners::Error::Invariant)?;
        Ok(world)
    }
    /// Execute an authenticated finite read through the installed native ports.
    /// No publication, draw, operation ID, or preparation counter is consumed.
    pub fn observe_wand(
        &self,
        read: wand::WandRead,
    ) -> Result<wand::WandObservation, OwnerBridgeError> {
        self.observe_wand_scoped(read, Scope::All)
    }
    /// Apply an additional caller scope ceiling (useful to hosts delegating reads).
    pub fn observe_wand_scoped(
        &self,
        read: wand::WandRead,
        scope: Scope,
    ) -> Result<wand::WandObservation, OwnerBridgeError> {
        read.query.validate()?;
        let request = encode(&read, wand::MAX_REQUEST_BYTES)?;
        self.session.with_runtime(&self.credential.borrow(), |b| {
            let caller = bridge_caller(b, &self.config)?;
            let bytes = b.kernel()?.invoke(
                caller,
                "observe",
                &request,
                Authority {
                    access: Access::Read,
                    scope,
                },
            )?;
            let wire: wand::WireObservation = decode(&bytes, wand::MAX_RESPONSE_BYTES)?;
            let stamps = bridge_stamps(b)?;
            let stamp = stamps
                .iter()
                .find(|s| wand::OwnerWitness::from(*s) == wire.witness)
                .ok_or(owners::Error::Stale)?
                .clone();
            let dependencies = wire
                .dependencies
                .iter()
                .map(|w| {
                    stamps
                        .iter()
                        .find(|s| wand::OwnerWitness::from(*s) == *w)
                        .cloned()
                        .ok_or(owners::Error::Stale)
                })
                .collect::<owners::Result<Vec<_>>>()?;
            Ok::<_, OwnerBridgeError>(wand::WandObservation {
                stamp,
                dependencies,
                facts: wire.facts,
            })
        })?
    }
    pub fn record_bill_terms(&self, id: ItemId, terms: BillTerms) -> Result<(), BillError> {
        self.receipt(id, terms)
            .map_err(|_| BillError::InvalidReceipt)?
    }
    fn receipt(
        &self,
        id: ItemId,
        terms: BillTerms,
    ) -> Result<Result<(), BillError>, OwnerBridgeError> {
        let request = encode(&ReceiptRequest { id, terms }, PAYLOAD)?;
        let result = self.session.with_runtime(&self.credential.borrow(), |b| {
            let caller = bridge_caller(b, &self.config)?;
            let cleanup = b.host().cleanup_identity()?;
            let (lease, operation) = b.update_kernel(|k| {
                let lease = k.capture_pipeline(
                    caller,
                    "run",
                    Authority {
                        access: Access::Write,
                        scope: Scope::All,
                    },
                )?;
                let id = k.issue_operation()?;
                let operation =
                    k.operation_binding_with_cleanup(id, lease, 1, COMMAND, &[cleanup])?;
                Ok::<_, kernel::Error>((lease, operation))
            })??;
            let response = b.kernel()?.invoke_operation(
                &operation,
                &request,
                Authority {
                    access: Access::Write,
                    scope: Scope::All,
                },
            );
            b.update_kernel(|k| k.release_pipeline(lease))??;
            let response = response?;
            Ok::<_, OwnerBridgeError>(decode(&response, PAYLOAD)?)
        })?;
        self.credential
            .replace(self.session.issue_credential(&self.key)?);
        result
    }
}

#[cfg(test)]
mod tests {
    mod admission;
    mod fresh_process;
    mod wand_ports;
    use super::*;
    use nethacked_arena::ItemLocation;
    use nethacked_types::Buc;

    fn source() -> (OwnerBridge, ItemId) {
        let mut w = SimulationWorld::new_with_seed(804);
        w.hero.skills.skills.clear();
        let object = w
            .ruleset
            .create_item_record(
                "wand of fire",
                ItemLocation::CarriedBy(w.player_id),
                Buc::Uncursed,
            )
            .unwrap();
        let id = w.arena.spawn_item(object);
        w.unpaid_items.push((id, 19));
        let c = OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone()).unwrap();
        let json = w.to_save_json().unwrap();
        (
            OwnerBridge::import_native(&json, json.len(), c).unwrap(),
            id,
        )
    }
    fn snapshots(bridge: &OwnerBridge) -> Vec<(owners::Stamp, Vec<u8>)> {
        bridge
            .session
            .with_runtime(&bridge.credential.borrow(), |b| {
                macro_rules! state {
                    ($t:ident, $key:literal) => {{
                        let owner = &b.owner::<$t>($key).unwrap().owner;
                        (owner.stamp().unwrap(), owner.inspect().unwrap())
                    }};
                }
                vec![
                    state!(GeographyState, "Geography"),
                    state!(ObjectsState, "Objects"),
                    state!(ActorsState, "Actors"),
                    state!(HeroState, "Hero"),
                    state!(EquipmentState, "Equipment"),
                    state!(BillingState, "Billing"),
                    state!(ClockState, "Clock"),
                    state!(RandomState, "Random"),
                    state!(SessionState, "Session"),
                ]
            })
            .unwrap()
    }
    #[test]
    fn owner_runtime_independent_allocations_source_disposal_and_billing_only_revision() {
        let (source, id) = source();
        let before = snapshots(&source);
        assert_eq!(
            before
                .iter()
                .map(|(s, _)| s.owner)
                .collect::<BTreeSet<_>>()
                .len(),
            9
        );
        let weak = source
            .session
            .with_runtime(&source.credential.borrow(), |b| {
                (
                    Rc::downgrade(&b.owner::<ObjectsState>("Objects").unwrap().owner),
                    Rc::downgrade(&b.owner::<BillingState>("Billing").unwrap().owner),
                )
            })
            .unwrap();
        let wire = source.capture().unwrap();
        let config = source.config.clone();
        drop(source);
        assert!(weak.0.upgrade().is_none());
        assert!(weak.1.upgrade().is_none());
        let restored = OwnerBridge::reconstruct(&wire, config).unwrap();
        let fresh = snapshots(&restored);
        for ((old, bytes), (new, rebuilt)) in before.iter().zip(&fresh) {
            assert_ne!(old.owner, new.owner);
            assert_eq!(old.generation, new.generation);
            assert_eq!(old.revision, new.revision);
            assert_eq!(bytes, rebuilt);
        }
        let terms = BillTerms {
            billed_quantity: Some(1),
            unit_price: Some(19),
            creditor: None,
            legacy_base_cost: 19,
        };
        restored.record_bill_terms(id, terms.clone()).unwrap();
        let after = snapshots(&restored);
        for (index, ((before, old), (after, new))) in fresh.iter().zip(&after).enumerate() {
            if index == 5 {
                assert_eq!(after.revision, before.revision + 1);
                assert_ne!(old, new);
            } else {
                assert_eq!(before, after);
                assert_eq!(old, new);
            }
        }
        assert_eq!(
            restored.record_bill_terms(id, terms),
            Err(BillError::ReceiptAlreadyRecorded)
        );
        assert_eq!(snapshots(&restored), after);
    }
    #[test]
    fn owner_runtime_exact_bounded_codecs_and_unavailable_current_catalog_refuse() {
        let (source, _) = source();
        let config = source.config.clone();
        let parts = source.partitions().unwrap();
        macro_rules! exact {
            ($state:ident, $value:expr) => {{
                let bytes = encode(&$value, PAYLOAD).unwrap();
                assert!(decode::<$state>(&bytes, bytes.len()).is_ok());
                assert!(decode::<$state>(&bytes, bytes.len() - 1).is_err());
                let mut value = serde_json::to_value(&$value).unwrap();
                value
                    .as_object_mut()
                    .unwrap()
                    .insert("unsupported".into(), serde_json::json!(true));
                assert!(decode::<$state>(&serde_json::to_vec(&value).unwrap(), PAYLOAD).is_err());
            }};
        }
        exact!(GeographyState, parts.geography);
        exact!(ObjectsState, parts.objects);
        exact!(ActorsState, parts.actors);
        exact!(HeroState, parts.hero);
        exact!(EquipmentState, parts.equipment);
        exact!(BillingState, parts.billing);
        exact!(ClockState, parts.clock);
        exact!(RandomState, parts.random);
        exact!(SessionState, parts.session);
        let mut nested = serde_json::to_value(&parts.billing).unwrap();
        nested["billing"]["unsupported"] = serde_json::json!(true);
        assert!(decode::<BillingState>(&serde_json::to_vec(&nested).unwrap(), PAYLOAD).is_err());
        assert!(decode::<RandomState>(b"{\"seed\":1,\"seed\":2}", PAYLOAD).is_err());
        assert!(decode::<RandomState>(&vec![b' '; PAYLOAD + 1], usize::MAX).is_err());
        let wire = source.capture().unwrap();
        drop(source);
        for missing_factory in [true, false] {
            let mut authority = restore_authority(&config).unwrap();
            if missing_factory {
                authority.owners.remove("Billing");
            } else {
                authority.owners.insert(
                    "Billing".into(),
                    Rc::new(session::TypedFactory::<BillingState> {
                        codec: Rc::new(BillingCodec(config.clone())),
                        catalog: op::RestoreCatalog::default(),
                    }),
                );
            }
            let c = session::SessionFactory::new(authority)
                .configuration(binding())
                .unwrap();
            let (destination, key, _) = session::Session::new(
                Kernel::new(kernel::Limits::default()).unwrap(),
                Host::new(limits()).unwrap(),
                vec![],
                vec![],
                op::AccountingCaptureCatalog::default(),
                session::Services::default(),
                c,
            )
            .unwrap();
            assert!(destination
                .prepare_restore(&key, &binding(), &wire)
                .is_err());
        }
    }
}
