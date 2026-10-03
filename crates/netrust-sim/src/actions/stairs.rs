//! Multi-floor dungeon navigation, level caching, persistence, and game victory.

use netrust_arena::{ActorId, ItemId, ItemLocation};
use netrust_core::energy::NORMAL_SPEED;
use netrust_data::{create_item_record, create_monster_record, ItemKindId, MonsterSpeciesId};
use netrust_dungeon::generate_dungeon_level;
use netrust_types::{Buc, Tile};

use crate::events::GameEvent;
use crate::world::{SimulationWorld, StoredLevel};

impl SimulationWorld {
    pub(crate) fn handle_descend(&mut self) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let Some(player) = self.arena.actors.get(self.player_id).cloned() else {
            return events;
        };

        if matches!(self.level.get_tile(player.coord), Tile::Stairs { up: false }) {
            let from_depth = self.depth;

            // 1. Pack current floor's non-player actors and floor items into StoredLevel
            let mut level_monsters = Vec::new();
            let monster_ids: Vec<ActorId> = self.arena.actors.iter()
                .filter(|(id, _)| *id != self.player_id)
                .map(|(id, _)| id)
                .collect();
            for mid in monster_ids {
                if let Some(actor) = self.arena.destroy_actor(mid) {
                    level_monsters.push(actor);
                }
            }

            let mut level_floor_items = Vec::new();
            let floor_item_ids: Vec<ItemId> = self.arena.items.iter()
                .filter(|(_, it)| matches!(it.location, ItemLocation::Floor(_)))
                .map(|(id, _)| id)
                .collect();
            for iid in floor_item_ids {
                if let Some(item) = self.arena.destroy_item(iid) {
                    level_floor_items.push(item);
                }
            }

            let stored_current = StoredLevel {
                level: self.level.clone(),
                monsters: level_monsters,
                floor_items: level_floor_items,
                unpaid_items: std::mem::take(&mut self.unpaid_items),
            };
            if let Some(pos) = self.stored_levels.iter().position(|(d, _)| *d == from_depth) {
                self.stored_levels[pos] = (from_depth, stored_current);
            } else {
                self.stored_levels.push((from_depth, stored_current));
            }

            // 2. Increment depth
            self.depth += 1;

            // 3. Restore or generate new level
            if let Some(pos) = self.stored_levels.iter().position(|(d, _)| *d == self.depth) {
                let (_, stored) = self.stored_levels.remove(pos);
                self.level = stored.level;
                for m in stored.monsters {
                    self.arena.spawn_actor(m);
                }
                for it in stored.floor_items {
                    self.arena.spawn_item(it);
                }
                self.unpaid_items = stored.unpaid_items;
            } else {
                let new_level = generate_dungeon_level(&mut self.rng);
                self.level = new_level;

                // Spawn monsters appropriate for deeper level
                for (i, room) in self.level.rooms.iter().enumerate() {
                    if i > 0 && i != self.level.rooms.len() - 1 {
                        let species = match self.depth {
                            1 => MonsterSpeciesId::Goblin,
                            2 => if i % 2 == 0 { MonsterSpeciesId::Hobgoblin } else { MonsterSpeciesId::Orc },
                            3 => if i % 2 == 0 { MonsterSpeciesId::GiantAnt } else { MonsterSpeciesId::Skeleton },
                            4 => MonsterSpeciesId::Vampire,
                            _ => MonsterSpeciesId::SilverDragon,
                        };
                        let monster = create_monster_record(species, room.center());
                        self.arena.spawn_actor(monster);
                    }
                }

                // If reached depth 5, spawn the legendary Amulet of Yendor!
                if self.depth == 5 {
                    if let Some(deepest_room) = self.level.rooms.last() {
                        let amulet = create_item_record(ItemKindId::AmuletOfYendor, ItemLocation::Floor(deepest_room.center()), Buc::Blessed);
                        self.arena.spawn_item(amulet);
                        events.push(GameEvent::LogMessage {
                            text: "A primordial cosmic radiance emanates from the deepest chamber of this floor...".into(),
                        });
                    }
                }
            }

            // Place player on stairs_up
            let new_coord = self.level.stairs_up;
            if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                p.coord = new_coord;
            }

            events.push(GameEvent::LevelChanged { from_depth, to_depth: self.depth });
            events.push(GameEvent::LogMessage { text: format!("You descend deeper into dungeon level {}.", self.depth) });
            self.scheduler.hero_act(NORMAL_SPEED);
        } else {
            events.push(GameEvent::LogMessage { text: "There are no stairs leading down here.".into() });
        }

        events
    }

    pub(crate) fn handle_ascend(&mut self) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let Some(player) = self.arena.actors.get(self.player_id).cloned() else {
            return events;
        };

        if matches!(self.level.get_tile(player.coord), Tile::Stairs { up: true }) {
            if self.depth > 1 {
                let from_depth = self.depth;

                // Pack current level
                let mut level_monsters = Vec::new();
                let monster_ids: Vec<ActorId> = self.arena.actors.iter()
                    .filter(|(id, _)| *id != self.player_id)
                    .map(|(id, _)| id)
                    .collect();
                for mid in monster_ids {
                    if let Some(actor) = self.arena.destroy_actor(mid) {
                        level_monsters.push(actor);
                    }
                }

                let mut level_floor_items = Vec::new();
                let floor_item_ids: Vec<ItemId> = self.arena.items.iter()
                    .filter(|(_, it)| matches!(it.location, ItemLocation::Floor(_)))
                    .map(|(id, _)| id)
                    .collect();
                for iid in floor_item_ids {
                    if let Some(item) = self.arena.destroy_item(iid) {
                        level_floor_items.push(item);
                    }
                }

                let stored_current = StoredLevel {
                    level: self.level.clone(),
                    monsters: level_monsters,
                    floor_items: level_floor_items,
                    unpaid_items: std::mem::take(&mut self.unpaid_items),
                };
                if let Some(pos) = self.stored_levels.iter().position(|(d, _)| *d == from_depth) {
                    self.stored_levels[pos] = (from_depth, stored_current);
                } else {
                    self.stored_levels.push((from_depth, stored_current));
                }

                self.depth -= 1;
                if let Some(pos) = self.stored_levels.iter().position(|(d, _)| *d == self.depth) {
                    let (_, stored) = self.stored_levels.remove(pos);
                    self.level = stored.level;
                    for m in stored.monsters {
                        self.arena.spawn_actor(m);
                    }
                    for it in stored.floor_items {
                        self.arena.spawn_item(it);
                    }
                    self.unpaid_items = stored.unpaid_items;
                }

                let new_coord = self.level.stairs_down;
                if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                    p.coord = new_coord;
                }
                events.push(GameEvent::LevelChanged { from_depth, to_depth: self.depth });
                events.push(GameEvent::LogMessage { text: format!("You ascend to dungeon level {}.", self.depth) });
                self.scheduler.hero_act(NORMAL_SPEED);
            } else {
                // Player is on depth 1 stairs up
                let has_amulet = self.arena.items_carried_by(self.player_id).iter().any(|&iid| {
                    self.arena.items.get(iid).map(|it| it.name.contains("Amulet of Yendor")).unwrap_or(false)
                });
                if has_amulet {
                    events.push(GameEvent::Victory);
                    events.push(GameEvent::LogMessage {
                        text: "You ascend from the dungeon carrying the Amulet of Yendor! An astral chorus welcomes you into immortality! You have won NetRust!".into(),
                    });
                } else {
                    events.push(GameEvent::LogMessage {
                        text: "An unseen celestial force bars your escape from the dungeon without the Amulet of Yendor!".into(),
                    });
                }
            }
        } else {
            events.push(GameEvent::LogMessage { text: "There are no stairs leading up here.".into() });
        }

        events
    }
}
