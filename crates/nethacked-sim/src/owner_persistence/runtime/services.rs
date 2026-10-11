//! Explicit M6-I8 services dispatch: Cast, Pray, Sacrifice, Donate, Pay,
//! PriceCheck, Wish, Mount and Dismount.
use super::*;
use crate::actions::command_frame::{self, BorrowedObjects};
use crate::passive_afflictions;
use crate::passive_dressing;
use crate::peace::{is_shopkeeper, is_temple_priest, monnam_article};
use crate::turns::natural_hp_heal;
use crate::GameEvent;
use nethacked_arena::{
    frozen_in_ice_box, ActorId, ActorRecord, ActorStorage, ItemId, ItemLocation, ItemRecord,
    ItemStorage,
};
use nethacked_core::energy::{
    hero_movement_gain, monster_movement_gain, SchedulerState, NORMAL_SPEED,
};
use nethacked_core::engraving::{is_elbereth_ward_active, onscary_exempt, Engraving};
use nethacked_core::inventory::EncumbranceTier;
use nethacked_core::{QuestProgress, SpellKind};
use nethacked_dungeon::DungeonLevel;
use nethacked_kernel::ModuleHandle;
use nethacked_types::{
    Alignment, AscensionOutcome, BranchId, Buc, Coord, Direction, DivineState, ItemClass, Locale,
    MountState, SacrificeResult, Tile, TrapRecord,
};
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use wand::OwnerWitness;

pub const HOME: &str = "services-home-i8-v1";
pub const CALLER: &str = "services-caller-i8-v1";
pub const PREVIEW: &str = "services_preview_i8";
pub const RUN: &str = "services_run_i8";

pub const READS: [&str; 9] = [
    "svc_clock_read",
    "svc_objects_read",
    "svc_actors_read",
    "svc_hero_read",
    "svc_equipment_read",
    "svc_billing_read",
    "svc_random_read",
    "svc_geography_read",
    "svc_session_read",
];

pub const WRITES: [&str; 8] = [
    "svc_clock_prepare",
    "svc_objects_prepare",
    "svc_actors_prepare",
    "svc_hero_prepare",
    "svc_equipment_prepare",
    "svc_billing_prepare",
    "svc_geography_prepare",
    "svc_random_prepare",
];

pub const POLICY: [u8; 32] = [73; 32];

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ServiceAction {
    Cast { spell_index: usize, dir: Direction },
    Pray,
    Sacrifice(usize),
    Donate(u32),
    Pay,
    PriceCheck(usize),
    Wish(String),
    Mount(ActorId),
    Dismount,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServicesRequest {
    pub action: ServiceAction,
    pub guards: Vec<OwnerWitness>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServicesReply {
    pub events: Vec<GameEvent>,
    pub spent_time: bool,
    pub rounds: u32,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ClockFacts {
    pub scheduler: SchedulerState,
    pub object_timers: crate::object_timers::ObjectTimerQueue,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ObjectsFacts {
    pub items: ItemStorage,
    pub artifact_history: BTreeSet<String>,
    pub stored_items: Vec<Vec<(ItemId, ItemRecord)>>,
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
    pub player_pw: u32,
    pub player_max_pw: u32,
    pub known_spells: Vec<(SpellKind, u32)>,
    pub divine_favor: i32,
    pub prayer_timeout: u32,
    pub divine_gift_count: u32,
    pub divine_protection: u32,
    pub priest_cheapskate: u32,
    pub player_luck: i32,
    pub hero_race: nethacked_data::RaceId,
    pub alignment_record: i32,
    pub progress: crate::progress::HeroProgress,
    pub genocide_registry: nethacked_types::GenocideRegistry,
    pub conducts: nethacked_types::ConductTracker,
    pub candelabrum_state: nethacked_core::CandelabrumState,
    pub ritual_progress: nethacked_core::RitualProgress,
    pub quest_state: nethacked_core::QuestState,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct EquipmentFacts {
    pub equipment: crate::actions::equipment::EquipmentState,
    pub wielded_item: Option<ItemId>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct BillingFacts {
    pub unpaid_items: Vec<(ItemId, u32)>,
    pub billing: crate::billing::BillingState,
    pub player_gold: u32,
    pub stored_unpaid_items: Vec<Vec<(ItemId, u32)>>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct GeographyFacts {
    pub level: DungeonLevel,
    pub branch: BranchId,
    pub depth: usize,
    pub vibrating_square: Option<Coord>,
    pub stored_keys: Vec<(BranchId, usize)>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SessionFacts {
    pub role_name: String,
    pub role_id: Option<nethacked_data::RoleId>,
    pub locale: Locale,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClockOutcome {
    pub scheduler: SchedulerState,
    pub object_timers: crate::object_timers::ObjectTimerQueue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectsOutcome {
    pub items: ItemStorage,
    pub artifact_history: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorsOutcome {
    pub actors: ActorStorage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeroOutcome {
    pub hero: nethacked_types::Hero,
    pub player_nutrition: i32,
    pub player_pw: u32,
    pub player_max_pw: u32,
    pub known_spells: Vec<(SpellKind, u32)>,
    pub divine_favor: i32,
    pub prayer_timeout: u32,
    pub divine_gift_count: u32,
    pub divine_protection: u32,
    pub priest_cheapskate: u32,
    pub player_luck: i32,
    pub alignment_record: i32,
    pub progress: crate::progress::HeroProgress,
    pub genocide_registry: nethacked_types::GenocideRegistry,
    pub conducts: nethacked_types::ConductTracker,
    pub candelabrum_state: nethacked_core::CandelabrumState,
    pub ritual_progress: nethacked_core::RitualProgress,
    pub quest_state: nethacked_core::QuestState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquipmentOutcome {
    pub equipment: crate::actions::equipment::EquipmentState,
    pub wielded_item: Option<ItemId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingOutcome {
    pub unpaid_items: Vec<(ItemId, u32)>,
    pub billing: crate::billing::BillingState,
    pub player_gold: u32,
}

/// Compact geography write: only the tiles, traps and engravings the action
/// actually changed. The full 80x21 level never rides in a candidate reply
/// (the 16 KiB publication reply bound); it flows as a read observation and
/// as the Geography owner's before/proposed state instead.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GeographyDelta {
    pub tiles: Vec<(Coord, Tile)>,
    pub traps: Vec<(Coord, TrapRecord)>,
    pub engravings: Vec<(Coord, Option<Engraving>)>,
}

impl GeographyDelta {
    pub fn apply(&self, level: &mut DungeonLevel) {
        for (coord, tile) in &self.tiles {
            level.set_tile(*coord, tile.clone());
        }
        for (coord, trap) in &self.traps {
            level.traps.insert(*coord, *trap);
        }
        for (coord, engraving) in &self.engravings {
            match engraving {
                Some(e) => level.set_engraving(*coord, e.clone()),
                None => {
                    level.remove_engraving(*coord);
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeographyOutcome {
    pub delta: GeographyDelta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServicesOutcome {
    pub spent_time: bool,
    pub rounds: u32,
    pub clock: Option<ClockOutcome>,
    pub actors: Option<ActorsOutcome>,
    pub hero: Option<HeroOutcome>,
    pub objects: Option<ObjectsOutcome>,
    pub equipment: Option<EquipmentOutcome>,
    pub billing: Option<BillingOutcome>,
    pub geography: Option<GeographyOutcome>,
    pub events: Vec<GameEvent>,
}

impl ServicesOutcome {
    pub fn empty() -> Self {
        Self {
            spent_time: false,
            rounds: 0,
            clock: None,
            actors: None,
            hero: None,
            objects: None,
            equipment: None,
            billing: None,
            geography: None,
            events: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PrepareV1 {
    pub(super) outcome: ServicesOutcome,
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

/// C `u.uhave.amulet` (peace.rs): the hero carries the genuine Amulet of
/// Yendor. Exact-name match, no item-class check.
fn hero_has_amulet(items: &ItemStorage, player_id: ActorId) -> bool {
    items.iter().any(|(_, it)| {
        it.location == ItemLocation::CarriedBy(player_id)
            && crate::actions::items::is_real_amulet(it)
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

fn actor_at(actors: &ActorStorage, coord: Coord) -> Option<ActorId> {
    actors
        .iter()
        .find(|(_, a)| !a.is_dead && a.coord == coord)
        .map(|(id, _)| id)
}

/// Arena `destroy_item`: removal plus artifact-history registration.
fn destroy_item(obj: &mut ObjectsFacts, id: ItemId) {
    if let Some(item) = obj.items.remove(id) {
        if let Some(artifact) = &item.artifact_id {
            obj.artifact_history.insert(artifact.clone());
        }
    }
}

/// Arena `spawn_item`: artifact-history registration plus insertion.
fn spawn_item(obj: &mut ObjectsFacts, item: ItemRecord) -> ItemId {
    if let Some(id) = &item.artifact_id {
        obj.artifact_history.insert(id.clone());
    }
    obj.items.insert(item)
}

/// Arena `items_carried_by`: carried items in storage order.
fn items_carried_by(items: &ItemStorage, actor: ActorId) -> Vec<ItemId> {
    items
        .iter()
        .filter_map(|(id, item)| {
            if item.location == ItemLocation::CarriedBy(actor) {
                Some(id)
            } else {
                None
            }
        })
        .collect()
}

/// C `shk.c:2947-2951` dunce/tourist surcharge for the hero (economy.rs
/// `hero_dunce_or_tourist`): a carried "dunce cap" stands in for a worn one;
/// the visible-shirt case is never true.
fn hero_dunce_or_tourist(
    items: &ItemStorage,
    player_id: ActorId,
    role_name: &str,
    ulevel: u32,
) -> bool {
    let dunce_cap = items_carried_by(items, player_id).into_iter().any(|id| {
        items
            .get(id)
            .is_some_and(|it| it.name.eq_ignore_ascii_case("dunce cap"))
    });
    nethacked_core::dunce_or_tourist_surcharge(dunce_cap, role_name == "Tourist", ulevel, false)
}

/// The shopkeeper's buy price for a base cost (economy.rs `shop_buy_price`).
fn shop_buy_price(
    items: &ItemStorage,
    player_id: ActorId,
    role_name: &str,
    ulevel: u32,
    base: u32,
) -> u32 {
    nethacked_core::buy_price(
        base,
        crate::actions::economy::DEFAULT_CHARISMA,
        hero_dunce_or_tourist(items, player_id, role_name, ulevel),
        false,
        false,
        false,
    )
}

/// The shopkeeper's sell price for a base cost (economy.rs `shop_sell_price`).
fn shop_sell_price(
    items: &ItemStorage,
    player_id: ActorId,
    role_name: &str,
    ulevel: u32,
    base: u32,
) -> u32 {
    nethacked_core::sell_price(
        base,
        hero_dunce_or_tourist(items, player_id, role_name, ulevel),
        false,
    )
}

/// The price owed for an unpaid shop item (world.rs `get_unpaid_cost` via
/// economy.rs `shop_buy_price`).
fn get_unpaid_cost(
    bill: &BillingFacts,
    items: &ItemStorage,
    player_id: ActorId,
    id: ItemId,
    role_name: &str,
    ulevel: u32,
) -> Option<u32> {
    bill.unpaid_items
        .iter()
        .find(|(i, _)| *i == id)
        .map(|(_, base)| shop_buy_price(items, player_id, role_name, ulevel, *base))
}

/// world.rs `item_birth_context`: container and monster generation contexts
/// derived from the hero's level, the world clock and the current geography.
fn item_birth_context(
    a: &ActorsFacts,
    h: &HeroFacts,
    geo: &GeographyFacts,
    clk: &ClockFacts,
) -> (
    nethacked_data::containers::ContainerContext,
    nethacked_data::generation::MonsterGenContext,
) {
    use nethacked_data::generation::{DungeonAlign, MonsterGenContext};
    let hero_level = a.actors.get(h.player_id).map_or(1, |p| p.level);
    (
        nethacked_data::containers::ContainerContext {
            hero_level,
            level_difficulty: geo.depth.min(u32::MAX as usize) as u32,
            moves: clk.scheduler.turn,
            in_mklev: false,
        },
        MonsterGenContext {
            hero_level: hero_level.min(i32::MAX as u32) as i32,
            level_difficulty: geo.depth.min(i32::MAX as usize) as i32,
            in_hell: geo.branch == BranchId::Gehennom,
            align: if geo.branch == BranchId::GnomishMines {
                DungeonAlign::Lawful
            } else {
                DungeonAlign::None
            },
            temperature: 0,
        },
    )
}

/// object_timers.rs `attach_generated_item` over the owner facts: materialize
/// a sampled tree once, preserving generation order and clock. No RNG or age
/// mutation occurs here. C mkbox_cnts starts then cancels ice corpse timers:
/// cancelled entries still consume an insertion identity.
fn attach_generated_item(
    obj: &mut ObjectsFacts,
    timers: &mut crate::object_timers::ObjectTimerQueue,
    mut tree: nethacked_data::containers::GeneratedItem,
    location: ItemLocation,
) -> Result<ItemId, crate::object_timers::ObjectTimerError> {
    tree.item.location = location;
    let id = spawn_item(obj, tree.item);
    use nethacked_data::corpses::{CorpseTimeoutRecipe as Recipe, CorpseTimerOperation as Op};
    // Explicit compatibility for old handbuilt primitive recipes. A
    // nonempty stream is authoritative; its summary is never replayed.
    if tree.corpse_timer_operations.is_empty() {
        if let Some(recipe) = tree.corpse_timeout {
            tree.corpse_timer_operations.push(Op::Start(recipe));
        }
    }
    let target = crate::object_timers::ObjectTimerTarget::Active(id);
    for operation in tree.corpse_timer_operations {
        match operation {
            Op::Cancel => {
                timers.cancel(target);
            }
            Op::Start(recipe) => {
                let (sampled_at, delay, kind) = match recipe {
                    Recipe::Rot { sampled_at, delay } => (
                        sampled_at,
                        delay,
                        crate::object_timers::ObjectTimerKind::RotCorpse,
                    ),
                    Recipe::Revive { sampled_at, delay } => (
                        sampled_at,
                        delay,
                        crate::object_timers::ObjectTimerKind::ReviveMonster,
                    ),
                    Recipe::UnsupportedRider => continue,
                };
                timers.start_at_preserving(
                    target,
                    kind,
                    sampled_at.checked_add_signed(delay),
                    true,
                )?;
            }
        }
    }
    if frozen_in_ice_box(&obj.items[id], |parent| obj.items.get(parent)) {
        timers.cancel(target);
    }
    for child in tree.contents {
        attach_generated_item(obj, timers, child, ItemLocation::InContainer(id))?;
    }
    Ok(id)
}

/// C `onscary(x, y, mtmp)` (peace.rs `elbereth_scares`) for the engraving
/// under the hero: does a written Elbereth scare `mon`?
fn elbereth_scares(
    ruleset: &crate::Ruleset,
    mon: &ActorRecord,
    engraving: Option<&Engraving>,
) -> bool {
    let arch = ruleset.monster_for_actor(mon);
    let is_s_human = arch.is_some_and(|a| a.is_human);
    let exempt = onscary_exempt(is_s_human, false, is_shopkeeper(ruleset, mon), false);
    is_elbereth_ward_active(
        engraving,
        mon.intrinsics.blind,
        mon.is_unique,
        mon.is_peaceful,
        exempt,
    )
}

/// C `setmangry(mtmp, TRUE)` (`mon.c:4265-4318`, peace.rs `setmangry`), called
/// after the hero's spell attack on a surviving monster.
#[allow(clippy::too_many_arguments)]
fn setmangry(
    a: &mut ActorsFacts,
    h: &mut HeroFacts,
    geo: &mut GeographyFacts,
    geo_delta: &mut GeographyDelta,
    clk: &ClockFacts,
    sess: &SessionFacts,
    config: &OwnerBridgeConfig,
    rng: &mut rand_chacha::ChaCha8Rng,
    target_id: ActorId,
    events: &mut Vec<GameEvent>,
) {
    let Some(target) = a.actors.get(target_id).cloned() else {
        return;
    };
    let Some(player) = a.actors.get(h.player_id).cloned() else {
        return;
    };
    let lim = nethacked_core::alignlim(clk.scheduler.turn);
    let hero_square: Coord = player.coord;
    let engraving = geo
        .level
        .get_engraving(hero_square)
        .filter(|e| e.text == "Elbereth")
        .cloned();
    if let Some(e) = engraving.as_ref() {
        if elbereth_scares(&config.ruleset, &target, Some(e)) || target.is_peaceful {
            events.push(GameEvent::LogMessage {
                text: nethacked_i18n::Messages::feel_hypocrite(sess.locale).into(),
            });
            // mon.c:4280 `adjalign((u.ualign.record > 5) ? -5 : -rnd(5))`.
            let n = if h.alignment_record > 5 {
                -5
            } else {
                -(rng.random_range(1..=5i32))
            };
            h.alignment_record = nethacked_core::adjalign(h.alignment_record, n, lim);
            if !player.intrinsics.blind {
                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::engraving_fades(sess.locale).into(),
                });
            }
            geo.level.remove_engraving(hero_square);
            geo_delta.engravings.push((hero_square, None));
        }
    }

    if !target.is_peaceful || target.is_tame {
        return;
    }
    if let Some(t) = a.actors.get_mut(target_id) {
        t.is_peaceful = false;
    }
    let delta = if is_temple_priest(&config.ruleset, &target) {
        // priest.c:370 p_coaligned: u.ualign.type == priest alignment.
        if target.alignment == player.alignment {
            -5
        } else {
            2
        }
    } else {
        -1
    };
    h.alignment_record = nethacked_core::adjalign(h.alignment_record, delta, lim);
    events.push(GameEvent::LogMessage {
        text: nethacked_i18n::Messages::gets_angry(
            &target.name,
            monnam_article(&config.ruleset, &target.name),
            sess.locale,
        ),
    });

    let quest_cfg = nethacked_core::get_role_quest_config_or_default(&sess.role_name);
    if target.name.eq_ignore_ascii_case(quest_cfg.leader_name) {
        let mut got_mad = 0u32;
        for (_, mon) in a.actors.iter_mut() {
            if !mon.is_dead
                && mon.is_peaceful
                && mon.name.eq_ignore_ascii_case(quest_cfg.guardian_name)
            {
                mon.is_peaceful = false;
                got_mad += 1;
            }
        }
        if got_mad > 0 {
            events.push(GameEvent::LogMessage {
                text: nethacked_i18n::Messages::guardians_angry_too(
                    quest_cfg.guardian_name,
                    got_mad > 1,
                    sess.locale,
                ),
            });
        }
    }
}

/// C `iter_mons(anger_quest_guardians)` (`mon.c:3733-3739`, combat.rs
/// `anger_quest_guardians`): each quest guardian gets `setmangry(mtmp, TRUE)`.
/// Only a peaceful, non-tame guardian turns hostile, costing `adjalign(-1)`.
fn anger_quest_guardians(
    a: &mut ActorsFacts,
    h: &mut HeroFacts,
    clk: &ClockFacts,
    sess: &SessionFacts,
    config: &OwnerBridgeConfig,
) {
    let quest_cfg = nethacked_core::get_role_quest_config_or_default(&sess.role_name);
    let guardian_ids: Vec<ActorId> = a
        .actors
        .iter()
        .filter(|(id, actor)| {
            *id != h.player_id
                && !actor.is_dead
                && (actor.name.eq_ignore_ascii_case(quest_cfg.guardian_name)
                    || config
                        .ruleset
                        .monster_for_actor(actor)
                        .is_some_and(|m| m.msound == nethacked_data::MonsterSound::Guardian))
        })
        .map(|(id, _)| id)
        .collect();

    let lim = nethacked_core::alignlim(clk.scheduler.turn);
    for id in guardian_ids {
        let Some(mon) = a.actors.get_mut(id) else {
            continue;
        };
        if !mon.is_peaceful || mon.is_tame {
            continue;
        }
        mon.is_peaceful = false;
        h.alignment_record = nethacked_core::adjalign(h.alignment_record, -1, lim);
    }
}

/// C `MAXULEV` (global.h).
const MAXULEV: u32 = 30;

/// progress.rs `stats`: the C role table (Valkyrie fallback) plus the hero's
/// race table.
fn stats(
    role_name: &str,
    hero_race: nethacked_data::RaceId,
) -> (
    &'static nethacked_data::roles::RoleStats,
    &'static nethacked_data::roles::RaceStats,
) {
    let role = nethacked_data::roles::role_stats(role_name)
        .or_else(|| nethacked_data::roles::role_stats("valkyrie"))
        .expect("C role table");
    (role, nethacked_data::roles::race_stats(hero_race))
}

/// progress.rs `hero_level`: the player actor's experience level.
fn hero_level(a: &ActorsFacts, h: &HeroFacts) -> u32 {
    a.actors.get(h.player_id).map_or(1, |p| p.level)
}

/// progress.rs `roll_advance`: fixed part plus one `rnd` draw per die.
fn roll_advance(
    rng: &mut rand_chacha::ChaCha8Rng,
    role: &nethacked_data::roles::Advance,
    race: &nethacked_data::roles::Advance,
    level: u32,
    xlev: u32,
) -> i32 {
    let (fix, rnd) = if level == 0 {
        ((role.infix + race.infix), [role.inrnd, race.inrnd])
    } else if level < xlev {
        ((role.lofix + race.lofix), [role.lornd, race.lornd])
    } else {
        ((role.hifix + race.hifix), [role.hirnd, race.hirnd])
    };
    let mut v = fix;
    for r in rnd {
        if r > 0 {
            v += rng.random_range(1..=r);
        }
    }
    v
}

/// C `newhp()` (attrib.c) for a level gain at the current level.
fn newhp(
    rng: &mut rand_chacha::ChaCha8Rng,
    a: &ActorsFacts,
    h: &HeroFacts,
    sess: &SessionFacts,
) -> i32 {
    let (role, race) = stats(&sess.role_name, h.hero_race);
    let level = hero_level(a, h);
    let mut hp = roll_advance(rng, &role.hp, &race.hp, level, role.xlev);
    if level > 0 {
        hp += match h.progress.attr_base[crate::progress::A_CON] {
            ..=3 => -2,
            4..=6 => -1,
            7..=14 => 0,
            15..=16 => 1,
            17 => 2,
            18 => 3,
            _ => 4,
        };
    }
    hp.max(1)
}

/// C `newpw()` (exper.c) without the `enermod` role multiplier's Wis term
/// simplified: role/race energy advance plus `enermod`.
fn newpw(
    rng: &mut rand_chacha::ChaCha8Rng,
    a: &ActorsFacts,
    h: &HeroFacts,
    sess: &SessionFacts,
) -> i32 {
    let (role, race) = stats(&sess.role_name, h.hero_race);
    let level = hero_level(a, h);
    let en = roll_advance(rng, &role.energy, &race.energy, level, role.xlev);
    let en = match sess.role_name.to_ascii_lowercase().as_str() {
        "priest" | "cleric" | "wizard" => 2 * en,
        "healer" | "knight" => (3 * en) / 2,
        "barbarian" | "valkyrie" => (3 * en) / 4,
        _ => en,
    };
    en.max(0)
}

/// C `pluslvl(TRUE)` (exper.c:307).
fn pluslvl(
    rng: &mut rand_chacha::ChaCha8Rng,
    a: &mut ActorsFacts,
    h: &mut HeroFacts,
    sess: &SessionFacts,
    events: &mut Vec<GameEvent>,
) {
    // C draws the HP increment before the energy increment (exper.c:308-309).
    let hpinc = newhp(rng, a, h, sess);
    let eninc = newpw(rng, a, h, sess);
    let level = hero_level(a, h);
    let idx = level as usize;
    if h.progress.hp_increments.len() <= idx {
        h.progress.hp_increments.resize(idx + 1, 0);
        h.progress.en_increments.resize(idx + 1, 0);
    }
    h.progress.hp_increments[idx] = hpinc;
    h.progress.en_increments[idx] = eninc;
    if let Some(p) = a.actors.get_mut(h.player_id) {
        p.max_hp = p.max_hp.saturating_add(hpinc as u32);
        p.hp = p.hp.saturating_add(hpinc as u32);
    }
    h.player_max_pw = h.player_max_pw.saturating_add(eninc as u32);
    h.player_pw = h.player_pw.saturating_add(eninc as u32);
    if level < MAXULEV {
        let cap = crate::progress::newuexp(level + 1);
        if h.progress.exp >= cap {
            h.progress.exp = cap - 1;
        }
        let new_level = level + 1;
        if let Some(p) = a.actors.get_mut(h.player_id) {
            p.level = new_level;
        }
        let back = if h.progress.level_max < new_level {
            ""
        } else {
            "back "
        };
        events.push(GameEvent::LogMessage {
            text: format!("Welcome {back}to experience level {new_level}."),
        });
        h.progress.level_max = h.progress.level_max.max(new_level);
    }
}

/// C `more_experienced(exp, 0)` then `newexplevel()` (exper.c:169, 300).
fn gain_experience(
    rng: &mut rand_chacha::ChaCha8Rng,
    a: &mut ActorsFacts,
    h: &mut HeroFacts,
    sess: &SessionFacts,
    exp: u64,
    events: &mut Vec<GameEvent>,
) {
    h.progress.exp = h.progress.exp.saturating_add(exp);
    let level = hero_level(a, h);
    if level < MAXULEV && h.progress.exp >= crate::progress::newuexp(level) {
        pluslvl(rng, a, h, sess, events);
    }
}

/// progress.rs `award_kill_experience`: C `experience()` for the victim, then
/// `gain_experience`.
fn award_kill_experience(
    rng: &mut rand_chacha::ChaCha8Rng,
    a: &mut ActorsFacts,
    h: &mut HeroFacts,
    sess: &SessionFacts,
    config: &OwnerBridgeConfig,
    victim: &ActorRecord,
    events: &mut Vec<GameEvent>,
) {
    let Some(def) = config.ruleset.monster_for_actor(victim) else {
        return;
    };
    let exp = crate::progress::experience(
        victim.level,
        victim.ac,
        def.speed,
        &def.attacks,
        def.has_flag("nasty"),
        def.id == Some(nethacked_data::MonsterSpeciesId::MAIL_DAEMON),
    );
    gain_experience(rng, a, h, sess, exp, events);
}

/// items.rs `handle_cast`: mana check, ForceBolt/MagicMissile beam damage
/// with kill resolution, CureLightWounds/ExtraHealing self-heals.
#[allow(clippy::too_many_arguments)]
fn execute_cast(
    clk: &mut ClockFacts,
    obj: &mut ObjectsFacts,
    a: &mut ActorsFacts,
    h: &mut HeroFacts,
    geo: &mut GeographyFacts,
    geo_delta: &mut GeographyDelta,
    sess: &SessionFacts,
    config: &OwnerBridgeConfig,
    rng: &mut rand_chacha::ChaCha8Rng,
    spell_index: usize,
    dir: Direction,
) -> owners::Result<Vec<GameEvent>> {
    let mut events = Vec::new();
    let Some(player) = a.actors.get(h.player_id).cloned() else {
        return Ok(events);
    };

    if spell_index < h.known_spells.len() {
        let (spell, _) = h.known_spells[spell_index];
        let cost = nethacked_core::magic::mana_cost(spell);
        if h.player_pw < cost {
            events.push(GameEvent::LogMessage {
                text: format!(
                    "You don't have enough mana! Requires {} Pw, you have {}.",
                    cost, h.player_pw
                ),
            });
        } else {
            h.player_pw -= cost;
            match spell {
                SpellKind::ForceBolt | SpellKind::MagicMissile => {
                    let path = nethacked_dungeon::trace_beam_path(&geo.level, player.coord, dir, 6);
                    events.push(GameEvent::BeamPropagated { path: path.clone() });
                    let spell_damage = if spell == SpellKind::ForceBolt {
                        18
                    } else {
                        14
                    };
                    for coord in path {
                        if let Some(target_id) = actor_at(&a.actors, coord) {
                            if target_id != h.player_id {
                                if let Some(target) = a.actors.get_mut(target_id) {
                                    target.hp = target.hp.saturating_sub(spell_damage);
                                    if target.hp == 0 {
                                        target.is_dead = true;
                                    }
                                    events.push(GameEvent::AttackLanded {
                                        attacker: h.player_id,
                                        target: target_id,
                                        damage: spell_damage,
                                        lethal: target.is_dead,
                                    });
                                }
                                if a.actors.get(target_id).is_some_and(|t| t.is_dead) {
                                    let name = a
                                        .actors
                                        .get(target_id)
                                        .map(|x| x.name.as_str())
                                        .unwrap_or("The monster");
                                    events.push(GameEvent::LogMessage {
                                        text: format!("{name} is slain by magic!"),
                                    });
                                    on_actor_killed(
                                        clk,
                                        obj,
                                        a,
                                        h,
                                        geo,
                                        sess,
                                        config,
                                        rng,
                                        h.player_id,
                                        target_id,
                                        &mut events,
                                    )?;
                                }
                                // C bhitm (zap.c:552-554, force bolt) and buzz
                                // (zap.c:4948, magic missile): a surviving
                                // target is woken with `wakeup(mon, TRUE)`.
                                if a.actors.get(target_id).is_some_and(|t| !t.is_dead) {
                                    setmangry(
                                        a,
                                        h,
                                        geo,
                                        geo_delta,
                                        clk,
                                        sess,
                                        config,
                                        rng,
                                        target_id,
                                        &mut events,
                                    );
                                }
                                break;
                            }
                        }
                    }
                    events.push(GameEvent::LogMessage {
                        text: format!("You cast {spell:?}!"),
                    });
                }
                SpellKind::CureLightWounds => {
                    if let Some(p) = a.actors.get_mut(h.player_id) {
                        p.hp = (p.hp + 12).min(p.max_hp);
                    }
                    events.push(GameEvent::LogMessage {
                        text: "You cast Cure Light Wounds! Your wounds close.".into(),
                    });
                }
                SpellKind::ExtraHealing => {
                    if let Some(p) = a.actors.get_mut(h.player_id) {
                        p.hp = (p.hp + 30).min(p.max_hp);
                    }
                    events.push(GameEvent::LogMessage {
                        text: "You cast Extra Healing! Divine vitality surges through you.".into(),
                    });
                }
            }
            clk.scheduler.hero_act(NORMAL_SPEED);
        }
    } else {
        events.push(GameEvent::LogMessage {
            text: "You do not know that spell.".into(),
        });
    }
    Ok(events)
}

/// items.rs `handle_wish`: carried wand of wishing, charge decrement, wish
/// parsing and item generation (amulet imitation, unwishable loss, corpse
/// tree, generic item).
#[allow(clippy::too_many_arguments)]
fn execute_wish(
    clk: &mut ClockFacts,
    obj: &mut ObjectsFacts,
    a: &ActorsFacts,
    h: &mut HeroFacts,
    geo: &GeographyFacts,
    sess: &SessionFacts,
    config: &OwnerBridgeConfig,
    rng: &mut rand_chacha::ChaCha8Rng,
    wish_str: String,
) -> owners::Result<Vec<GameEvent>> {
    let mut events = Vec::new();
    let Some(player) = a.actors.get(h.player_id).cloned() else {
        return Ok(events);
    };

    // Find carried wand of wishing
    let wow_id = items_carried_by(&obj.items, h.player_id)
        .into_iter()
        .find(|&id| {
            obj.items
                .get(id)
                .map(|it| it.class == ItemClass::Wand && it.name == "wand of wishing")
                .unwrap_or(false)
        });

    let Some(wid) = wow_id else {
        events.push(GameEvent::LogMessage {
            text: "You have no means of wishing.".into(),
        });
        return Ok(events);
    };
    {
        let Some(wand_item) = obj.items.get_mut(wid) else {
            return Ok(events);
        };
        let charges = wand_item.enchantment.max(0) as u32;
        let recharges = wand_item.recharged as u32;
        if let Some(new_w) =
            nethacked_core::artifacts_wands::zap_wand(nethacked_types::WandCharges {
                charges,
                recharges,
            })
        {
            wand_item.enchantment = new_w.charges.min(i8::MAX as u32) as i8;
        } else {
            events.push(GameEvent::LogMessage {
                text: nethacked_i18n::Messages::wish_empty(sess.locale).into(),
            });
            clk.scheduler.hero_act(NORMAL_SPEED);
            return Ok(events);
        }
    }
    nethacked_core::conducts::record_wish(&mut h.conducts);

    if let Some((item_query, ench, buc)) = nethacked_core::artifacts_wands::parse_wish(&wish_str) {
        let wanted = crate::actions::items::normalize_wish_name(&item_query);
        let matched_arch = config.ruleset.item(&wanted).cloned();

        match matched_arch {
            Some(arch) if arch.id == Some(nethacked_data::ItemKindId::AMULET_OF_YENDOR) => {
                if let Some(mut fake) = config.ruleset.create_item_record(
                    &arch.name,
                    ItemLocation::Floor(player.coord),
                    buc,
                ) {
                    fake.name = "cheap plastic imitation of the Amulet of Yendor".into();
                    spawn_item(obj, fake);
                }
                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::wish_granted(
                        "cheap plastic imitation of the Amulet of Yendor",
                        sess.locale,
                    ),
                });
            }
            Some(arch)
                if arch
                    .id
                    .map(|id| crate::actions::items::UNWISHABLE.contains(&id))
                    .unwrap_or(false) =>
            {
                events.push(GameEvent::LogMessage {
                    text: format!(
                        "You feel a vague sense of loss. The {} cannot be wished for.",
                        arch.name
                    ),
                });
            }
            Some(arch) if arch.id == Some(nethacked_data::ItemKindId::CORPSE) => {
                let (ctx, monster_ctx) = item_birth_context(a, h, geo, clk);
                let registry = &h.genocide_registry;
                let location = ItemLocation::Floor(player.coord);
                if let Some(mut tree) = config.ruleset.create_random_item_tree_with_monsters(
                    &arch.name,
                    location.clone(),
                    ctx,
                    &monster_ctx,
                    |d| nethacked_core::genocide::is_genocided(registry, &d.name, d.glyph),
                    |n| rng.random_range(0..n),
                ) {
                    tree.item.buc = buc;
                    // C objnam.c:5148–5169: generic corpse wishes consume
                    // initialization gender, then overwrite spe with zero.
                    tree.item.enchantment = 0;
                    let id = attach_generated_item(obj, &mut clk.object_timers, tree, location)
                        .map_err(|_| owners::Error::Invariant)?;
                    let item_name = obj.items[id].name.clone();
                    events.push(GameEvent::LogMessage {
                        text: nethacked_i18n::Messages::wish_granted(&item_name, sess.locale),
                    });
                }
            }
            Some(arch) => {
                if let Some(mut record) = config.ruleset.create_item_record(
                    &arch.name,
                    ItemLocation::Floor(player.coord),
                    buc,
                ) {
                    // Wands keep their initial charges; the parsed enchantment applies to other items.
                    if record.class != ItemClass::Wand {
                        record.enchantment = ench;
                    }
                    let spawned_id = spawn_item(obj, record);
                    let item_name = obj.items.get(spawned_id).unwrap().name.clone();
                    events.push(GameEvent::LogMessage {
                        text: nethacked_i18n::Messages::wish_granted(&item_name, sess.locale),
                    });
                } else {
                    events.push(GameEvent::LogMessage {
                        text: format!("You feel a vague sense of loss. You wished for '{wish_str}', but received nothing."),
                    });
                }
            }
            None => {
                events.push(GameEvent::LogMessage {
                    text: format!("You feel a vague sense of loss. You wished for '{wish_str}', but received nothing."),
                });
            }
        }
    } else {
        events.push(GameEvent::LogMessage {
            text: "Your mind draws a blank. Nothing happens.".into(),
        });
    }

    clk.scheduler.hero_act(NORMAL_SPEED);
    Ok(events)
}

/// religion.rs `handle_pray`: prayer timeout smite, cooldown set, altar
/// alignment outcomes and water consecration.
fn execute_pray(
    clk: &mut ClockFacts,
    obj: &mut ObjectsFacts,
    a: &mut ActorsFacts,
    h: &mut HeroFacts,
    geo: &GeographyFacts,
    sess: &SessionFacts,
) -> Vec<GameEvent> {
    let mut events = Vec::new();
    nethacked_core::conducts::record_altar_action(&mut h.conducts);
    let Some(player) = a.actors.get(h.player_id).cloned() else {
        return events;
    };

    let p_coord = player.coord;
    let current_tile = geo.level.get_tile(p_coord).clone();

    // 1. Prayer timeout check: Did the player pray too soon?
    if h.prayer_timeout > 0 {
        h.divine_favor = nethacked_core::religion::clamp_favor(h.divine_favor - 3);
        let smite_damage = 8u32;
        if let Some(p) = a.actors.get_mut(h.player_id) {
            p.hp = p.hp.saturating_sub(smite_damage).max(1);
        }
        events.push(GameEvent::LogMessage {
            text: nethacked_i18n::Messages::prayer_timeout(sess.locale).into(),
        });
        clk.scheduler.hero_act(NORMAL_SPEED);
        return events;
    }

    // Safe prayer sets cooldown
    h.prayer_timeout = 300;
    h.divine_favor = nethacked_core::religion::clamp_favor(h.divine_favor + 1);

    match current_tile {
        Tile::Altar { align } => {
            let is_coaligned = player.alignment == align;
            if is_coaligned {
                if let Some(p) = a.actors.get_mut(h.player_id) {
                    p.hp = p.max_hp;
                }
                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::prayer_coaligned(sess.locale).into(),
                });

                // Check for water consecration into Holy Water!
                let carried = items_carried_by(&obj.items, h.player_id);
                for item_id in carried {
                    if let Some(item) = obj.items.get_mut(item_id) {
                        if item.name.to_lowercase().contains("potion of water")
                            && item.buc == Buc::Uncursed
                        {
                            item.buc = nethacked_core::religion::consecrate_water(
                                item.buc,
                                true,
                                h.divine_favor,
                            );
                            events.push(GameEvent::LogMessage {
                                text: nethacked_i18n::Messages::holy_water_consecrated(sess.locale)
                                    .into(),
                            });
                            break;
                        }
                    }
                }
            } else if align == Alignment::Neutral {
                if let Some(p) = a.actors.get_mut(h.player_id) {
                    p.hp = (p.hp + 8).min(p.max_hp);
                }
                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::prayer_neutral(sess.locale).into(),
                });
            } else {
                // Wrath from opposing god!
                let wrath_damage = 6u32;
                if let Some(p) = a.actors.get_mut(h.player_id) {
                    p.hp = p.hp.saturating_sub(wrath_damage).max(1);
                }
                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::prayer_wrath(sess.locale).into(),
                });
            }
        }
        Tile::HighAltar { align } => {
            if player.alignment == align {
                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::prayer_high_altar_coaligned(sess.locale).into(),
                });
            } else {
                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::prayer_high_altar_foreign(sess.locale).into(),
                });
            }
        }
        _ => {
            events.push(GameEvent::LogMessage {
                text: nethacked_i18n::Messages::prayer_general(sess.locale).into(),
            });
        }
    }
    clk.scheduler.hero_act(NORMAL_SPEED);
    events
}

/// religion.rs `handle_sacrifice`: altar sacrifice with divine favor/gift
/// resolution, and high-altar amulet offering (ascension).
#[allow(clippy::too_many_arguments)]
fn execute_sacrifice(
    clk: &mut ClockFacts,
    obj: &mut ObjectsFacts,
    a: &mut ActorsFacts,
    h: &mut HeroFacts,
    eq: &mut EquipmentFacts,
    geo: &mut GeographyFacts,
    geo_delta: &mut GeographyDelta,
    sess: &SessionFacts,
    config: &OwnerBridgeConfig,
    idx: usize,
) -> Vec<GameEvent> {
    let mut events = Vec::new();
    nethacked_core::conducts::record_altar_action(&mut h.conducts);
    let Some(player) = a.actors.get(h.player_id).cloned() else {
        return events;
    };

    let p_coord = player.coord;
    let current_tile = geo.level.get_tile(p_coord).clone();
    match current_tile {
        Tile::Altar { align } => {
            let carried = items_carried_by(&obj.items, h.player_id);
            if idx < carried.len() {
                let item_id = carried[idx];
                let item_name = obj
                    .items
                    .get(item_id)
                    .map(|i| i.name.clone())
                    .unwrap_or_default();
                destroy_item(obj, item_id);
                if eq.wielded_item == Some(item_id) {
                    eq.wielded_item = None;
                }

                let corpse_nutrition = if item_name.contains("corpse") {
                    250
                } else {
                    100
                };
                let divine = DivineState {
                    favor: h.divine_favor,
                    prayer_timeout: h.prayer_timeout,
                    gift_count: h.divine_gift_count,
                };
                let (new_div, sac_res) = nethacked_core::religion::resolve_sacrifice(
                    divine,
                    player.alignment,
                    align,
                    corpse_nutrition,
                );
                h.divine_favor = new_div.favor;
                h.prayer_timeout = new_div.prayer_timeout;
                h.divine_gift_count = new_div.gift_count;

                match sac_res {
                    SacrificeResult::AltarConverted(new_align) => {
                        geo.level
                            .set_tile(p_coord, Tile::Altar { align: new_align });
                        geo_delta
                            .tiles
                            .push((p_coord, Tile::Altar { align: new_align }));
                        events.push(GameEvent::LogMessage {
                            text: nethacked_i18n::Messages::altar_converted(
                                &item_name,
                                new_align,
                                sess.locale,
                            ),
                        });
                    }
                    SacrificeResult::FavorIncreased(fav) => {
                        if let Some(p) = a.actors.get_mut(h.player_id) {
                            p.max_hp += 1;
                            p.hp += 1;
                        }
                        events.push(GameEvent::LogMessage {
                            text: nethacked_i18n::Messages::sacrifice_favor_increased(
                                &item_name,
                                fav,
                                sess.locale,
                            ),
                        });
                    }
                    SacrificeResult::DivineGift(artifact_name) => {
                        let gift_record = ItemRecord {
                            definition: None,
                            saved_monster_traits: None,
                            artifact_id: config
                                .ruleset
                                .artifact_id_for_item(&artifact_name)
                                .map(str::to_owned),
                            name: artifact_name.clone(),
                            class: ItemClass::Weapon,
                            weight: 30,
                            buc: Buc::Blessed,
                            is_container: false,
                            is_bag_of_holding: false,
                            enchantment: 2,
                            erosion: 0,
                            proofed: true,
                            location: ItemLocation::CarriedBy(h.player_id),
                            corpse_no_revive: false,
                            corpse_species: None,
                            is_ice_box: false,
                            corpse_race: None,
                            corpse_age: 0,
                            rot_threshold: 50,
                            recharged: 0,
                            quantity: 1,
                            poisoned: false,
                            locked: false,
                            trapped: false,
                            trap_known: false,
                        };
                        spawn_item(obj, gift_record);
                        events.push(GameEvent::LogMessage {
                            text: nethacked_i18n::Messages::divine_crowning(
                                &artifact_name,
                                sess.locale,
                            ),
                        });
                    }
                }
                clk.scheduler.hero_act(NORMAL_SPEED);
            } else {
                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::no_sacrifice_item(sess.locale).into(),
                });
            }
        }
        Tile::HighAltar { align } => {
            let carried = items_carried_by(&obj.items, h.player_id);
            if idx < carried.len() {
                let item_id = carried[idx];
                let is_amulet = obj
                    .items
                    .get(item_id)
                    .is_some_and(crate::actions::items::is_real_amulet);
                if is_amulet {
                    let outcome = nethacked_core::endgame::offer_amulet_on_high_altar(
                        true,
                        player.alignment,
                        align,
                    );
                    match outcome {
                        AscensionOutcome::Ascended(god_align) => {
                            destroy_item(obj, item_id);
                            if eq.wielded_item == Some(item_id) {
                                eq.wielded_item = None;
                            }
                            events.push(GameEvent::LogMessage {
                                text: nethacked_i18n::Messages::ascension_victory(
                                    god_align,
                                    sess.locale,
                                ),
                            });
                            events.push(GameEvent::Victory);
                        }
                        AscensionOutcome::Rejected(reason) => {
                            events.push(GameEvent::LogMessage {
                                text: nethacked_i18n::Messages::ascension_rejected(
                                    &reason,
                                    sess.locale,
                                ),
                            });
                            if let Some(p) = a.actors.get_mut(h.player_id) {
                                p.hp = p.hp.saturating_sub(15).max(1);
                            }
                        }
                    }
                } else {
                    events.push(GameEvent::LogMessage {
                        text: nethacked_i18n::Messages::high_altar_demands_amulet(sess.locale)
                            .into(),
                    });
                }
                clk.scheduler.hero_act(NORMAL_SPEED);
            } else {
                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::no_sacrifice_item(sess.locale).into(),
                });
            }
        }
        _ => {
            events.push(GameEvent::LogMessage {
                text: nethacked_i18n::Messages::no_altar(sess.locale).into(),
            });
        }
    }
    events
}

/// religion.rs `handle_donate`: temple donation to a nearby priest with
/// protection purchase, uncursing service and favor gain.
#[allow(clippy::too_many_arguments)]
fn execute_donate(
    clk: &mut ClockFacts,
    obj: &mut ObjectsFacts,
    a: &ActorsFacts,
    h: &mut HeroFacts,
    bill: &mut BillingFacts,
    rng: &mut rand_chacha::ChaCha8Rng,
    amount: u32,
) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let Some(player) = a.actors.get(h.player_id).cloned() else {
        return events;
    };

    // Find a priest on the floor near player (distance <= 6)
    let priest_id = a.actors.iter().find_map(|(id, actor)| {
        if !actor.is_dead
            && actor.name.to_lowercase().contains("priest")
            && actor.coord.chebyshev_distance(player.coord) <= 6
        {
            Some(id)
        } else {
            None
        }
    });

    let Some(_pid) = priest_id else {
        events.push(GameEvent::LogMessage {
            text: "There is no priest nearby to receive your donation.".into(),
        });
        return events;
    };

    // C priest.c:612: a hero without gold cannot make an offer.
    if bill.player_gold == 0 {
        events.push(GameEvent::LogMessage {
            text: "You do not have enough gold to make a donation.".into(),
        });
        return events;
    }

    // C priest.c:637-643. No `u.ulevelpeak` is tracked; the current level stands in.
    let gold_before = bill.player_gold;
    let rn2_101 = rng.random_range(0..101u32);
    let suggested =
        nethacked_core::priest_suggested_donation(player.level, h.priest_cheapskate, rn2_101);
    let quan = nethacked_core::priest_donation_quan(gold_before, suggested);
    // `amount == 0` offers the priest's suggested protection amount (`suggested*quan*2`);
    // C `bribe` (minion.c:379-382) caps an offer at the hero's gold.
    let requested = if amount == 0 {
        suggested.saturating_mul(quan).saturating_mul(2)
    } else {
        amount
    };
    let donation = requested.min(gold_before);
    bill.player_gold -= donation;

    let outcome =
        nethacked_core::priest_donation_outcome(donation, suggested, quan, bill.player_gold);
    match outcome {
        nethacked_core::DonationOutcome::Refused => {
            h.priest_cheapskate += 1;
            events.push(GameEvent::LogMessage {
                text: "The priest says: 'Thou shalt regret thine action!'".into(),
            });
        }
        nethacked_core::DonationOutcome::Cheapskate => {
            h.priest_cheapskate += 1;
            events.push(GameEvent::LogMessage {
                text: "The priest says: 'Cheapskate.'".into(),
            });
        }
        nethacked_core::DonationOutcome::Thanks => {
            events.push(GameEvent::LogMessage {
                text: "The priest thanks you for your contribution to the temple.".into(),
            });
        }
        nethacked_core::DonationOutcome::Clairvoyance => {
            // Clairvoyance is not modelled.
            events.push(GameEvent::LogMessage {
                text: "The priest says: 'Thou art indeed a pious individual. I bestow upon thee a blessing.'".into(),
            });
        }
        nethacked_core::DonationOutcome::Protection => {
            let prev_prot = h.divine_protection;
            let mut prot = prev_prot;
            for _ in 0..nethacked_core::protection_purchase_count(donation, suggested) {
                let roll = nethacked_core::protection_step_roll_bound(prot)
                    .map(|bound| rng.random_range(0..bound))
                    .unwrap_or(0);
                prot = nethacked_core::protection_purchase_step(prot, roll);
            }
            if prot > prev_prot {
                let gained = prot - prev_prot;
                h.divine_protection = prot;
                events.push(GameEvent::LogMessage {
                    text: format!("You feel much safer! You are granted +{gained} divine AC protection (total: +{prot})."),
                });
            } else {
                events.push(GameEvent::LogMessage {
                    text: "The priest says: 'Thy selfless generosity is deeply appreciated.'"
                        .into(),
                });
            }
        }
        nethacked_core::DonationOutcome::Selfless => {
            events.push(GameEvent::LogMessage {
                text: "The priest says: 'Thy selfless generosity is deeply appreciated.'".into(),
            });
        }
    }

    // Uncursing service: if donation >= 200 * level, uncurse cursed items
    let uncurse_threshold = 200 * player.level.max(1);
    if donation >= uncurse_threshold {
        let carried = items_carried_by(&obj.items, h.player_id);
        let mut uncursed_count = 0;
        for iid in carried {
            if let Some(item) = obj.items.get_mut(iid) {
                if item.buc == Buc::Cursed {
                    item.buc = nethacked_core::mines::priest_uncurse(item.buc);
                    uncursed_count += 1;
                }
            }
        }
        if uncursed_count > 0 {
            events.push(GameEvent::LogMessage {
                text: format!("The priest sprinkles holy water! {uncursed_count} cursed items glow and are uncursed."),
            });
        }
    }

    // NetHackED extra (not C): favor +2, but never for a refused or cheapskate offer.
    if !matches!(
        outcome,
        nethacked_core::DonationOutcome::Refused | nethacked_core::DonationOutcome::Cheapskate
    ) {
        h.divine_favor = nethacked_core::religion::clamp_favor(h.divine_favor + 2);
    }
    clk.scheduler.hero_act(NORMAL_SPEED);
    events
}

/// economy.rs `handle_pay`: settle all carried unpaid shop items. The
/// deferred-settlement guard lives in `outcome` (legacy checks it before the
/// hero fetch in `step_player_action`).
fn execute_pay(
    clk: &mut ClockFacts,
    obj: &ObjectsFacts,
    a: &ActorsFacts,
    h: &HeroFacts,
    bill: &mut BillingFacts,
    sess: &SessionFacts,
) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let carried = items_carried_by(&obj.items, h.player_id);
    let ulevel = a.actors.get(h.player_id).map_or(1, |p| p.level);
    let mut total_due = 0u32;
    let mut to_pay = Vec::new();

    for &item_id in &carried {
        if let Some(cost) = get_unpaid_cost(
            bill,
            &obj.items,
            h.player_id,
            item_id,
            &sess.role_name,
            ulevel,
        ) {
            total_due += cost;
            to_pay.push(item_id);
        }
    }

    if to_pay.is_empty() {
        events.push(GameEvent::LogMessage {
            text: nethacked_i18n::Messages::no_unpaid_items(sess.locale).into(),
        });
    } else if bill.player_gold < total_due {
        events.push(GameEvent::LogMessage {
            text: nethacked_i18n::Messages::not_enough_gold(
                total_due,
                bill.player_gold,
                sess.locale,
            ),
        });
    } else {
        bill.player_gold -= total_due;
        for id in to_pay {
            bill.unpaid_items.retain(|(i, _)| *i != id);
        }
        events.push(GameEvent::LogMessage {
            text: nethacked_i18n::Messages::pay_shopkeeper(total_due, sess.locale),
        });
        clk.scheduler.hero_act(NORMAL_SPEED);
    }

    events
}

/// economy.rs `handle_price_check`: shopkeeper appraisal of a carried item.
fn execute_price_check(
    clk: &mut ClockFacts,
    obj: &ObjectsFacts,
    a: &ActorsFacts,
    h: &HeroFacts,
    sess: &SessionFacts,
    config: &OwnerBridgeConfig,
    item_index: usize,
) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let carried = items_carried_by(&obj.items, h.player_id);
    if item_index >= carried.len() {
        events.push(GameEvent::LogMessage {
            text: nethacked_i18n::Messages::no_item_to_appraise(sess.locale).into(),
        });
        return events;
    }

    let has_shopkeeper = a
        .actors
        .values()
        .any(|x| x.name == "shopkeeper" && !x.is_dead);
    if !has_shopkeeper {
        events.push(GameEvent::LogMessage {
            text: nethacked_i18n::Messages::no_shopkeeper_to_appraise(sess.locale).into(),
        });
        return events;
    }

    let item_id = carried[item_index];
    if let Some(item) = obj.items.get(item_id) {
        let base_cost = config
            .ruleset
            .item(&item.name)
            .map(|it| it.cost)
            .unwrap_or(20);
        let ulevel = a.actors.get(h.player_id).map_or(1, |p| p.level);
        let buy = shop_buy_price(&obj.items, h.player_id, &sess.role_name, ulevel, base_cost);
        let sell = shop_sell_price(&obj.items, h.player_id, &sess.role_name, ulevel, base_cost);
        events.push(GameEvent::LogMessage {
            text: nethacked_i18n::Messages::price_appraisal(
                &item.name,
                sell,
                buy,
                base_cost,
                sess.locale,
            ),
        });
        clk.scheduler.hero_act(NORMAL_SPEED);
    }

    events
}

/// ranged.rs `handle_mount`: spend the turn first, then mount a tame steed.
fn execute_mount(
    clk: &mut ClockFacts,
    a: &mut ActorsFacts,
    h: &mut HeroFacts,
    sess: &SessionFacts,
    actor_id: ActorId,
) -> Vec<GameEvent> {
    let mut events = Vec::new();
    clk.scheduler.hero_act(NORMAL_SPEED);

    if let Some(target) = a.actors.get(actor_id) {
        if nethacked_core::ranged::can_mount(target.is_tame, true) {
            // simplified saddle check
            h.hero.mount = Some(MountState {
                steed_id: actor_id,
                saddle_equipped: true,
            });
            events.push(GameEvent::LogMessage {
                text: nethacked_i18n::Messages::mount_steed(sess.locale).into(),
            });
        } else {
            events.push(GameEvent::LogMessage {
                text: nethacked_i18n::Messages::cannot_mount(sess.locale).into(),
            });
        }
    }

    events
}

/// ranged.rs `handle_dismount`: spend the turn first, then leave the steed.
fn execute_dismount(
    clk: &mut ClockFacts,
    h: &mut HeroFacts,
    sess: &SessionFacts,
) -> Vec<GameEvent> {
    let mut events = Vec::new();
    clk.scheduler.hero_act(NORMAL_SPEED);

    if h.hero.mount.is_some() {
        h.hero.mount = None;
        events.push(GameEvent::LogMessage {
            text: nethacked_i18n::Messages::dismount_steed(sess.locale).into(),
        });
    } else {
        events.push(GameEvent::LogMessage {
            text: nethacked_i18n::Messages::not_mounted(sess.locale).into(),
        });
    }

    events
}

/// combat.rs `on_actor_killed` (C `mon.c:3676-3726`): conduct kill tracking,
/// quest leader / nemesis / guardian kill penalties and rewards, temple
/// priest divine protection cancellation, tame / peaceful / hostile alignment
/// adjustments via C `adjalign`, quest artifact generation, and corpse
/// spawning.
#[allow(clippy::too_many_arguments)]
fn on_actor_killed(
    clk: &mut ClockFacts,
    obj: &mut ObjectsFacts,
    a: &mut ActorsFacts,
    h: &mut HeroFacts,
    geo: &GeographyFacts,
    sess: &SessionFacts,
    config: &OwnerBridgeConfig,
    rng: &mut rand_chacha::ChaCha8Rng,
    attacker_id: ActorId,
    defender_id: ActorId,
    events: &mut Vec<GameEvent>,
) -> owners::Result<()> {
    let Some(defender) = a.actors.get(defender_id).cloned() else {
        return Ok(());
    };

    if attacker_id == h.player_id {
        nethacked_core::conducts::record_kill(&mut h.conducts);
        award_kill_experience(rng, a, h, sess, config, &defender, events);
    }

    events.push(GameEvent::LogMessage {
        text: nethacked_i18n::Messages::killed(&defender.name, sess.locale),
    });

    if defender_id != h.player_id {
        let quest_cfg = nethacked_core::get_role_quest_config_or_default(&sess.role_name);
        let mdef = config.ruleset.monster_for_actor(&defender);
        // C mon.c:3684 tests `m_id == quest_status.leader_m_id`: only the
        // hero's own quest leader, not any MS_LEADER monster (the Tourist
        // nemesis, Master of Thieves, is MS_LEADER).
        let is_leader = defender.name.eq_ignore_ascii_case(quest_cfg.leader_name);
        let is_nemesis = defender.name.eq_ignore_ascii_case(quest_cfg.nemesis_name)
            || mdef.is_some_and(|m| m.msound == nethacked_data::MonsterSound::Nemesis);
        // C mon.c:3691/3694 classify the alignment adjustment by `msound`
        // alone, so the MS_LEADER Tourist nemesis gets no "Real good!" bonus.
        let align_nemesis = mdef.is_some_and(|m| m.msound == nethacked_data::MonsterSound::Nemesis);
        let is_guardian = defender.name.eq_ignore_ascii_case(quest_cfg.guardian_name)
            || mdef.is_some_and(|m| m.msound == nethacked_data::MonsterSound::Guardian);
        let is_priest = mdef.is_some_and(|m| m.is_priest())
            || defender.name.to_ascii_lowercase().contains("priest");

        if attacker_id == h.player_id {
            let lim = nethacked_core::alignlim(clk.scheduler.turn);
            let hero_align = a
                .actors
                .get(h.player_id)
                .map(|p| p.alignment)
                .unwrap_or(Alignment::Neutral);

            if is_leader {
                // REAL BAD! mon.c:3678
                let penalty = -(h.alignment_record + lim / 2);
                h.alignment_record = nethacked_core::adjalign(h.alignment_record, penalty, lim);
                h.divine_favor = nethacked_core::religion::clamp_favor(h.divine_favor - 7);
                h.player_luck = nethacked_core::mines::clamp_luck(h.player_luck - 20);
                h.quest_state.killed_leader = true;
                let probably = h.quest_state.progress == QuestProgress::Completed;
                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::bad_idea(probably, sess.locale).into(),
                });
                anger_quest_guardians(a, h, clk, sess, config);
            } else if align_nemesis {
                // Real good! mon.c:3691
                if !h.quest_state.killed_leader {
                    h.alignment_record = nethacked_core::adjalign(h.alignment_record, lim / 4, lim);
                }
            } else if is_guardian {
                // Bad mon.c:3689
                h.alignment_record = nethacked_core::adjalign(h.alignment_record, -(lim / 8), lim);
                h.divine_favor = nethacked_core::religion::clamp_favor(h.divine_favor - 1);
                h.player_luck = nethacked_core::mines::clamp_luck(h.player_luck - 4);
                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::bad_idea(true, sess.locale).into(),
                });
            } else if is_priest {
                // C mon.c:3697-3705
                let coaligned = defender.alignment == hero_align;
                let adj = if coaligned { -2 } else { 2 };
                h.alignment_record = nethacked_core::adjalign(h.alignment_record, adj, lim);
                if coaligned {
                    h.divine_protection = 0;
                }
                let maligntyp = mdef.map(|m| m.maligntyp).unwrap_or(0);
                if maligntyp == nethacked_core::peace::A_NONE
                    || defender.name.to_ascii_lowercase().contains("moloch")
                {
                    h.alignment_record = nethacked_core::adjalign(h.alignment_record, lim / 4, lim);
                }
            } else if defender.is_tame {
                // C mon.c:3706
                h.alignment_record = nethacked_core::adjalign(h.alignment_record, -15, lim);
                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::distant_thunder(sess.locale).into(),
                });
            } else if defender.is_peaceful {
                // C mon.c:3722
                h.alignment_record = nethacked_core::adjalign(h.alignment_record, -5, lim);
            }

            // C mon.c:3725: malign was already adjusted for u.ualign.type and randomization
            h.alignment_record = nethacked_core::adjalign(h.alignment_record, defender.malign, lim);
        }

        if is_nemesis {
            nethacked_core::attack_nemesis(&mut h.quest_state, 9999);
            let art_id = match quest_cfg.role_name.to_lowercase().as_str() {
                "valkyrie" => nethacked_data::ItemKindId::ART_ORB_OF_FATE,
                "wizard" => nethacked_data::ItemKindId::ART_EYE_OF_THE_AETHIOPICA,
                "barbarian" => nethacked_data::ItemKindId::ART_HEART_OF_AHRIMAN,
                "knight" => nethacked_data::ItemKindId::ART_MAGIC_MIRROR_OF_MERLIN,
                "monk" => nethacked_data::ItemKindId::ART_EYES_OF_THE_OVERWORLD,
                "rogue" => nethacked_data::ItemKindId::ART_MASTER_KEY_OF_THIEVERY,
                "tourist" => nethacked_data::ItemKindId::ART_YENDORIAN_EXPRESS_CARD,
                "healer" => nethacked_data::ItemKindId::ART_STAFF_OF_AESCULAPIUS,
                _ => nethacked_data::ItemKindId::ART_ORB_OF_DETECTION,
            };
            if let Some(art_rec) = config.ruleset.create_item_record_by_id(
                art_id,
                ItemLocation::Floor(defender.coord),
                Buc::Blessed,
            ) {
                spawn_item(obj, art_rec);
            }
            events.push(GameEvent::LogMessage {
                text: nethacked_i18n::Messages::quest_nemesis_defeat(
                    quest_cfg.nemesis_name,
                    quest_cfg.artifact_name,
                    sess.locale,
                ),
            });
        }

        let (ctx, monster_ctx) = item_birth_context(a, h, geo, clk);
        let registry = &h.genocide_registry;
        if let Some(corpse) = config.ruleset.create_initialized_corpse_for_actor(
            &defender,
            ItemLocation::Floor(defender.coord),
            ctx,
            &monster_ctx,
            |d| nethacked_core::genocide::is_genocided(registry, &d.name, d.glyph),
            |n| rng.random_range(0..n),
        ) {
            attach_generated_item(
                obj,
                &mut clk.object_timers,
                corpse,
                ItemLocation::Floor(defender.coord),
            )
            .map_err(|_| owners::Error::Invariant)?;
        }
    }
    Ok(())
}

pub(crate) fn outcome(
    action: &ServiceAction,
    obs: &[Observation],
    config: &OwnerBridgeConfig,
    rng: &mut rand_chacha::ChaCha8Rng,
) -> owners::Result<ServicesOutcome> {
    let mut clk: ClockFacts = fact(obs, 0)?;
    let mut obj: ObjectsFacts = fact(obs, 1)?;
    let mut a: ActorsFacts = fact(obs, 2)?;
    let mut h: HeroFacts = fact(obs, 3)?;
    let mut eq: EquipmentFacts = fact(obs, 4)?;
    let mut bill: BillingFacts = fact(obs, 5)?;
    let mut geo: GeographyFacts = fact(obs, 7)?;
    let sess: SessionFacts = fact(obs, 8)?;

    // Legacy `step_player_action` checks deferred billing settlement before
    // the hero fetch (actions/mod.rs): a deferred Pay settles nothing and
    // spends no time.
    if matches!(action, ServiceAction::Pay)
        && (!bill.billing.live_terms.is_empty() || !bill.billing.consumed.is_empty())
    {
        return Ok(ServicesOutcome::empty());
    }

    let Some(mut hero_actor) = a.actors.get(h.player_id).cloned() else {
        return Err(owners::Error::Unsupported);
    };
    if hero_actor.is_dead || hero_actor.hp == 0 {
        return Ok(ServicesOutcome::empty());
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

    let energy_before = clk.scheduler.hero_energy;
    let turn_before = clk.scheduler.turn;

    // 2. Execute the service action
    let mut events = Vec::new();
    let mut geo_delta = GeographyDelta::default();

    // C multi < 0 (fall_asleep, paralysis): the hero cannot act this turn;
    // the action degenerates to a stationary wait (actions/mod.rs).
    let spent_time = if h.hero.afflictions.transient.helpless > 0 || eq.equipment.dressing.is_some()
    {
        clk.scheduler.hero_act(NORMAL_SPEED);
        clk.scheduler.hero_energy != energy_before
    } else {
        match action {
            ServiceAction::Cast { spell_index, dir } => {
                let cast_events = execute_cast(
                    &mut clk,
                    &mut obj,
                    &mut a,
                    &mut h,
                    &mut geo,
                    &mut geo_delta,
                    &sess,
                    config,
                    rng,
                    *spell_index,
                    *dir,
                )?;
                events.extend(cast_events);
                clk.scheduler.hero_energy != energy_before
            }
            ServiceAction::Pray => {
                let pray_events = execute_pray(&mut clk, &mut obj, &mut a, &mut h, &geo, &sess);
                events.extend(pray_events);
                clk.scheduler.hero_energy != energy_before
            }
            ServiceAction::Sacrifice(idx) => {
                let sacrifice_events = execute_sacrifice(
                    &mut clk,
                    &mut obj,
                    &mut a,
                    &mut h,
                    &mut eq,
                    &mut geo,
                    &mut geo_delta,
                    &sess,
                    config,
                    *idx,
                );
                events.extend(sacrifice_events);
                clk.scheduler.hero_energy != energy_before
            }
            ServiceAction::Donate(amount) => {
                let donate_events =
                    execute_donate(&mut clk, &mut obj, &a, &mut h, &mut bill, rng, *amount);
                events.extend(donate_events);
                clk.scheduler.hero_energy != energy_before
            }
            ServiceAction::Pay => {
                let pay_events = execute_pay(&mut clk, &obj, &a, &h, &mut bill, &sess);
                events.extend(pay_events);
                clk.scheduler.hero_energy != energy_before
            }
            ServiceAction::PriceCheck(item_index) => {
                let price_events =
                    execute_price_check(&mut clk, &obj, &a, &h, &sess, config, *item_index);
                events.extend(price_events);
                clk.scheduler.hero_energy != energy_before
            }
            ServiceAction::Wish(wish) => {
                let wish_events = execute_wish(
                    &mut clk,
                    &mut obj,
                    &a,
                    &mut h,
                    &geo,
                    &sess,
                    config,
                    rng,
                    wish.clone(),
                )?;
                events.extend(wish_events);
                clk.scheduler.hero_energy != energy_before
            }
            ServiceAction::Mount(actor_id) => {
                let mount_events = execute_mount(&mut clk, &mut a, &mut h, &sess, *actor_id);
                events.extend(mount_events);
                clk.scheduler.hero_energy != energy_before
            }
            ServiceAction::Dismount => {
                let dismount_events = execute_dismount(&mut clk, &mut h, &sess);
                events.extend(dismount_events);
                clk.scheduler.hero_energy != energy_before
            }
        }
    };

    // The action mutates the player actor in place (cast heals, prayer and
    // sacrifice hp, donation nothing, mount state lives in h.hero); fold
    // those into the loop's working copy so monster ticks and the final
    // write-back observe the post-action hero.
    if let Some(p) = a.actors.get(h.player_id) {
        hero_actor.coord = p.coord;
        hero_actor.hp = p.hp;
        hero_actor.max_hp = p.max_hp;
        hero_actor.level = p.level;
        hero_actor.is_dead = p.is_dead;
        hero_actor.intrinsics = p.intrinsics;
    }

    // 3. Final equipment reconciliation and derived AC (pre-turn-ticks)
    let borrowed = BorrowedObjects {
        items: &obj.items,
        rules: &config.ruleset,
    };
    eq.equipment = command_frame::reconcile(&eq.equipment, &borrowed, h.player_id);
    hero_actor.ac =
        command_frame::armor_class(&eq.equipment, &borrowed, h.player_id, h.divine_protection);

    // 4. Turn-stepping loop. Service actions never walk, so the legacy
    // `moved` flag (actions/mod.rs) is always false here.
    let moved = false;
    let mut rounds = 0u32;
    if spent_time {
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

                if let Some(p) = a.actors.get_mut(h.player_id) {
                    *p = hero_actor.clone();
                }

                let mon_clk = super::monster_ticks::ClockFacts {
                    turn: clk.scheduler.turn,
                };
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
                    mon_id, &mon_clk, &mon_obj, &mon_act, &mon_hero, &mon_geo, &mon_sess, config,
                    rng,
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
                    // Legacy monster attacks go through `damage_hero`, which
                    // syncs the hero's current form from the player actor (the
                    // authoritative hp/max_hp pair) before applying damage.
                    let hp = i32::try_from(h_hp).unwrap_or(i32::MAX);
                    let max_hp = i32::try_from(hero_actor.max_hp).unwrap_or(i32::MAX);
                    if let Some(poly) = h.hero.polymorph.as_mut() {
                        poly.hp = hp;
                        poly.max_hp = max_hp;
                    } else {
                        h.hero.base_hp = hp;
                        h.hero.base_max_hp = max_hp;
                    }
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

            if hero_actor.is_dead || hero_actor.hp == 0 || clk.scheduler.hero_energy >= NORMAL_SPEED
            {
                break;
            }

            if a.actors.iter().any(|(_, actor)| {
                !actor.is_player && !actor.is_dead && actor.hp > 0 && actor.movement >= NORMAL_SPEED
            }) {
                continue;
            }

            rounds += 1;

            for (_, actor) in a.actors.iter_mut() {
                if !actor.is_player && !actor.is_dead && actor.hp > 0 {
                    actor.mspec_used = actor.mspec_used.saturating_sub(1);
                }
            }

            for (_, actor) in a.actors.iter_mut() {
                if !actor.is_player && !actor.is_dead && actor.hp > 0 {
                    let gain =
                        monster_movement_gain(actor.speed, |bound| rng.random_range(0..bound));
                    actor.movement = actor.movement.saturating_add(gain);
                }
            }

            let burden = compute_hero_encumbrance(&h, &obj.items, &hero_actor);
            let properties = hero_actor.intrinsics;
            let gain = hero_movement_gain(
                clk.scheduler.hero_speed,
                properties.fast,
                properties.very_fast,
                burden,
                |bound| rng.random_range(0..bound),
            );
            clk.scheduler.hero_energy = clk.scheduler.hero_energy.saturating_add(gain);
            clk.scheduler.turn += 1;

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

            events.push(GameEvent::TurnAdvanced {
                turn: clk.scheduler.turn,
            });

            let (affliction_result, _, aff_events) =
                passive_afflictions::recovery(&mut h.hero.afflictions);
            events.extend(aff_events);
            if let Some(fatal_msg) =
                passive_afflictions::fatal_message(&affliction_result, sess.locale)
            {
                hero_actor.is_dead = true;
                hero_actor.hp = 0;
                events.push(fatal_msg);
            }

            while let Some(timer) = clk.object_timers.pop_due_active(clk.scheduler.turn) {
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
                                let due = clk.scheduler.turn.checked_add(rot_delay);
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
            ) && !(moved && burden >= EncumbranceTier::Stressed)
            {
                let level = hero_actor.level;
                let con = h.progress.attr_base[crate::progress::A_CON];
                let heal = natural_hp_heal(level, con, |bound| rng.random_range(0..bound));
                hero_actor.hp = hero_actor.hp.saturating_add(heal).min(hero_actor.max_hp);
            }

            let actor_summary = Some(crate::passive_metabolism::Actor {
                hp: hero_actor.hp,
                dead: hero_actor.is_dead,
            });
            let met_out = crate::passive_metabolism::tick(
                h.player_nutrition,
                actor_summary,
                clk.scheduler.turn,
                sess.locale,
            );
            h.player_nutrition = met_out.nutrition;
            if let Some(next) = met_out.actor {
                hero_actor.hp = next.hp;
                hero_actor.is_dead = next.dead;
            }
            events.extend(met_out.events);

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

            if clk.scheduler.hero_energy >= NORMAL_SPEED {
                break;
            }
        }
    }

    // 5. Post-loop. Legacy guards the prayer cooldown tick on spent_time
    // (actions/mod.rs); a refused action leaves the timeout untouched.
    if spent_time {
        h.prayer_timeout = nethacked_core::religion::tick_prayer_timeout(h.prayer_timeout);
    }

    let luck_period =
        nethacked_core::luck_decay_period(hero_has_amulet(&obj.items, h.player_id), false);
    if clk.scheduler.turn != turn_before
        && clk.scheduler.turn > 0
        && clk.scheduler.turn % luck_period == 0
    {
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

    Ok(ServicesOutcome {
        spent_time,
        rounds,
        clock: Some(ClockOutcome {
            scheduler: clk.scheduler,
            object_timers: clk.object_timers,
        }),
        actors: Some(ActorsOutcome { actors: a.actors }),
        hero: Some(HeroOutcome {
            hero: h.hero,
            player_nutrition: h.player_nutrition,
            player_pw: h.player_pw,
            player_max_pw: h.player_max_pw,
            known_spells: h.known_spells,
            divine_favor: h.divine_favor,
            prayer_timeout: h.prayer_timeout,
            divine_gift_count: h.divine_gift_count,
            divine_protection: h.divine_protection,
            priest_cheapskate: h.priest_cheapskate,
            player_luck: h.player_luck,
            alignment_record: h.alignment_record,
            progress: h.progress,
            genocide_registry: h.genocide_registry,
            conducts: h.conducts,
            candelabrum_state: h.candelabrum_state,
            ritual_progress: h.ritual_progress,
            quest_state: h.quest_state,
        }),
        objects: Some(ObjectsOutcome {
            items: obj.items,
            artifact_history: obj.artifact_history,
        }),
        equipment: Some(EquipmentOutcome {
            equipment: eq.equipment,
            wielded_item: eq.wielded_item,
        }),
        billing: Some(BillingOutcome {
            unpaid_items: bill.unpaid_items,
            billing: bill.billing,
            player_gold: bill.player_gold,
        }),
        geography: Some(GeographyOutcome { delta: geo_delta }),
        events,
    })
}

trait Domain: Clone + Serialize + 'static {
    const INDEX: usize;
    const WRITE_NAME: Option<&'static str>;
    fn read(
        &self,
        request: &[u8],
        obs: &[Observation],
        c: &OwnerBridgeConfig,
    ) -> owners::Result<Vec<u8>>;
    fn prepare(
        &self,
        _request: &[u8],
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

struct ServicesCodec<T>(OwnerBridgeConfig, std::marker::PhantomData<T>);
impl<T: Domain> Schema<T> for ServicesCodec<T> {
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
        s.read(request, obs, &self.0)
    }
}

impl<T: Domain> NativeBehavior<T> for ServicesCodec<T> {
    fn prepare(
        &self,
        s: &T,
        name: &str,
        request: &[u8],
        obs: &[Observation],
    ) -> owners::Result<Proposal<T>> {
        if T::WRITE_NAME != Some(name) {
            return Err(owners::Error::Unsupported);
        }
        if T::INDEX == 6 {
            return s.prepare(request, obs, &self.0);
        }
        if !obs.is_empty() {
            return Err(owners::Error::Schema);
        }
        let req: PrepareV1 = decode(request, PAYLOAD)?;
        s.prepare_intent(req, &self.0)
    }
}

fn proposed<T>(
    next: T,
    events: Vec<Vec<u8>>,
    result: &ServicesOutcome,
) -> owners::Result<Proposal<T>> {
    Ok(Proposal {
        next,
        events,
        reply: encode(result, PAYLOAD)?,
    })
}

/// Non-random candidates publish a brief empty reply: the full outcome rides
/// only on the Random candidate's reply (the accounting decodes it from
/// there), so the commit's 16 KiB reply bound carries one outcome, not seven.
fn proposed_brief<T>(next: T, events: Vec<Vec<u8>>) -> owners::Result<Proposal<T>> {
    Ok(Proposal {
        next,
        events,
        reply: Vec::new(),
    })
}

fn preview(obs: &[Observation]) -> owners::Result<ServicesOutcome> {
    fact(obs, 6)
}

impl Domain for ClockState {
    const INDEX: usize = 0;
    const WRITE_NAME: Option<&'static str> = Some(WRITES[0]);
    fn read(&self, _: &[u8], _: &[Observation], _: &OwnerBridgeConfig) -> owners::Result<Vec<u8>> {
        encode(
            &ClockFacts {
                scheduler: self.scheduler,
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
            next.scheduler = c.scheduler;
            next.object_timers = c.object_timers.clone();
        }
        proposed_brief(next, vec![])
    }
}

impl Domain for ObjectsState {
    const INDEX: usize = 1;
    const WRITE_NAME: Option<&'static str> = Some(WRITES[1]);
    fn read(&self, _: &[u8], _: &[Observation], _: &OwnerBridgeConfig) -> owners::Result<Vec<u8>> {
        encode(
            &ObjectsFacts {
                items: self.items.clone(),
                artifact_history: self.artifact_history.clone(),
                stored_items: self.stored_items.clone(),
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
            next.artifact_history = o.artifact_history.clone();
            // Cached records retain original handles for ancestry lookup
            // (turns.rs): stored-level corpses age one step per round, but a
            // corpse inside an ice box on that same stored level stays frozen.
            for stored in &mut next.stored_items {
                let advancing: Vec<usize> = stored
                    .iter()
                    .enumerate()
                    .filter_map(|(index, (_, item))| {
                        (item.is_corpse()
                            && !frozen_in_ice_box(item, |parent| {
                                stored
                                    .iter()
                                    .find(|(id, _)| *id == parent)
                                    .map(|(_, item)| item)
                            }))
                        .then_some(index)
                    })
                    .collect();
                for index in advancing {
                    stored[index].1.corpse_age = stored[index]
                        .1
                        .corpse_age
                        .saturating_add(req.outcome.rounds);
                }
            }
        }
        proposed_brief(next, vec![])
    }
}

impl Domain for ActorsState {
    const INDEX: usize = 2;
    const WRITE_NAME: Option<&'static str> = Some(WRITES[2]);
    fn read(&self, _: &[u8], _: &[Observation], _: &OwnerBridgeConfig) -> owners::Result<Vec<u8>> {
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
        proposed_brief(next, events)
    }
}

impl Domain for HeroState {
    const INDEX: usize = 3;
    const WRITE_NAME: Option<&'static str> = Some(WRITES[3]);
    fn read(&self, _: &[u8], _: &[Observation], _: &OwnerBridgeConfig) -> owners::Result<Vec<u8>> {
        encode(
            &HeroFacts {
                player_id: self.player_id,
                hero: self.hero.clone(),
                player_nutrition: self.player_nutrition,
                player_pw: self.player_pw,
                player_max_pw: self.player_max_pw,
                known_spells: self.known_spells.clone(),
                divine_favor: self.divine_state.favor,
                prayer_timeout: self.divine_state.prayer_timeout,
                divine_gift_count: self.divine_state.gift_count,
                divine_protection: self.divine_protection,
                priest_cheapskate: self.priest_cheapskate,
                player_luck: self.player_luck,
                hero_race: self.hero_race,
                alignment_record: self.alignment_record,
                progress: self.progress.clone(),
                genocide_registry: self.genocide_registry.clone(),
                conducts: self.conducts.clone(),
                candelabrum_state: self.candelabrum_state,
                ritual_progress: self.ritual_progress,
                quest_state: self.quest_state.clone(),
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
            next.player_pw = h.player_pw;
            next.player_max_pw = h.player_max_pw;
            next.known_spells = h.known_spells.clone();
            next.divine_state.favor = h.divine_favor;
            next.divine_state.prayer_timeout = h.prayer_timeout;
            next.divine_state.gift_count = h.divine_gift_count;
            next.divine_protection = h.divine_protection;
            next.priest_cheapskate = h.priest_cheapskate;
            next.player_luck = h.player_luck;
            next.alignment_record = h.alignment_record;
            next.progress = h.progress.clone();
            next.genocide_registry = h.genocide_registry.clone();
            next.conducts = h.conducts.clone();
            next.candelabrum_state = h.candelabrum_state;
            next.ritual_progress = h.ritual_progress;
            next.quest_state = h.quest_state.clone();
        }
        proposed_brief(next, vec![])
    }
}

impl Domain for EquipmentState {
    const INDEX: usize = 4;
    const WRITE_NAME: Option<&'static str> = Some(WRITES[4]);
    fn read(&self, _: &[u8], _: &[Observation], _: &OwnerBridgeConfig) -> owners::Result<Vec<u8>> {
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
            next.wielded_item = eq.wielded_item;
        }
        proposed_brief(next, vec![])
    }
}

impl Domain for BillingState {
    const INDEX: usize = 5;
    const WRITE_NAME: Option<&'static str> = Some(WRITES[5]);
    fn read(&self, _: &[u8], _: &[Observation], _: &OwnerBridgeConfig) -> owners::Result<Vec<u8>> {
        encode(
            &BillingFacts {
                unpaid_items: self.unpaid_items.clone(),
                billing: self.billing.clone(),
                player_gold: self.player_gold,
                stored_unpaid_items: self.stored_unpaid_items.clone(),
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
        if let Some(b) = &req.outcome.billing {
            next.unpaid_items = b.unpaid_items.clone();
            next.billing = b.billing.clone();
            next.player_gold = b.player_gold;
        }
        proposed_brief(next, vec![])
    }
}

impl Domain for RandomState {
    const INDEX: usize = 6;
    const WRITE_NAME: Option<&'static str> = Some(WRITES[7]);
    fn read(
        &self,
        request: &[u8],
        obs: &[Observation],
        c: &OwnerBridgeConfig,
    ) -> owners::Result<Vec<u8>> {
        let action: ServiceAction = decode(request, PAYLOAD)?;
        encode(&outcome(&action, obs, c, &mut self.rng.clone())?, PAYLOAD)
    }
    fn prepare(
        &self,
        request: &[u8],
        obs: &[Observation],
        c: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        let action: ServiceAction = decode(request, PAYLOAD)?;
        let mut next = self.clone();
        let result = outcome(&action, obs, c, &mut next.rng)?;
        proposed(next, vec![], &result)
    }
}

impl Domain for GeographyState {
    const INDEX: usize = 7;
    const WRITE_NAME: Option<&'static str> = Some(WRITES[6]);
    fn read(&self, _: &[u8], _: &[Observation], _: &OwnerBridgeConfig) -> owners::Result<Vec<u8>> {
        encode(
            &GeographyFacts {
                level: self.level.clone(),
                branch: self.current_branch,
                depth: self.depth,
                vibrating_square: self.vibrating_square,
                stored_keys: self.stored_keys.clone(),
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
        if let Some(g) = &req.outcome.geography {
            g.delta.apply(&mut next.level);
        }
        proposed_brief(next, vec![])
    }
}

impl Domain for SessionState {
    const INDEX: usize = 8;
    const WRITE_NAME: Option<&'static str> = None;
    fn read(&self, _: &[u8], _: &[Observation], _: &OwnerBridgeConfig) -> owners::Result<Vec<u8>> {
        encode(
            &SessionFacts {
                role_name: self.role_name.clone(),
                role_id: self.role_id,
                locale: self.locale,
            },
            PAYLOAD,
        )
    }
}

pub(super) struct ServicesAccounting {
    pub(super) config: OwnerBridgeConfig,
    pub(super) action: ServiceAction,
}

impl Accounting for ServicesAccounting {
    fn validate(&self, p: &[ProposalView], obs: &[Observation]) -> owners::Result<()> {
        if obs.len() != 9 {
            return Err(owners::Error::Schema);
        }
        let random = p
            .iter()
            .find(|x| x.receipt.command == WRITES[7])
            .ok_or(owners::Error::Invariant)?;
        let before: RandomState = decode(&random.before, PAYLOAD)?;
        let result: ServicesOutcome = decode(&random.reply, PAYLOAD)?;
        let mut expected = before.clone();
        let expected_outcome = outcome(&self.action, obs, &self.config, &mut expected.rng)?;
        if encode(&expected, PAYLOAD)? != random.proposed
            || encode(&expected_outcome, PAYLOAD)? != random.reply
            || encode(&expected_outcome, PAYLOAD)?
                != fact::<ServicesOutcome>(obs, 6).and_then(|r| encode(&r, PAYLOAD))?
            || !random.events.is_empty()
        {
            return Err(owners::Error::Invariant);
        }

        let expected_count = 8;
        if p.len() != expected_count {
            return Err(owners::Error::Invariant);
        }
        let req = PrepareV1 {
            outcome: result.clone(),
        };
        for x in p.iter().filter(|x| x.receipt.command != WRITES[7]) {
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
                "svc_clock_prepare" => check!(ClockState),
                "svc_objects_prepare" => check!(ObjectsState),
                "svc_actors_prepare" => check!(ActorsState),
                "svc_hero_prepare" => check!(HeroState),
                "svc_equipment_prepare" => check!(EquipmentState),
                "svc_billing_prepare" => check!(BillingState),
                "svc_geography_prepare" => check!(GeographyState),
                _ => return Err(owners::Error::Invariant),
            }
        }
        for n in WRITES {
            if p.iter().filter(|x| x.receipt.command == n).count() != 1 {
                return Err(owners::Error::Invariant);
            }
        }
        Ok(())
    }
}

fn identity() -> op::ArtifactIdentity {
    artifact("native-services-accounting-i8", 73)
}

pub(super) fn capture(
    a: &mut op::AccountingCaptureCatalog,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if c.services {
        a.register(
            identity(),
            POLICY,
            Rc::new(ServicesAccounting {
                config: c.clone(),
                action: ServiceAction::Pray,
            }),
        )?;
    }
    Ok(())
}

pub(super) fn restore(
    a: &mut op::AccountingRestoreCatalog,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if c.services {
        let cc = c.clone();
        a.allow(identity(), POLICY, move || {
            Ok(Rc::new(ServicesAccounting {
                config: cc.clone(),
                action: ServiceAction::Pray,
            }))
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

pub(super) struct ServicesTransport {
    kernel: Kernel,
    caller: ModuleHandle,
    routes: Rc<Routes>,
    resources: Vec<Resource>,
}

impl ServicesTransport {
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
            let action: ServiceAction = decode(bytes, PAYLOAD).map_err(native_refusal)?;
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
                &action,
                &obs,
                &c_preview,
                &mut rand_chacha::ChaCha8Rng::seed_from_u64(0),
            )
            .map_err(native_refusal)?;
            let guards = obs
                .iter()
                .map(|o| wand::OwnerWitness::from(o.stamp()))
                .collect();
            encode(&ServicesRequest { action, guards }, PAYLOAD).map_err(native_refusal)
        });

        let run_port: NativePort = Rc::new(move |bytes, view| {
            let req: ServicesRequest = decode(bytes, PAYLOAD).map_err(native_refusal)?;
            let action_bytes = encode(&req.action, PAYLOAD).map_err(native_refusal)?;
            let r = r.get()?;
            let read = || -> owners::Result<Vec<Observation>> {
                let mut obs = vec![
                    r.clock.query(READS[0], &action_bytes, &[], view)?,
                    r.objects.query(READS[1], &action_bytes, &[], view)?,
                    r.actors.query(READS[2], &action_bytes, &[], view)?,
                    r.hero.query(READS[3], &action_bytes, &[], view)?,
                    r.equipment.query(READS[4], &action_bytes, &[], view)?,
                    r.billing.query(READS[5], &action_bytes, &[], view)?,
                    r.geography.query(READS[7], &action_bytes, &[], view)?,
                    r.session.query(READS[8], &action_bytes, &[], view)?,
                ];
                obs.push(r.random.query(READS[6], &action_bytes, &obs, view)?);
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
                r.billing
                    .prepare(WRITES[5], &intent, vec![], view)
                    .map_err(|e| native_refusal_step("billing.prepare", e))?
                    .erase(),
                r.geography
                    .prepare(WRITES[6], &intent, vec![], view)
                    .map_err(|e| native_refusal_step("geography.prepare", e))?
                    .erase(),
                r.random
                    .prepare(WRITES[7], &action_bytes, other, view)
                    .map_err(|e| native_refusal_step("random.prepare", e))?
                    .erase(),
            ];
            let writes = vec![
                r.clock.id(),
                r.objects.id(),
                r.actors.id(),
                r.hero.id(),
                r.equipment.id(),
                r.billing.id(),
                r.geography.id(),
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
                    Rc::new(ServicesAccounting {
                        config: c.clone(),
                        action: req.action.clone(),
                    }),
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
                &ServicesReply {
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
            kernel.register_native(manifest(HOME, exports.clone(), vec![]), &[76; 32], ports)?;
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
        let caller_preview: NativePort = Rc::new(|b, v| v.call_dependency(17, b));
        let caller_run: NativePort = Rc::new(|b, v| v.call_dependency(18, b));
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
            &[77; 32],
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
                if let Some(write_name) = <$ty>::WRITE_NAME {
                    ports.insert(
                        write_name.into(),
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
                        schema: [index as u8 + 230; 32],
                        ports,
                    },
                    $state,
                    Backend::Native(Rc::new(ServicesCodec::<$ty>(
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
            ($owner:ident, $index:expr, $write:expr) => {
                resources.push(r.$owner.resource(READS[$index]).map_err(native_refusal)?);
                if let Some(write_name) = $write {
                    resources.push(r.$owner.resource(write_name).map_err(native_refusal)?);
                }
            };
        }
        resource!(clock, 0, Some(WRITES[0]));
        resource!(objects, 1, Some(WRITES[1]));
        resource!(actors, 2, Some(WRITES[2]));
        resource!(hero, 3, Some(WRITES[3]));
        resource!(equipment, 4, Some(WRITES[4]));
        resource!(billing, 5, Some(WRITES[5]));
        resource!(random, 6, Some(WRITES[7]));
        resource!(geography, 7, Some(WRITES[6]));
        resource!(session, 8, None);

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

        // Slot 17: PREVIEW
        kernel.issue_grant(caller, 17, scoped(Access::Read, resources.clone()))?;
        // Slot 18: RUN
        resources.push(host.publication_resource());
        kernel.issue_grant(caller, 18, scoped(Access::Write, resources.clone()))?;

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

    pub(super) fn preview(
        &mut self,
        action: &ServiceAction,
    ) -> Result<ServicesRequest, OwnerBridgeError> {
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
            .invoke_operation(&operation, &encode(action, PAYLOAD)?, authority);
        let _ = self.kernel.release_pipeline(lease);
        decode(&reply?, PAYLOAD).map_err(Into::into)
    }

    pub(super) fn execute(
        &mut self,
        request: ServicesRequest,
    ) -> Result<(ServicesReply, NativePartitions), OwnerBridgeError> {
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
        let outcome: ServicesReply = decode(&reply?, PAYLOAD)?;

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
    fn services_session_witnesses(&self) -> Result<Vec<wand::OwnerWitness>, OwnerBridgeError> {
        self.session.with_runtime(&self.credential.borrow(), |b| {
            Ok(bridge_stamps(b)?
                .iter()
                .map(wand::OwnerWitness::from)
                .collect())
        })?
    }

    pub fn prepare_service(
        &self,
        action: ServiceAction,
    ) -> Result<ServicesRequest, OwnerBridgeError> {
        if !self.config.services {
            return Err(owners::Error::Unsupported.into());
        }
        let parts = self.partitions()?;
        let mut transport = ServicesTransport::install(parts, self.config.clone())?;
        let _ = transport.preview(&action)?;
        let guards = self.services_session_witnesses()?;
        Ok(ServicesRequest { action, guards })
    }

    pub fn execute_service(
        &self,
        request: ServicesRequest,
    ) -> Result<ServicesReply, OwnerBridgeError> {
        if !self.config.services {
            return Err(owners::Error::Unsupported.into());
        }
        let current_witnesses = self.services_session_witnesses()?;
        if request.guards != current_witnesses {
            #[cfg(test)]
            STALE_REFUSALS.with(|x| x.set(x.get() + 1));
            return Err(owners::Error::Stale.into());
        }
        let parts = self.partitions()?;
        let mut transport = ServicesTransport::install(parts, self.config.clone())?;
        let (reply, updated) = transport.execute(request)?;
        let source = Self::install(updated, self.config.clone())?;
        let wire = source.capture()?;
        drop(source);
        self.restore(&wire)?;
        Ok(reply)
    }

    pub fn step_service(&self, action: ServiceAction) -> Result<ServicesReply, OwnerBridgeError> {
        let req = self.prepare_service(action)?;
        self.execute_service(req)
    }
}
