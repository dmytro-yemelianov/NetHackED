//! Procedural generation for the Gnomish Mines, Minetown, and Mines' End.
//!
//! Provides distinct cavern topology, Minetown commercial and temple structures,
//! and Mines' End deepest sanctuary with guaranteed Luckstone placement.

use netrust_types::{Alignment, BranchId, Coord, Tile, COLNO, ROWNO};
use rand::Rng;

use crate::generator::{carve_h_corr, carve_room, carve_v_corr};
use crate::level::DungeonLevel;
use crate::room::{Rect, Room, RoomType};

/// Result payload of generating a Minetown level.
#[derive(Debug, Clone)]
pub struct MinetownLayout {
    pub level: DungeonLevel,
    pub priest_coord: Coord,
    pub watchmen_coords: Vec<Coord>,
    pub shopkeeper_coords: Vec<Coord>,
    pub altar_coord: Coord,
}

/// Generates a standard Gnomish Mines cavern floor (Mines 1, 2, 4).
pub fn generate_mines_cavern_level<R: Rng>(rng: &mut R, sub_depth: usize) -> DungeonLevel {
    let mut level = DungeonLevel::new_solid(Tile::Stone);
    let mut rooms = Vec::new();

    // 4 to 6 irregular cavern chambers
    let num_rooms = rng.random_range(4..=6);
    for _ in 0..num_rooms {
        let w = rng.random_range(6..=12);
        let h = rng.random_range(5..=9);
        let x = rng.random_range(2..(COLNO - w - 2));
        let y = rng.random_range(2..(ROWNO - h - 2));
        let rect = Rect::new(x, y, w, h);

        carve_room(&mut level, &rect);
        rooms.push(Room::new(rect, RoomType::Normal));
    }

    // Connect cavern chambers with rugged winding corridors
    for i in 0..(rooms.len() - 1) {
        let r1 = &rooms[i];
        let r2 = &rooms[i + 1];
        let c1 = r1.center();
        let c2 = r2.center();

        if rng.random_bool(0.5) {
            carve_h_corr(&mut level, c1.x, c2.x, c1.y);
            carve_v_corr(&mut level, c1.y, c2.y, c2.x);
        } else {
            carve_v_corr(&mut level, c1.y, c2.y, c1.x);
            carve_h_corr(&mut level, c1.x, c2.x, c2.y);
        }
    }

    // Stairs Up (if level 1, connects back to Dungeons of Doom Dlvl 3)
    let up_c = rooms[0].center();
    if sub_depth == 1 {
        level.set_tile(
            up_c,
            Tile::BranchStairs {
                branch: BranchId::DungeonsOfDoom,
                level: 3,
                up: true,
            },
        );
    } else {
        level.set_tile(up_c, Tile::Stairs { up: true });
    }
    level.stairs_up = up_c;

    // Stairs Down (towards deeper mines)
    let down_c = rooms.last().unwrap().center();
    level.set_tile(down_c, Tile::Stairs { up: false });
    level.stairs_down = down_c;

    level.rooms = rooms;
    level
}

/// Generates Minetown (Mines level 3) complete with shops, temple, priest, and watchmen.
pub fn generate_minetown_level<R: Rng>(_rng: &mut R) -> MinetownLayout {
    let mut level = DungeonLevel::new_solid(Tile::Stone);

    // 1. Central Town Plaza
    let plaza_rect = Rect::new(28, 7, 24, 8);
    carve_room(&mut level, &plaza_rect);

    // 2. High Temple (north-east quadrant)
    let temple_rect = Rect::new(56, 3, 16, 8);
    carve_room(&mut level, &temple_rect);

    // Altar placed inside Temple
    let altar_coord = Coord::new_unchecked(64, 7);
    level.set_tile(altar_coord, Tile::Altar { align: Alignment::Lawful });

    // Priest stands directly in front of the altar
    let priest_coord = Coord::new_unchecked(64, 8);

    // 3. General Store (north-west)
    let shop1_rect = Rect::new(6, 3, 16, 7);
    carve_room(&mut level, &shop1_rect);

    // 4. Lighting & Delicatessen Store (south-west)
    let shop2_rect = Rect::new(6, 12, 16, 7);
    carve_room(&mut level, &shop2_rect);

    // General Store door (east wall) -> corridor -> plaza west wall
    level.set_tile(Coord::new_unchecked(22, 8), Tile::Room);
    // Delicatessen door (east wall) -> corridor -> plaza west wall
    level.set_tile(Coord::new_unchecked(22, 13), Tile::Room);
    // Temple door (west wall) -> corridor -> plaza east wall
    level.set_tile(Coord::new_unchecked(56, 9), Tile::Room);

    carve_h_corr(&mut level, 22, 28, 8);
    carve_h_corr(&mut level, 22, 28, 13);
    carve_h_corr(&mut level, 52, 56, 9);

    // Stairs Up (to Mines 2) inside Plaza
    let up_c = Coord::new_unchecked(32, 10);
    level.set_tile(up_c, Tile::Stairs { up: true });
    level.stairs_up = up_c;

    // Stairs Down (to Mines 4) inside Plaza
    let down_c = Coord::new_unchecked(48, 10);
    level.set_tile(down_c, Tile::Stairs { up: false });
    level.stairs_down = down_c;

    let shopkeeper_coords = vec![
        Coord::new_unchecked(14, 6),
        Coord::new_unchecked(14, 15),
    ];

    let watchmen_coords = vec![
        Coord::new_unchecked(35, 11),
        Coord::new_unchecked(45, 11),
        Coord::new_unchecked(54, 9),
    ];

    level.rooms = vec![
        Room::new(plaza_rect, RoomType::Normal),
        Room::new(temple_rect, RoomType::Temple { alignment: Alignment::Lawful }),
        Room::new(shop1_rect, RoomType::Shop),
        Room::new(shop2_rect, RoomType::Shop),
    ];

    MinetownLayout {
        level,
        priest_coord,
        watchmen_coords,
        shopkeeper_coords,
        altar_coord,
    }
}

/// Generates Mines' End (Mines level 5) with a deep sanctuary and guaranteed Luckstone.
pub fn generate_mines_end_level<R: Rng>(rng: &mut R) -> (DungeonLevel, Coord) {
    let mut level = DungeonLevel::new_solid(Tile::Stone);
    let mut rooms = Vec::new();

    // Sprawling labyrinthine chambers
    let num_rooms = 5;
    for i in 0..num_rooms {
        let x = 6 + i * 14;
        let y = rng.random_range(4..=12);
        let rect = Rect::new(x, y, 10, 6);
        carve_room(&mut level, &rect);
        rooms.push(Room::new(rect, RoomType::Normal));
    }

    // Connect sequentially
    for i in 0..(rooms.len() - 1) {
        let c1 = rooms[i].center();
        let c2 = rooms[i + 1].center();
        carve_h_corr(&mut level, c1.x, c2.x, c1.y);
        carve_v_corr(&mut level, c1.y, c2.y, c2.x);
    }

    // Stairs Up (to Mines 4) in first chamber
    let up_c = rooms[0].center();
    level.set_tile(up_c, Tile::Stairs { up: true });
    level.stairs_up = up_c;

    // No stairs down in Mines' End!
    level.stairs_down = Coord::new_unchecked(0, 0);

    // Sanctuary chamber at the far end (Luckstone spawn location)
    let sanctuary_coord = rooms.last().unwrap().center();

    level.rooms = rooms;
    (level, sanctuary_coord)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    #[test]
    fn test_mines_cavern_generation() {
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let level = generate_mines_cavern_level(&mut rng, 1);
        assert!(matches!(level.get_tile(level.stairs_up), Tile::BranchStairs { branch: BranchId::DungeonsOfDoom, .. }));
        assert!(matches!(level.get_tile(level.stairs_down), Tile::Stairs { up: false }));
        assert!(level.rooms.len() >= 4);
    }

    #[test]
    fn test_minetown_generation() {
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let layout = generate_minetown_level(&mut rng);
        assert!(matches!(layout.level.get_tile(layout.altar_coord), Tile::Altar { align: Alignment::Lawful }));
        assert_eq!(layout.watchmen_coords.len(), 3);
        assert_eq!(layout.shopkeeper_coords.len(), 2);
    }

    #[test]
    fn test_mines_end_luckstone_location() {
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let (level, luckstone_coord) = generate_mines_end_level(&mut rng);
        assert!(level.is_passable(luckstone_coord));
        assert!(matches!(level.get_tile(level.stairs_up), Tile::Stairs { up: true }));
        // No down stairs in Mines' End
        assert_eq!(level.stairs_down, Coord::new_unchecked(0, 0));
    }
}
