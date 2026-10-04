//! Core SimulationWorld state definition and character initialization.

use netrust_arena::{ActorId, EntityArena, ItemId, ItemLocation};
use netrust_core::{energy::SchedulerState, nutrition::hunger_of_nutrition, HungerState, SpellKind};
use netrust_data::{
    create_item_record, create_monster_record, spawn_player_character, CharacterConfig, ItemKindId,
    MonsterSpeciesId, RoleId,
};
use netrust_dungeon::{generate_dungeon_level, DungeonLevel, RoomType};
use netrust_types::{Buc, Coord, ItemClass};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

use crate::events::GameEvent;

/// Stored state of a dungeon level when player travels to other floors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredLevel {
    pub level: DungeonLevel,
    pub monsters: Vec<(netrust_arena::ActorId, netrust_arena::ActorRecord)>,
    pub items: Vec<(ItemId, netrust_arena::ItemRecord)>,
    pub unpaid_items: Vec<(ItemId, u32)>,
}

/// The complete, deterministic game simulation world.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationWorld {
    pub level: DungeonLevel,
    pub levels: Vec<DungeonLevel>,
    pub current_branch: netrust_types::BranchId,
    pub stored_levels: Vec<((netrust_types::BranchId, usize), StoredLevel)>,
    pub depth: usize,
    pub arena: EntityArena,
    pub player_id: ActorId,
    pub wielded_item: Option<ItemId>,
    pub scheduler: SchedulerState,
    pub unpaid_items: Vec<(ItemId, u32)>,
    pub player_gold: u32,
    pub player_nutrition: u32,
    pub player_pw: u32,
    pub player_max_pw: u32,
    pub known_spells: Vec<(SpellKind, u32)>,
    pub divine_state: netrust_types::DivineState,
    pub divine_protection: u32,
    pub player_luck: i32,
    pub hero: netrust_types::Hero,
    pub locale: netrust_types::Locale,
    pub bones_storage: Vec<netrust_types::BonesData>,
    #[serde(default)]
    pub candelabrum_state: netrust_core::CandelabrumState,
    #[serde(default)]
    pub ritual_progress: netrust_core::RitualProgress,
    #[serde(default)]
    pub vibrating_square: Option<Coord>,
    #[serde(default)]
    pub quest_state: netrust_core::QuestState,
    #[serde(default)]
    pub alignment_record: i32,
    #[serde(default)]
    pub role_name: String,
    #[serde(default = "default_rng")]
    pub rng: ChaCha8Rng,
    pub seed: u64,
    pub event_log: Vec<GameEvent>,
    #[serde(default)]
    pub genocide_registry: netrust_types::GenocideRegistry,
    #[serde(default)]
    pub conducts: netrust_types::ConductTracker,
}

pub fn default_rng() -> ChaCha8Rng {
    ChaCha8Rng::seed_from_u64(0)
}

impl SimulationWorld {
    /// Remove an actor, leaving anything it carried on the floor where it stood.
    pub(crate) fn remove_actor_dropping_items(&mut self, id: ActorId) {
        let Some(coord) = self.arena.actors.get(id).map(|a| a.coord) else { return };
        for item_id in self.arena.items_carried_by(id) {
            if let Some(item) = self.arena.items.get_mut(item_id) {
                item.location = ItemLocation::Floor(coord);
            }
        }
        self.arena.actors.remove(id);
    }

    pub(crate) fn actor_is_genocided(&self, name: &str) -> bool {
        let lower = name.to_lowercase();
        let class = netrust_data::monster_class_of(name);
        self.genocide_registry.genocided_species.contains(&lower)
            || class.map(|c| self.genocide_registry.genocided_classes.contains(&c)).unwrap_or(false)
    }

    /// Initialize a new deterministic simulation world with a custom character configuration.
    pub fn new_with_character(seed: u64, config: CharacterConfig) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let level = generate_dungeon_level(&mut rng);
        let mut arena = EntityArena::new();

        // Spawn player with character configuration (role, race, starting items)
        let (player_id, starting_items) = spawn_player_character(&config, level.stairs_up, &mut arena);

        let mut unpaid_items = Vec::new();
        let player_gold = if config.role == RoleId::Tourist { 200 } else { 50 };

        // Nutrition & Mana by role
        let player_nutrition = 900u32;
        let (player_pw, player_max_pw, known_spells) = match config.role {
            RoleId::Wizard => (25, 25, vec![(SpellKind::ForceBolt, 20000)]),
            RoleId::Healer => (20, 20, vec![(SpellKind::CureLightWounds, 20000)]),
            RoleId::Knight | RoleId::Monk => (10, 10, Vec::new()),
            _ => (5, 5, Vec::new()),
        };

        // Spawn starter food in player's pack
        arena.spawn_item(create_item_record(
            ItemKindId::FoodRation,
            ItemLocation::CarriedBy(player_id),
            Buc::Uncursed,
        ));

        // Process rooms: spawn Shopkeeper and merchandise in Shop, monsters in Normal rooms
        for (i, room) in level.rooms.iter().enumerate() {
            match room.room_type {
                RoomType::Shop => {
                    // Spawn peaceful Shopkeeper
                    let sk = create_monster_record(MonsterSpeciesId::Shopkeeper, room.center());
                    arena.spawn_actor(sk);

                    // Spawn shop merchandise on floor
                    let items_to_sell = [
                        ItemKindId::PotionOfExtraHealing,
                        ItemKindId::ScrollOfIdentify,
                        ItemKindId::LongSword,
                        ItemKindId::WandOfStriking,
                        ItemKindId::LeatherArmor,
                    ];
                    let mut off = 1;
                    for &kind in &items_to_sell {
                        let ix = (room.x1 + off).min(room.x2.saturating_sub(1));
                        let iy = (room.y1 + 1).min(room.y2.saturating_sub(1));
                        let ic = Coord::new_unchecked(ix, iy);
                        if ic != room.center() {
                            let item_rec = create_item_record(kind, ItemLocation::Floor(ic), Buc::Uncursed);
                            let cost = netrust_data::items::ITEM_CATALOG.iter().find(|it| it.id == kind).map(|it| it.cost).unwrap_or(30);
                            let item_id = arena.spawn_item(item_rec);
                            unpaid_items.push((item_id, cost));
                        }
                        off += 1;
                    }
                }
                RoomType::Normal if i > 0 && i != level.rooms.len() - 1 => {
                    let goblin = create_monster_record(MonsterSpeciesId::Goblin, room.center());
                    arena.spawn_actor(goblin);
                }
                _ => {}
            }
        }

        // Spawn starting floor items near stairs from declarative item catalog
        let item_coord1 = Coord::new(level.stairs_up.x + 1, level.stairs_up.y).unwrap_or(level.stairs_up);
        arena.spawn_item(create_item_record(ItemKindId::SilverSaber, ItemLocation::Floor(item_coord1), Buc::Uncursed));

        let item_coord2 = Coord::new(level.stairs_up.x, level.stairs_up.y + 1).unwrap_or(level.stairs_up);
        arena.spawn_item(create_item_record(ItemKindId::PotionOfHealing, ItemLocation::Floor(item_coord2), Buc::Blessed));

        let item_coord3 = Coord::new(level.stairs_up.x + 1, level.stairs_up.y + 1).unwrap_or(level.stairs_up);
        arena.spawn_item(create_item_record(ItemKindId::BagOfHolding, ItemLocation::Floor(item_coord3), Buc::Uncursed));

        // Auto-wield first starting weapon if any
        let wielded_item = starting_items.into_iter().find(|&id| {
            arena.items.get(id).map(|i| i.class == ItemClass::Weapon).unwrap_or(false)
        });

        Self {
            levels: vec![level.clone()],
            current_branch: netrust_types::BranchId::DungeonsOfDoom,
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
            divine_state: netrust_types::DivineState::default(),
            divine_protection: 0,
            player_luck: 0,
            hero: netrust_types::Hero {
                base_hp: arena.actors[player_id].hp as i32,
                base_max_hp: arena.actors[player_id].max_hp as i32,
                polymorph: None,
                lycanthropy: None, afflictions: netrust_types::AfflictionState::default(), skills: netrust_types::SkillTree::default(),
                mount: None,
                quivered_item: None,
            },
            locale: netrust_types::Locale::En,
            arena,
            bones_storage: Vec::new(),
            candelabrum_state: netrust_core::CandelabrumState::empty(),
            ritual_progress: netrust_core::RitualProgress::Uninitiated,
            vibrating_square: None,
            quest_state: netrust_core::QuestState::default(),
            alignment_record: 25, // Hero starts with pious devotion
            role_name: format!("{:?}", config.role),
            rng,
            seed,
            event_log: Vec::new(),
            genocide_registry: netrust_types::GenocideRegistry::default(),
            conducts: netrust_types::ConductTracker::default(),
        }
    }

    /// Set active locale for game event text and logging.
    pub fn set_locale(&mut self, locale: netrust_types::Locale) {
        self.locale = locale;
    }

    /// Retrieve active locale.
    pub fn get_locale(&self) -> netrust_types::Locale {
        self.locale
    }

    /// Initialize a new deterministic simulation world from a seed with default character.
    pub fn new_with_seed(seed: u64) -> Self {
        Self::new_with_character(seed, CharacterConfig::default())
    }

    /// Retrieve the unpaid debt cost for a shop item, if any.
    pub fn get_unpaid_cost(&self, id: ItemId) -> Option<u32> {
        self.unpaid_items.iter().find(|(i, _)| *i == id).map(|(_, c)| *c)
    }

    /// Check if an item is unpaid store merchandise.
    pub fn is_unpaid(&self, id: ItemId) -> bool {
        self.unpaid_items.iter().any(|(i, _)| *i == id)
    }

    /// Clear unpaid status after purchase.
    pub fn remove_unpaid(&mut self, id: ItemId) {
        self.unpaid_items.retain(|(i, _)| *i != id);
    }

    /// Find actor occupying a specific coordinate.
    pub fn actor_at(&self, coord: Coord) -> Option<ActorId> {
        self.arena
            .actors
            .iter()
            .find_map(|(id, actor)| {
                if actor.coord == coord && !actor.is_dead {
                    Some(id)
                } else {
                    None
                }
            })
    }

    /// Return the current hunger state based on nutrition points.
    pub fn hunger_state(&self) -> HungerState {
        hunger_of_nutrition(self.player_nutrition)
    }

    /// Progress one tick of luck decay based on carried luckstone.
    pub fn tick_luck_decay(&mut self) {
        let luckstone = self.arena.items_carried_by(self.player_id).into_iter().find_map(|iid| {
            self.arena.items.get(iid).filter(|it| it.name.to_lowercase().contains("luckstone")).map(|it| it.buc)
        });
        let stone_status = match luckstone {
            Some(Buc::Blessed) => netrust_core::mines::LuckstoneStatus::Blessed,
            Some(Buc::Uncursed) => netrust_core::mines::LuckstoneStatus::Uncursed,
            Some(Buc::Cursed) => netrust_core::mines::LuckstoneStatus::Cursed,
            None => netrust_core::mines::LuckstoneStatus::None,
        };
        self.player_luck = netrust_core::mines::step_luck_decay(self.player_luck, stone_status);
    }

    /// Compute tile visibility and monster perception for the hero, taking into account:
    /// - Field of View (FOV)
    /// - Light sources (carried lit oil lamps / magic lamps / candles)
    /// - Room darkness
    /// - Blindness intrinsic
    /// - Telepathy intrinsic (sensing minded monsters when blind or in darkness)
    pub fn compute_perception(&self) -> (std::collections::HashSet<Coord>, std::collections::HashSet<ActorId>) {
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
                if (it.name.contains("lamp") || it.name.contains("lantern") || it.name.contains("candle"))
                    && it.enchantment > 0
                {
                    let radius = if it.name.contains("lantern") {
                        netrust_core::lighting::LANTERN_RADIUS
                    } else if it.name.contains("candle") {
                        netrust_core::lighting::CANDLE_RADIUS
                    } else {
                        netrust_core::lighting::OIL_LAMP_RADIUS
                    };
                    light_sources.push((p_coord, radius));
                }
            }
        }

        // Compute standard FOV (radius 8)
        let fov = netrust_dungeon::compute_fov(&self.level, p_coord, 8);
        // Compute illumination from light sources
        let illuminated = netrust_dungeon::compute_illumination(&self.level, &light_sources);

        // Determine visible tiles
        let mut visible_tiles = HashSet::new();
        for &c in &fov {
            let dist = p_coord.chebyshev_distance(c) as u32;
            let is_dark = self.level.is_dark_at(c);
            let is_lit = illuminated.contains(&c);
            if netrust_core::lighting::can_see_tile(is_blind, dist, is_dark, is_lit) {
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
            let has_mind = netrust_core::lighting::monster_has_mind(&actor.name);
            if netrust_core::lighting::can_detect_monster(is_blind, has_telepathy, has_mind, tile_vis) {
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
                events.push(GameEvent::LogMessage { text: format!("You die... killed by {}.", cause) });
            }
        }
        events
    }
}
