//! Multi-floor dungeon navigation, level caching, persistence, and branch transitions.

use netrust_arena::{ActorId, ItemId, ItemLocation};
use netrust_core::energy::NORMAL_SPEED;
use netrust_data::{create_item_record, create_monster_record, ItemKindId, MonsterSpeciesId};
use netrust_dungeon::{generate_dungeon_level, generate_sokoban_level};
use netrust_types::{BranchCoord, BranchId, Buc, Coord, Tile};

use crate::events::GameEvent;
use crate::world::{SimulationWorld, StoredLevel};

impl SimulationWorld {
    /// Pack non-player actors and floor items of the current floor into StoredLevel cache.
    pub(crate) fn pack_current_level(&mut self) {
        let from_key = (self.current_branch, self.depth);

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

        if let Some(pos) = self.stored_levels.iter().position(|(k, _)| *k == from_key) {
            self.stored_levels[pos] = (from_key, stored_current);
        } else {
            self.stored_levels.push((from_key, stored_current));
        }
    }

    /// Restore an existing cached level or procedurally generate a new level.
    pub(crate) fn unpack_or_generate_level(&mut self, branch: BranchId, depth: usize) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let target_key = (branch, depth);

        if let Some(pos) = self.stored_levels.iter().position(|(k, _)| *k == target_key) {
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
            match branch {
                BranchId::Sokoban => {
                    let (soko_level, boulder_coords) = generate_sokoban_level(depth);
                    self.level = soko_level;

                    // Spawn puzzle boulders
                    for bc in boulder_coords {
                        let boulder = create_item_record(ItemKindId::Boulder, ItemLocation::Floor(bc), Buc::Uncursed);
                        self.arena.spawn_item(boulder);
                    }

                    // Spawn prize in prize chamber (at depth 1: Bag of Holding)
                    let prize_coord = Coord::new_unchecked(64, 10);
                    let prize_kind = if depth == 1 {
                        ItemKindId::BagOfHolding
                    } else {
                        ItemKindId::AmuletOfReflection
                    };
                    let prize = create_item_record(prize_kind, ItemLocation::Floor(prize_coord), Buc::Blessed);
                    self.arena.spawn_item(prize);

                    events.push(GameEvent::LogMessage {
                        text: "You step into the legendary Sokoban puzzle maze! Boulders and pits line the corridors.".into(),
                    });
                }
                _ => {
                    let mut new_level = generate_dungeon_level(&mut self.rng);

                    // Place branch stairs in Dungeons of Doom
                    if branch == BranchId::DungeonsOfDoom && depth == 4 {
                        if let Some(room) = new_level.rooms.get(1) {
                            let branch_coord = Coord::new_unchecked(room.x1 + 1, room.y1 + 1);
                            new_level.set_tile(branch_coord, Tile::BranchStairs {
                                branch: BranchId::Sokoban,
                                level: 1,
                                up: false,
                            });
                        }
                    }

                    self.level = new_level;

                    // Spawn monsters appropriate for depth
                    for (i, room) in self.level.rooms.iter().enumerate() {
                        if i > 0 && i != self.level.rooms.len() - 1 {
                            let species = match depth {
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

                    // At depth 5 in Dungeons of Doom, spawn the Amulet of Yendor
                    if branch == BranchId::DungeonsOfDoom && depth == 5 {
                        if let Some(deepest_room) = self.level.rooms.last() {
                            let amulet = create_item_record(ItemKindId::AmuletOfYendor, ItemLocation::Floor(deepest_room.center()), Buc::Blessed);
                            self.arena.spawn_item(amulet);
                            events.push(GameEvent::LogMessage {
                                text: "A primordial cosmic radiance emanates from the deepest chamber of this floor...".into(),
                            });
                        }
                    }
                }
            }
        }

        events
    }

    pub(crate) fn handle_descend(&mut self) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let Some(player) = self.arena.actors.get(self.player_id).cloned() else {
            return events;
        };

        let tile = self.level.get_tile(player.coord).clone();
        match tile {
            Tile::Stairs { up: false } => {
                let from_depth = self.depth;
                self.pack_current_level();
                self.depth += 1;
                let gen_events = self.unpack_or_generate_level(self.current_branch, self.depth);
                events.extend(gen_events);

                let new_coord = self.level.stairs_up;
                if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                    p.coord = new_coord;
                }

                events.push(GameEvent::LevelChanged { from_depth, to_depth: self.depth });
                events.push(GameEvent::LogMessage { text: format!("You descend deeper into dungeon level {}.", self.depth) });
                self.scheduler.hero_act(NORMAL_SPEED);
            }
            Tile::BranchStairs { branch, level, up: false } => {
                let from_depth = self.depth;
                self.pack_current_level();
                self.current_branch = branch;
                self.depth = level;
                let gen_events = self.unpack_or_generate_level(branch, level);
                events.extend(gen_events);

                let new_coord = self.level.stairs_up;
                if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                    p.coord = new_coord;
                }

                events.push(GameEvent::LevelChanged { from_depth, to_depth: self.depth });
                events.push(GameEvent::LogMessage { text: format!("You enter the {:?} branch (level {}).", branch, level) });
                self.scheduler.hero_act(NORMAL_SPEED);
            }
            _ => {
                events.push(GameEvent::LogMessage { text: "There are no stairs leading down here.".into() });
            }
        }

        events
    }

    pub(crate) fn handle_ascend(&mut self) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let Some(player) = self.arena.actors.get(self.player_id).cloned() else {
            return events;
        };

        let tile = self.level.get_tile(player.coord).clone();
        match tile {
            Tile::Stairs { up: true } => {
                if self.depth > 1 {
                    let from_depth = self.depth;
                    self.pack_current_level();
                    self.depth -= 1;
                    let gen_events = self.unpack_or_generate_level(self.current_branch, self.depth);
                    events.extend(gen_events);

                    let new_coord = self.level.stairs_down;
                    if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                        p.coord = new_coord;
                    }

                    events.push(GameEvent::LevelChanged { from_depth, to_depth: self.depth });
                    events.push(GameEvent::LogMessage { text: format!("You ascend to dungeon level {}.", self.depth) });
                    self.scheduler.hero_act(NORMAL_SPEED);
                } else if self.current_branch == BranchId::DungeonsOfDoom {
                    // Surface check for Victory with Amulet of Yendor
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
                } else {
                    // Branch level 1 stairs up returning to parent branch
                    if let Some(parent) = netrust_core::branch::exit_branch(BranchCoord {
                        branch: self.current_branch,
                        depth: self.depth,
                    }) {
                        let from_depth = self.depth;
                        self.pack_current_level();
                        self.current_branch = parent.branch;
                        self.depth = parent.depth;
                        let gen_events = self.unpack_or_generate_level(parent.branch, parent.depth);
                        events.extend(gen_events);

                        let new_coord = self.level.stairs_down;
                        if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                            p.coord = new_coord;
                        }

                        events.push(GameEvent::LevelChanged { from_depth, to_depth: self.depth });
                        events.push(GameEvent::LogMessage { text: format!("You return to {:?} level {}.", parent.branch, parent.depth) });
                        self.scheduler.hero_act(NORMAL_SPEED);
                    }
                }
            }
            Tile::BranchStairs { branch, level, up: true } => {
                let from_depth = self.depth;
                self.pack_current_level();
                self.current_branch = branch;
                self.depth = level;
                let gen_events = self.unpack_or_generate_level(branch, level);
                events.extend(gen_events);

                let new_coord = self.level.stairs_down;
                if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                    p.coord = new_coord;
                }

                events.push(GameEvent::LevelChanged { from_depth, to_depth: self.depth });
                events.push(GameEvent::LogMessage { text: format!("You return to {:?} level {}.", branch, level) });
                self.scheduler.hero_act(NORMAL_SPEED);
            }
            _ => {
                events.push(GameEvent::LogMessage { text: "There are no stairs leading up here.".into() });
            }
        }

        events
    }
}
