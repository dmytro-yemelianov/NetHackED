//! Explicit M6-I4 default Wait and action-time orchestration transport.
use super::*;
use crate::actions::command_frame::{self, BorrowedObjects};
use crate::passive_afflictions;
use crate::passive_dressing;
use crate::turns::natural_hp_heal;
use crate::GameEvent;
use nethacked_arena::{
    frozen_in_ice_box, ActorId, ActorRecord, ActorStorage, ItemId, ItemLocation, ItemStorage,
};
use nethacked_core::energy::{hero_movement_gain, monster_movement_gain, NORMAL_SPEED};
use nethacked_core::inventory::EncumbranceTier;
use nethacked_dungeon::DungeonLevel;
use nethacked_kernel::ModuleHandle;
use nethacked_types::{BranchId, Buc, ItemClass, Locale};
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use wand::OwnerWitness;

pub const HOME: &str = "wait-home-i4-v1";
pub const CALLER: &str = "wait-caller-i4-v1";
pub const PREVIEW: &str = "wait_preview_i4";
pub const RUN: &str = "wait_run_i4";

pub const READS: [&str; 9] = [
    "wait_clock_read",
    "wait_objects_read",
    "wait_actors_read",
    "wait_hero_read",
    "wait_equipment_read",
    "wait_billing_read",
    "wait_random_read",
    "wait_geography_read",
    "wait_session_read",
];

pub const WRITES: [&str; 7] = [
    "wait_clock_prepare",
    "wait_objects_prepare",
    "wait_actors_prepare",
    "wait_hero_prepare",
    "wait_equipment_prepare",
    "wait_billing_prepare",
    "wait_random_prepare",
];

pub const POLICY: [u8; 32] = [74; 32];

fn native_refusal(_: impl std::fmt::Debug) -> kernel::Error {
    kernel::Error::NativeRefusal
}

fn native_refusal_step(_: &str, _: impl std::fmt::Debug) -> kernel::Error {
    kernel::Error::NativeRefusal
}

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
    WRONG_RESOURCE.with(|c| c.set(wrong));
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WaitRequest {
    pub guards: Vec<OwnerWitness>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WaitReply {
    pub events: Vec<GameEvent>,
    pub spent_time: bool,
    pub rounds: u32,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ClockFacts {
    pub hero_energy: u32,
    pub hero_speed: u32,
    pub turn: u64,
    pub object_timers: crate::object_timers::ObjectTimerQueue,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ObjectsFacts {
    pub items: ItemStorage,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ActorsFacts {
    pub actors: ActorStorage,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct HeroFacts {
    pub player_id: ActorId,
    pub hero: nethacked_types::Hero,
    pub player_nutrition: i32,
    pub player_luck: i32,
    pub prayer_timeout: u32,
    pub divine_protection: u32,
    pub hero_race: nethacked_data::RaceId,
    pub alignment_record: i32,
    pub progress: crate::progress::HeroProgress,
    pub genocide_registry: nethacked_types::GenocideRegistry,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct EquipmentFacts {
    pub equipment: crate::actions::equipment::EquipmentState,
    pub wielded_item: Option<ItemId>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct BillingFacts;

#[derive(Clone, Serialize, Deserialize)]
pub struct GeographyFacts {
    pub level: DungeonLevel,
    pub branch: BranchId,
    pub depth: usize,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SessionFacts {
    pub role_name: String,
    pub locale: Locale,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClockOutcome {
    pub hero_energy: u32,
    pub turn: u64,
    pub object_timers: crate::object_timers::ObjectTimerQueue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectsOutcome {
    pub items: ItemStorage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorsOutcome {
    pub actors: ActorStorage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeroOutcome {
    pub hero: nethacked_types::Hero,
    pub player_nutrition: i32,
    pub player_luck: i32,
    pub prayer_timeout: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquipmentOutcome {
    pub equipment: crate::actions::equipment::EquipmentState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaitOutcome {
    pub spent_time: bool,
    pub rounds: u32,
    pub clock: Option<ClockOutcome>,
    pub actors: Option<ActorsOutcome>,
    pub hero: Option<HeroOutcome>,
    pub objects: Option<ObjectsOutcome>,
    pub equipment: Option<EquipmentOutcome>,
    pub events: Vec<GameEvent>,
}

impl WaitOutcome {
    pub fn empty() -> Self {
        Self {
            spent_time: false,
            rounds: 0,
            clock: None,
            actors: None,
            hero: None,
            objects: None,
            equipment: None,
            events: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PrepareV1 {
    pub(super) outcome: WaitOutcome,
}

fn compute_hero_encumbrance(
    h: &HeroFacts,
    items: &ItemStorage,
    player_actor: &ActorRecord,
) -> EncumbranceTier {
    let levitating = player_actor.intrinsics.levitation;
    let str_val = h.progress.attr_base[crate::progress::A_STR];
    let con_val = h.progress.attr_base[crate::progress::A_CON].clamp(3, 25);
    let weight_cap = nethacked_core::inventory::weight_cap(
        nethacked_core::inventory::capacity_strength(str_val),
        con_val,
        levitating,
        0,
    );
    let carried_weight = nethacked_arena::carried_weight(items, h.player_id);
    nethacked_core::inventory::encumbrance_tier(carried_weight, weight_cap)
}

fn hero_has_amulet(items: &ItemStorage, player_id: ActorId) -> bool {
    items.iter().any(|(_, it)| {
        it.location == ItemLocation::CarriedBy(player_id)
            && it.class == ItemClass::Amulet
            && it.name.eq_ignore_ascii_case("Amulet of Yendor")
    })
}

fn find_carried_luckstone(items: &ItemStorage, player_id: ActorId) -> Option<Buc> {
    items.iter().find_map(|(_, it)| {
        if it.location == ItemLocation::CarriedBy(player_id)
            && it.name.eq_ignore_ascii_case("luckstone")
        {
            Some(it.buc)
        } else {
            None
        }
    })
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

pub(crate) fn outcome(
    obs: &[Observation],
    config: &OwnerBridgeConfig,
    rng: &mut rand_chacha::ChaCha8Rng,
) -> owners::Result<WaitOutcome> {
    let mut clk: ClockFacts = fact(obs, 0)?;
    let mut obj: ObjectsFacts = fact(obs, 1)?;
    let mut a: ActorsFacts = fact(obs, 2)?;
    let mut h: HeroFacts = fact(obs, 3)?;
    let mut eq: EquipmentFacts = fact(obs, 4)?;
    let geo: GeographyFacts = fact(obs, 7)?;
    let sess: SessionFacts = fact(obs, 8)?;

    let Some(mut hero_actor) = a.actors.get(h.player_id).cloned() else {
        return Err(owners::Error::Unsupported);
    };
    if hero_actor.is_dead || hero_actor.hp == 0 {
        return Ok(WaitOutcome::empty());
    }

    // 1. Initial equipment reconciliation & AC refresh
    let borrowed = BorrowedObjects {
        items: &obj.items,
        rules: &config.ruleset,
    };
    eq.equipment = command_frame::reconcile(&eq.equipment, &borrowed, h.player_id);
    let initial_ac =
        command_frame::armor_class(&eq.equipment, &borrowed, h.player_id, h.divine_protection);
    hero_actor.ac = initial_ac;

    let energy_before = clk.hero_energy;
    let turn_before = clk.turn;

    // 2. Hero acts
    clk.hero_energy = clk.hero_energy.saturating_sub(NORMAL_SPEED);
    let spent_time = clk.hero_energy != energy_before;

    if !spent_time {
        if let Some(p) = a.actors.get_mut(h.player_id) {
            *p = hero_actor;
        }
        return Ok(WaitOutcome {
            spent_time: false,
            rounds: 0,
            clock: Some(ClockOutcome {
                hero_energy: clk.hero_energy,
                turn: clk.turn,
                object_timers: clk.object_timers,
            }),
            actors: Some(ActorsOutcome { actors: a.actors }),
            hero: Some(HeroOutcome {
                hero: h.hero,
                player_nutrition: h.player_nutrition,
                player_luck: h.player_luck,
                prayer_timeout: h.prayer_timeout,
            }),
            objects: Some(ObjectsOutcome { items: obj.items }),
            equipment: Some(EquipmentOutcome {
                equipment: eq.equipment,
            }),
            events: Vec::new(),
        });
    }

    let mut events = Vec::new();
    let mut rounds = 0;

    // 3. Process turn ticks loop
    loop {
        if hero_actor.is_dead || hero_actor.hp == 0 {
            break;
        }

        // Monster snapshot pass
        let mut snapshot: Vec<ActorId> = a
            .actors
            .iter()
            .filter_map(|(id, m)| {
                if !m.is_player && !m.is_dead && m.hp > 0 && m.movement >= NORMAL_SPEED {
                    Some(id)
                } else {
                    None
                }
            })
            .collect();
        snapshot.sort();

        for mon_id in snapshot {
            if hero_actor.is_dead || hero_actor.hp == 0 {
                break;
            }
            let can_move = a
                .actors
                .get(mon_id)
                .is_some_and(|m| !m.is_dead && m.hp > 0 && m.movement >= NORMAL_SPEED);
            if !can_move {
                continue;
            }

            // Sync hero_actor into a.actors before monster computes turn
            if let Some(p) = a.actors.get_mut(h.player_id) {
                *p = hero_actor.clone();
            }

            let mon_clk = super::monster_ticks::ClockFacts { turn: clk.turn };
            let mon_obj = super::monster_ticks::ObjectFacts {
                items: obj.items.clone(),
            };
            let mon_act = super::monster_ticks::ActorFacts {
                actors: a.actors.clone(),
            };
            let mon_hero = super::monster_ticks::HeroFacts {
                player: h.player_id,
                alignment_record: h.alignment_record,
                luck: h.player_luck,
                genocide: h.genocide_registry.clone(),
            };
            let mon_geo = super::monster_ticks::GeographyFacts {
                level: geo.level.clone(),
                branch: geo.branch,
                depth: geo.depth,
            };
            let mon_sess = super::monster_ticks::SessionFacts {
                role: sess.role_name.clone(),
                locale: sess.locale,
            };

            let monster_outcome = super::monster_ticks::compute_monster_outcome(
                mon_id, &mon_clk, &mon_obj, &mon_act, &mon_hero, &mon_geo, &mon_sess, config, rng,
            )?;

            if let Some(m) = a.actors.get_mut(mon_id) {
                m.movement = m.movement.saturating_sub(monster_outcome.movement_spent);
                if let Some(c) = monster_outcome.new_coord {
                    m.coord = c;
                }
                if let Some(hp) = monster_outcome.actor_hp {
                    m.hp = hp;
                }
                if let Some(flee) = monster_outcome.is_fleeing {
                    m.is_fleeing = flee;
                }
                if monster_outcome.is_dead {
                    m.is_dead = true;
                    m.hp = 0;
                }
            }
            if let Some(h_hp) = monster_outcome.hero_hp {
                hero_actor.hp = h_hp;
            }
            if monster_outcome.hero_dead {
                hero_actor.is_dead = true;
                hero_actor.hp = 0;
            }
            for spawn in monster_outcome.spawned_actors {
                a.actors.insert(spawn);
            }
            events.extend(monster_outcome.events);
        }

        if hero_actor.is_dead || hero_actor.hp == 0 || clk.hero_energy >= NORMAL_SPEED {
            break;
        }

        if a.actors.iter().any(|(_, actor)| {
            !actor.is_player && !actor.is_dead && actor.hp > 0 && actor.movement >= NORMAL_SPEED
        }) {
            continue;
        }

        // New world turn begins!
        rounds += 1;

        // Monster distress
        for (_, actor) in a.actors.iter_mut() {
            if !actor.is_player && !actor.is_dead && actor.hp > 0 {
                actor.mspec_used = actor.mspec_used.saturating_sub(1);
            }
        }

        // Monster movement gain
        for (_, actor) in a.actors.iter_mut() {
            if !actor.is_player && !actor.is_dead && actor.hp > 0 {
                let gain = monster_movement_gain(actor.speed, |bound| rng.random_range(0..bound));
                actor.movement = actor.movement.saturating_add(gain);
            }
        }

        // Hero movement gain
        let burden = compute_hero_encumbrance(&h, &obj.items, &hero_actor);
        let properties = hero_actor.intrinsics;
        let gain = hero_movement_gain(
            clk.hero_speed,
            properties.fast,
            properties.very_fast,
            burden,
            |bound| rng.random_range(0..bound),
        );
        clk.hero_energy = clk.hero_energy.saturating_add(gain);
        clk.turn += 1;

        // Advance corpse ages
        let advancing: Vec<_> = obj
            .items
            .iter()
            .filter_map(|(id, item)| {
                (item.is_corpse() && !frozen_in_ice_box(item, |parent| obj.items.get(parent)))
                    .then_some(id)
            })
            .collect();
        for id in advancing {
            if let Some(item) = obj.items.get_mut(id) {
                item.corpse_age = item.corpse_age.saturating_add(1);
            }
        }

        events.push(GameEvent::TurnAdvanced { turn: clk.turn });

        // Hero afflictions
        let (affliction_result, _, aff_events) =
            passive_afflictions::recovery(&mut h.hero.afflictions);
        events.extend(aff_events);
        if let Some(fatal_msg) = passive_afflictions::fatal_message(&affliction_result, sess.locale)
        {
            hero_actor.is_dead = true;
            hero_actor.hp = 0;
            events.push(fatal_msg);
        }

        // Object timers
        while let Some(timer) = clk.object_timers.pop_due_active(clk.turn) {
            events.push(GameEvent::ObjectTimerExpired { timer });
            if timer.kind == crate::object_timers::ObjectTimerKind::ReviveMonster {
                if let crate::object_timers::ObjectTimerTarget::Active(id) = timer.target {
                    let hero_alignment = a
                        .actors
                        .get(h.player_id)
                        .map(|p| p.alignment)
                        .unwrap_or(nethacked_types::Alignment::Neutral);
                    let input = crate::revival_law::RevivalInput {
                        ruleset: &config.ruleset,
                        items: &obj.items,
                        actors: &a.actors,
                        level: &geo.level,
                        genocide_registry: &h.genocide_registry,
                        player: h.player_id,
                        branch: geo.branch,
                        depth: geo.depth,
                        hero_alignment,
                        hero_race: h.hero_race,
                        alignment_record: h.alignment_record,
                        role_name: &sess.role_name,
                    };
                    match crate::revival_law::ordinary_troll_attempt(id, &input, rng) {
                        crate::revival_law::RevivalAttempt::Revived { actor } => {
                            obj.items.remove(id);
                            let name = actor.name.clone();
                            a.actors.insert(actor);
                            events.push(GameEvent::LogMessage {
                                text: format!("The {name} revives!"),
                            });
                        }
                        crate::revival_law::RevivalAttempt::Failed { rot_delay } => {
                            let due = clk.turn.checked_add(rot_delay);
                            let _ = clk.object_timers.start_at_preserving(
                                timer.target,
                                crate::object_timers::ObjectTimerKind::RotCorpse,
                                due,
                                true,
                            );
                        }
                        crate::revival_law::RevivalAttempt::Unsupported => {}
                    }
                }
            } else if timer.kind == crate::object_timers::ObjectTimerKind::RotCorpse {
                if let crate::object_timers::ObjectTimerTarget::Active(id) = timer.target {
                    obj.items.remove(id);
                    events.push(GameEvent::LogMessage {
                        text: "The corpse rots away completely.".into(),
                    });
                }
            }
        }

        // Natural HP regeneration (moved is false for Wait)
        let actor_info = crate::passive_regeneration::Actor {
            hp: hero_actor.hp,
            max_hp: hero_actor.max_hp,
            level: hero_actor.level,
            dead: hero_actor.is_dead,
            levitating: hero_actor.intrinsics.levitation,
        };
        if crate::passive_regeneration::eligible(
            Some(actor_info),
            h.hero.polymorph.is_some(),
            false,
            0,
            0,
            None,
        ) {
            let level = hero_actor.level;
            let con = h.progress.attr_base[crate::progress::A_CON];
            let heal = natural_hp_heal(level, con, |bound| rng.random_range(0..bound));
            hero_actor.hp = hero_actor.hp.saturating_add(heal).min(hero_actor.max_hp);
        }

        // Passive metabolism
        let actor_summary = Some(crate::passive_metabolism::Actor {
            hp: hero_actor.hp,
            dead: hero_actor.is_dead,
        });
        let met_out = crate::passive_metabolism::tick(
            h.player_nutrition,
            actor_summary,
            clk.turn,
            sess.locale,
        );
        h.player_nutrition = met_out.nutrition;
        if let Some(next) = met_out.actor {
            hero_actor.hp = next.hp;
            hero_actor.is_dead = next.dead;
        }
        events.extend(met_out.events);

        // Dressing tick
        let borrowed = BorrowedObjects {
            items: &obj.items,
            rules: &config.ruleset,
        };
        let dress_result = passive_dressing::tick(
            &eq.equipment,
            &borrowed,
            h.player_id,
            !hero_actor.is_dead,
            h.divine_protection,
        );
        eq.equipment = dress_result.equipment;
        if let Some(ac) = dress_result.ac {
            hero_actor.ac = ac;
        }
        events.extend(dress_result.events);

        if clk.hero_energy >= NORMAL_SPEED {
            break;
        }
    }

    // 4. Post-loop
    h.prayer_timeout = nethacked_core::religion::tick_prayer_timeout(h.prayer_timeout);

    let luck_period =
        nethacked_core::luck_decay_period(hero_has_amulet(&obj.items, h.player_id), false);
    if clk.turn != turn_before && clk.turn > 0 && clk.turn % luck_period == 0 {
        let luckstone = find_carried_luckstone(&obj.items, h.player_id);
        h.player_luck = nethacked_core::mines::step_luck_decay(h.player_luck, 0, luckstone);
    }

    // Final equipment reconciliation and derived AC
    let borrowed = BorrowedObjects {
        items: &obj.items,
        rules: &config.ruleset,
    };
    eq.equipment = command_frame::reconcile(&eq.equipment, &borrowed, h.player_id);
    hero_actor.ac =
        command_frame::armor_class(&eq.equipment, &borrowed, h.player_id, h.divine_protection);

    if let Some(p) = a.actors.get_mut(h.player_id) {
        *p = hero_actor;
    }

    Ok(WaitOutcome {
        spent_time,
        rounds,
        clock: Some(ClockOutcome {
            hero_energy: clk.hero_energy,
            turn: clk.turn,
            object_timers: clk.object_timers,
        }),
        actors: Some(ActorsOutcome { actors: a.actors }),
        hero: Some(HeroOutcome {
            hero: h.hero,
            player_nutrition: h.player_nutrition,
            player_luck: h.player_luck,
            prayer_timeout: h.prayer_timeout,
        }),
        objects: Some(ObjectsOutcome { items: obj.items }),
        equipment: Some(EquipmentOutcome {
            equipment: eq.equipment,
        }),
        events,
    })
}

trait Domain: Clone + Serialize + 'static {
    const INDEX: usize;
    fn read(&self, obs: &[Observation], c: &OwnerBridgeConfig) -> owners::Result<Vec<u8>>;
    fn prepare(
        &self,
        _obs: &[Observation],
        _c: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        Err(owners::Error::Unsupported)
    }
    fn prepare_intent(
        &self,
        _: PrepareV1,
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        Err(owners::Error::Unsupported)
    }
}

struct WaitCodec<T>(OwnerBridgeConfig, std::marker::PhantomData<T>);
impl<T: Domain> Schema<T> for WaitCodec<T> {
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
        _request: &[u8],
        obs: &[Observation],
    ) -> owners::Result<Vec<u8>> {
        if name != READS[T::INDEX] {
            return Err(owners::Error::Unsupported);
        }
        s.read(obs, &self.0)
    }
}

impl<T: Domain> NativeBehavior<T> for WaitCodec<T> {
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
            return s.prepare(obs, &self.0);
        }
        if !obs.is_empty() {
            return Err(owners::Error::Schema);
        }
        let req: PrepareV1 = decode(request, PAYLOAD)?;
        s.prepare_intent(req, &self.0)
    }
}

fn proposed<T>(next: T, events: Vec<Vec<u8>>, result: &WaitOutcome) -> owners::Result<Proposal<T>> {
    Ok(Proposal {
        next,
        events,
        reply: encode(result, PAYLOAD)?,
    })
}

fn preview(obs: &[Observation]) -> owners::Result<WaitOutcome> {
    fact(obs, 6)
}

impl Domain for ClockState {
    const INDEX: usize = 0;
    fn read(&self, _: &[Observation], _: &OwnerBridgeConfig) -> owners::Result<Vec<u8>> {
        encode(
            &ClockFacts {
                hero_energy: self.scheduler.hero_energy,
                hero_speed: self.scheduler.hero_speed,
                turn: self.scheduler.turn,
                object_timers: self.object_timers.clone(),
            },
            PAYLOAD,
        )
    }
    fn prepare_intent(
        &self,
        req: PrepareV1,
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        let mut next = self.clone();
        if let Some(c) = &req.outcome.clock {
            next.scheduler.hero_energy = c.hero_energy;
            next.scheduler.turn = c.turn;
            next.object_timers = c.object_timers.clone();
        }
        proposed(next, vec![], &req.outcome)
    }
}

impl Domain for ObjectsState {
    const INDEX: usize = 1;
    fn read(&self, _: &[Observation], _: &OwnerBridgeConfig) -> owners::Result<Vec<u8>> {
        encode(
            &ObjectsFacts {
                items: self.items.clone(),
            },
            PAYLOAD,
        )
    }
    fn prepare_intent(
        &self,
        req: PrepareV1,
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        let mut next = self.clone();
        if let Some(o) = &req.outcome.objects {
            next.items = o.items.clone();
            for stored in &mut next.stored_items {
                for (_, item) in stored.iter_mut() {
                    if item.is_corpse() {
                        item.corpse_age = item.corpse_age.saturating_add(req.outcome.rounds);
                    }
                }
            }
        }
        proposed(next, vec![], &req.outcome)
    }
}

impl Domain for ActorsState {
    const INDEX: usize = 2;
    fn read(&self, _: &[Observation], _: &OwnerBridgeConfig) -> owners::Result<Vec<u8>> {
        encode(
            &ActorsFacts {
                actors: self.actors.clone(),
            },
            PAYLOAD,
        )
    }
    fn prepare_intent(
        &self,
        req: PrepareV1,
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        let mut next = self.clone();
        if let Some(a) = &req.outcome.actors {
            next.actors = a.actors.clone();
        }
        let events = req
            .outcome
            .events
            .iter()
            .map(|e| encode(e, PAYLOAD))
            .collect::<owners::Result<Vec<_>>>()?;
        proposed(next, events, &req.outcome)
    }
}

impl Domain for HeroState {
    const INDEX: usize = 3;
    fn read(&self, _: &[Observation], _: &OwnerBridgeConfig) -> owners::Result<Vec<u8>> {
        encode(
            &HeroFacts {
                player_id: self.player_id,
                hero: self.hero.clone(),
                player_nutrition: self.player_nutrition,
                player_luck: self.player_luck,
                prayer_timeout: self.divine_state.prayer_timeout,
                divine_protection: self.divine_protection,
                hero_race: self.hero_race,
                alignment_record: self.alignment_record,
                progress: self.progress.clone(),
                genocide_registry: self.genocide_registry.clone(),
            },
            PAYLOAD,
        )
    }
    fn prepare_intent(
        &self,
        req: PrepareV1,
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        let mut next = self.clone();
        if let Some(h) = &req.outcome.hero {
            next.hero = h.hero.clone();
            next.player_nutrition = h.player_nutrition;
            next.player_luck = h.player_luck;
            next.divine_state.prayer_timeout = h.prayer_timeout;
        }
        proposed(next, vec![], &req.outcome)
    }
}

impl Domain for EquipmentState {
    const INDEX: usize = 4;
    fn read(&self, _: &[Observation], _: &OwnerBridgeConfig) -> owners::Result<Vec<u8>> {
        encode(
            &EquipmentFacts {
                equipment: self.equipment.clone(),
                wielded_item: self.wielded_item,
            },
            PAYLOAD,
        )
    }
    fn prepare_intent(
        &self,
        req: PrepareV1,
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        let mut next = self.clone();
        if let Some(eq) = &req.outcome.equipment {
            next.equipment = eq.equipment.clone();
        }
        proposed(next, vec![], &req.outcome)
    }
}

impl Domain for BillingState {
    const INDEX: usize = 5;
    fn read(&self, _: &[Observation], _: &OwnerBridgeConfig) -> owners::Result<Vec<u8>> {
        encode(&BillingFacts, PAYLOAD)
    }
    fn prepare_intent(
        &self,
        req: PrepareV1,
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        proposed(self.clone(), vec![], &req.outcome)
    }
}

impl Domain for RandomState {
    const INDEX: usize = 6;
    fn read(&self, obs: &[Observation], c: &OwnerBridgeConfig) -> owners::Result<Vec<u8>> {
        encode(&outcome(obs, c, &mut self.rng.clone())?, PAYLOAD)
    }
    fn prepare(
        &self,
        obs: &[Observation],
        c: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        let mut next = self.clone();
        let result = outcome(obs, c, &mut next.rng)?;
        proposed(next, vec![], &result)
    }
}

impl Domain for GeographyState {
    const INDEX: usize = 7;
    fn read(&self, _: &[Observation], _: &OwnerBridgeConfig) -> owners::Result<Vec<u8>> {
        encode(
            &GeographyFacts {
                level: self.level.clone(),
                branch: self.current_branch,
                depth: self.depth,
            },
            PAYLOAD,
        )
    }
}

impl Domain for SessionState {
    const INDEX: usize = 8;
    fn read(&self, _: &[Observation], _: &OwnerBridgeConfig) -> owners::Result<Vec<u8>> {
        encode(
            &SessionFacts {
                role_name: self.role_name.clone(),
                locale: self.locale,
            },
            PAYLOAD,
        )
    }
}

pub(super) struct WaitAccounting(pub(super) OwnerBridgeConfig);

impl Accounting for WaitAccounting {
    fn validate(&self, p: &[ProposalView], obs: &[Observation]) -> owners::Result<()> {
        if obs.len() != 9 {
            return Err(owners::Error::Schema);
        }
        let random = p
            .iter()
            .find(|x| x.receipt.command == WRITES[6])
            .ok_or(owners::Error::Invariant)?;
        let before: RandomState = decode(&random.before, PAYLOAD)?;
        let result: WaitOutcome = decode(&random.reply, PAYLOAD)?;
        let mut expected = before.clone();
        let expected_outcome = outcome(obs, &self.0, &mut expected.rng)?;
        if encode(&expected, PAYLOAD)? != random.proposed
            || encode(&expected_outcome, PAYLOAD)? != random.reply
            || encode(&expected_outcome, PAYLOAD)?
                != fact::<WaitOutcome>(obs, 6).and_then(|r| encode(&r, PAYLOAD))?
            || !random.events.is_empty()
        {
            return Err(owners::Error::Invariant);
        }

        let expected_count = 6;
        if p.len() != expected_count {
            return Err(owners::Error::Invariant);
        }
        let req = PrepareV1 {
            outcome: result.clone(),
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
                "wait_clock_prepare" => check!(ClockState),
                "wait_objects_prepare" => check!(ObjectsState),
                "wait_actors_prepare" => check!(ActorsState),
                "wait_hero_prepare" => check!(HeroState),
                "wait_equipment_prepare" => check!(EquipmentState),
                _ => return Err(owners::Error::Invariant),
            }
        }
        for n in [
            WRITES[0], WRITES[1], WRITES[2], WRITES[3], WRITES[4], WRITES[6],
        ] {
            if p.iter().filter(|x| x.receipt.command == n).count() != 1 {
                return Err(owners::Error::Invariant);
            }
        }
        Ok(())
    }
}

fn identity() -> op::ArtifactIdentity {
    artifact("native-wait-accounting-i4", 94)
}

pub(super) fn capture(
    a: &mut op::AccountingCaptureCatalog,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if c.wait_action {
        a.register(identity(), POLICY, Rc::new(WaitAccounting(c.clone())))?;
    }
    Ok(())
}

pub(super) fn restore(
    a: &mut op::AccountingRestoreCatalog,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if c.wait_action {
        let cc = c.clone();
        a.allow(identity(), POLICY, move || {
            Ok(Rc::new(WaitAccounting(cc.clone())))
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

pub(super) struct WaitTransport {
    kernel: Kernel,
    caller: ModuleHandle,
    routes: Rc<Routes>,
    resources: Vec<Resource>,
}

impl WaitTransport {
    pub(super) fn install(
        parts: NativePartitions,
        config: OwnerBridgeConfig,
    ) -> Result<Self, OwnerBridgeError> {
        let host = Host::new(limits())?;
        host.configure_publication(owners::PublicationLimits {
            retained_bytes: 32 * PAYLOAD,
            ..publication_limits()
        })?;
        let mut kernel = Kernel::new(kernel::Limits::default())?;
        let routes = Rc::new(Routes::Initial(RefCell::new(None)));
        let r = routes.clone();
        let c = config.clone();
        let r_preview = routes.clone();
        let c_preview = config.clone();

        let preview_port: NativePort = Rc::new(move |bytes, view| {
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
                &obs,
                &c_preview,
                &mut rand_chacha::ChaCha8Rng::seed_from_u64(0),
            )
            .map_err(native_refusal)?;
            let guards = obs
                .iter()
                .map(|o| wand::OwnerWitness::from(o.stamp()))
                .collect();
            encode(&WaitRequest { guards }, PAYLOAD).map_err(native_refusal)
        });

        let run_port: NativePort = Rc::new(move |bytes, view| {
            let _req: WaitRequest = decode(bytes, PAYLOAD).map_err(native_refusal)?;
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
            let obs = read().map_err(|e| native_refusal_step("read", e))?;

            let result = preview(&obs).map_err(|e| native_refusal_step("preview", e))?;
            let intent = encode(
                &PrepareV1 {
                    outcome: result.clone(),
                },
                PAYLOAD,
            )
            .map_err(|e| native_refusal_step("intent encode", e))?;
            let other: Vec<_> = obs
                .iter()
                .filter(|o| o.query() != READS[6])
                .cloned()
                .collect();
            let candidates = vec![
                r.clock
                    .prepare(WRITES[0], &intent, vec![], view)
                    .map_err(|e| native_refusal_step("clock.prepare", e))?
                    .erase(),
                r.objects
                    .prepare(WRITES[1], &intent, vec![], view)
                    .map_err(|e| native_refusal_step("objects.prepare", e))?
                    .erase(),
                r.actors
                    .prepare(WRITES[2], &intent, vec![], view)
                    .map_err(|e| native_refusal_step("actors.prepare", e))?
                    .erase(),
                r.hero
                    .prepare(WRITES[3], &intent, vec![], view)
                    .map_err(|e| native_refusal_step("hero.prepare", e))?
                    .erase(),
                r.equipment
                    .prepare(WRITES[4], &intent, vec![], view)
                    .map_err(|e| native_refusal_step("equipment.prepare", e))?
                    .erase(),
                r.random
                    .prepare(WRITES[6], bytes, other, view)
                    .map_err(|e| native_refusal_step("random.prepare", e))?
                    .erase(),
            ];
            let writes = vec![
                r.clock.id(),
                r.objects.id(),
                r.actors.id(),
                r.hero.id(),
                r.equipment.id(),
                r.random.id(),
            ];

            let batch = r
                .host
                .batch(
                    candidates,
                    obs,
                    &writes,
                    writes.clone(),
                    POLICY,
                    Rc::new(WaitAccounting(c.clone())),
                    cap(RUN),
                    view,
                )
                .map_err(|e| native_refusal_step("host.batch", e))?;
            if let Err(e) = batch.commit(view) {
                batch
                    .discard()
                    .map_err(|e| native_refusal_step("batch.discard", e))?;
                return Err(native_refusal_step("batch.commit", e));
            }
            encode(
                &WaitReply {
                    events: result.events,
                    spent_time: result.spent_time,
                    rounds: result.rounds,
                },
                PAYLOAD,
            )
            .map_err(|e| native_refusal_step("encode result", e))
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
            kernel.register_native(manifest(HOME, exports.clone(), vec![]), &[96; 32], ports)?;
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
            &[97; 32],
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
                        schema: [index as u8 + 190; 32],
                        ports,
                    },
                    $state,
                    Backend::Native(Rc::new(WaitCodec::<$ty>(
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

    pub(super) fn preview(&mut self) -> Result<WaitRequest, OwnerBridgeError> {
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
        let reply = self
            .kernel
            .invoke_operation(&operation, &encode(&(), PAYLOAD)?, authority);
        let _ = self.kernel.release_pipeline(lease);
        decode(&reply?, PAYLOAD).map_err(Into::into)
    }

    pub(super) fn execute(
        &mut self,
        request: WaitRequest,
    ) -> Result<(WaitReply, NativePartitions), OwnerBridgeError> {
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
        let reply =
            self.kernel
                .invoke_operation(&operation, &encode(&request, PAYLOAD)?, authority);
        let _ = self.kernel.release_pipeline(lease);
        let outcome: WaitReply = decode(&reply?, PAYLOAD)?;

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

        Ok((outcome, updated))
    }
}

impl OwnerBridge {
    fn wait_session_witnesses(&self) -> Result<Vec<wand::OwnerWitness>, OwnerBridgeError> {
        self.session.with_runtime(&self.credential.borrow(), |b| {
            Ok(bridge_stamps(b)?
                .iter()
                .map(wand::OwnerWitness::from)
                .collect())
        })?
    }

    pub fn prepare_wait(&self) -> Result<WaitRequest, OwnerBridgeError> {
        if !self.config.wait_action {
            return Err(owners::Error::Unsupported.into());
        }
        let parts = self.partitions()?;
        let mut transport = WaitTransport::install(parts, self.config.clone())?;
        let _ = transport.preview()?;
        let guards = self.wait_session_witnesses()?;
        Ok(WaitRequest { guards })
    }

    pub fn execute_wait(&self, request: WaitRequest) -> Result<WaitReply, OwnerBridgeError> {
        if !self.config.wait_action {
            return Err(owners::Error::Unsupported.into());
        }
        let current_witnesses = self.wait_session_witnesses()?;
        if request.guards != current_witnesses {
            #[cfg(test)]
            STALE_REFUSALS.with(|x| x.set(x.get() + 1));
            return Err(owners::Error::Stale.into());
        }
        let parts = self.partitions()?;
        let mut transport = WaitTransport::install(parts, self.config.clone())?;
        let (reply, updated) = transport.execute(request)?;
        let source = Self::install(updated, self.config.clone())?;
        let wire = source.capture()?;
        drop(source);
        self.restore(&wire)?;
        Ok(reply)
    }

    pub fn step_wait(&self) -> Result<WaitReply, OwnerBridgeError> {
        let req = self.prepare_wait()?;
        self.execute_wait(req)
    }
}
