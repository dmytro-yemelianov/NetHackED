//! Dungeon Grid, Field of View (FOV), and Procedural Generation for NetRust.

pub mod fov;
pub mod generator;
pub mod level;
pub mod raycast;
pub mod room;
pub mod sokoban;
pub mod endgame;
pub mod mines;
pub mod gehennom;
pub mod quest;
pub mod reach;

pub use fov::{compute_fov, compute_illumination};
pub use generator::{
    carve_h_corr, carve_room, carve_v_corr, generate_dungeon_level, validate_stair_connectivity,
};
pub use level::DungeonLevel;
pub use reach::{find_free_floor, reachable_from, reachable_from_with};
pub use raycast::trace_beam_path;
pub use room::{Rect, Room, RoomType};
pub use sokoban::generate_sokoban_level;
pub use endgame::{generate_astral_plane, generate_castle_level};
pub use mines::{generate_mines_cavern_level, generate_minetown_level, generate_mines_end_level, MinetownLayout};
pub use gehennom::{
    generate_gehennom_maze_level, generate_moloch_sanctum_level, generate_valley_of_the_dead,
};
pub use quest::{
    generate_quest_goal_level, generate_quest_home_level, generate_quest_locate_level,
    QuestGoalLayout, QuestHomeLayout,
};

#[cfg(test)]
mod tests {
    use super::*;
    use netrust_types::{Coord, Direction, Tile, ROWNO};
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    #[test]
    fn test_fov_reflexivity() {
        let level = DungeonLevel::new_solid(Tile::Stone);
        let origin = Coord::new(10, 10).unwrap();
        let fov = compute_fov(&level, origin, 5);
        assert!(fov.contains(&origin));
    }

    #[test]
    fn test_fov_open_room_visibility() {
        let mut level = DungeonLevel::new_solid(Tile::Stone);
        let room = Rect::new(5, 5, 8, 6);
        carve_room(&mut level, &room);

        let center = room.center();
        let fov = compute_fov(&level, center, 8);

        assert!(fov.contains(&Coord::new(6, 6).unwrap()));
        assert!(fov.contains(&Coord::new(7, 7).unwrap()));
    }

    #[test]
    fn test_procedural_generation_connectivity() {
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let level = generate_dungeon_level(&mut rng);

        assert!(!level.rooms.is_empty());
        assert!(validate_stair_connectivity(&level));
    }

    #[test]
    fn test_procedural_generation_shops_and_altars() {
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let level = generate_dungeon_level(&mut rng);

        assert!(level.rooms.len() >= 3);
        assert_eq!(level.rooms[0].room_type, RoomType::Normal);
        assert_eq!(level.rooms[1].room_type, RoomType::Shop);
        if level.rooms.len() >= 4 {
            assert!(matches!(level.rooms[2].room_type, RoomType::Temple { .. }));
            let temple_center = level.rooms[2].center();
            assert!(matches!(level.get_tile(temple_center), Tile::Altar { .. }));
        }
    }

    #[test]
    fn test_beam_wall_reflection() {
        let mut level = DungeonLevel::new_solid(Tile::Room);
        for y in 0..ROWNO {
            level.set_tile(Coord::new(15, y).unwrap(), Tile::Wall { horizontal: false });
        }

        let origin = Coord::new(12, 10).unwrap();
        let path = trace_beam_path(&level, origin, Direction::East, 6);

        assert!(path.contains(&Coord::new(13, 10).unwrap()));
        assert!(path.contains(&Coord::new(14, 10).unwrap()));
        assert!(!path.contains(&Coord::new(15, 10).unwrap()));
    }
}
