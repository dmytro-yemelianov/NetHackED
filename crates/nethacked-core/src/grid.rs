//! Dungeon Grid and Tile Mechanics.
//!
//! Modelled in Lean 4 in `NetMechanics.Grid` (machine-checked model; not formally linked to this code).
//! Replaces NetHack's overloaded 5-bit flags (`rm.h:struct rm`) with an algebraic sum type.

pub use nethacked_types::{Alignment, Coord, DoorState, Tile, COLNO, ROWNO};

/// Why a diagonal step is refused at a door (C `test_move`, hack.c:991).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagonalDoorBlock {
    /// The destination is an intact doorway (hack.c:1141).
    IntoDoorway,
    /// The mover stands in an intact doorway (hack.c:1209).
    OutOfDoorway,
}

/// Pure door rule of C `test_move` for a step of (`dx`, `dy`) from `from` to `to`.
///
/// A diagonal step is refused when the destination door is not doorless
/// (hack.c:1141, `!doorless_door(x, y)`, checked first) or when the mover
/// stands on such a door (hack.c:1209). `doorless_door` (hack.c:4063-4073)
/// treats `D_NODOOR` and `D_BROKEN` as doorless; open, closed and locked
/// doors are intact. Orthogonal steps are never blocked. Not modelled:
/// `Passes_walls` and the Rogue-level rule that makes every door intact
/// (hack.c:4071).
pub fn diagonal_door_block(
    from: &Tile,
    to: &Tile,
    dx: isize,
    dy: isize,
) -> Option<DiagonalDoorBlock> {
    if dx == 0 || dy == 0 {
        return None;
    }
    if to.has_intact_door() {
        Some(DiagonalDoorBlock::IntoDoorway)
    } else if from.has_intact_door() {
        Some(DiagonalDoorBlock::OutOfDoorway)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn door(state: DoorState) -> Tile {
        Tile::Door {
            state,
            trapped: false,
        }
    }

    /// hack.c:1141 (into) and :1209 (out of): diagonal steps are refused
    /// at doors that are not doorless (hack.c:4063), destination first.
    #[test]
    fn door_blocks_diagonal_truth_table() {
        let all = [
            DoorState::Open,
            DoorState::Closed,
            DoorState::Locked,
            DoorState::Broken,
            DoorState::NoDoor,
        ];
        let intact =
            |s: DoorState| matches!(s, DoorState::Open | DoorState::Closed | DoorState::Locked);
        let room = Tile::Room;
        for &s in &all {
            let d = door(s);
            for (dx, dy) in [(1, 1), (-1, 1), (1, -1), (-1, -1)] {
                // destination is the door
                let into = diagonal_door_block(&room, &d, dx, dy);
                assert_eq!(
                    into,
                    if intact(s) {
                        Some(DiagonalDoorBlock::IntoDoorway)
                    } else {
                        None
                    },
                    "into {s:?}"
                );
                // origin is the door
                let out = diagonal_door_block(&d, &room, dx, dy);
                assert_eq!(
                    out,
                    if intact(s) {
                        Some(DiagonalDoorBlock::OutOfDoorway)
                    } else {
                        None
                    },
                    "out of {s:?}"
                );
            }
            // orthogonal steps are never blocked
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                assert_eq!(diagonal_door_block(&room, &d, dx, dy), None);
                assert_eq!(diagonal_door_block(&d, &room, dx, dy), None);
            }
            // no movement
            assert_eq!(diagonal_door_block(&room, &d, 0, 0), None);
        }
        // destination is checked first when both are intact doors
        assert_eq!(
            diagonal_door_block(&door(DoorState::Open), &door(DoorState::Open), 1, 1),
            Some(DiagonalDoorBlock::IntoDoorway)
        );
        // broken destination, intact origin -> out of doorway
        assert_eq!(
            diagonal_door_block(&door(DoorState::Open), &door(DoorState::Broken), 1, 1),
            Some(DiagonalDoorBlock::OutOfDoorway)
        );
        // non-door terrain never blocks
        for t in [
            Tile::Room,
            Tile::Corr,
            Tile::Stairs { up: true },
            Tile::SecretDoor { locked: false },
            Tile::Wall { horizontal: true },
        ] {
            assert_eq!(diagonal_door_block(&t, &Tile::Room, 1, 1), None);
            assert_eq!(diagonal_door_block(&Tile::Room, &t, 1, 1), None);
        }
    }

    #[test]
    fn test_door_lifecycle() {
        let mut door = Tile::Door {
            state: DoorState::Closed,
            trapped: false,
        };
        assert!(!door.is_passable());

        door.open_door();
        assert_eq!(
            door,
            Tile::Door {
                state: DoorState::Open,
                trapped: false
            }
        );
        assert!(door.is_passable());

        door.close_door();
        assert_eq!(
            door,
            Tile::Door {
                state: DoorState::Closed,
                trapped: false
            }
        );
        assert!(!door.is_passable());

        door.break_door();
        assert_eq!(
            door,
            Tile::Door {
                state: DoorState::Broken,
                trapped: false
            }
        );
        assert!(door.is_passable());
    }

    #[test]
    fn test_secret_door_revelation() {
        let mut sdoor = Tile::SecretDoor { locked: true };
        assert!(!sdoor.is_passable());
        sdoor.reveal_secret_door();
        assert_eq!(
            sdoor,
            Tile::Door {
                state: DoorState::Locked,
                trapped: false
            }
        );
        assert!(!sdoor.is_passable());

        sdoor.unlock_door();
        assert_eq!(
            sdoor,
            Tile::Door {
                state: DoorState::Closed,
                trapped: false
            }
        );

        sdoor.open_door();
        assert!(sdoor.is_passable());
    }
}
