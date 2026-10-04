//! Multi-floor dungeon navigation, level caching, persistence, and branch transitions.

use netrust_arena::{ActorId, ItemId, ItemLocation};
use netrust_core::energy::NORMAL_SPEED;
use netrust_data::{create_item_record, create_monster_record, ItemKindId, MonsterSpeciesId};
use netrust_dungeon::{
    generate_dungeon_level, generate_gehennom_maze_level, generate_moloch_sanctum_level,
    generate_sokoban_level, generate_valley_of_the_dead,
};
use netrust_types::{BranchCoord, BranchId, Buc, Coord, Tile, COLNO, ROWNO};
use rand::Rng;

use crate::events::GameEvent;
use crate::world::{SimulationWorld, StoredLevel};

pub const SANCTUM_DEPTH: usize = 6;

/// Without the completed invocation, the push can never land on Moloch's Sanctum.
pub fn clamp_mysterious_force(pushed: usize, sanctum_open: bool) -> usize {
    if sanctum_open {
        pushed
    } else {
        pushed.min(SANCTUM_DEPTH - 1)
    }
}

/// Resolve a quest leader/nemesis display name (from `get_role_quest_config`,
/// C `role.c` `urole[]`) to its BESTIARY species, so spawning and the
/// nemesis-kill name check in combat can never disagree.
pub(crate) fn quest_species_by_name(name: &str) -> MonsterSpeciesId {
    netrust_data::BESTIARY
        .iter()
        .find(|m| m.name.eq_ignore_ascii_case(name))
        .map(|m| m.id)
        .unwrap_or_else(|| panic!("quest monster {name} missing from BESTIARY"))
}

impl SimulationWorld {
    /// Same-level random teleport (C `safe_teleds`, `do.c:1566`): move the hero to a
    /// random passable, unoccupied tile. Approximation: uniform over valid tiles
    /// rather than C's retry-until-`goodpos` loop; no-op if no tile exists.
    pub(crate) fn teleport_hero_randomly(&mut self) {
        let mut spots = Vec::new();
        for y in 0..ROWNO {
            for x in 0..COLNO {
                if let Some(c) = Coord::new(x, y) {
                    if self.level.is_passable(c) && self.actor_at(c).is_none() {
                        spots.push(c);
                    }
                }
            }
        }
        if spots.is_empty() {
            return;
        }
        let pick = spots[self.rng.random_range(0..spots.len())];
        if let Some(p) = self.arena.actors.get_mut(self.player_id) {
            p.coord = pick;
        }
        self.place_steed_with_hero();
    }

    /// Spawn a monster at or near `preferred` on a passable, unoccupied, non-stairs tile
    /// (searching outward up to radius 3 in row-major ring order). Returns `None` if no spot.
    ///
    /// Like C `makemon` (`makemon.c:1299`), the new monster's peacefulness is
    /// `peace_minded(ptr)`, drawn from the sim RNG only when C draws.
    pub fn spawn_monster_near(
        &mut self,
        species: MonsterSpeciesId,
        preferred: Coord,
    ) -> Option<ActorId> {
        for radius in 0..=3isize {
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    if dx.abs().max(dy.abs()) != radius {
                        continue;
                    }
                    let (x, y) = (preferred.x as isize + dx, preferred.y as isize + dy);
                    if x < 0 || y < 0 {
                        continue;
                    }
                    let Some(c) = Coord::new(x as usize, y as usize) else {
                        continue;
                    };
                    if self.level.is_passable(c)
                        && c != self.level.stairs_up
                        && c != self.level.stairs_down
                        && self.actor_at(c).is_none()
                    {
                        let mut rec = create_monster_record(species, c);
                        rec.is_peaceful = self.roll_spawn_peaceful(species);
                        return Some(self.arena.spawn_actor(rec));
                    }
                }
            }
        }
        None
    }

    /// Pack the current floor into the StoredLevel cache: every non-player, non-steed actor,
    /// every floor item, items carried by packed monsters and everything transitively inside
    /// those containers (cycle-safe), plus their unpaid-ledger entries. Items the hero carries
    /// stay in the arena; stale quiver/wield references are cleared first.
    pub(crate) fn pack_current_level(&mut self) {
        let from_key = (self.current_branch, self.depth);
        let steed_id = self.hero.mount.as_ref().map(|m| m.steed_id);

        // Quiver must refer to something the hero carries.
        if let Some(q) = self.hero.quivered_item {
            let carried = self
                .arena
                .items
                .get(q)
                .map(|it| it.location == ItemLocation::CarriedBy(self.player_id))
                .unwrap_or(false);
            if !carried {
                self.hero.quivered_item = None;
            }
        }

        // Wielded item must refer to something the hero carries.
        if let Some(w) = self.wielded_item {
            let carried = self
                .arena
                .items
                .get(w)
                .map(|it| it.location == ItemLocation::CarriedBy(self.player_id))
                .unwrap_or(false);
            if !carried {
                self.wielded_item = None;
            }
        }

        let monster_ids: Vec<ActorId> = self
            .arena
            .actors
            .iter()
            .filter(|(id, _)| *id != self.player_id && Some(*id) != steed_id)
            .map(|(id, _)| id)
            .collect();

        let monster_set: std::collections::HashSet<ActorId> = monster_ids.iter().copied().collect();
        // Items belonging to the level: floor items, items carried by packed monsters, and
        // everything transitively inside those.
        let mut item_ids: Vec<ItemId> = self
            .arena
            .items
            .iter()
            .filter(|(_, it)| match it.location {
                ItemLocation::Floor(_) => true,
                ItemLocation::CarriedBy(a) => monster_set.contains(&a),
                _ => false,
            })
            .map(|(id, _)| id)
            .collect();
        let mut item_set: std::collections::HashSet<ItemId> = item_ids.iter().copied().collect();
        let mut i = 0;
        while i < item_ids.len() {
            for child in self.arena.items_in_container(item_ids[i]) {
                if item_set.insert(child) {
                    item_ids.push(child);
                }
            }
            i += 1;
        }

        let mut monsters = Vec::with_capacity(monster_ids.len());
        for mid in monster_ids {
            if let Some(actor) = self.arena.destroy_actor(mid) {
                monsters.push((mid, actor));
            }
        }
        let mut items = Vec::with_capacity(item_ids.len());
        for iid in &item_ids {
            if let Some(item) = self.arena.destroy_item(*iid) {
                items.push((*iid, item));
            }
        }

        let (level_unpaid, hero_unpaid): (Vec<_>, Vec<_>) = std::mem::take(&mut self.unpaid_items)
            .into_iter()
            .partition(|(iid, _)| item_set.contains(iid));
        self.unpaid_items = hero_unpaid;

        let stored_current = StoredLevel {
            level: self.level.clone(),
            monsters,
            items,
            unpaid_items: level_unpaid,
        };

        if let Some(pos) = self.stored_levels.iter().position(|(k, _)| *k == from_key) {
            self.stored_levels[pos] = (from_key, stored_current);
        } else {
            self.stored_levels.push((from_key, stored_current));
        }
    }

    /// A mounted hero's steed arrives on the same square as the hero.
    pub(crate) fn place_steed_with_hero(&mut self) {
        let Some(steed_id) = self.hero.mount.as_ref().map(|m| m.steed_id) else {
            return;
        };
        let Some(hero_coord) = self.arena.actors.get(self.player_id).map(|p| p.coord) else {
            return;
        };
        if let Some(steed) = self.arena.actors.get_mut(steed_id) {
            steed.coord = hero_coord;
        } else {
            self.hero.mount = None;
        }
    }

    /// Restore an existing cached level or procedurally generate a new level.
    pub fn unpack_or_generate_level(&mut self, branch: BranchId, depth: usize) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let target_key = (branch, depth);

        if let Some(pos) = self
            .stored_levels
            .iter()
            .position(|(k, _)| *k == target_key)
        {
            let (_, stored) = self.stored_levels.remove(pos);
            self.level = stored.level;

            let mut actor_map = std::collections::HashMap::new();
            for (old, m) in stored.monsters {
                actor_map.insert(old, self.arena.spawn_actor(m));
            }
            let mut item_map = std::collections::HashMap::new();
            let mut spawned = Vec::with_capacity(stored.items.len());
            for (old, it) in stored.items {
                let new = self.arena.spawn_item(it);
                item_map.insert(old, new);
                spawned.push(new);
            }
            let fallback = ItemLocation::Floor(self.level.stairs_up);
            for new in spawned {
                if let Some(it) = self.arena.items.get_mut(new) {
                    it.location = match it.location.clone() {
                        ItemLocation::InContainer(old) => item_map
                            .get(&old)
                            .map(|n| ItemLocation::InContainer(*n))
                            .unwrap_or(fallback.clone()),
                        ItemLocation::CarriedBy(old) => actor_map
                            .get(&old)
                            .map(|n| ItemLocation::CarriedBy(*n))
                            .unwrap_or(fallback.clone()),
                        other => other,
                    };
                }
            }
            for (old, cost) in stored.unpaid_items {
                if let Some(new) = item_map.get(&old) {
                    self.unpaid_items.push((*new, cost));
                }
            }
        } else {
            match (branch, depth) {
                (BranchId::Sokoban, _) => {
                    let (soko_level, boulder_coords) = generate_sokoban_level(depth);
                    self.level = soko_level;

                    // Spawn puzzle boulders
                    for bc in boulder_coords {
                        let boulder = create_item_record(
                            ItemKindId::Boulder,
                            ItemLocation::Floor(bc),
                            Buc::Uncursed,
                        );
                        self.arena.spawn_item(boulder);
                    }

                    // Spawn prize in prize chamber (at depth 1: Bag of Holding)
                    let prize_coord = Coord::new_unchecked(64, 10);
                    let prize_kind = if depth == 1 {
                        ItemKindId::BagOfHolding
                    } else {
                        ItemKindId::AmuletOfReflection
                    };
                    let prize = create_item_record(
                        prize_kind,
                        ItemLocation::Floor(prize_coord),
                        Buc::Blessed,
                    );
                    self.arena.spawn_item(prize);

                    events.push(GameEvent::LogMessage {
                        text: "You step into the legendary Sokoban puzzle maze! Boulders and pits line the corridors.".into(),
                    });
                }
                (BranchId::GnomishMines, 3) => {
                    // Minetown! Complete with shops, temple, priest, and watchmen
                    let layout = netrust_dungeon::generate_minetown_level(&mut self.rng);
                    self.level = layout.level;

                    self.spawn_monster_near(MonsterSpeciesId::Priest, layout.priest_coord);

                    for wc in layout.watchmen_coords {
                        self.spawn_monster_near(MonsterSpeciesId::Watchman, wc);
                    }

                    for sc in layout.shopkeeper_coords {
                        self.spawn_monster_near(MonsterSpeciesId::Shopkeeper, sc);
                    }

                    events.push(GameEvent::LogMessage {
                        text: "Welcome to Minetown! Bustling shops and an ancient sanctuary stand before you.".into(),
                    });
                }
                (BranchId::GnomishMines, 5) => {
                    // Mines' End! Sprawling labyrinth with guaranteed Luckstone
                    let (lvl, luckstone_coord) =
                        netrust_dungeon::generate_mines_end_level(&mut self.rng);
                    self.level = lvl;

                    let luckstone = create_item_record(
                        ItemKindId::Luckstone,
                        ItemLocation::Floor(luckstone_coord),
                        Buc::Uncursed,
                    );
                    self.arena.spawn_item(luckstone);

                    let centers: Vec<Coord> = self.level.rooms.iter().map(|r| r.center()).collect();
                    for (i, &center) in centers.iter().enumerate() {
                        if i > 0 {
                            let species = if i % 2 == 0 {
                                MonsterSpeciesId::SilverDragon
                            } else {
                                MonsterSpeciesId::Vampire
                            };
                            self.spawn_monster_near(species, center);
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

                    let centers: Vec<Coord> = self.level.rooms.iter().map(|r| r.center()).collect();
                    for (i, &center) in centers.iter().enumerate() {
                        if i > 0 {
                            let species = if i % 2 == 0 {
                                MonsterSpeciesId::Gnome
                            } else {
                                MonsterSpeciesId::Dwarf
                            };
                            self.spawn_monster_near(species, center);
                        }
                    }

                    events.push(GameEvent::LogMessage {
                        text: "You descend into the rugged, dark caverns of the Gnomish Mines."
                            .into(),
                    });
                }
                (BranchId::Gehennom, 1) => {
                    // Valley of the Dead
                    let lvl = generate_valley_of_the_dead(&mut self.rng);
                    self.level = lvl;

                    let centers: Vec<Coord> = self.level.rooms.iter().map(|r| r.center()).collect();
                    for (i, &center) in centers.iter().enumerate() {
                        if i > 0 {
                            let species = if i % 2 == 0 {
                                MonsterSpeciesId::Skeleton
                            } else {
                                MonsterSpeciesId::Vampire
                            };
                            self.spawn_monster_near(species, center);
                        }
                    }

                    // Spawn Bell of Opening in Valley of the Dead
                    let bell_coord = self.level.rooms[1].center();
                    let bell = create_item_record(
                        ItemKindId::BellOfOpening,
                        ItemLocation::Floor(bell_coord),
                        Buc::Blessed,
                    );
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

                    let centers: Vec<Coord> = self.level.rooms.iter().map(|r| r.center()).collect();
                    for (i, &center) in centers.iter().enumerate() {
                        if i > 0 {
                            self.spawn_monster_near(MonsterSpeciesId::SilverDragon, center);
                        }
                    }

                    // Invocation items on free floor reachable from the up stairs
                    let vs_avoid: Vec<Coord> = self.vibrating_square.into_iter().collect();
                    let rects: Vec<netrust_dungeon::Rect> =
                        self.level.rooms.iter().map(|r| r.rect).collect();
                    let reach = netrust_dungeon::reachable_from(&self.level, self.level.stairs_up);
                    let mut order: Vec<Coord> =
                        netrust_dungeon::find_free_floor(&self.level, &rects[1], &vs_avoid)
                            .into_iter()
                            .filter(|c| reach.contains(c))
                            .collect();
                    for (i, rect) in rects.iter().enumerate() {
                        if i == 1 {
                            continue;
                        }
                        for c in netrust_dungeon::find_free_floor(&self.level, rect, &vs_avoid) {
                            if reach.contains(&c) && !order.contains(&c) {
                                order.push(c);
                            }
                        }
                    }
                    let mut spots = order.into_iter();
                    if let Some(c) = spots.next() {
                        self.arena.spawn_item(create_item_record(
                            ItemKindId::CandelabrumOfInvocation,
                            ItemLocation::Floor(c),
                            Buc::Uncursed,
                        ));
                    }
                    if let Some(c) = spots.next() {
                        self.arena.spawn_item(create_item_record(
                            ItemKindId::BookOfTheDead,
                            ItemLocation::Floor(c),
                            Buc::Blessed,
                        ));
                    }
                    for c in spots.take(7) {
                        self.arena.spawn_item(create_item_record(
                            ItemKindId::WaxCandle,
                            ItemLocation::Floor(c),
                            Buc::Uncursed,
                        ));
                    }

                    events.push(GameEvent::LogMessage {
                        text: "You reach the infernal bottom of Gehennom. A cryptic vibration resonates beneath the stone.".into(),
                    });
                }
                (BranchId::Gehennom, SANCTUM_DEPTH) => {
                    // Moloch's Sanctum
                    let (lvl, _spawn) = generate_moloch_sanctum_level(&mut self.rng);
                    self.level = lvl;

                    self.spawn_monster_near(MonsterSpeciesId::Priest, self.level.stairs_down);

                    let amulet = create_item_record(
                        ItemKindId::AmuletOfYendor,
                        ItemLocation::Floor(self.level.stairs_down),
                        Buc::Blessed,
                    );
                    self.arena.spawn_item(amulet);

                    events.push(GameEvent::LogMessage {
                        text: "You enter Moloch's Sanctum! Rivers of boiling lava surround the unholy high altar!".into(),
                    });
                }
                (BranchId::Gehennom, d) => {
                    // Intermediate Gehennom Mazes
                    let (lvl, _) = generate_gehennom_maze_level(&mut self.rng, d, false);
                    self.level = lvl;

                    let centers: Vec<Coord> = self.level.rooms.iter().map(|r| r.center()).collect();
                    for (i, &center) in centers.iter().enumerate() {
                        if i > 0 {
                            let species = if i % 2 == 0 {
                                MonsterSpeciesId::SilverDragon
                            } else {
                                MonsterSpeciesId::Vampire
                            };
                            self.spawn_monster_near(species, center);
                        }
                    }

                    events.push(GameEvent::LogMessage {
                        text: format!("You delve through the fiery, twisting corridors of Gehennom (level {d})."),
                    });
                }
                (BranchId::Quest, 1) => {
                    let layout =
                        netrust_dungeon::generate_quest_home_level(&mut self.rng, &self.role_name);
                    self.level = layout.level;

                    let quest_cfg = netrust_core::get_role_quest_config_or_default(&self.role_name);
                    let leader_species = quest_species_by_name(quest_cfg.leader_name);
                    // Leader and guardians are M2_PEACEFUL (and MS_LEADER /
                    // MS_GUARDIAN), so peace_minded makes them peaceful.
                    self.spawn_monster_near(leader_species, layout.leader_coord);

                    // C role.c `guardnum`: one guardian species per role.
                    let guardian_species = quest_species_by_name(quest_cfg.guardian_name);
                    for gc in layout.guardian_coords {
                        self.spawn_monster_near(guardian_species, gc);
                    }

                    events.push(GameEvent::LogMessage {
                        text: format!(
                            "You enter the Sanctuary of {}: '{}'.",
                            quest_cfg.leader_name, quest_cfg.home_desc
                        ),
                    });
                }
                (BranchId::Quest, 2) => {
                    let lvl = netrust_dungeon::generate_quest_locate_level(&mut self.rng, 2);
                    self.level = lvl;

                    let centers: Vec<Coord> = self.level.rooms.iter().map(|r| r.center()).collect();
                    for (i, &center) in centers.iter().enumerate() {
                        if i > 0 {
                            let species = if i % 2 == 0 {
                                MonsterSpeciesId::GiantAnt
                            } else {
                                MonsterSpeciesId::Skeleton
                            };
                            self.spawn_monster_near(species, center);
                        }
                    }

                    events.push(GameEvent::LogMessage {
                        text: "You navigate the treacherous labyrinth of the Quest trial.".into(),
                    });
                }
                (BranchId::Quest, 3) => {
                    let layout =
                        netrust_dungeon::generate_quest_goal_level(&mut self.rng, &self.role_name);
                    self.level = layout.level;

                    let quest_cfg = netrust_core::get_role_quest_config_or_default(&self.role_name);
                    let nemesis_species = quest_species_by_name(quest_cfg.nemesis_name);
                    if let Some(id) = self.spawn_monster_near(nemesis_species, layout.nemesis_coord)
                    {
                        // The Tourist nemesis is the (M2_PEACEFUL) Master of Thieves;
                        // the quest goal level creates it with `peaceful = 0`
                        // (Tou-goal.lua:117). Every other nemesis is M2_HOSTILE.
                        if let Some(a) = self.arena.actors.get_mut(id) {
                            a.is_peaceful = false;
                        }
                    }

                    events.push(GameEvent::LogMessage {
                        text: format!("You arrive at the inner sanctum: {}! {} glares at you with burning hatred!", quest_cfg.goal_desc, quest_cfg.nemesis_name),
                    });
                }
                _ => {
                    let mut new_level = generate_dungeon_level(&mut self.rng);

                    // Place branch stairs in Dungeons of Doom
                    if branch == BranchId::DungeonsOfDoom && depth == 3 {
                        if let Some(room) = new_level.rooms.get(1) {
                            let branch_coord = Coord::new_unchecked(room.x1 + 1, room.y1 + 1);
                            new_level.set_tile(
                                branch_coord,
                                Tile::BranchStairs {
                                    branch: BranchId::GnomishMines,
                                    level: 1,
                                    up: false,
                                },
                            );
                        }
                    }
                    if branch == BranchId::DungeonsOfDoom && depth == 4 {
                        if let Some(room) = new_level.rooms.get(1) {
                            let branch_coord = Coord::new_unchecked(room.x1 + 1, room.y1 + 1);
                            new_level.set_tile(
                                branch_coord,
                                Tile::BranchStairs {
                                    branch: BranchId::Sokoban,
                                    level: 1,
                                    up: false,
                                },
                            );
                        }
                        if let Some(room) = new_level.rooms.get(2) {
                            let quest_coord = Coord::new_unchecked(room.x1 + 1, room.y1 + 1);
                            new_level.set_tile(
                                quest_coord,
                                Tile::BranchStairs {
                                    branch: BranchId::Quest,
                                    level: 1,
                                    up: false,
                                },
                            );
                        }
                    }
                    if branch == BranchId::DungeonsOfDoom && depth == 5 {
                        if let Some(room) = new_level.rooms.first() {
                            let branch_coord = Coord::new_unchecked(room.x1 + 1, room.y1 + 1);
                            new_level.set_tile(
                                branch_coord,
                                Tile::BranchStairs {
                                    branch: BranchId::Gehennom,
                                    level: 1,
                                    up: false,
                                },
                            );
                        }
                    }

                    self.level = new_level;

                    // Spawn monsters appropriate for depth
                    let centers: Vec<Coord> = self.level.rooms.iter().map(|r| r.center()).collect();
                    for (i, &center) in centers.iter().enumerate() {
                        if i > 0 && i != centers.len() - 1 {
                            let species = match depth {
                                1 => MonsterSpeciesId::Goblin,
                                2 => {
                                    if i % 2 == 0 {
                                        MonsterSpeciesId::Hobgoblin
                                    } else {
                                        MonsterSpeciesId::Orc
                                    }
                                }
                                3 => {
                                    if i % 2 == 0 {
                                        MonsterSpeciesId::GiantAnt
                                    } else {
                                        MonsterSpeciesId::Skeleton
                                    }
                                }
                                4 => MonsterSpeciesId::Vampire,
                                _ => MonsterSpeciesId::SilverDragon,
                            };
                            let arch = netrust_data::get_monster_species(species);
                            if !netrust_core::genocide::is_genocided(
                                &self.genocide_registry,
                                arch.name,
                                arch.glyph,
                            ) {
                                self.spawn_monster_near(species, center);
                            }
                        }
                    }

                    // At depth 5 in Dungeons of Doom, spawn the Amulet of Yendor
                    if branch == BranchId::DungeonsOfDoom && depth == 5 {
                        if let Some(deepest_room) = self.level.rooms.last() {
                            let amulet = create_item_record(
                                ItemKindId::AmuletOfYendor,
                                ItemLocation::Floor(deepest_room.center()),
                                Buc::Blessed,
                            );
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
        if self.current_branch == BranchId::Gehennom
            && self.depth == 5
            && self.vibrating_square == Some(player.coord)
        {
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
                self.place_steed_with_hero();

                events.push(GameEvent::LevelChanged {
                    from_depth,
                    to_depth: self.depth,
                });
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
                if self.current_branch == BranchId::Quest && self.depth == 1 {
                    let p_lvl = self
                        .arena
                        .actors
                        .get(self.player_id)
                        .map(|p| p.level)
                        .unwrap_or(1);
                    let hero_elig = netrust_core::HeroQuestEligibility {
                        experience_level: p_lvl,
                        alignment_record: self.alignment_record,
                        is_hostile_to_leader: false,
                    };
                    let quest_cfg = netrust_core::get_role_quest_config_or_default(&self.role_name);
                    if self.quest_state.progress == netrust_core::QuestProgress::Unassigned {
                        if netrust_core::consult_leader(&mut self.quest_state, &hero_elig).is_err()
                        {
                            events.push(GameEvent::LogMessage {
                                text: netrust_i18n::Messages::quest_leader_reject_level(
                                    quest_cfg.leader_name,
                                    netrust_core::QUEST_MIN_LEVEL,
                                    self.locale,
                                ),
                            });
                            return events;
                        } else {
                            events.push(GameEvent::LogMessage {
                                text: netrust_i18n::Messages::quest_leader_accept(
                                    quest_cfg.leader_name,
                                    quest_cfg.artifact_name,
                                    quest_cfg.nemesis_name,
                                    self.locale,
                                ),
                            });
                        }
                    }
                }

                let from_depth = self.depth;
                self.pack_current_level();
                self.depth += 1;
                let gen_events = self.unpack_or_generate_level(self.current_branch, self.depth);
                events.extend(gen_events);

                let new_coord = self.level.stairs_up;
                if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                    p.coord = new_coord;
                }
                self.place_steed_with_hero();

                events.push(GameEvent::LevelChanged {
                    from_depth,
                    to_depth: self.depth,
                });
                events.push(GameEvent::LogMessage {
                    text: format!("You descend deeper into dungeon level {}.", self.depth),
                });
                self.scheduler.hero_act(NORMAL_SPEED);
            }
            Tile::BranchStairs {
                branch,
                level,
                up: false,
            } => {
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
                self.place_steed_with_hero();

                events.push(GameEvent::LevelChanged {
                    from_depth,
                    to_depth: self.depth,
                });
                events.push(GameEvent::LogMessage {
                    text: format!("You enter the {branch:?} branch (level {level})."),
                });
                self.scheduler.hero_act(NORMAL_SPEED);
            }
            _ => {
                events.push(GameEvent::LogMessage {
                    text: netrust_i18n::Messages::no_stairs_down(self.locale).into(),
                });
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
                    let has_amulet =
                        self.arena
                            .items_carried_by(self.player_id)
                            .iter()
                            .any(|&iid| {
                                self.arena
                                    .items
                                    .get(iid)
                                    .is_some_and(crate::actions::items::is_real_amulet)
                            });
                    if self.current_branch == BranchId::Gehennom && has_amulet {
                        let hero_align = self
                            .arena
                            .actors
                            .get(self.player_id)
                            .map_or(netrust_core::Alignment::Neutral, |p| p.alignment);
                        let t = self.rng.random::<u32>();
                        let a = self.rng.random::<u32>();
                        let b = self.rng.random::<u32>();
                        let c = self.rng.random::<u32>();
                        let outcome = netrust_core::mysterious_force(
                            self.depth,
                            SANCTUM_DEPTH,
                            self.mysterious_force_count,
                            hero_align,
                            t,
                            a,
                            b,
                        );
                        if outcome != netrust_core::MysteriousForceOutcome::NoEffect {
                            self.mysterious_force_count = self
                                .mysterious_force_count
                                .saturating_add(netrust_core::mysterious_force_counter_increment(
                                    self.depth, outcome, c,
                                ));
                            events.push(GameEvent::LogMessage {
                                text: "A mysterious force momentarily surrounds you...".into(),
                            });
                        }
                        if outcome == netrust_core::MysteriousForceOutcome::SameLevelTeleport {
                            self.teleport_hero_randomly();
                            self.scheduler.hero_act(NORMAL_SPEED);
                            return events;
                        }
                        let pushed = match outcome {
                            netrust_core::MysteriousForceOutcome::PushDown(p) => {
                                Some(clamp_mysterious_force(
                                    p,
                                    netrust_core::is_sanctum_accessible(self.ritual_progress),
                                ))
                            }
                            _ => None,
                        };
                        if let Some(pushed_depth) = pushed.filter(|&p| p != self.depth) {
                            let from_depth = self.depth;
                            self.pack_current_level();
                            self.depth = pushed_depth;
                            let gen_events =
                                self.unpack_or_generate_level(BranchId::Gehennom, self.depth);
                            events.extend(gen_events);

                            let new_coord = self.level.stairs_up;
                            if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                                p.coord = new_coord;
                            }
                            self.place_steed_with_hero();

                            events.push(GameEvent::LevelChanged {
                                from_depth,
                                to_depth: self.depth,
                            });
                            events.push(GameEvent::LogMessage {
                                text: format!("An eldritch Mysterious Force pushes you downward to level {pushed_depth}!"),
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
                    self.place_steed_with_hero();

                    events.push(GameEvent::LevelChanged {
                        from_depth,
                        to_depth: self.depth,
                    });
                    events.push(GameEvent::LogMessage {
                        text: format!("You ascend to dungeon level {}.", self.depth),
                    });
                    self.scheduler.hero_act(NORMAL_SPEED);
                } else if self.current_branch == BranchId::DungeonsOfDoom {
                    // Surface check for Victory with Amulet of Yendor
                    let has_amulet =
                        self.arena
                            .items_carried_by(self.player_id)
                            .iter()
                            .any(|&iid| {
                                self.arena
                                    .items
                                    .get(iid)
                                    .is_some_and(crate::actions::items::is_real_amulet)
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
                        self.place_steed_with_hero();

                        events.push(GameEvent::LevelChanged {
                            from_depth,
                            to_depth: self.depth,
                        });
                        events.push(GameEvent::LogMessage {
                            text: format!(
                                "You return to {:?} level {}.",
                                parent.branch, parent.depth
                            ),
                        });
                        self.scheduler.hero_act(NORMAL_SPEED);
                    }
                }
            }
            Tile::BranchStairs {
                branch,
                level,
                up: true,
            } => {
                if self.current_branch == BranchId::Quest && self.depth == 1 {
                    let quest_cfg = netrust_core::get_role_quest_config_or_default(&self.role_name);
                    if self.quest_state.progress == netrust_core::QuestProgress::NemesisDefeated
                        && self.quest_state.artifact_location
                            == netrust_core::ArtifactLocation::CarriedByHero
                    {
                        netrust_core::return_to_leader_with_artifact(&mut self.quest_state);
                        events.push(GameEvent::LogMessage {
                            text: netrust_i18n::Messages::quest_completed(
                                quest_cfg.leader_name,
                                quest_cfg.artifact_name,
                                self.locale,
                            ),
                        });
                    }
                }

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
                self.place_steed_with_hero();

                events.push(GameEvent::LevelChanged {
                    from_depth,
                    to_depth: self.depth,
                });
                events.push(GameEvent::LogMessage {
                    text: format!("You return to {branch:?} level {level}."),
                });
                self.scheduler.hero_act(NORMAL_SPEED);
            }
            _ => {
                events.push(GameEvent::LogMessage {
                    text: netrust_i18n::Messages::no_stairs_up(self.locale).into(),
                });
            }
        }

        events
    }
}
