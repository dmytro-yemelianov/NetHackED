//! Interactive agent session managing game state, stepping, and inspections.

use nethacked_data::roles::CharacterConfig;
use nethacked_sim::{ActionAst, Coord, GameEvent, SimulationWorld};

use crate::ascii::render_ascii_map;
use crate::observation::{ActorObservation, GameObservation, TileInspection};

/// High-level session managing an interactive agent interaction.
pub struct AgentSession {
    pub world: SimulationWorld,
    pub last_events: Vec<GameEvent>,
}

impl AgentSession {
    pub fn new(seed: u64) -> Self {
        Self {
            world: SimulationWorld::new_with_seed(seed),
            last_events: Vec::new(),
        }
    }

    pub fn new_with_character(seed: u64, config: CharacterConfig) -> Self {
        Self {
            world: SimulationWorld::new_with_character(seed, config),
            last_events: Vec::new(),
        }
    }

    pub fn new_with_ruleset(
        seed: u64,
        config: CharacterConfig,
        rs: std::sync::Arc<nethacked_data::ruleset::Ruleset>,
        rref: nethacked_data::ruleset::RulesetRef,
    ) -> Self {
        Self {
            world: SimulationWorld::new_with_character_and_ruleset(seed, config, rs, rref),
            last_events: Vec::new(),
        }
    }

    /// Current player coordinate, if the player actor exists.
    pub fn player_coord(&self) -> Option<Coord> {
        self.world
            .arena
            .actors
            .get(self.world.player_id)
            .map(|p| p.coord)
    }

    pub fn get_observation(&self) -> GameObservation {
        let player = self.world.arena.actors.get(self.world.player_id);
        let player_coord = player
            .map(|p| p.coord)
            .unwrap_or(Coord::new_unchecked(0, 0));
        let player_hp = player.map(|p| p.hp).unwrap_or(0);
        let player_max_hp = player.map(|p| p.max_hp).unwrap_or(0);
        let player_ac = player.map(|p| p.ac).unwrap_or(10);
        let is_game_over = player.map(|p| p.is_dead).unwrap_or(true);

        let (_visible, detected_monsters) = self.world.compute_perception();

        let mut visible_actors = Vec::new();
        for (id, actor) in self.world.arena.actors.iter() {
            if !actor.is_dead && (id == self.world.player_id || detected_monsters.contains(&id)) {
                visible_actors.push(ActorObservation {
                    name: actor.name.clone(),
                    coord: actor.coord,
                    hp: actor.hp,
                    max_hp: actor.max_hp,
                    is_player: id == self.world.player_id,
                });
            }
        }

        GameObservation {
            turn: self.world.scheduler.turn,
            depth: self.world.depth,
            player_coord,
            player_hp,
            player_max_hp,
            player_ac,
            player_nutrition: self.world.player_nutrition,
            player_pw: self.world.player_pw,
            player_max_pw: self.world.player_max_pw,
            player_gold: self.world.player_gold,
            visible_actors,
            ascii_map: render_ascii_map(&self.world),
            last_events: self.last_events.clone(),
            is_game_over,
        }
    }

    pub fn step(&mut self, action: ActionAst) -> GameObservation {
        self.last_events = self.world.step_player_action(action);
        self.get_observation()
    }

    pub fn inspect_tile(&self, x: usize, y: usize) -> Result<TileInspection, String> {
        let coord = Coord::new(x, y)
            .ok_or_else(|| format!("Coordinate ({x}, {y}) is outside the dungeon map"))?;
        let tile = self.world.level.get_tile(coord).clone();
        let occupant = self.world.actor_at(coord).and_then(|id| {
            self.world.arena.actors.get(id).map(|a| ActorObservation {
                name: a.name.clone(),
                coord: a.coord,
                hp: a.hp,
                max_hp: a.max_hp,
                is_player: id == self.world.player_id,
            })
        });

        Ok(TileInspection {
            coord,
            is_passable: tile.is_passable(),
            is_transparent: tile.is_transparent(),
            tile,
            occupant,
        })
    }
}
