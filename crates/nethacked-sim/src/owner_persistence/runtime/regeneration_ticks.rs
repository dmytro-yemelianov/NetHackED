//! Explicit natural HP phase with authenticated moving burden and one native RNG draw.
use super::phase_support::Metadata;
use super::*;
use crate::passive_regeneration as law;
#[cfg(test)]
use nethacked_arena::ItemId;
use nethacked_arena::{ActorId, ItemLocation};
use rand::Rng;
use wand::{CommandFacts, CommandQuery, OwnerWitness, WandFacts, WandQuery};

pub(super) const PREVIEW: &str = "regeneration_preview_i2c";
const RUN: &str = "regeneration_tick_i2c";
const HERO: &str = "regeneration_hero_i2c";
const OBJECTS: &str = "regeneration_objects_i2c";
pub(super) const RANDOM: &str = "regeneration_random_i2c";
const WRITE_ACTOR: &str = "regeneration_heal_i2c";
pub(super) const WRITE_RANDOM: &str = "regeneration_draw_i2c";
const POLICY: [u8; 32] = [132; 32];
#[cfg(test)]
thread_local! {
    static ACCOUNTING_REFUSALS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
#[cfg(test)]
pub(super) fn accounting_refusals() -> usize {
    ACCOUNTING_REFUSALS.with(std::cell::Cell::get)
}
#[cfg(test)]
type ObjectMutationState = (Vec<u8>, Vec<u8>, Vec<u8>, Vec<owners::CommitReceipt>);
#[cfg(test)]
thread_local! {
    static OBJECT_MUTATION: std::cell::Cell<Option<ItemId>> = const { std::cell::Cell::new(None) };
    static OBJECT_MUTATION_STATE: std::cell::RefCell<Option<ObjectMutationState>> = const { std::cell::RefCell::new(None) };
}
#[cfg(test)]
pub(super) fn mutate_object_on_run(id: ItemId) {
    OBJECT_MUTATION.with(|slot| slot.set(Some(id)));
    OBJECT_MUTATION_STATE.with(|slot| *slot.borrow_mut() = None);
}
#[cfg(test)]
pub(super) fn object_mutation_state() -> Option<ObjectMutationState> {
    OBJECT_MUTATION_STATE.with(|slot| slot.borrow().clone())
}
const ROUTES: [(&str, &str, Access); 5] = [
    ("Hero", HERO, Access::Read),
    ("Objects", OBJECTS, Access::Read),
    ("Random", RANDOM, Access::Read),
    ("Actors", WRITE_ACTOR, Access::Write),
    ("Random", WRITE_RANDOM, Access::Write),
];
const META: Metadata = Metadata {
    prefix: "regeneration_",
    routes: &ROUTES,
    preview: PREVIEW,
    run: RUN,
    dependent_write: WRITE_ACTOR,
    dependencies: &[("Hero", HERO, 4)],
};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegenerationRequest {
    pub moved: bool,
    guards: Vec<OwnerWitness>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegenerationReply {
    pub healed: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
enum Facts {
    Hero {
        player: ActorId,
        polymorphed: bool,
        strength: i32,
        con: i32,
    },
    Carried {
        player: ActorId,
        weight: u32,
        before: ObjectsState,
    },
    Draw(u32),
}
fn fact(o: &Observation) -> owners::Result<Facts> {
    decode(o.bytes(), wand::MAX_RESPONSE_BYTES)
}
fn hero(o: &Observation) -> owners::Result<(ActorId, bool, i32, i32)> {
    match fact(o)? {
        Facts::Hero {
            player,
            polymorphed,
            strength,
            con,
        } => Ok((player, polymorphed, strength, con)),
        _ => Err(owners::Error::Schema),
    }
}
fn actor(o: &Observation, player: ActorId) -> owners::Result<law::Actor> {
    match decode::<WandFacts>(o.bytes(), wand::MAX_RESPONSE_BYTES)? {
        WandFacts::Command(CommandFacts::Actor { target, actor }) if target == player => {
            Ok(law::Actor {
                hp: actor.hp,
                max_hp: actor.max_hp,
                level: actor.level,
                dead: actor.is_dead,
                levitating: actor.intrinsics.levitation,
            })
        }
        _ => Err(owners::Error::Schema),
    }
}
fn finite_weight(s: &ObjectsState, player: ActorId) -> owners::Result<u32> {
    let mut pending: Vec<_> = s
        .items
        .iter()
        .filter_map(|(id, item)| {
            (item.location == ItemLocation::CarriedBy(player)).then_some((id, 1usize))
        })
        .collect();
    let mut seen = Vec::new();
    while let Some((id, depth)) = pending.pop() {
        if depth > 32 || seen.len() >= 32 || seen.contains(&id) {
            return Err(owners::Error::Limit);
        }
        seen.push(id);
        for (child, item) in s.items.iter() {
            if item.location == ItemLocation::InContainer(id) {
                pending.push((child, depth + 1));
            }
        }
    }
    Ok(nethacked_arena::carried_weight(&s.items, player))
}
fn status(
    obs: &[Observation],
    moved: bool,
) -> owners::Result<(ActorId, law::Actor, i32, Option<u32>)> {
    if obs.len() < 2 {
        return Err(owners::Error::Schema);
    }
    let (id, poly, strength, con) = hero(&obs[0])?;
    let current = Some(actor(&obs[1], id)?);
    let weight = if moved {
        match obs.get(3).ok_or(owners::Error::Schema).and_then(fact)? {
            Facts::Carried { player, weight, .. } if player == id => Some(weight),
            _ => return Err(owners::Error::Schema),
        }
    } else {
        None
    };
    if !law::eligible(current, poly, moved, strength, con, weight) {
        return Err(owners::Error::Unsupported);
    }
    Ok((id, current.ok_or(owners::Error::Schema)?, con, weight))
}
fn random_fact(obs: &Observation) -> owners::Result<u32> {
    match fact(obs)? {
        Facts::Draw(draw) if draw < 100 => Ok(draw),
        _ => Err(owners::Error::Schema),
    }
}
pub(super) trait RegenerationDomain: Clone {
    fn regeneration_query(
        &self,
        _: &str,
        _: &[u8],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Vec<u8>> {
        Err(owners::Error::Unsupported)
    }
    fn regeneration_prepare(
        &self,
        _: &str,
        _: &[u8],
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        Err(owners::Error::Unsupported)
    }
}
impl RegenerationDomain for HeroState {
    fn regeneration_query(
        &self,
        name: &str,
        req: &[u8],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Vec<u8>> {
        if name != HERO || !req.is_empty() {
            return Err(owners::Error::Unsupported);
        }
        encode(
            &Facts::Hero {
                player: self.player_id,
                polymorphed: self.hero.polymorph.is_some(),
                strength: self.progress.attr_base[crate::progress::A_STR],
                con: self.progress.attr_base[crate::progress::A_CON],
            },
            wand::MAX_RESPONSE_BYTES,
        )
    }
}
impl RegenerationDomain for ActorsState {
    fn regeneration_prepare(
        &self,
        name: &str,
        req: &[u8],
        obs: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        if name != WRITE_ACTOR || obs.len() != 1 {
            return Err(owners::Error::Unsupported);
        }
        let (id, _, _, con) = hero(&obs[0])?;
        let draw: u32 = decode(req, wand::MAX_REQUEST_BYTES)?;
        if draw >= 100 {
            return Err(owners::Error::Schema);
        }
        let mut next = self.clone();
        let a = next.actors.get_mut(id).ok_or(owners::Error::Stale)?;
        let status = law::Actor {
            hp: a.hp,
            max_hp: a.max_hp,
            level: a.level,
            dead: a.is_dead,
            levitating: a.intrinsics.levitation,
        };
        a.hp = law::hp_after(status, con, draw);
        Ok(Proposal {
            next,
            events: vec![],
            reply: b"null".to_vec(),
        })
    }
}
impl RegenerationDomain for ObjectsState {
    fn regeneration_query(
        &self,
        name: &str,
        req: &[u8],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Vec<u8>> {
        if name != OBJECTS {
            return Err(owners::Error::Unsupported);
        }
        let id: ActorId = decode(req, wand::MAX_REQUEST_BYTES)?;
        encode(
            &Facts::Carried {
                player: id,
                weight: finite_weight(self, id)?,
                before: self.clone(),
            },
            wand::MAX_RESPONSE_BYTES,
        )
    }
}
impl RegenerationDomain for RandomState {
    fn regeneration_query(
        &self,
        name: &str,
        req: &[u8],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Vec<u8>> {
        if name != RANDOM || !req.is_empty() {
            return Err(owners::Error::Unsupported);
        }
        let mut rng = self.rng.clone();
        encode(
            &Facts::Draw(rng.random_range(0..100u32)),
            wand::MAX_RESPONSE_BYTES,
        )
    }
    fn regeneration_prepare(
        &self,
        name: &str,
        req: &[u8],
        obs: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        if name != WRITE_RANDOM || !obs.is_empty() {
            return Err(owners::Error::Unsupported);
        }
        let expected: u32 = decode(req, wand::MAX_REQUEST_BYTES)?;
        let mut next = self.clone();
        if next.rng.random_range(0..100u32) != expected {
            return Err(owners::Error::Stale);
        }
        Ok(Proposal {
            next,
            events: vec![],
            reply: b"null".to_vec(),
        })
    }
}
impl RegenerationDomain for EquipmentState {}
impl RegenerationDomain for GeographyState {}
impl RegenerationDomain for BillingState {}
impl RegenerationDomain for ClockState {}
impl RegenerationDomain for SessionState {}
pub(super) fn add_ports(
    name: &str,
    p: BTreeMap<String, Port>,
    ids: &BTreeMap<&str, owners::OwnerId>,
    c: &OwnerBridgeConfig,
) -> owners::Result<BTreeMap<String, Port>> {
    META.add_ports(c.regeneration_ticks, name, p, ids, c)
}
pub(super) fn validate_ports(
    name: &str,
    p: &BTreeMap<String, Port>,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    META.validate_ports(c.regeneration_ticks, name, p, c)
}
pub(super) fn filtered_ports(p: &BTreeMap<String, Port>) -> BTreeMap<String, Port> {
    META.filtered_ports(p)
}
pub(super) fn provider(m: Manifest, c: &OwnerBridgeConfig) -> Manifest {
    META.provider(c.regeneration_ticks, m)
}
pub(super) fn caller(m: Manifest, c: &OwnerBridgeConfig) -> Manifest {
    META.caller(c.regeneration_ticks, m, c)
}
pub(super) fn caller_ports(
    p: BTreeMap<String, NativePort>,
    c: &OwnerBridgeConfig,
) -> BTreeMap<String, NativePort> {
    META.caller_ports(c.regeneration_ticks, p, c)
}
pub(super) fn grant_names(c: &OwnerBridgeConfig) -> BTreeSet<String> {
    META.grant_names(c.regeneration_ticks)
}
pub(super) fn resources(r: &SourceRoutes) -> owners::Result<Vec<Resource>> {
    META.resources(r)
}
fn observations(r: &SourceRoutes, moved: bool, v: &CallerView) -> owners::Result<Vec<Observation>> {
    let h = r.hero.query(HERO, &[], &[], v)?;
    let (id, poly, strength, con) = hero(&h)?;
    let mut obs = vec![h];
    let (a, nested) = match r.observe(&WandQuery::Command(CommandQuery::Actor { target: id }), v) {
        Ok(value) => value,
        Err(owners::Error::Stale) => return Ok(obs),
        Err(error) => return Err(error),
    };
    let current = Some(actor(&a, id)?);
    obs.push(a);
    obs.extend(nested);
    if obs.len() != 3 {
        return Err(owners::Error::Schema);
    }
    if !law::eligible(current, poly, false, strength, con, None) {
        return Ok(obs);
    }
    if moved {
        obs.push(
            r.objects
                .query(OBJECTS, &encode(&id, wand::MAX_REQUEST_BYTES)?, &[], v)?,
        );
        if status(&obs, true).is_err() {
            return Ok(obs);
        }
    }
    obs.push(r.random.query(RANDOM, &[], &[], v)?);
    Ok(obs)
}
struct PhaseAccounting;
impl Accounting for PhaseAccounting {
    fn validate(&self, p: &[ProposalView], obs: &[Observation]) -> owners::Result<()> {
        let moved = obs.len() == 5;
        if obs.len() != if moved { 5 } else { 4 } {
            return Err(owners::Error::Schema);
        }
        if p.len() != 2 || p.iter().any(|x| !x.events.is_empty() || x.reply != b"null") {
            return Err(owners::Error::Invariant);
        }
        let (id, old, con, _weight) = status(obs, moved)?;
        if moved {
            match fact(obs.get(3).ok_or(owners::Error::Schema)?)? {
                Facts::Carried {
                    player,
                    weight,
                    before,
                } if player == id && finite_weight(&before, id)? == weight => {}
                _ => return Err(owners::Error::Invariant),
            }
        }
        let random_obs = obs
            .get(if moved { 4 } else { 3 })
            .ok_or(owners::Error::Schema)?;
        let draw = random_fact(random_obs)?;
        let a = p
            .iter()
            .find(|x| x.receipt.command == WRITE_ACTOR)
            .ok_or(owners::Error::Invariant)?;
        let r = p
            .iter()
            .find(|x| x.receipt.command == WRITE_RANDOM)
            .ok_or(owners::Error::Invariant)?;
        let mut actors: ActorsState = decode(&a.before, PAYLOAD)?;
        let before = actors.actors.get(id).ok_or(owners::Error::Stale)?;
        if (law::Actor {
            hp: before.hp,
            max_hp: before.max_hp,
            level: before.level,
            dead: before.is_dead,
            levitating: before.intrinsics.levitation,
        }) != old
        {
            return Err(owners::Error::Stale);
        }
        actors.actors.get_mut(id).ok_or(owners::Error::Stale)?.hp = law::hp_after(old, con, draw);
        if encode(&actors, PAYLOAD)? != a.proposed {
            return Err(owners::Error::Invariant);
        }
        let mut random: RandomState = decode(&r.before, PAYLOAD)?;
        if random.rng.random_range(0..100u32) != draw {
            return Err(owners::Error::Invariant);
        }
        if encode(&random, PAYLOAD)? != r.proposed {
            return Err(owners::Error::Invariant);
        }
        Ok(())
    }
}
fn identity() -> op::ArtifactIdentity {
    artifact("native-regeneration-accounting-i2c", 132)
}
pub(super) fn capture(
    a: &mut op::AccountingCaptureCatalog,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if c.regeneration_ticks {
        a.register(identity(), POLICY, Rc::new(PhaseAccounting))?;
    }
    Ok(())
}
pub(super) fn restore(
    a: &mut op::AccountingRestoreCatalog,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if c.regeneration_ticks {
        a.allow(identity(), POLICY, || Ok(Rc::new(PhaseAccounting)))?;
    }
    Ok(())
}
#[cfg(test)]
struct TestObjectMutationAccounting;
#[cfg(test)]
impl Accounting for TestObjectMutationAccounting {
    fn validate(&self, p: &[ProposalView], _: &[Observation]) -> owners::Result<()> {
        if p.len() != 1
            || p[0].receipt.command != "wand_objects_command_v2"
            || p[0].before == p[0].proposed
        {
            return Err(owners::Error::Invariant);
        }
        Ok(())
    }
}
#[cfg(test)]
fn publish_object_mutation(
    r: &SourceRoutes,
    id: ItemId,
    v: &CallerView,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    let command = wand::WandOwnerCommand::Objects(wand::ObjectOperation::PositiveCharge {
        target: wand::ObjectRef {
            id,
            residency: wand::Residency::Active,
        },
        expected: 3,
    });
    let mut immediate = Vec::new();
    let mut provenance = Vec::new();
    for query in wand_commands::dependencies(&command, c) {
        let (o, nested) = r.observe(&query, v)?;
        provenance.extend(nested);
        immediate.push(o.clone());
        provenance.push(o);
    }
    let request = wand::WandOperation {
        expected_owner: wand::OwnerWitness::from(&r.objects.stamp()?),
        dependencies: provenance
            .iter()
            .map(|o| wand::OwnerWitness::from(o.stamp()))
            .collect(),
        command,
    };
    let candidate = r
        .objects
        .prepare(
            "wand_objects_command_v2",
            &encode(&request, wand::MAX_REQUEST_BYTES)?,
            immediate,
            v,
        )?
        .erase();
    let owner = r.objects.id();
    let batch = r.host.batch(
        vec![candidate],
        provenance,
        &[owner],
        vec![owner],
        [227; 32],
        Rc::new(TestObjectMutationAccounting),
        profile_capability(c, "wand_objects_command_v2"),
        v,
    )?;
    batch.commit(v)?;
    OBJECT_MUTATION_STATE.with(|slot| {
        *slot.borrow_mut() = Some((
            r.objects.inspect()?,
            r.actors.inspect()?,
            r.random.inspect()?,
            r.host.history()?,
        ));
        Ok::<_, owners::Error>(())
    })?;
    Ok(())
}
pub(super) fn native(
    mut p: BTreeMap<String, NativePort>,
    r: Rc<Routes>,
    c: &OwnerBridgeConfig,
) -> BTreeMap<String, NativePort> {
    if c.regeneration_ticks {
        for (_, n, _) in ROUTES {
            p.insert(n.into(), Rc::new(|_, _| Err(kernel::Error::NativeRefusal)));
        }
        let rr = r.clone();
        p.insert(
            PREVIEW.into(),
            Rc::new(move |req, v| {
                let moved: bool = decode(req, wand::MAX_REQUEST_BYTES).map_err(native_refusal)?;
                let obs = observations(&rr.get()?, moved, v).map_err(native_refusal)?;
                encode(
                    &RegenerationRequest {
                        moved,
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
            Rc::new(move |req, v| {
                #[cfg(test)]
                let mutation = OBJECT_MUTATION.with(|slot| slot.take());
                #[cfg(test)]
                if let Some(id) = mutation {
                    publish_object_mutation(&r.get()?, id, v, &cc).map_err(native_refusal)?;
                }
                let result = execute(&r.get()?, req, v, &cc);
                #[cfg(test)]
                if mutation.is_some() {
                    assert_eq!(result.as_ref().err(), Some(&owners::Error::Stale));
                }
                result.map_err(native_refusal)
            }),
        );
    }
    p
}
fn execute(
    r: &SourceRoutes,
    req: &[u8],
    v: &CallerView,
    c: &OwnerBridgeConfig,
) -> owners::Result<Vec<u8>> {
    let request: RegenerationRequest = decode(req, wand::MAX_REQUEST_BYTES)?;
    let obs = observations(r, request.moved, v)?;
    if request.guards
        != obs
            .iter()
            .map(|o| OwnerWitness::from(o.stamp()))
            .collect::<Vec<_>>()
    {
        return Err(owners::Error::Stale);
    }
    if status(&obs, request.moved).is_err() {
        return encode(
            &RegenerationReply { healed: false },
            wand::MAX_RESPONSE_BYTES,
        );
    }
    let draw = random_fact(
        obs.get(if request.moved { 4 } else { 3 })
            .ok_or(owners::Error::Schema)?,
    )?;
    let a = r
        .actors
        .prepare(
            WRITE_ACTOR,
            &encode(&draw, wand::MAX_REQUEST_BYTES)?,
            vec![obs[0].clone()],
            v,
        )?
        .erase();
    let random = r
        .random
        .prepare(
            WRITE_RANDOM,
            &encode(&draw, wand::MAX_REQUEST_BYTES)?,
            vec![],
            v,
        )?
        .erase();
    let ids = vec![r.actors.id(), r.random.id()];
    #[cfg(test)]
    {
        let (_, old, con, _) = status(&obs, request.moved)?;
        let expected_hp = law::hp_after(old, con, draw);
        if let Some(bad_draw) = [0, 99]
            .into_iter()
            .find(|sample| law::hp_after(old, con, *sample) != expected_hp)
        {
            let prior = (r.actors.inspect()?, r.random.inspect()?, r.host.history()?);
            let bad_actor = r
                .actors
                .prepare(
                    WRITE_ACTOR,
                    &encode(&bad_draw, wand::MAX_REQUEST_BYTES)?,
                    vec![obs[0].clone()],
                    v,
                )?
                .erase();
            let good_random = r
                .random
                .prepare(
                    WRITE_RANDOM,
                    &encode(&draw, wand::MAX_REQUEST_BYTES)?,
                    vec![],
                    v,
                )?
                .erase();
            let attempt = r.host.batch(
                vec![bad_actor, good_random],
                obs.clone(),
                &ids,
                ids.clone(),
                POLICY,
                Rc::new(PhaseAccounting),
                profile_capability(c, RUN),
                v,
            );
            let refused = match attempt {
                Ok(b) => b.commit(v).map(|_| ()),
                Err(e) => Err(e),
            };
            assert_eq!(refused, Err(owners::Error::Invariant));
            assert_eq!(
                prior,
                (r.actors.inspect()?, r.random.inspect()?, r.host.history()?)
            );
            ACCOUNTING_REFUSALS.with(|count| count.set(count.get() + 1));
        }
    }
    let batch = r.host.batch(
        vec![a, random],
        obs.clone(),
        &ids,
        ids.clone(),
        POLICY,
        Rc::new(PhaseAccounting),
        profile_capability(c, RUN),
        v,
    )?;
    batch.commit(v)?;
    #[cfg(test)]
    {
        let published = (r.actors.inspect()?, r.random.inspect()?, r.host.history()?);
        assert_eq!(batch.commit(v), Err(owners::Error::Stale));
        assert_eq!(
            published,
            (r.actors.inspect()?, r.random.inspect()?, r.host.history()?)
        );
    }
    let (_, before, con, _) = status(&obs, request.moved)?;
    encode(
        &RegenerationReply {
            healed: law::hp_after(before, con, draw) > before.hp,
        },
        wand::MAX_RESPONSE_BYTES,
    )
}
impl OwnerBridge {
    pub fn prepare_regeneration_phase(
        &self,
        moved: bool,
    ) -> Result<RegenerationRequest, OwnerBridgeError> {
        decode(
            &self.frame_call(PREVIEW, &moved, Scope::All)?,
            wand::MAX_RESPONSE_BYTES,
        )
        .map_err(Into::into)
    }
    pub fn execute_regeneration_phase(
        &self,
        r: RegenerationRequest,
    ) -> Result<RegenerationReply, OwnerBridgeError> {
        self.execute_regeneration_phase_scoped(r, Scope::All)
    }
    pub fn execute_regeneration_phase_scoped(
        &self,
        r: RegenerationRequest,
        scope: Scope,
    ) -> Result<RegenerationReply, OwnerBridgeError> {
        decode(&self.frame_call(RUN, &r, scope)?, wand::MAX_RESPONSE_BYTES).map_err(Into::into)
    }
}
