//! Explicit current-Rust raw metabolism; no scheduling or regeneration.
use super::phase_support::Metadata;
use super::*;
use crate::passive_metabolism as law;
use nethacked_arena::ActorId;
use nethacked_types::Locale;
use wand::OwnerWitness;
pub(super) const PREVIEW: &str = "metabolic_preview_i2b";
const RUN: &str = "metabolic_tick_i2b";
const HERO: &str = "metabolic_hero_i2b";
const ACTOR: &str = "metabolic_actor_i2b";
const CLOCK: &str = "metabolic_clock_i2b";
const LOCALE: &str = "metabolic_locale_i2b";
const WRITE_HERO: &str = "metabolic_nutrition_i2b";
const WRITE_ACTOR: &str = "metabolic_consequence_i2b";
const RECORD: &str = "metabolic_record_i2b";
const POLICY: [u8; 32] = [131; 32];
const ROUTES: [(&str, &str, Access); 7] = [
    ("Hero", HERO, Access::Read),
    ("Actors", ACTOR, Access::Read),
    ("Clock", CLOCK, Access::Read),
    ("Session", LOCALE, Access::Read),
    ("Hero", WRITE_HERO, Access::Write),
    ("Actors", WRITE_ACTOR, Access::Write),
    ("Session", RECORD, Access::Write),
];
const META: Metadata = Metadata {
    prefix: "metabolic_",
    routes: &ROUTES,
    preview: PREVIEW,
    run: RUN,
    dependent_write: WRITE_ACTOR,
    dependencies: &[("Hero", HERO, 4), ("Session", LOCALE, 9)],
};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetabolicRequest {
    guards: Vec<OwnerWitness>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetabolicReply {
    pub events: Vec<crate::GameEvent>,
    pub error: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
enum Facts {
    Hero(ActorId, i32),
    Actor(Option<law::Actor>),
    Turn(u64),
    Locale(Locale),
}
fn fact(o: &Observation) -> owners::Result<Facts> {
    decode(o.bytes(), wand::MAX_RESPONSE_BYTES)
}
fn hero(o: &Observation) -> owners::Result<(ActorId, i32)> {
    match fact(o)? {
        Facts::Hero(id, n) => Ok((id, n)),
        _ => Err(owners::Error::Schema),
    }
}
fn outcome(obs: &[Observation]) -> owners::Result<(ActorId, law::Outcome)> {
    if obs.len() != 4 {
        return Err(owners::Error::Schema);
    }
    let (id, n) = hero(&obs[0])?;
    let Facts::Actor(actor) = fact(&obs[1])? else {
        return Err(owners::Error::Schema);
    };
    let Facts::Turn(turn) = fact(&obs[2])? else {
        return Err(owners::Error::Schema);
    };
    let Facts::Locale(locale) = fact(&obs[3])? else {
        return Err(owners::Error::Schema);
    };
    Ok((id, law::tick(n, actor, turn, locale)))
}
pub(super) trait MetabolicDomain: Clone {
    fn metabolic_query(
        &self,
        _name: &str,
        _request: &[u8],
        _c: &OwnerBridgeConfig,
    ) -> owners::Result<Vec<u8>> {
        Err(owners::Error::Unsupported)
    }
    fn metabolic_prepare(
        &self,
        _name: &str,
        _request: &[u8],
        _obs: &[Observation],
        _c: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        Err(owners::Error::Unsupported)
    }
}
impl MetabolicDomain for HeroState {
    fn metabolic_query(
        &self,
        name: &str,
        request: &[u8],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Vec<u8>> {
        if name != HERO || !request.is_empty() {
            return Err(owners::Error::Unsupported);
        }
        encode(
            &Facts::Hero(self.player_id, self.player_nutrition),
            wand::MAX_RESPONSE_BYTES,
        )
    }
    fn metabolic_prepare(
        &self,
        name: &str,
        request: &[u8],
        obs: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        if name != WRITE_HERO || !request.is_empty() || !obs.is_empty() {
            return Err(owners::Error::Unsupported);
        }
        let mut next = self.clone();
        next.player_nutrition = nethacked_core::nutrition::metabolic_tick(self.player_nutrition);
        Ok(Proposal {
            next,
            events: vec![],
            reply: b"null".to_vec(),
        })
    }
}
impl MetabolicDomain for ActorsState {
    fn metabolic_query(
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
            &Facts::Actor(self.actors.get(id).map(|a| law::Actor {
                hp: a.hp,
                dead: a.is_dead,
            })),
            wand::MAX_RESPONSE_BYTES,
        )
    }
    fn metabolic_prepare(
        &self,
        name: &str,
        request: &[u8],
        obs: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        if name != WRITE_ACTOR || obs.len() != 2 {
            return Err(owners::Error::Unsupported);
        }
        let (id, n) = hero(&obs[0])?;
        let Facts::Locale(locale) = fact(&obs[1])? else {
            return Err(owners::Error::Schema);
        };
        // Provisional argument is independently checked against genuine Clock
        // facts by the mandatory cross-owner accounting before publication.
        let turn: u64 = decode(request, wand::MAX_REQUEST_BYTES)?;
        let out = law::tick(
            n,
            self.actors.get(id).map(|a| law::Actor {
                hp: a.hp,
                dead: a.is_dead,
            }),
            turn,
            locale,
        );
        let mut next = self.clone();
        if let Some(status) = out.actor {
            if let Some(a) = next.actors.get_mut(id) {
                a.hp = status.hp;
                a.is_dead = status.dead;
            }
        }
        Ok(Proposal {
            next,
            events: out
                .events
                .iter()
                .map(|e| encode(e, wand::MAX_RESPONSE_BYTES))
                .collect::<owners::Result<_>>()?,
            reply: b"null".to_vec(),
        })
    }
}
impl MetabolicDomain for ClockState {
    fn metabolic_query(
        &self,
        name: &str,
        request: &[u8],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Vec<u8>> {
        if name != CLOCK || !request.is_empty() {
            return Err(owners::Error::Unsupported);
        }
        encode(&Facts::Turn(self.scheduler.turn), wand::MAX_RESPONSE_BYTES)
    }
}
impl MetabolicDomain for SessionState {
    fn metabolic_query(
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
    fn metabolic_prepare(
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
impl MetabolicDomain for ObjectsState {}
impl MetabolicDomain for EquipmentState {}
impl MetabolicDomain for GeographyState {}
impl MetabolicDomain for RandomState {}
pub(super) fn add_ports(
    name: &str,
    p: BTreeMap<String, Port>,
    ids: &BTreeMap<&str, owners::OwnerId>,
    c: &OwnerBridgeConfig,
) -> owners::Result<BTreeMap<String, Port>> {
    META.add_ports(c.metabolic_ticks, name, p, ids, c)
}
pub(super) fn validate_ports(
    name: &str,
    p: &BTreeMap<String, Port>,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    META.validate_ports(c.metabolic_ticks, name, p, c)
}
pub(super) fn filtered_ports(p: &BTreeMap<String, Port>) -> BTreeMap<String, Port> {
    META.filtered_ports(p)
}
pub(super) fn provider(m: Manifest, c: &OwnerBridgeConfig) -> Manifest {
    META.provider(c.metabolic_ticks, m)
}
pub(super) fn caller(m: Manifest, c: &OwnerBridgeConfig) -> Manifest {
    META.caller(c.metabolic_ticks, m, c)
}
pub(super) fn caller_ports(
    p: BTreeMap<String, NativePort>,
    c: &OwnerBridgeConfig,
) -> BTreeMap<String, NativePort> {
    META.caller_ports(c.metabolic_ticks, p, c)
}
pub(super) fn grant_names(c: &OwnerBridgeConfig) -> BTreeSet<String> {
    META.grant_names(c.metabolic_ticks)
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
        r.clock.query(CLOCK, &[], &[], v)?,
        r.session.query(LOCALE, &[], &[], v)?,
    ])
}
struct PhaseAccounting;
impl Accounting for PhaseAccounting {
    fn validate(&self, p: &[ProposalView], obs: &[Observation]) -> owners::Result<()> {
        if p.len() != 2 || p.iter().any(|p| p.reply != b"null") {
            return Err(owners::Error::Invariant);
        }
        let (id, out) = outcome(obs)?;
        let h = p
            .iter()
            .find(|p| p.receipt.command == WRITE_HERO)
            .ok_or(owners::Error::Invariant)?;
        let a = p
            .iter()
            .find(|p| p.receipt.command == WRITE_ACTOR)
            .ok_or(owners::Error::Invariant)?;
        if !h.events.is_empty()
            || a.events
                != out
                    .events
                    .iter()
                    .map(|e| encode(e, wand::MAX_RESPONSE_BYTES))
                    .collect::<owners::Result<Vec<_>>>()?
        {
            return Err(owners::Error::Invariant);
        }
        let mut hs: HeroState = decode(&h.before, PAYLOAD)?;
        let (oid, n) = hero(&obs[0])?;
        if hs.player_id != oid || hs.player_nutrition != n {
            return Err(owners::Error::Stale);
        }
        hs.player_nutrition = out.nutrition;
        let mut actors: ActorsState = decode(&a.before, PAYLOAD)?;
        let Facts::Actor(prior) = fact(&obs[1])? else {
            return Err(owners::Error::Schema);
        };
        if actors.actors.get(id).map(|a| law::Actor {
            hp: a.hp,
            dead: a.is_dead,
        }) != prior
        {
            return Err(owners::Error::Stale);
        }
        if let Some(status) = out.actor {
            if let Some(a) = actors.actors.get_mut(id) {
                a.hp = status.hp;
                a.is_dead = status.dead;
            }
        }
        if encode(&hs, PAYLOAD)? != h.proposed || encode(&actors, PAYLOAD)? != a.proposed {
            return Err(owners::Error::Invariant);
        }
        Ok(())
    }
}
fn identity() -> op::ArtifactIdentity {
    artifact("native-metabolic-accounting-i2b", 131)
}
pub(super) fn capture(
    a: &mut op::AccountingCaptureCatalog,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if c.metabolic_ticks {
        a.register(identity(), POLICY, Rc::new(PhaseAccounting))?;
    }
    Ok(())
}
pub(super) fn restore(
    a: &mut op::AccountingRestoreCatalog,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if c.metabolic_ticks {
        a.allow(identity(), POLICY, || Ok(Rc::new(PhaseAccounting)))?;
    }
    Ok(())
}
pub(super) fn native(
    mut p: BTreeMap<String, NativePort>,
    r: Rc<Routes>,
    c: &OwnerBridgeConfig,
) -> BTreeMap<String, NativePort> {
    if c.metabolic_ticks {
        for (_, n, _) in ROUTES {
            p.insert(n.into(), Rc::new(|_, _| Err(kernel::Error::NativeRefusal)));
        }
        let rr = r.clone();
        p.insert(
            PREVIEW.into(),
            Rc::new(move |_, v| {
                let obs = snapshot(&rr.get()?, v).map_err(native_refusal)?;
                encode(
                    &MetabolicRequest {
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
    let req: MetabolicRequest = decode(request, wand::MAX_REQUEST_BYTES)?;
    let obs = snapshot(r, v)?;
    if req.guards
        != obs
            .iter()
            .map(|o| OwnerWitness::from(o.stamp()))
            .collect::<Vec<_>>()
    {
        return Err(owners::Error::Stale);
    }
    let (_, out) = outcome(&obs)?;
    let Facts::Turn(turn) = fact(&obs[2])? else {
        return Err(owners::Error::Schema);
    };
    #[cfg(test)]
    let Facts::Locale(probe_locale) = fact(&obs[3])?
    else {
        return Err(owners::Error::Schema);
    };
    #[cfg(test)]
    if turn%10==0 && out.events.iter().any(|e|matches!(e,crate::GameEvent::LogMessage{text} if text==nethacked_i18n::Messages::hunger_fainting(probe_locale))) {
        let before=(r.hero.inspect()?,r.actors.inspect()?,r.clock.inspect()?,r.session.inspect()?,r.host.history()?);
        let bad_h=r.hero.prepare(WRITE_HERO,&[],vec![],v)?.erase();
        let bad_a=r.actors.prepare(WRITE_ACTOR,&encode(&turn.wrapping_add(1),wand::MAX_REQUEST_BYTES)?,vec![obs[0].clone(),obs[3].clone()],v)?.erase();
        let ids=vec![r.hero.id(),r.actors.id()];
        let attempt=r.host.batch(vec![bad_h,bad_a],obs.clone(),&ids,ids.clone(),POLICY,Rc::new(PhaseAccounting),profile_capability(c,RUN),v);
        let result=match attempt {Ok(b)=>b.commit(v).map(|_|()),Err(e)=>Err(e)};
        assert_eq!(result,Err(owners::Error::Invariant));
        assert_eq!(before,(r.hero.inspect()?,r.actors.inspect()?,r.clock.inspect()?,r.session.inspect()?,r.host.history()?));
    }
    let h = r.hero.prepare(WRITE_HERO, &[], vec![], v)?.erase();
    let a = r
        .actors
        .prepare(
            WRITE_ACTOR,
            &encode(&turn, wand::MAX_REQUEST_BYTES)?,
            vec![obs[0].clone(), obs[3].clone()],
            v,
        )?
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
    #[cfg(test)]
    {
        let before = (
            r.hero.inspect()?,
            r.actors.inspect()?,
            r.clock.inspect()?,
            r.host.history()?,
        );
        assert_eq!(batch.commit(v), Err(owners::Error::Stale));
        assert_eq!(
            before,
            (
                r.hero.inspect()?,
                r.actors.inspect()?,
                r.clock.inspect()?,
                r.host.history()?
            )
        );
    }
    let committed_events = committed
        .events
        .iter()
        .map(|e| decode::<crate::GameEvent>(e, wand::MAX_RESPONSE_BYTES))
        .collect::<owners::Result<Vec<_>>>()?;
    if committed_events != out.events {
        return Err(owners::Error::Invariant);
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
        &MetabolicReply {
            events: out.events,
            error: publication.err().map(|e| format!("{e:?}")),
        },
        wand::MAX_RESPONSE_BYTES,
    )
}
impl OwnerBridge {
    pub fn prepare_metabolic_phase(&self) -> Result<MetabolicRequest, OwnerBridgeError> {
        decode(
            &self.frame_call(PREVIEW, &(), Scope::All)?,
            wand::MAX_RESPONSE_BYTES,
        )
        .map_err(Into::into)
    }
    pub fn execute_metabolic_phase(
        &self,
        r: MetabolicRequest,
    ) -> Result<MetabolicReply, OwnerBridgeError> {
        self.execute_metabolic_phase_scoped(r, Scope::All)
    }
    pub fn execute_metabolic_phase_scoped(
        &self,
        r: MetabolicRequest,
        scope: Scope,
    ) -> Result<MetabolicReply, OwnerBridgeError> {
        decode(&self.frame_call(RUN, &r, scope)?, wand::MAX_RESPONSE_BYTES).map_err(Into::into)
    }
}
