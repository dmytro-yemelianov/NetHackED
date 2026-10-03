//! Multi-floor dungeon navigation, level caching, persistence, and branch transitions.

use netrust_arena::{ActorId, ItemId, ItemLocation};
use netrust_core::energy::NORMAL_SPEED;
use netrust_data::{create_item_record, create_monster_record, ItemKindId, MonsterSpeciesId};
use netrust_dungeon::{
    generate_dungeon_level, generate_gehennom_maze_level, generate_moloch_sanctum_level,
    generate_sokoban_level, generate_valley_of_the_dead,
};
use netrust_types::{BranchCoord, BranchId, Buc, Coord, Tile};
use rand::Rng;

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
            match (branch, depth) {
                (BranchId::Sokoban, _) => {
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
                (BranchId::GnomishMines, 3) => {
                    // Minetown! Complete with shops, temple, priest, and watchmen
                    let layout = netrust_dungeon::generate_minetown_level(&mut self.rng);
                    self.level = layout.level;

                    let priest = create_monster_record(MonsterSpeciesId::Priest, layout.priest_coord);
                    self.arena.spawn_actor(priest);

                    for wc in layout.watchmen_coords {
                        let watchman = create_monster_record(MonsterSpeciesId::Watchman, wc);
                        self.arena.spawn_actor(watchman);
                    }

                    for sc in layout.shopkeeper_coords {
                        let shopkeeper = create_monster_record(MonsterSpeciesId::Shopkeeper, sc);
                        self.arena.spawn_actor(shopkeeper);
                    }

                    events.push(GameEvent::LogMessage {
                        text: "Welcome to Minetown! Bustling shops and an ancient sanctuary stand before you.".into(),
                    });
                }
                (BranchId::GnomishMines, 5) => {
                    // Mines' End! Sprawling labyrinth with guaranteed Luckstone
                    let (lvl, luckstone_coord) = netrust_dungeon::generate_mines_end_level(&mut self.rng);
                    self.level = lvl;

                    let luckstone = create_item_record(ItemKindId::Luckstone, ItemLocation::Floor(luckstone_coord), Buc::Uncursed);
                    self.arena.spawn_item(luckstone);

                    for (i, room) in self.level.rooms.iter().enumerate() {
                        if i > 0 {
                            let species = if i % 2 == 0 { MonsterSpeciesId::SilverDragon } else { MonsterSpeciesId::Vampire };
                            let mon = create_monster_record(species, room.center());
                            self.arena.spawn_actor(mon);
                        }
                    }

                    events.push(GameEvent::LogMessage {
                        text: "You reach Mines' End! A legendary luckstone rests in the deepest shrine.".into(),
                    });
                }
                (BranchId::GnomishMines, d) => {
                    // Caverns (1, 2, 4)
                    let lvl = netrust_dungeon::generate_mines_cavern_level(&mut self.rng, d);
                    self.level = lvl;

                    for (i, room) in self.level.rooms.iter().enumerate() {
                        if i > 0 {
                            let species = if i % 2 == 0 { MonsterSpeciesId::Gnome } else { MonsterSpeciesId::Dwarf };
                            let mon = create_monster_record(species, room.center());
                            self.arena.spawn_actor(mon);
                        }
                    }

                    events.push(GameEvent::LogMessage {
                        text: "You descend into the rugged, dark caverns of the Gnomish Mines.".into(),
                    });
                }
                (BranchId::Gehennom, 1) => {
                    // Valley of the Dead
                    let lvl = generate_valley_of_the_dead(&mut self.rng);
                    self.level = lvl;

                    for (i, room) in self.level.rooms.iter().enumerate() {
                        if i > 0 {
                            let species = if i % 2 == 0 { MonsterSpeciesId::Skeleton } else { MonsterSpeciesId::Vampire };
                            let mon = create_monster_record(species, room.center());
                            self.arena.spawn_actor(mon);
                        }
                    }

                    // Spawn Bell of Opening in Valley of the Dead
                    let bell_coord = self.level.rooms[1].center();
                    let bell = create_item_record(ItemKindId::BellOfOpening, ItemLocation::Floor(bell_coord), Buc::Blessed);
                    self.arena.spawn_item(bell);

                    events.push(GameEvent::LogMessage {
                        text: "You cross into the gloomy, desolate Valley of the Dead...".into(),
                    });
                }
                (BranchId::Gehennom, 5) => {
                    // Deepest Gehennom Maze with Vibrating Square
                    let (lvl, vs) = generate_gehennom_maze_level(&mut self.rng, 5, true);
                    self.level = lvl;
                    self.vibrating_square = vs;

                    for (i, room) in self.level.rooms.iter().enumerate() {
                        if i > 0 {
                            let mon = create_monster_record(MonsterSpeciesId::SilverDragon, room.center());
                            self.arena.spawn_actor(mon);
                        }
                    }

                    // Spawn Candelabrum and Book of the Dead
                    let cand_coord = self.level.rooms[1].center();
                    let cand = create_item_record(ItemKindId::CandelabrumOfInvocation, ItemLocation::Floor(cand_coord), Buc::Uncursed);
                    self.arena.spawn_item(cand);

                    let book_coord = Coord::new_unchecked(self.level.rooms[1].x1 + 1, self.level.rooms[1].y1 + 1);
                    let book = create_item_record(ItemKindId::BookOfTheDead, ItemLocation::Floor(book_coord), Buc::Blessed);
                    self.arena.spawn_item(book);

                    // Spawn 7 wax candles
                    for c_i in 0..7 {
                        let candle_c = Coord::new_unchecked(self.level.rooms[0].x1 + 1 + c_i, self.level.rooms[0].y1 + 1);
                        let candle = create_item_record(ItemKindId::WaxCandle, ItemLocation::Floor(candle_c), Buc::Uncursed);
                        self.arena.spawn_item(candle);
                    }

                    events.push(GameEvent::LogMessage {
                        text: "You reach the infernal bottom of Gehennom. A cryptic vibration resonates beneath the stone.".into(),
                    });
                }
                (BranchId::Gehennom, 6) => {
                    // Moloch's Sanctum
                    let (lvl, _spawn) = generate_moloch_sanctum_level(&mut self.rng);
                    self.level = lvl;

                    let priest = create_monster_record(MonsterSpeciesId::Priest, self.level.stairs_down);
                    self.arena.spawn_actor(priest);

                    let amulet = create_item_record(ItemKindId::AmuletOfYendor, ItemLocation::Floor(self.level.stairs_down), Buc::Blessed);
                    self.arena.spawn_item(amulet);

                    events.push(GameEvent::LogMessage {
                        text: "You enter Moloch's Sanctum! Rivers of boiling lava surround the unholy high altar!".into(),
                    });
                }
                (BranchId::Gehennom, d) => {
                    // Intermediate Gehennom Mazes
                    let (lvl, _) = generate_gehennom_maze_level(&mut self.rng, d, false);
                    self.level = lvl;

                    for (i, room) in self.level.rooms.iter().enumerate() {
                        if i > 0 {
                            let species = if i % 2 == 0 { MonsterSpeciesId::SilverDragon } else { MonsterSpeciesId::Vampire };
                            let mon = create_monster_record(species, room.center());
                            self.arena.spawn_actor(mon);
                        }
                    }

                    events.push(GameEvent::LogMessage {
                        text: format!("You delve through the fiery, twisting corridors of Gehennom (level {}).", d),
                    });
                }
                _ => {
                    let mut new_level = generate_dungeon_level(&mut self.rng);

                    // Place branch stairs in Dungeons of Doom
                    if branch == BranchId::DungeonsOfDoom && depth == 3 {
                        if let Some(room) = new_level.rooms.get(1) {
                            let branch_coord = Coord::new_unchecked(room.x1 + 1, room.y1 + 1);
                            new_level.set_tile(branch_coord, Tile::BranchStairs {
                                branch: BranchId::GnomishMines,
                                level: 1,
                                up: false,
                            });
                        }
                    }
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
                    if branch == BranchId::DungeonsOfDoom && depth == 5 {
                        if let Some(room) = new_level.rooms.first() {
                            let branch_coord = Coord::new_unchecked(room.x1 + 1, room.y1 + 1);
                            new_level.set_tile(branch_coord, Tile::BranchStairs {
                                branch: BranchId::Gehennom,
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

        // Gehennom bottom floor: Vibrating Square subterranean portal
        if self.current_branch == BranchId::Gehennom && self.depth == 5 && self.vibrating_square == Some(player.coord) {
            if netrust_core::is_sanctum_accessible(self.ritual_progress) {
                let from_depth = self.depth;
                self.pack_current_level();
                self.depth = 6;
                let gen_events = self.unpack_or_generate_level(BranchId::Gehennom, 6);
                events.extend(gen_events);

                let new_coord = self.level.stairs_up;
                if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                    p.coord = new_coord;
                }

                events.push(GameEvent::LevelChanged { from_depth, to_depth: self.depth });
                events.push(GameEvent::LogMessage {
                    text: "You step through the subterranean portal into Moloch's Sanctum!".into(),
                });
                self.scheduler.hero_act(NORMAL_SPEED);
                return events;
            } else {
                events.push(GameEvent::LogMessage {
                    text: "You feel a strange vibration beneath your feet, but the subterranean way remains sealed. Perform the Invocation Ritual!".into(),
                });
                return events;
            }
        }

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
                    // Gehennom Mysterious Force when ascending with the real Amulet of Yendor
                    let has_amulet = self.arena.items_carried_by(self.player_id).iter().any(|&iid| {
                        self.arena.items.get(iid).map(|it| it.name.contains("Amulet of Yendor")).unwrap_or(false)
                    });
                    if self.current_branch == BranchId::Gehennom && has_amulet {
                        let roll = self.rng.random::<u32>();
                        if let Some(pushed_depth) = netrust_core::calculate_mysterious_force(self.depth, roll) {
                            let from_depth = self.depth;
                            self.pack_current_level();
                            self.depth = pushed_depth;
                            let gen_events = self.unpack_or_generate_level(BranchId::Gehennom, self.depth);
                            events.extend(gen_events);

                            let new_coord = self.level.stairs_up;
                            if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                                p.coord = new_coord;
                            }

                            events.push(GameEvent::LevelChanged { from_depth, to_depth: self.depth });
                            events.push(GameEvent::LogMessage {
                                text: format!("An eldritch Mysterious Force pushes you downward to level {}!", pushed_depth),
                            });
                            self.scheduler.hero_act(NORMAL_SPEED);
                            return events;
                        }
                    }

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
