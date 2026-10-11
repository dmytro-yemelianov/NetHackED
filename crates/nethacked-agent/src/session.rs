//! Interactive agent session managing game state, stepping, and inspections.

use nethacked_data::roles::CharacterConfig;
use nethacked_sim::owner_persistence::OwnerGameSession;
use nethacked_sim::{ActionAst, Coord, GameEvent, SimulationWorld};

use crate::ascii::render_ascii_map;
use crate::observation::{ActorObservation, GameObservation, TileInspection};

/// High-level session managing an interactive agent interaction.
///
/// Gameplay dispatches through the owner runtime: the world is imported into
/// the independent owners once at construction and every action reaches owner
/// execution through [`OwnerGameSession`]; there is no World reconstruction
/// fallback. [`AgentSession::world`] exposes the read-only native observation
/// surface as of the last step.
pub struct AgentSession {
    session: OwnerGameSession,
    pub last_events: Vec<GameEvent>,
}

impl AgentSession {
    pub fn new(seed: u64) -> Result<Self, String> {
        Self::from_world(SimulationWorld::new_with_seed(seed))
    }

    pub fn new_with_character(seed: u64, config: CharacterConfig) -> Result<Self, String> {
        Self::from_world(SimulationWorld::new_with_character(seed, config))
    }

    pub fn new_with_ruleset(
        seed: u64,
        config: CharacterConfig,
        rs: std::sync::Arc<nethacked_data::ruleset::Ruleset>,
        rref: nethacked_data::ruleset::RulesetRef,
    ) -> Result<Self, String> {
        Self::from_world(SimulationWorld::new_with_character_and_ruleset(
            seed, config, rs, rref,
        ))
    }

    /// Import an existing world (e.g. a fixture mutated before construction).
    pub fn new_with_world(world: SimulationWorld) -> Result<Self, String> {
        Self::from_world(world)
    }

    fn from_world(world: SimulationWorld) -> Result<Self, String> {
        Ok(Self {
            session: OwnerGameSession::new(world).map_err(|e| e.to_string())?,
            last_events: Vec::new(),
        })
    }

    /// Read-only native world surface as of the last step.
    pub fn world(&self) -> &SimulationWorld {
        self.session.world()
    }

    /// Current player coordinate, if the player actor exists.
    pub fn player_coord(&self) -> Option<Coord> {
        self.session
            .world()
            .arena
            .actors
            .get(self.session.world().player_id)
            .map(|p| p.coord)
    }

    pub fn get_observation(&self) -> GameObservation {
        observe_world(self.session.world(), &self.last_events)
    }

    pub fn step(&mut self, action: ActionAst) -> Result<GameObservation, String> {
        self.last_events = self.session.step(action).map_err(|e| e.to_string())?;
        Ok(self.get_observation())
    }

    /// Reset to a fresh world (new game): the world is re-imported into the
    /// owner runtime exactly once.
    pub fn reset(&mut self, world: SimulationWorld) -> Result<(), String> {
        self.session.reset(world).map_err(|e| e.to_string())?;
        self.last_events.clear();
        Ok(())
    }

    /// Change the active locale for game event text and logging.
    pub fn set_locale(&mut self, locale: nethacked_types::Locale) -> Result<(), String> {
        self.session.set_locale(locale).map_err(|e| e.to_string())
    }

    pub fn inspect_tile(&self, x: usize, y: usize) -> Result<TileInspection, String> {
        let world = self.session.world();
        let coord = Coord::new(x, y)
            .ok_or_else(|| format!("Coordinate ({x}, {y}) is outside the dungeon map"))?;
        let tile = world.level.get_tile(coord).clone();
        let occupant = world.actor_at(coord).and_then(|id| {
            world.arena.actors.get(id).map(|a| ActorObservation {
                name: a.name.clone(),
                coord: a.coord,
                hp: a.hp,
                max_hp: a.max_hp,
                is_player: id == world.player_id,
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

/// Build a [`GameObservation`] directly from a native world and event slice.
///
/// Legacy oracle harnesses (arena benchmarks, golden tests) step
/// `SimulationWorld` themselves and observe through this without installing
/// an owner runtime.
pub(crate) fn observe_world(world: &SimulationWorld, last_events: &[GameEvent]) -> GameObservation {
    let player = world.arena.actors.get(world.player_id);
    let player_coord = player
        .map(|p| p.coord)
        .unwrap_or(Coord::new_unchecked(0, 0));
    let player_hp = player.map(|p| p.hp).unwrap_or(0);
    let player_max_hp = player.map(|p| p.max_hp).unwrap_or(0);
    let player_ac = player.map(|p| p.ac).unwrap_or(10);
    let is_game_over = player.map(|p| p.is_dead).unwrap_or(true);

    let (_visible, detected_monsters) = world.compute_perception();

    let mut visible_actors = Vec::new();
    for (id, actor) in world.arena.actors.iter() {
        if !actor.is_dead && (id == world.player_id || detected_monsters.contains(&id)) {
            visible_actors.push(ActorObservation {
                name: actor.name.clone(),
                coord: actor.coord,
                hp: actor.hp,
                max_hp: actor.max_hp,
                is_player: id == world.player_id,
            });
        }
    }

    GameObservation {
        turn: world.scheduler.turn,
        depth: world.depth,
        player_coord,
        player_hp,
        player_max_hp,
        player_ac,
        player_nutrition: world.player_nutrition,
        player_pw: world.player_pw,
        player_max_pw: world.player_max_pw,
        player_gold: world.player_gold,
        visible_actors,
        ascii_map: render_ascii_map(world),
        last_events: last_events.to_vec(),
        is_game_over,
    }
}
