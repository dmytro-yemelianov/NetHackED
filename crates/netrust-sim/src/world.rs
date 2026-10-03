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
    pub monsters: Vec<netrust_arena::ActorRecord>,
    pub floor_items: Vec<netrust_arena::ItemRecord>,
    pub unpaid_items: Vec<(ItemId, u32)>,
}

/// The complete, deterministic game simulation world.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationWorld {
    pub level: DungeonLevel,
    pub levels: Vec<DungeonLevel>,
    pub stored_levels: Vec<(usize, StoredLevel)>,
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
    #[serde(skip, default = "default_rng")]
    pub rng: ChaCha8Rng,
    pub seed: u64,
    pub event_log: Vec<GameEvent>,
}

pub fn default_rng() -> ChaCha8Rng {
    ChaCha8Rng::seed_from_u64(0)
}

impl SimulationWorld {
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
            stored_levels: Vec::new(),
            depth: 1,
            level,
            arena,
            player_id,
            wielded_item,
            scheduler: SchedulerState::new(12, 10),
            unpaid_items,
            player_gold,
            player_nutrition,
            player_pw,
            player_max_pw,
            known_spells,
            rng,
            seed,
            event_log: Vec::new(),
        }
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
}
