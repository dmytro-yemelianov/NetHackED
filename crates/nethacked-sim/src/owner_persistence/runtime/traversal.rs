//! Explicit M6-I9 traversal dispatch: Ascend and Descend with level
//! pack/unpack residency, branch transitions, the vibrating-square portal,
//! the mysterious force, and legacy-exact object timer catch-up.
use super::*;
use crate::actions::command_frame::{self, BorrowedObjects};
use crate::actions::stairs::{clamp_mysterious_force, quest_species_by_name, SANCTUM_DEPTH};
use crate::actions::wand::ports::SelectedObject;
use crate::passive_afflictions;
use crate::passive_dressing;
use crate::turns::natural_hp_heal;
use crate::GameEvent;
use nethacked_arena::{
    frozen_in_ice_box, ActorId, ActorRecord, ActorStorage, ItemId, ItemLocation, ItemRecord,
    ItemStorage,
};
use nethacked_core::energy::{
    hero_movement_gain, monster_movement_gain, SchedulerState, NORMAL_SPEED,
};
use nethacked_core::engraving::Engraving;
use nethacked_core::inventory::EncumbranceTier;
use nethacked_core::{QuestProgress, SpellKind};
use nethacked_data::ruleset::MonsterDef;
use nethacked_data::{ItemKindId, MonsterSound, MonsterSpeciesId, RaceId, RoleId};
use nethacked_dungeon::{
    find_free_floor, generate_dungeon_level, generate_gehennom_maze_level,
    generate_mines_cavern_level, generate_mines_end_level, generate_minetown_level,
    generate_moloch_sanctum_level, generate_quest_goal_level, generate_quest_home_level,
    generate_quest_locate_level, generate_sokoban_level, generate_valley_of_the_dead,
    reachable_from, DungeonLevel, Rect,
};
use nethacked_kernel::ModuleHandle;
use nethacked_types::{
    Alignment, BranchCoord, BranchId, Buc, Coord, Direction, EndgamePlane, GenocideRegistry,
    Locale, Tile, COLNO, ROWNO,
};
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use wand::OwnerWitness;

pub const HOME: &str = "traversal-home-i9-v1";
pub const CALLER: &str = "traversal-caller-i9-v1";
pub const PREVIEW: &str = "traversal_preview_i9";
pub const RUN: &str = "traversal_run_i9";

pub const READS: [&str; 9] = [
    "trav_clock_read",
    "trav_objects_read",
    "trav_actors_read",
    "trav_hero_read",
    "trav_equipment_read",
    "trav_billing_read",
    "trav_random_read",
    "trav_geography_read",
    "trav_session_read",
];

pub const WRITES: [&str; 8] = [
    "trav_clock_prepare",
    "trav_objects_prepare",
    "trav_actors_prepare",
    "trav_hero_prepare",
    "trav_equipment_prepare",
    "trav_billing_prepare",
    "trav_geography_prepare",
    "trav_random_prepare",
];

pub const POLICY: [u8; 32] = [30; 32];

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
pub enum TraversalAction {
    Ascend,
    Descend,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraversalRequest {
    pub action: TraversalAction,
    pub guards: Vec<OwnerWitness>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraversalReply {
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
    pub stored_monsters: Vec<Vec<(ActorId, ActorRecord)>>,
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
    pub mysterious_force_count: u32,
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
    pub stored_levels: Vec<DungeonLevel>,
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
    pub stored_items: Vec<Vec<(ItemId, ItemRecord)>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorsOutcome {
    pub actors: ActorStorage,
    pub stored_monsters: Vec<Vec<(ActorId, ActorRecord)>>,
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
    pub mysterious_force_count: u32,
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
    pub stored_unpaid_items: Vec<Vec<(ItemId, u32)>>,
}

/// Full geography write: traversal replaces the whole level and reorders the
/// stored-level cache, so the complete residency rides in the outcome (the
/// traversal transport's 1 MiB reply bounds carry it).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeographyOutcome {
    pub level: DungeonLevel,
    pub branch: BranchId,
    pub depth: usize,
    pub vibrating_square: Option<Coord>,
    pub stored_keys: Vec<(BranchId, usize)>,
    pub stored_levels: Vec<DungeonLevel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraversalOutcome {
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

impl TraversalOutcome {
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
    pub(super) outcome: TraversalOutcome,
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
            in_mklev: true,
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

/// object_timers.rs `start_object_timer_at` residency lookup over the owner
/// facts: live arena items plus cached stored-level items.
fn timer_target_exists(
    items: &ItemStorage,
    stored_keys: &[(BranchId, usize)],
    stored_items: &[Vec<(ItemId, ItemRecord)>],
    target: crate::object_timers::ObjectTimerTarget,
) -> bool {
    use crate::object_timers::ObjectTimerTarget;
    match target {
        ObjectTimerTarget::Active(id) => items.contains_key(id),
        ObjectTimerTarget::Cached {
            branch,
            depth,
            item,
        } => stored_keys
            .iter()
            .position(|key| *key == (branch, depth))
            .is_some_and(|pos| stored_items[pos].iter().any(|(id, _)| *id == item)),
    }
}

/// object_timers.rs `attach_generated_item` over the owner facts: materialize
/// a sampled tree once, preserving generation order and clock. No RNG or age
/// mutation occurs here. C mkbox_cnts starts then cancels ice corpse timers:
/// cancelled entries still consume an insertion identity.
fn attach_generated_item(
    obj: &mut ObjectsFacts,
    timers: &mut crate::object_timers::ObjectTimerQueue,
    geo: &GeographyFacts,
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
                timers.start_at(target, kind, sampled_at.checked_add_signed(delay), |t| {
                    timer_target_exists(&obj.items, &geo.stored_keys, &obj.stored_items, t)
                })?;
            }
        }
    }
    if frozen_in_ice_box(&obj.items[id], |parent| obj.items.get(parent)) {
        timers.cancel(target);
    }
    for child in tree.contents {
        attach_generated_item(obj, timers, geo, child, ItemLocation::InContainer(id))?;
    }
    Ok(id)
}

/// Arena `items_in_container`: direct children in storage order.
fn items_in_container(items: &ItemStorage, container: ItemId) -> Vec<ItemId> {
    items
        .iter()
        .filter_map(|(id, item)| {
            if item.location == ItemLocation::InContainer(container) {
                Some(id)
            } else {
                None
            }
        })
        .collect()
}

/// peace.rs `hero_alignment`: the player actor's alignment, neutral when the
/// hero record is absent.
fn hero_alignment(a: &ActorsFacts, h: &HeroFacts) -> Alignment {
    a.actors
        .get(h.player_id)
        .map(|p| p.alignment)
        .unwrap_or(Alignment::Neutral)
}

/// world.rs `forget_rot_corpse_references`: clear every represented reference
/// after the rot callback removes an item. The legacy immediate
/// `reconcile_equipment`/`recompute_hero_ac` pair is a no-op for a corpse
/// (corpses never occupy armor slots or dressing targets, and clearing a
/// wielded/quivered corpse does not change AC); the transport's boundary
/// reconciliations produce the same committed state.
fn forget_rot_corpse_references(
    h: &mut HeroFacts,
    eq: &mut EquipmentFacts,
    bill: &mut BillingFacts,
    id: ItemId,
) {
    if eq.wielded_item == Some(id) {
        eq.wielded_item = None;
    }
    if h.hero.quivered_item == Some(id) {
        h.hero.quivered_item = None;
    }
    // world.rs `remove_unpaid`: `let _ = try_remove_unpaid(id)`; a frozen or
    // consumed obligation keeps its unpaid entry for the settlement route.
    if !bill.billing.live_terms.iter().any(|(i, _)| *i == id)
        && !bill.billing.consumed.iter().any(|b| b.origin == id)
    {
        bill.unpaid_items.retain(|(i, _)| *i != id);
    }
    // actions/equipment.rs `remove_destroyed_item_equipment` minus the no-op
    // reconcile/AC pair (see above).
    eq.equipment.armor.retain(|(_, item)| *item != id);
    if eq.equipment.dressing.as_ref().is_some_and(|d| d.item == id) {
        eq.equipment.dressing = None;
    }
}

/// corpse_revival.rs `revive_ordinary_troll` over the owner facts. Expiry was
/// consumed and reported before this callback; unsupported families remain
/// neutral and ordinary failures reschedule fresh rot.
#[allow(clippy::too_many_arguments)]
fn revive_ordinary_troll(
    id: ItemId,
    clk: &mut ClockFacts,
    obj: &mut ObjectsFacts,
    a: &mut ActorsFacts,
    h: &mut HeroFacts,
    eq: &mut EquipmentFacts,
    bill: &mut BillingFacts,
    geo: &GeographyFacts,
    sess: &SessionFacts,
    config: &OwnerBridgeConfig,
    rng: &mut rand_chacha::ChaCha8Rng,
) -> owners::Result<()> {
    let input = crate::revival_law::RevivalInput {
        ruleset: &config.ruleset,
        items: &obj.items,
        actors: &a.actors,
        level: &geo.level,
        genocide_registry: &h.genocide_registry,
        player: h.player_id,
        branch: geo.branch,
        depth: geo.depth,
        hero_alignment: hero_alignment(a, h),
        hero_race: h.hero_race,
        alignment_record: h.alignment_record,
        role_name: &sess.role_name,
    };
    let attempt = crate::revival_law::ordinary_troll_attempt(id, &input, rng);
    let target = crate::object_timers::ObjectTimerTarget::Active(id);
    match attempt {
        crate::revival_law::RevivalAttempt::Unsupported => {}
        crate::revival_law::RevivalAttempt::Failed { rot_delay } => {
            if !clk.object_timers.entries().iter().any(|t| {
                t.target == target && t.kind == crate::object_timers::ObjectTimerKind::RotCorpse
            }) {
                let due = clk.scheduler.turn.checked_add(rot_delay);
                clk.object_timers
                    .start_at(
                        target,
                        crate::object_timers::ObjectTimerKind::RotCorpse,
                        due,
                        |t| timer_target_exists(&obj.items, &geo.stored_keys, &obj.stored_items, t),
                    )
                    .map_err(|_| owners::Error::Invariant)?;
            }
        }
        crate::revival_law::RevivalAttempt::Revived { actor } => {
            // world.rs `normalize_revival_actor_allocator`: match SlotMap's
            // restored freelist before the revival inserts one actor.
            a.actors = serde_json::from_value(
                serde_json::to_value(&a.actors).expect("actor arena serializes"),
            )
            .expect("serialized actor arena restores");
            a.actors.insert(actor);
            if obj.items[id].quantity > 1 {
                let _split_identity = rng.random_range(0..2u32);
                clk.object_timers.consume_split_timer_serials(target);
                obj.items[id].quantity -= 1;
            } else {
                clk.object_timers.cancel(target);
                destroy_item(obj, id);
                forget_rot_corpse_references(h, eq, bill, id);
            }
        }
    }
    Ok(())
}

/// object_timers.rs `run_object_timers` over the owner facts: reconcile, then
/// drain every due active timer. Legacy emits only `ObjectTimerExpired`
/// events; revival and rot produce no messages.
#[allow(clippy::too_many_arguments)]
fn run_object_timers(
    clk: &mut ClockFacts,
    obj: &mut ObjectsFacts,
    a: &mut ActorsFacts,
    h: &mut HeroFacts,
    eq: &mut EquipmentFacts,
    bill: &mut BillingFacts,
    geo: &GeographyFacts,
    sess: &SessionFacts,
    config: &OwnerBridgeConfig,
    rng: &mut rand_chacha::ChaCha8Rng,
) -> owners::Result<Vec<GameEvent>> {
    clk.object_timers
        .reconcile(|t| timer_target_exists(&obj.items, &geo.stored_keys, &obj.stored_items, t));
    let mut events = Vec::new();
    while let Some(timer) = clk.object_timers.pop_due_active(clk.scheduler.turn) {
        events.push(GameEvent::ObjectTimerExpired { timer });
        if timer.kind == crate::object_timers::ObjectTimerKind::ReviveMonster {
            let crate::object_timers::ObjectTimerTarget::Active(id) = timer.target else {
                unreachable!()
            };
            revive_ordinary_troll(id, clk, obj, a, h, eq, bill, geo, sess, config, rng)?;
        }
        if timer.kind == crate::object_timers::ObjectTimerKind::RotCorpse {
            let crate::object_timers::ObjectTimerTarget::Active(id) = timer.target else {
                unreachable!()
            };
            // C dig.c:2127–2140 buries a corpse's contents before disposal.
            // That owner is missing: no recursive destruction substitute.
            if !obj.items.get(id).is_some_and(|item| item.is_corpse())
                || !items_in_container(&obj.items, id).is_empty()
            {
                continue;
            }
            // C dealloc_obj cancels all remaining object timers; the due
            // entry and expiry envelope already precede this real effect.
            clk.object_timers.cancel(timer.target);
            destroy_item(obj, id);
            forget_rot_corpse_references(h, eq, bill, id);
        }
    }
    Ok(events)
}

/// world.rs `register_existing_artifacts` over the owner facts: register
/// imported, cached and explicitly mutated records without RNG.
fn register_existing_artifacts(obj: &mut ObjectsFacts, rules: &crate::Ruleset) {
    let history = &mut obj.artifact_history;
    let mut register = |item: &mut ItemRecord| {
        if item.artifact_id.is_none() {
            item.artifact_id = rules.artifact_id_for_item(&item.name).map(str::to_owned);
        }
        if let Some(id) = &item.artifact_id {
            history.insert(id.clone());
        }
    };
    for (_, item) in obj.items.iter_mut() {
        register(item);
    }
    for stored in &mut obj.stored_items {
        for (_, item) in stored.iter_mut() {
            register(item);
        }
    }
}

/// world.rs `monster_spawn_context`: the current sim geography exposes Astral
/// only; elemental planes can be supplied directly to the data factory when
/// that geography is modeled.
fn monster_spawn_context(geo: &GeographyFacts) -> nethacked_data::monsters::MonsterSpawnContext {
    nethacked_data::monsters::MonsterSpawnContext {
        endgame_plane: (geo.branch == BranchId::AstralPlane).then_some(EndgamePlane::Astral),
    }
}

/// world.rs `actor_is_genocided` over the owner facts.
fn actor_is_genocided(rules: &crate::Ruleset, genocide: &GenocideRegistry, name: &str) -> bool {
    let lower = name.to_lowercase();
    let class = rules.monster_class_of(name);
    genocide.genocided_species.contains(&lower)
        || class
            .map(|c| genocide.genocided_classes.contains(&c))
            .unwrap_or(false)
}

/// peace.rs `set_monster_malign` over the owner facts: precalculate the
/// alignment adjustment upon monster death.
fn set_monster_malign(
    rec: &mut ActorRecord,
    def: &MonsterDef,
    role_name: &str,
    hero_alignment: Alignment,
) {
    let quest_cfg = nethacked_core::get_role_quest_config_or_default(role_name);
    let is_leader = matches!(def.msound, MonsterSound::Leader)
        || rec.name.eq_ignore_ascii_case(quest_cfg.leader_name);
    rec.malign = nethacked_core::calculate_malign(
        def.maligntyp,
        hero_alignment,
        rec.is_peaceful,
        is_leader,
        def.peaceful_by_default,
        def.always_hostile,
    );
}

/// peace.rs `roll_spawn_peaceful` over the owner facts: `peace_minded` for a
/// newly created monster, drawing from the sim RNG only when C draws.
fn roll_spawn_peaceful(
    rules: &crate::Ruleset,
    species: MonsterSpeciesId,
    hero_alignment: Alignment,
    hero_race: RaceId,
    alignment_record: i32,
    has_amulet: bool,
    rng: &mut impl Rng,
) -> bool {
    let Some(def) = rules.monster_by_id(species) else {
        return false;
    };
    let input =
        crate::peace::peace_input(def, hero_alignment, hero_race, alignment_record, has_amulet);
    crate::peace::roll_peace_minded(&input, rng)
}

/// stairs.rs `spawn_monster_by_name` over the owner facts: spawn at or near
/// `preferred` on a passable, unoccupied, non-stairs tile (searching outward
/// up to radius 3 in row-major ring order). Returns `None` if no spot or
/// genocided.
#[allow(clippy::too_many_arguments)]
fn spawn_monster_by_name(
    a: &mut ActorsFacts,
    obj: &ObjectsFacts,
    geo: &GeographyFacts,
    h: &HeroFacts,
    sess: &SessionFacts,
    config: &OwnerBridgeConfig,
    rng: &mut rand_chacha::ChaCha8Rng,
    name: &str,
    preferred: Coord,
) -> Option<ActorId> {
    if actor_is_genocided(&config.ruleset, &h.genocide_registry, name) {
        return None;
    }
    let def = config.ruleset.monster(name)?;
    for radius in 0..=3isize {
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx.abs().max(dy.abs()) != radius {
                    continue;
                }
                let (x, y) = (preferred.x as isize + dx, preferred.y as isize + dy);
                if x < 0 || y < 0 {
                    continue;
                }
                let Some(c) = Coord::new(x as usize, y as usize) else {
                    continue;
                };
                if geo.level.is_passable(c)
                    && c != geo.level.stairs_up
                    && c != geo.level.stairs_down
                    && actor_at(&a.actors, c).is_none()
                {
                    let mut rec = config.ruleset.create_monster_record_with_context(
                        name,
                        c,
                        monster_spawn_context(geo),
                        rng,
                    )?;
                    if let Some(species) = def.id {
                        rec.is_peaceful = roll_spawn_peaceful(
                            &config.ruleset,
                            species,
                            hero_alignment(a, h),
                            h.hero_race,
                            h.alignment_record,
                            hero_has_amulet(&obj.items, h.player_id),
                            rng,
                        );
                    } else {
                        rec.is_peaceful = def.peaceful_by_default;
                    }
                    set_monster_malign(&mut rec, def, &sess.role_name, hero_alignment(a, h));
                    return Some(a.actors.insert(rec));
                }
            }
        }
    }
    None
}

/// stairs.rs `spawn_monster_near` over the owner facts.
#[allow(clippy::too_many_arguments)]
fn spawn_monster_near(
    a: &mut ActorsFacts,
    obj: &ObjectsFacts,
    geo: &GeographyFacts,
    h: &HeroFacts,
    sess: &SessionFacts,
    config: &OwnerBridgeConfig,
    rng: &mut rand_chacha::ChaCha8Rng,
    species: MonsterSpeciesId,
    preferred: Coord,
) -> Option<ActorId> {
    let name = config
        .ruleset
        .monster_by_id(species)
        .map(|m| m.name.clone())?;
    spawn_monster_by_name(a, obj, geo, h, sess, config, rng, &name, preferred)
}

/// stairs.rs `place_steed_with_hero` over the owner facts: a mounted hero's
/// steed arrives on the same square as the hero.
fn place_steed_with_hero(a: &mut ActorsFacts, h: &mut HeroFacts) {
    let Some(steed_id) = h.hero.mount.as_ref().map(|m| m.steed_id) else {
        return;
    };
    let Some(hero_coord) = a.actors.get(h.player_id).map(|p| p.coord) else {
        return;
    };
    if let Some(steed) = a.actors.get_mut(steed_id) {
        steed.coord = hero_coord;
    } else {
        h.hero.mount = None;
    }
}

/// stairs.rs `teleport_hero_randomly` over the owner facts: same-level random
/// teleport (C `safe_teleds`, `do.c:1566`) to a random passable, unoccupied
/// tile; no-op if no tile exists.
fn teleport_hero_randomly(
    a: &mut ActorsFacts,
    h: &mut HeroFacts,
    geo: &GeographyFacts,
    rng: &mut rand_chacha::ChaCha8Rng,
) {
    let mut spots = Vec::new();
    for y in 0..ROWNO {
        for x in 0..COLNO {
            if let Some(c) = Coord::new(x, y) {
                if geo.level.is_passable(c) && actor_at(&a.actors, c).is_none() {
                    spots.push(c);
                }
            }
        }
    }
    if spots.is_empty() {
        return;
    }
    let pick = spots[rng.random_range(0..spots.len())];
    if let Some(p) = a.actors.get_mut(h.player_id) {
        p.coord = pick;
    }
    place_steed_with_hero(a, h);
}

/// stairs.rs `pack_current_level` over the owner facts: pack the current
/// floor into the stored-level cache. Every non-player, non-steed actor,
/// every floor item, items carried by packed monsters and everything
/// transitively inside those containers (cycle-safe), plus their
/// unpaid-ledger entries. Items the hero carries stay in the arena; stale
/// quiver/wield references are cleared first.
#[allow(clippy::too_many_arguments)]
fn pack_current_level(
    clk: &mut ClockFacts,
    obj: &mut ObjectsFacts,
    a: &mut ActorsFacts,
    h: &mut HeroFacts,
    eq: &mut EquipmentFacts,
    bill: &mut BillingFacts,
    geo: &mut GeographyFacts,
    config: &OwnerBridgeConfig,
) {
    register_existing_artifacts(obj, &config.ruleset);
    let from_key = (geo.branch, geo.depth);
    let steed_id = h.hero.mount.as_ref().map(|m| m.steed_id);

    // Quiver must refer to something the hero carries.
    if let Some(q) = h.hero.quivered_item {
        let carried = obj
            .items
            .get(q)
            .map(|it| it.location == ItemLocation::CarriedBy(h.player_id))
            .unwrap_or(false);
        if !carried {
            h.hero.quivered_item = None;
        }
    }

    // Wielded item must refer to something the hero carries.
    if let Some(w) = eq.wielded_item {
        let carried = obj
            .items
            .get(w)
            .map(|it| it.location == ItemLocation::CarriedBy(h.player_id))
            .unwrap_or(false);
        if !carried {
            eq.wielded_item = None;
        }
    }

    let monster_ids: Vec<ActorId> = a
        .actors
        .iter()
        .filter(|(id, _)| *id != h.player_id && Some(*id) != steed_id)
        .map(|(id, _)| id)
        .collect();

    // Lookup-only sets: membership tests never drive ordering.
    let monster_set: std::collections::HashSet<ActorId> = monster_ids.iter().copied().collect();
    // Items belonging to the level: floor items, items carried by packed
    // monsters, and everything transitively inside those.
    let mut item_ids: Vec<ItemId> = obj
        .items
        .iter()
        .filter(|(_, it)| match it.location {
            ItemLocation::Floor(_) => true,
            ItemLocation::CarriedBy(actor) => monster_set.contains(&actor),
            _ => false,
        })
        .map(|(id, _)| id)
        .collect();
    let mut item_set: std::collections::HashSet<ItemId> = item_ids.iter().copied().collect();
    let mut i = 0;
    while i < item_ids.len() {
        for child in items_in_container(&obj.items, item_ids[i]) {
            if item_set.insert(child) {
                item_ids.push(child);
            }
        }
        i += 1;
    }

    let mut monsters = Vec::with_capacity(monster_ids.len());
    for mid in monster_ids {
        if let Some(actor) = a.actors.remove(mid) {
            monsters.push((mid, actor));
        }
    }
    let mut items = Vec::with_capacity(item_ids.len());
    clk.object_timers
        .cache_active_targets(from_key.0, from_key.1, &item_ids);
    for iid in &item_ids {
        if let Some(item) = obj.items.get(*iid).cloned() {
            destroy_item(obj, *iid);
            items.push((*iid, item));
        }
    }

    let (level_unpaid, hero_unpaid): (Vec<_>, Vec<_>) = std::mem::take(&mut bill.unpaid_items)
        .into_iter()
        .partition(|(iid, _)| item_set.contains(iid));
    bill.unpaid_items = hero_unpaid;

    if let Some(pos) = geo.stored_keys.iter().position(|k| *k == from_key) {
        geo.stored_levels[pos] = geo.level.clone();
        obj.stored_items[pos] = items;
        a.stored_monsters[pos] = monsters;
        bill.stored_unpaid_items[pos] = level_unpaid;
    } else {
        geo.stored_keys.push(from_key);
        geo.stored_levels.push(geo.level.clone());
        obj.stored_items.push(items);
        a.stored_monsters.push(monsters);
        bill.stored_unpaid_items.push(level_unpaid);
    }
}

/// stairs.rs `roll_room_object_with_artifacts` over the owner facts: kind
/// selection, supported initialization, then the room continuation draw.
/// Rejected placement still consumes initialization for the selected object.
#[allow(clippy::too_many_arguments)]
fn roll_room_object_with_artifacts(
    rules: &crate::Ruleset,
    level: &DungeonLevel,
    coord: Option<Coord>,
    table: nethacked_data::generation::ObjectTable,
    ctx: nethacked_data::containers::ContainerContext,
    monsters: Option<(
        &nethacked_data::generation::MonsterGenContext,
        &GenocideRegistry,
    )>,
    mut artifacts: Option<(&str, &mut BTreeSet<String>)>,
    mut rn2: impl FnMut(u32) -> u32,
) -> (Option<nethacked_data::containers::GeneratedItem>, bool) {
    let selected = rules.room_item_definition(None, table, &mut rn2);
    let kind = selected.and_then(|d| d.id);
    let spot = coord.filter(|&c| level.is_passable(c));
    let item = selected.and_then(|def| {
        let name = &def.name;
        let location = spot.map_or(ItemLocation::Limbo, ItemLocation::Floor);
        if let Some((monster_ctx, registry)) = monsters {
            rules.create_random_item_tree_with_monsters(
                name,
                location,
                ctx,
                monster_ctx,
                |d| nethacked_core::genocide::is_genocided(registry, &d.name, d.glyph),
                &mut rn2,
            )
        } else {
            rules.create_random_item_tree(name, location, ctx, &mut rn2)
        }
    });
    let item = item.map(|mut tree| {
        if let (Some(kind), Some((role, history))) = (kind, artifacts.as_mut()) {
            rules.random_existing_artifact(&mut tree.item, kind, role, history, &mut rn2);
        }
        tree
    });
    let item = item.filter(|_| spot.is_some());
    let keep_generating = rn2(5) == 0;
    (item, keep_generating)
}

/// stairs.rs `unpack_or_generate_level` over the owner facts: restore an
/// existing cached level or procedurally generate a new level, then catch up
/// object timers (C do.c:1816-1823).
#[allow(clippy::too_many_arguments)]
fn unpack_or_generate_level(
    clk: &mut ClockFacts,
    obj: &mut ObjectsFacts,
    a: &mut ActorsFacts,
    h: &mut HeroFacts,
    eq: &mut EquipmentFacts,
    bill: &mut BillingFacts,
    geo: &mut GeographyFacts,
    sess: &SessionFacts,
    config: &OwnerBridgeConfig,
    rng: &mut rand_chacha::ChaCha8Rng,
    branch: BranchId,
    depth: usize,
) -> owners::Result<Vec<GameEvent>> {
    register_existing_artifacts(obj, &config.ruleset);
    let mut events = Vec::new();
    let target_key = (branch, depth);

    if let Some(pos) = geo.stored_keys.iter().position(|k| *k == target_key) {
        let stored_level = geo.stored_levels.remove(pos);
        geo.stored_keys.remove(pos);
        let stored_items = obj.stored_items.remove(pos);
        let stored_monsters = a.stored_monsters.remove(pos);
        let stored_unpaid = bill.stored_unpaid_items.remove(pos);
        geo.level = stored_level;

        // slotmap 1.1.1 serializes slots/versions but rebuilds its private
        // freelist on deserialize (basic.rs:1357-1397). Normalize both live
        // allocators to that same order before reminting cached handles, so
        // a mid-cache save cannot change subsequent IDs or events. Existing
        // occupied handles, records and slot versions are intact.
        a.actors = serde_json::from_value(
            serde_json::to_value(&a.actors).expect("actor arena serializes"),
        )
        .expect("serialized actor arena restores");
        obj.items = serde_json::from_value(
            serde_json::to_value(&obj.items).expect("item arena serializes"),
        )
        .expect("serialized item arena restores");

        // Lookup-only maps: reminting order follows the stored Vec order.
        let mut actor_map = std::collections::HashMap::new();
        for (old, m) in stored_monsters {
            actor_map.insert(old, a.actors.insert(m));
        }
        let mut item_map = std::collections::HashMap::new();
        let mut spawned = Vec::with_capacity(stored_items.len());
        for (old, it) in stored_items {
            let new = spawn_item(obj, it);
            item_map.insert(old, new);
            spawned.push(new);
        }
        let fallback = ItemLocation::Floor(geo.level.stairs_up);
        for new in spawned {
            if let Some(it) = obj.items.get_mut(new) {
                it.location = match it.location.clone() {
                    ItemLocation::InContainer(old) => item_map
                        .get(&old)
                        .map(|n| ItemLocation::InContainer(*n))
                        .unwrap_or(fallback.clone()),
                    ItemLocation::CarriedBy(old) => actor_map
                        .get(&old)
                        .map(|n| ItemLocation::CarriedBy(*n))
                        .unwrap_or(fallback.clone()),
                    other => other,
                };
            }
        }
        for (old, cost) in stored_unpaid {
            if let Some(new) = item_map.get(&old) {
                bill.unpaid_items.push((*new, cost));
            }
        }
        // Only restored live handles move. Detached origins and creditor
        // identities remain immutable historical traces, in Vec order.
        for (id, _) in &mut bill.billing.live_terms {
            if let Some(new) = item_map.get(id) {
                *id = *new;
            }
        }
        clk.object_timers
            .restore_cached_targets(branch, depth, |old| item_map.get(&old).copied());
    } else {
        match (branch, depth) {
            (BranchId::Sokoban, _) => {
                let (soko_level, boulder_coords) = generate_sokoban_level(depth);
                geo.level = soko_level;

                // Spawn puzzle boulders
                for bc in boulder_coords {
                    if let Some(boulder) = config.ruleset.create_item_record_by_id(
                        ItemKindId::BOULDER,
                        ItemLocation::Floor(bc),
                        Buc::Uncursed,
                    ) {
                        spawn_item(obj, boulder);
                    }
                }

                // Spawn prize in prize chamber (at depth 1: Bag of Holding)
                let prize_coord = Coord::new_unchecked(64, 10);
                let prize_kind = if depth == 1 {
                    ItemKindId::BAG_OF_HOLDING
                } else {
                    ItemKindId::AMULET_OF_REFLECTION
                };
                if let Some(prize) = config.ruleset.create_item_record_by_id(
                    prize_kind,
                    ItemLocation::Floor(prize_coord),
                    Buc::Blessed,
                ) {
                    spawn_item(obj, prize);
                }

                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::enter_sokoban(sess.locale).into(),
                });
            }
            (BranchId::GnomishMines, 3) => {
                // Minetown! Complete with shops, temple, priest, and watchmen
                let layout = generate_minetown_level(rng);
                geo.level = layout.level;

                spawn_monster_near(
                    a,
                    obj,
                    geo,
                    h,
                    sess,
                    config,
                    rng,
                    MonsterSpeciesId::ALIGNED_CLERIC,
                    layout.priest_coord,
                );

                for wc in layout.watchmen_coords {
                    spawn_monster_near(
                        a,
                        obj,
                        geo,
                        h,
                        sess,
                        config,
                        rng,
                        MonsterSpeciesId::WATCHMAN,
                        wc,
                    );
                }

                for sc in layout.shopkeeper_coords {
                    spawn_monster_near(
                        a,
                        obj,
                        geo,
                        h,
                        sess,
                        config,
                        rng,
                        MonsterSpeciesId::SHOPKEEPER,
                        sc,
                    );
                }

                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::enter_minetown(sess.locale).into(),
                });
            }
            (BranchId::GnomishMines, 5) => {
                // Mines' End! Sprawling labyrinth with guaranteed Luckstone
                let (lvl, luckstone_coord) = generate_mines_end_level(rng);
                geo.level = lvl;

                if let Some(luckstone) = config.ruleset.create_item_record_by_id(
                    ItemKindId::LUCKSTONE,
                    ItemLocation::Floor(luckstone_coord),
                    Buc::Uncursed,
                ) {
                    spawn_item(obj, luckstone);
                }

                let centers: Vec<Coord> = geo.level.rooms.iter().map(|r| r.center()).collect();
                for (i, &center) in centers.iter().enumerate() {
                    if i > 0 {
                        let species = if i % 2 == 0 {
                            MonsterSpeciesId::SILVER_DRAGON
                        } else {
                            MonsterSpeciesId::VAMPIRE
                        };
                        spawn_monster_near(a, obj, geo, h, sess, config, rng, species, center);
                    }
                }

                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::reach_mines_end(sess.locale).into(),
                });
            }
            (BranchId::GnomishMines, d) => {
                // Caverns (1, 2, 4)
                let lvl = generate_mines_cavern_level(rng, d);
                geo.level = lvl;

                let centers: Vec<Coord> = geo.level.rooms.iter().map(|r| r.center()).collect();
                for (i, &center) in centers.iter().enumerate() {
                    if i > 0 {
                        let species = if i % 2 == 0 {
                            MonsterSpeciesId::GNOME
                        } else {
                            MonsterSpeciesId::DWARF
                        };
                        spawn_monster_near(a, obj, geo, h, sess, config, rng, species, center);
                    }
                }

                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::descend_gnomish_mines(sess.locale).into(),
                });
            }
            (BranchId::Gehennom, 1) => {
                // Valley of the Dead
                let lvl = generate_valley_of_the_dead(rng);
                geo.level = lvl;

                let centers: Vec<Coord> = geo.level.rooms.iter().map(|r| r.center()).collect();
                for (i, &center) in centers.iter().enumerate() {
                    if i > 0 {
                        let species = if i % 2 == 0 {
                            MonsterSpeciesId::SKELETON
                        } else {
                            MonsterSpeciesId::VAMPIRE
                        };
                        spawn_monster_near(a, obj, geo, h, sess, config, rng, species, center);
                    }
                }

                // Spawn Bell of Opening in Valley of the Dead
                let bell_coord = geo.level.rooms[1].center();
                if let Some(bell) = config.ruleset.create_item_record_by_id(
                    ItemKindId::BELL_OF_OPENING,
                    ItemLocation::Floor(bell_coord),
                    Buc::Blessed,
                ) {
                    spawn_item(obj, bell);
                }

                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::cross_valley_of_dead(sess.locale).into(),
                });
            }
            (BranchId::Gehennom, 5) => {
                // Deepest Gehennom Maze with Vibrating Square
                let (lvl, vs) = generate_gehennom_maze_level(rng, 5, true);
                geo.level = lvl;
                geo.vibrating_square = vs;

                let centers: Vec<Coord> = geo.level.rooms.iter().map(|r| r.center()).collect();
                for (i, &center) in centers.iter().enumerate() {
                    if i > 0 {
                        spawn_monster_near(
                            a,
                            obj,
                            geo,
                            h,
                            sess,
                            config,
                            rng,
                            MonsterSpeciesId::SILVER_DRAGON,
                            center,
                        );
                    }
                }

                // Invocation items on free floor reachable from the up stairs
                let vs_avoid: Vec<Coord> = geo.vibrating_square.into_iter().collect();
                let rects: Vec<Rect> = geo.level.rooms.iter().map(|r| r.rect).collect();
                let reach = reachable_from(&geo.level, geo.level.stairs_up);
                let mut order: Vec<Coord> = find_free_floor(&geo.level, &rects[1], &vs_avoid)
                    .into_iter()
                    .filter(|c| reach.contains(c))
                    .collect();
                for (i, rect) in rects.iter().enumerate() {
                    if i == 1 {
                        continue;
                    }
                    for c in find_free_floor(&geo.level, rect, &vs_avoid) {
                        if reach.contains(&c) && !order.contains(&c) {
                            order.push(c);
                        }
                    }
                }
                let mut spots = order.into_iter();
                if let Some(c) = spots.next() {
                    if let Some(it) = config.ruleset.create_item_record_by_id(
                        ItemKindId::CANDELABRUM_OF_INVOCATION,
                        ItemLocation::Floor(c),
                        Buc::Uncursed,
                    ) {
                        spawn_item(obj, it);
                    }
                }
                if let Some(c) = spots.next() {
                    if let Some(it) = config.ruleset.create_item_record_by_id(
                        ItemKindId::SPE_BOOK_OF_THE_DEAD,
                        ItemLocation::Floor(c),
                        Buc::Blessed,
                    ) {
                        spawn_item(obj, it);
                    }
                }
                for c in spots.take(7) {
                    if let Some(it) = config.ruleset.create_item_record_by_id(
                        ItemKindId::WAX_CANDLE,
                        ItemLocation::Floor(c),
                        Buc::Uncursed,
                    ) {
                        spawn_item(obj, it);
                    }
                }

                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::reach_bottom_gehennom(sess.locale).into(),
                });
            }
            (BranchId::Gehennom, SANCTUM_DEPTH) => {
                // Moloch's Sanctum
                let (lvl, _spawn) = generate_moloch_sanctum_level(rng);
                geo.level = lvl;

                // C `priestini` makes the high priest peaceful; `intemple`
                // (priest.c:449-456) turns him hostile the first time the
                // hero enters the temple, with no alignment penalty
                // (`set_malign`). The sim has no temple-entry hook, so it
                // applies this on arrival (documented divergence).
                let priest = spawn_monster_near(
                    a,
                    obj,
                    geo,
                    h,
                    sess,
                    config,
                    rng,
                    MonsterSpeciesId::ALIGNED_CLERIC,
                    geo.level.stairs_down,
                );
                // `set_malign` (priest.c:456) reads the shrine alignment
                // `EPRI->shralign`, which is A_NONE here, not the species'.
                let hero_align = hero_alignment(a, h);
                if let Some(m) = priest.and_then(|id| a.actors.get_mut(id)) {
                    m.is_peaceful = false;
                    m.malign = nethacked_core::calculate_malign(
                        nethacked_core::peace::A_NONE,
                        hero_align,
                        false,
                        false,
                        false,
                        false,
                    );
                }

                if let Some(amulet) = config.ruleset.create_item_record_by_id(
                    ItemKindId::AMULET_OF_YENDOR,
                    ItemLocation::Floor(geo.level.stairs_down),
                    Buc::Blessed,
                ) {
                    spawn_item(obj, amulet);
                }

                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::enter_moloch_sanctum(sess.locale).into(),
                });
                if priest.is_some() {
                    // priest.c:451-452, spoken with `verbalize1`.
                    events.push(GameEvent::LogMessage {
                        text: nethacked_i18n::Messages::sanctum_infidel(sess.locale).into(),
                    });
                    events.push(GameEvent::LogMessage {
                        text: nethacked_i18n::Messages::sanctum_be_gone(sess.locale).into(),
                    });
                }
            }
            (BranchId::Gehennom, d) => {
                // Intermediate Gehennom Mazes
                let (lvl, _) = generate_gehennom_maze_level(rng, d, false);
                geo.level = lvl;

                let centers: Vec<Coord> = geo.level.rooms.iter().map(|r| r.center()).collect();
                for (i, &center) in centers.iter().enumerate() {
                    if i > 0 {
                        let species = if i % 2 == 0 {
                            MonsterSpeciesId::SILVER_DRAGON
                        } else {
                            MonsterSpeciesId::VAMPIRE
                        };
                        spawn_monster_near(a, obj, geo, h, sess, config, rng, species, center);
                    }
                }

                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::delve_gehennom(d, sess.locale),
                });
            }
            (BranchId::Quest, 1) => {
                let layout = generate_quest_home_level(rng, &sess.role_name);
                geo.level = layout.level;

                let quest_cfg = nethacked_core::get_role_quest_config_or_default(&sess.role_name);
                if let Some(leader_species) =
                    quest_species_by_name(&config.ruleset, quest_cfg.leader_name)
                {
                    // Leader and guardians are M2_PEACEFUL (and MS_LEADER /
                    // MS_GUARDIAN), so peace_minded makes them peaceful.
                    spawn_monster_near(
                        a,
                        obj,
                        geo,
                        h,
                        sess,
                        config,
                        rng,
                        leader_species,
                        layout.leader_coord,
                    );
                } else if spawn_monster_by_name(
                    a,
                    obj,
                    geo,
                    h,
                    sess,
                    config,
                    rng,
                    quest_cfg.leader_name,
                    layout.leader_coord,
                )
                .is_none()
                {
                    events.push(GameEvent::LogMessage {
                        text: format!(
                            "Quest leader '{}' missing from ruleset.",
                            quest_cfg.leader_name
                        ),
                    });
                }

                // C role.c `guardnum`: one guardian species per role.
                if let Some(guardian_species) =
                    quest_species_by_name(&config.ruleset, quest_cfg.guardian_name)
                {
                    for gc in layout.guardian_coords {
                        spawn_monster_near(a, obj, geo, h, sess, config, rng, guardian_species, gc);
                    }
                } else {
                    for gc in layout.guardian_coords {
                        spawn_monster_by_name(
                            a,
                            obj,
                            geo,
                            h,
                            sess,
                            config,
                            rng,
                            quest_cfg.guardian_name,
                            gc,
                        );
                    }
                }

                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::enter_quest_sanctuary(
                        quest_cfg.leader_name,
                        quest_cfg.home_desc,
                        sess.locale,
                    ),
                });
            }
            (BranchId::Quest, 2) => {
                let lvl = generate_quest_locate_level(rng, 2);
                geo.level = lvl;

                let centers: Vec<Coord> = geo.level.rooms.iter().map(|r| r.center()).collect();
                for (i, &center) in centers.iter().enumerate() {
                    if i > 0 {
                        let species = if i % 2 == 0 {
                            MonsterSpeciesId::GIANT_ANT
                        } else {
                            MonsterSpeciesId::SKELETON
                        };
                        spawn_monster_near(a, obj, geo, h, sess, config, rng, species, center);
                    }
                }

                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::labyrinth_quest_trial(sess.locale).into(),
                });
            }
            (BranchId::Quest, 3) => {
                let layout = generate_quest_goal_level(rng, &sess.role_name);
                geo.level = layout.level;

                let quest_cfg = nethacked_core::get_role_quest_config_or_default(&sess.role_name);
                let nemesis_id = if let Some(nemesis_species) =
                    quest_species_by_name(&config.ruleset, quest_cfg.nemesis_name)
                {
                    spawn_monster_near(
                        a,
                        obj,
                        geo,
                        h,
                        sess,
                        config,
                        rng,
                        nemesis_species,
                        layout.nemesis_coord,
                    )
                } else {
                    spawn_monster_by_name(
                        a,
                        obj,
                        geo,
                        h,
                        sess,
                        config,
                        rng,
                        quest_cfg.nemesis_name,
                        layout.nemesis_coord,
                    )
                };
                if let Some(id) = nemesis_id {
                    // The Tourist nemesis is the (M2_PEACEFUL) Master of
                    // Thieves; the quest goal level creates it with
                    // `peaceful = 0` (Tou-goal.lua:117). Every other nemesis
                    // is M2_HOSTILE. sp_lev.c:2127-2130: changing `mpeaceful`
                    // re-runs `set_malign`.
                    if let Some(mut rec) = a.actors.get(id).cloned() {
                        rec.is_peaceful = false;
                        if let Some(def) = config.ruleset.monster_for_actor(&rec) {
                            set_monster_malign(
                                &mut rec,
                                def,
                                &sess.role_name,
                                hero_alignment(a, h),
                            );
                        }
                        if let Some(slot) = a.actors.get_mut(id) {
                            *slot = rec;
                        }
                    }
                }

                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::arrive_quest_nemesis(
                        quest_cfg.goal_desc,
                        quest_cfg.nemesis_name,
                        sess.locale,
                    ),
                });
            }
            _ => {
                let mut new_level = generate_dungeon_level(rng);

                // Place branch stairs in Dungeons of Doom
                if branch == BranchId::DungeonsOfDoom && depth == 3 {
                    if let Some(room) = new_level.rooms.get(1) {
                        let branch_coord = Coord::new_unchecked(room.x1 + 1, room.y1 + 1);
                        new_level.set_tile(
                            branch_coord,
                            Tile::BranchStairs {
                                branch: BranchId::GnomishMines,
                                level: 1,
                                up: false,
                            },
                        );
                    }
                }
                if branch == BranchId::DungeonsOfDoom && depth == 4 {
                    if let Some(room) = new_level.rooms.get(1) {
                        let branch_coord = Coord::new_unchecked(room.x1 + 1, room.y1 + 1);
                        new_level.set_tile(
                            branch_coord,
                            Tile::BranchStairs {
                                branch: BranchId::Sokoban,
                                level: 1,
                                up: false,
                            },
                        );
                    }
                    if let Some(room) = new_level.rooms.get(2) {
                        let quest_coord = Coord::new_unchecked(room.x1 + 1, room.y1 + 1);
                        new_level.set_tile(
                            quest_coord,
                            Tile::BranchStairs {
                                branch: BranchId::Quest,
                                level: 1,
                                up: false,
                            },
                        );
                    }
                }
                if branch == BranchId::DungeonsOfDoom && depth == 5 {
                    if let Some(room) = new_level.rooms.first() {
                        let branch_coord = Coord::new_unchecked(room.x1 + 1, room.y1 + 1);
                        new_level.set_tile(
                            branch_coord,
                            Tile::BranchStairs {
                                branch: BranchId::Gehennom,
                                level: 1,
                                up: false,
                            },
                        );
                    }
                }

                geo.level = new_level;

                // C mklev.c fill_ordinary_room (973-975): each room gets a
                // random monster with probability 1/3 (always while carrying
                // the Amulet), chosen by makemon.c rndmonst() from the data.
                // The arrival room (first) and the last room stay empty here.
                let (item_ctx, ctx) = item_birth_context(a, h, geo, clk);
                let has_amulet = hero_has_amulet(&obj.items, h.player_id);
                let centers: Vec<Coord> = geo.level.rooms.iter().map(|r| r.center()).collect();
                for (i, &center) in centers.iter().enumerate() {
                    if i == 0 || i == centers.len() - 1 {
                        continue;
                    }
                    if !has_amulet && rng.random_range(0..3u32) != 0 {
                        continue;
                    }
                    // Released C mklev.c:975 explicitly sets MM_NOGRP.
                    // Selection and initialization share the public owned
                    // fixed-coordinate owner; groups stay suppressed.
                    spawn_random_monster_at_on_level(
                        a, obj, geo, h, clk, sess, config, rng, center, branch, depth,
                    );
                }

                // C mklev.c fill_ordinary_room (1157-1170): with probability
                // 1/3 a room gets a random object, then more while !rn2(5);
                // kinds come from mkobj() over the C class/prob data.
                // C mkobj.c:869: supported BUC/spe and weapon stack/poison
                // fields. Species/fuel/erosion remain gaps; containers and
                // existing-object artifacts are supported.
                let table = if branch == BranchId::Gehennom {
                    nethacked_data::generation::ObjectTable::Hell
                } else {
                    nethacked_data::generation::ObjectTable::Dungeon
                };
                let rooms: Vec<(usize, usize, usize, usize)> = geo
                    .level
                    .rooms
                    .iter()
                    .map(|r| (r.x1, r.y1, r.x2, r.y2))
                    .collect();
                for (x1, y1, x2, y2) in rooms {
                    if rng.random_range(0..3u32) != 0 {
                        continue;
                    }
                    loop {
                        let x = rng.random_range(x1..=x2);
                        let y = rng.random_range(y1..=y2);
                        register_existing_artifacts(obj, &config.ruleset);
                        let (item, keep_generating) = roll_room_object_with_artifacts(
                            &config.ruleset,
                            &geo.level,
                            Coord::new(x, y),
                            table,
                            item_ctx,
                            Some((&ctx, &h.genocide_registry)),
                            Some((&sess.role_name, &mut obj.artifact_history)),
                            |n| rng.random_range(0..n),
                        );
                        if let Some(item) = item {
                            let location = item.item.location.clone();
                            attach_generated_item(obj, &mut clk.object_timers, geo, item, location)
                                .expect("generated item deadline fits the world clock");
                        }
                        if !keep_generating {
                            break;
                        }
                    }
                }

                // At depth 5 in Dungeons of Doom, spawn the Amulet of Yendor
                if branch == BranchId::DungeonsOfDoom && depth == 5 {
                    if let Some(deepest_room) = geo.level.rooms.last() {
                        if let Some(amulet) = config.ruleset.create_item_record_by_id(
                            ItemKindId::AMULET_OF_YENDOR,
                            ItemLocation::Floor(deepest_room.center()),
                            Buc::Blessed,
                        ) {
                            spawn_item(obj, amulet);
                        }
                        events.push(GameEvent::LogMessage {
                            text: nethacked_i18n::Messages::amulet_cosmic_radiance(sess.locale)
                                .into(),
                        });
                    }
                }
            }
        }
    }

    // C do.c:1816-1823: catch up after delivery and reference restoration.
    events.extend(run_object_timers(
        clk, obj, a, h, eq, bill, geo, sess, config, rng,
    )?);
    Ok(events)
}

/// stairs.rs `handle_descend` over the owner facts.
#[allow(clippy::too_many_arguments)]
fn execute_descend(
    clk: &mut ClockFacts,
    obj: &mut ObjectsFacts,
    a: &mut ActorsFacts,
    h: &mut HeroFacts,
    eq: &mut EquipmentFacts,
    bill: &mut BillingFacts,
    geo: &mut GeographyFacts,
    sess: &SessionFacts,
    config: &OwnerBridgeConfig,
    rng: &mut rand_chacha::ChaCha8Rng,
) -> owners::Result<Vec<GameEvent>> {
    let mut events = Vec::new();
    let Some(player) = a.actors.get(h.player_id).cloned() else {
        return Ok(events);
    };

    // Gehennom bottom floor: Vibrating Square subterranean portal
    if geo.branch == BranchId::Gehennom
        && geo.depth == 5
        && geo.vibrating_square == Some(player.coord)
    {
        if nethacked_core::is_sanctum_accessible(h.ritual_progress) {
            let from_depth = geo.depth;
            pack_current_level(clk, obj, a, h, eq, bill, geo, config);
            geo.depth = 6;
            let gen_events = unpack_or_generate_level(
                clk,
                obj,
                a,
                h,
                eq,
                bill,
                geo,
                sess,
                config,
                rng,
                BranchId::Gehennom,
                6,
            )?;
            events.extend(gen_events);

            let new_coord = geo.level.stairs_up;
            if let Some(p) = a.actors.get_mut(h.player_id) {
                p.coord = new_coord;
            }
            place_steed_with_hero(a, h);

            events.push(GameEvent::LevelChanged {
                from_depth,
                to_depth: geo.depth,
            });
            events.push(GameEvent::LogMessage {
                text: nethacked_i18n::Messages::portal_moloch_sanctum(sess.locale).into(),
            });
            clk.scheduler.hero_act(NORMAL_SPEED);
            return Ok(events);
        } else {
            events.push(GameEvent::LogMessage {
                text: nethacked_i18n::Messages::portal_sealed_need_ritual(sess.locale).into(),
            });
            return Ok(events);
        }
    }

    let tile = geo.level.get_tile(player.coord).clone();
    match tile {
        Tile::Stairs { up: false } => {
            if geo.branch == BranchId::Quest && geo.depth == 1 {
                let p_lvl = a.actors.get(h.player_id).map(|p| p.level).unwrap_or(1);
                let hero_elig = nethacked_core::HeroQuestEligibility {
                    experience_level: p_lvl,
                    alignment_record: h.alignment_record,
                    is_hostile_to_leader: false,
                };
                let quest_cfg = nethacked_core::get_role_quest_config_or_default(&sess.role_name);
                if h.quest_state.progress == QuestProgress::Unassigned {
                    if nethacked_core::consult_leader(&mut h.quest_state, &hero_elig).is_err() {
                        events.push(GameEvent::LogMessage {
                            text: nethacked_i18n::Messages::quest_leader_reject_level(
                                quest_cfg.leader_name,
                                nethacked_core::QUEST_MIN_LEVEL,
                                sess.locale,
                            ),
                        });
                        return Ok(events);
                    } else {
                        events.push(GameEvent::LogMessage {
                            text: nethacked_i18n::Messages::quest_leader_accept(
                                quest_cfg.leader_name,
                                quest_cfg.artifact_name,
                                quest_cfg.nemesis_name,
                                sess.locale,
                            ),
                        });
                    }
                }
            }

            let from_depth = geo.depth;
            pack_current_level(clk, obj, a, h, eq, bill, geo, config);
            geo.depth += 1;
            let gen_events = unpack_or_generate_level(
                clk, obj, a, h, eq, bill, geo, sess, config, rng, geo.branch, geo.depth,
            )?;
            events.extend(gen_events);

            let new_coord = geo.level.stairs_up;
            if let Some(p) = a.actors.get_mut(h.player_id) {
                p.coord = new_coord;
            }
            place_steed_with_hero(a, h);

            events.push(GameEvent::LevelChanged {
                from_depth,
                to_depth: geo.depth,
            });
            events.push(GameEvent::LogMessage {
                text: nethacked_i18n::Messages::descend_deeper(geo.depth, sess.locale),
            });
            clk.scheduler.hero_act(NORMAL_SPEED);
        }
        Tile::BranchStairs {
            branch,
            level,
            up: false,
        } => {
            let from_depth = geo.depth;
            pack_current_level(clk, obj, a, h, eq, bill, geo, config);
            geo.branch = branch;
            geo.depth = level;
            let gen_events = unpack_or_generate_level(
                clk, obj, a, h, eq, bill, geo, sess, config, rng, branch, level,
            )?;
            events.extend(gen_events);

            let new_coord = geo.level.stairs_up;
            if let Some(p) = a.actors.get_mut(h.player_id) {
                p.coord = new_coord;
            }
            place_steed_with_hero(a, h);

            events.push(GameEvent::LevelChanged {
                from_depth,
                to_depth: geo.depth,
            });
            events.push(GameEvent::LogMessage {
                text: nethacked_i18n::Messages::enter_branch(
                    &format!("{branch:?}"),
                    level,
                    sess.locale,
                ),
            });
            clk.scheduler.hero_act(NORMAL_SPEED);
        }
        _ => {
            events.push(GameEvent::LogMessage {
                text: nethacked_i18n::Messages::no_stairs_down(sess.locale).into(),
            });
        }
    }

    Ok(events)
}

/// stairs.rs `handle_ascend` over the owner facts.
#[allow(clippy::too_many_arguments)]
fn execute_ascend(
    clk: &mut ClockFacts,
    obj: &mut ObjectsFacts,
    a: &mut ActorsFacts,
    h: &mut HeroFacts,
    eq: &mut EquipmentFacts,
    bill: &mut BillingFacts,
    geo: &mut GeographyFacts,
    sess: &SessionFacts,
    config: &OwnerBridgeConfig,
    rng: &mut rand_chacha::ChaCha8Rng,
) -> owners::Result<Vec<GameEvent>> {
    let mut events = Vec::new();
    let Some(player) = a.actors.get(h.player_id).cloned() else {
        return Ok(events);
    };

    let tile = geo.level.get_tile(player.coord).clone();
    match tile {
        Tile::Stairs { up: true } => {
            if geo.depth > 1 {
                // Gehennom Mysterious Force when ascending with the real
                // Amulet of Yendor
                let has_amulet = hero_has_amulet(&obj.items, h.player_id);
                if geo.branch == BranchId::Gehennom && has_amulet {
                    let hero_align = hero_alignment(a, h);
                    let t = rng.random::<u32>();
                    let da = rng.random::<u32>();
                    let b = rng.random::<u32>();
                    let c = rng.random::<u32>();
                    let mf_outcome = nethacked_core::mysterious_force(
                        geo.depth,
                        SANCTUM_DEPTH,
                        h.mysterious_force_count,
                        hero_align,
                        t,
                        da,
                        b,
                    );
                    if mf_outcome != nethacked_core::MysteriousForceOutcome::NoEffect {
                        h.mysterious_force_count = h.mysterious_force_count.saturating_add(
                            nethacked_core::mysterious_force_counter_increment(
                                geo.depth, mf_outcome, c,
                            ),
                        );
                        events.push(GameEvent::LogMessage {
                            text: nethacked_i18n::Messages::mysterious_force_surrounds(sess.locale)
                                .into(),
                        });
                    }
                    if mf_outcome == nethacked_core::MysteriousForceOutcome::SameLevelTeleport {
                        teleport_hero_randomly(a, h, geo, rng);
                        clk.scheduler.hero_act(NORMAL_SPEED);
                        return Ok(events);
                    }
                    let pushed = match mf_outcome {
                        nethacked_core::MysteriousForceOutcome::PushDown(p) => {
                            Some(clamp_mysterious_force(
                                p,
                                nethacked_core::is_sanctum_accessible(h.ritual_progress),
                            ))
                        }
                        _ => None,
                    };
                    if let Some(pushed_depth) = pushed.filter(|&p| p != geo.depth) {
                        let from_depth = geo.depth;
                        pack_current_level(clk, obj, a, h, eq, bill, geo, config);
                        geo.depth = pushed_depth;
                        let gen_events = unpack_or_generate_level(
                            clk,
                            obj,
                            a,
                            h,
                            eq,
                            bill,
                            geo,
                            sess,
                            config,
                            rng,
                            BranchId::Gehennom,
                            geo.depth,
                        )?;
                        events.extend(gen_events);

                        let new_coord = geo.level.stairs_up;
                        if let Some(p) = a.actors.get_mut(h.player_id) {
                            p.coord = new_coord;
                        }
                        place_steed_with_hero(a, h);

                        events.push(GameEvent::LevelChanged {
                            from_depth,
                            to_depth: geo.depth,
                        });
                        events.push(GameEvent::LogMessage {
                            text: nethacked_i18n::Messages::mysterious_force_pushes(
                                pushed_depth,
                                sess.locale,
                            ),
                        });
                        clk.scheduler.hero_act(NORMAL_SPEED);
                        return Ok(events);
                    }
                }

                let from_depth = geo.depth;
                pack_current_level(clk, obj, a, h, eq, bill, geo, config);
                geo.depth -= 1;
                let gen_events = unpack_or_generate_level(
                    clk, obj, a, h, eq, bill, geo, sess, config, rng, geo.branch, geo.depth,
                )?;
                events.extend(gen_events);

                let new_coord = geo.level.stairs_down;
                if let Some(p) = a.actors.get_mut(h.player_id) {
                    p.coord = new_coord;
                }
                place_steed_with_hero(a, h);

                events.push(GameEvent::LevelChanged {
                    from_depth,
                    to_depth: geo.depth,
                });
                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::ascend_to_level(geo.depth, sess.locale),
                });
                clk.scheduler.hero_act(NORMAL_SPEED);
            } else if geo.branch == BranchId::DungeonsOfDoom {
                // Surface check for Victory with Amulet of Yendor
                let has_amulet = hero_has_amulet(&obj.items, h.player_id);
                if has_amulet {
                    events.push(GameEvent::Victory);
                    events.push(GameEvent::LogMessage {
                        text: nethacked_i18n::Messages::victory_ascend_amulet(sess.locale).into(),
                    });
                } else {
                    events.push(GameEvent::LogMessage {
                        text: nethacked_i18n::Messages::celestial_force_bars_escape(sess.locale)
                            .into(),
                    });
                }
            } else {
                // Branch level 1 stairs up returning to parent branch
                if let Some(parent) = nethacked_core::branch::exit_branch(BranchCoord {
                    branch: geo.branch,
                    depth: geo.depth,
                }) {
                    let from_depth = geo.depth;
                    pack_current_level(clk, obj, a, h, eq, bill, geo, config);
                    geo.branch = parent.branch;
                    geo.depth = parent.depth;
                    let gen_events = unpack_or_generate_level(
                        clk,
                        obj,
                        a,
                        h,
                        eq,
                        bill,
                        geo,
                        sess,
                        config,
                        rng,
                        parent.branch,
                        parent.depth,
                    )?;
                    events.extend(gen_events);

                    let new_coord = geo.level.stairs_down;
                    if let Some(p) = a.actors.get_mut(h.player_id) {
                        p.coord = new_coord;
                    }
                    place_steed_with_hero(a, h);

                    events.push(GameEvent::LevelChanged {
                        from_depth,
                        to_depth: geo.depth,
                    });
                    events.push(GameEvent::LogMessage {
                        text: nethacked_i18n::Messages::return_to_branch_level(
                            &format!("{:?}", parent.branch),
                            parent.depth,
                            sess.locale,
                        ),
                    });
                    clk.scheduler.hero_act(NORMAL_SPEED);
                }
            }
        }
        Tile::BranchStairs {
            branch,
            level,
            up: true,
        } => {
            if geo.branch == BranchId::Quest && geo.depth == 1 {
                let quest_cfg = nethacked_core::get_role_quest_config_or_default(&sess.role_name);
                if h.quest_state.progress == QuestProgress::NemesisDefeated
                    && h.quest_state.artifact_location
                        == nethacked_core::ArtifactLocation::CarriedByHero
                {
                    nethacked_core::return_to_leader_with_artifact(&mut h.quest_state);
                    events.push(GameEvent::LogMessage {
                        text: nethacked_i18n::Messages::quest_completed(
                            quest_cfg.leader_name,
                            quest_cfg.artifact_name,
                            sess.locale,
                        ),
                    });
                }
            }

            let from_depth = geo.depth;
            pack_current_level(clk, obj, a, h, eq, bill, geo, config);
            geo.branch = branch;
            geo.depth = level;
            let gen_events = unpack_or_generate_level(
                clk, obj, a, h, eq, bill, geo, sess, config, rng, branch, level,
            )?;
            events.extend(gen_events);

            let new_coord = geo.level.stairs_down;
            if let Some(p) = a.actors.get_mut(h.player_id) {
                p.coord = new_coord;
            }
            place_steed_with_hero(a, h);

            events.push(GameEvent::LevelChanged {
                from_depth,
                to_depth: geo.depth,
            });
            events.push(GameEvent::LogMessage {
                text: nethacked_i18n::Messages::return_to_branch_level(
                    &format!("{branch:?}"),
                    level,
                    sess.locale,
                ),
            });
            clk.scheduler.hero_act(NORMAL_SPEED);
        }
        _ => {
            events.push(GameEvent::LogMessage {
                text: nethacked_i18n::Messages::no_stairs_up(sess.locale).into(),
            });
        }
    }

    Ok(events)
}

/// wand_legacy.rs `LegacyActors` over the owner facts: the borrowed
/// compatibility adapter for the shared wand creation slice. No adapter
/// retains the legacy host.
struct TraversalActors<'a> {
    actors: &'a mut ActorStorage,
    genocide: &'a GenocideRegistry,
}
impl crate::actions::wand::ports::Actors for TraversalActors<'_> {
    fn actor(&self, id: ActorId) -> Option<ActorRecord> {
        self.actors.get(id).cloned()
    }
    fn at(&self, coord: Coord) -> Option<ActorId> {
        self.actors
            .iter()
            .find_map(|(id, actor)| (actor.coord == coord && !actor.is_dead).then_some(id))
    }
    fn damage(&mut self, id: ActorId, damage: u32) -> Option<bool> {
        let actor = self.actors.get_mut(id)?;
        actor.hp = actor.hp.saturating_sub(damage);
        if actor.hp == 0 {
            actor.is_dead = true;
        }
        Some(actor.is_dead)
    }
    fn polymorph(&mut self, id: ActorId, def: &MonsterDef) {
        if let Some(actor) = self.actors.get_mut(id) {
            actor.species = Some(def.actor_species());
            actor.name = def.name.clone();
            actor.hp = def.base_hp;
            actor.max_hp = def.max_hp;
            actor.ac = def.ac;
            actor.speed = def.speed;
            actor.level = def.level;
        }
    }
    fn make_hostile(&mut self, id: ActorId) {
        if let Some(actor) = self.actors.get_mut(id) {
            actor.is_peaceful = false;
        }
    }
    fn guardians(&self, name: &str, include_sound: bool, ruleset: &crate::Ruleset) -> Vec<ActorId> {
        self.actors
            .iter()
            .filter(|(_, actor)| {
                !actor.is_dead
                    && (actor.name.eq_ignore_ascii_case(name)
                        || (include_sound
                            && ruleset
                                .monster_for_actor(actor)
                                .is_some_and(|def| def.msound == MonsterSound::Guardian)))
            })
            .map(|(id, _)| id)
            .collect()
    }
    fn spawn_initialized(&mut self, actor: ActorRecord) -> ActorId {
        // Preserve the preexisting restored SlotMap freelist normalization.
        *self.actors = serde_json::from_value(
            serde_json::to_value(&self.actors).expect("actor arena serializes"),
        )
        .expect("serialized actor arena restores");
        self.actors.insert(actor)
    }
    fn force_hostile_malign(&mut self, id: ActorId, malign: i32) {
        if let Some(actor) = self.actors.get_mut(id) {
            actor.is_peaceful = false;
            actor.malign = malign;
        }
    }
    fn gain_level(&mut self, id: ActorId, hp: u32, level: u32) {
        if let Some(actor) = self.actors.get_mut(id) {
            actor.max_hp = actor.max_hp.saturating_add(hp);
            actor.hp = actor.hp.saturating_add(hp);
            actor.level = level;
        }
    }
    fn genocided(&self, name: &str, glyph: char) -> bool {
        nethacked_core::genocide::is_genocided(self.genocide, name, glyph)
    }
}

/// wand_legacy.rs `CreationObjects` over the owner facts.
struct TraversalObjects<'a>(&'a ItemStorage);
impl crate::actions::wand::ports::ObjectFacts for TraversalObjects<'_> {
    fn carried(&self, actor: ActorId) -> Vec<ItemId> {
        self.0
            .iter()
            .filter_map(|(id, item)| {
                (item.location == ItemLocation::CarriedBy(actor)).then_some(id)
            })
            .collect()
    }
    fn object(&self, id: ItemId) -> Option<SelectedObject> {
        self.0.get(id).map(|item| SelectedObject {
            class: item.class,
            name: item.name.clone(),
            definition: item.definition.clone(),
            buc: item.buc,
            enchantment: item.enchantment,
        })
    }
    fn floor_kind(&self, coord: Coord, kind: ItemKindId, ruleset: &crate::Ruleset) -> bool {
        self.0.iter().any(|(_, item)| {
            item.location == ItemLocation::Floor(coord)
                && ruleset
                    .item(&item.name)
                    .is_some_and(|def| def.id == Some(kind))
        })
    }
    fn has_amulet(&self, actor: ActorId) -> bool {
        self.carried(actor).into_iter().any(|id| {
            self.0
                .get(id)
                .is_some_and(crate::actions::items::is_real_amulet)
        })
    }
}

/// wand_legacy.rs `CreationGeography` over the owner facts.
struct TraversalGeography<'a>(&'a DungeonLevel);
impl crate::actions::wand::ports::GeographyFacts for TraversalGeography<'_> {
    fn tile(&self, coord: Coord) -> Tile {
        self.0.get_tile(coord).clone()
    }
    fn trajectory(&self, origin: Coord, dir: Direction, energy: u32) -> Vec<Coord> {
        nethacked_dungeon::trace_beam_path(self.0, origin, dir, energy)
    }
    fn engraving(&self, coord: Coord) -> Option<Engraving> {
        self.0.get_engraving(coord).cloned()
    }
}

/// wand_legacy.rs `CreationClock` over the owner facts.
struct TraversalClock<'a>(&'a SchedulerState);
impl crate::actions::wand::ports::WorldClock for TraversalClock<'_> {
    fn turn(&self) -> u64 {
        self.0.turn
    }
}

/// wand_legacy.rs `CreationHero` over the owner facts.
struct TraversalHero<'a> {
    role_id: Option<RoleId>,
    role_name: &'a str,
    race: RaceId,
    alignment: i32,
}
impl crate::actions::wand::ports::HeroFacts for TraversalHero<'_> {
    fn role_id(&self) -> Option<RoleId> {
        self.role_id
    }
    fn role_name(&self) -> &str {
        self.role_name
    }
    fn race(&self) -> RaceId {
        self.race
    }
    fn alignment_record(&self) -> i32 {
        self.alignment
    }
}

/// monster_generation.rs `spawn_random_monster_at_on_level` over the owner
/// facts: ordinary fixed-coordinate random generation delegates to the shared
/// wand slice.
#[allow(clippy::too_many_arguments)]
fn spawn_random_monster_at_on_level(
    a: &mut ActorsFacts,
    obj: &ObjectsFacts,
    geo: &GeographyFacts,
    h: &HeroFacts,
    clk: &ClockFacts,
    sess: &SessionFacts,
    config: &OwnerBridgeConfig,
    rng: &mut rand_chacha::ChaCha8Rng,
    coord: Coord,
    branch: BranchId,
    depth: usize,
) -> Option<ActorId> {
    let session = crate::actions::wand::ports::Session {
        ruleset: &config.ruleset,
        locale: sess.locale,
        branch: geo.branch,
        depth: geo.depth,
    };
    let objects = TraversalObjects(&obj.items);
    let mut actors = TraversalActors {
        actors: &mut a.actors,
        genocide: &h.genocide_registry,
    };
    let geography = TraversalGeography(&geo.level);
    let clock = TraversalClock(&clk.scheduler);
    let hero = TraversalHero {
        role_id: sess.role_id,
        role_name: &sess.role_name,
        race: h.hero_race,
        alignment: h.alignment_record,
    };
    crate::actions::wand::spawn_at(
        coord,
        branch,
        depth,
        h.player_id,
        &objects,
        &mut actors,
        &geography,
        &hero,
        &clock,
        rng,
        &session,
    )
}

pub(crate) fn outcome(
    action: &TraversalAction,
    obs: &[Observation],
    config: &OwnerBridgeConfig,
    rng: &mut rand_chacha::ChaCha8Rng,
) -> owners::Result<TraversalOutcome> {
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
        return Ok(TraversalOutcome::empty());
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

    // 2. Execute the traversal action
    let mut events = Vec::new();

    // C multi < 0 (fall_asleep, paralysis): the hero cannot act this turn;
    // the action degenerates to a stationary wait (actions/mod.rs).
    let spent_time = if h.hero.afflictions.transient.helpless > 0 || eq.equipment.dressing.is_some()
    {
        clk.scheduler.hero_act(NORMAL_SPEED);
        clk.scheduler.hero_energy != energy_before
    } else {
        match action {
            TraversalAction::Ascend => {
                let ascend_events = execute_ascend(
                    &mut clk, &mut obj, &mut a, &mut h, &mut eq, &mut bill, &mut geo, &sess,
                    config, rng,
                )?;
                events.extend(ascend_events);
                clk.scheduler.hero_energy != energy_before
            }
            TraversalAction::Descend => {
                let descend_events = execute_descend(
                    &mut clk, &mut obj, &mut a, &mut h, &mut eq, &mut bill, &mut geo, &sess,
                    config, rng,
                )?;
                events.extend(descend_events);
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

    // 4. Turn-stepping loop. Traversal actions never walk, so the legacy
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

            events.extend(run_object_timers(
                &mut clk, &mut obj, &mut a, &mut h, &mut eq, &mut bill, &geo, &sess, config, rng,
            )?);

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

    Ok(TraversalOutcome {
        spent_time,
        rounds,
        clock: Some(ClockOutcome {
            scheduler: clk.scheduler,
            object_timers: clk.object_timers,
        }),
        actors: Some(ActorsOutcome {
            actors: a.actors,
            stored_monsters: a.stored_monsters,
        }),
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
            mysterious_force_count: h.mysterious_force_count,
        }),
        objects: Some(ObjectsOutcome {
            items: obj.items,
            artifact_history: obj.artifact_history,
            stored_items: obj.stored_items,
        }),
        equipment: Some(EquipmentOutcome {
            equipment: eq.equipment,
            wielded_item: eq.wielded_item,
        }),
        billing: Some(BillingOutcome {
            unpaid_items: bill.unpaid_items,
            billing: bill.billing,
            player_gold: bill.player_gold,
            stored_unpaid_items: bill.stored_unpaid_items,
        }),
        geography: Some(GeographyOutcome {
            level: geo.level,
            branch: geo.branch,
            depth: geo.depth,
            vibrating_square: geo.vibrating_square,
            stored_keys: geo.stored_keys,
            stored_levels: geo.stored_levels,
        }),
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

struct TraversalCodec<T>(OwnerBridgeConfig, std::marker::PhantomData<T>);
impl<T: Domain> Schema<T> for TraversalCodec<T> {
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

impl<T: Domain> NativeBehavior<T> for TraversalCodec<T> {
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
    result: &TraversalOutcome,
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

fn preview(obs: &[Observation]) -> owners::Result<TraversalOutcome> {
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
            next.stored_items = o.stored_items.clone();
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
                stored_monsters: self.stored_monsters.clone(),
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
            next.stored_monsters = a.stored_monsters.clone();
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
                mysterious_force_count: self.mysterious_force_count,
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
            next.mysterious_force_count = h.mysterious_force_count;
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
            next.stored_unpaid_items = b.stored_unpaid_items.clone();
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
        let action: TraversalAction = decode(request, PAYLOAD)?;
        encode(&outcome(&action, obs, c, &mut self.rng.clone())?, PAYLOAD)
    }
    fn prepare(
        &self,
        request: &[u8],
        obs: &[Observation],
        c: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        let action: TraversalAction = decode(request, PAYLOAD)?;
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
                stored_levels: self.stored_levels.clone(),
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
            next.level = g.level.clone();
            next.current_branch = g.branch;
            next.depth = g.depth;
            next.vibrating_square = g.vibrating_square;
            next.stored_keys = g.stored_keys.clone();
            next.stored_levels = g.stored_levels.clone();
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

pub(super) struct TraversalAccounting {
    pub(super) config: OwnerBridgeConfig,
    pub(super) action: TraversalAction,
}

impl Accounting for TraversalAccounting {
    fn validate(&self, p: &[ProposalView], obs: &[Observation]) -> owners::Result<()> {
        if obs.len() != 9 {
            return Err(owners::Error::Schema);
        }
        let random = p
            .iter()
            .find(|x| x.receipt.command == WRITES[7])
            .ok_or(owners::Error::Invariant)?;
        let before: RandomState = decode(&random.before, PAYLOAD)?;
        let result: TraversalOutcome = decode(&random.reply, PAYLOAD)?;
        let mut expected = before.clone();
        let expected_outcome = outcome(&self.action, obs, &self.config, &mut expected.rng)?;
        if encode(&expected, PAYLOAD)? != random.proposed
            || encode(&expected_outcome, PAYLOAD)? != random.reply
            || encode(&expected_outcome, PAYLOAD)?
                != fact::<TraversalOutcome>(obs, 6).and_then(|r| encode(&r, PAYLOAD))?
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
                "trav_clock_prepare" => check!(ClockState),
                "trav_objects_prepare" => check!(ObjectsState),
                "trav_actors_prepare" => check!(ActorsState),
                "trav_hero_prepare" => check!(HeroState),
                "trav_equipment_prepare" => check!(EquipmentState),
                "trav_billing_prepare" => check!(BillingState),
                "trav_geography_prepare" => check!(GeographyState),
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
    artifact("native-traversal-accounting-i9", 30)
}

pub(super) fn capture(
    a: &mut op::AccountingCaptureCatalog,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if c.traversal {
        a.register(
            identity(),
            POLICY,
            Rc::new(TraversalAccounting {
                config: c.clone(),
                action: TraversalAction::Descend,
            }),
        )?;
    }
    Ok(())
}

pub(super) fn restore(
    a: &mut op::AccountingRestoreCatalog,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if c.traversal {
        let cc = c.clone();
        a.allow(identity(), POLICY, move || {
            Ok(Rc::new(TraversalAccounting {
                config: cc.clone(),
                action: TraversalAction::Descend,
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

/// Traversal carries full level residency (stored levels, items, monsters)
/// through observations, the prepare intent and the Random candidate's
/// outcome reply. The 8 MiB endpoint bounds sit above the 4 MiB `PAYLOAD`
/// encode cap so the transport never binds first.
fn live_endpoint(access: Access) -> Endpoint {
    Endpoint {
        major: 1,
        access,
        max_request: 8 * 1024 * 1024,
        max_reply: 8 * 1024 * 1024,
    }
}

fn scoped(access: Access, resources: Vec<Resource>) -> Authority {
    Authority {
        access,
        scope: Scope::Keys(resources.into_iter().collect()),
    }
}

pub(super) struct TraversalTransport {
    kernel: Kernel,
    caller: ModuleHandle,
    routes: Rc<Routes>,
    resources: Vec<Resource>,
}

impl TraversalTransport {
    pub(super) fn install(
        parts: NativePartitions,
        config: OwnerBridgeConfig,
    ) -> Result<Self, OwnerBridgeError> {
        let host = Host::new(owners::Limits {
            payload_bytes: PAYLOAD,
            observation_bytes: 3 * PAYLOAD,
            reply_bytes: 8 * 1024 * 1024,
            ..Default::default()
        })?;
        host.configure_publication(owners::PublicationLimits {
            reply_bytes: 8 * 1024 * 1024,
            retained_bytes: 32 * PAYLOAD,
            ..publication_limits()
        })?;
        let mut kernel = Kernel::new(kernel::Limits {
            payload_bytes: 8 * 1024 * 1024,
            ..Default::default()
        })?;
        let routes = Rc::new(Routes::Initial(RefCell::new(None)));
        let r = routes.clone();
        let c = config.clone();
        let r_preview = routes.clone();
        let c_preview = config.clone();

        let preview_port: NativePort = Rc::new(move |bytes, view| {
            let action: TraversalAction = decode(bytes, PAYLOAD).map_err(native_refusal)?;
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
            encode(&TraversalRequest { action, guards }, PAYLOAD).map_err(native_refusal)
        });

        let run_port: NativePort = Rc::new(move |bytes, view| {
            let req: TraversalRequest = decode(bytes, PAYLOAD).map_err(native_refusal)?;
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
                    Rc::new(TraversalAccounting {
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
                &TraversalReply {
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
            kernel.register_native(manifest(HOME, exports.clone(), vec![]), &[78; 32], ports)?;
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
            &[79; 32],
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
                        schema: [index as u8 + 240; 32],
                        ports,
                    },
                    $state,
                    Backend::Native(Rc::new(TraversalCodec::<$ty>(
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
        action: &TraversalAction,
    ) -> Result<TraversalRequest, OwnerBridgeError> {
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
        request: TraversalRequest,
    ) -> Result<(TraversalReply, NativePartitions), OwnerBridgeError> {
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
        let outcome: TraversalReply = decode(&reply?, PAYLOAD)?;

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
    fn traversal_session_witnesses(&self) -> Result<Vec<wand::OwnerWitness>, OwnerBridgeError> {
        self.session.with_runtime(&self.credential.borrow(), |b| {
            Ok(bridge_stamps(b)?
                .iter()
                .map(wand::OwnerWitness::from)
                .collect())
        })?
    }

    pub fn prepare_traversal(
        &self,
        action: TraversalAction,
    ) -> Result<TraversalRequest, OwnerBridgeError> {
        if !self.config.traversal {
            return Err(owners::Error::Unsupported.into());
        }
        let parts = self.partitions()?;
        let mut transport = TraversalTransport::install(parts, self.config.clone())?;
        let _ = transport.preview(&action)?;
        let guards = self.traversal_session_witnesses()?;
        Ok(TraversalRequest { action, guards })
    }

    pub fn execute_traversal(
        &self,
        request: TraversalRequest,
    ) -> Result<TraversalReply, OwnerBridgeError> {
        if !self.config.traversal {
            return Err(owners::Error::Unsupported.into());
        }
        let current_witnesses = self.traversal_session_witnesses()?;
        if request.guards != current_witnesses {
            #[cfg(test)]
            STALE_REFUSALS.with(|x| x.set(x.get() + 1));
            return Err(owners::Error::Stale.into());
        }
        let parts = self.partitions()?;
        let mut transport = TraversalTransport::install(parts, self.config.clone())?;
        let (reply, updated) = transport.execute(request)?;
        let source = Self::install(updated, self.config.clone())?;
        let wire = source.capture()?;
        drop(source);
        self.restore(&wire)?;
        Ok(reply)
    }

    pub fn step_traversal(
        &self,
        action: TraversalAction,
    ) -> Result<TraversalReply, OwnerBridgeError> {
        let req = self.prepare_traversal(action)?;
        self.execute_traversal(req)
    }
}
