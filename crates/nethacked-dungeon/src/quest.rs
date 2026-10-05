//! Procedural generation for the Class Quest branch.
//!
//! Generates the Quest Home (Leader Sanctuary), Quest Locate (Intermediate trial),
//! and Quest Goal (Nemesis Boss Lair).

use nethacked_types::{Alignment, BranchId, Coord, DoorState, Tile, COLNO, ROWNO};
use rand::Rng;

use crate::generator::{carve_h_corr, carve_room, carve_v_corr};
use crate::level::DungeonLevel;
use crate::room::{Rect, Room, RoomType};

/// Layout artifact returned from Quest Home level generation.
#[derive(Debug, Clone)]
pub struct QuestHomeLayout {
    pub level: DungeonLevel,
    pub leader_coord: Coord,
    pub guardian_coords: Vec<Coord>,
}

/// Layout artifact returned from Quest Goal level generation.
#[derive(Debug, Clone)]
pub struct QuestGoalLayout {
    pub level: DungeonLevel,
    pub nemesis_coord: Coord,
}

/// Generates the Quest Home level containing the Leader's sanctuary.
pub fn generate_quest_home_level<R: Rng>(rng: &mut R, _role_name: &str) -> QuestHomeLayout {
    let mut level = DungeonLevel::new_solid(Tile::Stone);

    // 1. Entrance Hall with Portal / Stairs back to Dungeons of Doom
    let entrance_rect = Rect::new(6, 6, 14, 9);
    carve_room(&mut level, &entrance_rect);
    let up_coord = Coord::new_unchecked(10, 10);
    level.stairs_up = up_coord;
    level.set_tile(
        up_coord,
        Tile::BranchStairs {
            branch: BranchId::DungeonsOfDoom,
            level: 4,
            up: true,
        },
    );

    // 2. Grand Sanctuary / Throne Room where Leader resides
    let sanctuary_rect = Rect::new(32, 4, 20, 13);
    carve_room(&mut level, &sanctuary_rect);
    let leader_coord = Coord::new_unchecked(42, 10);

    // Leader's sacred altar in the sanctuary
    let altar_coord = Coord::new_unchecked(42, 8);
    level.set_tile(
        altar_coord,
        Tile::Altar {
            align: Alignment::Neutral,
        },
    );

    // 3. Vault room with stairs down into Quest Locate
    let descent_rect = Rect::new(62, 7, 12, 7);
    carve_room(&mut level, &descent_rect);
    let down_coord = Coord::new_unchecked(68, 10);
    level.stairs_down = down_coord;
    level.set_tile(down_coord, Tile::Stairs { up: false });

    // 4. Corridors connecting entrance to sanctuary and sanctuary to descent
    carve_h_corr(&mut level, 20, 32, 10);
    carve_h_corr(&mut level, 52, 62, 10);

    // Add wooden doors at sanctuary thresholds
    level.set_tile(
        Coord::new_unchecked(32, 10),
        Tile::Door {
            state: DoorState::Open,
            trapped: false,
        },
    );
    level.set_tile(
        Coord::new_unchecked(51, 10),
        Tile::Door {
            state: DoorState::Closed,
            trapped: false,
        },
    );

    // Quest Guardians flanking the entrance to the Sanctuary
    let guardian_coords = vec![
        Coord::new_unchecked(35, 8),
        Coord::new_unchecked(35, 12),
        Coord::new_unchecked(48, 8),
        Coord::new_unchecked(48, 12),
    ];

    // Aesthetic lighting / decorative torches
    for _ in 0..4 {
        let rx = rng.random_range(34..48);
        let ry = rng.random_range(6..14);
        let c = Coord::new_unchecked(rx, ry);
        if *level.get_tile(c) == Tile::Room && c != leader_coord && c != altar_coord {
            // keep room floor
        }
    }

    QuestHomeLayout {
        level,
        leader_coord,
        guardian_coords,
    }
}

/// Generates Quest Locate intermediate trial level.
pub fn generate_quest_locate_level<R: Rng>(rng: &mut R, _depth: usize) -> DungeonLevel {
    let mut level = DungeonLevel::new_solid(Tile::Stone);
    let mut rooms = Vec::new();

    // 4 challenge chambers connected in a maze-like loop
    let coords = [
        (8, 4, 14, 6),
        (32, 3, 16, 7),
        (58, 5, 14, 6),
        (30, 13, 20, 6),
    ];

    for (x, y, w, h) in coords {
        let r = Rect::new(x, y, w, h);
        carve_room(&mut level, &r);
        rooms.push(Room::new(r, RoomType::Normal));
    }

    // Connect sequentially
    for i in 0..(rooms.len() - 1) {
        let c1 = rooms[i].center();
        let c2 = rooms[i + 1].center();
        carve_h_corr(&mut level, c1.x, c2.x, c1.y);
        carve_v_corr(&mut level, c1.y, c2.y, c2.x);
    }

    let up_c = rooms[0].center();
    level.stairs_up = up_c;
    level.set_tile(up_c, Tile::Stairs { up: true });

    let down_c = rooms[rooms.len() - 1].center();
    level.stairs_down = down_c;
    level.set_tile(down_c, Tile::Stairs { up: false });

    // Scattered hazard pits
    for _ in 0..5 {
        let px = rng.random_range(10..COLNO - 10);
        let py = rng.random_range(3..ROWNO - 3);
        let c = Coord::new_unchecked(px, py);
        if *level.get_tile(c) == Tile::Corr {
            level.set_tile(c, Tile::Pit { filled: false });
            if !crate::reach::reachable_from(&level, level.stairs_up).contains(&level.stairs_down) {
                level.set_tile(c, Tile::Corr);
            }
        }
    }

    level
}

/// Generates the Quest Goal level (Nemesis Boss Lair).
pub fn generate_quest_goal_level<R: Rng>(_rng: &mut R, role_name: &str) -> QuestGoalLayout {
    let mut level = DungeonLevel::new_solid(Tile::Stone);

    // 1. Arrival Chamber (Stairs Up)
    let arrival_rect = Rect::new(6, 8, 12, 6);
    carve_room(&mut level, &arrival_rect);
    let up_coord = Coord::new_unchecked(10, 10);
    level.stairs_up = up_coord;
    level.set_tile(up_coord, Tile::Stairs { up: true });

    // 2. Central Boss Citadel
    let citadel_rect = Rect::new(36, 4, 24, 13);
    carve_room(&mut level, &citadel_rect);
    let nemesis_coord = Coord::new_unchecked(48, 10);

    // High dark altar in the Nemesis citadel
    let unholy_altar = Coord::new_unchecked(52, 10);
    level.set_tile(
        unholy_altar,
        Tile::Altar {
            align: Alignment::Chaotic,
        },
    );

    // 3. Surrounding moat or lava river depending on role lore
    let is_volcano =
        role_name.eq_ignore_ascii_case("valkyrie") || role_name.eq_ignore_ascii_case("knight");

    for x in 34..62 {
        for y in [3, 17] {
            let c = Coord::new_unchecked(x, y);
            level.set_tile(c, if is_volcano { Tile::Lava } else { Tile::Moat });
        }
    }
    for y in 3..=17 {
        for x in [34, 61] {
            let c = Coord::new_unchecked(x, y);
            level.set_tile(c, if is_volcano { Tile::Lava } else { Tile::Moat });
        }
    }

    // Causeway / bridge crossing the moat into the citadel
    carve_h_corr(&mut level, 18, 36, 10);
    level.set_tile(Coord::new_unchecked(34, 10), Tile::Room);
    level.set_tile(Coord::new_unchecked(35, 10), Tile::Room);

    // Iron gate / locked door guarding the sanctum
    level.set_tile(
        Coord::new_unchecked(36, 10),
        Tile::Door {
            state: DoorState::Closed,
            trapped: false,
        },
    );

    // Minor decorative pillars
    level.set_tile(
        Coord::new_unchecked(42, 7),
        Tile::Wall { horizontal: false },
    );
    level.set_tile(
        Coord::new_unchecked(42, 13),
        Tile::Wall { horizontal: false },
    );
    level.set_tile(
        Coord::new_unchecked(54, 7),
        Tile::Wall { horizontal: false },
    );
    level.set_tile(
        Coord::new_unchecked(54, 13),
        Tile::Wall { horizontal: false },
    );

    QuestGoalLayout {
        level,
        nemesis_coord,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha20Rng;

    #[test]
    fn test_quest_home_generation() {
        let mut rng = ChaCha20Rng::seed_from_u64(12345);
        let layout = generate_quest_home_level(&mut rng, "Valkyrie");
        assert!(layout.level.is_passable(layout.leader_coord));
        assert!(matches!(
            layout.level.get_tile(layout.level.stairs_up),
            Tile::BranchStairs {
                branch: BranchId::DungeonsOfDoom,
                ..
            }
        ));
        assert!(matches!(
            layout.level.get_tile(layout.level.stairs_down),
            Tile::Stairs { up: false }
        ));
        assert!(!layout.guardian_coords.is_empty());
    }

    #[test]
    fn test_quest_locate_generation() {
        let mut rng = ChaCha20Rng::seed_from_u64(54321);
        let level = generate_quest_locate_level(&mut rng, 2);
        assert!(level.is_passable(level.stairs_up));
        assert!(level.is_passable(level.stairs_down));
    }

    #[test]
    fn test_quest_goal_generation() {
        let mut rng = ChaCha20Rng::seed_from_u64(99999);
        let layout = generate_quest_goal_level(&mut rng, "Valkyrie");
        assert!(layout.level.is_passable(layout.nemesis_coord));
        assert!(matches!(
            layout.level.get_tile(layout.level.stairs_up),
            Tile::Stairs { up: true }
        ));
    }
}
