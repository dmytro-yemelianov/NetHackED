//! Structured observation payloads for LLMs and AI agents.

use netrust_sim::{Coord, GameEvent, Tile};
use serde::{Deserialize, Serialize};

/// Observation of a visible actor for an LLM agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActorObservation {
    pub name: String,
    pub coord: Coord,
    pub hp: u32,
    pub max_hp: u32,
    pub is_player: bool,
}

/// Comprehensive, structured state observation delivered to an agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameObservation {
    pub turn: u64,
    pub depth: usize,
    pub player_coord: Coord,
    pub player_hp: u32,
    pub player_max_hp: u32,
    pub player_ac: i32,
    pub player_nutrition: u32,
    pub player_pw: u32,
    pub player_max_pw: u32,
    pub player_gold: u32,
    pub visible_actors: Vec<ActorObservation>,
    pub ascii_map: String,
    pub last_events: Vec<GameEvent>,
    pub is_game_over: bool,
}

/// Detailed inspection of a specific tile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TileInspection {
    pub coord: Coord,
    pub tile: Tile,
    pub is_passable: bool,
    pub is_transparent: bool,
    pub occupant: Option<ActorObservation>,
}
