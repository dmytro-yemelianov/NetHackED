//! Explicit M6-I6 inventory-domain dispatch: PickUp, Drop, Wield, Wear/Remove
//! suit dressing, container Put/Take, Quiver and Fire.
use super::*;
use crate::actions::command_frame::{self, BorrowedObjects};
use crate::actions::equipment::Dressing;
use crate::passive_afflictions;
use crate::passive_dressing;
use crate::peace::{is_shopkeeper, is_temple_priest, monnam_article};
use crate::turns::natural_hp_heal;
use crate::GameEvent;
use nethacked_arena::{
    calculate_total_weight, carried_weight, frozen_in_ice_box, ActorId, ActorRecord, ActorStorage,
    ItemId, ItemLocation, ItemStorage,
};
use nethacked_core::ac::ArmorSlot;
use nethacked_core::energy::{hero_movement_gain, monster_movement_gain, NORMAL_SPEED};
use nethacked_core::engraving::{is_elbereth_ward_active, onscary_exempt, Engraving};
use nethacked_core::inventory::EncumbranceTier;
use nethacked_core::ranged::resolve_projectile_impact;
use nethacked_core::{BagCheckItem, BagCheckKind};
use nethacked_dungeon::DungeonLevel;
use nethacked_kernel::ModuleHandle;
use nethacked_types::{
    BranchId, Buc, Coord, Direction, ItemClass, Locale, SlotId, Tile, TrapRecord,
};
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use wand::OwnerWitness;

pub const HOME: &str = "inventory-home-i6-v1";
pub const CALLER: &str = "inventory-caller-i6-v1";
pub const PREVIEW: &str = "inventory_preview_i6";
pub const RUN: &str = "inventory_run_i6";

pub const READS: [&str; 9] = [
    "inventory_clock_read",
    "inventory_objects_read",
    "inventory_actors_read",
    "inventory_hero_read",
    "inventory_equipment_read",
    "inventory_billing_read",
    "inventory_random_read",
    "inventory_geography_read",
    "inventory_session_read",
];

pub const WRITES: [&str; 7] = [
    "inventory_clock_prepare",
    "inventory_objects_prepare",
    "inventory_actors_prepare",
    "inventory_hero_prepare",
    "inventory_equipment_prepare",
    "inventory_geography_prepare",
    "inventory_random_prepare",
];

pub const POLICY: [u8; 32] = [69; 32];

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
pub enum InventoryAction {
    PickUp,
    Drop(usize),
    Wield(usize),
    Wear(usize),
    Remove(usize),
    PutInContainer {
        item_index: usize,
        container_index: usize,
    },
    TakeFromContainer {
        container_index: usize,
        item_index: usize,
    },
    Quiver(SlotId),
    Fire(Direction),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InventoryRequest {
    pub action: InventoryAction,
    pub guards: Vec<OwnerWitness>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InventoryReply {
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
    pub artifact_history: BTreeSet<String>,
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
}

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
    pub player_luck: i32,
    pub prayer_timeout: u32,
    pub alignment_record: i32,
    pub quest_state: nethacked_core::QuestState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquipmentOutcome {
    pub equipment: crate::actions::equipment::EquipmentState,
    pub wielded_item: Option<ItemId>,
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
pub struct InventoryOutcome {
    pub spent_time: bool,
    pub rounds: u32,
    pub clock: Option<ClockOutcome>,
    pub actors: Option<ActorsOutcome>,
    pub hero: Option<HeroOutcome>,
    pub objects: Option<ObjectsOutcome>,
    pub equipment: Option<EquipmentOutcome>,
    pub geography: Option<GeographyOutcome>,
    pub events: Vec<GameEvent>,
}

impl InventoryOutcome {
    pub fn empty() -> Self {
        Self {
            spent_time: false,
            rounds: 0,
            clock: None,
            actors: None,
            hero: None,
            objects: None,
            equipment: None,
            geography: None,
            events: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PrepareV1 {
    pub(super) outcome: InventoryOutcome,
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

/// Ordinary-form capacity from stored attributes and live levitation
/// (inventory.rs `hero_weight_capacity`).
fn hero_weight_capacity(h: &HeroFacts, levitating: bool) -> u32 {
    nethacked_core::inventory::weight_cap(
        nethacked_core::inventory::capacity_strength(h.progress.attr_base[crate::progress::A_STR]),
        h.progress.attr_base[crate::progress::A_CON].clamp(3, 25),
        levitating,
        0,
    )
}

/// C `carry_count` prospective check (inventory.rs `can_lift_weight`).
fn can_lift_weight(h: &HeroFacts, levitating: bool, weight: u32) -> bool {
    weight < hero_weight_capacity(h, levitating).saturating_mul(3)
}

fn items_at_floor(items: &ItemStorage, coord: Coord) -> Vec<ItemId> {
    items
        .iter()
        .filter_map(|(id, item)| {
            if item.location == ItemLocation::Floor(coord) {
                Some(id)
            } else {
                None
            }
        })
        .collect()
}

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

/// Arena `destroy_item`: removal plus artifact-history registration.
fn destroy_item(obj: &mut ObjectsFacts, id: ItemId) {
    if let Some(item) = obj.items.remove(id) {
        if let Some(artifact) = &item.artifact_id {
            obj.artifact_history.insert(artifact.clone());
        }
    }
}

/// Destroys an item and everything inside it (inventory.rs
/// `destroy_item_tree`).
fn destroy_item_tree(obj: &mut ObjectsFacts, id: ItemId) {
    for c in items_in_container(&obj.items, id) {
        destroy_item_tree(obj, c);
    }
    destroy_item(obj, id);
}

/// Builds the C `mbag_explodes` view of an arena item and its recursive
/// contents. Kind is detected by name; charges are `enchantment` (`obj->spe`).
fn bag_check_tree(items: &ItemStorage, id: ItemId) -> BagCheckItem {
    let rec = items.get(id);
    let kind = match rec {
        Some(r) if r.is_bag_of_holding => BagCheckKind::BagOfHolding,
        Some(r) if r.name.contains("wand of cancellation") => BagCheckKind::WandOfCancellation {
            charges: i32::from(r.enchantment),
        },
        Some(r) if r.name.contains("bag of tricks") => BagCheckKind::BagOfTricks {
            charges: i32::from(r.enchantment),
        },
        _ => BagCheckKind::Other,
    };
    BagCheckItem {
        kind,
        children: items_in_container(items, id)
            .into_iter()
            .map(|c| bag_check_tree(items, c))
            .collect(),
    }
}

/// C `do_boh_explosion` (`pickup.c:2517-2532`): each content item is destroyed
/// with probability 1/13, otherwise scattered at the hero's square.
fn do_boh_explosion(
    obj: &mut ObjectsFacts,
    actors: &ActorStorage,
    player_id: ActorId,
    bag: ItemId,
    rng: &mut rand_chacha::ChaCha8Rng,
) {
    let hero_coord = actors.get(player_id).map(|p| p.coord);
    for content in items_in_container(&obj.items, bag) {
        if rng.random_range(0..13u32) != 0 {
            if let (Some(c), Some(rec)) = (hero_coord, obj.items.get_mut(content)) {
                rec.location = ItemLocation::Floor(c);
                continue;
            }
        }
        destroy_item_tree(obj, content);
    }
}

/// C pickup.c:2644–2663,2758–2797 acts on the selected object only
/// (object_timers.rs `corpse_freeze_snapshot`).
fn corpse_freeze_snapshot(items: &ItemStorage, root: ItemId) -> Vec<(ItemId, bool)> {
    let Some(item) = items.get(root).filter(|item| item.is_corpse()) else {
        return Vec::new();
    };
    vec![(root, frozen_in_ice_box(item, |parent| items.get(parent)))]
}

/// Port of object_timers.rs `apply_corpse_freeze_transitions`: only the
/// selected corpse's successful direct-parent transition. The legacy
/// `start_object_timer_at` reconcile pass is a no-op for queues whose targets
/// all exist (the export-time snapshot validation); the fallible tail maps the
/// legacy `expect` onto an invariant error instead of panicking across the
/// kernel boundary.
#[allow(clippy::too_many_arguments)]
fn apply_corpse_freeze_transitions(
    obj: &mut ObjectsFacts,
    actors: &ActorStorage,
    clk: &mut ClockFacts,
    player_id: ActorId,
    ruleset: &crate::Ruleset,
    before: Vec<(ItemId, bool)>,
    rng: &mut rand_chacha::ChaCha8Rng,
) -> owners::Result<()> {
    use nethacked_data::corpses::{CorpseDefinition as Definition, CorpseTimeoutRecipe as Recipe};
    use nethacked_data::monsters::CorpseThawPolicy;
    for (id, was_frozen) in before {
        let Some(item) = obj.items.get(id).cloned() else {
            continue;
        };
        let frozen = frozen_in_ice_box(&item, |parent| obj.items.get(parent));
        if frozen == was_frozen {
            continue;
        }
        let target = crate::object_timers::ObjectTimerTarget::Active(id);
        if frozen {
            clk.object_timers.cancel(target);
            // C pickup.c:2648–2663 tests BODY Ice Troll, not saved species.
            if matches!(ruleset.corpse_body_definition(&item), Definition::Owned(def)
                if def.corpse_thaw_policy == CorpseThawPolicy::IceTroll)
            {
                if let Some(traits) = obj
                    .items
                    .get_mut(id)
                    .and_then(|i| i.saved_monster_traits.as_mut())
                {
                    traits.actor.is_cancelled = false;
                }
            }
            continue;
        }
        let no_revive = match ruleset.corpse_revival_definition(&item) {
            Definition::Owned(def) => def.corpse_thaw_policy == CorpseThawPolicy::Ordinary,
            Definition::OrdinaryUnspecified => true,
            // Explicitly unsupported original keys: no body/name guess.
            Definition::Unresolved => continue,
        };
        let body = ruleset.corpse_body_definition(&item);
        if matches!(body, Definition::Unresolved) {
            continue;
        }
        if let Some(i) = obj.items.get_mut(id) {
            i.corpse_no_revive = no_revive;
        }
        clk.object_timers.cancel(target);
        let hero_level = actors.get(player_id).map_or(1, |p| p.level);
        let ctx = nethacked_core::corpse_timeout::CorpseTimeoutContext {
            elapsed_age: item.corpse_age,
            moves: clk.turn,
            hero_level,
            in_mklev: false,
        };
        let recipe = match body {
            Definition::Owned(def) => {
                nethacked_data::corpses::sample_corpse_timeout(def, ctx, |n| rng.random_range(0..n))
            }
            Definition::OrdinaryUnspecified => Some(Recipe::Rot {
                sampled_at: ctx.moves,
                delay: nethacked_core::corpse_timeout::initial_rot_delay(ctx, |n| {
                    rng.random_range(0..n)
                }),
            }),
            Definition::Unresolved => unreachable!("checked supplied body key above"),
        };
        let (kind, sampled_at, delay) = match recipe {
            Some(Recipe::Rot { sampled_at, delay }) => (
                crate::object_timers::ObjectTimerKind::RotCorpse,
                sampled_at,
                delay,
            ),
            Some(Recipe::Revive { sampled_at, delay }) => (
                crate::object_timers::ObjectTimerKind::ReviveMonster,
                sampled_at,
                delay,
            ),
            Some(Recipe::UnsupportedRider) | None => continue,
        };
        clk.object_timers
            .start_at_preserving(target, kind, sampled_at.checked_add_signed(delay), true)
            .map_err(|_| owners::Error::Invariant)?;
    }
    Ok(())
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
        .map(|(_, base)| {
            nethacked_core::buy_price(
                *base,
                crate::actions::economy::DEFAULT_CHARISMA,
                hero_dunce_or_tourist(items, player_id, role_name, ulevel),
                false,
                false,
                false,
            )
        })
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
/// after the hero's projectile attack on a surviving monster.
#[allow(clippy::too_many_arguments)]
fn setmangry(
    actors: &mut ActorStorage,
    h: &mut HeroFacts,
    geo: &mut GeographyFacts,
    geo_delta: &mut GeographyDelta,
    target_id: ActorId,
    turn: u64,
    locale: Locale,
    ruleset: &crate::Ruleset,
    role_name: &str,
    rng: &mut rand_chacha::ChaCha8Rng,
    events: &mut Vec<GameEvent>,
) {
    let Some(target) = actors.get(target_id).cloned() else {
        return;
    };
    let Some(player) = actors.get(h.player_id).cloned() else {
        return;
    };
    let lim = nethacked_core::alignlim(turn);
    let hero_square: Coord = player.coord;
    let engraving = geo
        .level
        .get_engraving(hero_square)
        .filter(|e| e.text == "Elbereth")
        .cloned();
    if let Some(e) = engraving.as_ref() {
        if elbereth_scares(ruleset, &target, Some(e)) || target.is_peaceful {
            events.push(GameEvent::LogMessage {
                text: nethacked_i18n::Messages::feel_hypocrite(locale).into(),
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
                    text: nethacked_i18n::Messages::engraving_fades(locale).into(),
                });
            }
            geo.level.remove_engraving(hero_square);
            geo_delta.engravings.push((hero_square, None));
        }
    }

    if !target.is_peaceful || target.is_tame {
        return;
    }
    if let Some(t) = actors.get_mut(target_id) {
        t.is_peaceful = false;
    }
    let delta = if is_temple_priest(ruleset, &target) {
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
            monnam_article(ruleset, &target.name),
            locale,
        ),
    });

    let quest_cfg = nethacked_core::get_role_quest_config_or_default(role_name);
    if target.name.eq_ignore_ascii_case(quest_cfg.leader_name) {
        let mut got_mad = 0u32;
        for (_, mon) in actors.iter_mut() {
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
                    locale,
                ),
            });
        }
    }
}

fn execute_pickup(
    clk: &mut ClockFacts,
    obj: &mut ObjectsFacts,
    a: &ActorsFacts,
    h: &mut HeroFacts,
    bill: &BillingFacts,
    sess: &SessionFacts,
) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let Some(player) = a.actors.get(h.player_id).cloned() else {
        return events;
    };

    let floor_items = items_at_floor(&obj.items, player.coord);
    let chosen_item = floor_items.iter().copied().max_by_key(|&iid| {
        if let Some(item) = obj.items.get(iid) {
            if item.name.contains("Amulet")
                || item.name.starts_with("The ")
                || item.name.contains("Orb")
            {
                100
            } else if item.name == "corpse" {
                1
            } else {
                10
            }
        } else {
            0
        }
    });

    if let Some(item_id) = chosen_item {
        let weight = carried_weight(&obj.items, h.player_id)
            .saturating_add(calculate_total_weight(&obj.items, item_id));
        if !can_lift_weight(h, player.intrinsics.levitation, weight) {
            events.push(GameEvent::LogMessage {
                text: "You are carrying too much to pick that up.".into(),
            });
            clk.hero_energy = clk.hero_energy.saturating_sub(NORMAL_SPEED);
            return events;
        }
        if let Some(item) = obj.items.get_mut(item_id) {
            item.location = ItemLocation::CarriedBy(h.player_id);
            let name = item.name.clone();
            events.push(GameEvent::ItemPickedUp {
                actor: h.player_id,
                item: item_id,
            });
            events.push(GameEvent::LogMessage {
                text: nethacked_i18n::Messages::pickup_item(&name, sess.locale),
            });

            let quest_cfg = nethacked_core::get_role_quest_config_or_default(&sess.role_name);
            if name.eq_ignore_ascii_case(quest_cfg.artifact_name) {
                nethacked_core::pick_up_quest_artifact(&mut h.quest_state);
                events.push(GameEvent::LogMessage {
                    text: format!("A surge of celestial energy surges through your veins as you take hold of {name}!"),
                });
            }

            if let Some(cost) = get_unpaid_cost(
                bill,
                &obj.items,
                h.player_id,
                item_id,
                &sess.role_name,
                player.level,
            ) {
                events.push(GameEvent::LogMessage {
                    text: format!("The shopkeeper says: 'That will be {cost} zorkmids.'"),
                });
            }
            clk.hero_energy = clk.hero_energy.saturating_sub(NORMAL_SPEED);
        }
    } else {
        events.push(GameEvent::LogMessage {
            text: nethacked_i18n::Messages::nothing_to_pickup(sess.locale).into(),
        });
    }

    events
}

fn execute_drop(
    clk: &mut ClockFacts,
    obj: &mut ObjectsFacts,
    a: &ActorsFacts,
    h: &HeroFacts,
    eq: &mut EquipmentFacts,
    idx: usize,
) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let Some(player) = a.actors.get(h.player_id).cloned() else {
        return events;
    };

    let carried = items_carried_by(&obj.items, h.player_id);
    if idx < carried.len() {
        let item_id = carried[idx];
        // C canletgo/do.c:665-670,718-719: no time and no transfer.
        if eq.equipment.worn(ArmorSlot::Suit) == Some(item_id) {
            return vec![GameEvent::LogMessage {
                text: "You cannot drop something you are wearing.".into(),
            }];
        }
        if let Some(item) = obj.items.get_mut(item_id) {
            item.location = ItemLocation::Floor(player.coord);
            let name = item.name.clone();
            if eq.wielded_item == Some(item_id) {
                eq.wielded_item = None;
            }
            events.push(GameEvent::ItemDropped {
                actor: h.player_id,
                item: item_id,
            });
            events.push(GameEvent::LogMessage {
                text: format!("You drop the {name}."),
            });
            clk.hero_energy = clk.hero_energy.saturating_sub(NORMAL_SPEED);
        }
    } else {
        events.push(GameEvent::LogMessage {
            text: "You don't have that item in your pack.".into(),
        });
    }

    events
}

fn execute_wield(
    clk: &mut ClockFacts,
    obj: &ObjectsFacts,
    h: &HeroFacts,
    eq: &mut EquipmentFacts,
    idx: usize,
) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let carried = items_carried_by(&obj.items, h.player_id);
    if idx < carried.len() {
        let item_id = carried[idx];
        eq.wielded_item = Some(item_id);
        let name = obj
            .items
            .get(item_id)
            .map(|i| i.name.clone())
            .unwrap_or_default();
        events.push(GameEvent::ItemWielded {
            actor: h.player_id,
            item: item_id,
        });
        events.push(GameEvent::LogMessage {
            text: format!("You wield the {name}."),
        });
        clk.hero_energy = clk.hero_energy.saturating_sub(NORMAL_SPEED);
    } else {
        events.push(GameEvent::LogMessage {
            text: "You don't have that item to wield.".into(),
        });
    }

    events
}

/// One public suit slot, using released C negative-multi/afternmv timing
/// (equipment.rs `handle_dressing` + `finish_dressing`).
#[allow(clippy::too_many_arguments)]
fn execute_dressing(
    clk: &mut ClockFacts,
    obj: &ObjectsFacts,
    a: &mut ActorsFacts,
    h: &mut HeroFacts,
    eq: &mut EquipmentFacts,
    config: &OwnerBridgeConfig,
    index: usize,
    putting_on: bool,
) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let Some(item_id) = items_carried_by(&obj.items, h.player_id)
        .get(index)
        .copied()
    else {
        return vec![GameEvent::LogMessage {
            text: "You don't have that item.".into(),
        }];
    };
    let item = obj.items.get(item_id).unwrap();
    let Some(def) = config.ruleset.item(&item.name) else {
        return events;
    };
    // Eligibility and delay are owned facts, never display-name guesses.
    if item.class != ItemClass::Armor
        || !def
            .armor
            .as_ref()
            .is_some_and(|a| a.slot == ArmorSlot::Suit)
    {
        return vec![GameEvent::LogMessage {
            text: "This action requires a suit of armor.".into(),
        }];
    }
    // Form fitting/racial exceptions need the effective-form equipment path.
    if h.hero.polymorph.is_some() {
        return vec![GameEvent::LogMessage {
            text: "Suit dressing in this form is not supported.".into(),
        }];
    }
    let worn = eq.equipment.worn(ArmorSlot::Suit);
    if eq.equipment.worn(ArmorSlot::Cloak).is_some()
        || (putting_on && worn.is_some())
        || (!putting_on && worn != Some(item_id))
    {
        return vec![GameEvent::LogMessage {
            text: "You cannot wear or remove that suit now.".into(),
        }];
    }
    // C select_off's basic curse check returns without setting takeoff.mask;
    // armor_or_accessory_off then returns ECMD_OK, spending no turn.
    if !putting_on && item.buc == Buc::Cursed {
        return vec![GameEvent::LogMessage {
            text: "You can't. It is cursed.".into(),
        }];
    }
    let delay = def.oc_delay;
    if putting_on {
        eq.equipment.armor.push((ArmorSlot::Suit, item_id));
        // Released C dowear removes W_WEAPONS before setting W_ARM.
        if eq.wielded_item == Some(item_id) {
            eq.wielded_item = None;
        }
        if h.hero.quivered_item == Some(item_id) {
            h.hero.quivered_item = None;
        }
    }
    eq.equipment.dressing = Some(Dressing {
        item: item_id,
        putting_on,
        turns_left: delay,
    });
    events.push(GameEvent::DressingStarted {
        item: item_id,
        putting_on,
    });
    if delay == 0 {
        let dressing = eq.equipment.dressing.take().unwrap();
        if !dressing.putting_on {
            eq.equipment.armor.retain(|(_, id)| *id != dressing.item);
        }
        events.push(GameEvent::DressingFinished {
            item: dressing.item,
            putting_on: dressing.putting_on,
        });
        let borrowed = BorrowedObjects {
            items: &obj.items,
            rules: &config.ruleset,
        };
        let ac =
            command_frame::armor_class(&eq.equipment, &borrowed, h.player_id, h.divine_protection);
        if let Some(p) = a.actors.get_mut(h.player_id) {
            p.ac = ac;
        }
    }
    clk.hero_energy = clk.hero_energy.saturating_sub(NORMAL_SPEED);
    events
}

#[allow(clippy::too_many_arguments)]
fn execute_put_in_container(
    clk: &mut ClockFacts,
    obj: &mut ObjectsFacts,
    a: &mut ActorsFacts,
    h: &HeroFacts,
    eq: &mut EquipmentFacts,
    config: &OwnerBridgeConfig,
    rng: &mut rand_chacha::ChaCha8Rng,
    item_index: usize,
    container_index: usize,
) -> owners::Result<Vec<GameEvent>> {
    let mut events = Vec::new();
    let carried = items_carried_by(&obj.items, h.player_id);
    if item_index < carried.len() && container_index < carried.len() {
        let item_id = carried[item_index];
        let container_id = carried[container_index];
        // C in_container rejects worn armor before even checking magic bags.
        // Indexed selection maps to menu_loot's n=1 attempt: even a rejected
        // callback costs time (pickup.c:3365-3376,3391), without bag RNG.
        if eq.equipment.worn(ArmorSlot::Suit) == Some(item_id) {
            clk.hero_energy = clk.hero_energy.saturating_sub(NORMAL_SPEED);
            return Ok(vec![GameEvent::LogMessage {
                text: "You cannot stash something you are wearing.".into(),
            }]);
        }
        if item_id == container_id {
            events.push(GameEvent::LogMessage {
                text: "You cannot put something inside itself!".into(),
            });
        } else {
            let item = obj.items.get(item_id).unwrap().clone();
            let container = obj.items.get(container_id).unwrap().clone();
            if !container.is_container {
                events.push(GameEvent::LogMessage {
                    text: format!("The {} is not a container.", container.name),
                });
            } else if container.is_bag_of_holding && {
                // pickup.c:2658: only a BoH checks the inserted object.
                let tree = bag_check_tree(&obj.items, item_id);
                nethacked_core::mbag_explodes(&tree, 0, &mut |bound| {
                    rng.random_range(0..bound.max(1))
                })
            } {
                // Magical container explosion!
                events.push(GameEvent::LogMessage {
                    text: "The magical energies rupture the fabric of space! The bag explodes with a blinding flash!".into(),
                });
                if eq.wielded_item == Some(item_id) || eq.wielded_item == Some(container_id) {
                    eq.wielded_item = None;
                }
                // pickup.c:2667-2668: an inserted BoH scatters its own contents
                // first, then the outer bag's (pickup.c:2683).
                if item.is_bag_of_holding {
                    do_boh_explosion(obj, &a.actors, h.player_id, item_id, rng);
                }
                do_boh_explosion(obj, &a.actors, h.player_id, container_id, rng);
                // The inserted object was never inserted: obfree() deletes it.
                // obfree deletes contents too; a BoH's were already extracted above.
                destroy_item_tree(obj, item_id);
                destroy_item(obj, container_id);
                // pickup.c:2693: losehp(d(6, 6), "magical explosion").
                let damage: u32 = (0..6).map(|_| rng.random_range(1..=6u32)).sum();
                if let Some(p) = a.actors.get_mut(h.player_id) {
                    p.hp = p.hp.saturating_sub(damage);
                    if p.hp == 0 {
                        p.is_dead = true;
                    }
                }
                clk.hero_energy = clk.hero_energy.saturating_sub(NORMAL_SPEED);
            } else {
                let frozen = corpse_freeze_snapshot(&obj.items, item_id);
                if let Some(i) = obj.items.get_mut(item_id) {
                    i.location = ItemLocation::InContainer(container_id);
                }
                apply_corpse_freeze_transitions(
                    obj,
                    &a.actors,
                    clk,
                    h.player_id,
                    &config.ruleset,
                    frozen,
                    rng,
                )?;
                if eq.wielded_item == Some(item_id) {
                    eq.wielded_item = None;
                }
                events.push(GameEvent::LogMessage {
                    text: format!("You put the {} into the {}.", item.name, container.name),
                });
                clk.hero_energy = clk.hero_energy.saturating_sub(NORMAL_SPEED);
            }
        }
    } else {
        events.push(GameEvent::LogMessage {
            text: "Invalid item index for container action.".into(),
        });
    }

    Ok(events)
}

#[allow(clippy::too_many_arguments)]
fn execute_take_from_container(
    clk: &mut ClockFacts,
    obj: &mut ObjectsFacts,
    a: &ActorsFacts,
    h: &HeroFacts,
    config: &OwnerBridgeConfig,
    rng: &mut rand_chacha::ChaCha8Rng,
    container_index: usize,
    item_index: usize,
) -> owners::Result<Vec<GameEvent>> {
    let mut events = Vec::new();
    let carried = items_carried_by(&obj.items, h.player_id);
    if container_index < carried.len() {
        let container_id = carried[container_index];
        let in_container = items_in_container(&obj.items, container_id);
        if item_index < in_container.len() {
            let item_id = in_container[item_index];
            let frozen = corpse_freeze_snapshot(&obj.items, item_id);
            // Project the actual root total, including all enclosing bag
            // rounding, rather than adding the child's weight twice.
            if let Some(i) = obj.items.get_mut(item_id) {
                i.location = ItemLocation::CarriedBy(h.player_id);
            }
            let weight = carried_weight(&obj.items, h.player_id);
            let levitating = a
                .actors
                .get(h.player_id)
                .map(|p| p.intrinsics.levitation)
                .unwrap_or(false);
            if !can_lift_weight(h, levitating, weight) {
                if let Some(i) = obj.items.get_mut(item_id) {
                    i.location = ItemLocation::InContainer(container_id);
                }
                events.push(GameEvent::LogMessage {
                    text: "You are carrying too much to remove that item.".into(),
                });
                clk.hero_energy = clk.hero_energy.saturating_sub(NORMAL_SPEED);
                return Ok(events);
            }
            let item_name = obj
                .items
                .get(item_id)
                .map(|i| i.name.clone())
                .unwrap_or_default();
            let container_name = obj
                .items
                .get(container_id)
                .map(|c| c.name.clone())
                .unwrap_or_default();
            if let Some(i) = obj.items.get_mut(item_id) {
                i.location = ItemLocation::CarriedBy(h.player_id);
            }
            apply_corpse_freeze_transitions(
                obj,
                &a.actors,
                clk,
                h.player_id,
                &config.ruleset,
                frozen,
                rng,
            )?;
            events.push(GameEvent::LogMessage {
                text: format!("You take the {item_name} out of the {container_name}."),
            });
            clk.hero_energy = clk.hero_energy.saturating_sub(NORMAL_SPEED);
        } else {
            events.push(GameEvent::LogMessage {
                text: "There is no such item in that container.".into(),
            });
        }
    } else {
        events.push(GameEvent::LogMessage {
            text: "Invalid container index.".into(),
        });
    }

    Ok(events)
}

fn execute_quiver(
    clk: &mut ClockFacts,
    obj: &ObjectsFacts,
    h: &mut HeroFacts,
    eq: &EquipmentFacts,
    sess: &SessionFacts,
    idx: SlotId,
) -> Vec<GameEvent> {
    let mut events = Vec::new();
    // C doquiver_core/wield.c:565-567 refuses worn armor with ECMD_OK.
    if eq.equipment.worn(ArmorSlot::Suit) == Some(idx) {
        return vec![GameEvent::LogMessage {
            text: "You cannot ready that!".into(),
        }];
    }
    clk.hero_energy = clk.hero_energy.saturating_sub(NORMAL_SPEED);

    // Check if item exists in arena and is carried by player
    let items = items_carried_by(&obj.items, h.player_id);
    if items.contains(&idx) {
        h.hero.quivered_item = Some(idx);
        let item_name = obj
            .items
            .get(idx)
            .map(|i| i.name.as_str())
            .unwrap_or("item");
        events.push(GameEvent::LogMessage {
            text: nethacked_i18n::Messages::quiver_success(item_name, sess.locale),
        });
    }
    events
}

#[allow(clippy::too_many_arguments)]
fn execute_fire(
    clk: &mut ClockFacts,
    obj: &mut ObjectsFacts,
    a: &mut ActorsFacts,
    h: &mut HeroFacts,
    eq: &EquipmentFacts,
    geo: &mut GeographyFacts,
    geo_delta: &mut GeographyDelta,
    sess: &SessionFacts,
    config: &OwnerBridgeConfig,
    rng: &mut rand_chacha::ChaCha8Rng,
    dir: Direction,
) -> Vec<GameEvent> {
    let mut events = Vec::new();
    // Defend imported/stale quiver state too: canletgo before throwing
    // returns ECMD_OK without damage, transfer, breakage RNG or time.
    if h.hero
        .quivered_item
        .is_some_and(|id| eq.equipment.worn(ArmorSlot::Suit) == Some(id))
    {
        return vec![GameEvent::LogMessage {
            text: "You cannot throw something you are wearing.".into(),
        }];
    }
    // With no selected projectile, the AST cannot answer C's refill
    // prompt; dofire/dothrow.c:544-585 cancels without spending time.
    if h.hero.quivered_item.is_none() {
        return vec![GameEvent::LogMessage {
            text: nethacked_i18n::Messages::quiver_empty(sess.locale).into(),
        }];
    }
    clk.hero_energy = clk.hero_energy.saturating_sub(NORMAL_SPEED);

    let quivered_item_opt = h.hero.quivered_item;
    if let Some(item_id) = quivered_item_opt {
        let start = a.actors.get(h.player_id).map(|a| a.coord).unwrap();
        let mut current = start;
        let mut target_actor = None;

        for _ in 0..10 {
            if let Some(next) = current.step(dir) {
                current = next;
                if let Some(actor_id) = actor_at(&a.actors, current) {
                    target_actor = Some(actor_id);
                    break;
                }
                if !geo.level.is_passable(current) {
                    break;
                }
            } else {
                break;
            }
        }

        if let Some(actor_id) = target_actor {
            let damage = 2; // Simple damage for now
            events.push(GameEvent::LogMessage {
                text: nethacked_i18n::Messages::hit_monster(sess.locale).into(),
            });

            let lethal = if let Some(target) = a.actors.get_mut(actor_id) {
                target.hp = target.hp.saturating_sub(damage);
                target.hp == 0
            } else {
                false
            };

            events.push(GameEvent::AttackLanded {
                attacker: h.player_id,
                target: actor_id,
                damage,
                lethal,
            });

            if lethal {
                a.actors.remove(actor_id);
            } else if actor_id != h.player_id {
                // C thitmonst -> hmon -> hmon_hitmon (uhitm.c:1923-1926):
                // `wakeup(mon, TRUE)` -> setmangry.
                setmangry(
                    &mut a.actors,
                    h,
                    geo,
                    geo_delta,
                    actor_id,
                    clk.turn,
                    sess.locale,
                    &config.ruleset,
                    &sess.role_name,
                    rng,
                    &mut events,
                );
            }

            // Breakage check
            let roll: u8 = rng.random_range(0..100);
            if resolve_projectile_impact(25, roll) {
                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::projectile_breaks(sess.locale).into(),
                });
                destroy_item(obj, item_id);
                h.hero.quivered_item = None;
            } else {
                if let Some(item) = obj.items.get_mut(item_id) {
                    item.location = ItemLocation::Floor(current);
                }
                h.hero.quivered_item = None;
            }
        } else {
            events.push(GameEvent::LogMessage {
                text: nethacked_i18n::Messages::projectile_misses(sess.locale).into(),
            });
            if let Some(item) = obj.items.get_mut(item_id) {
                item.location = ItemLocation::Floor(current);
            }
            h.hero.quivered_item = None;
        }
    } else {
        events.push(GameEvent::LogMessage {
            text: nethacked_i18n::Messages::quiver_empty(sess.locale).into(),
        });
    }

    events
}

pub(crate) fn outcome(
    action: &InventoryAction,
    obs: &[Observation],
    config: &OwnerBridgeConfig,
    rng: &mut rand_chacha::ChaCha8Rng,
) -> owners::Result<InventoryOutcome> {
    let mut clk: ClockFacts = fact(obs, 0)?;
    let mut obj: ObjectsFacts = fact(obs, 1)?;
    let mut a: ActorsFacts = fact(obs, 2)?;
    let mut h: HeroFacts = fact(obs, 3)?;
    let mut eq: EquipmentFacts = fact(obs, 4)?;
    let bill: BillingFacts = fact(obs, 5)?;
    let mut geo: GeographyFacts = fact(obs, 7)?;
    let sess: SessionFacts = fact(obs, 8)?;

    let Some(mut hero_actor) = a.actors.get(h.player_id).cloned() else {
        return Err(owners::Error::Unsupported);
    };
    if hero_actor.is_dead || hero_actor.hp == 0 {
        return Ok(InventoryOutcome::empty());
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

    // 2. Execute the inventory action
    let mut events = Vec::new();
    let mut geo_delta = GeographyDelta::default();

    // C multi < 0 (fall_asleep, paralysis): the hero cannot act this turn;
    // the action degenerates to a stationary wait (actions/mod.rs).
    let spent_time = if h.hero.afflictions.transient.helpless > 0 || eq.equipment.dressing.is_some()
    {
        clk.hero_energy = clk.hero_energy.saturating_sub(NORMAL_SPEED);
        clk.hero_energy != energy_before
    } else {
        match action {
            InventoryAction::PickUp => {
                let pickup_events = execute_pickup(&mut clk, &mut obj, &a, &mut h, &bill, &sess);
                events.extend(pickup_events);
                clk.hero_energy != energy_before
            }
            InventoryAction::Drop(idx) => {
                let drop_events = execute_drop(&mut clk, &mut obj, &a, &h, &mut eq, *idx);
                events.extend(drop_events);
                clk.hero_energy != energy_before
            }
            InventoryAction::Wield(idx) => {
                let wield_events = execute_wield(&mut clk, &obj, &h, &mut eq, *idx);
                events.extend(wield_events);
                clk.hero_energy != energy_before
            }
            InventoryAction::Wear(idx) => {
                let wear_events =
                    execute_dressing(&mut clk, &obj, &mut a, &mut h, &mut eq, config, *idx, true);
                events.extend(wear_events);
                clk.hero_energy != energy_before
            }
            InventoryAction::Remove(idx) => {
                let remove_events =
                    execute_dressing(&mut clk, &obj, &mut a, &mut h, &mut eq, config, *idx, false);
                events.extend(remove_events);
                clk.hero_energy != energy_before
            }
            InventoryAction::PutInContainer {
                item_index,
                container_index,
            } => {
                let put_events = execute_put_in_container(
                    &mut clk,
                    &mut obj,
                    &mut a,
                    &h,
                    &mut eq,
                    config,
                    rng,
                    *item_index,
                    *container_index,
                )?;
                events.extend(put_events);
                clk.hero_energy != energy_before
            }
            InventoryAction::TakeFromContainer {
                container_index,
                item_index,
            } => {
                let take_events = execute_take_from_container(
                    &mut clk,
                    &mut obj,
                    &a,
                    &h,
                    config,
                    rng,
                    *container_index,
                    *item_index,
                )?;
                events.extend(take_events);
                clk.hero_energy != energy_before
            }
            InventoryAction::Quiver(idx) => {
                let quiver_events = execute_quiver(&mut clk, &obj, &mut h, &eq, &sess, *idx);
                events.extend(quiver_events);
                clk.hero_energy != energy_before
            }
            InventoryAction::Fire(dir) => {
                let fire_events = execute_fire(
                    &mut clk,
                    &mut obj,
                    &mut a,
                    &mut h,
                    &eq,
                    &mut geo,
                    &mut geo_delta,
                    &sess,
                    config,
                    rng,
                    *dir,
                );
                events.extend(fire_events);
                clk.hero_energy != energy_before
            }
        }
    };

    // The action mutates the player actor in place (BoH explosion damage);
    // fold those into the loop's working copy so monster ticks and the final
    // write-back observe the post-action hero.
    if let Some(p) = a.actors.get(h.player_id) {
        hero_actor.coord = p.coord;
        hero_actor.hp = p.hp;
        hero_actor.is_dead = p.is_dead;
    }

    // 3. Final equipment reconciliation and derived AC (pre-turn-ticks)
    let borrowed = BorrowedObjects {
        items: &obj.items,
        rules: &config.ruleset,
    };
    eq.equipment = command_frame::reconcile(&eq.equipment, &borrowed, h.player_id);
    hero_actor.ac =
        command_frame::armor_class(&eq.equipment, &borrowed, h.player_id, h.divine_protection);

    // 4. Turn-stepping loop. Inventory actions never walk, so the legacy
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

            if hero_actor.is_dead || hero_actor.hp == 0 || clk.hero_energy >= NORMAL_SPEED {
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
                clk.hero_speed,
                properties.fast,
                properties.very_fast,
                burden,
                |bound| rng.random_range(0..bound),
            );
            clk.hero_energy = clk.hero_energy.saturating_add(gain);
            clk.turn += 1;

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
                clk.turn,
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

            if clk.hero_energy >= NORMAL_SPEED {
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

    Ok(InventoryOutcome {
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
            alignment_record: h.alignment_record,
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

struct InventoryCodec<T>(OwnerBridgeConfig, std::marker::PhantomData<T>);
impl<T: Domain> Schema<T> for InventoryCodec<T> {
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

impl<T: Domain> NativeBehavior<T> for InventoryCodec<T> {
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
    result: &InventoryOutcome,
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

fn preview(obs: &[Observation]) -> owners::Result<InventoryOutcome> {
    fact(obs, 6)
}

impl Domain for ClockState {
    const INDEX: usize = 0;
    const WRITE_NAME: Option<&'static str> = Some(WRITES[0]);
    fn read(&self, _: &[u8], _: &[Observation], _: &OwnerBridgeConfig) -> owners::Result<Vec<u8>> {
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
            for stored in &mut next.stored_items {
                for (_, item) in stored.iter_mut() {
                    if item.is_corpse() {
                        item.corpse_age = item.corpse_age.saturating_add(req.outcome.rounds);
                    }
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
                player_luck: self.player_luck,
                prayer_timeout: self.divine_state.prayer_timeout,
                divine_protection: self.divine_protection,
                hero_race: self.hero_race,
                alignment_record: self.alignment_record,
                progress: self.progress.clone(),
                genocide_registry: self.genocide_registry.clone(),
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
            next.player_luck = h.player_luck;
            next.divine_state.prayer_timeout = h.prayer_timeout;
            next.alignment_record = h.alignment_record;
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
    const WRITE_NAME: Option<&'static str> = None;
    fn read(&self, _: &[u8], _: &[Observation], _: &OwnerBridgeConfig) -> owners::Result<Vec<u8>> {
        encode(
            &BillingFacts {
                unpaid_items: self.unpaid_items.clone(),
            },
            PAYLOAD,
        )
    }
}

impl Domain for RandomState {
    const INDEX: usize = 6;
    const WRITE_NAME: Option<&'static str> = Some(WRITES[6]);
    fn read(
        &self,
        request: &[u8],
        obs: &[Observation],
        c: &OwnerBridgeConfig,
    ) -> owners::Result<Vec<u8>> {
        let action: InventoryAction = decode(request, PAYLOAD)?;
        encode(&outcome(&action, obs, c, &mut self.rng.clone())?, PAYLOAD)
    }
    fn prepare(
        &self,
        request: &[u8],
        obs: &[Observation],
        c: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        let action: InventoryAction = decode(request, PAYLOAD)?;
        let mut next = self.clone();
        let result = outcome(&action, obs, c, &mut next.rng)?;
        proposed(next, vec![], &result)
    }
}

impl Domain for GeographyState {
    const INDEX: usize = 7;
    const WRITE_NAME: Option<&'static str> = Some(WRITES[5]);
    fn read(&self, _: &[u8], _: &[Observation], _: &OwnerBridgeConfig) -> owners::Result<Vec<u8>> {
        encode(
            &GeographyFacts {
                level: self.level.clone(),
                branch: self.current_branch,
                depth: self.depth,
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
                locale: self.locale,
            },
            PAYLOAD,
        )
    }
}

pub(super) struct InventoryAccounting {
    pub(super) config: OwnerBridgeConfig,
    pub(super) action: InventoryAction,
}

impl Accounting for InventoryAccounting {
    fn validate(&self, p: &[ProposalView], obs: &[Observation]) -> owners::Result<()> {
        if obs.len() != 9 {
            return Err(owners::Error::Schema);
        }
        let random = p
            .iter()
            .find(|x| x.receipt.command == WRITES[6])
            .ok_or(owners::Error::Invariant)?;
        let before: RandomState = decode(&random.before, PAYLOAD)?;
        let result: InventoryOutcome = decode(&random.reply, PAYLOAD)?;
        let mut expected = before.clone();
        let expected_outcome = outcome(&self.action, obs, &self.config, &mut expected.rng)?;
        if encode(&expected, PAYLOAD)? != random.proposed
            || encode(&expected_outcome, PAYLOAD)? != random.reply
            || encode(&expected_outcome, PAYLOAD)?
                != fact::<InventoryOutcome>(obs, 6).and_then(|r| encode(&r, PAYLOAD))?
            || !random.events.is_empty()
        {
            return Err(owners::Error::Invariant);
        }

        let expected_count = 7;
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
                "inventory_clock_prepare" => check!(ClockState),
                "inventory_objects_prepare" => check!(ObjectsState),
                "inventory_actors_prepare" => check!(ActorsState),
                "inventory_hero_prepare" => check!(HeroState),
                "inventory_equipment_prepare" => check!(EquipmentState),
                "inventory_geography_prepare" => check!(GeographyState),
                _ => return Err(owners::Error::Invariant),
            }
        }
        for n in [
            WRITES[0], WRITES[1], WRITES[2], WRITES[3], WRITES[4], WRITES[5], WRITES[6],
        ] {
            if p.iter().filter(|x| x.receipt.command == n).count() != 1 {
                return Err(owners::Error::Invariant);
            }
        }
        Ok(())
    }
}

fn identity() -> op::ArtifactIdentity {
    artifact("native-inventory-accounting-i6", 69)
}

pub(super) fn capture(
    a: &mut op::AccountingCaptureCatalog,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if c.inventory_actions {
        a.register(
            identity(),
            POLICY,
            Rc::new(InventoryAccounting {
                config: c.clone(),
                action: InventoryAction::PickUp,
            }),
        )?;
    }
    Ok(())
}

pub(super) fn restore(
    a: &mut op::AccountingRestoreCatalog,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if c.inventory_actions {
        let cc = c.clone();
        a.allow(identity(), POLICY, move || {
            Ok(Rc::new(InventoryAccounting {
                config: cc.clone(),
                action: InventoryAction::PickUp,
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

pub(super) struct InventoryTransport {
    kernel: Kernel,
    caller: ModuleHandle,
    routes: Rc<Routes>,
    resources: Vec<Resource>,
}

impl InventoryTransport {
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
            let action: InventoryAction = decode(bytes, PAYLOAD).map_err(native_refusal)?;
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
            encode(&InventoryRequest { action, guards }, PAYLOAD).map_err(native_refusal)
        });

        let run_port: NativePort = Rc::new(move |bytes, view| {
            let req: InventoryRequest = decode(bytes, PAYLOAD).map_err(native_refusal)?;
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
                r.geography
                    .prepare(WRITES[5], &intent, vec![], view)
                    .map_err(|e| native_refusal_step("geography.prepare", e))?
                    .erase(),
                r.random
                    .prepare(WRITES[6], &action_bytes, other, view)
                    .map_err(|e| native_refusal_step("random.prepare", e))?
                    .erase(),
            ];
            let writes = vec![
                r.clock.id(),
                r.objects.id(),
                r.actors.id(),
                r.hero.id(),
                r.equipment.id(),
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
                    Rc::new(InventoryAccounting {
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
                &InventoryReply {
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
            kernel.register_native(manifest(HOME, exports.clone(), vec![]), &[66; 32], ports)?;
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
            &[67; 32],
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
                        schema: [index as u8 + 160; 32],
                        ports,
                    },
                    $state,
                    Backend::Native(Rc::new(InventoryCodec::<$ty>(
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
        resource!(billing, 5, None);
        resource!(random, 6, Some(WRITES[6]));
        resource!(geography, 7, Some(WRITES[5]));
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

    pub(super) fn preview(
        &mut self,
        action: &InventoryAction,
    ) -> Result<InventoryRequest, OwnerBridgeError> {
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
        request: InventoryRequest,
    ) -> Result<(InventoryReply, NativePartitions), OwnerBridgeError> {
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
        let outcome: InventoryReply = decode(&reply?, PAYLOAD)?;

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
    fn inventory_session_witnesses(&self) -> Result<Vec<wand::OwnerWitness>, OwnerBridgeError> {
        self.session.with_runtime(&self.credential.borrow(), |b| {
            Ok(bridge_stamps(b)?
                .iter()
                .map(wand::OwnerWitness::from)
                .collect())
        })?
    }

    pub fn prepare_inventory(
        &self,
        action: InventoryAction,
    ) -> Result<InventoryRequest, OwnerBridgeError> {
        if !self.config.inventory_actions {
            return Err(owners::Error::Unsupported.into());
        }
        let parts = self.partitions()?;
        let mut transport = InventoryTransport::install(parts, self.config.clone())?;
        let _ = transport.preview(&action)?;
        let guards = self.inventory_session_witnesses()?;
        Ok(InventoryRequest { action, guards })
    }

    pub fn execute_inventory(
        &self,
        request: InventoryRequest,
    ) -> Result<InventoryReply, OwnerBridgeError> {
        if !self.config.inventory_actions {
            return Err(owners::Error::Unsupported.into());
        }
        let current_witnesses = self.inventory_session_witnesses()?;
        if request.guards != current_witnesses {
            #[cfg(test)]
            STALE_REFUSALS.with(|x| x.set(x.get() + 1));
            return Err(owners::Error::Stale.into());
        }
        let parts = self.partitions()?;
        let mut transport = InventoryTransport::install(parts, self.config.clone())?;
        let (reply, updated) = transport.execute(request)?;
        let source = Self::install(updated, self.config.clone())?;
        let wire = source.capture()?;
        drop(source);
        self.restore(&wire)?;
        Ok(reply)
    }

    pub fn step_inventory(
        &self,
        action: InventoryAction,
    ) -> Result<InventoryReply, OwnerBridgeError> {
        let req = self.prepare_inventory(action)?;
        self.execute_inventory(req)
    }
}
