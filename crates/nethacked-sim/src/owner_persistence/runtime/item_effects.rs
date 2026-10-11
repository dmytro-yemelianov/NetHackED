//! Explicit M6-I7 item-effects dispatch: Dip, Quaff, Read, Eat, Rub, Apply,
//! Engrave and both wand-zap entry paths.
use super::*;
use crate::actions::command_frame::{self, BorrowedObjects};
use crate::passive_afflictions;
use crate::passive_dressing;
use crate::turns::natural_hp_heal;
use crate::GameEvent;
use nethacked_arena::{
    frozen_in_ice_box, ActorId, ActorRecord, ActorStorage, ItemId, ItemLocation, ItemRecord,
    ItemStorage,
};
use nethacked_core::buc::WaterType;
use nethacked_core::energy::{
    hero_movement_gain, monster_movement_gain, SchedulerState, NORMAL_SPEED,
};
use nethacked_core::engraving::{Engraving, EngravingMedium};
use nethacked_core::inventory::EncumbranceTier;
use nethacked_core::SpellKind;
use nethacked_dungeon::DungeonLevel;
use nethacked_kernel::ModuleHandle;
use nethacked_types::{
    BranchId, Buc, Coord, Direction, ItemClass, Locale, SlotId, Tile, TrapRecord,
};
use rand::{Rng, RngCore, SeedableRng};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use wand::OwnerWitness;

pub const HOME: &str = "item-effects-home-i7-v1";
pub const CALLER: &str = "item-effects-caller-i7-v1";
pub const PREVIEW: &str = "item_effects_preview_i7";
pub const RUN: &str = "item_effects_run_i7";

pub const READS: [&str; 9] = [
    "itemfx_clock_read",
    "itemfx_objects_read",
    "itemfx_actors_read",
    "itemfx_hero_read",
    "itemfx_equipment_read",
    "itemfx_billing_read",
    "itemfx_random_read",
    "itemfx_geography_read",
    "itemfx_session_read",
];

pub const WRITES: [&str; 8] = [
    "itemfx_clock_prepare",
    "itemfx_objects_prepare",
    "itemfx_actors_prepare",
    "itemfx_hero_prepare",
    "itemfx_equipment_prepare",
    "itemfx_billing_prepare",
    "itemfx_geography_prepare",
    "itemfx_random_prepare",
];

pub const POLICY: [u8; 32] = [72; 32];

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
pub enum ItemEffectAction {
    Dip {
        item_index: usize,
        into_water: WaterType,
    },
    Quaff(usize),
    Read(usize),
    Eat(usize),
    Rub(usize),
    Apply(usize),
    Engrave {
        text: String,
        medium: EngravingMedium,
    },
    ZapWand {
        dir: Direction,
        energy: u32,
    },
    ZapSelectedWand {
        item_index: u32,
        expected_item: Option<SlotId>,
        dir: Direction,
        energy: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemEffectsRequest {
    pub action: ItemEffectAction,
    pub guards: Vec<OwnerWitness>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemEffectsReply {
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
    pub divine_protection: u32,
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
    pub stored_unpaid_items: Vec<Vec<(ItemId, u32)>>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct GeographyFacts {
    #[serde(with = "nethacked_dungeon::compact_level")]
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
    pub divine_protection: u32,
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
pub struct ItemEffectsOutcome {
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

impl ItemEffectsOutcome {
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
    pub(super) outcome: ItemEffectsOutcome,
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

/// Arena `spawn_actor`: plain insertion.
fn spawn_actor(actors: &mut ActorStorage, actor: ActorRecord) {
    actors.insert(actor);
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

/// Draws one C enchant roll from the shared RNG (`rn2(n)`: `0..n`,
/// `rnd(n)`: `1..=n`); no draw yields 0 (items.rs `draw_enchant`).
fn draw_enchant(rng: &mut rand_chacha::ChaCha8Rng, draw: nethacked_core::EnchantDraw) -> u32 {
    match draw {
        nethacked_core::EnchantDraw::None => 0,
        nethacked_core::EnchantDraw::Rn2(n) => rng.random_range(0..n.max(1)),
        nethacked_core::EnchantDraw::Rnd(n) => rng.random_range(1..=n.max(1)),
    }
}

/// Applies an enchant scroll outcome to item `id` and logs it (items.rs
/// `apply_enchant_outcome`).
fn apply_enchant_outcome(
    obj: &mut ObjectsFacts,
    eq: &mut EquipmentFacts,
    id: ItemId,
    outcome: nethacked_core::EnchantOutcome,
    is_armor: bool,
    events: &mut Vec<GameEvent>,
) {
    match outcome {
        nethacked_core::EnchantOutcome::Evaporated => {
            let name = obj
                .items
                .get(id)
                .map(|it| it.name.clone())
                .unwrap_or_default();
            destroy_item(obj, id);
            if eq.wielded_item == Some(id) {
                eq.wielded_item = None;
            }
            events.push(GameEvent::LogMessage {
                text: format!("Your {name} glows violently and evaporates!"),
            });
        }
        nethacked_core::EnchantOutcome::Changed(new_spe) => {
            if let Some(it) = obj.items.get_mut(id) {
                it.enchantment = new_spe;
                let text = if is_armor {
                    format!(
                        "Your {} glows with a protective silver sheen! ({:+})",
                        it.name, new_spe
                    )
                } else {
                    format!(
                        "Your {} glows with a silvery aura! ({:+})",
                        it.name, new_spe
                    )
                };
                events.push(GameEvent::LogMessage { text });
            }
        }
    }
}

/// Current sim geography exposes Astral only (world.rs
/// `monster_spawn_context`).
fn monster_spawn_context(branch: BranchId) -> nethacked_data::monsters::MonsterSpawnContext {
    nethacked_data::monsters::MonsterSpawnContext {
        endgame_plane: (branch == BranchId::AstralPlane)
            .then_some(nethacked_types::EndgamePlane::Astral),
    }
}

/// C `set_malign(mtmp)` (`makemon.c:2320-2366`, peace.rs
/// `set_monster_malign`): precalculate alignment adjustment upon monster
/// death.
fn set_monster_malign(
    actors: &ActorStorage,
    player_id: ActorId,
    role_name: &str,
    rec: &mut ActorRecord,
    def: &nethacked_data::ruleset::MonsterDef,
) {
    let quest_cfg = nethacked_core::get_role_quest_config_or_default(role_name);
    let is_leader = matches!(def.msound, nethacked_data::MonsterSound::Leader)
        || rec.name.eq_ignore_ascii_case(quest_cfg.leader_name);
    let hero_alignment = actors
        .get(player_id)
        .map(|p| p.alignment)
        .unwrap_or(nethacked_types::Alignment::Neutral);
    rec.malign = nethacked_core::peace::calculate_malign(
        def.maligntyp,
        hero_alignment,
        rec.is_peaceful,
        is_leader,
        def.peaceful_by_default,
        def.always_hostile,
    );
}

/// world.rs `actor_is_genocided`.
fn actor_is_genocided(
    ruleset: &crate::Ruleset,
    registry: &nethacked_types::GenocideRegistry,
    name: &str,
) -> bool {
    let lower = name.to_lowercase();
    let class = ruleset.monster_class_of(name);
    registry.genocided_species.contains(&lower)
        || class
            .map(|c| registry.genocided_classes.contains(&c))
            .unwrap_or(false)
}

/// The genocide scroll's floor wipe: remove genocided actors, leaving anything
/// they carried on the floor where they stood (world.rs
/// `remove_actor_dropping_items`).
fn genocide_wipe(
    obj: &mut ObjectsFacts,
    a: &mut ActorsFacts,
    h: &HeroFacts,
    ruleset: &crate::Ruleset,
) {
    let mut to_remove = Vec::new();
    for (aid, actor) in a.actors.iter() {
        if !actor.is_player && actor_is_genocided(ruleset, &h.genocide_registry, &actor.name) {
            to_remove.push(aid);
        }
    }
    for aid in to_remove {
        let coord = a.actors.get(aid).map(|x| x.coord);
        if let Some(coord) = coord {
            for item_id in items_carried_by(&obj.items, aid) {
                if let Some(item) = obj.items.get_mut(item_id) {
                    item.location = ItemLocation::Floor(coord);
                }
            }
        }
        a.actors.remove(aid);
    }
}

/// Wand-port geography adapter that records every mutation into the compact
/// delta, so the full 80x21 level never rides in a candidate reply.
struct DeltaGeography<'a> {
    level: &'a mut DungeonLevel,
    delta: &'a mut GeographyDelta,
}

impl crate::actions::wand::ports::GeographyFacts for DeltaGeography<'_> {
    fn tile(&self, coord: Coord) -> Tile {
        self.level.get_tile(coord).clone()
    }
    fn trajectory(&self, origin: Coord, dir: Direction, energy: u32) -> Vec<Coord> {
        nethacked_dungeon::trace_beam_path(self.level, origin, dir, energy)
    }
    fn engraving(&self, coord: Coord) -> Option<Engraving> {
        self.level.get_engraving(coord).cloned()
    }
}

impl crate::actions::wand::ports::Geography for DeltaGeography<'_> {
    fn reveal_secret_door(&mut self, coord: Coord) -> bool {
        let tile = self.level.get_tile_mut(coord);
        if matches!(tile, Tile::SecretDoor { .. }) {
            tile.reveal_secret_door();
            let tile = self.level.get_tile(coord).clone();
            self.delta.tiles.push((coord, tile));
            true
        } else {
            false
        }
    }
    fn collapse_drawbridge(&mut self, coord: Coord) {
        self.level.set_tile(coord, Tile::Moat);
        self.delta.tiles.push((coord, Tile::Moat));
    }
    fn break_door(&mut self, coord: Coord) {
        self.level.get_tile_mut(coord).break_door();
        let tile = self.level.get_tile(coord).clone();
        self.delta.tiles.push((coord, tile));
    }
    fn freeze_pool(&mut self, coord: Coord) {
        self.level.set_tile(coord, Tile::Pool { frozen: true });
        self.delta.tiles.push((coord, Tile::Pool { frozen: true }));
    }
    fn erase_engraving(&mut self, coord: Coord) {
        self.level.remove_engraving(coord);
        self.delta.engravings.push((coord, None));
    }
}

fn execute_dip(
    clk: &mut ClockFacts,
    obj: &mut ObjectsFacts,
    h: &HeroFacts,
    sess: &SessionFacts,
    item_index: usize,
    into_water: WaterType,
) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let carried = items_carried_by(&obj.items, h.player_id);
    if item_index < carried.len() {
        let item_id = carried[item_index];
        if let Some(item) = obj.items.get_mut(item_id) {
            let prev_buc = item.buc;
            item.buc = nethacked_core::buc::dip_water(into_water, prev_buc);

            // Handle potion dilution & water transformation
            if item.class == ItemClass::Potion {
                if into_water == WaterType::Plain {
                    if item.name.contains("extra healing") {
                        let old = item.name.clone();
                        item.name = "potion of healing".into();
                        events.push(GameEvent::LogMessage {
                            text: nethacked_i18n::Messages::potion_diluted(
                                &old,
                                &item.name,
                                sess.locale,
                            ),
                        });
                    } else if !item.name.contains("water") {
                        let old = item.name.clone();
                        item.name = "potion of water".into();
                        events.push(GameEvent::LogMessage {
                            text: nethacked_i18n::Messages::potion_diluted(
                                &old,
                                &item.name,
                                sess.locale,
                            ),
                        });
                    }
                } else if into_water == WaterType::Holy && item.name.contains("water") {
                    item.name = "potion of holy water".into();
                } else if into_water == WaterType::Unholy && item.name.contains("water") {
                    item.name = "potion of unholy water".into();
                }
            }

            let status_desc = match item.buc {
                Buc::Blessed => "glows with a pure amber aura (blessed)!",
                Buc::Uncursed => "glows softly and feels purified (uncursed).",
                Buc::Cursed => "emits an ominous black glow (cursed)!",
            };
            events.push(GameEvent::LogMessage {
                text: format!(
                    "You dip the {} into the water. It {}",
                    item.name, status_desc
                ),
            });
            clk.scheduler.hero_act(NORMAL_SPEED);
        }
    } else {
        events.push(GameEvent::LogMessage {
            text: "You don't have that item to dip.".into(),
        });
    }
    events
}

fn execute_quaff(
    clk: &mut ClockFacts,
    obj: &mut ObjectsFacts,
    a: &mut ActorsFacts,
    h: &mut HeroFacts,
    idx: usize,
) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let carried = items_carried_by(&obj.items, h.player_id);
    if idx < carried.len() {
        let item_id = carried[idx];
        let item = obj.items.get(item_id).cloned();
        if let Some(item) = item {
            if item.class == ItemClass::Potion {
                destroy_item(obj, item_id);
                if item.name.contains("healing") {
                    if let Some(p) = a.actors.get_mut(h.player_id) {
                        p.hp = (p.hp + 10).min(p.max_hp);
                    }
                    events.push(GameEvent::LogMessage {
                        text: "You quaff the potion. You feel much better!".into(),
                    });
                } else if item.name.contains("speed") {
                    if let Some(p) = a.actors.get_mut(h.player_id) {
                        p.intrinsics.fast = true;
                    }
                    events.push(GameEvent::LogMessage {
                        text: "You quaff the potion. You are moving much faster!".into(),
                    });
                } else if item.name.contains("polymorph") {
                    // Apply polymorph to self. Save the current (actor)
                    // HP as the base form first: it is restored on
                    // rehumanize (C polymon sets only u.mh, polyself.c:872).
                    if let Some(p) = a.actors.get(h.player_id) {
                        if h.hero.polymorph.is_none() {
                            crate::SimulationWorld::sync_hero_form_from_actor(&mut h.hero, p);
                        }
                    }
                    h.hero.polymorph = Some(nethacked_types::PolymorphForm {
                        monster_id: 1, // Dummy id
                        hp: 20,
                        max_hp: 20,
                        duration: 100,
                    });
                    if let Some(p) = a.actors.get_mut(h.player_id) {
                        p.hp = 20;
                        p.max_hp = 20;
                    }
                    events.push(GameEvent::LogMessage {
                        text: "You feel a change coming over you... you polymorph!".into(),
                    });
                } else if item.name.contains("acid") {
                    nethacked_core::afflictions::cure_petrification(&mut h.hero);
                    events.push(GameEvent::LogMessage {
                        text: "You quaff the potion of acid. It burns, but you feel less stiff!"
                            .into(),
                    });
                } else {
                    events.push(GameEvent::LogMessage {
                        text: format!("You quaff the {}. It tastes like water.", item.name),
                    });
                }
                clk.scheduler.hero_act(NORMAL_SPEED);
            } else {
                events.push(GameEvent::LogMessage {
                    text: "You can only quaff potions!".into(),
                });
            }
        }
    } else {
        events.push(GameEvent::LogMessage {
            text: "You have no such potion to quaff.".into(),
        });
    }
    events
}

#[allow(clippy::too_many_arguments)]
fn execute_read(
    clk: &mut ClockFacts,
    obj: &mut ObjectsFacts,
    a: &mut ActorsFacts,
    h: &mut HeroFacts,
    eq: &mut EquipmentFacts,
    geo: &GeographyFacts,
    sess: &SessionFacts,
    config: &OwnerBridgeConfig,
    rng: &mut rand_chacha::ChaCha8Rng,
    idx: usize,
) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let carried = items_carried_by(&obj.items, h.player_id);
    if idx < carried.len() {
        let item_id = carried[idx];
        let item = obj.items.get(item_id).cloned();
        if let Some(item) = item {
            if item.class == ItemClass::Scroll {
                nethacked_core::conducts::record_read(&mut h.conducts);
                destroy_item(obj, item_id);
                if item.name.contains("teleport") {
                    if geo.level.rooms.len() > 1 {
                        let room_idx = (rng.next_u32() as usize) % geo.level.rooms.len();
                        let new_c = geo.level.rooms[room_idx].center();
                        if let Some(p) = a.actors.get_mut(h.player_id) {
                            p.coord = new_c;
                        }
                        events.push(GameEvent::LogMessage { text: "You read the scroll of teleportation and vanish in a flash of light!".into() });
                    }
                } else if item.name.contains("curse") {
                    for id in items_carried_by(&obj.items, h.player_id) {
                        if let Some(it) = obj.items.get_mut(id) {
                            it.buc = nethacked_core::buc::uncurse(it.buc);
                        }
                    }
                    events.push(GameEvent::LogMessage { text: "You feel as though someone is helping you. Your possessions are uncursed!".into() });
                } else if item.name.contains("enchant weapon") {
                    let wielded = eq
                        .wielded_item
                        .and_then(|id| obj.items.get(id).map(|w| (id, w.enchantment)));
                    if let Some((wielded_id, spe)) = wielded {
                        // C draws the amount (read.c:1667) before chwepon's
                        // evaporation roll (wield.c:999-1000).
                        let gain_roll =
                            draw_enchant(rng, nethacked_core::weapon_gain_draw(spe, item.buc));
                        let evaporate_roll = draw_enchant(
                            rng,
                            nethacked_core::weapon_evaporation_draw(spe, item.buc),
                        );
                        let outcome = nethacked_core::enchant_weapon(
                            spe,
                            item.buc,
                            evaporate_roll,
                            gain_roll,
                        );
                        apply_enchant_outcome(obj, eq, wielded_id, outcome, false, &mut events);
                    } else {
                        events.push(GameEvent::LogMessage {
                            text: "Your hands itch for a moment.".into(),
                        });
                    }
                } else if item.name.contains("enchant armor") {
                    // Carried armor stands in for C `some_armor` (no worn slots).
                    let armor = items_carried_by(&obj.items, h.player_id)
                        .into_iter()
                        .find_map(|id| {
                            obj.items
                                .get(id)
                                .filter(|it| it.class == ItemClass::Armor)
                                .map(|it| (id, it.enchantment, it.name.clone()))
                        });
                    if let Some((aid, spe, name)) = armor {
                        // Item records carry no object kind: elven / oc_magic are
                        // classified by name; the cornuthaum is special for Wizards.
                        let special = nethacked_core::armor_is_elven(&name)
                            || (sess.role_name == "Wizard" && name == "cornuthaum");
                        let magical = nethacked_core::armor_is_magical(&name);
                        let evaporate_roll = draw_enchant(
                            rng,
                            nethacked_core::armor_evaporation_draw(spe, item.buc, special),
                        );
                        // C draws the gain only when the armor survives (read.c:1179).
                        let gain_roll = if nethacked_core::armor_evaporates(
                            spe,
                            item.buc,
                            special,
                            evaporate_roll,
                        ) {
                            0
                        } else {
                            draw_enchant(
                                rng,
                                nethacked_core::armor_gain_draw(spe, item.buc, special, magical),
                            )
                        };
                        let outcome = nethacked_core::enchant_armor(
                            spe,
                            item.buc,
                            special,
                            magical,
                            evaporate_roll,
                            gain_roll,
                        );
                        if let nethacked_core::EnchantOutcome::Changed(_) = outcome {
                            // read.c:1115 seffect_enchant_armor: the armor's BUC
                            // follows the scroll (curse / bless / uncurse).
                            if let Some(armor) = obj.items.get_mut(aid) {
                                armor.buc = match (item.buc, armor.buc) {
                                    (Buc::Cursed, _) => Buc::Cursed,
                                    (Buc::Blessed, _) => Buc::Blessed,
                                    (Buc::Uncursed, Buc::Cursed) => Buc::Uncursed,
                                    (Buc::Uncursed, b) => b,
                                };
                            }
                        }
                        apply_enchant_outcome(obj, eq, aid, outcome, true, &mut events);
                    } else {
                        events.push(GameEvent::LogMessage {
                            text: "Your skin feels warm for a moment.".into(),
                        });
                    }
                } else if item.name.contains("charging") {
                    let wand_id =
                        items_carried_by(&obj.items, h.player_id)
                            .into_iter()
                            .find(|&id| {
                                obj.items
                                    .get(id)
                                    .map(|it| it.class == ItemClass::Wand)
                                    .unwrap_or(false)
                            });
                    if let Some(wid) = wand_id {
                        let (wand_name, charges, recharges, wand_blessed) = {
                            let w = obj.items.get(wid).unwrap();
                            (
                                w.name.clone(),
                                w.enchantment.max(0) as u32,
                                w.recharged as u32,
                                w.buc == Buc::Blessed,
                            )
                        };
                        // read.c:737: lim = 1 wishing, 8 directional, 15 non-directional.
                        let is_wishing = wand_name.contains("wishing");
                        // Catalog `wand_dir` (objects.h oc_dir); wands outside the catalog
                        // fall back to the NODIR name list.
                        let nodir = match config.ruleset.item(&wand_name) {
                            Some(a) => a.wand_dir == Some(nethacked_data::WandDir::NoDir),
                            None => [
                                "light",
                                "secret door detection",
                                "create monster",
                                "enlightenment",
                            ]
                            .iter()
                            .any(|k| wand_name.contains(k)),
                        };
                        let lim: u32 = if is_wishing {
                            1
                        } else if nodir {
                            15
                        } else {
                            8
                        };
                        // read.c:741: rn2(343) is drawn only when n > 0 && !wishing
                        // (C short-circuits); then rn1(5, lim-4) and, uncursed, rnd(n).
                        let roll_343 =
                            if nethacked_core::artifacts_wands::recharge_needs_explosion_roll(
                                recharges, is_wishing,
                            ) {
                                rng.random_range(0..343u32)
                            } else {
                                0
                            };
                        let rn5 = if lim > 1 && item.buc != Buc::Cursed {
                            rng.random_range(0..5u32)
                        } else {
                            0
                        };
                        let rnd_roll = if lim > 1 && item.buc == Buc::Uncursed {
                            rng.random_range(1..=(lim - 4 + rn5))
                        } else {
                            1
                        };
                        let wand_state = nethacked_types::WandCharges { charges, recharges };
                        match nethacked_core::artifacts_wands::recharge_wand(
                            wand_state,
                            item.buc,
                            lim,
                            is_wishing,
                            wand_blessed,
                            roll_343,
                            rn5,
                            rnd_roll,
                        ) {
                            nethacked_types::RechargeResult::Exploded => {
                                destroy_item(obj, wid);
                                // read.c:763 wand_explode(obj, rnd(lim)); :2420-2445 dmg = d(n, k)
                                // with n = max(2, spe + chg).
                                let chg = rng.random_range(1..=lim);
                                let dice = nethacked_core::artifacts_wands::wand_explode_dice(
                                    charges as i32,
                                    chg,
                                );
                                let k = nethacked_core::artifacts_wands::wand_explode_die_size(
                                    &wand_name,
                                );
                                let dmg: i32 =
                                    (0..dice).map(|_| rng.random_range(1..=k) as i32).sum();
                                if let Some(p) = a.actors.get_mut(h.player_id) {
                                    p.hp = p.hp.saturating_sub(dmg.max(0) as u32);
                                    if p.hp == 0 {
                                        p.is_dead = true;
                                    }
                                }
                                events.push(GameEvent::LogMessage {
                                    text: nethacked_i18n::Messages::wand_exploded(
                                        &wand_name,
                                        sess.locale,
                                    ),
                                });
                            }
                            nethacked_types::RechargeResult::Success(new_w) => {
                                if let Some(w_mut) = obj.items.get_mut(wid) {
                                    let charges_now = new_w.charges;
                                    w_mut.enchantment = charges_now.min(i8::MAX as u32) as i8;
                                    w_mut.recharged = new_w.recharges.min(u8::MAX as u32) as u8;
                                    events.push(GameEvent::LogMessage {
                                        text: nethacked_i18n::Messages::wand_recharged(
                                            &wand_name,
                                            charges_now,
                                            new_w.recharges,
                                            sess.locale,
                                        ),
                                    });
                                }
                            }
                        }
                    } else {
                        events.push(GameEvent::LogMessage {
                            text: "You have no wands to recharge.".into(),
                        });
                    }
                } else if item.name.contains("genocide") {
                    h.conducts.genocideless = false;
                    match item.buc {
                        Buc::Cursed => {
                            let summon_count =
                                nethacked_core::genocide::cursed_genocide_summon_count();
                            let player_c = a
                                .actors
                                .get(h.player_id)
                                .map(|p| p.coord)
                                .unwrap_or(Coord::new_unchecked(1, 1));
                            for _ in 0..summon_count {
                                let spawn_c = player_c
                                    .neighbors()
                                    .into_iter()
                                    .find(|&c| {
                                        geo.level.is_passable(c) && actor_at(&a.actors, c).is_none()
                                    })
                                    .unwrap_or(player_c);
                                if let Some(mut mon) =
                                    config.ruleset.create_monster_record_by_id_with_context(
                                        nethacked_data::MonsterSpeciesId::GOBLIN,
                                        spawn_c,
                                        monster_spawn_context(geo.branch),
                                        rng,
                                    )
                                {
                                    mon.name = "hostile goblin".into();
                                    mon.is_peaceful = false;
                                    if let Some(def) = config
                                        .ruleset
                                        .monster_by_id(nethacked_data::MonsterSpeciesId::GOBLIN)
                                    {
                                        set_monster_malign(
                                            &a.actors,
                                            h.player_id,
                                            &sess.role_name,
                                            &mut mon,
                                            def,
                                        );
                                    }
                                    spawn_actor(&mut a.actors, mon);
                                }
                            }
                            events.push(GameEvent::LogMessage {
                                text: "You read the cursed scroll of genocide. Monsters appear!"
                                    .into(),
                            });
                        }
                        Buc::Uncursed => {
                            let target =
                                nethacked_types::GenocideTarget::Species("goblin".to_string());
                            nethacked_core::genocide::apply_genocide(
                                &mut h.genocide_registry,
                                target.clone(),
                            );
                            // Wipe from current floor
                            genocide_wipe(obj, a, h, &config.ruleset);
                            events.push(GameEvent::LogMessage {
                                text: "You read the scroll of genocide. A species is wiped out!"
                                    .into(),
                            });
                        }
                        Buc::Blessed => {
                            let target = nethacked_types::GenocideTarget::Class('L'); // Lich class for example
                            nethacked_core::genocide::apply_genocide(
                                &mut h.genocide_registry,
                                target.clone(),
                            );
                            // Wipe from current floor
                            genocide_wipe(obj, a, h, &config.ruleset);
                            events.push(GameEvent::LogMessage { text: "You read the blessed scroll of genocide. A whole class of monsters is wiped out!".into() });
                        }
                    }
                } else {
                    events.push(GameEvent::LogMessage {
                        text: format!("You read the {}. Knowledge fills your mind!", item.name),
                    });
                }
                clk.scheduler.hero_act(NORMAL_SPEED);
            } else if item.class == ItemClass::Spellbook {
                nethacked_core::conducts::record_read(&mut h.conducts);
                if item.name.contains("Book of the Dead") {
                    let player_coord = a
                        .actors
                        .get(h.player_id)
                        .map(|p| p.coord)
                        .unwrap_or(Coord::new_unchecked(0, 0));
                    let on_vs = geo.vibrating_square == Some(player_coord);
                    h.ritual_progress = nethacked_core::step_ritual(
                        h.ritual_progress,
                        nethacked_core::InvocationStep::ReadBook,
                        on_vs,
                        &h.candelabrum_state,
                    );
                    if nethacked_core::is_sanctum_accessible(h.ritual_progress) {
                        events.push(GameEvent::LogMessage {
                            text: "The cavern trembles violently! A subterranean portal to Moloch's Sanctum opens before you!".into(),
                        });
                    } else if !on_vs {
                        events.push(GameEvent::LogMessage {
                            text: "You recite the eldritch litany of the Book of the Dead, but nothing happens. You are not on the Vibrating Square!".into(),
                        });
                    } else {
                        events.push(GameEvent::LogMessage {
                            text: "You read from the Book of the Dead, but the ritual sequence is incomplete.".into(),
                        });
                    }
                } else {
                    let spell = if item.name.contains("force bolt") {
                        SpellKind::ForceBolt
                    } else {
                        SpellKind::CureLightWounds
                    };
                    if !h.known_spells.iter().any(|(s, _)| *s == spell) {
                        h.known_spells.push((spell, 20000));
                    }
                    events.push(GameEvent::LogMessage {
                        text: format!("You study the {} and memorize the spell!", item.name),
                    });
                }
                clk.scheduler.hero_act(NORMAL_SPEED);
            } else {
                events.push(GameEvent::LogMessage {
                    text: "You can only read scrolls or spellbooks!".into(),
                });
            }
        }
    } else {
        events.push(GameEvent::LogMessage {
            text: "You have no such scroll or book to read.".into(),
        });
    }
    events
}

fn execute_eat(
    clk: &mut ClockFacts,
    obj: &mut ObjectsFacts,
    a: &mut ActorsFacts,
    h: &mut HeroFacts,
    config: &OwnerBridgeConfig,
    idx: usize,
) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let carried = items_carried_by(&obj.items, h.player_id);
    if idx < carried.len() {
        let item_id = carried[idx];
        let item = obj.items.get(item_id).cloned();
        if let Some(item) = item {
            if item.class == ItemClass::Food {
                destroy_item(obj, item_id);
                // Catalog `oc_nutrition` (objects.h FOOD); corpses (catalog 0, C takes it
                // from the monster) and uncatalogued food keep the flat 400.
                let nut_gain = match config.ruleset.item(&item.name) {
                    Some(a) if a.nutrition > 0 => a.nutrition as i32,
                    _ => {
                        if item.name.contains("ration") {
                            800
                        } else if item.name.contains("apple") {
                            50
                        } else {
                            400 // corpse
                        }
                    }
                };
                h.player_nutrition = (h.player_nutrition + nut_gain).min(2000);

                let is_meat = config
                    .ruleset
                    .item(&item.name)
                    .map(|i| i.is_meat())
                    .unwrap_or_else(|| item.name.contains("corpse"));
                if is_meat {
                    nethacked_core::conducts::record_eat_meat(&mut h.conducts);
                    let corpse_race = item.corpse_race.as_deref().unwrap_or("unknown");
                    if nethacked_core::nutrition::is_cannibalism(corpse_race, "human") {
                        events.push(GameEvent::LogMessage {
                            text: "You cannibal! You feel deeply ashamed.".into(),
                        });
                    }
                    if nethacked_core::nutrition::is_corpse_tainted(
                        item.corpse_age,
                        item.rot_threshold,
                    ) {
                        events.push(GameEvent::LogMessage {
                            text: "Ugh, this corpse is tainted!".into(),
                        });
                    }

                    let monster_name = item.name.replace(" corpse", "");
                    if let Some(intrinsic) =
                        nethacked_core::nutrition::intrinsic_from_corpse(&monster_name)
                    {
                        if let Some(p) = a.actors.get_mut(h.player_id) {
                            match intrinsic.as_str() {
                                "fire_resistance" => p.intrinsics.fire_resistance = true,
                                "cold_resistance" => p.intrinsics.cold_resistance = true,
                                "shock_resistance" => p.intrinsics.shock_resistance = true,
                                "poison_resistance" => p.intrinsics.poison_resistance = true,
                                "sleep_resistance" => p.intrinsics.sleep_resistance = true,
                                "telepathy" => p.intrinsics.telepathy = true,
                                "see_invisible" => p.intrinsics.see_invisible = true,
                                _ => {}
                            }
                        }
                        events.push(GameEvent::LogMessage {
                            text: format!("You gained {intrinsic}!"),
                        });
                    }

                    if item.name.contains("lizard") {
                        nethacked_core::afflictions::cure_petrification(&mut h.hero);
                        events.push(GameEvent::LogMessage {
                            text: "You eat the lizard corpse. You feel limber!".into(),
                        });
                    } else if item.name.contains("ant")
                        || item.name.contains("kobold")
                        || item.name.contains("orc")
                    {
                        if let Some(p) = a.actors.get_mut(h.player_id) {
                            p.intrinsics.poison_resistance = true;
                        }
                        events.push(GameEvent::LogMessage {
                            text: "You feel a healthy warmth suffuse your body! You gained Poison Resistance!".into(),
                        });
                    }
                }
                events.push(GameEvent::LogMessage {
                    text: format!("You eat the {}. Delicious!", item.name),
                });
                clk.scheduler.hero_act(NORMAL_SPEED);
            } else {
                events.push(GameEvent::LogMessage {
                    text: "That is not edible!".into(),
                });
            }
        }
    } else {
        events.push(GameEvent::LogMessage {
            text: "You have nothing to eat in that slot.".into(),
        });
    }
    events
}

#[allow(clippy::too_many_arguments)]
fn execute_rub(
    clk: &mut ClockFacts,
    obj: &mut ObjectsFacts,
    a: &mut ActorsFacts,
    h: &HeroFacts,
    geo: &GeographyFacts,
    sess: &SessionFacts,
    config: &OwnerBridgeConfig,
    rng: &mut rand_chacha::ChaCha8Rng,
    item_index: usize,
) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let carried = items_carried_by(&obj.items, h.player_id);
    if item_index >= carried.len() {
        events.push(GameEvent::LogMessage {
            text: "You don't have that item to rub.".into(),
        });
        return events;
    }

    let item_id = carried[item_index];
    let item = obj.items.get(item_id).cloned();
    if let Some(item) = item {
        let is_magic = item.name.contains("magic lamp");
        let is_oil = item.name.contains("oil lamp");

        if is_magic {
            let (res, consumed) = nethacked_core::rub_lamp(true, true, item.buc, 1000);
            if consumed {
                if let Some(it_mut) = obj.items.get_mut(item_id) {
                    it_mut.name = "oil lamp".into();
                }
            }

            match res {
                nethacked_core::RubResult::WishGranted => {
                    events.push(GameEvent::LogMessage {
                        text: nethacked_i18n::Messages::djinni_wishing(sess.locale).into(),
                    });
                    // Grant an immediate boon / blessed scroll of identify
                    let player_c = a
                        .actors
                        .get(h.player_id)
                        .map(|p| p.coord)
                        .unwrap_or(Coord::new_unchecked(1, 1));
                    if let Some(gift) = config.ruleset.create_item_record_by_id(
                        nethacked_data::ItemKindId::SCR_IDENTIFY,
                        ItemLocation::Floor(player_c),
                        Buc::Blessed,
                    ) {
                        spawn_item(obj, gift);
                    }
                }
                nethacked_core::RubResult::PeacefulDjinni => {
                    events.push(GameEvent::LogMessage {
                        text: nethacked_i18n::Messages::djinni_peaceful(sess.locale).into(),
                    });
                }
                nethacked_core::RubResult::HostileDjinni => {
                    events.push(GameEvent::LogMessage {
                        text: nethacked_i18n::Messages::djinni_hostile(sess.locale).into(),
                    });
                    let player_c = a
                        .actors
                        .get(h.player_id)
                        .map(|p| p.coord)
                        .unwrap_or(Coord::new_unchecked(1, 1));
                    let spawn_c = player_c
                        .neighbors()
                        .into_iter()
                        .find(|&c| geo.level.is_passable(c) && actor_at(&a.actors, c).is_none())
                        .unwrap_or(player_c);
                    if let Some(mut mon) = config.ruleset.create_monster_record_by_id_with_context(
                        nethacked_data::MonsterSpeciesId::DJINNI,
                        spawn_c,
                        monster_spawn_context(geo.branch),
                        rng,
                    ) {
                        mon.name = "hostile djinni".into();
                        mon.is_peaceful = false;
                        if let Some(def) = config
                            .ruleset
                            .monster_by_id(nethacked_data::MonsterSpeciesId::DJINNI)
                        {
                            set_monster_malign(
                                &a.actors,
                                h.player_id,
                                &sess.role_name,
                                &mut mon,
                                def,
                            );
                        }
                        spawn_actor(&mut a.actors, mon);
                    }
                }
                _ => {
                    events.push(GameEvent::LogMessage {
                        text: nethacked_i18n::Messages::lamp_smoke(sess.locale).into(),
                    });
                }
            }
            clk.scheduler.hero_act(NORMAL_SPEED);
        } else if is_oil {
            events.push(GameEvent::LogMessage {
                text: nethacked_i18n::Messages::lamp_smoke(sess.locale).into(),
            });
            clk.scheduler.hero_act(NORMAL_SPEED);
        } else {
            events.push(GameEvent::LogMessage {
                text: format!("Rubbing the {} doesn't seem to do anything.", item.name),
            });
        }
    }

    events
}

fn execute_apply(
    clk: &mut ClockFacts,
    obj: &mut ObjectsFacts,
    a: &ActorsFacts,
    h: &mut HeroFacts,
    geo: &GeographyFacts,
    item_index: usize,
) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let carried = items_carried_by(&obj.items, h.player_id);
    if item_index < carried.len() {
        let item_id = carried[item_index];
        let item_name = if let Some(it) = obj.items.get(item_id) {
            it.name.clone()
        } else {
            return events;
        };

        let name = item_name.to_lowercase();
        let player_coord = a
            .actors
            .get(h.player_id)
            .map(|p| p.coord)
            .unwrap_or(Coord::new_unchecked(0, 0));
        let on_vs = geo.vibrating_square == Some(player_coord);

        if name.contains("bell of opening") {
            h.ritual_progress = nethacked_core::step_ritual(
                h.ritual_progress,
                nethacked_core::InvocationStep::RingBell,
                on_vs,
                &h.candelabrum_state,
            );
            events.push(GameEvent::LogMessage {
                text: "You ring the Bell of Opening. It produces an otherworldly, reverberating silver chime.".into(),
            });
            clk.scheduler.hero_act(NORMAL_SPEED);
        } else if name.contains("candelabrum") {
            if h.candelabrum_state.light() {
                h.ritual_progress = nethacked_core::step_ritual(
                    h.ritual_progress,
                    nethacked_core::InvocationStep::LightCandelabrum,
                    on_vs,
                    &h.candelabrum_state,
                );
                events.push(GameEvent::LogMessage {
                    text: "The seven candles on the Candelabrum of Invocation blaze with holy incandescent flame!".into(),
                });
            } else {
                events.push(GameEvent::LogMessage {
                    text: format!("The Candelabrum of Invocation is not ready. It needs 7 candles (currently has {}).", h.candelabrum_state.candle_count),
                });
            }
            clk.scheduler.hero_act(NORMAL_SPEED);
        } else if name.contains("candle") {
            let has_candelabrum = carried.iter().any(|&cid| {
                obj.items
                    .get(cid)
                    .map(|it| it.name.contains("Candelabrum"))
                    .unwrap_or(false)
            });
            if has_candelabrum && h.candelabrum_state.attach_candle() {
                destroy_item(obj, item_id);
                events.push(GameEvent::LogMessage {
                    text: format!("You attach the candle to the Candelabrum of Invocation. ({}/7 candles attached)", h.candelabrum_state.candle_count),
                });
                clk.scheduler.hero_act(NORMAL_SPEED);
            } else {
                // Regular candle lighting toggle
                if let Some(item) = obj.items.get_mut(item_id) {
                    if item.enchantment <= 0 {
                        item.enchantment = 1;
                        events.push(GameEvent::LogMessage {
                            text: format!(
                                "You light the {}. It casts a bright illumination.",
                                item.name
                            ),
                        });
                    } else {
                        item.enchantment = 0;
                        events.push(GameEvent::LogMessage {
                            text: format!("You extinguish the {}.", item.name),
                        });
                    }
                }
                clk.scheduler.hero_act(NORMAL_SPEED);
            }
        } else if name.contains("lamp") || name.contains("lantern") {
            if let Some(item) = obj.items.get_mut(item_id) {
                if item.enchantment <= 0 {
                    item.enchantment = 1;
                    events.push(GameEvent::LogMessage {
                        text: format!(
                            "You light the {}. It casts a bright illumination.",
                            item.name
                        ),
                    });
                } else {
                    item.enchantment = 0;
                    events.push(GameEvent::LogMessage {
                        text: format!("You extinguish the {}.", item.name),
                    });
                }
            }
            clk.scheduler.hero_act(NORMAL_SPEED);
        } else {
            events.push(GameEvent::LogMessage {
                text: format!("You don't know how to apply the {item_name}."),
            });
        }
    } else {
        events.push(GameEvent::LogMessage {
            text: "You don't have that item in your pack.".into(),
        });
    }
    events
}

fn execute_engrave(
    clk: &mut ClockFacts,
    a: &ActorsFacts,
    h: &HeroFacts,
    geo: &mut GeographyFacts,
    geo_delta: &mut GeographyDelta,
    text: String,
    medium: EngravingMedium,
) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let Some(player) = a.actors.get(h.player_id).cloned() else {
        return events;
    };

    let p_coord = player.coord;
    let engraving = Engraving::new(text.clone(), medium);
    geo.level.set_engraving(p_coord, engraving.clone());
    geo_delta.engravings.push((p_coord, Some(engraving)));
    events.push(GameEvent::EngravingAdded {
        coord: p_coord,
        text: text.clone(),
    });
    let medium_desc = match medium {
        EngravingMedium::Dust(_) => "in the dust",
        EngravingMedium::Burned => "with searing fire",
        EngravingMedium::Carved(_) => "into the stone",
        EngravingMedium::Marked(_) => "with marking ink",
    };
    events.push(GameEvent::LogMessage {
        text: format!("You write \"{text}\" {medium_desc} on the floor."),
    });
    clk.scheduler.hero_act(NORMAL_SPEED);

    events
}

/// Stored-level item slices paired with their routing keys (wand_legacy.rs
/// `CachedItems`).
type CachedItems<'a> = ((BranchId, usize), &'a [(ItemId, ItemRecord)]);
/// Stored-level item/unpaid slice pairs (item_lifetime_legacy.rs
/// `CachedBilling`).
type CachedBilling<'a> = (&'a [(ItemId, ItemRecord)], &'a [(ItemId, u32)]);

#[allow(clippy::too_many_arguments)]
fn execute_zap(
    clk: &mut ClockFacts,
    obj: &mut ObjectsFacts,
    a: &mut ActorsFacts,
    h: &mut HeroFacts,
    eq: &mut EquipmentFacts,
    bill: &mut BillingFacts,
    geo: &mut GeographyFacts,
    geo_delta: &mut GeographyDelta,
    sess: &SessionFacts,
    config: &OwnerBridgeConfig,
    rng: &mut rand_chacha::ChaCha8Rng,
    selection: crate::actions::wand::Selection,
    dir: Direction,
    energy: u32,
) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let cached_objects: Vec<CachedItems<'_>> = geo
        .stored_keys
        .iter()
        .zip(obj.stored_items.iter())
        .map(|(key, items)| (*key, items.as_slice()))
        .collect();
    let cached_billing: Vec<CachedBilling<'_>> = obj
        .stored_items
        .iter()
        .zip(bill.stored_unpaid_items.iter())
        .map(|(items, unpaid)| (items.as_slice(), unpaid.as_slice()))
        .collect();
    let mut geography = DeltaGeography {
        level: &mut geo.level,
        delta: geo_delta,
    };
    let outcome = crate::actions::wand_legacy::zap_working(
        h.player_id,
        selection,
        dir,
        energy,
        &mut obj.items,
        &mut obj.artifact_history,
        cached_objects,
        &mut a.actors,
        &h.genocide_registry,
        &mut geography,
        &mut clk.scheduler,
        &mut clk.object_timers,
        h.hero.polymorph.is_some(),
        sess.role_id,
        &sess.role_name,
        h.hero_race,
        &mut h.alignment_record,
        &mut h.conducts,
        &mut h.quest_state,
        &mut h.divine_favor,
        &mut h.player_luck,
        &mut h.divine_protection,
        &mut h.progress,
        &mut h.player_pw,
        &mut h.player_max_pw,
        rng,
        &config.ruleset,
        sess.locale,
        geo.branch,
        geo.depth,
    );
    events.extend(outcome.events);
    match crate::actions::item_lifetime_legacy::finalize_working(
        h.player_id,
        &config.ruleset,
        &mut obj.items,
        &mut eq.equipment,
        &mut eq.wielded_item,
        &mut h.hero.quivered_item,
        &mut clk.object_timers,
        &mut bill.billing,
        &mut bill.unpaid_items,
        cached_billing,
        outcome.survivor,
    ) {
        Ok(Some(_)) => events.push(GameEvent::LogMessage {
            text: "Your wand turns to dust.".into(),
        }),
        Ok(None) => {}
        Err(_) => events.push(GameEvent::LogMessage {
            text: "The wand cannot be retired: its accounting is inconsistent.".into(),
        }),
    }
    events
}

pub(crate) fn outcome(
    action: &ItemEffectAction,
    obs: &[Observation],
    config: &OwnerBridgeConfig,
    rng: &mut rand_chacha::ChaCha8Rng,
) -> owners::Result<ItemEffectsOutcome> {
    let mut clk: ClockFacts = fact(obs, 0)?;
    let mut obj: ObjectsFacts = fact(obs, 1)?;
    let mut a: ActorsFacts = fact(obs, 2)?;
    let mut h: HeroFacts = fact(obs, 3)?;
    let mut eq: EquipmentFacts = fact(obs, 4)?;
    let mut bill: BillingFacts = fact(obs, 5)?;
    let mut geo: GeographyFacts = fact(obs, 7)?;
    let sess: SessionFacts = fact(obs, 8)?;

    let Some(mut hero_actor) = a.actors.get(h.player_id).cloned() else {
        return Err(owners::Error::Unsupported);
    };
    if hero_actor.is_dead || hero_actor.hp == 0 {
        return Ok(ItemEffectsOutcome::empty());
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

    // 2. Execute the item-effect action
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
            ItemEffectAction::Dip {
                item_index,
                into_water,
            } => {
                let dip_events =
                    execute_dip(&mut clk, &mut obj, &h, &sess, *item_index, *into_water);
                events.extend(dip_events);
                clk.scheduler.hero_energy != energy_before
            }
            ItemEffectAction::Quaff(idx) => {
                let quaff_events = execute_quaff(&mut clk, &mut obj, &mut a, &mut h, *idx);
                events.extend(quaff_events);
                clk.scheduler.hero_energy != energy_before
            }
            ItemEffectAction::Read(idx) => {
                let read_events = execute_read(
                    &mut clk, &mut obj, &mut a, &mut h, &mut eq, &geo, &sess, config, rng, *idx,
                );
                events.extend(read_events);
                clk.scheduler.hero_energy != energy_before
            }
            ItemEffectAction::Eat(idx) => {
                let eat_events = execute_eat(&mut clk, &mut obj, &mut a, &mut h, config, *idx);
                events.extend(eat_events);
                clk.scheduler.hero_energy != energy_before
            }
            ItemEffectAction::Rub(idx) => {
                let rub_events = execute_rub(
                    &mut clk, &mut obj, &mut a, &h, &geo, &sess, config, rng, *idx,
                );
                events.extend(rub_events);
                clk.scheduler.hero_energy != energy_before
            }
            ItemEffectAction::Apply(idx) => {
                let apply_events = execute_apply(&mut clk, &mut obj, &a, &mut h, &geo, *idx);
                events.extend(apply_events);
                clk.scheduler.hero_energy != energy_before
            }
            ItemEffectAction::Engrave { text, medium } => {
                let engrave_events = execute_engrave(
                    &mut clk,
                    &a,
                    &h,
                    &mut geo,
                    &mut geo_delta,
                    text.clone(),
                    *medium,
                );
                events.extend(engrave_events);
                clk.scheduler.hero_energy != energy_before
            }
            ItemEffectAction::ZapWand { dir, energy } => {
                let zap_events = execute_zap(
                    &mut clk,
                    &mut obj,
                    &mut a,
                    &mut h,
                    &mut eq,
                    &mut bill,
                    &mut geo,
                    &mut geo_delta,
                    &sess,
                    config,
                    rng,
                    crate::actions::wand::Selection::First,
                    *dir,
                    *energy,
                );
                events.extend(zap_events);
                clk.scheduler.hero_energy != energy_before
            }
            ItemEffectAction::ZapSelectedWand {
                item_index,
                expected_item,
                dir,
                energy,
            } => {
                let zap_events = execute_zap(
                    &mut clk,
                    &mut obj,
                    &mut a,
                    &mut h,
                    &mut eq,
                    &mut bill,
                    &mut geo,
                    &mut geo_delta,
                    &sess,
                    config,
                    rng,
                    crate::actions::wand::Selection::Selected {
                        index: *item_index,
                        expected: *expected_item,
                    },
                    *dir,
                    *energy,
                );
                events.extend(zap_events);
                clk.scheduler.hero_energy != energy_before
            }
        }
    };

    // The action mutates the player actor in place (quaff/eat intrinsics,
    // polymorph hp, wand level gains, teleport coord); fold those into the
    // loop's working copy so monster ticks and the final write-back observe
    // the post-action hero.
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

    // 4. Turn-stepping loop. Item-effect actions never walk, so the legacy
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

    Ok(ItemEffectsOutcome {
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
            divine_protection: h.divine_protection,
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

struct ItemEffectsCodec<T>(OwnerBridgeConfig, std::marker::PhantomData<T>);
impl<T: Domain> Schema<T> for ItemEffectsCodec<T> {
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

impl<T: Domain> NativeBehavior<T> for ItemEffectsCodec<T> {
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
    result: &ItemEffectsOutcome,
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

fn preview(obs: &[Observation]) -> owners::Result<ItemEffectsOutcome> {
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
                divine_protection: self.divine_protection,
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
            next.divine_protection = h.divine_protection;
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
        let action: ItemEffectAction = decode(request, PAYLOAD)?;
        encode(&outcome(&action, obs, c, &mut self.rng.clone())?, PAYLOAD)
    }
    fn prepare(
        &self,
        request: &[u8],
        obs: &[Observation],
        c: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        let action: ItemEffectAction = decode(request, PAYLOAD)?;
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

pub(super) struct ItemEffectsAccounting {
    pub(super) config: OwnerBridgeConfig,
    pub(super) action: ItemEffectAction,
}

impl Accounting for ItemEffectsAccounting {
    fn validate(&self, p: &[ProposalView], obs: &[Observation]) -> owners::Result<()> {
        if obs.len() != 9 {
            return Err(owners::Error::Schema);
        }
        let random = p
            .iter()
            .find(|x| x.receipt.command == WRITES[7])
            .ok_or(owners::Error::Invariant)?;
        let before: RandomState = decode(&random.before, PAYLOAD)?;
        let result: ItemEffectsOutcome = decode(&random.reply, PAYLOAD)?;
        let mut expected = before.clone();
        let expected_outcome = outcome(&self.action, obs, &self.config, &mut expected.rng)?;
        if encode(&expected, PAYLOAD)? != random.proposed
            || encode(&expected_outcome, PAYLOAD)? != random.reply
            || encode(&expected_outcome, PAYLOAD)?
                != fact::<ItemEffectsOutcome>(obs, 6).and_then(|r| encode(&r, PAYLOAD))?
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
                "itemfx_clock_prepare" => check!(ClockState),
                "itemfx_objects_prepare" => check!(ObjectsState),
                "itemfx_actors_prepare" => check!(ActorsState),
                "itemfx_hero_prepare" => check!(HeroState),
                "itemfx_equipment_prepare" => check!(EquipmentState),
                "itemfx_billing_prepare" => check!(BillingState),
                "itemfx_geography_prepare" => check!(GeographyState),
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
    artifact("native-item-effects-accounting-i7", 72)
}

pub(super) fn capture(
    a: &mut op::AccountingCaptureCatalog,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if c.item_effects {
        a.register(
            identity(),
            POLICY,
            Rc::new(ItemEffectsAccounting {
                config: c.clone(),
                action: ItemEffectAction::Quaff(0),
            }),
        )?;
    }
    Ok(())
}

pub(super) fn restore(
    a: &mut op::AccountingRestoreCatalog,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if c.item_effects {
        let cc = c.clone();
        a.allow(identity(), POLICY, move || {
            Ok(Rc::new(ItemEffectsAccounting {
                config: cc.clone(),
                action: ItemEffectAction::Quaff(0),
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

pub(super) struct ItemEffectsTransport {
    kernel: Kernel,
    caller: ModuleHandle,
    routes: Rc<Routes>,
    resources: Vec<Resource>,
}

impl ItemEffectsTransport {
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
            let action: ItemEffectAction = decode(bytes, PAYLOAD).map_err(native_refusal)?;
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
            encode(&ItemEffectsRequest { action, guards }, PAYLOAD).map_err(native_refusal)
        });

        let run_port: NativePort = Rc::new(move |bytes, view| {
            let req: ItemEffectsRequest = decode(bytes, PAYLOAD).map_err(native_refusal)?;
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
                    Rc::new(ItemEffectsAccounting {
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
                &ItemEffectsReply {
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
            kernel.register_native(manifest(HOME, exports.clone(), vec![]), &[70; 32], ports)?;
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
            &[71; 32],
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
                        schema: [index as u8 + 200; 32],
                        ports,
                    },
                    $state,
                    Backend::Native(Rc::new(ItemEffectsCodec::<$ty>(
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
        action: &ItemEffectAction,
    ) -> Result<ItemEffectsRequest, OwnerBridgeError> {
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
        request: ItemEffectsRequest,
    ) -> Result<(ItemEffectsReply, NativePartitions), OwnerBridgeError> {
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
        let outcome: ItemEffectsReply = decode(&reply?, PAYLOAD)?;

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
    fn item_effects_session_witnesses(&self) -> Result<Vec<wand::OwnerWitness>, OwnerBridgeError> {
        self.session.with_runtime(&self.credential.borrow(), |b| {
            Ok(bridge_stamps(b)?
                .iter()
                .map(wand::OwnerWitness::from)
                .collect())
        })?
    }

    pub fn prepare_item_effect(
        &self,
        action: ItemEffectAction,
    ) -> Result<ItemEffectsRequest, OwnerBridgeError> {
        if !self.config.item_effects {
            return Err(owners::Error::Unsupported.into());
        }
        let parts = self.partitions()?;
        let mut transport = ItemEffectsTransport::install(parts, self.config.clone())?;
        let _ = transport.preview(&action)?;
        let guards = self.item_effects_session_witnesses()?;
        Ok(ItemEffectsRequest { action, guards })
    }

    pub fn execute_item_effect(
        &self,
        request: ItemEffectsRequest,
    ) -> Result<ItemEffectsReply, OwnerBridgeError> {
        if !self.config.item_effects {
            return Err(owners::Error::Unsupported.into());
        }
        let current_witnesses = self.item_effects_session_witnesses()?;
        if request.guards != current_witnesses {
            #[cfg(test)]
            STALE_REFUSALS.with(|x| x.set(x.get() + 1));
            return Err(owners::Error::Stale.into());
        }
        let parts = self.partitions()?;
        let mut transport = ItemEffectsTransport::install(parts, self.config.clone())?;
        let (reply, updated) = transport.execute(request)?;
        let source = Self::install(updated, self.config.clone())?;
        let wire = source.capture()?;
        drop(source);
        self.restore(&wire)?;
        Ok(reply)
    }

    pub fn step_item_effect(
        &self,
        action: ItemEffectAction,
    ) -> Result<ItemEffectsReply, OwnerBridgeError> {
        let req = self.prepare_item_effect(action)?;
        self.execute_item_effect(req)
    }
}
