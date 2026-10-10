//! Borrowed compatibility adapters. No adapter retains the legacy host.
use super::wand::{self, ports::*};
use crate::{object_timers::*, progress::HeroProgress, world::SimulationWorld, GameEvent};
use nethacked_arena::{
    ActorId, ActorRecord, ActorStorage, ItemId, ItemLocation, ItemRecord, ItemStorage,
};
use nethacked_core::{
    energy::{SchedulerState, NORMAL_SPEED},
    engraving::Engraving,
    QuestProgress, QuestState,
};
use nethacked_data::{
    ruleset::{MonsterDef, Ruleset},
    ItemKindId, RaceId, RoleId,
};
use nethacked_dungeon::DungeonLevel;
use nethacked_types::{BranchId, ConductTracker, Coord, Direction, GenocideRegistry, Tile};
use std::collections::BTreeSet;

type CachedItems<'a> = ((BranchId, usize), &'a [(ItemId, ItemRecord)]);

struct LegacyObjects<'a> {
    items: &'a mut ItemStorage,
    artifact_history: &'a mut BTreeSet<String>,
    cached: Vec<CachedItems<'a>>,
}
impl ObjectFacts for LegacyObjects<'_> {
    fn carried(&self, actor: ActorId) -> Vec<ItemId> {
        self.items
            .iter()
            .filter_map(|(id, item)| {
                (item.location == ItemLocation::CarriedBy(actor)).then_some(id)
            })
            .collect()
    }
    fn object(&self, id: ItemId) -> Option<SelectedObject> {
        self.items.get(id).map(|item| SelectedObject {
            class: item.class,
            name: item.name.clone(),
            definition: item.definition.clone(),
            buc: item.buc,
            enchantment: item.enchantment,
        })
    }
    fn floor_kind(&self, coord: Coord, kind: ItemKindId, ruleset: &Ruleset) -> bool {
        self.items.iter().any(|(_, item)| {
            item.location == ItemLocation::Floor(coord)
                && ruleset
                    .item(&item.name)
                    .is_some_and(|def| def.id == Some(kind))
        })
    }
    fn has_amulet(&self, actor: ActorId) -> bool {
        self.carried(actor)
            .into_iter()
            .any(|id| self.items.get(id).is_some_and(super::items::is_real_amulet))
    }
}
impl Objects for LegacyObjects<'_> {
    fn spend_positive_charge(&mut self, id: ItemId, remaining: i8) {
        if let Some(item) = self.items.get_mut(id) {
            item.enchantment = remaining;
        }
    }
    fn insert_generated(&mut self, item: ItemRecord) -> ItemId {
        if let Some(id) = &item.artifact_id {
            self.artifact_history.insert(id.clone());
        }
        self.items.insert(item)
    }
    fn timer_target_exists(&self, target: ObjectTimerTarget) -> bool {
        match target {
            ObjectTimerTarget::Active(id) => self.items.contains_key(id),
            ObjectTimerTarget::Cached {
                branch,
                depth,
                item,
            } => self
                .cached
                .iter()
                .find(|(key, _)| *key == (branch, depth))
                .is_some_and(|(_, items)| items.iter().any(|(id, _)| *id == item)),
        }
    }
    fn frozen(&self, id: ItemId) -> bool {
        nethacked_arena::frozen_in_ice_box(&self.items[id], |parent| self.items.get(parent))
    }
}
struct LegacyActors<'a> {
    actors: &'a mut ActorStorage,
    genocide: &'a GenocideRegistry,
}
impl Actors for LegacyActors<'_> {
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
    fn guardians(&self, name: &str, include_sound: bool, ruleset: &Ruleset) -> Vec<ActorId> {
        self.actors
            .iter()
            .filter(|(_, actor)| {
                !actor.is_dead
                    && (actor.name.eq_ignore_ascii_case(name)
                        || (include_sound
                            && ruleset.monster_for_actor(actor).is_some_and(|def| {
                                def.msound == nethacked_data::MonsterSound::Guardian
                            })))
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
struct LegacyGeography<'a>(&'a mut DungeonLevel);
impl GeographyFacts for LegacyGeography<'_> {
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
impl Geography for LegacyGeography<'_> {
    fn reveal_secret_door(&mut self, coord: Coord) -> bool {
        let tile = self.0.get_tile_mut(coord);
        if matches!(tile, Tile::SecretDoor { .. }) {
            tile.reveal_secret_door();
            true
        } else {
            false
        }
    }
    fn collapse_drawbridge(&mut self, coord: Coord) {
        self.0.set_tile(coord, Tile::Moat);
    }
    fn break_door(&mut self, coord: Coord) {
        self.0.get_tile_mut(coord).break_door();
    }
    fn freeze_pool(&mut self, coord: Coord) {
        self.0.set_tile(coord, Tile::Pool { frozen: true });
    }
    fn erase_engraving(&mut self, coord: Coord) {
        self.0.remove_engraving(coord);
    }
}
struct LegacyClock<'a> {
    scheduler: &'a mut SchedulerState,
    timers: &'a mut ObjectTimerQueue,
}
impl WorldClock for LegacyClock<'_> {
    fn turn(&self) -> u64 {
        self.scheduler.turn
    }
}
impl Clock for LegacyClock<'_> {
    fn hero_act(&mut self) {
        self.scheduler.hero_act(NORMAL_SPEED);
    }
    fn start_corpse_timer(
        &mut self,
        target: ObjectTimerTarget,
        kind: ObjectTimerKind,
        due: Option<u64>,
        objects: &impl Objects,
    ) -> Result<u64, ObjectTimerError> {
        self.timers.start_at(target, kind, due, |target| {
            objects.timer_target_exists(target)
        })
    }
    fn cancel_corpse_timers(&mut self, target: ObjectTimerTarget) {
        self.timers.cancel(target);
    }
}
struct LegacyHero<'a> {
    polymorphed: bool,
    role_id: Option<RoleId>,
    role_name: &'a str,
    race: RaceId,
    alignment: &'a mut i32,
    conducts: &'a mut ConductTracker,
    quest: &'a mut QuestState,
    favor: &'a mut i32,
    luck: &'a mut i32,
    protection: &'a mut u32,
    progress: &'a mut HeroProgress,
    power: &'a mut u32,
    max_power: &'a mut u32,
}
impl HeroFacts for LegacyHero<'_> {
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
        *self.alignment
    }
}
impl Hero for LegacyHero<'_> {
    fn polymorphed(&self) -> bool {
        self.polymorphed
    }
    fn adjust_alignment(&mut self, delta: i32, limit: i32) {
        *self.alignment = nethacked_core::adjalign(*self.alignment, delta, limit);
    }
    fn record_kill(&mut self) {
        nethacked_core::conducts::record_kill(self.conducts);
    }
    fn quest_leader_killed(&self) -> bool {
        self.quest.killed_leader
    }
    fn quest_progress(&self) -> QuestProgress {
        self.quest.progress
    }
    fn mark_quest_leader_killed(&mut self) {
        self.quest.killed_leader = true;
    }
    fn defeat_quest_nemesis(&mut self) {
        nethacked_core::attack_nemesis(self.quest, 9999);
    }
    fn adjust_favor(&mut self, delta: i32) {
        *self.favor = nethacked_core::religion::clamp_favor(*self.favor + delta);
    }
    fn adjust_luck(&mut self, delta: i32) {
        *self.luck = nethacked_core::mines::clamp_luck(*self.luck + delta);
    }
    fn clear_divine_protection(&mut self) {
        *self.protection = 0;
    }
    fn credit_experience(&mut self, experience: u64) {
        self.progress.exp = self.progress.exp.saturating_add(experience);
    }
    fn experience(&self) -> u64 {
        self.progress.exp
    }
    fn level_max(&self) -> u32 {
        self.progress.level_max
    }
    fn constitution(&self) -> i32 {
        self.progress.attr_base[crate::progress::A_CON]
    }
    fn record_level_gain(&mut self, from: u32, hp: i32, energy: i32, experience: u64) {
        let idx = from as usize;
        if self.progress.hp_increments.len() <= idx {
            self.progress.hp_increments.resize(idx + 1, 0);
            self.progress.en_increments.resize(idx + 1, 0);
        }
        self.progress.hp_increments[idx] = hp;
        self.progress.en_increments[idx] = energy;
        *self.max_power = self.max_power.saturating_add(energy as u32);
        *self.power = self.power.saturating_add(energy as u32);
        self.progress.exp = experience;
        self.progress.level_max = self.progress.level_max.max(from + 1);
    }
}

pub(super) fn zap(
    world: &mut SimulationWorld,
    selection: wand::Selection,
    dir: Direction,
    energy: u32,
) -> Vec<GameEvent> {
    let session = Session {
        ruleset: &world.ruleset,
        locale: world.locale,
        branch: world.current_branch,
        depth: world.depth,
    };
    let mut objects = LegacyObjects {
        items: &mut world.arena.items,
        artifact_history: &mut world.arena.artifact_history,
        cached: world
            .stored_levels
            .iter()
            .map(|(key, stored)| (*key, stored.items.as_slice()))
            .collect(),
    };
    let mut actors = LegacyActors {
        actors: &mut world.arena.actors,
        genocide: &world.genocide_registry,
    };
    let mut geography = LegacyGeography(&mut world.level);
    let mut clock = LegacyClock {
        scheduler: &mut world.scheduler,
        timers: &mut world.object_timers,
    };
    let mut hero = LegacyHero {
        polymorphed: world.hero.polymorph.is_some(),
        role_id: world.role_id,
        role_name: &world.role_name,
        race: world.hero_race,
        alignment: &mut world.alignment_record,
        conducts: &mut world.conducts,
        quest: &mut world.quest_state,
        favor: &mut world.divine_state.favor,
        luck: &mut world.player_luck,
        protection: &mut world.divine_protection,
        progress: &mut world.progress,
        power: &mut world.player_pw,
        max_power: &mut world.player_max_pw,
    };
    let mut outcome = wand::zap_with_survivor(
        selection,
        world.player_id,
        dir,
        energy,
        &mut objects,
        &mut actors,
        &mut geography,
        &mut hero,
        &mut clock,
        &mut world.rng,
        &session,
    );
    match super::item_lifetime_legacy::finalize(world, outcome.survivor) {
        Ok(Some(_)) => outcome.events.push(GameEvent::LogMessage {
            text: "Your wand turns to dust.".into(),
        }),
        Ok(None) => {}
        Err(_) => outcome.events.push(GameEvent::LogMessage {
            text: "The wand cannot be retired: its accounting is inconsistent.".into(),
        }),
    }
    outcome.events
}

pub(crate) fn spawn_random_at(
    world: &mut SimulationWorld,
    coord: Coord,
    branch: BranchId,
    depth: usize,
) -> Option<ActorId> {
    let session = Session {
        ruleset: &world.ruleset,
        locale: world.locale,
        branch: world.current_branch,
        depth: world.depth,
    };
    let objects = CreationObjects(&world.arena.items);
    let mut actors = LegacyActors {
        actors: &mut world.arena.actors,
        genocide: &world.genocide_registry,
    };
    let geography = CreationGeography(&world.level);
    let clock = CreationClock(&world.scheduler);
    let hero = CreationHero {
        role_id: world.role_id,
        role_name: &world.role_name,
        race: world.hero_race,
        alignment: world.alignment_record,
    };
    wand::creation::spawn_at(
        coord,
        branch,
        depth,
        world.player_id,
        &objects,
        &mut actors,
        &geography,
        &hero,
        &clock,
        &mut world.rng,
        &session,
    )
}

struct CreationObjects<'a>(&'a ItemStorage);
impl ObjectFacts for CreationObjects<'_> {
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
    fn floor_kind(&self, coord: Coord, kind: ItemKindId, ruleset: &Ruleset) -> bool {
        self.0.iter().any(|(_, item)| {
            item.location == ItemLocation::Floor(coord)
                && ruleset
                    .item(&item.name)
                    .is_some_and(|def| def.id == Some(kind))
        })
    }
    fn has_amulet(&self, actor: ActorId) -> bool {
        self.carried(actor)
            .into_iter()
            .any(|id| self.0.get(id).is_some_and(super::items::is_real_amulet))
    }
}

struct CreationGeography<'a>(&'a DungeonLevel);
impl GeographyFacts for CreationGeography<'_> {
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

struct CreationClock<'a>(&'a SchedulerState);
impl WorldClock for CreationClock<'_> {
    fn turn(&self) -> u64 {
        self.0.turn
    }
}
struct CreationHero<'a> {
    role_id: Option<RoleId>,
    role_name: &'a str,
    race: RaceId,
    alignment: i32,
}
impl HeroFacts for CreationHero<'_> {
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
