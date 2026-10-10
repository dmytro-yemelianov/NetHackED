//! Live-only, opt-in transport experiment. No resolver or persistence installation.
use super::*;
use crate::revival_law::{ordinary_troll_attempt, RevivalAttempt, RevivalInput};
use crate::{GameEvent, ObjectTimer, ObjectTimerKind, ObjectTimerTarget};
use nethacked_arena::{ActorId, ActorRecord, ActorStorage, ItemStorage};
use nethacked_data::corpses::CorpseDefinition;
use nethacked_kernel::ModuleHandle;
use nethacked_types::{Alignment, BranchId, Coord, GenocideRegistry, Tile};
use rand::{RngCore, SeedableRng};
use serde::{Deserialize, Serialize};

const HOME: &str = "timer-live-home-v1";
const CALLER: &str = "timer-live-caller-v1";
const RUN: &str = "timer_live_run";
const READS: [&str; 9] = [
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
const WRITES: [&str; 7] = [
    "timer_clock_prepare",
    "timer_objects_prepare",
    "timer_actors_prepare",
    "timer_hero_prepare",
    "timer_equipment_prepare",
    "timer_billing_prepare",
    "timer_random_prepare",
];
const POLICY: [u8; 32] = [211; 32];

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
#[derive(Clone, Serialize, Deserialize)]
enum Outcome {
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
#[derive(Clone, Serialize, Deserialize)]
enum HeroPrepareV1 {
    Unsupported,
}
#[derive(Clone, Serialize, Deserialize)]
enum EquipmentPrepareV1 {
    Unsupported,
}
#[derive(Clone, Serialize, Deserialize)]
enum BillingPrepareV1 {
    Unsupported,
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
    // Random is absent during its local preview/preparation; facts retain stable indices.
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
struct LiveCodec<T>(OwnerBridgeConfig, std::marker::PhantomData<T>);
impl<T: Domain> Schema<T> for LiveCodec<T> {
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
impl<T: Domain> NativeBehavior<T> for LiveCodec<T> {
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
        if self.items.len() != 1 || self.items.get(id).is_none() {
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
        if self.actors.len() > 8 {
            return Err(owners::Error::Unsupported);
        }
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
        // The only accepted equipment domain is empty equipment.
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

struct LiveAccounting {
    config: OwnerBridgeConfig,
    item: ItemId,
}
impl Accounting for LiveAccounting {
    fn validate(&self, p: &[ProposalView], obs: &[Observation]) -> owners::Result<()> {
        if obs.len() != 9 {
            return Err(owners::Error::Schema);
        }
        let random = p
            .iter()
            .find(|x| x.receipt.command == WRITES[6])
            .ok_or(owners::Error::Invariant)?;
        let before: RandomState = decode(&random.before, PAYLOAD)?;
        let mut expected = before.clone();
        let result = outcome(self.item, obs, &self.config, &mut expected.rng)?;
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
            item: self.item,
            attempt: result,
        };
        for x in p.iter().filter(|x| x.receipt.command != WRITES[6]) {
            macro_rules! check {
                ($state:ty) => {{
                    let old: $state = decode(&x.before, PAYLOAD)?;
                    let expected = old.prepare_intent(req.clone(), &self.config)?;
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
        // Every mandatory participant must appear exactly once.
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

struct Live {
    kernel: Kernel,
    caller: ModuleHandle,
    routes: Rc<Routes>,
    resources: Vec<Resource>,
    config: OwnerBridgeConfig,
    // Test-only stale evidence retained as immutable authenticated observations.
    stale: Rc<RefCell<Option<Vec<Observation>>>>,
    captured: Rc<RefCell<Option<Vec<Observation>>>>,
}
impl Live {
    fn install(parts: NativePartitions, config: OwnerBridgeConfig) -> Self {
        let host = Host::new(limits()).unwrap();
        host.configure_publication(publication_limits()).unwrap();
        let mut kernel = Kernel::new(kernel::Limits::default()).unwrap();
        let routes = Rc::new(Routes::Initial(RefCell::new(None)));
        let stale: Rc<RefCell<Option<Vec<Observation>>>> = Rc::new(RefCell::new(None));
        let captured: Rc<RefCell<Option<Vec<Observation>>>> = Rc::new(RefCell::new(None));
        let r = routes.clone();
        let c = config.clone();
        let old = stale.clone();
        let stash = captured.clone();
        let run: NativePort = Rc::new(move |bytes, view| {
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
            let obs = if let Some(o) = old.borrow().as_ref() {
                o.clone()
            } else {
                let fresh = read().map_err(native_refusal)?;
                *stash.borrow_mut() = Some(fresh.clone());
                fresh
            };
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
                    Rc::new(LiveAccounting {
                        config: c.clone(),
                        item: req.item,
                    }),
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
        exports.insert(RUN.into(), live_endpoint(Access::Write));
        let refuse: NativePort = Rc::new(|_, _| Err(kernel::Error::NativeRefusal));
        let mut ports: BTreeMap<String, NativePort> = exports
            .keys()
            .cloned()
            .map(|n| (n, refuse.clone()))
            .collect();
        ports.insert(RUN.into(), run);
        let home = kernel
            .register_native(manifest(HOME, exports.clone(), vec![]), &[212; 32], ports)
            .unwrap();
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
            .chain([kernel::Dependency {
                capability: cap(RUN),
                access: Access::Write,
            }])
            .collect();
        assert_eq!(exports.len(), 17);
        assert_eq!(deps.len(), 17);
        assert!(deps.len() < 64);
        let caller_run: NativePort = Rc::new(|b, v| v.call_dependency(16, b));
        let caller = kernel
            .register_native(
                manifest(
                    CALLER,
                    [(RUN.into(), live_endpoint(Access::Write))].into(),
                    deps,
                ),
                &[213; 32],
                [(RUN.into(), caller_run)].into(),
            )
            .unwrap();
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
                    Backend::Native(Rc::new(LiveCodec::<$ty>(
                        config.clone(),
                        std::marker::PhantomData,
                    ))),
                )
                .unwrap()
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
                    schema: $owner.stamp().unwrap().schema,
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
                resources.push(r.$owner.resource(READS[$index]).unwrap());
                if let Some(write_name) = WRITES.get($index) {
                    resources.push(r.$owner.resource(write_name).unwrap());
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
                .find(|r| r.key.ends_with(&format!(".{n}")))
                .unwrap()
                .clone();
            kernel
                .issue_grant(
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
                )
                .unwrap();
        }
        resources.push(host.publication_resource());
        kernel
            .issue_grant(caller, 16, scoped(Access::Write, resources.clone()))
            .unwrap();
        if let Routes::Initial(slot) = routes.as_ref() {
            *slot.borrow_mut() = Some(r);
        }
        Self {
            kernel,
            caller,
            routes,
            resources,
            config,
            stale,
            captured,
        }
    }
    fn config(&self) -> &OwnerBridgeConfig {
        &self.config
    }
    fn set_stale(&self, obs: Option<Vec<Observation>>) {
        *self.stale.borrow_mut() = obs;
    }
    fn captured_observations(&self) -> Option<Vec<Observation>> {
        self.captured.borrow().clone()
    }
    fn run_with_authority(&mut self, id: ItemId, authority: Authority) -> kernel::Result<Vec<u8>> {
        let r = self.routes.get().unwrap();
        let cleanup = r.host.cleanup_identity().unwrap();
        let lease = self
            .kernel
            .capture_pipeline(self.caller, RUN, authority.clone())?;
        let op_id = self.kernel.issue_operation()?;
        let operation =
            self.kernel
                .operation_binding_with_cleanup(op_id, lease, 1, RUN, &[cleanup])?;
        self.kernel.invoke_operation(
            &operation,
            &encode(&DueRequest { item: id }, PAYLOAD).unwrap(),
            authority,
        )
    }
    fn run(&mut self, id: ItemId) -> kernel::Result<Vec<u8>> {
        let authority = scoped(Access::Write, self.resources.clone());
        self.run_with_authority(id, authority)
    }
}

fn fixture(seed: u64, failure: bool) -> (SimulationWorld, ItemId) {
    let mut w = SimulationWorld::new_with_seed(seed);
    w.arena.actors.retain(|id, _| id == w.player_id);
    w.arena.items = Default::default();
    w.equipment = Default::default();
    w.wielded_item = None;
    w.hero.quivered_item = None;
    w.unpaid_items.clear();
    w.hero.skills.skills.clear();
    w.level = nethacked_dungeon::DungeonLevel::new_solid(Tile::Room);
    w.arena.actors[w.player_id].coord = Coord::new_unchecked(5, 5);
    w.arena.actors[w.player_id].level = 18;
    w.depth = 10;
    w.scheduler.turn = 73;
    w.rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
    let mut item = w
        .ruleset
        .create_corpse_record("troll", ItemLocation::Floor(Coord::new_unchecked(20, 10)))
        .unwrap();
    item.corpse_no_revive = failure;
    let id = w.arena.spawn_item(item);
    w.start_object_timer(
        ObjectTimerTarget::Active(id),
        ObjectTimerKind::ReviveMonster,
        0,
    )
    .unwrap();
    (w, id)
}

#[test]
fn live_transaction_matches_actual_legacy_callback() {
    for failure in [false, true] {
        let (mut oracle, id) = fixture(804, failure);
        let c = OwnerBridgeConfig::new(oracle.ruleset.clone(), oracle.ruleset_ref.clone()).unwrap();
        let parts = NativePartitions::from_native(oracle.clone());
        let mut live = Live::install(parts, c);
        let expected_events = oracle.run_object_timers();
        let result = live.run(id).unwrap();
        assert!(
            matches!(
                decode::<Outcome>(&result, PAYLOAD).unwrap(),
                Outcome::Failed { .. }
            ) == failure
        );
        let r = live.routes.get().unwrap();
        assert_eq!(r.host.history().unwrap().len(), 1);
        assert_eq!(r.host.pending().unwrap(), 0);
        assert!(!expected_events.is_empty());

        let clock: ClockState = decode(&r.clock.inspect().unwrap(), PAYLOAD).unwrap();
        let objects: ObjectsState = decode(&r.objects.inspect().unwrap(), PAYLOAD).unwrap();
        let actors: ActorsState = decode(&r.actors.inspect().unwrap(), PAYLOAD).unwrap();
        let hero: HeroState = decode(&r.hero.inspect().unwrap(), PAYLOAD).unwrap();
        let equipment: EquipmentState = decode(&r.equipment.inspect().unwrap(), PAYLOAD).unwrap();
        let billing: BillingState = decode(&r.billing.inspect().unwrap(), PAYLOAD).unwrap();
        let random: RandomState = decode(&r.random.inspect().unwrap(), PAYLOAD).unwrap();
        let geography: GeographyState = decode(&r.geography.inspect().unwrap(), PAYLOAD).unwrap();
        let session: SessionState = decode(&r.session.inspect().unwrap(), PAYLOAD).unwrap();

        assert_eq!(
            clock.object_timers.entries(),
            oracle.object_timers.entries()
        );
        assert_eq!(objects.items.len(), oracle.arena.items.len());
        assert_eq!(actors.actors.len(), oracle.arena.actors.len());
        for (item_id, item) in objects.items.iter() {
            assert_eq!(oracle.arena.items.get(item_id), Some(item));
        }
        for (actor_id, actor) in actors.actors.iter() {
            assert_eq!(oracle.arena.actors.get(actor_id), Some(actor));
        }
        let mut oracle_rng = oracle.rng.clone();
        let mut live_rng = random.rng.clone();
        for _ in 0..10 {
            assert_eq!(oracle_rng.next_u64(), live_rng.next_u64());
        }

        let reloaded_parts = NativePartitions {
            geography,
            objects,
            actors,
            hero,
            equipment,
            billing,
            clock,
            random,
            session,
        };
        let reloaded_world = reloaded_parts
            .into_native(oracle.ruleset.clone(), &oracle.ruleset_ref)
            .unwrap();
        let s1 = reloaded_world.to_save_json().unwrap();
        let s2 = oracle.to_save_json().unwrap();
        assert_eq!(s1, s2);
        assert_eq!(live.config().ruleset_ref, oracle.ruleset_ref);
    }
}

#[test]
fn stale_observation_reuse_is_refused() {
    let (oracle, id) = fixture(804, false);
    let c = OwnerBridgeConfig::new(oracle.ruleset.clone(), oracle.ruleset_ref.clone()).unwrap();
    let parts = NativePartitions::from_native(oracle.clone());
    let mut live = Live::install(parts, c);

    // Initial transaction succeeds and captures fresh observations.
    let result = live.run(id);
    assert!(result.is_ok());
    let r = live.routes.get().unwrap();
    assert_eq!(r.host.history().unwrap().len(), 1);
    assert_eq!(r.host.pending().unwrap(), 0);

    let captured = live.captured_observations().expect("captured observations");
    assert_eq!(captured.len(), 9);

    // Attempting to reuse observations whose owner stamps were incremented by the commit is refused.
    live.set_stale(Some(captured));
    let err = live.run(id).unwrap_err();
    assert_eq!(err, kernel::Error::NativeRefusal);

    // No new commit occurred, and no pending transaction was leaked.
    assert_eq!(r.host.history().unwrap().len(), 1);
    assert_eq!(r.host.pending().unwrap(), 0);
}

#[test]
fn unauthorized_or_wrong_resource_access_is_refused() {
    let (oracle, id) = fixture(804, false);
    let c = OwnerBridgeConfig::new(oracle.ruleset.clone(), oracle.ruleset_ref.clone()).unwrap();
    let parts = NativePartitions::from_native(oracle.clone());
    let mut live = Live::install(parts, c);

    // Sub-access authority (Read instead of Write on RUN endpoint) is rejected at pipeline capture.
    let read_only = scoped(Access::Read, live.resources.clone());
    let err = live.run_with_authority(id, read_only).unwrap_err();
    assert_eq!(err, kernel::Error::Authority);

    // Empty resources authority is refused during invocation (missing permits for queries).
    let empty_resources = scoped(Access::Write, vec![]);
    let err = live.run_with_authority(id, empty_resources).unwrap_err();
    assert_eq!(err, kernel::Error::NativeRefusal);

    // Authority missing the host coordinator publication resource is refused at batch coordinator permit check.
    let pub_res = live.routes.get().unwrap().host.publication_resource();
    let without_publication = scoped(
        Access::Write,
        live.resources
            .iter()
            .filter(|r| **r != pub_res)
            .cloned()
            .collect(),
    );
    let err = live
        .run_with_authority(id, without_publication)
        .unwrap_err();
    assert_eq!(err, kernel::Error::NativeRefusal);

    // Authority missing clock resources is refused when querying clock.
    let clock_write = live
        .resources
        .iter()
        .find(|r| r.key.ends_with(".timer_clock_prepare"))
        .unwrap()
        .clone();
    let without_clock = scoped(
        Access::Write,
        live.resources
            .iter()
            .filter(|r| **r != clock_write)
            .cloned()
            .collect(),
    );
    let err = live.run_with_authority(id, without_clock).unwrap_err();
    assert_eq!(err, kernel::Error::NativeRefusal);

    // State remains completely untouched: zero commits, zero pending.
    let r = live.routes.get().unwrap();
    assert_eq!(r.host.history().unwrap().len(), 0);
    assert_eq!(r.host.pending().unwrap(), 0);
}
