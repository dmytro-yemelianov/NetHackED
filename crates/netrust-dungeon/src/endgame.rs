//! Endgame Level Generators: The Castle and The Astral Plane.
//!
//! Models Castle drawbridge fortresses and Astral Plane High Altars.

use crate::level::DungeonLevel;
use crate::room::Rect;
use netrust_types::{Alignment, BranchId, Coord, Tile, COLNO, ROWNO};
use rand::Rng;

/// Generates the Castle level featuring a central fortress surrounded by a moat and drawbridge.
///
/// Returns (level, player_spawn_coord).
pub fn generate_castle_level<R: Rng>(_rng: &mut R) -> (DungeonLevel, Coord) {
    let mut level = DungeonLevel::new_solid(Tile::Stone);

    // Carve surrounding courtyard
    for y in 2..ROWNO - 2 {
        for x in 2..COLNO - 2 {
            level.set_tile(Coord::new_unchecked(x, y), Tile::Room);
        }
    }

    // Fortress bounding box: center of map
    let f_x = 20;
    let f_y = 5;
    let f_w = 40;
    let f_h = 12;

    // Draw moat around fortress
    for x in f_x..f_x + f_w {
        level.set_tile(Coord::new_unchecked(x, f_y), Tile::Moat);
        level.set_tile(Coord::new_unchecked(x, f_y + f_h - 1), Tile::Moat);
    }
    for y in f_y..f_y + f_h {
        level.set_tile(Coord::new_unchecked(f_x, y), Tile::Moat);
        level.set_tile(Coord::new_unchecked(f_x + f_w - 1, y), Tile::Moat);
    }

    // Inside fortress walls
    for x in (f_x + 2)..(f_x + f_w - 2) {
        level.set_tile(Coord::new_unchecked(x, f_y + 2), Tile::Wall { horizontal: true });
        level.set_tile(Coord::new_unchecked(x, f_y + f_h - 3), Tile::Wall { horizontal: true });
    }
    for y in (f_y + 2)..(f_y + f_h - 2) {
        level.set_tile(Coord::new_unchecked(f_x + 2, y), Tile::Wall { horizontal: false });
        level.set_tile(Coord::new_unchecked(f_x + f_w - 3, y), Tile::Wall { horizontal: false });
    }

    // Drawbridge over western moat
    let bridge_y = f_y + f_h / 2;
    level.set_tile(Coord::new_unchecked(f_x, bridge_y), Tile::Drawbridge { open: false });
    // Fortress gate
    level.set_tile(Coord::new_unchecked(f_x + 2, bridge_y), Tile::Room);

    // Stairs up in courtyard
    let stairs_up = Coord::new_unchecked(5, 5);
    level.set_tile(stairs_up, Tile::Stairs { up: true });

    // Stairs down into Gehennom inside the throne room
    let stairs_down = Coord::new_unchecked(f_x + f_w / 2, bridge_y);
    level.set_tile(
        stairs_down,
        Tile::BranchStairs {
            branch: BranchId::Gehennom,
            level: 1,
            up: false,
        },
    );

    let player_spawn = Coord::new_unchecked(stairs_up.x + 1, stairs_up.y);
    (level, player_spawn)
}

/// Generates the Astral Plane featuring 3 Sanctuaries with High Altars (Lawful, Neutral, Chaotic).
///
/// Returns (level, player_spawn_coord).
pub fn generate_astral_plane<R: Rng>(_rng: &mut R) -> (DungeonLevel, Coord) {
    let mut level = DungeonLevel::new_solid(Tile::Stone);

    // Carve open astral void corridor connecting the 3 Sanctuaries
    for x in 10..70 {
        for y in 9..13 {
            level.set_tile(Coord::new_unchecked(x, y), Tile::Room);
        }
    }

    // 3 Sanctuaries: Left (Lawful), Center (Neutral), Right (Chaotic)
    let altars = [
        (15, 11, Alignment::Lawful),
        (40, 11, Alignment::Neutral),
        (65, 11, Alignment::Chaotic),
    ];

    for (ax, ay, align) in altars {
        let room_rect = Rect::new(ax - 3, ay - 3, 7, 7);
        for y in room_rect.y1..room_rect.y2 {
            for x in room_rect.x1..room_rect.x2 {
                level.set_tile(Coord::new_unchecked(x, y), Tile::Room);
            }
        }
        level.set_tile(Coord::new_unchecked(ax, ay), Tile::HighAltar { align });
    }

    // Arrival spawn in center astral passage
    let player_spawn = Coord::new_unchecked(30, 11);
    (level, player_spawn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    #[test]
    fn test_castle_has_drawbridge_and_moat() {
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let (level, _) = generate_castle_level(&mut rng);

        let mut has_drawbridge = false;
        let mut has_moat = false;

        for row in &level.tiles {
            for tile in row {
                if matches!(tile, Tile::Drawbridge { .. }) {
                    has_drawbridge = true;
                }
                if matches!(tile, Tile::Moat) {
                    has_moat = true;
                }
            }
        }

        assert!(has_drawbridge);
        assert!(has_moat);
    }

    #[test]
    fn test_astral_plane_has_three_high_altars() {
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let (level, _) = generate_astral_plane(&mut rng);

        let mut high_altars = 0;
        for row in &level.tiles {
            for tile in row {
                if matches!(tile, Tile::HighAltar { .. }) {
                    high_altars += 1;
                }
            }
        }

        assert_eq!(high_altars, 3);
    }
}
