//! PyO3 Python bindings exposing NetRust as an RL Gymnasium environment.

use pyo3::prelude::*;
use pyo3::types::PyDict;
use netrust_agent::{render_ascii_map, AgentSession, GameObservation};
use netrust_core::ast::{ActionAst, Direction};
use netrust_core::engraving::EngravingMedium;
use netrust_sim::GameEvent;

#[pyclass]
pub struct NetRustEnv {
    session: AgentSession,
    seed: u64,
    step_count: usize,
    max_steps: usize,
}

#[pymethods]
impl NetRustEnv {
    #[new]
    #[pyo3(signature = (seed=None, max_steps=1000))]
    pub fn new(seed: Option<u64>, max_steps: Option<usize>) -> Self {
        let actual_seed = seed.unwrap_or(42);
        let max_s = max_steps.unwrap_or(1000);
        let session = AgentSession::new(actual_seed);
        Self {
            session,
            seed: actual_seed,
            step_count: 0,
            max_steps: max_s,
        }
    }

    /// Reset environment to initial or given seed.
    /// Returns (observation, info)
    #[pyo3(signature = (seed=None))]
    pub fn reset<'py>(&mut self, py: Python<'py>, seed: Option<u64>) -> PyResult<(Bound<'py, PyDict>, Bound<'py, PyDict>)> {
        if let Some(s) = seed {
            self.seed = s;
        }
        self.session = AgentSession::new(self.seed);
        self.step_count = 0;

        let obs = self.build_observation(py)?;
        let info = PyDict::new(py);
        info.set_item("seed", self.seed)?;
        info.set_item("turn", self.session.world.scheduler.turn)?;
        info.set_item("depth", self.session.world.depth)?;

        Ok((obs, info))
    }

    /// Step the environment with a discrete action index.
    /// Action indices:
    /// 0..7: Move compass (North, East, South, West, NE, SE, SW, NW)
    /// 8: Wait
    /// 9: Descend stairs
    /// 10: Ascend stairs
    /// 11: PickUp
    /// 12: Pay
    /// 13: Pray
    /// 14: Search / Elbereth
    /// Returns: (observation, reward, terminated, truncated, info)
    pub fn step<'py>(
        &mut self,
        py: Python<'py>,
        action: usize,
    ) -> PyResult<(Bound<'py, PyDict>, f64, bool, bool, Bound<'py, PyDict>)> {
        self.step_count += 1;
        let prev_depth = self.session.world.depth;
        let prev_gold = self.session.world.player_gold;

        let action_ast = match action {
            0 => ActionAst::Move(Direction::North),
            1 => ActionAst::Move(Direction::East),
            2 => ActionAst::Move(Direction::South),
            3 => ActionAst::Move(Direction::West),
            4 => ActionAst::Move(Direction::NorthEast),
            5 => ActionAst::Move(Direction::SouthEast),
            6 => ActionAst::Move(Direction::SouthWest),
            7 => ActionAst::Move(Direction::NorthWest),
            8 => ActionAst::Wait,
            9 => ActionAst::Descend,
            10 => ActionAst::Ascend,
            11 => ActionAst::PickUp,
            12 => ActionAst::Pay,
            13 => ActionAst::Pray,
            14 => ActionAst::Engrave {
                text: "Elbereth".into(),
                medium: EngravingMedium::Dust(1),
            },
            _ => ActionAst::Wait,
        };

        let obs_state = self.session.step(action_ast);
        let events = &self.session.last_events;

        // Compute reward
        let mut reward = -0.01; // Step penalty to encourage efficiency
        let player = self.session.world.arena.actors.get(self.session.world.player_id).cloned();
        let is_dead = player.as_ref().map(|p| p.is_dead || p.hp == 0).unwrap_or(true);
        let won = events.iter().any(|e| matches!(e, GameEvent::Victory));

        if won {
            reward += 1000.0;
        }
        if is_dead {
            reward -= 100.0;
        }
        if self.session.world.depth > prev_depth {
            reward += 50.0 * (self.session.world.depth - prev_depth) as f64;
        }
        if self.session.world.player_gold > prev_gold {
            reward += (self.session.world.player_gold - prev_gold) as f64 * 0.1;
        }
        for e in events {
            if let GameEvent::AttackLanded { damage, lethal, .. } = e {
                reward += *damage as f64 * 0.5;
                if *lethal {
                    reward += 10.0;
                }
            }
        }

        let terminated = is_dead || won;
        let truncated = self.step_count >= self.max_steps;

        let obs = self.obs_to_dict(py, &obs_state)?;
        let info = PyDict::new(py);
        info.set_item("step", self.step_count)?;
        info.set_item("turn", self.session.world.scheduler.turn)?;
        info.set_item("depth", self.session.world.depth)?;
        info.set_item("won", won)?;
        info.set_item("is_dead", is_dead)?;

        Ok((obs, reward, terminated, truncated, info))
    }

    /// Render current dungeon floor as ASCII string.
    pub fn render(&self) -> String {
        render_ascii_map(&self.session.world)
    }

    /// Inspect observation telemetry as JSON string.
    pub fn get_observation_json(&self) -> String {
        let obs = self.session.get_observation();
        serde_json::to_string_pretty(&obs).unwrap_or_else(|_| "{}".into())
    }
}

impl NetRustEnv {
    fn build_observation<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let obs = self.session.get_observation();
        self.obs_to_dict(py, &obs)
    }

    fn obs_to_dict<'py>(&self, py: Python<'py>, obs: &GameObservation) -> PyResult<Bound<'py, PyDict>> {
        let obs_dict = PyDict::new(py);
        obs_dict.set_item("ascii_map", &obs.ascii_map)?;
        obs_dict.set_item("player_x", obs.player_coord.x)?;
        obs_dict.set_item("player_y", obs.player_coord.y)?;
        obs_dict.set_item("player_hp", obs.player_hp)?;
        obs_dict.set_item("player_max_hp", obs.player_max_hp)?;
        obs_dict.set_item("player_ac", obs.player_ac)?;
        obs_dict.set_item("depth", obs.depth)?;
        obs_dict.set_item("gold", obs.player_gold)?;
        obs_dict.set_item("turn", obs.turn)?;
        obs_dict.set_item("nutrition", obs.player_nutrition)?;
        obs_dict.set_item("pw", obs.player_pw)?;
        obs_dict.set_item("is_dead", obs.is_game_over)?;
        obs_dict.set_item("num_visible_actors", obs.visible_actors.len())?;
        Ok(obs_dict)
    }
}

#[pymodule]
fn netrust_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<NetRustEnv>()?;
    Ok(())
}
