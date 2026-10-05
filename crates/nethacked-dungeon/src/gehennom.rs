//! Procedural generation for Gehennom, the Valley of the Dead, and Moloch's Sanctum.
//!
//! Models the infernal subterranean domain of NetHack featuring lava pools,
//! mazes, the Vibrating Square on the bottom level, and Moloch's Sanctum.

use nethacked_types::{Alignment, BranchId, Coord, Tile, COLNO, ROWNO};
use rand::Rng;

use crate::generator::{carve_h_corr, carve_room, carve_v_corr};
use crate::level::DungeonLevel;
use crate::room::{Rect, Room, RoomType};

/// Generates the Valley of the Dead connecting the Castle to Gehennom proper.
pub fn generate_valley_of_the_dead<R: Rng>(rng: &mut R) -> DungeonLevel {
    let mut level = DungeonLevel::new_solid(Tile::Stone);
    let mut rooms = Vec::new();

    // 3 central necropolis chambers
    let central_y = ROWNO / 2;
    for i in 0..3 {
        let x = 10 + i * 22;
        let y = central_y.saturating_sub(4);
        let rect = Rect::new(x, y, 16, 8);
        carve_room(&mut level, &rect);
        rooms.push(Room::new(rect, RoomType::Normal));
    }

    // Connect necropolis halls with gloomy corridors
    for i in 0..(rooms.len() - 1) {
        let c1 = rooms[i].center();
        let c2 = rooms[i + 1].center();
        carve_h_corr(&mut level, c1.x, c2.x, c1.y);
    }

    // Stairs Up connecting back to Castle
    let up_coord = rooms[0].center();
    level.stairs_up = up_coord;
    level.set_tile(
        up_coord,
        Tile::BranchStairs {
            branch: BranchId::DungeonsOfDoom,
            level: 5,
            up: true,
        },
    );

    // Stairs Down to Gehennom maze level 1
    let down_coord = rooms[rooms.len() - 1].center();
    level.stairs_down = down_coord;
    level.set_tile(down_coord, Tile::Stairs { up: false });

    // Scattered lava rivers in surrounding stone
    for _ in 0..8 {
        let lx = rng.random_range(4..COLNO - 4);
        let ly = rng.random_range(2..ROWNO - 2);
        let c = Coord::new_unchecked(lx, ly);
        if *level.get_tile(c) == Tile::Stone {
            level.set_tile(c, Tile::Lava);
        }
    }

    level.rooms = rooms;
    level.is_dark = true;
    level
}

/// Generates an infernal Gehennom maze level carved through stone and lava.
///
/// If `has_vibrating_square` is true (the deepest floor of Gehennom),
/// the Vibrating Square coordinate is chosen on an open floor tile.
pub fn generate_gehennom_maze_level<R: Rng>(
    rng: &mut R,
    _depth: usize,
    has_vibrating_square: bool,
) -> (DungeonLevel, Option<Coord>) {
    let mut level = DungeonLevel::new_solid(Tile::Stone);
    let mut rooms = Vec::new();

    // 4 to 6 chambers separated by rugged corridors
    let num_rooms = rng.random_range(4..=6);
    for _ in 0..num_rooms {
        let w = rng.random_range(6..=12);
        let h = rng.random_range(4..=7);
        let x = rng.random_range(2..(COLNO - w - 2));
        let y = rng.random_range(2..(ROWNO - h - 2));
        let rect = Rect::new(x, y, w, h);

        carve_room(&mut level, &rect);
        rooms.push(Room::new(rect, RoomType::Normal));
    }

    // Connect rooms
    for i in 0..(rooms.len() - 1) {
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

    let up_c = rooms[0].center();
    level.stairs_up = up_c;
    level.set_tile(up_c, Tile::Stairs { up: true });

    let vibrating_square = if has_vibrating_square {
        // Bottom level: no standard stairs down; Vibrating Square acts as the gateway!
        let last_room = rooms.last().unwrap();
        let vs_coord = Coord::new_unchecked(last_room.x1 + 2, last_room.y1 + 2);
        level.stairs_down = vs_coord;
        Some(vs_coord)
    } else {
        let down_c = rooms[rooms.len() - 1].center();
        level.stairs_down = down_c;
        level.set_tile(down_c, Tile::Stairs { up: false });
        None
    };

    // Spawn lava fissures, never cutting off a room or the way down
    let mut keep: Vec<Coord> = rooms.iter().map(|r| r.center()).collect();
    keep.push(level.stairs_down);
    for r in &rooms {
        let c = Coord::new_unchecked(r.x1 + 1, r.y1 + 1);
        if c == level.stairs_up
            || c == level.stairs_down
            || Some(c) == vibrating_square
            || *level.get_tile(c) != Tile::Room
        {
            continue;
        }
        level.set_tile(c, Tile::Lava);
        let reach = crate::reach::reachable_from(&level, level.stairs_up);
        if !keep.iter().all(|k| reach.contains(k)) {
            level.set_tile(c, Tile::Room);
        }
    }

    level.rooms = rooms;
    level.is_dark = true;
    (level, vibrating_square)
}

/// Generates Moloch's Sanctum level.
///
/// Features a massive temple surrounded by a lake of fiery lava.
/// Contains the High Altar of Moloch and High Priest chamber.
pub fn generate_moloch_sanctum_level<R: Rng>(_rng: &mut R) -> (DungeonLevel, Coord) {
    let mut level = DungeonLevel::new_solid(Tile::Stone);

    // Carve infernal lake of lava around the inner temple
    for y in 2..ROWNO - 2 {
        for x in 4..COLNO - 4 {
            level.set_tile(Coord::new_unchecked(x, y), Tile::Lava);
        }
    }

    // Island Sanctuary for Moloch's Temple: Center (x: 24..56, y: 4..16)
    let s_x = 24;
    let s_y = 4;
    let s_w = 32;
    let s_h = 13;

    for y in s_y..(s_y + s_h) {
        for x in s_x..(s_x + s_w) {
            level.set_tile(Coord::new_unchecked(x, y), Tile::Room);
        }
    }

    // Temple outer walls
    for x in s_x..(s_x + s_w) {
        level.set_tile(
            Coord::new_unchecked(x, s_y),
            Tile::Wall { horizontal: true },
        );
        level.set_tile(
            Coord::new_unchecked(x, s_y + s_h - 1),
            Tile::Wall { horizontal: true },
        );
    }
    for y in s_y..(s_y + s_h) {
        level.set_tile(
            Coord::new_unchecked(s_x, y),
            Tile::Wall { horizontal: false },
        );
        level.set_tile(
            Coord::new_unchecked(s_x + s_w - 1, y),
            Tile::Wall { horizontal: false },
        );
    }

    // Gate in the western wall
    let gate_y = s_y + s_h / 2;
    level.set_tile(Coord::new_unchecked(s_x, gate_y), Tile::Room);

    // Bridge/causeway spanning the lava to the western shore
    for x in 4..s_x {
        level.set_tile(Coord::new_unchecked(x, gate_y), Tile::Room);
    }

    // High Altar of Moloch in center of Sanctum
    let altar_coord = Coord::new_unchecked(s_x + s_w / 2, gate_y);
    level.set_tile(
        altar_coord,
        Tile::HighAltar {
            align: Alignment::Chaotic,
        },
    );

    // Portal entrance spawn point on western shore
    let entrance_spawn = Coord::new_unchecked(5, gate_y);
    level.set_tile(entrance_spawn, Tile::Stairs { up: true });
    level.stairs_up = entrance_spawn;
    level.stairs_down = altar_coord;

    let sanctum_rect = Rect::new(s_x, s_y, s_w, s_h);
    level.rooms = vec![Room::new(
        sanctum_rect,
        RoomType::Temple {
            alignment: Alignment::Chaotic,
        },
    )];
    level.is_dark = true;

    (level, entrance_spawn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    #[test]
    fn test_valley_of_the_dead_generation() {
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let level = generate_valley_of_the_dead(&mut rng);
        assert!(!level.rooms.is_empty());
        assert!(level.is_dark);
        assert!(matches!(
            level.get_tile(level.stairs_up),
            Tile::BranchStairs { .. }
        ));
        assert!(matches!(
            level.get_tile(level.stairs_down),
            Tile::Stairs { up: false }
        ));
    }

    #[test]
    fn test_gehennom_maze_with_vibrating_square() {
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let (level, vs) = generate_gehennom_maze_level(&mut rng, 5, true);
        assert!(vs.is_some());
        assert!(level.is_dark);
        let vs_coord = vs.unwrap();
        assert_eq!(*level.get_tile(vs_coord), Tile::Room);
    }

    #[test]
    fn test_moloch_sanctum_generation() {
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let (level, spawn) = generate_moloch_sanctum_level(&mut rng);
        assert!(level.is_dark);
        assert!(level.is_passable(spawn));

        let mut has_altar = false;
        let mut has_lava = false;
        for row in &level.tiles {
            for tile in row {
                if matches!(tile, Tile::HighAltar { .. }) {
                    has_altar = true;
                }
                if matches!(tile, Tile::Lava) {
                    has_lava = true;
                }
            }
        }
        assert!(has_altar);
        assert!(has_lava);
    }
}
