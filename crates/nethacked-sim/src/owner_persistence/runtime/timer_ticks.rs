//! Explicit M6-I2e object timer phase with Troll revival and microkernel transport.
use super::*;
use crate::revival_law::{ordinary_troll_attempt, RevivalAttempt, RevivalInput};
use crate::{GameEvent, ObjectTimer, ObjectTimerKind, ObjectTimerTarget};
use nethacked_arena::{ActorId, ActorRecord, ActorStorage, ItemId, ItemLocation, ItemStorage};
use nethacked_data::corpses::CorpseDefinition;
use nethacked_kernel::ModuleHandle;
use nethacked_types::{Alignment, BranchId, GenocideRegistry};
use rand::SeedableRng;
use serde::{Deserialize, Serialize};

pub const HOME: &str = "timer-home-i2e-v1";
pub const CALLER: &str = "timer-caller-i2e-v1";
pub const PREVIEW: &str = "timer_preview_i2e";
pub const RUN: &str = "timer_run_i2e";
pub const READS: [&str; 9] = [
    "timer_clock_read",
    "timer_objects_read",
    "timer_actors_read",
    "timer_hero_read",
    "timer_equipment_read",
    "timer_billing_read",
    "timer_random_read",
    "timer_geography_read",
    "timer_session_read",
];
pub const WRITES: [&str; 7] = [
    "timer_clock_prepare",
    "timer_objects_prepare",
    "timer_actors_prepare",
    "timer_hero_prepare",
    "timer_equipment_prepare",
    "timer_billing_prepare",
    "timer_random_prepare",
];
pub const POLICY: [u8; 32] = [211; 32];

#[cfg(test)]
thread_local! {
    static STALE_REFUSALS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static WRONG_RESOURCE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
#[cfg(test)]
pub(super) fn stale_refusals() -> usize {
    STALE_REFUSALS.with(std::cell::Cell::get)
}
#[cfg(test)]
pub(super) fn trigger_wrong_resource(wrong: bool) {
    WRONG_RESOURCE.with(|s| s.set(wrong));
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimerRequest {
    pub item: ItemId,
    pub guards: Vec<wand::OwnerWitness>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimerReply {
    pub item: ItemId,
    pub outcome: Outcome,
    pub events: Vec<GameEvent>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DueRequest {
    item: ItemId,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PrepareV1 {
    item: ItemId,
    attempt: Outcome,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    Failed { rot_delay: u64 },
    Revived { actor: Box<ActorRecord> },
}

#[derive(Clone, Serialize, Deserialize)]
struct ClockFacts {
    turn: u64,
    due: Option<ObjectTimer>,
}

#[derive(Clone, Serialize, Deserialize)]
struct ObjectFacts {
    items: ItemStorage,
}

#[derive(Clone, Serialize, Deserialize)]
struct ActorFacts {
    actors: ActorStorage,
}

#[derive(Clone, Serialize, Deserialize)]
struct HeroFacts {
    player: ActorId,
    race: nethacked_data::RaceId,
    alignment_record: i32,
    genocide: GenocideRegistry,
    quivered: Option<ItemId>,
}

#[derive(Clone, Serialize, Deserialize)]
struct GeographyFacts {
    #[serde(with = "nethacked_dungeon::compact_level")]
    level: nethacked_dungeon::DungeonLevel,
    branch: BranchId,
    depth: usize,
}

#[derive(Clone, Serialize, Deserialize)]
struct SessionFacts {
    role: String,
}

#[derive(Clone, Serialize, Deserialize)]
enum EquipmentReadV1 {
    Unreferenced(bool),
}

#[derive(Clone, Serialize, Deserialize)]
enum BillingReadV1 {
    Unreferenced(bool),
}

fn fact<T: DeserializeOwned + Serialize>(obs: &[Observation], index: usize) -> owners::Result<T> {
    let o = obs
        .iter()
        .find(|o| o.query() == READS[index])
        .ok_or(owners::Error::Schema)?;
    if o.query() != READS[index] {
        return Err(owners::Error::Schema);
    }
    decode(o.bytes(), PAYLOAD)
}

fn outcome(
    id: ItemId,
    obs: &[Observation],
    config: &OwnerBridgeConfig,
    rng: &mut rand_chacha::ChaCha8Rng,
) -> owners::Result<Outcome> {
    let c: ClockFacts = fact(obs, 0)?;
    let o: ObjectFacts = fact(obs, 1)?;
    let a: ActorFacts = fact(obs, 2)?;
    let h: HeroFacts = fact(obs, 3)?;
    let e: EquipmentReadV1 = fact(obs, 4)?;
    let b: BillingReadV1 = fact(obs, 5)?;
    let g: GeographyFacts = fact(obs, 7)?;
    let s: SessionFacts = fact(obs, 8)?;
    let due = c.due.ok_or(owners::Error::Unsupported)?;
    let item = o.items.get(id).ok_or(owners::Error::Unsupported)?;
    let CorpseDefinition::Owned(body) = config.ruleset.corpse_body_definition(item) else {
        return Err(owners::Error::Unsupported);
    };
    if due.target != ObjectTimerTarget::Active(id)
        || due.kind != ObjectTimerKind::ReviveMonster
        || due.due > c.turn
        || item.quantity != 1
        || item.saved_monster_traits.is_some()
        || !matches!(item.location, ItemLocation::Floor(_))
        || !item.is_corpse()
        || body.glyph != 'T'
        || body.corpse_timeout_policy != nethacked_data::monsters::CorpseTimeoutPolicy::Ordinary
        || body.corpse_thaw_policy != nethacked_data::monsters::CorpseThawPolicy::Ordinary
        || h.quivered == Some(id)
        || !matches!(e, EquipmentReadV1::Unreferenced(true))
        || !matches!(b, BillingReadV1::Unreferenced(true))
    {
        return Err(owners::Error::Unsupported);
    }
    let hero_alignment = a
        .actors
        .get(h.player)
        .map(|p| p.alignment)
        .unwrap_or(Alignment::Neutral);
    let input = RevivalInput {
        ruleset: &config.ruleset,
        items: &o.items,
        actors: &a.actors,
        level: &g.level,
        genocide_registry: &h.genocide,
        player: h.player,
        branch: g.branch,
        depth: g.depth,
        hero_alignment,
        hero_race: h.race,
        alignment_record: h.alignment_record,
        role_name: &s.role,
    };
    match ordinary_troll_attempt(id, &input, rng) {
        RevivalAttempt::Unsupported => Err(owners::Error::Unsupported),
        RevivalAttempt::Failed { rot_delay } => Ok(Outcome::Failed { rot_delay }),
        RevivalAttempt::Revived { actor } => Ok(Outcome::Revived {
            actor: Box::new(actor),
        }),
    }
}

trait Domain: Clone + Serialize + 'static {
    const INDEX: usize;
    fn read(
        &self,
        id: ItemId,
        obs: &[Observation],
        c: &OwnerBridgeConfig,
    ) -> owners::Result<Vec<u8>>;
    fn prepare(
        &self,
        id: ItemId,
        obs: &[Observation],
        c: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>>;
    fn prepare_intent(
        &self,
        _: PrepareV1,
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        Err(owners::Error::Unsupported)
    }
}

struct TimerCodec<T>(OwnerBridgeConfig, std::marker::PhantomData<T>);
impl<T: Domain> Schema<T> for TimerCodec<T> {
    fn validate(&self, _: &T) -> owners::Result<()> {
        Ok(())
    }
    fn encode(&self, s: &T) -> owners::Result<Vec<u8>> {
        encode(s, PAYLOAD)
    }
    fn query(
        &self,
        s: &T,
        name: &str,
        request: &[u8],
        obs: &[Observation],
    ) -> owners::Result<Vec<u8>> {
        if name != READS[T::INDEX] {
            return Err(owners::Error::Unsupported);
        }
        let req: DueRequest = decode(request, PAYLOAD)?;
        s.read(req.item, obs, &self.0)
    }
}

impl<T: Domain> NativeBehavior<T> for TimerCodec<T> {
    fn prepare(
        &self,
        s: &T,
        name: &str,
        request: &[u8],
        obs: &[Observation],
    ) -> owners::Result<Proposal<T>> {
        if WRITES.get(T::INDEX).copied() != Some(name) {
            return Err(owners::Error::Unsupported);
        }
        if T::INDEX == 6 {
            let req: DueRequest = decode(request, PAYLOAD)?;
            return s.prepare(req.item, obs, &self.0);
        }
        if !obs.is_empty() {
            return Err(owners::Error::Schema);
        }
        let req: PrepareV1 = decode(request, PAYLOAD)?;
        s.prepare_intent(req, &self.0)
    }
}

fn proposed<T>(next: T, events: Vec<Vec<u8>>, result: &Outcome) -> owners::Result<Proposal<T>> {
    Ok(Proposal {
        next,
        events,
        reply: encode(result, PAYLOAD)?,
    })
}

fn preview(obs: &[Observation]) -> owners::Result<Outcome> {
    fact(obs, 6)
}

impl Domain for ClockState {
    const INDEX: usize = 0;
    fn read(&self, _: ItemId, _: &[Observation], _: &OwnerBridgeConfig) -> owners::Result<Vec<u8>> {
        let mut q = self.object_timers.clone();
        encode(
            &ClockFacts {
                turn: self.scheduler.turn,
                due: q.pop_due_active(self.scheduler.turn),
            },
            PAYLOAD,
        )
    }
    fn prepare(
        &self,
        _: ItemId,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        Err(owners::Error::Unsupported)
    }
    fn prepare_intent(
        &self,
        req: PrepareV1,
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        let id = req.item;
        let result = req.attempt;
        let mut next = self.clone();
        let timer = next
            .object_timers
            .pop_due_active(next.scheduler.turn)
            .ok_or(owners::Error::Unsupported)?;
        if timer.target != ObjectTimerTarget::Active(id)
            || timer.kind != ObjectTimerKind::ReviveMonster
        {
            return Err(owners::Error::Unsupported);
        }
        match &result {
            Outcome::Revived { .. } => {
                next.object_timers.cancel(timer.target);
            }
            Outcome::Failed { rot_delay } => {
                if !next
                    .object_timers
                    .entries()
                    .iter()
                    .any(|t| t.target == timer.target && t.kind == ObjectTimerKind::RotCorpse)
                {
                    next.object_timers
                        .start_at_preserving(
                            timer.target,
                            ObjectTimerKind::RotCorpse,
                            next.scheduler.turn.checked_add(*rot_delay),
                            true,
                        )
                        .map_err(|_| owners::Error::Invariant)?;
                }
            }
        }
        proposed(
            next,
            vec![encode(&GameEvent::ObjectTimerExpired { timer }, PAYLOAD)?],
            &result,
        )
    }
}

impl Domain for ObjectsState {
    const INDEX: usize = 1;
    fn read(
        &self,
        id: ItemId,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Vec<u8>> {
        if self.items.get(id).is_none() {
            return Err(owners::Error::Unsupported);
        }
        encode(
            &ObjectFacts {
                items: self.items.clone(),
            },
            PAYLOAD,
        )
    }
    fn prepare(
        &self,
        _: ItemId,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        Err(owners::Error::Unsupported)
    }
    fn prepare_intent(
        &self,
        req: PrepareV1,
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        let id = req.item;
        let result = req.attempt;
        let mut next = self.clone();
        if matches!(result, Outcome::Revived { .. }) {
            next.items.remove(id).ok_or(owners::Error::Stale)?;
        }
        proposed(next, vec![], &result)
    }
}

impl Domain for ActorsState {
    const INDEX: usize = 2;
    fn read(&self, _: ItemId, _: &[Observation], _: &OwnerBridgeConfig) -> owners::Result<Vec<u8>> {
        encode(
            &ActorFacts {
                actors: self.actors.clone(),
            },
            PAYLOAD,
        )
    }
    fn prepare(
        &self,
        _: ItemId,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        Err(owners::Error::Unsupported)
    }
    fn prepare_intent(
        &self,
        req: PrepareV1,
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        let result = req.attempt;
        let mut next = self.clone();
        if let Outcome::Revived { actor } = &result {
            next.actors = serde_json::from_value(
                serde_json::to_value(&next.actors).map_err(|_| owners::Error::Schema)?,
            )
            .map_err(|_| owners::Error::Schema)?;
            next.actors.insert(*actor.clone());
        }
        proposed(next, vec![], &result)
    }
}

impl Domain for HeroState {
    const INDEX: usize = 3;
    fn read(&self, _: ItemId, _: &[Observation], _: &OwnerBridgeConfig) -> owners::Result<Vec<u8>> {
        encode(
            &HeroFacts {
                player: self.player_id,
                race: self.hero_race,
                alignment_record: self.alignment_record,
                genocide: self.genocide_registry.clone(),
                quivered: self.hero.quivered_item,
            },
            PAYLOAD,
        )
    }
    fn prepare(
        &self,
        _: ItemId,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        Err(owners::Error::Unsupported)
    }
}

impl Domain for EquipmentState {
    const INDEX: usize = 4;
    fn read(
        &self,
        id: ItemId,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Vec<u8>> {
        let empty = encode(&self.equipment, PAYLOAD)?
            == encode(
                &crate::actions::equipment::EquipmentState::default(),
                PAYLOAD,
            )?;
        encode(
            &EquipmentReadV1::Unreferenced(self.wielded_item != Some(id) && empty),
            PAYLOAD,
        )
    }
    fn prepare(
        &self,
        _: ItemId,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        Err(owners::Error::Unsupported)
    }
}

impl Domain for BillingState {
    const INDEX: usize = 5;
    fn read(
        &self,
        id: ItemId,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Vec<u8>> {
        encode(
            &BillingReadV1::Unreferenced(
                !self
                    .unpaid_items
                    .iter()
                    .chain(self.stored_unpaid_items.iter().flatten())
                    .any(|(i, _)| *i == id),
            ),
            PAYLOAD,
        )
    }
    fn prepare(
        &self,
        _: ItemId,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        Err(owners::Error::Unsupported)
    }
}

impl Domain for RandomState {
    const INDEX: usize = 6;
    fn read(
        &self,
        id: ItemId,
        obs: &[Observation],
        c: &OwnerBridgeConfig,
    ) -> owners::Result<Vec<u8>> {
        encode(&outcome(id, obs, c, &mut self.rng.clone())?, PAYLOAD)
    }
    fn prepare(
        &self,
        id: ItemId,
        obs: &[Observation],
        c: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        let mut next = self.clone();
        let result = outcome(id, obs, c, &mut next.rng)?;
        proposed(next, vec![], &result)
    }
}

impl Domain for GeographyState {
    const INDEX: usize = 7;
    fn read(&self, _: ItemId, _: &[Observation], _: &OwnerBridgeConfig) -> owners::Result<Vec<u8>> {
        encode(
            &GeographyFacts {
                level: self.level.clone(),
                branch: self.current_branch,
                depth: self.depth,
            },
            PAYLOAD,
        )
    }
    fn prepare(
        &self,
        _: ItemId,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        Err(owners::Error::Unsupported)
    }
}

impl Domain for SessionState {
    const INDEX: usize = 8;
    fn read(&self, _: ItemId, _: &[Observation], _: &OwnerBridgeConfig) -> owners::Result<Vec<u8>> {
        encode(
            &SessionFacts {
                role: self.role_name.clone(),
            },
            PAYLOAD,
        )
    }
    fn prepare(
        &self,
        _: ItemId,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        Err(owners::Error::Unsupported)
    }
}

pub(super) struct TimerAccounting(pub(super) OwnerBridgeConfig);
impl Accounting for TimerAccounting {
    fn validate(&self, p: &[ProposalView], obs: &[Observation]) -> owners::Result<()> {
        if obs.len() != 9 {
            return Err(owners::Error::Schema);
        }
        let clock_facts: ClockFacts = fact(obs, 0)?;
        let due = clock_facts.due.ok_or(owners::Error::Invariant)?;
        let item = match due.target {
            ObjectTimerTarget::Active(id) => id,
            _ => return Err(owners::Error::Invariant),
        };
        let random = p
            .iter()
            .find(|x| x.receipt.command == WRITES[6])
            .ok_or(owners::Error::Invariant)?;
        let before: RandomState = decode(&random.before, PAYLOAD)?;
        let mut expected = before.clone();
        let result = outcome(item, obs, &self.0, &mut expected.rng)?;
        if encode(&expected, PAYLOAD)? != random.proposed
            || encode(&result, PAYLOAD)? != random.reply
            || encode(&result, PAYLOAD)?
                != fact::<Outcome>(obs, 6).and_then(|r| encode(&r, PAYLOAD))?
            || !random.events.is_empty()
        {
            return Err(owners::Error::Invariant);
        }
        let revived = matches!(result, Outcome::Revived { .. });
        if p.len() != if revived { 4 } else { 2 } {
            return Err(owners::Error::Invariant);
        }
        let req = PrepareV1 {
            item,
            attempt: result,
        };
        for x in p.iter().filter(|x| x.receipt.command != WRITES[6]) {
            macro_rules! check {
                ($state:ty) => {{
                    let old: $state = decode(&x.before, PAYLOAD)?;
                    let expected = old.prepare_intent(req.clone(), &self.0)?;
                    if encode(&expected.next, PAYLOAD)? != x.proposed
                        || expected.events != x.events
                        || expected.reply != x.reply
                    {
                        return Err(owners::Error::Invariant);
                    }
                }};
            }
            match x.receipt.command.as_str() {
                "timer_clock_prepare" => check!(ClockState),
                "timer_objects_prepare" if revived => check!(ObjectsState),
                "timer_actors_prepare" if revived => check!(ActorsState),
                _ => return Err(owners::Error::Invariant),
            }
        }
        for n in [WRITES[0], WRITES[6]].into_iter().chain(if revived {
            vec![WRITES[1], WRITES[2]]
        } else {
            vec![]
        }) {
            if p.iter().filter(|x| x.receipt.command == n).count() != 1 {
                return Err(owners::Error::Invariant);
            }
        }
        Ok(())
    }
}

fn identity() -> op::ArtifactIdentity {
    artifact("native-timer-accounting-i2e", 101)
}

pub(super) fn capture(
    a: &mut op::AccountingCaptureCatalog,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if c.timer_ticks {
        a.register(identity(), POLICY, Rc::new(TimerAccounting(c.clone())))?;
    }
    Ok(())
}

pub(super) fn restore(
    a: &mut op::AccountingRestoreCatalog,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if c.timer_ticks {
        let cc = c.clone();
        a.allow(identity(), POLICY, move || {
            Ok(Rc::new(TimerAccounting(cc.clone())))
        })?;
    }
    Ok(())
}

fn cap(name: &str) -> Capability {
    Capability {
        provider: HOME.into(),
        endpoint: name.into(),
        major: 1,
    }
}

fn manifest(
    id: &str,
    exports: BTreeMap<String, Endpoint>,
    dependencies: Vec<kernel::Dependency>,
) -> Manifest {
    Manifest {
        id: id.into(),
        exports,
        dependencies,
        extension_points: BTreeSet::new(),
        extension: None,
    }
}

fn live_endpoint(access: Access) -> Endpoint {
    Endpoint {
        major: 1,
        access,
        max_request: 16 * 1024,
        max_reply: 16 * 1024,
    }
}

fn scoped(access: Access, resources: Vec<Resource>) -> Authority {
    Authority {
        access,
        scope: Scope::Keys(resources.into_iter().collect()),
    }
}

pub(super) struct TimerTransport {
    kernel: Kernel,
    caller: ModuleHandle,
    routes: Rc<Routes>,
    resources: Vec<Resource>,
}

impl TimerTransport {
    pub(super) fn install(
        parts: NativePartitions,
        config: OwnerBridgeConfig,
    ) -> Result<Self, OwnerBridgeError> {
        let host = Host::new(limits())?;
        host.configure_publication(publication_limits())?;
        let mut kernel = Kernel::new(kernel::Limits::default())?;
        let routes = Rc::new(Routes::Initial(RefCell::new(None)));
        let r = routes.clone();
        let c = config.clone();
        let r_preview = routes.clone();
        let c_preview = config.clone();

        let preview_port: NativePort = Rc::new(move |bytes, view| {
            let req: DueRequest = decode(bytes, PAYLOAD).map_err(native_refusal)?;
            let r = r_preview.get()?;
            let read = || -> owners::Result<Vec<Observation>> {
                let mut obs = vec![
                    r.clock.query(READS[0], bytes, &[], view)?,
                    r.objects.query(READS[1], bytes, &[], view)?,
                    r.actors.query(READS[2], bytes, &[], view)?,
                    r.hero.query(READS[3], bytes, &[], view)?,
                    r.equipment.query(READS[4], bytes, &[], view)?,
                    r.billing.query(READS[5], bytes, &[], view)?,
                    r.geography.query(READS[7], bytes, &[], view)?,
                    r.session.query(READS[8], bytes, &[], view)?,
                ];
                obs.push(r.random.query(READS[6], bytes, &obs, view)?);
                Ok(obs)
            };
            let obs = read().map_err(native_refusal)?;
            let _ = outcome(
                req.item,
                &obs,
                &c_preview,
                &mut rand_chacha::ChaCha8Rng::seed_from_u64(0),
            )
            .map_err(native_refusal)?;
            let guards = obs
                .iter()
                .map(|o| wand::OwnerWitness::from(o.stamp()))
                .collect();
            encode(
                &TimerRequest {
                    item: req.item,
                    guards,
                },
                PAYLOAD,
            )
            .map_err(native_refusal)
        });

        let run_port: NativePort = Rc::new(move |bytes, view| {
            let req: DueRequest = decode(bytes, PAYLOAD).map_err(native_refusal)?;
            let r = r.get()?;
            let read = || -> owners::Result<Vec<Observation>> {
                let mut obs = vec![
                    r.clock.query(READS[0], bytes, &[], view)?,
                    r.objects.query(READS[1], bytes, &[], view)?,
                    r.actors.query(READS[2], bytes, &[], view)?,
                    r.hero.query(READS[3], bytes, &[], view)?,
                    r.equipment.query(READS[4], bytes, &[], view)?,
                    r.billing.query(READS[5], bytes, &[], view)?,
                    r.geography.query(READS[7], bytes, &[], view)?,
                    r.session.query(READS[8], bytes, &[], view)?,
                ];
                obs.push(r.random.query(READS[6], bytes, &obs, view)?);
                Ok(obs)
            };
            let obs = read().map_err(native_refusal)?;

            let result = preview(&obs).map_err(native_refusal)?;
            let intent = encode(
                &PrepareV1 {
                    item: req.item,
                    attempt: result.clone(),
                },
                PAYLOAD,
            )
            .map_err(native_refusal)?;
            let other: Vec<_> = obs
                .iter()
                .filter(|o| o.query() != READS[6])
                .cloned()
                .collect();
            let mut candidates = vec![
                r.clock
                    .prepare(WRITES[0], &intent, vec![], view)
                    .map_err(native_refusal)?
                    .erase(),
                r.random
                    .prepare(WRITES[6], bytes, other, view)
                    .map_err(native_refusal)?
                    .erase(),
            ];
            let mut writes = vec![r.clock.id(), r.random.id()];
            if matches!(result, Outcome::Revived { .. }) {
                candidates.push(
                    r.objects
                        .prepare(WRITES[1], &intent, vec![], view)
                        .map_err(native_refusal)?
                        .erase(),
                );
                candidates.push(
                    r.actors
                        .prepare(WRITES[2], &intent, vec![], view)
                        .map_err(native_refusal)?
                        .erase(),
                );
                writes.extend([r.objects.id(), r.actors.id()]);
            }
            let batch = r
                .host
                .batch(
                    candidates,
                    obs,
                    &writes,
                    writes.clone(),
                    POLICY,
                    Rc::new(TimerAccounting(c.clone())),
                    cap(RUN),
                    view,
                )
                .map_err(native_refusal)?;
            if let Err(e) = batch.commit(view) {
                batch.discard().map_err(native_refusal)?;
                return Err(native_refusal(e));
            }
            encode(&result, PAYLOAD).map_err(native_refusal)
        });

        let mut exports: BTreeMap<_, _> = READS
            .into_iter()
            .map(|n| (n.into(), live_endpoint(Access::Read)))
            .chain(
                WRITES
                    .into_iter()
                    .map(|n| (n.into(), live_endpoint(Access::Write))),
            )
            .collect();
        exports.insert(PREVIEW.into(), live_endpoint(Access::Read));
        exports.insert(RUN.into(), live_endpoint(Access::Write));

        let refuse: NativePort = Rc::new(|_, _| Err(kernel::Error::NativeRefusal));
        let mut ports: BTreeMap<String, NativePort> = exports
            .keys()
            .cloned()
            .map(|n| (n, refuse.clone()))
            .collect();
        ports.insert(PREVIEW.into(), preview_port);
        ports.insert(RUN.into(), run_port);

        let home =
            kernel.register_native(manifest(HOME, exports.clone(), vec![]), &[212; 32], ports)?;
        let deps: Vec<_> = READS
            .into_iter()
            .map(|n| kernel::Dependency {
                capability: cap(n),
                access: Access::Read,
            })
            .chain(WRITES.into_iter().map(|n| kernel::Dependency {
                capability: cap(n),
                access: Access::Write,
            }))
            .chain([
                kernel::Dependency {
                    capability: cap(PREVIEW),
                    access: Access::Read,
                },
                kernel::Dependency {
                    capability: cap(RUN),
                    access: Access::Write,
                },
            ])
            .collect();
        let caller_preview: NativePort = Rc::new(|b, v| v.call_dependency(16, b));
        let caller_run: NativePort = Rc::new(|b, v| v.call_dependency(17, b));
        let caller = kernel.register_native(
            manifest(
                CALLER,
                [
                    (PREVIEW.into(), live_endpoint(Access::Read)),
                    (RUN.into(), live_endpoint(Access::Write)),
                ]
                .into(),
                deps,
            ),
            &[213; 32],
            [(PREVIEW.into(), caller_preview), (RUN.into(), caller_run)].into(),
        )?;

        macro_rules! enroll {
            ($state:expr, $ty:ty, $deps:expr) => {{
                let index = <$ty>::INDEX;
                let mut ports = BTreeMap::from([(
                    READS[index].into(),
                    Port {
                        access: Access::Read,
                        major: 1,
                        dependencies: $deps,
                    },
                )]);
                if let Some(write_name) = WRITES.get(index) {
                    ports.insert(
                        (*write_name).into(),
                        Port {
                            access: Access::Write,
                            major: 1,
                            dependencies: if index == 6 {
                                ports[READS[index]].dependencies.clone()
                            } else {
                                vec![]
                            },
                        },
                    );
                }
                host.enroll(
                    &kernel,
                    OwnerSpec {
                        module: home,
                        schema: [index as u8 + 220; 32],
                        ports,
                    },
                    $state,
                    Backend::Native(Rc::new(TimerCodec::<$ty>(
                        config.clone(),
                        std::marker::PhantomData,
                    ))),
                )?
            }};
        }

        let clock = enroll!(parts.clock, ClockState, vec![]);
        let objects = enroll!(parts.objects, ObjectsState, vec![]);
        let actors = enroll!(parts.actors, ActorsState, vec![]);
        let hero = enroll!(parts.hero, HeroState, vec![]);
        let equipment = enroll!(parts.equipment, EquipmentState, vec![]);
        let billing = enroll!(parts.billing, BillingState, vec![]);
        let geography = enroll!(parts.geography, GeographyState, vec![]);
        let session = enroll!(parts.session, SessionState, vec![]);
        let mut deps = vec![];
        macro_rules! dependency {
            ($owner:ident, $index:expr) => {
                deps.push(owners::Dependency {
                    owner: $owner.id(),
                    query: READS[$index].into(),
                    schema: $owner.stamp().map_err(native_refusal)?.schema,
                });
            };
        }
        dependency!(clock, 0);
        dependency!(objects, 1);
        dependency!(actors, 2);
        dependency!(hero, 3);
        dependency!(equipment, 4);
        dependency!(billing, 5);
        dependency!(geography, 7);
        dependency!(session, 8);
        let random = enroll!(parts.random, RandomState, deps);
        let r = SourceRoutes {
            host: host.clone(),
            objects,
            hero,
            session,
            billing,
            geography,
            actors,
            equipment,
            clock,
            random,
        };
        let mut resources = vec![];
        macro_rules! resource {
            ($owner:ident, $index:expr) => {
                resources.push(r.$owner.resource(READS[$index]).map_err(native_refusal)?);
                if let Some(write_name) = WRITES.get($index) {
                    resources.push(r.$owner.resource(write_name).map_err(native_refusal)?);
                }
            };
        }
        resource!(clock, 0);
        resource!(objects, 1);
        resource!(actors, 2);
        resource!(hero, 3);
        resource!(equipment, 4);
        resource!(billing, 5);
        resource!(random, 6);
        resource!(geography, 7);
        resource!(session, 8);
        for (slot, n) in READS.into_iter().chain(WRITES).enumerate() {
            let resource = resources
                .iter()
                .find(|res| res.key.ends_with(&format!(".{n}")))
                .ok_or(owners::Error::Schema)?
                .clone();
            kernel.issue_grant(
                caller,
                slot,
                scoped(
                    if slot < 9 {
                        Access::Read
                    } else {
                        Access::Write
                    },
                    vec![resource],
                ),
            )?;
        }
        // Slot 16: PREVIEW
        kernel.issue_grant(caller, 16, scoped(Access::Read, resources.clone()))?;
        // Slot 17: RUN
        resources.push(host.publication_resource());
        kernel.issue_grant(caller, 17, scoped(Access::Write, resources.clone()))?;

        if let Routes::Initial(slot) = routes.as_ref() {
            *slot.borrow_mut() = Some(r);
        }
        Ok(Self {
            kernel,
            caller,
            routes,
            resources,
        })
    }

    pub(super) fn preview(&mut self, item: ItemId) -> Result<TimerRequest, OwnerBridgeError> {
        let r = self.routes.get()?;
        let cleanup = r.host.cleanup_identity()?;
        let authority = scoped(Access::Read, self.resources.clone());
        let lease = self
            .kernel
            .capture_pipeline(self.caller, PREVIEW, authority.clone())?;
        let op_id = self.kernel.issue_operation()?;
        let operation =
            self.kernel
                .operation_binding_with_cleanup(op_id, lease, 1, PREVIEW, &[cleanup])?;
        let reply = self.kernel.invoke_operation(
            &operation,
            &encode(&DueRequest { item }, PAYLOAD)?,
            authority,
        );
        let _ = self.kernel.release_pipeline(lease);
        decode(&reply?, PAYLOAD).map_err(Into::into)
    }

    pub(super) fn execute(
        &mut self,
        request: TimerRequest,
    ) -> Result<(TimerReply, NativePartitions), OwnerBridgeError> {
        let r = self.routes.get()?;
        let cleanup = r.host.cleanup_identity()?;
        #[allow(unused_mut)]
        let mut authority = scoped(Access::Write, self.resources.clone());
        #[cfg(test)]
        if WRONG_RESOURCE.with(std::cell::Cell::get) {
            authority.scope = Scope::Keys(BTreeSet::new());
        }
        let lease = self
            .kernel
            .capture_pipeline(self.caller, RUN, authority.clone())?;
        let op_id = self.kernel.issue_operation()?;
        let operation =
            self.kernel
                .operation_binding_with_cleanup(op_id, lease, 1, RUN, &[cleanup])?;
        let reply = self.kernel.invoke_operation(
            &operation,
            &encode(&DueRequest { item: request.item }, PAYLOAD)?,
            authority,
        );
        let _ = self.kernel.release_pipeline(lease);
        let outcome: Outcome = decode(&reply?, PAYLOAD)?;

        let history = r.host.history()?;
        let events = history
            .last()
            .map(|c| {
                c.events
                    .iter()
                    .map(|e| decode::<GameEvent>(e, PAYLOAD))
                    .collect::<owners::Result<Vec<_>>>()
            })
            .transpose()?
            .unwrap_or_default();

        let updated = NativePartitions {
            geography: decode(&r.geography.inspect()?, PAYLOAD)?,
            objects: decode(&r.objects.inspect()?, PAYLOAD)?,
            actors: decode(&r.actors.inspect()?, PAYLOAD)?,
            hero: decode(&r.hero.inspect()?, PAYLOAD)?,
            equipment: decode(&r.equipment.inspect()?, PAYLOAD)?,
            billing: decode(&r.billing.inspect()?, PAYLOAD)?,
            clock: decode(&r.clock.inspect()?, PAYLOAD)?,
            random: decode(&r.random.inspect()?, PAYLOAD)?,
            session: decode(&r.session.inspect()?, PAYLOAD)?,
        };

        Ok((
            TimerReply {
                item: request.item,
                outcome,
                events,
            },
            updated,
        ))
    }
}

impl OwnerBridge {
    fn session_witnesses(&self) -> Result<Vec<wand::OwnerWitness>, OwnerBridgeError> {
        self.session.with_runtime(&self.credential.borrow(), |b| {
            Ok(bridge_stamps(b)?
                .iter()
                .map(wand::OwnerWitness::from)
                .collect())
        })?
    }

    pub fn prepare_timer_tick(&self, item: ItemId) -> Result<TimerRequest, OwnerBridgeError> {
        if !self.config.timer_ticks {
            return Err(owners::Error::Unsupported.into());
        }
        let parts = self.partitions()?;
        let mut transport = TimerTransport::install(parts, self.config.clone())?;
        let _ = transport.preview(item)?;
        let guards = self.session_witnesses()?;
        Ok(TimerRequest { item, guards })
    }

    pub fn execute_timer_tick(
        &self,
        request: TimerRequest,
    ) -> Result<TimerReply, OwnerBridgeError> {
        if !self.config.timer_ticks {
            return Err(owners::Error::Unsupported.into());
        }
        let current_witnesses = self.session_witnesses()?;
        if request.guards != current_witnesses {
            #[cfg(test)]
            STALE_REFUSALS.with(|x| x.set(x.get() + 1));
            return Err(owners::Error::Stale.into());
        }
        let parts = self.partitions()?;
        let mut transport = TimerTransport::install(parts, self.config.clone())?;
        let (reply, updated) = transport.execute(request)?;
        let source = Self::install(updated, self.config.clone())?;
        let wire = source.capture()?;
        drop(source);
        self.restore(&wire)?;
        Ok(reply)
    }

    pub fn tick_timer(&self, item: ItemId) -> Result<TimerReply, OwnerBridgeError> {
        let req = self.prepare_timer_tick(item)?;
        self.execute_timer_tick(req)
    }

    pub fn tick_timers(&self) -> Result<Vec<GameEvent>, OwnerBridgeError> {
        if !self.config.timer_ticks {
            return Err(owners::Error::Unsupported.into());
        }
        let mut all_events = Vec::new();
        loop {
            let parts = self.partitions()?;
            let mut timers = parts.clock.object_timers.clone();
            let Some(timer) = timers.pop_due_active(parts.clock.scheduler.turn) else {
                break;
            };
            let ObjectTimerTarget::Active(item) = timer.target else {
                break;
            };
            let reply = self.tick_timer(item)?;
            all_events.extend(reply.events);
        }
        Ok(all_events)
    }
}
