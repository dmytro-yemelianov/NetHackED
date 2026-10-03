//! Complete, deterministic simulation engine for NetRust.

pub mod actions;
pub mod bones;
pub mod combat;
pub mod events;
pub mod monsters;
pub mod turns;
pub mod world;

pub use events::GameEvent;
pub use world::{default_rng, SimulationWorld, StoredLevel};

// Re-exports from related crates for convenient downstream consumption
pub use netrust_core::nutrition::hunger_of_nutrition;
pub use netrust_core::{ActionAst, HungerState, SpellKind};
pub use netrust_data::{CharacterConfig, Gender, RaceId, RoleId};
pub use netrust_types::{
    Alignment, Buc, Coord, Direction, DoorState, Intrinsics, ItemClass, Tile, COLNO, ROWNO,
};
