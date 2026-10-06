//! Complete, deterministic simulation engine for NetHackED.

pub mod actions;
mod ad_effects;
pub mod bones;
pub mod combat;
pub mod events;
pub mod monsters;
pub mod peace;
pub mod progress;
pub mod turns;
pub mod world;

pub use actions::economy::EconomyLedger;
pub use actions::items::{is_real_amulet, normalize_wish_name};
pub use actions::stairs::{clamp_mysterious_force, SANCTUM_DEPTH};
pub use events::GameEvent;
pub use world::{default_rng, LoadError, SimulationWorld, StoredLevel, DEFAULT_PLAYER_CON};

// Re-exports from related crates for convenient downstream consumption
pub use nethacked_core::nutrition::hunger_of_nutrition;
pub use nethacked_core::{ActionAst, HungerState, SpellKind};
pub use nethacked_data::ruleset::{Ruleset, RulesetRef};
pub use nethacked_data::{CharacterConfig, Gender, ItemKindId, MonsterSpeciesId, RaceId, RoleId};
pub use nethacked_types::{
    Alignment, Buc, Coord, Direction, DoorState, Intrinsics, ItemClass, Tile, COLNO, ROWNO,
};
