//! Bounded refusal witnesses for the shared algorithm, with separate ports.
use nethacked_arena::{
    ActorId, ActorRecord, ActorStorage, ItemDefinition, ItemId, ItemLocation, ItemRecord,
    ItemStorage,
};
use nethacked_core::{engraving::Engraving, QuestProgress, QuestState};
use nethacked_data::{
    ruleset::{MonsterDef, Ruleset},
    ItemKindId, RaceId, RoleId,
};
use nethacked_sim::{
    actions::wand::{self, ports::*, Selection, WandError, WandFailure},
    object_timers::*,
    GameEvent, SimulationWorld,
};
use nethacked_types::{BranchId, Buc, Coord, Direction, Locale, Tile};
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use sha2::{Digest, Sha256};
use std::{cell::RefCell, rc::Rc, sync::Arc};
const ORIGIN: Coord = Coord::new_unchecked(20, 10);
const TARGET: Coord = Coord::new_unchecked(21, 10);
type Error = usize;
struct State {
    actors: ActorStorage,
    items: ItemStorage,
    level: nethacked_dungeon::DungeonLevel,
    rng: ChaCha8Rng,
    turn: u64,
    align: i32,
    exp: u64,
    kills: u32,
    quest: QuestState,
    favor: i32,
    luck: i32,
    protection: u32,
    level_max: u32,
    increments: Vec<(u32, i32, i32, u64)>,
    timers: Vec<(ObjectTimerTarget, ObjectTimerKind, u64)>,
}
impl State {
    fn digest(&self) -> [u8; 32] {
        let json = serde_json::to_vec(&(
            &self.actors,
            &self.items,
            &self.level,
            &self.rng,
            self.turn,
            self.align,
            self.exp,
            self.kills,
            &self.quest,
            self.favor,
            self.luck,
            self.protection,
            self.level_max,
            &self.increments,
            &self.timers,
        ))
        .unwrap();
        Sha256::digest(json).into()
    }
}
#[derive(Default)]
struct Trace {
    fail: Option<usize>,
    calls: Vec<(&'static str, [u8; 32])>,
    draws: Vec<WandDraw>,
    timer_error: bool,
}
#[derive(Clone)]
struct Port {
    state: Rc<RefCell<State>>,
    trace: Rc<RefCell<Trace>>,
    role: Option<RoleId>,
    race: RaceId,
}
impl Port {
    fn enter(&self, name: &'static str) -> Result<(), Error> {
        let mut trace = self.trace.borrow_mut();
        let index = trace.calls.len();
        trace.calls.push((name, self.state.borrow().digest()));
        if trace.fail == Some(index) {
            Err(index)
        } else {
            Ok(())
        }
    }
}
impl TryObjectFacts<Error> for Port {
    fn carried(&self, actor: ActorId) -> Result<Vec<ItemId>, Error> {
        self.enter("objects.carried")?;
        Ok(self
            .state
            .borrow()
            .items
            .iter()
            .filter_map(|(id, item)| {
                (item.location == ItemLocation::CarriedBy(actor)).then_some(id)
            })
            .collect())
    }
    fn object(&self, id: ItemId) -> Result<Option<SelectedObject>, Error> {
        self.enter("objects.object")?;
        Ok(self
            .state
            .borrow()
            .items
            .get(id)
            .map(|item| SelectedObject {
                class: item.class,
                name: item.name.clone(),
                definition: item.definition.clone(),
                buc: item.buc,
                enchantment: item.enchantment,
            }))
    }
    fn floor_kind(&self, coord: Coord, kind: ItemKindId, ruleset: &Ruleset) -> Result<bool, Error> {
        self.enter("objects.floor_kind")?;
        Ok(self.state.borrow().items.iter().any(|(_, item)| {
            item.location == ItemLocation::Floor(coord)
                && ruleset.item(&item.name).is_some_and(|d| d.id == Some(kind))
        }))
    }
    fn has_amulet(&self, actor: ActorId) -> Result<bool, Error> {
        self.enter("objects.has_amulet")?;
        Ok(self.state.borrow().items.iter().any(|(_, item)| {
            item.location == ItemLocation::CarriedBy(actor) && item.name == "Amulet of Yendor"
        }))
    }
}
impl TryObjects<Error> for Port {
    fn spend_positive_charge(&mut self, id: ItemId, remaining: i8) -> Result<(), Error> {
        self.enter("objects.spend_positive_charge")?;

        if let Some(item) = self.state.borrow_mut().items.get_mut(id) {
            item.enchantment = remaining;
        };
        Ok(())
    }
    fn insert_generated(&mut self, item: ItemRecord) -> Result<ItemId, Error> {
        self.enter("objects.insert_generated")?;
        Ok(self.state.borrow_mut().items.insert(item))
    }
    fn timer_target_exists(&self, target: ObjectTimerTarget) -> Result<bool, Error> {
        self.enter("objects.timer_target_exists")?;
        Ok(match target {
            ObjectTimerTarget::Active(id) => self.state.borrow().items.get(id).is_some(),
            _ => false,
        })
    }
    fn frozen(&self, id: ItemId) -> Result<bool, Error> {
        self.enter("objects.frozen")?;
        Ok({
            let state = self.state.borrow();
            state.items.get(id).is_some_and(|item| {
                nethacked_arena::frozen_in_ice_box(item, |parent| state.items.get(parent))
            })
        })
    }
}
impl TryActors<Error> for Port {
    fn actor(&self, id: ActorId) -> Result<Option<ActorRecord>, Error> {
        self.enter("actors.actor")?;
        Ok(self.state.borrow().actors.get(id).cloned())
    }
    fn at(&self, coord: Coord) -> Result<Option<ActorId>, Error> {
        self.enter("actors.at")?;
        Ok(self
            .state
            .borrow()
            .actors
            .iter()
            .find_map(|(id, a)| (a.coord == coord && !a.is_dead).then_some(id)))
    }
    fn damage(&mut self, id: ActorId, damage: u32) -> Result<Option<bool>, Error> {
        self.enter("actors.damage")?;
        Ok({
            let mut state = self.state.borrow_mut();
            state.actors.get_mut(id).map(|a| {
                a.hp = a.hp.saturating_sub(damage);
                a.is_dead = a.hp == 0;
                a.is_dead
            })
        })
    }
    fn polymorph(&mut self, id: ActorId, definition: &MonsterDef) -> Result<(), Error> {
        self.enter("actors.polymorph")?;

        if let Some(a) = self.state.borrow_mut().actors.get_mut(id) {
            a.species = Some(definition.actor_species());
            a.name = definition.name.clone();
            a.hp = definition.base_hp;
            a.max_hp = definition.max_hp;
            a.ac = definition.ac;
            a.speed = definition.speed;
            a.level = definition.level;
        };
        Ok(())
    }
    fn make_hostile(&mut self, id: ActorId) -> Result<(), Error> {
        self.enter("actors.make_hostile")?;

        if let Some(a) = self.state.borrow_mut().actors.get_mut(id) {
            a.is_peaceful = false;
        };
        Ok(())
    }
    fn guardians(
        &self,
        name: &str,
        include_sound: bool,
        ruleset: &Ruleset,
    ) -> Result<Vec<ActorId>, Error> {
        self.enter("actors.guardians")?;
        Ok(self
            .state
            .borrow()
            .actors
            .iter()
            .filter(|(_, a)| {
                !a.is_dead
                    && (a.name.eq_ignore_ascii_case(name)
                        || (include_sound
                            && ruleset.monster_for_actor(a).is_some_and(|d| {
                                d.msound == nethacked_data::MonsterSound::Guardian
                            })))
            })
            .map(|(id, _)| id)
            .collect())
    }
    fn spawn_initialized(&mut self, actor: ActorRecord) -> Result<ActorId, Error> {
        self.enter("actors.spawn_initialized")?;
        Ok(self.state.borrow_mut().actors.insert(actor))
    }
    fn force_hostile_malign(&mut self, id: ActorId, malign: i32) -> Result<(), Error> {
        self.enter("actors.force_hostile_malign")?;

        if let Some(a) = self.state.borrow_mut().actors.get_mut(id) {
            a.is_peaceful = false;
            a.malign = malign;
        };
        Ok(())
    }
    fn gain_level(&mut self, id: ActorId, hp: u32, level: u32) -> Result<(), Error> {
        self.enter("actors.gain_level")?;

        if let Some(a) = self.state.borrow_mut().actors.get_mut(id) {
            a.max_hp = a.max_hp.saturating_add(hp);
            a.hp = a.hp.saturating_add(hp);
            a.level = level;
        };
        Ok(())
    }
    fn genocided(&self, name: &str, glyph: char) -> Result<bool, Error> {
        self.enter("actors.genocided")?;
        Ok({
            let _ = (name, glyph);
            false
        })
    }
    fn genocide_filter(&self) -> Result<impl Fn(&str, char) -> bool + '_, Error> {
        self.enter("actors.genocide_filter")?;
        Ok(|_name: &str, _glyph: char| false)
    }
}
impl TryGeographyFacts<Error> for Port {
    fn tile(&self, coord: Coord) -> Result<Tile, Error> {
        self.enter("geography.tile")?;
        Ok(self.state.borrow().level.get_tile(coord).clone())
    }
    fn trajectory(
        &self,
        origin: Coord,
        direction: Direction,
        energy: u32,
    ) -> Result<Vec<Coord>, Error> {
        self.enter("geography.trajectory")?;
        Ok(nethacked_dungeon::trace_beam_path(
            &self.state.borrow().level,
            origin,
            direction,
            energy,
        ))
    }
    fn engraving(&self, coord: Coord) -> Result<Option<Engraving>, Error> {
        self.enter("geography.engraving")?;
        Ok(self.state.borrow().level.get_engraving(coord).cloned())
    }
}
impl TryGeography<Error> for Port {
    fn reveal_secret_door(&mut self, coord: Coord) -> Result<bool, Error> {
        self.enter("geography.reveal_secret_door")?;
        Ok({
            let mut s = self.state.borrow_mut();
            if matches!(s.level.get_tile(coord), Tile::SecretDoor { .. }) {
                s.level.get_tile_mut(coord).reveal_secret_door();
                true
            } else {
                false
            }
        })
    }
    fn collapse_drawbridge(&mut self, coord: Coord) -> Result<(), Error> {
        self.enter("geography.collapse_drawbridge")?;
        self.state.borrow_mut().level.set_tile(coord, Tile::Moat);
        Ok(())
    }
    fn break_door(&mut self, coord: Coord) -> Result<(), Error> {
        self.enter("geography.break_door")?;
        self.state
            .borrow_mut()
            .level
            .get_tile_mut(coord)
            .break_door();
        Ok(())
    }
    fn freeze_pool(&mut self, coord: Coord) -> Result<(), Error> {
        self.enter("geography.freeze_pool")?;
        self.state
            .borrow_mut()
            .level
            .set_tile(coord, Tile::Pool { frozen: true });
        Ok(())
    }
    fn erase_engraving(&mut self, coord: Coord) -> Result<(), Error> {
        self.enter("geography.erase_engraving")?;
        {
            self.state.borrow_mut().level.remove_engraving(coord);
        };
        Ok(())
    }
}
impl TryWorldClock<Error> for Port {
    fn turn(&self) -> Result<u64, Error> {
        self.enter("clock.turn")?;
        Ok(self.state.borrow().turn)
    }
}
impl TryClock<Error> for Port {
    fn hero_act(&mut self) -> Result<(), Error> {
        self.enter("clock.hero_act")?;
        {
            self.state.borrow_mut().turn += 1;
        };
        Ok(())
    }
    fn start_corpse_timer(
        &mut self,
        target: ObjectTimerTarget,
        kind: ObjectTimerKind,
        due: Option<u64>,
        objects: &mut impl TryObjects<Error>,
    ) -> Result<Result<u64, ObjectTimerError>, Error> {
        self.enter("clock.start_corpse_timer")?;
        let exists = objects.timer_target_exists(target)?;
        if self.trace.borrow().timer_error {
            return Ok(Err(ObjectTimerError::Duplicate));
        }
        if !exists {
            return Ok(Err(ObjectTimerError::MissingObject));
        }
        let Some(due) = due else {
            return Ok(Err(ObjectTimerError::ClockOverflow));
        };
        let mut state = self.state.borrow_mut();
        state.timers.push((target, kind, due));
        Ok(Ok(state.timers.len() as u64))
    }
    fn cancel_corpse_timers(&mut self, target: ObjectTimerTarget) -> Result<(), Error> {
        self.enter("clock.cancel_corpse_timers")?;
        {
            self.state
                .borrow_mut()
                .timers
                .retain(|(t, _, _)| *t != target);
        };
        Ok(())
    }
}
impl TryHeroFacts<Error> for Port {
    fn role_id(&self) -> Result<Option<RoleId>, Error> {
        self.enter("hero.role_id")?;
        Ok(self.role)
    }
    fn role_name(&self) -> Result<&str, Error> {
        self.enter("hero.role_name")?;
        Ok("valkyrie")
    }
    fn race(&self) -> Result<RaceId, Error> {
        self.enter("hero.race")?;
        Ok(self.race)
    }
    fn alignment_record(&self) -> Result<i32, Error> {
        self.enter("hero.alignment_record")?;
        Ok(self.state.borrow().align)
    }
}
impl TryHero<Error> for Port {
    fn polymorphed(&self) -> Result<bool, Error> {
        self.enter("hero.polymorphed")?;
        Ok(false)
    }
    fn adjust_alignment(&mut self, delta: i32, limit: i32) -> Result<(), Error> {
        self.enter("hero.adjust_alignment")?;
        {
            let mut s = self.state.borrow_mut();
            s.align = nethacked_core::adjalign(s.align, delta, limit);
        };
        Ok(())
    }
    fn record_kill(&mut self) -> Result<(), Error> {
        self.enter("hero.record_kill")?;
        {
            self.state.borrow_mut().kills += 1;
        };
        Ok(())
    }
    fn quest_leader_killed(&self) -> Result<bool, Error> {
        self.enter("hero.quest_leader_killed")?;
        Ok(self.state.borrow().quest.killed_leader)
    }
    fn quest_progress(&self) -> Result<QuestProgress, Error> {
        self.enter("hero.quest_progress")?;
        Ok(self.state.borrow().quest.progress)
    }
    fn mark_quest_leader_killed(&mut self) -> Result<(), Error> {
        self.enter("hero.mark_quest_leader_killed")?;
        {
            self.state.borrow_mut().quest.killed_leader = true;
        };
        Ok(())
    }
    fn defeat_quest_nemesis(&mut self) -> Result<(), Error> {
        self.enter("hero.defeat_quest_nemesis")?;
        {
            nethacked_core::attack_nemesis(&mut self.state.borrow_mut().quest, 9999);
        };
        Ok(())
    }
    fn adjust_favor(&mut self, delta: i32) -> Result<(), Error> {
        self.enter("hero.adjust_favor")?;
        {
            self.state.borrow_mut().favor += delta;
        };
        Ok(())
    }
    fn adjust_luck(&mut self, delta: i32) -> Result<(), Error> {
        self.enter("hero.adjust_luck")?;
        {
            self.state.borrow_mut().luck += delta;
        };
        Ok(())
    }
    fn clear_divine_protection(&mut self) -> Result<(), Error> {
        self.enter("hero.clear_divine_protection")?;
        {
            self.state.borrow_mut().protection = 0;
        };
        Ok(())
    }
    fn credit_experience(&mut self, experience: u64) -> Result<(), Error> {
        self.enter("hero.credit_experience")?;
        {
            self.state.borrow_mut().exp += experience;
        };
        Ok(())
    }
    fn experience(&self) -> Result<u64, Error> {
        self.enter("hero.experience")?;
        Ok(self.state.borrow().exp)
    }
    fn level_max(&self) -> Result<u32, Error> {
        self.enter("hero.level_max")?;
        Ok(self.state.borrow().level_max)
    }
    fn constitution(&self) -> Result<i32, Error> {
        self.enter("hero.constitution")?;
        Ok(16)
    }
    fn record_level_gain(
        &mut self,
        from: u32,
        hp: i32,
        energy: i32,
        experience: u64,
    ) -> Result<(), Error> {
        self.enter("hero.record_level_gain")?;
        {
            let mut s = self.state.borrow_mut();
            s.exp = experience;
            s.level_max = s.level_max.max(from + 1);
            s.increments.push((from, hp, energy, experience));
        };
        Ok(())
    }
}

impl Random<Error> for Port {
    fn sample(&mut self, draw: WandDraw) -> Result<u32, Error> {
        self.enter("random.sample")?;
        self.trace.borrow_mut().draws.push(draw);
        Ok(draw.sample(&mut self.state.borrow_mut().rng))
    }
}
struct Fixture {
    port: Port,
    ruleset: Arc<Ruleset>,
    player: ActorId,
    wand: ItemId,
}
impl Fixture {
    fn new(wand_name: &str, target_name: Option<&str>, creation: bool) -> Self {
        let mut world = SimulationWorld::new_with_seed(53);
        world.arena.actors.retain(|id, _| id == world.player_id);
        world.arena.items.clear();
        world.level = nethacked_dungeon::DungeonLevel::new_solid(Tile::Room);
        world.arena.actors[world.player_id].coord = ORIGIN;
        world.arena.actors[world.player_id].level = 2;
        world.arena.actors[world.player_id].alignment = nethacked_types::Alignment::Lawful;
        let mut ruleset = (*world.ruleset).clone();
        // Small source-faithful factory fixture: no huge reservoir records.
        if creation {
            let original = ruleset
                .original_monster_for_role(world.role_id.unwrap())
                .unwrap()
                .name
                .clone();
            ruleset
                .monsters
                .retain(|d| d.name == "newt" || d.name == original);
            let def = ruleset
                .monsters
                .iter_mut()
                .find(|d| d.name == "newt")
                .unwrap();
            def.sample_hp = true;
            def.maligntyp = 1;
            def.always_hostile = false;
            def.peaceful_by_default = false;
        } else {
            ruleset
                .monsters
                .retain(|d| matches!(d.name.as_str(), "newt" | "troll" | "human"));
        }
        ruleset.reindex();
        let mut wand = ruleset
            .create_item_record(
                wand_name,
                ItemLocation::CarriedBy(world.player_id),
                Buc::Uncursed,
            )
            .unwrap();
        wand.enchantment = 3;
        let wid = world.arena.spawn_item(wand);
        if let Some(name) = target_name {
            let mut a = world
                .ruleset
                .create_monster_record(name, TARGET, &mut world.rng)
                .unwrap();
            a.hp = 1;
            a.max_hp = 1;
            a.speed = 0;
            a.movement = 0;
            world.arena.spawn_actor(a);
        }
        if wand_name == "wand of striking" {
            world
                .level
                .set_tile(TARGET, Tile::Drawbridge { open: false });
        }
        if wand_name == "wand of cold" {
            world.level.set_tile(TARGET, Tile::Pool { frozen: false });
        }
        let state = State {
            actors: world.arena.actors,
            items: world.arena.items,
            level: world.level,
            rng: ChaCha8Rng::seed_from_u64(100),
            turn: 0,
            align: 10,
            exp: 39,
            kills: 0,
            quest: world.quest_state,
            favor: 0,
            luck: 0,
            protection: 0,
            level_max: 2,
            increments: Vec::new(),
            timers: Default::default(),
        };
        Self {
            port: Port {
                state: Rc::new(RefCell::new(state)),
                trace: Default::default(),
                role: world.role_id,
                race: world.hero_race,
            },
            ruleset: Arc::new(ruleset),
            player: world.player_id,
            wand: wid,
        }
    }
    fn run(
        &self,
        selection: Selection,
        locale: Locale,
    ) -> Result<Vec<GameEvent>, WandFailure<Error>> {
        let session = Session {
            ruleset: &self.ruleset,
            locale,
            branch: BranchId::DungeonsOfDoom,
            depth: 4,
        };
        wand::try_zap(
            selection,
            self.player,
            Direction::East,
            6,
            &mut self.port.clone(),
            &mut self.port.clone(),
            &mut self.port.clone(),
            &mut self.port.clone(),
            &mut self.port.clone(),
            &mut self.port.clone(),
            &session,
        )
    }
    fn selected(&self) -> Selection {
        Selection::Selected {
            index: 0,
            expected: Some(self.wand),
        }
    }
}
fn json(events: &[GameEvent]) -> serde_json::Value {
    serde_json::to_value(events).unwrap()
}

#[test]
fn every_observed_authority_and_nested_draw_refusal_stops_with_exact_prefix() {
    let mut classes = std::collections::BTreeSet::new();
    let mut hp_draw = false;
    let mut peace_draw = false;
    let mut corpse_draw = false;
    let mut advance_draw = false;
    for (wand, target, creation) in [
        ("wand of striking", None, false),
        ("wand of cold", None, false),
        ("wand of death", Some("newt"), false),
        ("wand of death", Some("troll"), false),
        ("wand of create monster", None, true),
    ] {
        let success = Fixture::new(wand, target, creation);
        let events = success.run(success.selected(), Locale::En).unwrap();
        let calls = success.port.trace.borrow().calls.clone();
        let draws = success.port.trace.borrow().draws.clone();
        assert!(calls.len() < 400, "fixture must stay bounded");
        advance_draw |= draws.iter().any(|d| matches!(d, WandDraw::SignedDie(_)));
        hp_draw |= draws.iter().any(|d| matches!(d, WandDraw::Monster(_)));
        peace_draw |= creation && draws.contains(&WandDraw::Below(16 + 10));
        corpse_draw |= target.is_some() && draws.iter().any(|d| matches!(d, WandDraw::Below(_)));
        for (index, (name, digest)) in calls.iter().enumerate() {
            classes.insert(name.split('.').next().unwrap());
            let failing = Fixture::new(wand, target, creation);
            failing.port.trace.borrow_mut().fail = Some(index);
            let failure = failing.run(failing.selected(), Locale::En).unwrap_err();
            assert_eq!(failure.error, WandError::Authority(index), "{wand}: {name}");
            let trace = failing.port.trace.borrow();
            assert_eq!(trace.calls, calls[..=index], "no later operations: {name}");
            assert_eq!(
                failing.port.state.borrow().digest(),
                *digest,
                "applied state and RNG prefix: {name}"
            );
            assert_eq!(trace.draws, draws[..trace.draws.len()]);
            assert_eq!(
                json(&failure.events),
                json(&events[..failure.events.len()]),
                "event prefix: {name}"
            );
            if *name == "clock.hero_act" {
                assert_eq!(json(&failure.events), json(&events));
            }
            if *name == "hero.record_kill" {
                assert!(failure
                    .events
                    .iter()
                    .any(|e| matches!(e, GameEvent::AttackLanded { lethal: true, .. })));
            }
        }
    }
    assert_eq!(
        classes,
        ["actors", "clock", "geography", "hero", "objects", "random"]
            .into_iter()
            .collect()
    );
    assert!(
        hp_draw && peace_draw && corpse_draw && advance_draw,
        "hp={hp_draw},peace={peace_draw},corpse={corpse_draw},advance={advance_draw}"
    );
}

#[test]
fn timer_domain_failure_preserves_published_corpse_and_events() {
    let f = Fixture::new("wand of death", Some("newt"), false);
    f.port.trace.borrow_mut().timer_error = true;
    let failure = f.run(f.selected(), Locale::En).unwrap_err();
    assert_eq!(failure.error, WandError::Timer(ObjectTimerError::Duplicate));
    assert_eq!(f.port.state.borrow().items.len(), 2);
    assert_eq!(f.port.state.borrow().items[f.wand].enchantment, 2);
    assert_eq!(f.port.state.borrow().turn, 0);
    assert!(failure
        .events
        .iter()
        .any(|e| matches!(e, GameEvent::AttackLanded { lethal: true, .. })));
    assert_eq!(
        f.port.trace.borrow().calls.last().unwrap().0,
        "objects.timer_target_exists"
    );
}

#[test]
fn ordinary_selection_and_locale_results_use_no_entropy() {
    for case in 0..7 {
        let mut f = Fixture::new("wand of striking", None, false);
        let mut selection = f.selected();
        let mut locale = Locale::En;
        match case {
            0 => {
                f.port.state.borrow_mut().actors.remove(f.player);
                selection = Selection::First;
            }
            1 => {
                selection = Selection::Selected {
                    index: u32::MAX,
                    expected: None,
                }
            }
            2 => {
                let foreign = f.port.state.borrow_mut().items.insert(
                    f.ruleset
                        .create_item_record("rock", ItemLocation::Floor(TARGET), Buc::Uncursed)
                        .unwrap(),
                );
                selection = Selection::Selected {
                    index: 0,
                    expected: Some(foreign),
                };
            }
            3 => f.port.state.borrow_mut().items[f.wand].class = nethacked_types::ItemClass::Weapon,
            4 => {
                f.port.state.borrow_mut().items[f.wand].definition =
                    Some(ItemDefinition::Custom("unavailable fixture".into()))
            }
            5 => {
                f.port.state.borrow_mut().items[f.wand].enchantment = 0;
                locale = Locale::Uk;
            }
            _ => {
                f.ruleset = Arc::new({
                    let mut r = (*f.ruleset).clone();
                    r.monsters.clear();
                    r
                });
                f.port.state.borrow_mut().items[f.wand].name = "wand of create monster".into();
                f.port.state.borrow_mut().items[f.wand].definition = None;
            }
        }
        let before = f.port.state.borrow().rng.clone().next_u64();
        let events = f.run(selection, locale).unwrap();
        assert!(f.port.trace.borrow().draws.is_empty());
        assert_eq!(before, f.port.state.borrow().rng.clone().next_u64());
        if case == 0 {
            assert!(events.is_empty());
        }
        if case == 5 {
            assert_eq!(
                json(&events),
                json(&[GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::wand_empty(Locale::Uk).into()
                }])
            );
        }
    }
}

#[test]
fn peace_refusals_and_short_circuit_preserve_prescribed_draws() {
    use nethacked_core::peace::{try_peace_minded, PeaceMindedInput};
    let input = PeaceMindedInput::default();
    for fail in 0..2 {
        let mut calls = Vec::new();
        let result = try_peace_minded(&input, |bound| {
            let index = calls.len();
            calls.push(bound);
            if index == fail {
                Err(index)
            } else {
                Ok(1)
            }
        });
        assert_eq!(result, Err(fail));
        assert_eq!(calls, [16, 2][..=fail]);
    }
    let mut calls = 0;
    assert_eq!(
        try_peace_minded(&input, |_| {
            calls += 1;
            Ok::<_, usize>(0)
        }),
        Ok(false)
    );
    assert_eq!(calls, 1);
    assert_eq!(
        try_peace_minded(
            &PeaceMindedInput {
                always_peaceful: true,
                ..input
            },
            |_| Err(99)
        ),
        Ok(true)
    );
}
