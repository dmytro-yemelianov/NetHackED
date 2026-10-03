//! Procedural room-and-corridor dungeon generator.

use std::collections::{HashSet, VecDeque};
use netrust_types::{Alignment, Coord, Tile, COLNO, ROWNO};
use rand::Rng;

use crate::level::DungeonLevel;
use crate::room::{Rect, Room, RoomType};

/// Carve a rectangular room into the level.
pub fn carve_room(level: &mut DungeonLevel, rect: &Rect) {
    for x in rect.x1..=rect.x2 {
        for y in rect.y1..=rect.y2 {
            let is_border = x == rect.x1 || x == rect.x2 || y == rect.y1 || y == rect.y2;
            let coord = Coord::new_unchecked(x, y);
            if is_border {
                level.set_tile(
                    coord,
                    Tile::Wall {
                        horizontal: y == rect.y1 || y == rect.y2,
                    },
                );
            } else {
                level.set_tile(coord, Tile::Room);
            }
        }
    }
}

/// Carve a horizontal corridor.
pub fn carve_h_corr(level: &mut DungeonLevel, x1: usize, x2: usize, y: usize) {
    let min = x1.min(x2);
    let max = x1.max(x2);
    for x in min..=max {
        let c = Coord::new_unchecked(x, y);
        if !matches!(level.get_tile(c), Tile::Room) {
            level.set_tile(c, Tile::Corr);
        }
    }
}

/// Carve a vertical corridor.
pub fn carve_v_corr(level: &mut DungeonLevel, y1: usize, y2: usize, x: usize) {
    let min = y1.min(y2);
    let max = y1.max(y2);
    for y in min..=max {
        let c = Coord::new_unchecked(x, y);
        if !matches!(level.get_tile(c), Tile::Room) {
            level.set_tile(c, Tile::Corr);
        }
    }
}

/// Procedural room-and-corridor dungeon generator.
pub fn generate_dungeon_level<R: Rng>(rng: &mut R) -> DungeonLevel {
    let mut level = DungeonLevel::new_solid(Tile::Stone);
    let num_rooms = rng.random_range(4..=6);
    let mut rooms = Vec::new();

    for _ in 0..30 {
        if rooms.len() >= num_rooms {
            break;
        }
        let w = rng.random_range(6..=12);
        let h = rng.random_range(4..=7);
        let x = rng.random_range(1..=(COLNO - w - 2));
        let y = rng.random_range(1..=(ROWNO - h - 2));
        let new_rect = Rect::new(x, y, w, h);

        let overlap = rooms.iter().any(|r: &Rect| r.intersects(&new_rect));
        if !overlap {
            carve_room(&mut level, &new_rect);
            rooms.push(new_rect);
        }
    }

    // Connect sequential rooms with corridors
    for i in 0..rooms.len().saturating_sub(1) {
        let c1 = rooms[i].center();
        let c2 = rooms[i + 1].center();

        if rng.random_bool(0.5) {
            carve_h_corr(&mut level, c1.x, c2.x, c1.y);
            carve_v_corr(&mut level, c1.y, c2.y, c2.x);
        } else {
            carve_v_corr(&mut level, c1.y, c2.y, c1.x);
            carve_h_corr(&mut level, c1.x, c2.x, c2.y);
        }
    }

    // Place stairs
    if !rooms.is_empty() {
        let up = rooms[0].center();
        let down = rooms[rooms.len() - 1].center();
        level.set_tile(up, Tile::Stairs { up: true });
        level.set_tile(down, Tile::Stairs { up: false });
        level.stairs_up = up;
        level.stairs_down = down;
    }
    // Classify rooms (Room 0 is Normal with stairs up, Room 1 is Shop if >= 3 rooms, Room 2 is Temple with Altar if >= 4 rooms)
    let total_rooms = rooms.len();
    let mut classified_rooms = Vec::with_capacity(total_rooms);
    for (i, rect) in rooms.into_iter().enumerate() {
        let room_type = if i == 1 && total_rooms >= 3 {
            RoomType::Shop
        } else if i == 2 && total_rooms >= 4 {
            let align = match rng.random_range(0..3) {
                0 => Alignment::Lawful,
                1 => Alignment::Neutral,
                _ => Alignment::Chaotic,
            };
            level.set_tile(rect.center(), Tile::Altar { align });
            RoomType::Temple { alignment: align }
        } else {
            RoomType::Normal
        };
        classified_rooms.push(Room::new(rect, room_type));
    }
    level.rooms = classified_rooms;

    level
}

/// Verify reachability between stairs up and stairs down using BFS.
pub fn validate_stair_connectivity(level: &DungeonLevel) -> bool {
    let start = level.stairs_up;
    let target = level.stairs_down;
    if start == target {
        return true;
    }

    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back(start);
    visited.insert(start);

    while let Some(current) = queue.pop_front() {
        if current == target {
            return true;
        }

        for neighbor in current.neighbors() {
            if level.is_passable(neighbor) && !visited.contains(&neighbor) {
                visited.insert(neighbor);
                queue.push_back(neighbor);
            }
        }
    }

    false
}
