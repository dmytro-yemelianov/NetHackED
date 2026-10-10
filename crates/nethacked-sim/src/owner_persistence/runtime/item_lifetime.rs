//! Authenticated lifetime facts and a sealed five-owner publication.
use super::*;
use crate::billing::{ConsumedBillRecord, WholeDisposal};
use crate::item_lifetime::{self as algorithm, LifetimePort, PreparedWholeDisposal};
use nethacked_arena::{ItemLocation, ItemRecord};
use wand::*;

pub(super) const NATIVE_ID: &[u8] = b"NetHackED M5-I3 lifetime owner code v4";
pub(super) const CALLER_ID: &[u8] = b"NetHackED M5-I3 gameplay and lifetime caller v4";
pub(super) const COMMAND: &str = "wand_lifetime_v4";
pub(super) const ENDPOINT: &str = "finalize_v4";
pub(super) const OBJECT_QUERY: &str = "wand_lifetime_object_v4";
pub(super) const BILL_QUERY: &str = "wand_lifetime_billing_v4";
pub(super) const READS: [(&str, &str); 2] = [("Objects", OBJECT_QUERY), ("Billing", BILL_QUERY)];
pub(super) const LIFETIME_OWNERS: [&str; 5] = ["Objects", "Equipment", "Hero", "Billing", "Clock"];
const POLICY: [u8; 32] = [247; 32];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectFacts {
    pub target: ObjectRef,
    pub object: Option<ItemRecord>,
    pub cached_index: Option<usize>,
    pub ambiguous: bool,
    pub eligible: Result<(), BillError>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerFacts {
    pub target: ObjectRef,
    pub result: Result<(Option<u32>, Option<BillTerms>), BillError>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LifetimeAction {
    pub survivor: Option<ObjectRef>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifetimeError {
    Owner,
    Billing(BillError),
}
impl From<BillError> for LifetimeError {
    fn from(e: BillError) -> Self {
        Self::Billing(e)
    }
}
impl From<owners::Error> for LifetimeError {
    fn from(_: owners::Error) -> Self {
        Self::Owner
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifetimeReply {
    Unchanged,
    Retired { billed: bool },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Retirement {
    target: ObjectRef,
    object: ItemRecord,
    base: Option<u32>,
    terms: Option<BillTerms>,
    consumed: Option<ConsumedBillRecord>,
}
fn geo(obs: &[Observation], index: usize, target: ObjectRef) -> owners::Result<GeographyFacts> {
    match fact(obs, index)? {
        WandFacts::Geography(g) if g.residency == target.residency => Ok(g),
        _ => Err(owners::Error::Schema),
    }
}
fn fact(obs: &[Observation], index: usize) -> owners::Result<WandFacts> {
    decode(
        obs.get(index).ok_or(owners::Error::Schema)?.bytes(),
        MAX_RESPONSE_BYTES,
    )
}
fn selected(s: &ObjectsState, target: ObjectRef, index: Option<usize>) -> Option<&ItemRecord> {
    match index {
        None => s.items.get(target.id),
        Some(i) => s
            .stored_items
            .get(i)?
            .iter()
            .find(|(id, _)| *id == target.id)
            .map(|(_, o)| o),
    }
}
pub(super) fn object_facts(
    s: &ObjectsState,
    target: ObjectRef,
    obs: &[Observation],
    c: &OwnerBridgeConfig,
) -> owners::Result<ObjectFacts> {
    if !c.item_lifetime {
        return Err(owners::Error::Unsupported);
    }
    let g = geo(obs, 2, target)?;
    let player = match fact(obs, 0)? {
        WandFacts::Hero(h) => h.player,
        _ => return Err(owners::Error::Schema),
    };
    let object = selected(s, target, g.stored_index);
    let occurrences = usize::from(s.items.get(target.id).is_some())
        + s.stored_items
            .iter()
            .flat_map(|items| items.iter())
            .filter(|(id, _)| *id == target.id)
            .count();
    let eligible = (|| {
        let o = object.ok_or(BillError::StaleItem)?;
        if target.residency == Residency::Active && o.location != ItemLocation::CarriedBy(player) {
            return Err(BillError::NotCarried);
        }
        let definition = match &o.definition {
            Some(d) => c.ruleset.item_for_definition(d),
            None => c.ruleset.item(&o.name),
        }
        .ok_or(BillError::InvalidDefinition)?;
        if definition.class != o.class {
            return Err(BillError::InvalidDefinition);
        }
        let child = match g.stored_index {
            None => s
                .items
                .iter()
                .any(|(_, item)| item.location == ItemLocation::InContainer(target.id)),
            Some(i) => s.stored_items[i]
                .iter()
                .any(|(_, item)| item.location == ItemLocation::InContainer(target.id)),
        };
        if o.class != nethacked_types::ItemClass::Wand || o.is_container || child {
            return Err(BillError::UnsupportedBillingScope);
        }
        if o.quantity == 0 {
            return Err(BillError::ZeroQuantity);
        }
        Ok(())
    })();
    Ok(ObjectFacts {
        target,
        object: object.cloned(),
        cached_index: g.stored_index,
        ambiguous: occurrences > 1,
        eligible,
    })
}
fn object_observation(obs: &[Observation], target: ObjectRef) -> owners::Result<ObjectFacts> {
    for o in obs {
        if o.query() == OBJECT_QUERY {
            return match decode(o.bytes(), MAX_RESPONSE_BYTES)? {
                WandFacts::Command(CommandFacts::LifetimeObject(f)) if f.target == target => Ok(f),
                _ => Err(owners::Error::Schema),
            };
        }
    }
    Err(owners::Error::Schema)
}
fn ledger(
    s: &BillingState,
    object: &ObjectFacts,
) -> Result<(Option<u32>, Option<BillTerms>), BillError> {
    object.eligible.clone()?;
    let id = object.target.id;
    if s.billing.consumed.iter().any(|r| r.origin == id) {
        return Err(BillError::AlreadyConsumed);
    }
    let receipts: Vec<_> = s
        .billing
        .live_terms
        .iter()
        .filter(|(i, _)| *i == id)
        .collect();
    let debts: Vec<_> = s
        .unpaid_items
        .iter()
        .chain(s.stored_unpaid_items.iter().flatten())
        .filter(|(i, _)| *i == id)
        .collect();
    // Historical receipts lack residency. Refuse an ambiguous identity before
    // deleting either the active object or a cached alias.
    if object.ambiguous || receipts.len() > 1 || debts.len() > 1 {
        return Err(BillError::DuplicateLedger);
    }
    let scoped = match object.cached_index {
        None => &s.unpaid_items,
        Some(i) => s
            .stored_unpaid_items
            .get(i)
            .ok_or(BillError::InvalidReceipt)?,
    };
    if !debts.is_empty() && !scoped.iter().any(|(i, _)| *i == id) {
        return Err(BillError::InvalidReceipt);
    }
    let base = debts.first().map(|(_, base)| *base);
    let terms = receipts.first().map(|(_, terms)| terms.clone());
    if let Some(terms) = &terms {
        validate_bill_terms(
            terms,
            object.object.as_ref().ok_or(BillError::StaleItem)?.quantity,
            base.ok_or(BillError::InvalidReceipt)?,
        )?;
    }
    Ok((base, terms))
}
pub(super) fn billing_facts(
    s: &BillingState,
    target: ObjectRef,
    obs: &[Observation],
    c: &OwnerBridgeConfig,
) -> owners::Result<LedgerFacts> {
    if !c.item_lifetime {
        return Err(owners::Error::Unsupported);
    }
    let object = object_observation(obs, target)?;
    let g = geo(obs, 1, target)?;
    if object.cached_index != g.stored_index {
        return Err(owners::Error::Schema);
    }
    Ok(LedgerFacts {
        target,
        result: ledger(s, &object),
    })
}
fn queries(target: ObjectRef) -> Vec<WandQuery> {
    vec![
        WandQuery::Command(CommandQuery::LifetimeObject { target }),
        WandQuery::Equipment(target),
        WandQuery::Hero,
        WandQuery::Command(CommandQuery::LifetimeBilling { target }),
        WandQuery::Clock(target),
        WandQuery::Geography(GeographyRequest {
            residency: target.residency,
            cells: vec![],
            trajectory: None,
        }),
        WandQuery::Session,
    ]
}
fn declarations(role: &str) -> Vec<(&'static str, &'static str, u8)> {
    let allowed: &[&str] = match role {
        "Objects" => &["Hero", "Geography", "Session"],
        "Hero" => &[],
        "Equipment" | "Billing" | "Clock" => &["Objects"],
        _ => &[],
    };
    [
        ("Objects", OBJECT_QUERY, 2),
        ("Hero", "wand_hero", 4),
        ("Billing", BILL_QUERY, 6),
        ("Geography", "wand_geography", 1),
        ("Session", "wand_session", 9),
    ]
    .into_iter()
    .filter(|(owner, _, _)| allowed.contains(owner))
    .collect()
}
pub(super) fn add_ports(
    role: &str,
    mut ports: BTreeMap<String, Port>,
    ids: &BTreeMap<&str, owners::OwnerId>,
    c: &OwnerBridgeConfig,
) -> owners::Result<BTreeMap<String, Port>> {
    if c.item_lifetime && LIFETIME_OWNERS.contains(&role) {
        let deps = declarations(role)
            .into_iter()
            .map(|(owner, query, tag)| {
                Ok(Dependency {
                    owner: *ids.get(owner).ok_or(owners::Error::Schema)?,
                    query: query.into(),
                    schema: [c.schema_tag(tag); 32],
                })
            })
            .collect::<owners::Result<_>>()?;
        ports.insert(COMMAND.into(), port(Access::Write, deps));
    }
    Ok(ports)
}
pub(super) fn validate_ports(
    role: &str,
    ports: &BTreeMap<String, Port>,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if c.item_lifetime && LIFETIME_OWNERS.contains(&role) {
        let p = ports.get(COMMAND).ok_or(owners::Error::Schema)?;
        let expected = declarations(role);
        if p.access != Access::Write
            || p.major != 1
            || p.dependencies.len() != expected.len()
            || p.dependencies
                .iter()
                .zip(expected)
                .any(|(d, (_, q, tag))| d.query != q || d.schema != [c.schema_tag(tag); 32])
        {
            return Err(owners::Error::Schema);
        }
    }
    Ok(())
}
fn validate_request(
    request: &Retirement,
    obs: &[Observation],
    role: &str,
) -> owners::Result<Option<usize>> {
    let index = if role == "Objects" {
        let geographic = obs
            .iter()
            .find(|o| o.query() == "wand_geography")
            .ok_or(owners::Error::Schema)?;
        geo(std::slice::from_ref(geographic), 0, request.target)?.stored_index
    } else if role == "Hero" {
        None
    } else {
        object_observation(obs, request.target)?.cached_index
    };
    if algorithm::negative_survivor(
        Some(request.target),
        Some((request.target, &request.object)),
    )
    .is_none()
    {
        return Err(owners::Error::Invariant);
    }
    if role != "Objects" && role != "Hero" {
        let f = object_observation(obs, request.target)?;
        f.eligible.map_err(|_| owners::Error::Invariant)?;
        if f.object.as_ref() != Some(&request.object) || f.cached_index != index {
            return Err(owners::Error::Stale);
        }
    }
    let consumed = algorithm::prepare_disposal(
        request.target.id,
        &request.object,
        request.base,
        request.terms.as_ref(),
    )
    .map_err(|_| owners::Error::Invariant)?
    .into_consumed();
    if consumed != request.consumed {
        return Err(owners::Error::Invariant);
    }
    Ok(index)
}
pub(crate) trait LifetimeDomain: Clone {
    fn prepare_lifetime(
        &self,
        _: &Retirement,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        Err(owners::Error::Unsupported)
    }
}
macro_rules! unsupported { ($($state:ty),*) => { $(impl LifetimeDomain for $state {})* }; }
unsupported!(GeographyState, ActorsState, RandomState, SessionState);
fn proposal<T>(next: T, request: &Retirement, billing: bool) -> owners::Result<Proposal<T>> {
    use sha2::{Digest, Sha256};
    // Bind the complete decision without repeating object/receipt bytes in
    // historical replies. Actual proposed states remain with private owners.
    let reply = Sha256::digest(encode(request, MAX_REQUEST_BYTES)?).to_vec();
    Ok(Proposal {
        next,
        events: if billing { vec![dust()?] } else { vec![] },
        reply,
    })
}
fn dust() -> owners::Result<Vec<u8>> {
    encode(
        &crate::GameEvent::LogMessage {
            text: "Your wand turns to dust.".into(),
        },
        MAX_RESPONSE_BYTES,
    )
}
impl LifetimeDomain for ObjectsState {
    fn prepare_lifetime(
        &self,
        request: &Retirement,
        obs: &[Observation],
        c: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        let index = validate_request(request, obs, "Objects")?;
        if selected(self, request.target, index) != Some(&request.object) {
            return Err(owners::Error::Stale);
        }
        // Recompute this owner's bounded eligibility using authenticated facts.
        let find = |name| {
            obs.iter()
                .find(|o| o.query() == name)
                .cloned()
                .ok_or(owners::Error::Schema)
        };
        let object = object_facts(
            self,
            request.target,
            &[
                find("wand_hero")?,
                find("wand_session")?,
                find("wand_geography")?,
            ],
            c,
        )?;
        object.eligible.map_err(|_| owners::Error::Invariant)?;
        if object.ambiguous {
            return Err(owners::Error::Invariant);
        }
        let mut next = self.clone();
        match index {
            None => {
                next.items
                    .remove(request.target.id)
                    .ok_or(owners::Error::Stale)?;
            }
            Some(i) => next.stored_items[i].retain(|(id, _)| *id != request.target.id),
        }
        proposal(next, request, false)
    }
}
impl LifetimeDomain for EquipmentState {
    fn prepare_lifetime(
        &self,
        request: &Retirement,
        obs: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        validate_request(request, obs, "Equipment")?;
        let mut next = self.clone();
        if request.target.residency == Residency::Active {
            algorithm::clear_equipment(
                &mut next.equipment,
                &mut next.wielded_item,
                request.target.id,
            );
        }
        proposal(next, request, false)
    }
}
impl LifetimeDomain for HeroState {
    fn prepare_lifetime(
        &self,
        request: &Retirement,
        obs: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        validate_request(request, obs, "Hero")?;
        let mut next = self.clone();
        if request.target.residency == Residency::Active {
            algorithm::clear_reference(&mut next.hero.quivered_item, request.target.id);
        }
        proposal(next, request, false)
    }
}
impl LifetimeDomain for ClockState {
    fn prepare_lifetime(
        &self,
        request: &Retirement,
        obs: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        validate_request(request, obs, "Clock")?;
        let mut next = self.clone();
        next.object_timers.cancel(match request.target.residency {
            Residency::Active => crate::ObjectTimerTarget::Active(request.target.id),
            Residency::Cached { branch, depth } => crate::ObjectTimerTarget::Cached {
                branch,
                depth,
                item: request.target.id,
            },
        });
        proposal(next, request, false)
    }
}
impl LifetimeDomain for BillingState {
    fn prepare_lifetime(
        &self,
        request: &Retirement,
        obs: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        let index = validate_request(request, obs, "Billing")?;
        let object = object_observation(obs, request.target)?;
        if ledger(self, &object).map_err(|_| owners::Error::Invariant)?
            != (request.base, request.terms.clone())
        {
            return Err(owners::Error::Stale);
        }
        let mut next = self.clone();
        if let Some(consumed) = &request.consumed {
            next.billing.consumed.push(consumed.clone());
        }
        match index {
            None => next.unpaid_items.retain(|(id, _)| *id != request.target.id),
            Some(i) => next
                .stored_unpaid_items
                .get_mut(i)
                .ok_or(owners::Error::Schema)?
                .retain(|(id, _)| *id != request.target.id),
        }
        next.billing
            .live_terms
            .retain(|(id, _)| *id != request.target.id);
        proposal(next, request, true)
    }
}

struct LifetimeAccounting;
impl Accounting for LifetimeAccounting {
    fn validate(
        &self,
        proposals: &[ProposalView],
        observations: &[Observation],
    ) -> owners::Result<()> {
        if proposals.len() != LIFETIME_OWNERS.len() {
            return Err(owners::Error::Invariant);
        }
        let mut owners = BTreeSet::new();
        let mut canonical = None;
        let mut roles = BTreeSet::new();
        for p in proposals {
            let role = match p.receipt.stamp.schema[0] {
                232 => "Objects",
                235 => "Equipment",
                234 => "Hero",
                236 => "Billing",
                237 => "Clock",
                _ => return Err(owners::Error::Invariant),
            };
            let (query, tag) = match role {
                "Objects" => (OBJECT_QUERY, 2),
                "Equipment" => ("wand_equipment", 5),
                "Hero" => ("wand_hero", 4),
                "Billing" => (BILL_QUERY, 6),
                "Clock" => ("wand_clock", 7),
                _ => return Err(owners::Error::Invariant),
            };
            if p.receipt.command != COMMAND
                || p.receipt.stamp.schema != [230 + tag; 32]
                || !owners.insert(p.receipt.stamp.owner)
                || !roles.insert(role)
                || (matches!(role, "Objects" | "Billing")
                    && !observations
                        .iter()
                        .any(|o| o.query() == query && o.stamp() == &p.receipt.stamp))
                || p.receipt.stamp.module != proposals[0].receipt.stamp.module
                || p.events
                    != if role == "Billing" {
                        vec![dust()?]
                    } else {
                        vec![]
                    }
            {
                return Err(owners::Error::Invariant);
            }
            if p.reply.len() != 32 || canonical.as_ref().is_some_and(|prior| prior != &p.reply) {
                return Err(owners::Error::Invariant);
            }
            canonical = Some(p.reply.clone());
        }
        Ok(())
    }
}
fn accounting_identity() -> op::ArtifactIdentity {
    artifact("native-lifetime-i3-five-owner-accounting", 247)
}
pub(super) fn capture_accounting(
    a: &mut op::AccountingCaptureCatalog,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if c.item_lifetime {
        a.register(accounting_identity(), POLICY, Rc::new(LifetimeAccounting))?;
    }
    Ok(())
}
pub(super) fn restore_accounting(
    a: &mut op::AccountingRestoreCatalog,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if c.item_lifetime {
        a.allow(accounting_identity(), POLICY, || {
            Ok(Rc::new(LifetimeAccounting))
        })?;
    }
    Ok(())
}
struct NativeLifetime<'a, 'v> {
    routes: &'a SourceRoutes,
    view: &'a CallerView<'v>,
    config: &'a OwnerBridgeConfig,
    object: Option<ObjectFacts>,
    ledger: Option<LedgerFacts>,
}
impl LifetimePort for NativeLifetime<'_, '_> {
    type Target = ObjectRef;
    type Error = LifetimeError;
    fn object(&mut self, target: ObjectRef) -> Result<Option<ItemRecord>, Self::Error> {
        let (o, _) = self.routes.observe(
            &WandQuery::Command(CommandQuery::LifetimeObject { target }),
            self.view,
        )?;
        let WandFacts::Command(CommandFacts::LifetimeObject(f)) =
            decode(o.bytes(), MAX_RESPONSE_BYTES)?
        else {
            return Err(LifetimeError::Owner);
        };
        if f.target != target {
            return Err(LifetimeError::Owner);
        }
        let record = f.object.clone();
        self.object = Some(f);
        Ok(record)
    }
    fn origin(&self, target: ObjectRef) -> ItemId {
        target.id
    }
    fn accounting(
        &mut self,
        target: ObjectRef,
        _: &ItemRecord,
    ) -> Result<(Option<u32>, Option<BillTerms>), Self::Error> {
        let (o, _) = self.routes.observe(
            &WandQuery::Command(CommandQuery::LifetimeBilling { target }),
            self.view,
        )?;
        let WandFacts::Command(CommandFacts::LifetimeBilling(f)) =
            decode(o.bytes(), MAX_RESPONSE_BYTES)?
        else {
            return Err(LifetimeError::Owner);
        };
        if f.target != target {
            return Err(LifetimeError::Owner);
        }
        let result = f.result.clone().map_err(LifetimeError::Billing)?;
        self.ledger = Some(f);
        Ok(result)
    }
    fn publish(
        &mut self,
        target: ObjectRef,
        prepared: PreparedWholeDisposal,
    ) -> Result<WholeDisposal, Self::Error> {
        let object = self
            .object
            .as_ref()
            .and_then(|f| f.object.clone())
            .ok_or(LifetimeError::Owner)?;
        let (base, terms) = self
            .ledger
            .as_ref()
            .ok_or(LifetimeError::Owner)?
            .result
            .clone()
            .map_err(LifetimeError::Billing)?;
        let request = Retirement {
            target,
            object,
            base,
            terms,
            consumed: prepared.into_consumed(),
        };
        let bytes = encode(&request, MAX_REQUEST_BYTES)?;
        let mut immediate = vec![];
        let mut provenance = vec![];
        for q in queries(target) {
            let (o, _) = self.routes.observe(&q, self.view)?;
            // Equipment, Hero and Clock are sealed writers: their own prepare
            // stamps already protect their full local state. Retain the two
            // finite decision roots; their nested residency/code observations
            // remain authenticated by the existing Host protocol.
            if matches!(
                q,
                WandQuery::Command(
                    CommandQuery::LifetimeObject { .. } | CommandQuery::LifetimeBilling { .. }
                )
            ) {
                provenance.push(o.clone());
            }
            immediate.push(o);
        }
        // The selected object and accounting are checked again by private
        // owners against these fresh, invocation-authenticated observations.
        let observations = |role| {
            immediate
                .iter()
                .filter(|o| {
                    declarations(role)
                        .iter()
                        .any(|(_, query, _)| o.query() == *query)
                })
                .cloned()
                .collect::<Vec<_>>()
        };
        let mut candidates = vec![];
        macro_rules! stage {
            ($field:ident, $role:literal) => {
                candidates.push(
                    self.routes
                        .$field
                        .prepare(COMMAND, &bytes, observations($role), self.view)?
                        .erase(),
                );
            };
        }
        stage!(objects, "Objects");
        stage!(equipment, "Equipment");
        stage!(hero, "Hero");
        stage!(billing, "Billing");
        stage!(clock, "Clock");
        let ids = LIFETIME_OWNERS
            .iter()
            .map(|role| self.routes.stamp(role).map(|s| s.owner))
            .collect::<owners::Result<Vec<_>>>()?;
        let batch = self.routes.host.batch(
            candidates,
            provenance,
            &ids,
            ids.clone(),
            POLICY,
            Rc::new(LifetimeAccounting),
            profile_capability(self.config, COMMAND),
            self.view,
        )?;
        if let Err(e) = batch.commit(self.view) {
            batch.discard()?;
            return Err(e.into());
        }
        Ok(if request.consumed.is_some() {
            WholeDisposal::RetainedBilled { origin: target.id }
        } else {
            WholeDisposal::Unbilled { origin: target.id }
        })
    }
}
fn execute(
    routes: &SourceRoutes,
    bytes: &[u8],
    view: &CallerView,
    c: &OwnerBridgeConfig,
) -> owners::Result<Vec<u8>> {
    if !c.item_lifetime
        || view.operation().is_none_or(|o| {
            o.phase() != ENDPOINT
                && o.phase() != super::wand_gameplay::ENDPOINT
                && !super::wand_workflows::allows_gameplay_owner(c, view, o.phase())
        })
    {
        return Err(owners::Error::Authority);
    }
    let action: LifetimeAction = decode(bytes, MAX_REQUEST_BYTES)?;
    let mut port = NativeLifetime {
        routes,
        view,
        config: c,
        object: None,
        ledger: None,
    };
    let result =
        algorithm::finalize_survivor(&mut port, action.survivor).map(|result| match result {
            None => LifetimeReply::Unchanged,
            Some(WholeDisposal::Unbilled { .. }) => LifetimeReply::Retired { billed: false },
            Some(WholeDisposal::RetainedBilled { .. }) => LifetimeReply::Retired { billed: true },
        });
    encode(&result, MAX_RESPONSE_BYTES)
}
pub(super) fn dependency_slot(c: &OwnerBridgeConfig) -> usize {
    3 + super::wand_commands::read_routes(c).len() + 7
}
pub(super) fn native_ports(
    mut ports: BTreeMap<String, NativePort>,
    routes: Rc<Routes>,
    c: OwnerBridgeConfig,
) -> BTreeMap<String, NativePort> {
    if c.item_lifetime {
        ports.insert(
            COMMAND.into(),
            Rc::new(move |request, view| {
                execute(&routes.get()?, request, view, &c).map_err(native_refusal)
            }),
        );
    }
    ports
}
pub(super) fn caller_ports(
    mut ports: BTreeMap<String, NativePort>,
    c: &OwnerBridgeConfig,
) -> BTreeMap<String, NativePort> {
    if c.item_lifetime {
        let slot = dependency_slot(c);
        ports.insert(
            ENDPOINT.into(),
            Rc::new(move |request, view| view.call_dependency(slot, request)),
        );
    }
    ports
}

impl OwnerBridge {
    /// Finalize the explicitly retained identity through the approved lifetime
    /// module. Absence and nonnegative survivors are complete no-ops.
    pub fn finalize_retained_object(
        &self,
        survivor: Option<ObjectRef>,
    ) -> Result<Result<LifetimeReply, LifetimeError>, OwnerBridgeError> {
        self.finalize_retained_object_scoped(survivor, Scope::All)
    }
    pub fn finalize_retained_object_scoped(
        &self,
        survivor: Option<ObjectRef>,
        scope: Scope,
    ) -> Result<Result<LifetimeReply, LifetimeError>, OwnerBridgeError> {
        if !self.config.item_lifetime {
            return Err(owners::Error::Unsupported.into());
        }
        let bytes = encode(&LifetimeAction { survivor }, MAX_REQUEST_BYTES)?;
        let response = self.session.with_runtime(&self.credential.borrow(), |b| {
            let caller = bridge_caller(b, &self.config)?;
            if !approved_profiles(&self.config).iter().any(|profile| {
                profile.item_lifetime
                    && b.kernel().is_ok_and(|k| {
                        k.manifest(caller)
                            .is_ok_and(|m| m == &profile_caller(profile))
                    })
            }) {
                return Err(owners::Error::Unsupported.into());
            }
            let cleanup = b.host().cleanup_identity()?;
            let (lease, operation) = b.update_kernel(|k| {
                let lease = k.capture_pipeline(
                    caller,
                    ENDPOINT,
                    Authority {
                        access: Access::Write,
                        scope: scope.clone(),
                    },
                )?;
                let id = k.issue_operation()?;
                let operation =
                    k.operation_binding_with_cleanup(id, lease, 1, ENDPOINT, &[cleanup])?;
                Ok::<_, kernel::Error>((lease, operation))
            })??;
            let result = b.kernel()?.invoke_operation(
                &operation,
                &bytes,
                Authority {
                    access: Access::Write,
                    scope,
                },
            );
            b.update_kernel(|k| k.release_pipeline(lease))??;
            Ok::<_, OwnerBridgeError>(decode(&result?, MAX_RESPONSE_BYTES)?)
        })?;
        self.session
            .replace_configuration(&self.key, restore_configuration(&self.config)?)?;
        self.credential
            .replace(self.session.issue_credential(&self.key)?);
        response
    }
}

/// Exact resources used by this finite module, including nested readonly facts.
/// It receives no actor, random, geometry-write or unrelated command authority.
pub(super) fn grant_names() -> BTreeSet<String> {
    let mut keys: BTreeSet<_> = [
        "publication",
        "1.wand_geography",
        "2.wand_hero",
        "3.wand_session",
        "4.wand_objects",
        "4.wand_lifetime_object_v4",
        "6.wand_equipment",
        "7.wand_lifetime_billing_v4",
        "8.wand_clock",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    for id in [2, 4, 6, 7, 8] {
        keys.insert(format!("{id}.{COMMAND}"));
    }
    keys
}
pub(super) fn grant_keys(keys: &BTreeSet<Resource>) -> BTreeSet<Resource> {
    let allowed = grant_names();
    keys.iter()
        .filter(|r| allowed.contains(&r.key))
        .cloned()
        .collect()
}
