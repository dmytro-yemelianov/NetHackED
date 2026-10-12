//! Explicit M6-I5 movement, doors, traps and inline Search/Untrap dispatch.
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
use nethacked_core::engraving::Engraving;
use nethacked_core::inventory::EncumbranceTier;
use nethacked_dungeon::{DungeonLevel, RoomType};
use nethacked_kernel::ModuleHandle;
use nethacked_types::{
    BranchId, Buc, Coord, Direction, DoorState, ItemClass, Locale, Tile, TrapRecord,
};
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use wand::OwnerWitness;

pub const HOME: &str = "movement-home-i5-v1";
pub const CALLER: &str = "movement-caller-i5-v1";
pub const PREVIEW: &str = "movement_preview_i5";
pub const RUN: &str = "movement_run_i5";

pub const READS: [&str; 9] = [
    "movement_clock_read",
    "movement_objects_read",
    "movement_actors_read",
    "movement_hero_read",
    "movement_equipment_read",
    "movement_billing_read",
    "movement_random_read",
    "movement_geography_read",
    "movement_session_read",
];

pub const WRITES: [&str; 7] = [
    "movement_clock_prepare",
    "movement_objects_prepare",
    "movement_actors_prepare",
    "movement_hero_prepare",
    "movement_equipment_prepare",
    "movement_geography_prepare",
    "movement_random_prepare",
];

pub const POLICY: [u8; 32] = [68; 32];

fn native_refusal(e: owners::Error) -> kernel::Error {
    match e {
        owners::Error::OverBound => kernel::Error::OverBound,
        _ => kernel::Error::NativeRefusal,
    }
}

fn native_refusal_step(_: &str, e: owners::Error) -> kernel::Error {
    match e {
        owners::Error::OverBound => kernel::Error::OverBound,
        _ => kernel::Error::NativeRefusal,
    }
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
pub enum MovementAction {
    Move(Direction),
    OpenDoor(Coord),
    CloseDoor(Coord),
    Search,
    Untrap(Coord),
    Kick(Coord),
    MeleeAttack(Coord),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MovementRequest {
    pub action: MovementAction,
    pub guards: Vec<OwnerWitness>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MovementReply {
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
pub struct BillingFacts {
    pub unpaid_items: Vec<(ItemId, u32)>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct GeographyFacts {
    #[serde(with = "nethacked_dungeon::compact_level")]
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
pub struct MovementOutcome {
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

impl MovementOutcome {
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
    pub(super) outcome: MovementOutcome,
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

fn actor_at(actors: &ActorStorage, coord: Coord) -> Option<ActorId> {
    actors
        .iter()
        .find(|(_, a)| !a.is_dead && a.coord == coord)
        .map(|(id, _)| id)
}

fn is_safemon(
    actors: &ActorStorage,
    player_id: ActorId,
    target_id: ActorId,
    hero: &nethacked_types::Hero,
) -> bool {
    let Some(target) = actors.get(target_id) else {
        return false;
    };
    let hero_blind = actors.get(player_id).is_some_and(|p| p.intrinsics.blind);
    let t = &hero.afflictions.transient;
    target.is_peaceful && !hero_blind && t.confused == 0 && t.hallucinating == 0 && t.stunned == 0
}

fn hero_moves_at_random(hero: &nethacked_types::Hero, rng: &mut rand_chacha::ChaCha8Rng) -> bool {
    let t = &hero.afflictions.transient;
    let (stunned, confused) = (t.stunned > 0, t.confused > 0);
    stunned || (confused && rng.random_range(0..5u32) == 0)
}

fn compute_move_cost(hero: &nethacked_types::Hero, actors: &ActorStorage) -> u32 {
    let mount_cost = hero.mount.as_ref().and_then(|m| {
        actors
            .get(m.steed_id)
            .map(|s| (NORMAL_SPEED * 12) / s.speed.max(1))
    });
    nethacked_core::ranged::effective_movement_cost(NORMAL_SPEED, mount_cost)
}

#[allow(clippy::too_many_arguments)]
fn bump_peaceful(
    actors: &mut ActorStorage,
    level: &DungeonLevel,
    ruleset: &crate::Ruleset,
    role_name: &str,
    player_id: ActorId,
    target_id: ActorId,
    hero_from: Coord,
    target_coord: Coord,
    locale: Locale,
    rng: &mut rand_chacha::ChaCha8Rng,
) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let Some(target) = actors.get(target_id).cloned() else {
        return events;
    };
    let must_stop = rng.random_range(0..7u32) == 0;
    let inshop = !must_stop
        && level.room_at(target_coord).is_some_and(|room| {
            room.room_type == RoomType::Shop
                && actors
                    .values()
                    .any(|a| !a.is_dead && a.name == "shopkeeper" && room.contains_inner(a.coord))
        });
    if must_stop || inshop {
        events.push(GameEvent::LogMessage {
            text: nethacked_i18n::Messages::peaceful_in_the_way(
                &target.name,
                crate::peace::monnam_article(ruleset, &target.name),
                locale,
            ),
        });
        return events;
    }
    let quest_cfg = nethacked_core::get_role_quest_config_or_default(role_name);
    let mundisplaceable = ruleset.monster_for_actor(&target).is_some_and(|a| {
        matches!(
            a.id,
            Some(
                nethacked_data::MonsterSpeciesId::ALIGNED_CLERIC
                    | nethacked_data::MonsterSpeciesId::SHOPKEEPER,
            )
        )
    }) || target.name.eq_ignore_ascii_case(quest_cfg.leader_name);
    let trap_at_hero = level.traps.contains_key(&hero_from);
    if mundisplaceable || trap_at_hero || !level.is_passable(hero_from) {
        events.push(GameEvent::LogMessage {
            text: nethacked_i18n::Messages::peaceful_wont_swap(
                &target.name,
                crate::peace::monnam_article(ruleset, &target.name),
                locale,
            ),
        });
        return events;
    }
    if let Some(p) = actors.get_mut(player_id) {
        p.coord = target_coord;
    }
    if let Some(m) = actors.get_mut(target_id) {
        m.coord = hero_from;
    }
    events.push(GameEvent::ActorMoved {
        actor: player_id,
        from: hero_from,
        to: target_coord,
    });
    events.push(GameEvent::ActorMoved {
        actor: target_id,
        from: target_coord,
        to: hero_from,
    });
    events.push(GameEvent::LogMessage {
        text: nethacked_i18n::Messages::swap_with_peaceful(&target.name, locale),
    });
    events
}

#[allow(clippy::too_many_arguments)]
fn compute_hero_melee_attack(
    actors: &mut ActorStorage,
    items: &ItemStorage,
    wielded_item: Option<ItemId>,
    player_id: ActorId,
    target_id: ActorId,
    progress: &crate::progress::HeroProgress,
    player_luck: i32,
    ruleset: &crate::Ruleset,
    rng: &mut rand_chacha::ChaCha8Rng,
) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let Some(attacker) = actors.get(player_id).cloned() else {
        return events;
    };
    let Some(defender) = actors.get(target_id).cloned() else {
        return events;
    };
    if attacker.is_dead || defender.is_dead {
        return events;
    }

    let str_val = progress.attr_base[crate::progress::A_STR];
    let dex_val = progress.attr_base[crate::progress::A_DEX];

    let weapon_ench = if let Some(wid) = wielded_item {
        items.get(wid).map(|w| w.enchantment as i32).unwrap_or(0)
    } else {
        0
    };

    let weapon_dice: Option<(u32, u32)> = if let Some(wid) = wielded_item {
        items
            .get(wid)
            .and_then(|w| ruleset.item(&w.name).and_then(|i| i.weapon_damage_dice()))
    } else {
        None
    };

    let target_large = ruleset
        .monster_for_actor(&defender)
        .map(|m| m.size >= nethacked_data::MonsterSize::Large)
        .unwrap_or(false);
    let martial_arts = false;

    let attr_bonus =
        nethacked_core::combat::ordinary_hero_attribute_hit_bonus(str_val, dex_val, attacker.level);
    let str_dmg_bonus = nethacked_core::combat::ordinary_hero_strength_damage_bonus(str_val);

    let to_hit = nethacked_core::combat::to_hit_value(
        attacker.level as i32,
        player_luck,
        weapon_ench,
        attr_bonus,
        defender.ac,
    );

    let d20: u32 = rng.random_range(1..=20);
    if nethacked_core::combat::attack_hits(d20, to_hit) {
        let die =
            nethacked_core::combat::weapon_damage_die(weapon_dice, target_large, martial_arts);
        let roll = if die == 0 {
            0
        } else {
            rng.random_range(1..=die)
        };
        let base_roll =
            nethacked_core::combat::dmgval(weapon_dice, target_large, martial_arts, roll);
        let damage = nethacked_core::combat::melee_damage(base_roll, weapon_ench, str_dmg_bonus);
        let new_hp = defender.hp.saturating_sub(damage);
        let lethal = new_hp == 0;
        events.push(GameEvent::AttackLanded {
            attacker: player_id,
            target: target_id,
            damage,
            lethal,
        });
        if let Some(d) = actors.get_mut(target_id) {
            d.hp = new_hp;
            if lethal {
                d.is_dead = true;
            }
        }
        if lethal {
            events.push(GameEvent::LogMessage {
                text: format!("You kill the {}!", defender.name),
            });
        }
    } else {
        events.push(GameEvent::AttackMissed {
            attacker: player_id,
            target: target_id,
        });
    }
    events
}

#[allow(clippy::too_many_arguments)]
fn execute_move(
    dir: Direction,
    clk: &mut ClockFacts,
    obj: &mut ObjectsFacts,
    a: &mut ActorsFacts,
    h: &mut HeroFacts,
    eq: &mut EquipmentFacts,
    geo: &mut GeographyFacts,
    delta: &mut GeographyDelta,
    bill: &BillingFacts,
    sess: &SessionFacts,
    config: &OwnerBridgeConfig,
    rng: &mut rand_chacha::ChaCha8Rng,
) -> owners::Result<(Vec<GameEvent>, bool, bool)> {
    let mut events = Vec::new();
    let energy_before = clk.hero_energy;
    let Some(player) = a.actors.get(h.player_id).cloned() else {
        return Ok((events, false, false));
    };

    let burden = compute_hero_encumbrance(h, &obj.items, &player);
    if burden == EncumbranceTier::Overloaded {
        events.push(GameEvent::LogMessage {
            text: "You collapse under your load.".into(),
        });
        clk.hero_energy = clk.hero_energy.saturating_sub(NORMAL_SPEED);
        return Ok((events, true, false));
    }

    let move_cost = compute_move_cost(&h.hero, &a.actors);
    let (dx, dy) = dir.delta();
    let nx = player.coord.x as isize + dx;
    let ny = player.coord.y as isize + dy;

    let Some(target_coord) = Coord::new(nx as usize, ny as usize) else {
        return Ok((events, false, false));
    };

    let mut moved = false;

    if let Some(target_id) = actor_at(&a.actors, target_coord) {
        let is_target_tame = a.actors.get(target_id).map(|a| a.is_tame).unwrap_or(false);
        if is_target_tame {
            let pet_name = a
                .actors
                .get(target_id)
                .map(|a| a.name.clone())
                .unwrap_or_else(|| "pet".into());
            let from = player.coord;
            if let Some(p) = a.actors.get_mut(h.player_id) {
                p.coord = target_coord;
            }
            if let Some(pet) = a.actors.get_mut(target_id) {
                pet.coord = from;
            }
            events.push(GameEvent::ActorMoved {
                actor: h.player_id,
                from,
                to: target_coord,
            });
            events.push(GameEvent::ActorMoved {
                actor: target_id,
                from: target_coord,
                to: from,
            });
            events.push(GameEvent::LogMessage {
                text: nethacked_i18n::Messages::displace_pet(&pet_name, sess.locale),
            });
            moved = true;
            clk.hero_energy = clk.hero_energy.saturating_sub(move_cost);
        } else if is_safemon(&a.actors, h.player_id, target_id, &h.hero) {
            events.extend(bump_peaceful(
                &mut a.actors,
                &geo.level,
                &config.ruleset,
                &sess.role_name,
                h.player_id,
                target_id,
                player.coord,
                target_coord,
                sess.locale,
                rng,
            ));
            moved = a
                .actors
                .get(h.player_id)
                .is_some_and(|p| p.coord == target_coord);
            clk.hero_energy = clk.hero_energy.saturating_sub(move_cost);
        } else {
            events.extend(compute_hero_melee_attack(
                &mut a.actors,
                &obj.items,
                eq.wielded_item,
                h.player_id,
                target_id,
                &h.progress,
                h.player_luck,
                &config.ruleset,
                rng,
            ));
            clk.hero_energy = clk.hero_energy.saturating_sub(move_cost);
        }
    } else if let Some(boulder_id) = obj.items.iter().find_map(|(id, item)| {
        if item.location == ItemLocation::Floor(target_coord) && item.name == "boulder" {
            Some(id)
        } else {
            None
        }
    }) {
        if let Some(next_c) = target_coord.step(dir) {
            let is_occupied = actor_at(&a.actors, next_c).is_some()
                || obj
                    .items
                    .values()
                    .any(|it| it.location == ItemLocation::Floor(next_c) && it.name == "boulder");
            let outcome = nethacked_core::sokoban::push_boulder(
                target_coord,
                dir,
                geo.level.get_tile(next_c),
                is_occupied,
            );
            match outcome {
                nethacked_core::sokoban::PushOutcome::Moved(new_pos) => {
                    if let Some(it) = obj.items.get_mut(boulder_id) {
                        it.location = ItemLocation::Floor(new_pos);
                    }
                    events.push(GameEvent::LogMessage {
                        text: nethacked_i18n::Messages::push_boulder(sess.locale).into(),
                    });
                    clk.hero_energy = clk.hero_energy.saturating_sub(move_cost);
                }
                nethacked_core::sokoban::PushOutcome::FilledPit(pit_pos) => {
                    obj.items.remove(boulder_id);
                    geo.level.set_tile(pit_pos, Tile::Pit { filled: true });
                    delta.tiles.push((pit_pos, Tile::Pit { filled: true }));
                    events.push(GameEvent::LogMessage {
                        text: nethacked_i18n::Messages::boulder_falls_into_pit(sess.locale).into(),
                    });
                    clk.hero_energy = clk.hero_energy.saturating_sub(move_cost);
                }
                nethacked_core::sokoban::PushOutcome::Blocked => {
                    events.push(GameEvent::LogMessage {
                        text: nethacked_i18n::Messages::boulder_wont_budge(sess.locale).into(),
                    });
                }
            }
        } else {
            events.push(GameEvent::LogMessage {
                text: nethacked_i18n::Messages::boulder_wont_budge(sess.locale).into(),
            });
        }
    } else {
        let tile = geo.level.get_tile(target_coord).clone();
        match tile {
            Tile::Door {
                state: DoorState::Closed,
                trapped,
            } => {
                let new_tile = Tile::Door {
                    state: DoorState::Open,
                    trapped,
                };
                geo.level.set_tile(target_coord, new_tile.clone());
                delta.tiles.push((target_coord, new_tile));
                events.push(GameEvent::DoorToggled {
                    coord: target_coord,
                    new_state: DoorState::Open,
                });
                clk.hero_energy = clk.hero_energy.saturating_sub(move_cost);
            }
            Tile::Drawbridge { open: false } => {
                geo.level
                    .set_tile(target_coord, Tile::Drawbridge { open: true });
                delta
                    .tiles
                    .push((target_coord, Tile::Drawbridge { open: true }));
                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::drawbridge_lower(sess.locale).into(),
                });
                clk.hero_energy = clk.hero_energy.saturating_sub(move_cost);
            }
            _ if tile.is_passable() => {
                let from = player.coord;
                if let Some(p) = a.actors.get_mut(h.player_id) {
                    p.coord = target_coord;
                }
                events.push(GameEvent::ActorMoved {
                    actor: h.player_id,
                    from,
                    to: target_coord,
                });
                moved = true;

                let smudged = geo.level.smudge_engraving(from);
                if smudged.is_some() {
                    delta
                        .engravings
                        .push((from, geo.level.get_engraving(from).cloned()));
                }
                if let Some(outcome) = smudged {
                    match outcome {
                        Some(remaining_e) => {
                            events.push(GameEvent::EngravingDegraded {
                                coord: from,
                                remaining: Some(remaining_e.text),
                            });
                        }
                        None => {
                            events.push(GameEvent::EngravingDegraded {
                                coord: from,
                                remaining: None,
                            });
                            events.push(GameEvent::LogMessage {
                                text: nethacked_i18n::Messages::engraving_wiped_by_footsteps(
                                    sess.locale,
                                )
                                .into(),
                            });
                        }
                    }
                }

                if let Some(e) = geo.level.get_engraving(target_coord) {
                    events.push(GameEvent::LogMessage {
                        text: nethacked_i18n::Messages::floor_engraving_text(&e.text, sess.locale),
                    });
                }

                let is_flying = a
                    .actors
                    .get(h.player_id)
                    .map(|a| a.intrinsics.levitation)
                    .unwrap_or(false);
                let rn2_5: u32 = if geo.level.traps.contains_key(&target_coord) {
                    rng.random_range(0..5)
                } else {
                    1
                };
                let triggered = if let Some(trap) = geo.level.traps.get_mut(&target_coord) {
                    let triggered = nethacked_core::traps::trigger_trap(trap, is_flying, rn2_5);
                    delta.traps.push((target_coord, *trap));
                    triggered
                } else {
                    None
                };
                if let Some(triggered_type) = triggered {
                    match triggered_type {
                        nethacked_types::TrapType::Arrow | nethacked_types::TrapType::Dart => {
                            events.push(GameEvent::LogMessage {
                                text: nethacked_i18n::Messages::trap_shoots(
                                    &format!("{triggered_type:?}"),
                                    sess.locale,
                                ),
                            });
                            // C `damage_player` (world.rs): lethal hits announce death.
                            let dmg = 2u32;
                            if let Some(p) = a.actors.get_mut(h.player_id) {
                                p.hp = p.hp.saturating_sub(dmg);
                                if p.hp == 0 && !p.is_dead {
                                    p.is_dead = true;
                                    events.push(GameEvent::LogMessage {
                                        text: format!(
                                            "You die... killed by {} trap.",
                                            format!("{triggered_type:?}").to_lowercase()
                                        ),
                                    });
                                }
                            }
                        }
                        nethacked_types::TrapType::Teleport => {
                            events.push(GameEvent::LogMessage {
                                text: nethacked_i18n::Messages::trap_teleport(sess.locale).into(),
                            });
                        }
                        nethacked_types::TrapType::LevelTeleport => {
                            events.push(GameEvent::LogMessage {
                                text: nethacked_i18n::Messages::trap_level_teleport(sess.locale)
                                    .into(),
                            });
                        }
                        nethacked_types::TrapType::Pit | nethacked_types::TrapType::SpikedPit => {
                            events.push(GameEvent::LogMessage {
                                text: nethacked_i18n::Messages::trap_fall_into_pit(sess.locale)
                                    .into(),
                            });
                        }
                        _ => {
                            events.push(GameEvent::LogMessage {
                                text: nethacked_i18n::Messages::trap_trigger_generic(
                                    &format!("{triggered_type:?}"),
                                    sess.locale,
                                ),
                            });
                        }
                    }
                }

                let from_shop = geo
                    .level
                    .room_at(from)
                    .map(|r| r.room_type == RoomType::Shop)
                    .unwrap_or(false);
                let to_shop = geo
                    .level
                    .room_at(target_coord)
                    .map(|r| r.room_type == RoomType::Shop)
                    .unwrap_or(false);
                if from_shop && !to_shop {
                    let has_unpaid = obj.items.iter().any(|(id, item)| {
                        item.location == ItemLocation::CarriedBy(h.player_id)
                            && bill.unpaid_items.iter().any(|(uid, _)| *uid == id)
                    });
                    if has_unpaid {
                        events.push(GameEvent::LogMessage {
                            text: nethacked_i18n::Messages::shopkeeper_shout(sess.locale).into(),
                        });
                        for (_, actor) in a.actors.iter_mut() {
                            if actor.name == "shopkeeper" && !actor.is_dead {
                                actor.is_peaceful = false;
                            }
                        }
                    }
                }

                clk.hero_energy = clk.hero_energy.saturating_sub(move_cost);
            }
            _ => {
                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::bump_wall(sess.locale).into(),
                });
            }
        }
    }

    let spent = clk.hero_energy != energy_before;
    Ok((events, spent, moved))
}

fn execute_open_door(
    coord: Coord,
    clk: &mut ClockFacts,
    a: &ActorsFacts,
    h: &HeroFacts,
    geo: &mut GeographyFacts,
    delta: &mut GeographyDelta,
) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let Some(player) = a.actors.get(h.player_id).cloned() else {
        return events;
    };
    if player.coord.is_adjacent(coord) {
        let tile = geo.level.get_tile(coord).clone();
        if let Tile::Door {
            state: DoorState::Closed,
            trapped,
        } = tile
        {
            let new_tile = Tile::Door {
                state: DoorState::Open,
                trapped,
            };
            geo.level.set_tile(coord, new_tile.clone());
            delta.tiles.push((coord, new_tile));
            events.push(GameEvent::DoorToggled {
                coord,
                new_state: DoorState::Open,
            });
            clk.hero_energy = clk.hero_energy.saturating_sub(NORMAL_SPEED);
        } else {
            events.push(GameEvent::LogMessage {
                text: "There is no closed door there to open.".into(),
            });
        }
    }
    events
}

fn execute_close_door(
    coord: Coord,
    clk: &mut ClockFacts,
    a: &ActorsFacts,
    h: &HeroFacts,
    geo: &mut GeographyFacts,
    delta: &mut GeographyDelta,
) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let Some(player) = a.actors.get(h.player_id).cloned() else {
        return events;
    };
    if player.coord.is_adjacent(coord) {
        let tile = geo.level.get_tile(coord).clone();
        if let Tile::Door {
            state: DoorState::Open,
            trapped,
        } = tile
        {
            let new_tile = Tile::Door {
                state: DoorState::Closed,
                trapped,
            };
            geo.level.set_tile(coord, new_tile.clone());
            delta.tiles.push((coord, new_tile));
            events.push(GameEvent::DoorToggled {
                coord,
                new_state: DoorState::Closed,
            });
            clk.hero_energy = clk.hero_energy.saturating_sub(NORMAL_SPEED);
        } else {
            events.push(GameEvent::LogMessage {
                text: "There is no open door there to close.".into(),
            });
        }
    }
    events
}

fn execute_search(
    clk: &mut ClockFacts,
    a: &ActorsFacts,
    h: &HeroFacts,
    geo: &mut GeographyFacts,
    delta: &mut GeographyDelta,
    sess: &SessionFacts,
) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let mut found = false;
    let Some(player) = a.actors.get(h.player_id) else {
        return events;
    };
    for neighbor in player.coord.neighbors() {
        if let Some(trap) = geo.level.traps.get_mut(&neighbor) {
            if trap.state == nethacked_types::TrapState::Hidden {
                trap.state = nethacked_types::TrapState::Revealed;
                delta.traps.push((neighbor, *trap));
                events.push(GameEvent::LogMessage {
                    text: format!("You find a {:?} trap!", trap.trap_type),
                });
                found = true;
            }
        }
    }
    if !found {
        events.push(GameEvent::LogMessage {
            text: nethacked_i18n::Messages::search_nothing(sess.locale).into(),
        });
    }
    clk.hero_energy = clk.hero_energy.saturating_sub(NORMAL_SPEED);
    events
}

fn execute_untrap(
    coord: Coord,
    clk: &mut ClockFacts,
    geo: &mut GeographyFacts,
    delta: &mut GeographyDelta,
    sess: &SessionFacts,
) -> Vec<GameEvent> {
    let mut events = Vec::new();
    if let Some(trap) = geo.level.traps.get_mut(&coord) {
        if nethacked_core::traps::disarm_trap(trap) {
            delta.traps.push((coord, *trap));
            events.push(GameEvent::LogMessage {
                text: nethacked_i18n::Messages::trap_disarmed(sess.locale).into(),
            });
        } else {
            let text = if sess.locale == Locale::Uk {
                "Пастку вже знешкоджено."
            } else {
                "The trap is already disarmed."
            };
            events.push(GameEvent::LogMessage { text: text.into() });
        }
    } else {
        events.push(GameEvent::LogMessage {
            text: nethacked_i18n::Messages::no_trap(sess.locale).into(),
        });
    }
    clk.hero_energy = clk.hero_energy.saturating_sub(NORMAL_SPEED);
    events
}

/// C `dokick` (actions/doors.rs `handle_kick`): an adjacent door shatters
/// (unless already broken, which is a silent no-op); any other adjacent tile
/// is a wall kick. Both spend the action time; a non-adjacent kick does
/// nothing at all.
fn execute_kick(
    coord: Coord,
    clk: &mut ClockFacts,
    a: &ActorsFacts,
    h: &HeroFacts,
    geo: &mut GeographyFacts,
    delta: &mut GeographyDelta,
) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let Some(player) = a.actors.get(h.player_id).cloned() else {
        return events;
    };
    if player.coord.is_adjacent(coord) {
        let tile = geo.level.get_tile(coord).clone();
        if let Tile::Door { state, .. } = tile {
            if state != DoorState::Broken {
                let new_tile = Tile::Door {
                    state: DoorState::Broken,
                    trapped: false,
                };
                geo.level.set_tile(coord, new_tile.clone());
                delta.tiles.push((coord, new_tile));
                events.push(GameEvent::DoorToggled {
                    coord,
                    new_state: DoorState::Broken,
                });
                events.push(GameEvent::LogMessage {
                    text: "As you kick the door, it shatters into splinters!".into(),
                });
                clk.hero_energy = clk.hero_energy.saturating_sub(NORMAL_SPEED);
            }
        } else {
            events.push(GameEvent::LogMessage {
                text: "WHAMMM!!! You kick the wall!".into(),
            });
            clk.hero_energy = clk.hero_energy.saturating_sub(NORMAL_SPEED);
        }
    }
    events
}

/// C `doattack` (actions/doors.rs `handle_melee_attack`): an adjacent occupied
/// tile resolves hero melee combat through the same accepted owner law as a
/// melee bump (`compute_hero_melee_attack`); an adjacent empty tile swings at
/// empty air. Both spend the action time; a non-adjacent attack does nothing.
#[allow(clippy::too_many_arguments)]
fn execute_melee_attack(
    coord: Coord,
    clk: &mut ClockFacts,
    obj: &ObjectsFacts,
    a: &mut ActorsFacts,
    h: &HeroFacts,
    eq: &EquipmentFacts,
    config: &OwnerBridgeConfig,
    rng: &mut rand_chacha::ChaCha8Rng,
) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let Some(player) = a.actors.get(h.player_id).cloned() else {
        return events;
    };
    if player.coord.is_adjacent(coord) {
        if let Some(target_id) = actor_at(&a.actors, coord) {
            events.extend(compute_hero_melee_attack(
                &mut a.actors,
                &obj.items,
                eq.wielded_item,
                h.player_id,
                target_id,
                &h.progress,
                h.player_luck,
                &config.ruleset,
                rng,
            ));
            clk.hero_energy = clk.hero_energy.saturating_sub(NORMAL_SPEED);
        } else {
            events.push(GameEvent::LogMessage {
                text: "You swing at empty air.".into(),
            });
            clk.hero_energy = clk.hero_energy.saturating_sub(NORMAL_SPEED);
        }
    }
    events
}

pub(crate) fn outcome(
    action: &MovementAction,
    obs: &[Observation],
    config: &OwnerBridgeConfig,
    rng: &mut rand_chacha::ChaCha8Rng,
) -> owners::Result<MovementOutcome> {
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
        return Ok(MovementOutcome::empty());
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

    // 2. Execute the movement action
    let mut events = Vec::new();
    let mut moved = false;
    let mut geo_delta = GeographyDelta::default();

    let action = if h.hero.afflictions.transient.helpless > 0 || eq.equipment.dressing.is_some() {
        MovementAction::Move(Direction::None)
    } else {
        action.clone()
    };

    // C domove refuses overload before impaired_movement (hack.c:2730,2747);
    // a refused move must not draw a direction. The helpless/dressing
    // substitution above yields Move(None), which is a stationary wait and
    // must never draw a random direction either.
    let action = match action {
        MovementAction::Move(dir)
            if dir != Direction::None
                && compute_hero_encumbrance(&h, &obj.items, &hero_actor)
                    != EncumbranceTier::Overloaded
                && hero_moves_at_random(&h.hero, rng) =>
        {
            const DIRS: [Direction; 8] = [
                Direction::North,
                Direction::NorthEast,
                Direction::East,
                Direction::SouthEast,
                Direction::South,
                Direction::SouthWest,
                Direction::West,
                Direction::NorthWest,
            ];
            MovementAction::Move(DIRS[rng.random_range(0..DIRS.len())])
        }
        other => other,
    };

    let spent_time = match action {
        MovementAction::Move(Direction::None) => {
            clk.hero_energy = clk.hero_energy.saturating_sub(NORMAL_SPEED);
            clk.hero_energy != energy_before
        }
        MovementAction::Move(dir) => {
            let (move_events, spent, was_moved) = execute_move(
                dir,
                &mut clk,
                &mut obj,
                &mut a,
                &mut h,
                &mut eq,
                &mut geo,
                &mut geo_delta,
                &bill,
                &sess,
                config,
                rng,
            )?;
            events.extend(move_events);
            moved = was_moved;
            spent
        }
        MovementAction::OpenDoor(coord) => {
            let door_events = execute_open_door(coord, &mut clk, &a, &h, &mut geo, &mut geo_delta);
            events.extend(door_events);
            clk.hero_energy != energy_before
        }
        MovementAction::CloseDoor(coord) => {
            let door_events = execute_close_door(coord, &mut clk, &a, &h, &mut geo, &mut geo_delta);
            events.extend(door_events);
            clk.hero_energy != energy_before
        }
        MovementAction::Search => {
            let search_events = execute_search(&mut clk, &a, &h, &mut geo, &mut geo_delta, &sess);
            events.extend(search_events);
            clk.hero_energy != energy_before
        }
        MovementAction::Untrap(coord) => {
            let untrap_events = execute_untrap(coord, &mut clk, &mut geo, &mut geo_delta, &sess);
            events.extend(untrap_events);
            clk.hero_energy != energy_before
        }
        MovementAction::Kick(coord) => {
            let kick_events = execute_kick(coord, &mut clk, &a, &h, &mut geo, &mut geo_delta);
            events.extend(kick_events);
            clk.hero_energy != energy_before
        }
        MovementAction::MeleeAttack(coord) => {
            let melee_events =
                execute_melee_attack(coord, &mut clk, &obj, &mut a, &h, &eq, config, rng);
            events.extend(melee_events);
            clk.hero_energy != energy_before
        }
    };

    // The action mutates the player actor in place (coord, trap damage); fold
    // those into the loop's working copy so monster ticks and the final
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

    // 4. Turn-stepping loop
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

    // 5. Post-loop
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

    Ok(MovementOutcome {
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

struct MovementCodec<T>(OwnerBridgeConfig, std::marker::PhantomData<T>);
impl<T: Domain> Schema<T> for MovementCodec<T> {
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

impl<T: Domain> NativeBehavior<T> for MovementCodec<T> {
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
    result: &MovementOutcome,
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

fn preview(obs: &[Observation]) -> owners::Result<MovementOutcome> {
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
        let action: MovementAction = decode(request, PAYLOAD)?;
        encode(&outcome(&action, obs, c, &mut self.rng.clone())?, PAYLOAD)
    }
    fn prepare(
        &self,
        request: &[u8],
        obs: &[Observation],
        c: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        let action: MovementAction = decode(request, PAYLOAD)?;
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

pub(super) struct MovementAccounting {
    pub(super) config: OwnerBridgeConfig,
    pub(super) action: MovementAction,
}

impl Accounting for MovementAccounting {
    fn validate(&self, p: &[ProposalView], obs: &[Observation]) -> owners::Result<()> {
        if obs.len() != 9 {
            return Err(owners::Error::Schema);
        }
        let random = p
            .iter()
            .find(|x| x.receipt.command == WRITES[6])
            .ok_or(owners::Error::Invariant)?;
        let before: RandomState = decode(&random.before, PAYLOAD)?;
        let result: MovementOutcome = decode(&random.reply, PAYLOAD)?;
        let mut expected = before.clone();
        let expected_outcome = outcome(&self.action, obs, &self.config, &mut expected.rng)?;
        if encode(&expected, PAYLOAD)? != random.proposed
            || encode(&expected_outcome, PAYLOAD)? != random.reply
            || encode(&expected_outcome, PAYLOAD)?
                != fact::<MovementOutcome>(obs, 6).and_then(|r| encode(&r, PAYLOAD))?
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
                "movement_clock_prepare" => check!(ClockState),
                "movement_objects_prepare" => check!(ObjectsState),
                "movement_actors_prepare" => check!(ActorsState),
                "movement_hero_prepare" => check!(HeroState),
                "movement_equipment_prepare" => check!(EquipmentState),
                "movement_geography_prepare" => check!(GeographyState),
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
    artifact("native-movement-accounting-i5", 68)
}

pub(super) fn capture(
    a: &mut op::AccountingCaptureCatalog,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if c.movement_actions {
        a.register(
            identity(),
            POLICY,
            Rc::new(MovementAccounting {
                config: c.clone(),
                action: MovementAction::Search,
            }),
        )?;
    }
    Ok(())
}

pub(super) fn restore(
    a: &mut op::AccountingRestoreCatalog,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if c.movement_actions {
        let cc = c.clone();
        a.allow(identity(), POLICY, move || {
            Ok(Rc::new(MovementAccounting {
                config: cc.clone(),
                action: MovementAction::Search,
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

pub(super) struct MovementTransport {
    kernel: Kernel,
    caller: ModuleHandle,
    routes: Rc<Routes>,
    resources: Vec<Resource>,
}

impl MovementTransport {
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
            let action: MovementAction = decode(bytes, PAYLOAD).map_err(native_refusal)?;
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
            encode(&MovementRequest { action, guards }, PAYLOAD).map_err(native_refusal)
        });

        let run_port: NativePort = Rc::new(move |bytes, view| {
            let req: MovementRequest = decode(bytes, PAYLOAD).map_err(native_refusal)?;
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
                    Rc::new(MovementAccounting {
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
                &MovementReply {
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
            kernel.register_native(manifest(HOME, exports.clone(), vec![]), &[98; 32], ports)?;
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
            &[99; 32],
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
                        schema: [index as u8 + 170; 32],
                        ports,
                    },
                    $state,
                    Backend::Native(Rc::new(MovementCodec::<$ty>(
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
        action: &MovementAction,
    ) -> Result<MovementRequest, OwnerBridgeError> {
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
        request: MovementRequest,
    ) -> Result<(MovementReply, NativePartitions), OwnerBridgeError> {
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
        let outcome: MovementReply = decode(&reply?, PAYLOAD)?;

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
    fn movement_session_witnesses(&self) -> Result<Vec<wand::OwnerWitness>, OwnerBridgeError> {
        self.session.with_runtime(&self.credential.borrow(), |b| {
            Ok(bridge_stamps(b)?
                .iter()
                .map(wand::OwnerWitness::from)
                .collect())
        })?
    }

    pub fn prepare_movement(
        &self,
        action: MovementAction,
    ) -> Result<MovementRequest, OwnerBridgeError> {
        if !self.config.movement_actions {
            return Err(owners::Error::Unsupported.into());
        }
        let parts = self.partitions()?;
        let mut transport = MovementTransport::install(parts, self.config.clone())?;
        let _ = transport.preview(&action)?;
        let guards = self.movement_session_witnesses()?;
        Ok(MovementRequest { action, guards })
    }

    pub fn execute_movement(
        &self,
        request: MovementRequest,
    ) -> Result<MovementReply, OwnerBridgeError> {
        if !self.config.movement_actions {
            return Err(owners::Error::Unsupported.into());
        }
        let current_witnesses = self.movement_session_witnesses()?;
        if request.guards != current_witnesses {
            #[cfg(test)]
            STALE_REFUSALS.with(|x| x.set(x.get() + 1));
            return Err(owners::Error::Stale.into());
        }
        let parts = self.partitions()?;
        let mut transport = MovementTransport::install(parts, self.config.clone())?;
        let (reply, updated) = transport.execute(request)?;
        let source = Self::install(updated, self.config.clone())?;
        let wire = source.capture()?;
        drop(source);
        self.restore(&wire)?;
        Ok(reply)
    }

    pub fn step_movement(&self, action: MovementAction) -> Result<MovementReply, OwnerBridgeError> {
        let req = self.prepare_movement(action)?;
        self.execute_movement(req)
    }
}
