//! Dungeon Grid and Tile Mechanics.
//!
//! Formalized and verified in `NetMechanics.Grid`.
//! Replaces NetHack's overloaded 5-bit flags (`rm.h:struct rm`) with an algebraic sum type.

pub use nethacked_types::{Alignment, Coord, DoorState, Tile, COLNO, ROWNO};

#[cfg(test)]
mod tests {
    use super::*;

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
