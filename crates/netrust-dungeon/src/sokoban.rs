//! Procedural and template generation for Sokoban puzzle levels.

use crate::level::DungeonLevel;
use crate::room::{Rect, Room, RoomType};
use netrust_types::{BranchId, Coord, Tile};

/// Generates a Sokoban puzzle level with stairs up, puzzle corridors, pits, and boulder positions.
///
/// Returns the constructed `DungeonLevel` and a list of boulder coordinates to spawn.
pub fn generate_sokoban_level(_floor: usize) -> (DungeonLevel, Vec<Coord>) {
    let mut level = DungeonLevel::new_solid(Tile::Stone);

    // 1. Entry Room (left side)
    let entry_rect = Rect::new(4, 7, 7, 7);
    crate::generator::carve_room(&mut level, &entry_rect);

    let stairs_up = Coord::new_unchecked(6, 10);
    level.set_tile(
        stairs_up,
        Tile::BranchStairs {
            branch: BranchId::DungeonsOfDoom,
            level: 4,
            up: true,
        },
    );
    level.stairs_up = stairs_up;

    // 2. Prize Room (right side)
    let prize_rect = Rect::new(60, 7, 10, 7);
    crate::generator::carve_room(&mut level, &prize_rect);

    let prize_stairs = Coord::new_unchecked(65, 10);
    level.set_tile(prize_stairs, Tile::Stairs { up: false });
    level.stairs_down = prize_stairs;

    // 3. Central Puzzle Hallway (x: 11..60, y: 8..13)
    let puzzle_rect = Rect::new(11, 8, 49, 5);
    crate::generator::carve_room(&mut level, &puzzle_rect);

    // Add doorways connecting rooms
    level.set_tile(Coord::new_unchecked(11, 10), Tile::Room);
    level.set_tile(Coord::new_unchecked(60, 10), Tile::Room);

    // 4. Place Pit Chasm blocking the exit to the prize room (at x = 50, y = 9..12)
    for y in 9..=12 {
        level.set_tile(Coord::new_unchecked(50, y), Tile::Pit { filled: false });
    }

    // 5. Initial boulder positions in the puzzle hallway
    let boulder_coords = vec![
        Coord::new_unchecked(20, 10),
        Coord::new_unchecked(28, 9),
        Coord::new_unchecked(36, 11),
        Coord::new_unchecked(44, 10),
    ];

    level.rooms = vec![
        Room::new(entry_rect, RoomType::Normal),
        Room::new(puzzle_rect, RoomType::Normal),
        Room::new(prize_rect, RoomType::Normal),
    ];

    (level, boulder_coords)
}
