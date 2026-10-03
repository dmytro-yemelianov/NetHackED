//! Observable game events emitted during simulation steps.

use netrust_arena::{ActorId, ItemId};
use netrust_types::{Coord, DoorState};
use serde::{Deserialize, Serialize};

/// Structured observable game events emitted to the UI/presentation layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameEvent {
    ActorMoved {
        actor: ActorId,
        from: Coord,
        to: Coord,
    },
    AttackLanded {
        attacker: ActorId,
        target: ActorId,
        damage: u32,
        lethal: bool,
    },
    AttackMissed {
        attacker: ActorId,
        target: ActorId,
    },
    BeamPropagated {
        path: Vec<Coord>,
    },
    DoorToggled {
        coord: Coord,
        new_state: DoorState,
    },
    ItemPickedUp {
        actor: ActorId,
        item: ItemId,
    },
    ItemDropped {
        actor: ActorId,
        item: ItemId,
    },
    ItemWielded {
        actor: ActorId,
        item: ItemId,
    },
    LevelChanged {
        from_depth: usize,
        to_depth: usize,
    },
    TurnAdvanced {
        turn: u64,
    },
    EngravingAdded {
        coord: Coord,
        text: String,
    },
    EngravingDegraded {
        coord: Coord,
        remaining: Option<String>,
    },
    Victory,
    LogMessage {
        text: String,
    },
}
