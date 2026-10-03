//! Sokoban puzzle and boulder pushing mechanics.
//!
//! Formally verified in Lean 4 (`NetMechanics.Sokoban`).

use netrust_types::{Coord, Direction, Tile};

/// Result of attempting to push a boulder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushOutcome {
    /// Boulder successfully pushed into an empty walkable cell.
    Moved(Coord),
    /// Boulder pushed into an unfilled pit, filling it and consuming the boulder.
    FilledPit(Coord),
    /// Pushing blocked by a wall, closed door, another boulder, or an entity.
    Blocked,
}

/// Calculate the push outcome when an entity attempts to push a boulder located at `boulder_pos`
/// in the given `dir` towards `target_tile`.
pub fn push_boulder(
    boulder_pos: Coord,
    dir: Direction,
    target_tile: &Tile,
    is_target_occupied: bool,
) -> PushOutcome {
    if is_target_occupied {
        return PushOutcome::Blocked;
    }

    let Some(next_pos) = boulder_pos.step(dir) else {
        return PushOutcome::Blocked;
    };

    match target_tile {
        Tile::Wall { .. } | Tile::Stone | Tile::Door { .. } | Tile::SecretDoor { .. } | Tile::Lava => {
            PushOutcome::Blocked
        }
        Tile::Pit { filled: false } => PushOutcome::FilledPit(next_pos),
        Tile::Room | Tile::Corr | Tile::Pit { filled: true } | Tile::Stairs { .. } | Tile::BranchStairs { .. } | Tile::Altar { .. } => {
            PushOutcome::Moved(next_pos)
        }
        Tile::Pool { frozen } => {
            if *frozen {
                PushOutcome::Moved(next_pos)
            } else {
                PushOutcome::Blocked
            }
        }
    }
}
