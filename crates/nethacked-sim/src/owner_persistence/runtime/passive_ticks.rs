//! Explicit raw affliction phase. No scheduling, Clock, or RNG advancement.
use super::*;
use crate::passive_afflictions as law;
use nethacked_arena::ActorId;
use nethacked_types::{AfflictionState, Locale};
use wand::OwnerWitness;
pub(super) const PREVIEW: &str = "passive_preview_i2a";
const RUN: &str = "passive_tick_i2a";
const HERO: &str = "passive_hero_i2a";
const ACTOR: &str = "passive_actor_i2a";
const LOCALE: &str = "passive_locale_i2a";
const WRITE_HERO: &str = "passive_countdown_i2a";
const WRITE_ACTOR: &str = "passive_status_i2a";
const RECORD: &str = "passive_record_i2a";
const POLICY: [u8; 32] = [151; 32];
const ROUTES: [(&str, &str, Access); 6] = [
    ("Hero", HERO, Access::Read),
    ("Actors", ACTOR, Access::Read),
    ("Session", LOCALE, Access::Read),
    ("Hero", WRITE_HERO, Access::Write),
    ("Actors", WRITE_ACTOR, Access::Write),
    ("Session", RECORD, Access::Write),
];
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AfflictionRequest {
    guards: Vec<OwnerWitness>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AfflictionReply {
    pub events: Vec<crate::GameEvent>,
    pub error: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
enum Facts {
    Hero(ActorId, AfflictionState),
    Actor(Option<(bool, bool)>),
    Locale(Locale),
}
fn fact(o: &Observation) -> owners::Result<Facts> {
    decode(o.bytes(), wand::MAX_RESPONSE_BYTES)
}
fn hero(o: &Observation) -> owners::Result<(ActorId, AfflictionState)> {
    match fact(o)? {
        Facts::Hero(id, s) => Ok((id, s)),
        _ => Err(owners::Error::Schema),
    }
}
fn outcome(
    obs: &[Observation],
) -> owners::Result<(ActorId, AfflictionState, bool, bool, Vec<crate::GameEvent>)> {
    if obs.len() != 3 {
        return Err(owners::Error::Schema);
    }
    let (id, mut state) = hero(&obs[0])?;
    if !matches!(fact(&obs[1])?, Facts::Actor(_)) {
        return Err(owners::Error::Schema);
    }
    let Facts::Locale(locale) = fact(&obs[2])? else {
        return Err(owners::Error::Schema);
    };
    let (result, clear, mut events) = law::recovery(&mut state);
    let death = law::fatal_message(&result, locale);
    let fatal = death.is_some();
    events.extend(death);
    Ok((id, state, clear, fatal, events))
}
pub(super) trait PassiveDomain: Clone {
    fn passive_query(
        &self,
        _name: &str,
        _request: &[u8],
        _c: &OwnerBridgeConfig,
    ) -> owners::Result<Vec<u8>> {
        Err(owners::Error::Unsupported)
    }
    fn passive_prepare(
        &self,
        _name: &str,
        _request: &[u8],
        _obs: &[Observation],
        _c: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        Err(owners::Error::Unsupported)
    }
}
impl PassiveDomain for HeroState {
    fn passive_query(
        &self,
        name: &str,
        request: &[u8],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Vec<u8>> {
        if name != HERO || !request.is_empty() {
            return Err(owners::Error::Unsupported);
        }
        encode(
            &Facts::Hero(self.player_id, self.hero.afflictions.clone()),
            wand::MAX_RESPONSE_BYTES,
        )
    }
    fn passive_prepare(
        &self,
        name: &str,
        request: &[u8],
        obs: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        if name != WRITE_HERO || !obs.is_empty() {
            return Err(owners::Error::Unsupported);
        }
        let locale: Locale = decode(request, wand::MAX_REQUEST_BYTES)?;
        let mut state = self.hero.afflictions.clone();
        let (result, _, mut events) = law::recovery(&mut state);
        events.extend(law::fatal_message(&result, locale));
        let mut next = self.clone();
        next.hero.afflictions = state;
        Ok(Proposal {
            next,
            events: events
                .iter()
                .map(|e| encode(e, wand::MAX_RESPONSE_BYTES))
                .collect::<owners::Result<_>>()?,
            reply: b"null".to_vec(),
        })
    }
}
impl PassiveDomain for ActorsState {
    fn passive_query(
        &self,
        name: &str,
        request: &[u8],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Vec<u8>> {
        if name != ACTOR {
            return Err(owners::Error::Unsupported);
        }
        let id: ActorId = decode(request, wand::MAX_REQUEST_BYTES)?;
        encode(
            &Facts::Actor(self.actors.get(id).map(|a| (a.is_dead, a.intrinsics.blind))),
            wand::MAX_RESPONSE_BYTES,
        )
    }
    fn passive_prepare(
        &self,
        name: &str,
        request: &[u8],
        obs: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        if name != WRITE_ACTOR || !request.is_empty() {
            return Err(owners::Error::Unsupported);
        }
        if obs.len() != 2 {
            return Err(owners::Error::Schema);
        }
        let (id, mut state) = hero(&obs[0])?;
        let Facts::Locale(locale) = fact(&obs[1])? else {
            return Err(owners::Error::Schema);
        };
        let (result, clear, _) = law::recovery(&mut state);
        let fatal = law::fatal_message(&result, locale).is_some();

        let mut next = self.clone();
        if let Some(a) = next.actors.get_mut(id) {
            if clear {
                a.intrinsics.blind = false;
            }
            if fatal {
                a.is_dead = true;
            }
        }
        Ok(Proposal {
            next,
            events: vec![],
            reply: b"null".to_vec(),
        })
    }
}
impl PassiveDomain for SessionState {
    fn passive_query(
        &self,
        name: &str,
        request: &[u8],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Vec<u8>> {
        if name != LOCALE || !request.is_empty() {
            return Err(owners::Error::Unsupported);
        }
        encode(&Facts::Locale(self.locale), wand::MAX_RESPONSE_BYTES)
    }
    fn passive_prepare(
        &self,
        name: &str,
        request: &[u8],
        obs: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        if name != RECORD {
            return Err(owners::Error::Unsupported);
        }
        super::phase_support::prepare_record(self, request, obs)
    }
}
impl PassiveDomain for ObjectsState {}
impl PassiveDomain for EquipmentState {}
impl PassiveDomain for GeographyState {}
impl PassiveDomain for ClockState {}
impl PassiveDomain for RandomState {}
const META: super::phase_support::Metadata = super::phase_support::Metadata {
    prefix: "passive_",
    routes: &ROUTES,
    preview: PREVIEW,
    run: RUN,
    dependent_write: WRITE_ACTOR,
    dependencies: &[("Hero", HERO, 4), ("Session", LOCALE, 9)],
};
pub(super) fn add_ports(
    name: &str,
    p: BTreeMap<String, Port>,
    ids: &BTreeMap<&str, owners::OwnerId>,
    c: &OwnerBridgeConfig,
) -> owners::Result<BTreeMap<String, Port>> {
    META.add_ports(c.passive_ticks, name, p, ids, c)
}
pub(super) fn validate_ports(
    name: &str,
    p: &BTreeMap<String, Port>,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    META.validate_ports(c.passive_ticks, name, p, c)
}
pub(super) fn filtered_ports(p: &BTreeMap<String, Port>) -> BTreeMap<String, Port> {
    META.filtered_ports(p)
}
pub(super) fn provider(m: Manifest, c: &OwnerBridgeConfig) -> Manifest {
    META.provider(c.passive_ticks, m)
}
pub(super) fn caller(m: Manifest, c: &OwnerBridgeConfig) -> Manifest {
    META.caller(c.passive_ticks, m, c)
}
pub(super) fn caller_ports(
    p: BTreeMap<String, NativePort>,
    c: &OwnerBridgeConfig,
) -> BTreeMap<String, NativePort> {
    META.caller_ports(c.passive_ticks, p, c)
}
pub(super) fn grant_names(c: &OwnerBridgeConfig) -> BTreeSet<String> {
    META.grant_names(c.passive_ticks)
}
pub(super) fn resources(r: &SourceRoutes) -> owners::Result<Vec<Resource>> {
    META.resources(r)
}
fn snapshot(r: &SourceRoutes, v: &CallerView) -> owners::Result<Vec<Observation>> {
    let h = r.hero.query(HERO, &[], &[], v)?;
    let (id, _) = hero(&h)?;
    Ok(vec![
        h,
        r.actors
            .query(ACTOR, &encode(&id, wand::MAX_REQUEST_BYTES)?, &[], v)?,
        r.session.query(LOCALE, &[], &[], v)?,
    ])
}
struct PhaseAccounting;
impl Accounting for PhaseAccounting {
    fn validate(&self, p: &[ProposalView], obs: &[Observation]) -> owners::Result<()> {
        let (id, state, clear, fatal, events) = outcome(obs)?;
        if p.iter().any(|p| p.reply != b"null") {
            return Err(owners::Error::Invariant);
        }
        if p.len() != 2 {
            return Err(owners::Error::Invariant);
        }
        let h = p
            .iter()
            .find(|x| x.receipt.command == WRITE_HERO)
            .ok_or(owners::Error::Invariant)?;
        let a = p
            .iter()
            .find(|x| x.receipt.command == WRITE_ACTOR)
            .ok_or(owners::Error::Invariant)?;
        let expected_events = events
            .iter()
            .map(|e| encode(e, wand::MAX_RESPONSE_BYTES))
            .collect::<owners::Result<Vec<_>>>()?;
        if h.events != expected_events || !a.events.is_empty() {
            return Err(owners::Error::Invariant);
        }
        let mut hs: HeroState = decode(&h.before, PAYLOAD)?;
        let (oid, original) = hero(&obs[0])?;
        if hs.player_id != oid
            || encode(&hs.hero.afflictions, wand::MAX_RESPONSE_BYTES)?
                != encode(&original, wand::MAX_RESPONSE_BYTES)?
        {
            return Err(owners::Error::Stale);
        }
        hs.hero.afflictions = state;
        let mut actors: ActorsState = decode(&a.before, PAYLOAD)?;
        let Facts::Actor(prior) = fact(&obs[1])? else {
            return Err(owners::Error::Schema);
        };
        if actors
            .actors
            .get(id)
            .map(|a| (a.is_dead, a.intrinsics.blind))
            != prior
        {
            return Err(owners::Error::Stale);
        }
        if let Some(a) = actors.actors.get_mut(id) {
            if clear {
                a.intrinsics.blind = false;
            }
            if fatal {
                a.is_dead = true;
            }
        }
        if encode(&hs, PAYLOAD)? != h.proposed || encode(&actors, PAYLOAD)? != a.proposed {
            return Err(owners::Error::Invariant);
        }
        Ok(())
    }
}
fn identity() -> op::ArtifactIdentity {
    artifact("native-passive-affliction-accounting-i2a", 151)
}
pub(super) fn capture(
    a: &mut op::AccountingCaptureCatalog,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if c.passive_ticks {
        a.register(identity(), POLICY, Rc::new(PhaseAccounting))?;
    }
    Ok(())
}
pub(super) fn restore(
    a: &mut op::AccountingRestoreCatalog,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if c.passive_ticks {
        a.allow(identity(), POLICY, || Ok(Rc::new(PhaseAccounting)))?;
    }
    Ok(())
}
pub(super) fn native(
    mut p: BTreeMap<String, NativePort>,
    r: Rc<Routes>,
    c: &OwnerBridgeConfig,
) -> BTreeMap<String, NativePort> {
    if c.passive_ticks {
        for (_, n, _) in ROUTES {
            p.insert(n.into(), Rc::new(|_, _| Err(kernel::Error::NativeRefusal)));
        }
        let rr = r.clone();
        p.insert(
            PREVIEW.into(),
            Rc::new(move |_, v| {
                let obs = snapshot(&rr.get()?, v).map_err(native_refusal)?;
                encode(
                    &AfflictionRequest {
                        guards: obs.iter().map(|o| OwnerWitness::from(o.stamp())).collect(),
                    },
                    wand::MAX_RESPONSE_BYTES,
                )
                .map_err(native_refusal)
            }),
        );
        let cc = c.clone();
        p.insert(
            RUN.into(),
            Rc::new(move |req, v| execute(&r.get()?, req, v, &cc).map_err(native_refusal)),
        );
    }
    p
}
fn execute(
    r: &SourceRoutes,
    request: &[u8],
    v: &CallerView,
    c: &OwnerBridgeConfig,
) -> owners::Result<Vec<u8>> {
    let req: AfflictionRequest = decode(request, wand::MAX_REQUEST_BYTES)?;
    let obs = snapshot(r, v)?;
    if req.guards
        != obs
            .iter()
            .map(|o| OwnerWitness::from(o.stamp()))
            .collect::<Vec<_>>()
    {
        return Err(owners::Error::Stale);
    }
    let (_, _, _, _, events) = outcome(&obs)?;
    let Facts::Locale(locale) = fact(&obs[2])? else {
        return Err(owners::Error::Schema);
    };
    let h = r
        .hero
        .prepare(
            WRITE_HERO,
            &encode(&locale, wand::MAX_REQUEST_BYTES)?,
            vec![],
            v,
        )?
        .erase();
    let a = r
        .actors
        .prepare(WRITE_ACTOR, &[], vec![obs[0].clone(), obs[2].clone()], v)?
        .erase();
    let ids = vec![r.hero.id(), r.actors.id()];
    let batch = r.host.batch(
        vec![h, a],
        obs.clone(),
        &ids,
        ids.clone(),
        POLICY,
        Rc::new(PhaseAccounting),
        profile_capability(c, RUN),
        v,
    )?;
    let committed = batch.commit(v)?;
    let committed_events = committed
        .events
        .iter()
        .map(|e| decode::<crate::GameEvent>(e, wand::MAX_RESPONSE_BYTES))
        .collect::<owners::Result<Vec<_>>>()?;
    if committed_events != events {
        return Err(owners::Error::Invariant);
    }
    #[cfg(test)]
    {
        let before = (r.hero.inspect()?, r.actors.inspect()?, r.host.history()?);
        assert_eq!(batch.commit(v), Err(owners::Error::Stale));
        assert_eq!(
            before,
            (r.hero.inspect()?, r.actors.inspect()?, r.host.history()?)
        );
    }
    let publication = super::phase_support::publish_session(
        r,
        &committed,
        RECORD,
        POLICY,
        profile_capability(c, RUN),
        v,
    );
    encode(
        &AfflictionReply {
            events,
            error: publication.err().map(|e| format!("{e:?}")),
        },
        wand::MAX_RESPONSE_BYTES,
    )
}
impl OwnerBridge {
    pub fn prepare_affliction_phase(&self) -> Result<AfflictionRequest, OwnerBridgeError> {
        decode(
            &self.frame_call(PREVIEW, &(), Scope::All)?,
            wand::MAX_RESPONSE_BYTES,
        )
        .map_err(Into::into)
    }
    pub fn execute_affliction_phase(
        &self,
        r: AfflictionRequest,
    ) -> Result<AfflictionReply, OwnerBridgeError> {
        self.execute_affliction_phase_scoped(r, Scope::All)
    }
    pub fn execute_affliction_phase_scoped(
        &self,
        r: AfflictionRequest,
        scope: Scope,
    ) -> Result<AfflictionReply, OwnerBridgeError> {
        decode(&self.frame_call(RUN, &r, scope)?, wand::MAX_RESPONSE_BYTES).map_err(Into::into)
    }
}
