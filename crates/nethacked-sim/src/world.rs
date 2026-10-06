use rand::Rng;
use std::sync::Arc;

use nethacked_arena::{ActorId, EntityArena, ItemId, ItemLocation};
use nethacked_core::{
    armor_base_ac, armor_slot, energy::SchedulerState, find_ac, nutrition::hunger_of_nutrition,
    HungerState, SpellKind,
};
use nethacked_data::ruleset::{Ruleset, RulesetRef};
use nethacked_data::{CharacterConfig, ItemKindId, MonsterSpeciesId, RoleId};
use nethacked_dungeon::{generate_dungeon_level, DungeonLevel, RoomType};
use nethacked_types::{Buc, Coord, ItemClass};
/// Constitution used for starvation thresholds (`eat.c:3437`); the sim has no
/// Con attribute yet, so every hero uses this documented default.
pub const DEFAULT_PLAYER_CON: i32 = 10;

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

use crate::events::GameEvent;

/// Stored state of a dungeon level when player travels to other floors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredLevel {
    pub level: DungeonLevel,
    pub monsters: Vec<(nethacked_arena::ActorId, nethacked_arena::ActorRecord)>,
    pub items: Vec<(ItemId, nethacked_arena::ItemRecord)>,
    pub unpaid_items: Vec<(ItemId, u32)>,
}

fn default_ruleset() -> Arc<Ruleset> {
    Ruleset::vanilla()
}

fn default_ruleset_ref() -> RulesetRef {
    RulesetRef::vanilla()
}

/// Error returned when loading a simulation save fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    Json(String),
    RulesetMismatch {
        expected: Box<RulesetRef>,
        found: Box<RulesetRef>,
    },
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Json(msg) => write!(f, "JSON load error: {msg}"),
            LoadError::RulesetMismatch { expected, found } => {
                write!(
                    f,
                    "Ruleset mismatch: expected {expected:?}, found {found:?}"
                )
            }
        }
    }
}

impl std::error::Error for LoadError {}

/// The complete, deterministic game simulation world.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationWorld {
    #[serde(skip, default = "default_ruleset")]
    pub ruleset: Arc<Ruleset>,
    #[serde(default = "default_ruleset_ref")]
    pub ruleset_ref: RulesetRef,
    pub level: DungeonLevel,
    pub levels: Vec<DungeonLevel>,
    pub current_branch: nethacked_types::BranchId,
    pub stored_levels: Vec<((nethacked_types::BranchId, usize), StoredLevel)>,
    pub depth: usize,
    pub arena: EntityArena,
    pub player_id: ActorId,
    pub wielded_item: Option<ItemId>,
    pub scheduler: SchedulerState,
    /// Unpaid shop merchandise as `(item, base cost)`; see [`Self::get_unpaid_cost`].
    pub unpaid_items: Vec<(ItemId, u32)>,
    pub player_gold: u32,
    pub player_nutrition: i32,
    pub player_pw: u32,
    pub player_max_pw: u32,
    pub known_spells: Vec<(SpellKind, u32)>,
    pub divine_state: nethacked_types::DivineState,
    pub divine_protection: u32,
    /// Temple priest `cheapskate_count` (C `priest.c:562`); one counter for all priests.
    #[serde(default)]
    pub priest_cheapskate: u32,
    pub player_luck: i32,
    pub hero: nethacked_types::Hero,
    pub locale: nethacked_types::Locale,
    pub bones_storage: Vec<nethacked_types::BonesData>,
    #[serde(default)]
    pub candelabrum_state: nethacked_core::CandelabrumState,
    #[serde(default)]
    pub ritual_progress: nethacked_core::RitualProgress,
    #[serde(default)]
    pub vibrating_square: Option<Coord>,
    #[serde(default)]
    pub quest_state: nethacked_core::QuestState,
    #[serde(default)]
    pub alignment_record: i32,
    /// Hero race (C `gu.urace`), used by `peace_minded`'s race rules
    /// (`race_peaceful`/`race_hostile`, makemon.c:2283-2286).
    #[serde(default)]
    pub hero_race: nethacked_data::RaceId,
    /// C `context.mysteryforce`: decay counter of the Mysterious Force
    /// (`do.c:1543,1563`); grows by `rn2(diff + 2)` each time it triggers.
    #[serde(default)]
    pub mysterious_force_count: u32,
    #[serde(default)]
    pub role_name: String,
    #[serde(default = "default_rng")]
    pub rng: ChaCha8Rng,
    pub seed: u64,
    pub event_log: Vec<GameEvent>,
    #[serde(default)]
    pub max_event_log_len: Option<usize>,
    #[serde(default)]
    pub genocide_registry: nethacked_types::GenocideRegistry,
    #[serde(default)]
    pub conducts: nethacked_types::ConductTracker,
    /// Hero attributes and experience (C `u.acurr`, `u.uexp`, `u.uhpinc`).
    #[serde(default)]
    pub progress: crate::progress::HeroProgress,
}

/// The sim's fallback starting alignment record when a role definition is missing
/// (C starts at `urole.initrecord`, attrib.c:1094; roles default to 10).
pub const INITIAL_ALIGNMENT_RECORD: i32 = 10;

/// Default maximum length of the sliding window for `event_log` (prevents unbounded memory growth).
pub const DEFAULT_MAX_EVENT_LOG_LEN: usize = 10_000;

pub fn default_rng() -> ChaCha8Rng {
    ChaCha8Rng::seed_from_u64(0)
}

impl SimulationWorld {
    /// Default maximum length of the sliding window for `event_log` (prevents unbounded memory growth).
    pub const DEFAULT_MAX_EVENT_LOG_LEN: usize = DEFAULT_MAX_EVENT_LOG_LEN;

    /// Remove an actor, leaving anything it carried on the floor where it stood.
    pub(crate) fn remove_actor_dropping_items(&mut self, id: ActorId) {
        let Some(coord) = self.arena.actors.get(id).map(|a| a.coord) else {
            return;
        };
        for item_id in self.arena.items_carried_by(id) {
            if let Some(item) = self.arena.items.get_mut(item_id) {
                item.location = ItemLocation::Floor(coord);
            }
        }
        self.arena.actors.remove(id);
    }

    pub(crate) fn actor_is_genocided(&self, name: &str) -> bool {
        let lower = name.to_lowercase();
        let class = self.ruleset.monster_class_of(name);
        self.genocide_registry.genocided_species.contains(&lower)
            || class
                .map(|c| self.genocide_registry.genocided_classes.contains(&c))
                .unwrap_or(false)
    }

    /// Initialize a new deterministic simulation world with a custom character configuration and ruleset.
    pub fn new_with_character_and_ruleset(
        seed: u64,
        config: CharacterConfig,
        ruleset: Arc<Ruleset>,
        ruleset_ref: RulesetRef,
    ) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let level = generate_dungeon_level(&mut rng);
        let mut arena = EntityArena::new();

        // Spawn player with character configuration (role, race, starting items)
        let (player_id, starting_items) =
            ruleset.spawn_player_character(&config, level.stairs_up, &mut arena);

        let mut unpaid_items = Vec::new();
        let player_gold = if config.role == RoleId::Tourist {
            200
        } else {
            50
        };

        // Nutrition & Mana by role
        let player_nutrition = 900i32;
        let (player_pw, player_max_pw, known_spells) = match config.role {
            RoleId::Wizard => (25, 25, vec![(SpellKind::ForceBolt, 20000)]),
            RoleId::Healer => (20, 20, vec![(SpellKind::CureLightWounds, 20000)]),
            RoleId::Knight | RoleId::Monk => (10, 10, Vec::new()),
            _ => (5, 5, Vec::new()),
        };

        // Spawn starter food in player's pack
        if let Some(item_rec) = ruleset.create_item_record_by_id(
            ItemKindId::FOOD_RATION,
            ItemLocation::CarriedBy(player_id),
            Buc::Uncursed,
        ) {
            arena.spawn_item(item_rec);
        }

        let init_align = ruleset
            .role(config.role)
            .map(|r| r.initial_alignment_record)
            .unwrap_or(INITIAL_ALIGNMENT_RECORD);

        // Process rooms: spawn Shopkeeper and merchandise in Shop, monsters in Normal rooms
        for (i, room) in level.rooms.iter().enumerate() {
            match room.room_type {
                RoomType::Shop => {
                    // Spawn peaceful Shopkeeper
                    if let Some(mut sk) = ruleset
                        .create_monster_record_by_id(MonsterSpeciesId::SHOPKEEPER, room.center())
                    {
                        if let Some(sk_def) = ruleset.monster_by_id(MonsterSpeciesId::SHOPKEEPER) {
                            sk.malign = nethacked_core::calculate_malign(
                                sk_def.maligntyp,
                                config.alignment,
                                sk.is_peaceful,
                                false,
                                sk_def.peaceful_by_default,
                                sk_def.always_hostile,
                            );
                        }
                        arena.spawn_actor(sk);
                    }

                    // Spawn shop merchandise on floor
                    let items_to_sell = [
                        ItemKindId::POT_EXTRA_HEALING,
                        ItemKindId::SCR_IDENTIFY,
                        ItemKindId::LONG_SWORD,
                        ItemKindId::WAN_STRIKING,
                        ItemKindId::LEATHER_ARMOR,
                    ];
                    let mut off = 1;
                    for &kind in &items_to_sell {
                        let ix = (room.x1 + off).min(room.x2.saturating_sub(1));
                        let iy = (room.y1 + 1).min(room.y2.saturating_sub(1));
                        let ic = Coord::new_unchecked(ix, iy);
                        if ic != room.center() {
                            if let Some(item_rec) = ruleset.create_item_record_by_id(
                                kind,
                                ItemLocation::Floor(ic),
                                Buc::Uncursed,
                            ) {
                                let cost = ruleset.item_by_id(kind).map(|it| it.cost).unwrap_or(30);
                                let item_id = arena.spawn_item(item_rec);
                                unpaid_items.push((item_id, cost));
                            }
                        }
                        off += 1;
                    }
                }
                RoomType::Normal if i > 0 && i != level.rooms.len() - 1 => {
                    if let Some(mut goblin) =
                        ruleset.create_monster_record_by_id(MonsterSpeciesId::GOBLIN, room.center())
                    {
                        if let Some(gob_def) = ruleset.monster_by_id(MonsterSpeciesId::GOBLIN) {
                            let input = crate::peace::peace_input(
                                gob_def,
                                config.alignment,
                                config.race,
                                init_align,
                                false,
                            );
                            goblin.is_peaceful = crate::peace::roll_peace_minded(&input, &mut rng);
                            goblin.malign = nethacked_core::calculate_malign(
                                gob_def.maligntyp,
                                config.alignment,
                                goblin.is_peaceful,
                                false,
                                gob_def.peaceful_by_default,
                                gob_def.always_hostile,
                            );
                        }
                        arena.spawn_actor(goblin);
                    }
                }
                _ => {}
            }
        }

        // Spawn starting floor items near stairs from declarative item catalog
        let item_coord1 =
            Coord::new(level.stairs_up.x + 1, level.stairs_up.y).unwrap_or(level.stairs_up);
        if let Some(it) = ruleset.create_item_record_by_id(
            ItemKindId::SILVER_SABER,
            ItemLocation::Floor(item_coord1),
            Buc::Uncursed,
        ) {
            arena.spawn_item(it);
        }

        let item_coord2 =
            Coord::new(level.stairs_up.x, level.stairs_up.y + 1).unwrap_or(level.stairs_up);
        if let Some(it) = ruleset.create_item_record_by_id(
            ItemKindId::POT_HEALING,
            ItemLocation::Floor(item_coord2),
            Buc::Blessed,
        ) {
            arena.spawn_item(it);
        }

        let item_coord3 =
            Coord::new(level.stairs_up.x + 1, level.stairs_up.y + 1).unwrap_or(level.stairs_up);
        if let Some(it) = ruleset.create_item_record_by_id(
            ItemKindId::BAG_OF_HOLDING,
            ItemLocation::Floor(item_coord3),
            Buc::Uncursed,
        ) {
            arena.spawn_item(it);
        }

        // Auto-wield first starting weapon if any
        let wielded_item = starting_items.into_iter().find(|&id| {
            arena
                .items
                .get(id)
                .map(|i| i.class == ItemClass::Weapon)
                .unwrap_or(false)
        });

        let starting_skills = ruleset
            .role(config.role)
            .map(|r| r.skills.iter().copied().collect())
            .unwrap_or_else(|| {
                nethacked_data::starting_skills(config.role)
                    .into_iter()
                    .collect()
            });

        let mut sim = Self {
            ruleset,
            ruleset_ref,
            levels: vec![level.clone()],
            current_branch: nethacked_types::BranchId::DungeonsOfDoom,
            stored_levels: Vec::new(),
            depth: 1,
            level,
            player_id,
            wielded_item,
            scheduler: SchedulerState::new(12, 10),
            unpaid_items,
            player_gold,
            player_nutrition,
            player_pw,
            player_max_pw,
            known_spells,
            divine_state: nethacked_types::DivineState::default(),
            divine_protection: 0,
            priest_cheapskate: 0,
            player_luck: 0,
            hero: nethacked_types::Hero {
                base_hp: arena.actors[player_id].hp as i32,
                base_max_hp: arena.actors[player_id].max_hp as i32,
                polymorph: None,
                lycanthropy: None,
                afflictions: nethacked_types::AfflictionState::default(),
                skills: nethacked_types::SkillTree {
                    skills: starting_skills,
                    ..nethacked_types::SkillTree::default()
                },
                mount: None,
                quivered_item: None,
            },
            locale: nethacked_types::Locale::En,
            arena,
            bones_storage: Vec::new(),
            candelabrum_state: nethacked_core::CandelabrumState::empty(),
            ritual_progress: nethacked_core::RitualProgress::Uninitiated,
            vibrating_square: None,
            quest_state: nethacked_core::QuestState::default(),
            alignment_record: init_align,
            hero_race: config.race,
            mysterious_force_count: 0,
            role_name: format!("{:?}", config.role),
            rng,
            seed,
            event_log: Vec::new(),
            max_event_log_len: Some(DEFAULT_MAX_EVENT_LOG_LEN),
            genocide_registry: nethacked_types::GenocideRegistry::default(),
            conducts: nethacked_types::ConductTracker::default(),
            progress: crate::progress::HeroProgress::default(),
        };
        sim.init_hero_attributes();
        sim.recompute_hero_ac();
        sim
    }

    /// Initialize a new deterministic simulation world with a custom character configuration.
    pub fn new_with_character(seed: u64, config: CharacterConfig) -> Self {
        Self::new_with_character_and_ruleset(
            seed,
            config,
            Ruleset::vanilla(),
            RulesetRef::vanilla(),
        )
    }

    /// Serialize the simulation state to JSON string.
    pub fn to_save_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserialize a saved simulation world, validating ruleset reference compatibility.
    pub fn from_save_json(
        json: &str,
        rs: Arc<Ruleset>,
        rref: &RulesetRef,
    ) -> Result<Self, LoadError> {
        let mut world: SimulationWorld =
            serde_json::from_str(json).map_err(|e| LoadError::Json(e.to_string()))?;
        if &world.ruleset_ref != rref {
            return Err(LoadError::RulesetMismatch {
                expected: Box::new(rref.clone()),
                found: Box::new(world.ruleset_ref),
            });
        }
        world.ruleset = rs;
        Ok(world)
    }

    /// Returns the current coordinate of the player character.
    pub fn hero_coord(&self) -> Coord {
        self.arena
            .actors
            .get(self.player_id)
            .map(|a| a.coord)
            .unwrap_or(Coord::new_unchecked(1, 1))
    }

    /// Compute hero's current AC according to NetHack 5.0 C `find_ac(void)` (`do_wear.c:2473-2507`).
    ///
    /// Human hero has base AC 10 (`mons[u.umonnum].ac`, `do_wear.c:2475`).
    /// Worn armor pieces are gathered from the hero's carried items: for each
    /// [`nethacked_core::ArmorSlot`], at most one piece is counted as worn (the first carried item
    /// matching that slot wins; additional carried items in the same slot do not stack).
    /// Divine protection is subtracted as C `u.ublessed`.
    pub fn compute_hero_ac(&self) -> i32 {
        let mut worn_slots = std::collections::HashSet::new();
        let mut worn_armor = Vec::new();

        for item_id in self.arena.items_carried_by(self.player_id) {
            if let Some(item) = self.arena.items.get(item_id) {
                if item.class == ItemClass::Armor {
                    let (slot, a_ac) = if let Some(def) = self.ruleset.item(&item.name) {
                        let slot = def
                            .armor
                            .as_ref()
                            .map(|a| a.slot)
                            .or_else(|| armor_slot(&item.name));
                        let base_ac = def
                            .armor
                            .as_ref()
                            .map(|a| a.base_ac)
                            .unwrap_or(def.ac_bonus);
                        (slot, base_ac)
                    } else {
                        (armor_slot(&item.name), armor_base_ac(&item.name))
                    };
                    if let Some(slot) = slot {
                        if worn_slots.insert(slot) {
                            worn_armor.push((a_ac, item.enchantment as i32, item.erosion));
                        }
                    }
                }
            }
        }

        find_ac(10, &worn_armor, self.divine_protection as i32)
    }

    /// Recomputes the hero's AC and updates `player.ac`.
    pub fn recompute_hero_ac(&mut self) {
        let ac = self.compute_hero_ac();
        if let Some(player) = self.arena.actors.get_mut(self.player_id) {
            player.ac = ac;
        }
    }

    /// Set active locale for game event text and logging.
    pub fn set_locale(&mut self, locale: nethacked_types::Locale) {
        self.locale = locale;
    }

    /// Retrieve active locale.
    pub fn get_locale(&self) -> nethacked_types::Locale {
        self.locale
    }

    /// Initialize a new deterministic simulation world from a seed with default character.
    pub fn new_with_seed(seed: u64) -> Self {
        Self::new_with_character(seed, CharacterConfig::default())
    }

    /// Retrieve the price owed for an unpaid shop item, if any: the ledger holds the base
    /// cost (`oc_cost`), priced through C `get_cost` (`shk.c:2877`) for the current hero.
    pub fn get_unpaid_cost(&self, id: ItemId) -> Option<u32> {
        self.unpaid_items
            .iter()
            .find(|(i, _)| *i == id)
            .map(|(_, base)| self.shop_buy_price(*base))
    }

    /// Check if an item is unpaid store merchandise.
    pub fn is_unpaid(&self, id: ItemId) -> bool {
        self.unpaid_items.iter().any(|(i, _)| *i == id)
    }

    /// Clear unpaid status after purchase.
    pub fn remove_unpaid(&mut self, id: ItemId) {
        self.unpaid_items.retain(|(i, _)| *i != id);
    }

    /// Retrieve an EconomyLedger snapshot of current unpaid store items.
    pub fn economy_ledger(&self) -> crate::actions::economy::EconomyLedger {
        crate::actions::economy::EconomyLedger::with_items(self.unpaid_items.clone())
    }

    /// Append game events to the event log, maintaining a sliding window limit if configured.
    pub fn record_events(&mut self, events: &[GameEvent]) {
        self.event_log.extend(events.iter().cloned());
        let cap = self
            .max_event_log_len
            .unwrap_or(Self::DEFAULT_MAX_EVENT_LOG_LEN);
        if self.event_log.len() > cap {
            let excess = self.event_log.len() - cap;
            self.event_log.drain(0..excess);
        }
    }

    /// Set or clear the maximum capacity of the event log circular buffer.
    pub fn set_max_event_log_len(&mut self, cap: Option<usize>) {
        self.max_event_log_len = cap;
        if let Some(cap) = cap {
            if self.event_log.len() > cap {
                let excess = self.event_log.len() - cap;
                self.event_log.drain(0..excess);
            }
        }
    }

    /// Find actor occupying a specific coordinate.
    pub fn actor_at(&self, coord: Coord) -> Option<ActorId> {
        self.arena.actors.iter().find_map(|(id, actor)| {
            if actor.coord == coord && !actor.is_dead {
                Some(id)
            } else {
                None
            }
        })
    }

    /// Return the current hunger state based on nutrition points.
    pub fn hunger_state(&self) -> HungerState {
        hunger_of_nutrition(self.player_nutrition, DEFAULT_PLAYER_CON)
    }

    /// Luck timeout period in turns for the hero's current state (C `timeout.c:595-620`).
    ///
    /// The Amulet of Yendor must be carried. `divine_state` has no god-anger field yet,
    /// so `god_angry` is always false (documented limitation).
    pub fn luck_timeout_period(&self) -> u64 {
        nethacked_core::luck_decay_period(self.hero_has_amulet(), false)
    }

    /// Progress one tick of luck decay based on carried luckstone (C `timeout.c:595-620`).
    ///
    /// Base luck is 0: moon phase / Friday 13th are not tracked.
    pub fn tick_luck_decay(&mut self) {
        let luckstone = self
            .arena
            .items_carried_by(self.player_id)
            .into_iter()
            .find_map(|iid| {
                self.arena
                    .items
                    .get(iid)
                    .filter(|it| it.name.eq_ignore_ascii_case("luckstone"))
                    .map(|it| it.buc)
            });
        self.player_luck = nethacked_core::mines::step_luck_decay(self.player_luck, 0, luckstone);
    }

    /// Compute tile visibility and monster perception for the hero, taking into account:
    /// - Field of View (FOV)
    /// - Light sources (carried lit oil lamps / magic lamps / candles)
    /// - Room darkness
    /// - Blindness intrinsic
    /// - Telepathy intrinsic (sensing minded monsters when blind or in darkness)
    pub fn compute_perception(
        &self,
    ) -> (
        std::collections::HashSet<Coord>,
        std::collections::HashSet<ActorId>,
    ) {
        use std::collections::HashSet;

        let Some(player) = self.arena.actors.get(self.player_id) else {
            return (HashSet::new(), HashSet::new());
        };
        let p_coord = player.coord;
        let is_blind = player.intrinsics.blind;
        let has_telepathy = player.intrinsics.telepathy;

        // 1. Gather all active light sources in the level
        let mut light_sources: Vec<(Coord, u32)> = Vec::new();

        // Check player carried items for lit lamp
        let carried = self.arena.items_carried_by(self.player_id);
        for iid in carried {
            if let Some(it) = self.arena.items.get(iid) {
                if let Some(arch) = self.ruleset.item(&it.name) {
                    if arch.is_light_source() && it.enchantment > 0 {
                        let radius = arch.light_radius();
                        if radius > 0 {
                            light_sources.push((p_coord, radius));
                        }
                    }
                }
            }
        }

        // Compute standard FOV (radius 8)
        let fov = nethacked_dungeon::compute_fov(&self.level, p_coord, 8);
        // Compute illumination from light sources
        let illuminated = nethacked_dungeon::compute_illumination(&self.level, &light_sources);

        // Determine visible tiles
        let mut visible_tiles = HashSet::new();
        for &c in &fov {
            let dist = p_coord.chebyshev_distance(c) as u32;
            let is_dark = self.level.is_dark_at(c);
            let is_lit = illuminated.contains(&c);
            if nethacked_core::lighting::can_see_tile(is_blind, dist, is_dark, is_lit) {
                visible_tiles.insert(c);
            }
        }

        // Determine detected monsters
        let mut detected_monsters = HashSet::new();
        for (aid, actor) in self.arena.actors.iter() {
            if actor.is_dead {
                continue;
            }
            let tile_vis = visible_tiles.contains(&actor.coord);
            let has_mind = nethacked_core::lighting::monster_has_mind(&actor.name);
            if nethacked_core::lighting::can_detect_monster(
                is_blind,
                has_telepathy,
                has_mind,
                tile_vis,
            ) {
                detected_monsters.insert(aid);
            }
        }

        (visible_tiles, detected_monsters)
    }

    /// Apply damage to the hero without underflow; marks death at 0 HP.
    pub fn damage_player(&mut self, amount: u32, cause: &str) -> Vec<GameEvent> {
        let mut events = Vec::new();
        if let Some(p) = self.arena.actors.get_mut(self.player_id) {
            p.hp = p.hp.saturating_sub(amount);
            if p.hp == 0 && !p.is_dead {
                p.is_dead = true;
                events.push(GameEvent::LogMessage {
                    text: format!("You die... killed by {cause}."),
                });
            }
        }
        events
    }

    /// C `erode_armor()` for the hero's worn armor.
    pub(crate) fn erode_hero_armor(
        &mut self,
        kind: crate::progress::ErosionKind,
        events: &mut Vec<GameEvent>,
    ) {
        // Armor erosion logic (simplified - messages only for now)
        match kind {
            crate::progress::ErosionKind::Rust => {
                say(events, "Your armor rusts!");
            }
            crate::progress::ErosionKind::Corrode => {
                say(events, "Your armor corrodes!");
            }
            crate::progress::ErosionKind::Rot => {
                say(events, "Your armor rots!");
            }
        }
    }

    /// Steal an item from the hero (AD_SITM).
    pub(crate) fn steal_hero_item(
        &mut self,
        attacker: &nethacked_arena::ActorRecord,
        events: &mut Vec<GameEvent>,
    ) {
        // Simplified - just report the theft
        say(
            events,
            &format!("{} steals an item!", capitalize(&attacker.name)),
        );
    }

    /// Steal gold from the hero (AD_SGLD).
    pub(crate) fn steal_hero_gold(
        &mut self,
        attacker: &nethacked_arena::ActorRecord,
        events: &mut Vec<GameEvent>,
    ) {
        let stolen = (self.player_gold as f32 * 0.1).max(1.0) as u32;
        let stolen = stolen.min(self.player_gold);
        self.player_gold = self.player_gold.saturating_sub(stolen);
        say(
            events,
            &format!("{} steals {} gold!", capitalize(&attacker.name), stolen),
        );
    }

    /// Foocubus seduction (AD_SEDU).
    pub(crate) fn seduce_hero(
        &mut self,
        attacker: &nethacked_arena::ActorRecord,
        events: &mut Vec<GameEvent>,
    ) {
        if self.rng.random_range(0..2u32) == 0 {
            self.steal_hero_gold(attacker, events);
        } else {
            self.steal_hero_item(attacker, events);
        }
        // Foocubus teleports away after
        say(events, &format!("{} vanishes!", capitalize(&attacker.name)));
    }

    /// Teleport the hero (AD_TLPT).
    pub(crate) fn teleport_hero(&mut self, events: &mut Vec<GameEvent>) {
        // Simplified - just report teleport
        say(events, "You are teleported!");
    }

    /// Disenchant hero's item (AD_ENCH).
    pub(crate) fn disenchant_hero_item(&mut self, events: &mut Vec<GameEvent>) {
        say(events, "Your equipment feels less effective.");
    }

    /// Polymorph the hero (AD_POLY).
    pub(crate) fn polymorph_hero(&mut self, _dmg: u32, events: &mut Vec<GameEvent>) {
        say(events, "You feel a change coming over you.");
    }

    /// Make hero sick (AD_DISE).
    pub(crate) fn make_sick(&mut self, _dmg: u32, events: &mut Vec<GameEvent>) {
        say(events, "You feel deathly sick.");
    }

    /// Curse hero's items (AD_CURS).
    pub(crate) fn curse_hero_items(&mut self, events: &mut Vec<GameEvent>) {
        say(
            events,
            "You feel a malignant aura surround your possessions.",
        );
    }
}

fn say(events: &mut Vec<GameEvent>, text: &str) {
    events.push(GameEvent::LogMessage {
        text: text.to_string(),
    });
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}
