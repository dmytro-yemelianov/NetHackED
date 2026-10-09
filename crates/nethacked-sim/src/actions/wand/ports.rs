//! Command-specific observations and operations for the implemented wand slice.
use crate::object_timers::{ObjectTimerError, ObjectTimerKind, ObjectTimerTarget};
use nethacked_arena::{ActorId, ActorRecord, ItemDefinition, ItemId, ItemRecord};
use nethacked_core::{engraving::Engraving, QuestProgress};
use nethacked_data::{
    ruleset::{MonsterDef, Ruleset},
    ItemKindId, RaceId, RoleId,
};
use nethacked_types::{BranchId, Coord, Direction, Locale, Tile};

pub struct Session<'a> {
    pub ruleset: &'a Ruleset,
    pub locale: Locale,
    pub branch: BranchId,
    pub depth: usize,
}

pub struct SelectedObject {
    pub class: nethacked_types::ItemClass,
    pub name: String,
    pub definition: Option<ItemDefinition>,
    pub buc: nethacked_types::Buc,
    pub enchantment: i8,
}

pub trait ObjectFacts {
    fn carried(&self, actor: ActorId) -> Vec<ItemId>;
    fn object(&self, id: ItemId) -> Option<SelectedObject>;
    fn floor_kind(&self, coord: Coord, kind: ItemKindId, ruleset: &Ruleset) -> bool;
    fn has_amulet(&self, actor: ActorId) -> bool;
}

pub trait Objects: ObjectFacts {
    fn spend_positive_charge(&mut self, id: ItemId, remaining: i8);
    fn insert_generated(&mut self, item: ItemRecord) -> ItemId;
    fn timer_target_exists(&self, target: ObjectTimerTarget) -> bool;
    fn frozen(&self, id: ItemId) -> bool;
}

pub trait Actors {
    fn actor(&self, id: ActorId) -> Option<ActorRecord>;
    fn at(&self, coord: Coord) -> Option<ActorId>;
    fn damage(&mut self, id: ActorId, damage: u32) -> Option<bool>;
    fn polymorph(&mut self, id: ActorId, definition: &MonsterDef);
    fn make_hostile(&mut self, id: ActorId);
    fn guardians(&self, name: &str, include_sound: bool, ruleset: &Ruleset) -> Vec<ActorId>;
    fn spawn_initialized(&mut self, actor: ActorRecord) -> ActorId;
    fn force_hostile_malign(&mut self, id: ActorId, malign: i32);
    fn gain_level(&mut self, id: ActorId, hp: u32, level: u32);
    fn genocided(&self, name: &str, glyph: char) -> bool;
}

pub trait GeographyFacts {
    fn tile(&self, coord: Coord) -> Tile;
    fn trajectory(&self, origin: Coord, direction: Direction, energy: u32) -> Vec<Coord>;
    fn engraving(&self, coord: Coord) -> Option<Engraving>;
}

pub trait Geography: GeographyFacts {
    fn reveal_secret_door(&mut self, coord: Coord) -> bool;
    fn collapse_drawbridge(&mut self, coord: Coord);
    fn break_door(&mut self, coord: Coord);
    fn freeze_pool(&mut self, coord: Coord);
    fn erase_engraving(&mut self, coord: Coord);
}

pub trait WorldClock {
    fn turn(&self) -> u64;
}

pub trait Clock: WorldClock {
    fn hero_act(&mut self);
    fn start_corpse_timer(
        &mut self,
        target: ObjectTimerTarget,
        kind: ObjectTimerKind,
        due: Option<u64>,
        objects: &impl Objects,
    ) -> Result<u64, ObjectTimerError>;
    fn cancel_corpse_timers(&mut self, target: ObjectTimerTarget);
}

/// Hero observations needed by creation, anger and the existing kill prefix.
pub trait HeroFacts {
    fn role_id(&self) -> Option<RoleId>;
    fn role_name(&self) -> &str;
    fn race(&self) -> RaceId;
    fn alignment_record(&self) -> i32;
}

pub trait Hero: HeroFacts {
    fn polymorphed(&self) -> bool;
    fn adjust_alignment(&mut self, delta: i32, limit: i32);
    fn record_kill(&mut self);
    fn quest_leader_killed(&self) -> bool;
    fn quest_progress(&self) -> QuestProgress;
    fn mark_quest_leader_killed(&mut self);
    fn defeat_quest_nemesis(&mut self);
    fn adjust_favor(&mut self, delta: i32);
    fn adjust_luck(&mut self, delta: i32);
    fn clear_divine_protection(&mut self);
    fn credit_experience(&mut self, experience: u64);
    fn experience(&self) -> u64;
    fn level_max(&self) -> u32;
    fn constitution(&self) -> i32;
    fn record_level_gain(&mut self, from: u32, hp: i32, energy: i32, experience: u64);
}

/// Borrowed adapter from the existing infallible ports.
pub struct InfalliblePort<'a, T>(pub &'a mut T);

pub trait TryObjectFacts<E> {
    fn carried(&self, actor: ActorId) -> Result<Vec<ItemId>, E>;
    fn object(&self, id: ItemId) -> Result<Option<SelectedObject>, E>;
    fn floor_kind(&self, coord: Coord, kind: ItemKindId, ruleset: &Ruleset) -> Result<bool, E>;
    fn has_amulet(&self, actor: ActorId) -> Result<bool, E>;
}
impl<T: ObjectFacts> TryObjectFacts<std::convert::Infallible> for InfalliblePort<'_, T> {
    fn carried(&self, actor: ActorId) -> Result<Vec<ItemId>, std::convert::Infallible> {
        Ok(self.0.carried(actor))
    }
    fn object(&self, id: ItemId) -> Result<Option<SelectedObject>, std::convert::Infallible> {
        Ok(self.0.object(id))
    }
    fn floor_kind(
        &self,
        coord: Coord,
        kind: ItemKindId,
        ruleset: &Ruleset,
    ) -> Result<bool, std::convert::Infallible> {
        Ok(self.0.floor_kind(coord, kind, ruleset))
    }
    fn has_amulet(&self, actor: ActorId) -> Result<bool, std::convert::Infallible> {
        Ok(self.0.has_amulet(actor))
    }
}

pub trait TryObjects<E>: TryObjectFacts<E> {
    fn spend_positive_charge(&mut self, id: ItemId, remaining: i8) -> Result<(), E>;
    fn insert_generated(&mut self, item: ItemRecord) -> Result<ItemId, E>;
    fn timer_target_exists(&self, target: ObjectTimerTarget) -> Result<bool, E>;
    fn frozen(&self, id: ItemId) -> Result<bool, E>;
}
impl<T: Objects> TryObjects<std::convert::Infallible> for InfalliblePort<'_, T> {
    fn spend_positive_charge(
        &mut self,
        id: ItemId,
        remaining: i8,
    ) -> Result<(), std::convert::Infallible> {
        self.0.spend_positive_charge(id, remaining);
        Ok(())
    }
    fn insert_generated(&mut self, item: ItemRecord) -> Result<ItemId, std::convert::Infallible> {
        Ok(self.0.insert_generated(item))
    }
    fn timer_target_exists(
        &self,
        target: ObjectTimerTarget,
    ) -> Result<bool, std::convert::Infallible> {
        Ok(self.0.timer_target_exists(target))
    }
    fn frozen(&self, id: ItemId) -> Result<bool, std::convert::Infallible> {
        Ok(self.0.frozen(id))
    }
}

pub trait TryActors<E> {
    fn actor(&self, id: ActorId) -> Result<Option<ActorRecord>, E>;
    fn at(&self, coord: Coord) -> Result<Option<ActorId>, E>;
    fn damage(&mut self, id: ActorId, damage: u32) -> Result<Option<bool>, E>;
    fn polymorph(&mut self, id: ActorId, definition: &MonsterDef) -> Result<(), E>;
    fn make_hostile(&mut self, id: ActorId) -> Result<(), E>;
    fn guardians(
        &self,
        name: &str,
        include_sound: bool,
        ruleset: &Ruleset,
    ) -> Result<Vec<ActorId>, E>;
    fn spawn_initialized(&mut self, actor: ActorRecord) -> Result<ActorId, E>;
    fn force_hostile_malign(&mut self, id: ActorId, malign: i32) -> Result<(), E>;
    fn gain_level(&mut self, id: ActorId, hp: u32, level: u32) -> Result<(), E>;
    fn genocided(&self, name: &str, glyph: char) -> Result<bool, E>;
    /// Acquire a pure genocide predicate at the factory call site. The returned
    /// predicate must use observed facts only, without authority calls, draws or effects.
    /// This keeps the data factories' pure `gone` callbacks separate from
    /// fallible authority acquisition. No predicate is retained across calls.
    fn genocide_filter(&self) -> Result<impl Fn(&str, char) -> bool + '_, E>;
}
impl<T: Actors> TryActors<std::convert::Infallible> for InfalliblePort<'_, T> {
    fn actor(&self, id: ActorId) -> Result<Option<ActorRecord>, std::convert::Infallible> {
        Ok(self.0.actor(id))
    }
    fn at(&self, coord: Coord) -> Result<Option<ActorId>, std::convert::Infallible> {
        Ok(self.0.at(coord))
    }
    fn damage(
        &mut self,
        id: ActorId,
        damage: u32,
    ) -> Result<Option<bool>, std::convert::Infallible> {
        Ok(self.0.damage(id, damage))
    }
    fn polymorph(
        &mut self,
        id: ActorId,
        definition: &MonsterDef,
    ) -> Result<(), std::convert::Infallible> {
        self.0.polymorph(id, definition);
        Ok(())
    }
    fn make_hostile(&mut self, id: ActorId) -> Result<(), std::convert::Infallible> {
        self.0.make_hostile(id);
        Ok(())
    }
    fn guardians(
        &self,
        name: &str,
        include_sound: bool,
        ruleset: &Ruleset,
    ) -> Result<Vec<ActorId>, std::convert::Infallible> {
        Ok(self.0.guardians(name, include_sound, ruleset))
    }
    fn spawn_initialized(
        &mut self,
        actor: ActorRecord,
    ) -> Result<ActorId, std::convert::Infallible> {
        Ok(self.0.spawn_initialized(actor))
    }
    fn force_hostile_malign(
        &mut self,
        id: ActorId,
        malign: i32,
    ) -> Result<(), std::convert::Infallible> {
        self.0.force_hostile_malign(id, malign);
        Ok(())
    }
    fn gain_level(
        &mut self,
        id: ActorId,
        hp: u32,
        level: u32,
    ) -> Result<(), std::convert::Infallible> {
        self.0.gain_level(id, hp, level);
        Ok(())
    }
    fn genocided(&self, name: &str, glyph: char) -> Result<bool, std::convert::Infallible> {
        Ok(self.0.genocided(name, glyph))
    }
    fn genocide_filter(
        &self,
    ) -> Result<impl Fn(&str, char) -> bool + '_, std::convert::Infallible> {
        Ok(|name: &str, glyph: char| self.0.genocided(name, glyph))
    }
}

pub trait TryGeographyFacts<E> {
    fn tile(&self, coord: Coord) -> Result<Tile, E>;
    fn trajectory(&self, origin: Coord, direction: Direction, energy: u32)
        -> Result<Vec<Coord>, E>;
    fn engraving(&self, coord: Coord) -> Result<Option<Engraving>, E>;
}
impl<T: GeographyFacts> TryGeographyFacts<std::convert::Infallible> for InfalliblePort<'_, T> {
    fn tile(&self, coord: Coord) -> Result<Tile, std::convert::Infallible> {
        Ok(self.0.tile(coord))
    }
    fn trajectory(
        &self,
        origin: Coord,
        direction: Direction,
        energy: u32,
    ) -> Result<Vec<Coord>, std::convert::Infallible> {
        Ok(self.0.trajectory(origin, direction, energy))
    }
    fn engraving(&self, coord: Coord) -> Result<Option<Engraving>, std::convert::Infallible> {
        Ok(self.0.engraving(coord))
    }
}

pub trait TryGeography<E>: TryGeographyFacts<E> {
    fn reveal_secret_door(&mut self, coord: Coord) -> Result<bool, E>;
    fn collapse_drawbridge(&mut self, coord: Coord) -> Result<(), E>;
    fn break_door(&mut self, coord: Coord) -> Result<(), E>;
    fn freeze_pool(&mut self, coord: Coord) -> Result<(), E>;
    fn erase_engraving(&mut self, coord: Coord) -> Result<(), E>;
}
impl<T: Geography> TryGeography<std::convert::Infallible> for InfalliblePort<'_, T> {
    fn reveal_secret_door(&mut self, coord: Coord) -> Result<bool, std::convert::Infallible> {
        Ok(self.0.reveal_secret_door(coord))
    }
    fn collapse_drawbridge(&mut self, coord: Coord) -> Result<(), std::convert::Infallible> {
        self.0.collapse_drawbridge(coord);
        Ok(())
    }
    fn break_door(&mut self, coord: Coord) -> Result<(), std::convert::Infallible> {
        self.0.break_door(coord);
        Ok(())
    }
    fn freeze_pool(&mut self, coord: Coord) -> Result<(), std::convert::Infallible> {
        self.0.freeze_pool(coord);
        Ok(())
    }
    fn erase_engraving(&mut self, coord: Coord) -> Result<(), std::convert::Infallible> {
        self.0.erase_engraving(coord);
        Ok(())
    }
}

pub trait TryWorldClock<E> {
    fn turn(&self) -> Result<u64, E>;
}
impl<T: WorldClock> TryWorldClock<std::convert::Infallible> for InfalliblePort<'_, T> {
    fn turn(&self) -> Result<u64, std::convert::Infallible> {
        Ok(self.0.turn())
    }
}

pub trait TryClock<E>: TryWorldClock<E> {
    fn hero_act(&mut self) -> Result<(), E>;
    fn start_corpse_timer(
        &mut self,
        target: ObjectTimerTarget,
        kind: ObjectTimerKind,
        due: Option<u64>,
        objects: &mut impl TryObjects<E>,
    ) -> Result<Result<u64, ObjectTimerError>, E>;
    fn cancel_corpse_timers(&mut self, target: ObjectTimerTarget) -> Result<(), E>;
}
impl<T: Clock> TryClock<std::convert::Infallible> for InfalliblePort<'_, T> {
    fn hero_act(&mut self) -> Result<(), std::convert::Infallible> {
        self.0.hero_act();
        Ok(())
    }
    fn start_corpse_timer(
        &mut self,
        target: ObjectTimerTarget,
        kind: ObjectTimerKind,
        due: Option<u64>,
        objects: &mut impl TryObjects<std::convert::Infallible>,
    ) -> Result<Result<u64, ObjectTimerError>, std::convert::Infallible> {
        Ok(self
            .0
            .start_corpse_timer(target, kind, due, &LegacyObjects(objects)))
    }
    fn cancel_corpse_timers(
        &mut self,
        target: ObjectTimerTarget,
    ) -> Result<(), std::convert::Infallible> {
        self.0.cancel_corpse_timers(target);
        Ok(())
    }
}

pub trait TryHeroFacts<E> {
    fn role_id(&self) -> Result<Option<RoleId>, E>;
    fn role_name(&self) -> Result<&str, E>;
    fn race(&self) -> Result<RaceId, E>;
    fn alignment_record(&self) -> Result<i32, E>;
}
impl<T: HeroFacts> TryHeroFacts<std::convert::Infallible> for InfalliblePort<'_, T> {
    fn role_id(&self) -> Result<Option<RoleId>, std::convert::Infallible> {
        Ok(self.0.role_id())
    }
    fn role_name(&self) -> Result<&str, std::convert::Infallible> {
        Ok(self.0.role_name())
    }
    fn race(&self) -> Result<RaceId, std::convert::Infallible> {
        Ok(self.0.race())
    }
    fn alignment_record(&self) -> Result<i32, std::convert::Infallible> {
        Ok(self.0.alignment_record())
    }
}

pub trait TryHero<E>: TryHeroFacts<E> {
    fn polymorphed(&self) -> Result<bool, E>;
    fn adjust_alignment(&mut self, delta: i32, limit: i32) -> Result<(), E>;
    fn record_kill(&mut self) -> Result<(), E>;
    fn quest_leader_killed(&self) -> Result<bool, E>;
    fn quest_progress(&self) -> Result<QuestProgress, E>;
    fn mark_quest_leader_killed(&mut self) -> Result<(), E>;
    fn defeat_quest_nemesis(&mut self) -> Result<(), E>;
    fn adjust_favor(&mut self, delta: i32) -> Result<(), E>;
    fn adjust_luck(&mut self, delta: i32) -> Result<(), E>;
    fn clear_divine_protection(&mut self) -> Result<(), E>;
    fn credit_experience(&mut self, experience: u64) -> Result<(), E>;
    fn experience(&self) -> Result<u64, E>;
    fn level_max(&self) -> Result<u32, E>;
    fn constitution(&self) -> Result<i32, E>;
    fn record_level_gain(
        &mut self,
        from: u32,
        hp: i32,
        energy: i32,
        experience: u64,
    ) -> Result<(), E>;
}
impl<T: Hero> TryHero<std::convert::Infallible> for InfalliblePort<'_, T> {
    fn polymorphed(&self) -> Result<bool, std::convert::Infallible> {
        Ok(self.0.polymorphed())
    }
    fn adjust_alignment(&mut self, delta: i32, limit: i32) -> Result<(), std::convert::Infallible> {
        self.0.adjust_alignment(delta, limit);
        Ok(())
    }
    fn record_kill(&mut self) -> Result<(), std::convert::Infallible> {
        self.0.record_kill();
        Ok(())
    }
    fn quest_leader_killed(&self) -> Result<bool, std::convert::Infallible> {
        Ok(self.0.quest_leader_killed())
    }
    fn quest_progress(&self) -> Result<QuestProgress, std::convert::Infallible> {
        Ok(self.0.quest_progress())
    }
    fn mark_quest_leader_killed(&mut self) -> Result<(), std::convert::Infallible> {
        self.0.mark_quest_leader_killed();
        Ok(())
    }
    fn defeat_quest_nemesis(&mut self) -> Result<(), std::convert::Infallible> {
        self.0.defeat_quest_nemesis();
        Ok(())
    }
    fn adjust_favor(&mut self, delta: i32) -> Result<(), std::convert::Infallible> {
        self.0.adjust_favor(delta);
        Ok(())
    }
    fn adjust_luck(&mut self, delta: i32) -> Result<(), std::convert::Infallible> {
        self.0.adjust_luck(delta);
        Ok(())
    }
    fn clear_divine_protection(&mut self) -> Result<(), std::convert::Infallible> {
        self.0.clear_divine_protection();
        Ok(())
    }
    fn credit_experience(&mut self, experience: u64) -> Result<(), std::convert::Infallible> {
        self.0.credit_experience(experience);
        Ok(())
    }
    fn experience(&self) -> Result<u64, std::convert::Infallible> {
        Ok(self.0.experience())
    }
    fn level_max(&self) -> Result<u32, std::convert::Infallible> {
        Ok(self.0.level_max())
    }
    fn constitution(&self) -> Result<i32, std::convert::Infallible> {
        Ok(self.0.constitution())
    }
    fn record_level_gain(
        &mut self,
        from: u32,
        hp: i32,
        energy: i32,
        experience: u64,
    ) -> Result<(), std::convert::Infallible> {
        self.0.record_level_gain(from, hp, energy, experience);
        Ok(())
    }
}

/// Reverse adapter used only by an infallible clock's borrowed timer API.
struct LegacyObjects<'a, T>(&'a mut T);
fn infallible<T>(value: Result<T, std::convert::Infallible>) -> T {
    match value {
        Ok(value) => value,
        Err(error) => match error {},
    }
}
impl<T: TryObjects<std::convert::Infallible>> ObjectFacts for LegacyObjects<'_, T> {
    fn carried(&self, actor: ActorId) -> Vec<ItemId> {
        infallible(self.0.carried(actor))
    }
    fn object(&self, id: ItemId) -> Option<SelectedObject> {
        infallible(self.0.object(id))
    }
    fn floor_kind(&self, coord: Coord, kind: ItemKindId, ruleset: &Ruleset) -> bool {
        infallible(self.0.floor_kind(coord, kind, ruleset))
    }
    fn has_amulet(&self, actor: ActorId) -> bool {
        infallible(self.0.has_amulet(actor))
    }
}
impl<T: TryObjects<std::convert::Infallible>> Objects for LegacyObjects<'_, T> {
    fn spend_positive_charge(&mut self, id: ItemId, remaining: i8) {
        infallible(self.0.spend_positive_charge(id, remaining))
    }
    fn insert_generated(&mut self, item: ItemRecord) -> ItemId {
        infallible(self.0.insert_generated(item))
    }
    fn timer_target_exists(&self, target: ObjectTimerTarget) -> bool {
        infallible(self.0.timer_target_exists(target))
    }
    fn frozen(&self, id: ItemId) -> bool {
        infallible(self.0.frozen(id))
    }
}

/// Semantic requests preserve the original rand types, ranges and draw order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WandDraw {
    Below(u32),
    SignedDie(i32),
    Monster(nethacked_data::monsters::MonsterDraw),
}
impl WandDraw {
    pub fn sample(self, rng: &mut impl rand::Rng) -> u32 {
        match self {
            Self::Below(n) => rng.random_range(0..n),
            Self::SignedDie(n) => rng.random_range(1..=n) as u32,
            Self::Monster(draw) => draw.sample(rng),
        }
    }
}
pub trait Random<E> {
    fn sample(&mut self, draw: WandDraw) -> Result<u32, E>;
}
impl<T: rand::Rng> Random<std::convert::Infallible> for InfalliblePort<'_, T> {
    fn sample(&mut self, draw: WandDraw) -> Result<u32, std::convert::Infallible> {
        Ok(draw.sample(self.0))
    }
}

impl<T: ObjectFacts> ObjectFacts for &T {
    fn carried(&self, actor: ActorId) -> Vec<ItemId> {
        (*self).carried(actor)
    }
    fn object(&self, id: ItemId) -> Option<SelectedObject> {
        (*self).object(id)
    }
    fn floor_kind(&self, coord: Coord, kind: ItemKindId, ruleset: &Ruleset) -> bool {
        (*self).floor_kind(coord, kind, ruleset)
    }
    fn has_amulet(&self, actor: ActorId) -> bool {
        (*self).has_amulet(actor)
    }
}

impl<T: GeographyFacts> GeographyFacts for &T {
    fn tile(&self, coord: Coord) -> Tile {
        (*self).tile(coord)
    }
    fn trajectory(&self, origin: Coord, direction: Direction, energy: u32) -> Vec<Coord> {
        (*self).trajectory(origin, direction, energy)
    }
    fn engraving(&self, coord: Coord) -> Option<Engraving> {
        (*self).engraving(coord)
    }
}

impl<T: WorldClock> WorldClock for &T {
    fn turn(&self) -> u64 {
        (*self).turn()
    }
}

impl<T: HeroFacts> HeroFacts for &T {
    fn role_id(&self) -> Option<RoleId> {
        (*self).role_id()
    }
    fn role_name(&self) -> &str {
        (*self).role_name()
    }
    fn race(&self) -> RaceId {
        (*self).race()
    }
    fn alignment_record(&self) -> i32 {
        (*self).alignment_record()
    }
}
