//! Explicit M6-I1 frame profile. Current cancellation only; no turn orchestration.
use super::*;
use crate::actions::{command_frame as law, equipment::EquipmentState as Equipment};
use nethacked_arena::{ActorId, ItemId};
use wand::OwnerWitness;
pub(super) const MAX: usize = 32;
pub(super) const PREVIEW: &str = "frame_preview_i1";
pub(super) const RUN: &str = "frame_cancel_i1";
pub(super) const EQ: &str = "frame_equipment_i1";
pub(super) const ITEMS: &str = "frame_items_i1";
pub(super) const HERO: &str = "frame_hero_i1";
pub(super) const ACTOR: &str = "frame_actor_i1";
pub(super) const WRITE_EQ: &str = "frame_reconcile_i1";
pub(super) const WRITE_AC: &str = "frame_ac_i1";
pub(super) const RECORD: &str = "frame_record_i1";
const POLICY: [u8; 32] = [181; 32];
const ROUTES: [(&str, &str, Access); 7] = [
    ("Equipment", EQ, Access::Read),
    ("Objects", ITEMS, Access::Read),
    ("Hero", HERO, Access::Read),
    ("Actors", ACTOR, Access::Read),
    ("Equipment", WRITE_EQ, Access::Write),
    ("Actors", WRITE_AC, Access::Write),
    ("Session", RECORD, Access::Write),
];
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefaultRequest {
    guards: Vec<OwnerWitness>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefaultReply {
    pub events: Vec<crate::GameEvent>,
    pub error: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) enum Facts {
    Equipment(Equipment),
    Hero {
        player: ActorId,
        helpless: u32,
        protection: u32,
    },
    Items(Vec<(ItemId, Option<law::ArmorItem>)>),
    Actor(Option<bool>),
}
pub(super) struct Items(Vec<(ItemId, Option<law::ArmorItem>)>);
impl law::Objects for Items {
    fn armor(&self, id: ItemId) -> Option<law::ArmorItem> {
        self.0
            .iter()
            .find(|(i, _)| *i == id)
            .and_then(|(_, x)| x.clone())
    }
}
pub(super) fn fact(o: &Observation) -> owners::Result<Facts> {
    decode(o.bytes(), wand::MAX_RESPONSE_BYTES)
}
pub(super) fn hero(o: &Observation) -> owners::Result<(ActorId, u32, u32)> {
    match fact(o)? {
        Facts::Hero {
            player,
            helpless,
            protection,
        } => Ok((player, helpless, protection)),
        _ => Err(owners::Error::Schema),
    }
}
pub(super) fn items(o: &Observation) -> owners::Result<Items> {
    match fact(o)? {
        Facts::Items(x) => Ok(Items(x)),
        _ => Err(owners::Error::Schema),
    }
}
fn message() -> crate::GameEvent {
    crate::GameEvent::LogMessage {
        text: "Never mind.".into(),
    }
}
fn record(s: &mut SessionState) {
    s.event_log.push(message());
    let cap = s
        .max_event_log_len
        .unwrap_or(crate::SimulationWorld::DEFAULT_MAX_EVENT_LOG_LEN);
    if s.event_log.len() > cap {
        s.event_log.drain(0..s.event_log.len() - cap);
    }
}
pub(super) trait FrameDomain: Clone {
    fn frame_query(
        &self,
        _name: &str,
        _request: &[u8],
        _c: &OwnerBridgeConfig,
    ) -> owners::Result<Vec<u8>> {
        Err(owners::Error::Unsupported)
    }
    fn frame_prepare(
        &self,
        _name: &str,
        _request: &[u8],
        _obs: &[Observation],
        _c: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        Err(owners::Error::Unsupported)
    }
}
impl FrameDomain for ObjectsState {
    fn frame_query(
        &self,
        name: &str,
        request: &[u8],
        c: &OwnerBridgeConfig,
    ) -> owners::Result<Vec<u8>> {
        if name != ITEMS {
            return Err(owners::Error::Unsupported);
        }
        let ids: Vec<ItemId> = decode(request, wand::MAX_REQUEST_BYTES)?;
        if ids.len() > MAX {
            return Err(owners::Error::Limit);
        }
        let borrowed = law::BorrowedObjects {
            items: &self.items,
            rules: &c.ruleset,
        };
        use law::Objects;
        encode(
            &Facts::Items(ids.into_iter().map(|id| (id, borrowed.armor(id))).collect()),
            wand::MAX_RESPONSE_BYTES,
        )
    }
}
impl FrameDomain for HeroState {
    fn frame_query(
        &self,
        _name: &str,
        _request: &[u8],
        _c: &OwnerBridgeConfig,
    ) -> owners::Result<Vec<u8>> {
        encode(
            &Facts::Hero {
                player: self.player_id,
                helpless: self.hero.afflictions.transient.helpless,
                protection: self.divine_protection,
            },
            wand::MAX_RESPONSE_BYTES,
        )
    }
}
impl FrameDomain for EquipmentState {
    fn frame_query(
        &self,
        _name: &str,
        _request: &[u8],
        _c: &OwnerBridgeConfig,
    ) -> owners::Result<Vec<u8>> {
        if self.equipment.armor.len() > MAX {
            return Err(owners::Error::Limit);
        }
        encode(
            &Facts::Equipment(self.equipment.clone()),
            wand::MAX_RESPONSE_BYTES,
        )
    }
    fn frame_prepare(
        &self,
        name: &str,
        _request: &[u8],
        obs: &[Observation],
        _c: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        if _c.dressing_ticks && super::dressing_ticks::tagged(_request) {
            return super::dressing_ticks::prepare_equipment(self, name, _request, obs);
        }
        if name != WRITE_EQ {
            return Err(owners::Error::Unsupported);
        }
        let (player, helpless, _) = hero(&obs[0])?;
        let eq = law::reconcile(&self.equipment, &items(&obs[1])?, player);
        if helpless > 0 || eq.dressing.is_some() {
            return Err(owners::Error::Unsupported);
        }
        let mut next = self.clone();
        next.equipment = eq;
        Ok(Proposal {
            next,
            events: vec![],
            reply: encode(&(), wand::MAX_RESPONSE_BYTES)?,
        })
    }
}
impl FrameDomain for ActorsState {
    fn frame_query(
        &self,
        name: &str,
        request: &[u8],
        _c: &OwnerBridgeConfig,
    ) -> owners::Result<Vec<u8>> {
        if name != ACTOR {
            return Err(owners::Error::Unsupported);
        }
        let player: ActorId = decode(request, wand::MAX_REQUEST_BYTES)?;
        encode(
            &Facts::Actor(self.actors.get(player).map(|a| a.is_dead)),
            wand::MAX_RESPONSE_BYTES,
        )
    }
    fn frame_prepare(
        &self,
        name: &str,
        request: &[u8],
        obs: &[Observation],
        _c: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        if _c.dressing_ticks && super::dressing_ticks::tagged(request) {
            return super::dressing_ticks::prepare_actors(self, name, request, obs);
        }
        if name != WRITE_AC {
            return Err(owners::Error::Unsupported);
        }
        let (player, helpless, protection) = hero(&obs[0])?;
        let eq: Equipment = decode(request, wand::MAX_REQUEST_BYTES)?;
        if eq.armor.len() > MAX || helpless > 0 || eq.dressing.is_some() {
            return Err(owners::Error::Unsupported);
        }
        let ac = law::armor_class(&eq, &items(&obs[1])?, player, protection);
        let mut next = self.clone();
        let actor = next.actors.get_mut(player).ok_or(owners::Error::Stale)?;
        if actor.is_dead {
            return Err(owners::Error::Stale);
        }
        actor.ac = ac;
        Ok(Proposal {
            next,
            events: vec![],
            reply: encode(&(), wand::MAX_RESPONSE_BYTES)?,
        })
    }
}
impl FrameDomain for SessionState {
    fn frame_prepare(
        &self,
        name: &str,
        _request: &[u8],
        _obs: &[Observation],
        _c: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        if _c.dressing_ticks
            && serde_json::from_slice::<serde_json::Value>(_request)
                .ok()
                .is_some_and(|v| v.is_array())
        {
            return super::phase_support::prepare_record(self, _request, _obs);
        }
        if name != RECORD {
            return Err(owners::Error::Unsupported);
        }
        let mut next = self.clone();
        record(&mut next);
        Ok(Proposal {
            next,
            events: vec![],
            reply: encode(&(), wand::MAX_RESPONSE_BYTES)?,
        })
    }
}
impl FrameDomain for GeographyState {}
impl FrameDomain for ClockState {}
impl FrameDomain for RandomState {}
pub(super) fn add_ports(
    name: &str,
    mut ports: BTreeMap<String, Port>,
    ids: &BTreeMap<&str, owners::OwnerId>,
    c: &OwnerBridgeConfig,
) -> owners::Result<BTreeMap<String, Port>> {
    if !c.default_commands {
        return Ok(ports);
    }
    for (owner, endpoint, access) in ROUTES {
        if owner == name {
            let deps = if endpoint == WRITE_EQ || endpoint == WRITE_AC {
                vec![("Hero", HERO, 4), ("Objects", ITEMS, 2)]
            } else {
                vec![]
            };
            let dependencies = deps
                .into_iter()
                .map(|(owner, query, tag)| {
                    Ok(Dependency {
                        owner: *ids.get(owner).ok_or(owners::Error::Schema)?,
                        query: query.into(),
                        schema: [c.schema_tag(tag); 32],
                    })
                })
                .collect::<owners::Result<_>>()?;
            ports.insert(endpoint.into(), port(access, dependencies));
        }
    }
    Ok(ports)
}
pub(super) fn validate_ports(
    name: &str,
    p: &BTreeMap<String, Port>,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if !c.default_commands {
        return Ok(());
    }
    let expected = ROUTES
        .iter()
        .filter(|(owner, _, _)| *owner == name)
        .collect::<Vec<_>>();
    if p.keys().filter(|n| n.starts_with("frame_")).count() != expected.len() {
        return Err(owners::Error::Schema);
    }
    for (_, n, a) in expected {
        let port = p.get(*n).ok_or(owners::Error::Schema)?;
        let deps = if *n == WRITE_EQ || *n == WRITE_AC {
            vec![(HERO, 4), (ITEMS, 2)]
        } else {
            vec![]
        };
        if port.major != 1
            || port.access != *a
            || port.dependencies.len() != deps.len()
            || port
                .dependencies
                .iter()
                .zip(deps)
                .any(|(d, (q, t))| d.query != q || d.schema != [c.schema_tag(t); 32])
        {
            return Err(owners::Error::Schema);
        }
    }
    Ok(())
}
pub(super) fn filtered_ports(ports: &BTreeMap<String, Port>) -> BTreeMap<String, Port> {
    ports
        .iter()
        .filter(|(n, _)| !n.starts_with("frame_"))
        .map(|(n, p)| (n.clone(), p.clone()))
        .collect()
}
pub(super) fn provider(mut m: Manifest, c: &OwnerBridgeConfig) -> Manifest {
    if c.default_commands {
        for (_, n, a) in ROUTES
            .into_iter()
            .chain([("", PREVIEW, Access::Read), ("", RUN, Access::Write)])
        {
            m.exports.insert(n.into(), ep(a));
        }
    }
    m
}
fn ep(a: Access) -> Endpoint {
    Endpoint {
        max_request: wand::MAX_REQUEST_BYTES as u32,
        max_reply: wand::MAX_RESPONSE_BYTES as u32,
        ..endpoint(a)
    }
}
pub(super) fn caller(mut m: Manifest, c: &OwnerBridgeConfig) -> Manifest {
    if c.default_commands {
        for (_, n, a) in ROUTES
            .into_iter()
            .chain([("", PREVIEW, Access::Read), ("", RUN, Access::Write)])
        {
            m.dependencies.push(kernel::Dependency {
                capability: profile_capability(c, n),
                access: a,
            });
        }
        m.exports.insert(PREVIEW.into(), ep(Access::Read));
        m.exports.insert(RUN.into(), ep(Access::Write));
    }
    m
}
pub(super) fn caller_ports(
    mut p: BTreeMap<String, NativePort>,
    c: &OwnerBridgeConfig,
) -> BTreeMap<String, NativePort> {
    if c.default_commands {
        let m = profile_caller(c);
        for name in [PREVIEW, RUN] {
            let slot = m
                .dependencies
                .iter()
                .position(|d| d.capability.endpoint == name)
                .unwrap();
            p.insert(
                name.into(),
                Rc::new(move |request, view| view.call_dependency(slot, request)),
            );
        }
    }
    p
}
pub(super) fn grant_names(c: &OwnerBridgeConfig) -> BTreeSet<String> {
    if !c.default_commands {
        return BTreeSet::new();
    }
    ROUTES
        .into_iter()
        .map(|(owner, n, _)| {
            let id = match owner {
                "Objects" => 4,
                "Hero" => 2,
                "Equipment" => 6,
                "Actors" => 5,
                "Session" => 3,
                _ => 0,
            };
            format!("{id}.{n}")
        })
        .collect()
}
pub(super) fn resources(r: &SourceRoutes) -> owners::Result<Vec<Resource>> {
    ROUTES
        .into_iter()
        .map(|(owner, n, _)| match owner {
            "Objects" => r.objects.resource(n),
            "Hero" => r.hero.resource(n),
            "Equipment" => r.equipment.resource(n),
            "Actors" => r.actors.resource(n),
            "Session" => r.session.resource(n),
            _ => Err(owners::Error::Schema),
        })
        .collect()
}
struct Snapshot {
    obs: Vec<Observation>,
    next: Equipment,
    noop: bool,
}
fn snapshot(
    r: &SourceRoutes,
    view: &CallerView,
    c: &OwnerBridgeConfig,
) -> owners::Result<Snapshot> {
    let h = r.hero.query(HERO, &[], &[], view)?;
    let (player, helpless, _) = hero(&h)?;
    let a = r
        .actors
        .query(ACTOR, &encode(&player, wand::MAX_REQUEST_BYTES)?, &[], view)?;
    if matches!(fact(&a)?, Facts::Actor(None | Some(true))) {
        return Ok(Snapshot {
            obs: vec![h, a],
            next: Equipment::default(),
            noop: true,
        });
    }
    let e = r.equipment.query(EQ, &[], &[], view)?;
    let Facts::Equipment(eq) = fact(&e)? else {
        return Err(owners::Error::Schema);
    };
    let mut ids = eq.armor.iter().map(|(_, id)| *id).collect::<Vec<_>>();
    if let Some(d) = &eq.dressing {
        ids.push(d.item);
    }
    ids.sort();
    ids.dedup();
    if ids.len() > MAX {
        return Err(owners::Error::Limit);
    }
    let o = r
        .objects
        .query(ITEMS, &encode(&ids, wand::MAX_REQUEST_BYTES)?, &[], view)?;
    let next = law::reconcile(&eq, &items(&o)?, player);
    if helpless > 0 || next.dressing.is_some() {
        return Err(owners::Error::Unsupported);
    }
    let _ = c;
    Ok(Snapshot {
        obs: vec![h, a, e, o],
        next,
        noop: false,
    })
}
struct FrameAccounting;
impl Accounting for FrameAccounting {
    fn validate(&self, p: &[ProposalView], obs: &[Observation]) -> owners::Result<()> {
        if p.iter().any(|p| !p.events.is_empty() || p.reply != b"null") {
            return Err(owners::Error::Invariant);
        }
        if p.len() == 1 && p[0].receipt.command == RECORD {
            let mut before: SessionState = decode(&p[0].before, PAYLOAD)?;
            record(&mut before);
            if encode(&before, PAYLOAD)? != p[0].proposed {
                return Err(owners::Error::Invariant);
            }
            return Ok(());
        }
        if p.len() != 2 || obs.len() != 4 {
            return Err(owners::Error::Invariant);
        }
        let equipment_view = p
            .iter()
            .find(|p| p.receipt.command == WRITE_EQ)
            .ok_or(owners::Error::Invariant)?;
        let actor_view = p
            .iter()
            .find(|p| p.receipt.command == WRITE_AC)
            .ok_or(owners::Error::Invariant)?;
        let (player, helpless, protection) = hero(&obs[0])?;
        let Facts::Equipment(original) = fact(&obs[2])? else {
            return Err(owners::Error::Schema);
        };
        let expected = law::reconcile(&original, &items(&obs[3])?, player);
        if helpless > 0 || expected.dressing.is_some() {
            return Err(owners::Error::Unsupported);
        }
        let mut equipment: EquipmentState = decode(&equipment_view.before, PAYLOAD)?;
        if equipment.equipment != original {
            return Err(owners::Error::Stale);
        }
        equipment.equipment = expected.clone();
        let mut actors: ActorsState = decode(&actor_view.before, PAYLOAD)?;
        let a = actors.actors.get_mut(player).ok_or(owners::Error::Stale)?;
        if a.is_dead {
            return Err(owners::Error::Stale);
        }
        a.ac = law::armor_class(&expected, &items(&obs[3])?, player, protection);
        if encode(&equipment, PAYLOAD)? != equipment_view.proposed
            || encode(&actors, PAYLOAD)? != actor_view.proposed
        {
            return Err(owners::Error::Invariant);
        }
        Ok(())
    }
}
fn identity() -> op::ArtifactIdentity {
    artifact("native-default-frame-accounting-i1", 181)
}
pub(super) fn capture(
    a: &mut op::AccountingCaptureCatalog,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if c.default_commands {
        a.register(identity(), POLICY, Rc::new(FrameAccounting))?;
    }
    Ok(())
}
pub(super) fn restore(
    a: &mut op::AccountingRestoreCatalog,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if c.default_commands {
        a.allow(identity(), POLICY, || Ok(Rc::new(FrameAccounting)))?;
    }
    Ok(())
}
pub(super) fn native(
    mut p: BTreeMap<String, NativePort>,
    routes: Rc<Routes>,
    c: &OwnerBridgeConfig,
) -> BTreeMap<String, NativePort> {
    if c.default_commands {
        for (_, name, _) in ROUTES {
            p.insert(
                name.into(),
                Rc::new(|_, _| Err(kernel::Error::NativeRefusal)),
            );
        }
        let rr = routes.clone();
        let cc = c.clone();
        p.insert(
            PREVIEW.into(),
            Rc::new(move |request, view| {
                if cc.dressing_ticks && super::dressing_ticks::tagged(request) {
                    return super::dressing_ticks::preview(&rr.get()?, view)
                        .map_err(native_refusal);
                }
                let s = snapshot(&rr.get()?, view, &cc).map_err(native_refusal)?;
                encode(
                    &DefaultRequest {
                        guards: s
                            .obs
                            .iter()
                            .map(|o| OwnerWitness::from(o.stamp()))
                            .collect(),
                    },
                    wand::MAX_RESPONSE_BYTES,
                )
                .map_err(native_refusal)
            }),
        );
        let cc = c.clone();
        p.insert(
            RUN.into(),
            Rc::new(move |request, view| {
                if cc.dressing_ticks && super::dressing_ticks::tagged(request) {
                    return super::dressing_ticks::execute(&routes.get()?, request, view, &cc)
                        .map_err(native_refusal);
                }
                execute(&routes.get()?, request, view, &cc).map_err(native_refusal)
            }),
        );
    }
    p
}
fn execute(
    r: &SourceRoutes,
    request: &[u8],
    view: &CallerView,
    c: &OwnerBridgeConfig,
) -> owners::Result<Vec<u8>> {
    let req: DefaultRequest = decode(request, wand::MAX_REQUEST_BYTES)?;
    let s = snapshot(r, view, c)?;
    if req.guards
        != s.obs
            .iter()
            .map(|o| OwnerWitness::from(o.stamp()))
            .collect::<Vec<_>>()
    {
        return Err(owners::Error::Stale);
    }
    if s.noop {
        return encode(
            &DefaultReply {
                events: vec![],
                error: None,
            },
            wand::MAX_RESPONSE_BYTES,
        );
    }
    let deps = vec![s.obs[0].clone(), s.obs[3].clone()];
    let e = r
        .equipment
        .prepare(WRITE_EQ, &[], deps.clone(), view)?
        .erase();
    let a = r
        .actors
        .prepare(
            WRITE_AC,
            &encode(&s.next, wand::MAX_REQUEST_BYTES)?,
            deps,
            view,
        )?
        .erase();
    let ids = vec![r.equipment.id(), r.actors.id()];
    let batch = r.host.batch(
        vec![e, a],
        s.obs,
        &ids,
        ids.clone(),
        POLICY,
        Rc::new(FrameAccounting),
        profile_capability(c, RUN),
        view,
    )?;
    batch.commit(view)?;
    #[cfg(test)]
    {
        let states = (r.equipment.inspect()?, r.actors.inspect()?);
        let history = r.host.history()?;
        assert_eq!(batch.commit(view), Err(owners::Error::Stale));
        assert_eq!(states, (r.equipment.inspect()?, r.actors.inspect()?));
        assert_eq!(history, r.host.history()?);
    }
    let events = vec![message()];
    let publication = (|| {
        let candidate = r.session.prepare(RECORD, &[], vec![], view)?.erase();
        let ids = vec![r.session.id()];
        let b = r.host.batch(
            vec![candidate],
            vec![],
            &ids,
            ids.clone(),
            POLICY,
            Rc::new(FrameAccounting),
            profile_capability(c, RUN),
            view,
        )?;
        b.commit(view)?;
        Ok::<_, owners::Error>(())
    })();
    encode(
        &DefaultReply {
            events,
            error: publication.err().map(|e| format!("{e:?}")),
        },
        wand::MAX_RESPONSE_BYTES,
    )
}
impl OwnerBridge {
    pub(super) fn frame_call<T: Serialize>(
        &self,
        name: &str,
        request: &T,
        scope: Scope,
    ) -> Result<Vec<u8>, OwnerBridgeError> {
        let request = encode(request, wand::MAX_REQUEST_BYTES)?;
        let response = self.session.with_runtime(&self.credential.borrow(), |b| {
            let caller = bridge_caller(b, &self.config)?;
            if !b.kernel()?.manifest(caller)?.exports.contains_key(name) {
                return Err(owners::Error::Unsupported.into());
            }
            let cleanup = b.host().cleanup_identity()?;
            let (lease, operation) = b.update_kernel(|k| {
                let lease = k.capture_pipeline(
                    caller,
                    name,
                    Authority {
                        access: if name == PREVIEW
                            || name == super::passive_ticks::PREVIEW
                            || name == super::metabolic_ticks::PREVIEW
                            || name == super::regeneration_ticks::PREVIEW
                        {
                            Access::Read
                        } else {
                            Access::Write
                        },
                        scope: scope.clone(),
                    },
                )?;
                let id = k.issue_operation()?;
                let operation = k.operation_binding_with_cleanup(id, lease, 1, name, &[cleanup])?;
                Ok::<_, kernel::Error>((lease, operation))
            })??;
            let response = b.kernel()?.invoke_operation(
                &operation,
                &request,
                Authority {
                    access: if name == PREVIEW
                        || name == super::passive_ticks::PREVIEW
                        || name == super::metabolic_ticks::PREVIEW
                        || name == super::regeneration_ticks::PREVIEW
                    {
                        Access::Read
                    } else {
                        Access::Write
                    },
                    scope: scope.clone(),
                },
            );
            b.update_kernel(|k| k.release_pipeline(lease))??;
            Ok::<_, OwnerBridgeError>(response?)
        })?;
        self.session
            .replace_configuration(&self.key, restore_configuration(&self.config)?)?;
        self.credential
            .replace(self.session.issue_credential(&self.key)?);
        response
    }
    pub fn prepare_default_cancel(&self) -> Result<DefaultRequest, OwnerBridgeError> {
        decode(
            &self.frame_call(PREVIEW, &(), Scope::All)?,
            wand::MAX_RESPONSE_BYTES,
        )
        .map_err(Into::into)
    }
    pub fn execute_default_cancel(
        &self,
        request: DefaultRequest,
    ) -> Result<DefaultReply, OwnerBridgeError> {
        self.execute_default_cancel_scoped(request, Scope::All)
    }
    /// Apply the host delegation ceiling to a prepared cancellation. A refused
    /// Session publication leaves the reported equipment/AC prefix committed.
    pub fn execute_default_cancel_scoped(
        &self,
        request: DefaultRequest,
        scope: Scope,
    ) -> Result<DefaultReply, OwnerBridgeError> {
        decode(
            &self.frame_call(RUN, &request, scope)?,
            wand::MAX_RESPONSE_BYTES,
        )
        .map_err(Into::into)
    }
    pub fn dispatch_default_action(
        &self,
        action: nethacked_core::ActionAst,
    ) -> Result<DefaultReply, OwnerBridgeError> {
        match action {
            nethacked_core::ActionAst::CancelZapSelection => {
                self.execute_default_cancel(self.prepare_default_cancel()?)
            }
            nethacked_core::ActionAst::Wait => {
                if !self.config.wait_action {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply = self.step_wait()?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::Move(dir) => {
                if !self.config.movement_actions {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply =
                    self.step_movement(super::movement_actions::MovementAction::Move(dir))?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::OpenDoor(coord) => {
                if !self.config.movement_actions {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply =
                    self.step_movement(super::movement_actions::MovementAction::OpenDoor(coord))?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::CloseDoor(coord) => {
                if !self.config.movement_actions {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply =
                    self.step_movement(super::movement_actions::MovementAction::CloseDoor(coord))?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::Search => {
                if !self.config.movement_actions {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply = self.step_movement(super::movement_actions::MovementAction::Search)?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::Untrap(coord) => {
                if !self.config.movement_actions {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply =
                    self.step_movement(super::movement_actions::MovementAction::Untrap(coord))?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::Kick(coord) => {
                if !self.config.movement_actions {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply =
                    self.step_movement(super::movement_actions::MovementAction::Kick(coord))?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::MeleeAttack(coord) => {
                if !self.config.movement_actions {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply = self
                    .step_movement(super::movement_actions::MovementAction::MeleeAttack(coord))?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::PickUp => {
                if !self.config.inventory_actions {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply =
                    self.step_inventory(super::inventory_actions::InventoryAction::PickUp)?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::Drop(idx) => {
                if !self.config.inventory_actions {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply =
                    self.step_inventory(super::inventory_actions::InventoryAction::Drop(idx))?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::Wield(idx) => {
                if !self.config.inventory_actions {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply =
                    self.step_inventory(super::inventory_actions::InventoryAction::Wield(idx))?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::Wear(idx) => {
                if !self.config.inventory_actions {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply =
                    self.step_inventory(super::inventory_actions::InventoryAction::Wear(idx))?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::Remove(idx) => {
                if !self.config.inventory_actions {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply =
                    self.step_inventory(super::inventory_actions::InventoryAction::Remove(idx))?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::PutInContainer {
                item_index,
                container_index,
            } => {
                if !self.config.inventory_actions {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply = self.step_inventory(
                    super::inventory_actions::InventoryAction::PutInContainer {
                        item_index,
                        container_index,
                    },
                )?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::TakeFromContainer {
                container_index,
                item_index,
            } => {
                if !self.config.inventory_actions {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply = self.step_inventory(
                    super::inventory_actions::InventoryAction::TakeFromContainer {
                        container_index,
                        item_index,
                    },
                )?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::Quiver(idx) => {
                if !self.config.inventory_actions {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply =
                    self.step_inventory(super::inventory_actions::InventoryAction::Quiver(idx))?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::Fire(dir) => {
                if !self.config.inventory_actions {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply =
                    self.step_inventory(super::inventory_actions::InventoryAction::Fire(dir))?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::Dip {
                item_index,
                into_water,
            } => {
                if !self.config.item_effects {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply = self.step_item_effect(super::item_effects::ItemEffectAction::Dip {
                    item_index,
                    into_water,
                })?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::Quaff(idx) => {
                if !self.config.item_effects {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply =
                    self.step_item_effect(super::item_effects::ItemEffectAction::Quaff(idx))?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::Read(idx) => {
                if !self.config.item_effects {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply =
                    self.step_item_effect(super::item_effects::ItemEffectAction::Read(idx))?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::Eat(idx) => {
                if !self.config.item_effects {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply =
                    self.step_item_effect(super::item_effects::ItemEffectAction::Eat(idx))?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::Rub(idx) => {
                if !self.config.item_effects {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply =
                    self.step_item_effect(super::item_effects::ItemEffectAction::Rub(idx))?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::Apply(idx) => {
                if !self.config.item_effects {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply =
                    self.step_item_effect(super::item_effects::ItemEffectAction::Apply(idx))?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::Engrave { text, medium } => {
                if !self.config.item_effects {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply =
                    self.step_item_effect(super::item_effects::ItemEffectAction::Engrave {
                        text,
                        medium,
                    })?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::ZapWand { dir, energy } => {
                if !self.config.item_effects {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply =
                    self.step_item_effect(super::item_effects::ItemEffectAction::ZapWand {
                        dir,
                        energy,
                    })?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::ZapSelectedWand {
                item_index,
                expected_item,
                dir,
                energy,
            } => {
                if !self.config.item_effects {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply = self.step_item_effect(
                    super::item_effects::ItemEffectAction::ZapSelectedWand {
                        item_index,
                        expected_item,
                        dir,
                        energy,
                    },
                )?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::Cast { spell_index, dir } => {
                if !self.config.services {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply =
                    self.step_service(super::services::ServiceAction::Cast { spell_index, dir })?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::Pray => {
                if !self.config.services {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply = self.step_service(super::services::ServiceAction::Pray)?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::Sacrifice(idx) => {
                if !self.config.services {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply = self.step_service(super::services::ServiceAction::Sacrifice(idx))?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::Donate(amount) => {
                if !self.config.services {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply = self.step_service(super::services::ServiceAction::Donate(amount))?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::Pay => {
                if !self.config.services {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply = self.step_service(super::services::ServiceAction::Pay)?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::PriceCheck(item_index) => {
                if !self.config.services {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply =
                    self.step_service(super::services::ServiceAction::PriceCheck(item_index))?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::Wish(wish) => {
                if !self.config.services {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply = self.step_service(super::services::ServiceAction::Wish(wish))?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::Mount(actor_id) => {
                if !self.config.services {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply = self.step_service(super::services::ServiceAction::Mount(actor_id))?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::Dismount => {
                if !self.config.services {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply = self.step_service(super::services::ServiceAction::Dismount)?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::Ascend => {
                if !self.config.traversal {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply = self.step_traversal(super::traversal::TraversalAction::Ascend)?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
            nethacked_core::ActionAst::Descend => {
                if !self.config.traversal {
                    return Err(owners::Error::Unsupported.into());
                }
                let reply = self.step_traversal(super::traversal::TraversalAction::Descend)?;
                Ok(DefaultReply {
                    events: reply.events,
                    error: None,
                })
            }
        }
    }
}
