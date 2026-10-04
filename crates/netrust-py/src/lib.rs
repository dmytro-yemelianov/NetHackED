//! PyO3 Python bindings exposing NetRust as an RL Gymnasium environment.

use netrust_agent::{render_ascii_map, AgentSession, GameObservation};
use netrust_core::ast::{ActionAst, Direction};
use netrust_core::engraving::EngravingMedium;
use netrust_sim::GameEvent;
use netrust_types::ItemClass;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

/// (observation, reward, terminated, truncated, info) as returned by `step`.
type StepOutput<'py> = (Bound<'py, PyDict>, f64, bool, bool, Bound<'py, PyDict>);

pub const ACTION_NAMES: [&str; 26] = [
    "MOVE_N",
    "MOVE_E",
    "MOVE_S",
    "MOVE_W",
    "MOVE_NE",
    "MOVE_SE",
    "MOVE_SW",
    "MOVE_NW",
    "WAIT",
    "DESCEND",
    "ASCEND",
    "PICKUP",
    "SEARCH",
    "UNTRAP",
    "FIRE_N",
    "FIRE_E",
    "FIRE_S",
    "FIRE_W",
    "QUIVER",
    "EAT",
    "QUAFF",
    "READ",
    "ZAP_WAND",
    "PRAY",
    "PAY",
    "ENGRAVE_ELBERETH",
];

/// Tracks tiles seen on the current dungeon level; resets when the depth changes.
#[derive(Default)]
struct ExplorationTracker {
    depth: Option<u32>,
    tiles: std::collections::HashSet<netrust_types::Coord>,
}

impl ExplorationTracker {
    /// Record visible tiles at `depth`; returns how many were newly explored.
    /// A depth change discards the previous level's tiles first.
    fn observe(
        &mut self,
        depth: u32,
        visible: impl IntoIterator<Item = netrust_types::Coord>,
    ) -> usize {
        if self.depth != Some(depth) {
            self.tiles.clear();
            self.depth = Some(depth);
        }
        let before = self.tiles.len();
        self.tiles.extend(visible);
        self.tiles.len() - before
    }

    fn clear(&mut self) {
        self.tiles.clear();
        self.depth = None;
    }

    fn len(&self) -> usize {
        self.tiles.len()
    }
}

#[pyclass]
pub struct NetRustEnv {
    session: AgentSession,
    seed: u64,
    step_count: usize,
    max_steps: usize,
    conduct_masking: bool,
    explored: ExplorationTracker,
}

#[pymethods]
impl NetRustEnv {
    #[new]
    #[pyo3(signature = (seed=None, max_steps=1000, conduct_masking=true))]
    pub fn new(seed: Option<u64>, max_steps: Option<usize>, conduct_masking: Option<bool>) -> Self {
        let actual_seed = seed.unwrap_or(42);
        let max_s = max_steps.unwrap_or(1000);
        let mask = conduct_masking.unwrap_or(true);
        let session = AgentSession::new(actual_seed);
        let mut env = Self {
            session,
            seed: actual_seed,
            step_count: 0,
            max_steps: max_s,
            conduct_masking: mask,
            explored: ExplorationTracker::default(),
        };
        env.record_exploration();
        env
    }

    /// Reset environment to initial or given seed.
    /// Returns (observation, info)
    #[pyo3(signature = (seed=None))]
    pub fn reset<'py>(
        &mut self,
        py: Python<'py>,
        seed: Option<u64>,
    ) -> PyResult<(Bound<'py, PyDict>, Bound<'py, PyDict>)> {
        if let Some(s) = seed {
            self.seed = s;
        }
        self.session = AgentSession::new(self.seed);
        self.step_count = 0;
        self.explored.clear();
        self.record_exploration();

        let obs = self.build_observation(py)?;
        let info = PyDict::new(py);
        info.set_item("seed", self.seed)?;
        info.set_item("turn", self.session.world.scheduler.turn)?;
        info.set_item("depth", self.session.world.depth)?;

        Ok((obs, info))
    }

    /// Number of tiles explored on the current dungeon level.
    #[getter]
    pub fn explored_count(&self) -> usize {
        self.explored.len()
    }

    /// Number of discrete actions.
    pub fn action_space_size(&self) -> usize {
        ACTION_NAMES.len()
    }

    /// Meaning / name of each discrete action index.
    pub fn get_action_meanings(&self) -> Vec<String> {
        ACTION_NAMES.iter().map(|s| s.to_string()).collect()
    }

    /// Enable or disable strict voluntary conduct filtering in action masks.
    pub fn set_conduct_masking(&mut self, enabled: bool) {
        self.conduct_masking = enabled;
    }

    /// Get current action mask as a list of booleans (size 26).
    pub fn get_action_mask(&self) -> Vec<bool> {
        self.compute_action_mask()
    }

    /// Step the environment with a discrete action index (0..25).
    /// Returns: (observation, reward, terminated, truncated, info)
    pub fn step<'py>(&mut self, py: Python<'py>, action: usize) -> PyResult<StepOutput<'py>> {
        self.step_count += 1;
        let prev_depth = self.session.world.depth;
        let prev_gold = self.session.world.player_gold;
        let prev_pacifist = self.session.world.conducts.pacifist;
        let prev_vegan = self.session.world.conducts.vegan;
        let prev_illiterate = self.session.world.conducts.illiterate;
        let prev_atheist = self.session.world.conducts.atheist;

        let action_ast = self.resolve_action(action);
        let obs_state = self.session.step(action_ast);
        let events = self.session.last_events.clone();

        let new_explored = self.record_exploration();

        // Compute reward
        let mut reward = -0.01; // Step penalty to encourage efficiency
        let player = self
            .session
            .world
            .arena
            .actors
            .get(self.session.world.player_id)
            .cloned();
        let is_dead = player
            .as_ref()
            .map(|p| p.is_dead || p.hp == 0)
            .unwrap_or(true);
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
        if new_explored > 0 {
            reward += new_explored as f64 * 0.5;
        }

        // Conduct preservation bonus or violation penalty
        if prev_pacifist && !self.session.world.conducts.pacifist {
            reward -= 50.0;
        }
        if prev_vegan && !self.session.world.conducts.vegan {
            reward -= 20.0;
        }
        if prev_illiterate && !self.session.world.conducts.illiterate {
            reward -= 20.0;
        }
        if prev_atheist && !self.session.world.conducts.atheist {
            reward -= 20.0;
        }

        for e in events {
            if let GameEvent::AttackLanded { damage, lethal, .. } = e {
                reward += damage as f64 * 0.5;
                if lethal {
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
        info.set_item("gold", self.session.world.player_gold)?;

        Ok((obs, reward, terminated, truncated, info))
    }

    /// Render current dungeon floor as ASCII string.
    pub fn render(&self) -> String {
        render_ascii_map(&self.session.world)
    }

    /// Return active voluntary conducts as a Python dict.
    pub fn get_active_conducts<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let conducts = PyDict::new(py);
        let c = &self.session.world.conducts;
        conducts.set_item("pacifist", c.pacifist)?;
        conducts.set_item("vegan", c.vegan)?;
        conducts.set_item("vegetarian", c.vegetarian)?;
        conducts.set_item("atheist", c.atheist)?;
        conducts.set_item("illiterate", c.illiterate)?;
        conducts.set_item("genocideless", c.genocideless)?;
        conducts.set_item("polypileless", c.polypileless)?;
        conducts.set_item("wishless", c.wishless)?;
        Ok(conducts)
    }

    /// Inspect observation telemetry as JSON string.
    pub fn get_observation_json(&self) -> String {
        let obs = self.session.get_observation();
        serde_json::to_string_pretty(&obs).unwrap_or_else(|_| "{}".into())
    }
}

impl NetRustEnv {
    fn record_exploration(&mut self) -> usize {
        let (visible_tiles, _) = self.session.world.compute_perception();
        self.explored
            .observe(self.session.world.depth as u32, visible_tiles)
    }

    fn resolve_action(&self, action: usize) -> ActionAst {
        let carried = self
            .session
            .world
            .arena
            .items_carried_by(self.session.world.player_id);
        let player_coord = self
            .session
            .world
            .arena
            .actors
            .get(self.session.world.player_id)
            .map(|a| a.coord)
            .unwrap_or(netrust_types::Coord::new_unchecked(0, 0));

        match action {
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
            12 => ActionAst::Search,
            13 => {
                // Untrap adjacent trap if found, otherwise self tile
                let trap_coord = player_coord
                    .neighbors()
                    .into_iter()
                    .find(|c| self.session.world.level.traps.contains_key(c))
                    .unwrap_or(player_coord);
                ActionAst::Untrap(trap_coord)
            }
            14 => ActionAst::Fire(Direction::North),
            15 => ActionAst::Fire(Direction::East),
            16 => ActionAst::Fire(Direction::South),
            17 => ActionAst::Fire(Direction::West),
            18 => {
                // Quiver first suitable item (ammo/weapon)
                let item_to_quiver = carried
                    .iter()
                    .find(|&&id| {
                        if let Some(item) = self.session.world.arena.items.get(id) {
                            item.class == ItemClass::Weapon
                                || item.name.contains("arrow")
                                || item.name.contains("dart")
                        } else {
                            false
                        }
                    })
                    .copied();
                if let Some(id) = item_to_quiver {
                    ActionAst::Quiver(id)
                } else if let Some(&id) = carried.first() {
                    ActionAst::Quiver(id)
                } else {
                    ActionAst::Wait
                }
            }
            19 => {
                // Eat first food or corpse
                let food_idx = carried
                    .iter()
                    .position(|&id| {
                        if let Some(item) = self.session.world.arena.items.get(id) {
                            item.class == ItemClass::Food
                                || item.corpse_race.is_some()
                                || item.name.contains("corpse")
                        } else {
                            false
                        }
                    })
                    .unwrap_or(0);
                ActionAst::Eat(food_idx)
            }
            20 => {
                // Quaff first potion
                let pot_idx = carried
                    .iter()
                    .position(|&id| {
                        self.session
                            .world
                            .arena
                            .items
                            .get(id)
                            .map(|i| i.class == ItemClass::Potion)
                            .unwrap_or(false)
                    })
                    .unwrap_or(0);
                ActionAst::Quaff(pot_idx)
            }
            21 => {
                // Read first scroll
                let scroll_idx = carried
                    .iter()
                    .position(|&id| {
                        self.session
                            .world
                            .arena
                            .items
                            .get(id)
                            .map(|i| i.class == ItemClass::Scroll)
                            .unwrap_or(false)
                    })
                    .unwrap_or(0);
                ActionAst::Read(scroll_idx)
            }
            22 => ActionAst::ZapWand {
                dir: Direction::East,
                energy: netrust_agent::commands::ZAP_ENERGY,
            },
            23 => ActionAst::Pray,
            24 => ActionAst::Pay,
            25 => ActionAst::Engrave {
                text: "Elbereth".into(),
                medium: EngravingMedium::Dust(1),
            },
            _ => ActionAst::Wait,
        }
    }

    fn compute_action_mask(&self) -> Vec<bool> {
        let mut mask = vec![true; ACTION_NAMES.len()];
        let world = &self.session.world;
        let carried = world.arena.items_carried_by(world.player_id);
        let player = world.arena.actors.get(world.player_id);
        let player_coord = player
            .map(|p| p.coord)
            .unwrap_or(netrust_types::Coord::new_unchecked(0, 0));

        // Descend & Ascend: only valid on stairs
        let on_stairs_down = world.level.stairs_down == player_coord;
        let on_stairs_up = world.level.stairs_up == player_coord;
        mask[9] = on_stairs_down;
        mask[10] = on_stairs_up;

        // PickUp: only valid if floor items exist at player coord
        let floor_has_items = world
            .arena
            .items
            .iter()
            .any(|(_, it)| it.location == netrust_arena::ItemLocation::Floor(player_coord));
        mask[11] = floor_has_items;

        // Untrap: only valid if adjacent or current tile has a revealed trap
        let near_trap = world
            .level
            .traps
            .iter()
            .any(|(&coord, _)| coord == player_coord || player_coord.is_adjacent(coord));
        mask[13] = near_trap;

        // Quiver & Fire
        let has_quiverable = carried.iter().any(|&id| {
            world
                .arena
                .items
                .get(id)
                .map(|i| i.class == ItemClass::Weapon || i.name.contains("arrow"))
                .unwrap_or(false)
        });
        mask[18] = has_quiverable;
        let has_quivered = world.hero.quivered_item.is_some();
        mask[14] = has_quivered;
        mask[15] = has_quivered;
        mask[16] = has_quivered;
        mask[17] = has_quivered;

        // Eat: only valid if carried has food
        let has_food = carried.iter().any(|&id| {
            world
                .arena
                .items
                .get(id)
                .map(|i| i.class == ItemClass::Food || i.corpse_race.is_some())
                .unwrap_or(false)
        });
        mask[19] = has_food;

        // Quaff: only valid if carried has potion
        let has_potion = carried.iter().any(|&id| {
            world
                .arena
                .items
                .get(id)
                .map(|i| i.class == ItemClass::Potion)
                .unwrap_or(false)
        });
        mask[20] = has_potion;

        // Read: only valid if carried has scroll
        let has_scroll = carried.iter().any(|&id| {
            world
                .arena
                .items
                .get(id)
                .map(|i| i.class == ItemClass::Scroll)
                .unwrap_or(false)
        });
        mask[21] = has_scroll;

        // Zap wand: only valid if carried has wand
        let has_wand = carried.iter().any(|&id| {
            world
                .arena
                .items
                .get(id)
                .map(|i| i.class == ItemClass::Wand)
                .unwrap_or(false)
        });
        mask[22] = has_wand;

        // Conduct constraints masking
        if self.conduct_masking {
            let c = &world.conducts;
            if c.illiterate {
                mask[21] = false; // Read
                mask[25] = false; // Engrave
            }
            if c.atheist {
                mask[23] = false; // Pray
            }
            if c.pacifist {
                // If pacifist, mask out fire if an enemy is in that direction
                mask[14] = false;
                mask[15] = false;
                mask[16] = false;
                mask[17] = false;

                // Also check moves into adjacent hostile actors
                let dirs = [
                    Direction::North,
                    Direction::East,
                    Direction::South,
                    Direction::West,
                    Direction::NorthEast,
                    Direction::SouthEast,
                    Direction::SouthWest,
                    Direction::NorthWest,
                ];
                for (i, dir) in dirs.iter().enumerate() {
                    if let Some(target) = player_coord.step(*dir) {
                        if let Some(actor_id) = world.actor_at(target) {
                            if actor_id != world.player_id {
                                mask[i] = false; // Mask out bumping into monster
                            }
                        }
                    }
                }
            }
            if c.vegan || c.vegetarian {
                let only_meat = carried.iter().all(|&id| {
                    world
                        .arena
                        .items
                        .get(id)
                        .map(|i| {
                            i.corpse_race.is_some()
                                || i.name.contains("corpse")
                                || i.name.contains("meat")
                        })
                        .unwrap_or(true)
                });
                if only_meat {
                    mask[19] = false; // Eat
                }
            }
        }

        mask
    }

    fn build_observation<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let obs = self.session.get_observation();
        self.obs_to_dict(py, &obs)
    }

    fn obs_to_dict<'py>(
        &self,
        py: Python<'py>,
        obs: &GameObservation,
    ) -> PyResult<Bound<'py, PyDict>> {
        let obs_dict = PyDict::new(py);
        let world = &self.session.world;

        obs_dict.set_item("ascii_map", &obs.ascii_map)?;

        // Map glyphs as flat list of ASCII integer codes (80x21 = 1680 numbers)
        let glyphs_list = PyList::empty(py);
        for b in obs.ascii_map.bytes() {
            if b != b'\n' && b != b'\r' {
                glyphs_list.append(b as usize)?;
            }
        }
        obs_dict.set_item("map_glyphs", glyphs_list)?;

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
        obs_dict.set_item("max_pw", obs.player_max_pw)?;
        obs_dict.set_item("is_dead", obs.is_game_over)?;

        // Afflictions
        let aff_dict = PyDict::new(py);
        aff_dict.set_item(
            "petrification",
            world
                .hero
                .afflictions
                .petrification
                .as_ref()
                .map(|p| p.turns_remaining)
                .unwrap_or(0),
        )?;
        aff_dict.set_item(
            "sliming",
            world
                .hero
                .afflictions
                .sliming
                .as_ref()
                .map(|s| s.turns_remaining)
                .unwrap_or(0),
        )?;
        aff_dict.set_item("confused", world.hero.afflictions.transient.confused)?;
        aff_dict.set_item("stunned", world.hero.afflictions.transient.stunned)?;
        aff_dict.set_item(
            "hallucinating",
            world.hero.afflictions.transient.hallucinating,
        )?;
        aff_dict.set_item("polymorphed", world.hero.polymorph.is_some())?;
        aff_dict.set_item("mounted", world.hero.mount.is_some())?;
        obs_dict.set_item("afflictions", aff_dict)?;

        // Conducts
        let conducts_dict = PyDict::new(py);
        conducts_dict.set_item("pacifist", world.conducts.pacifist)?;
        conducts_dict.set_item("vegan", world.conducts.vegan)?;
        conducts_dict.set_item("vegetarian", world.conducts.vegetarian)?;
        conducts_dict.set_item("atheist", world.conducts.atheist)?;
        conducts_dict.set_item("illiterate", world.conducts.illiterate)?;
        conducts_dict.set_item("genocideless", world.conducts.genocideless)?;
        conducts_dict.set_item("polypileless", world.conducts.polypileless)?;
        conducts_dict.set_item("wishless", world.conducts.wishless)?;
        obs_dict.set_item("conducts", conducts_dict)?;

        // Action mask
        let mask = self.compute_action_mask();
        let mask_list = PyList::new(py, mask)?;
        obs_dict.set_item("action_mask", mask_list)?;

        // Visible actors
        let actors_list = PyList::empty(py);
        for actor in &obs.visible_actors {
            let act_dict = PyDict::new(py);
            act_dict.set_item("name", &actor.name)?;
            act_dict.set_item("x", actor.coord.x)?;
            act_dict.set_item("y", actor.coord.y)?;
            act_dict.set_item("hp", actor.hp)?;
            act_dict.set_item("max_hp", actor.max_hp)?;
            act_dict.set_item("is_player", actor.is_player)?;
            actors_list.append(act_dict)?;
        }
        obs_dict.set_item("visible_actors", actors_list)?;
        obs_dict.set_item("num_visible_actors", obs.visible_actors.len())?;

        // Carried items
        let carried = world.arena.items_carried_by(world.player_id);
        let items_list = PyList::empty(py);
        for id in &carried {
            if let Some(item) = world.arena.items.get(*id) {
                let it_dict = PyDict::new(py);
                it_dict.set_item("name", &item.name)?;
                it_dict.set_item("class", format!("{:?}", item.class))?;
                it_dict.set_item("buc", format!("{:?}", item.buc))?;
                it_dict.set_item("weight", item.weight)?;
                it_dict.set_item("enchantment", item.enchantment)?;
                items_list.append(it_dict)?;
            }
        }
        obs_dict.set_item("inventory", items_list)?;
        obs_dict.set_item("inventory_count", carried.len())?;

        Ok(obs_dict)
    }
}

#[pymodule]
fn netrust_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<NetRustEnv>()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrust_types::Coord;

    fn c(x: i32, y: i32) -> Coord {
        Coord::new_unchecked(x as _, y as _)
    }

    #[test]
    fn exploration_resets_per_level() {
        let mut t = ExplorationTracker::default();
        assert_eq!(t.observe(1, [c(1, 1), c(2, 2)]), 2);
        assert_eq!(t.observe(1, [c(1, 1), c(3, 3)]), 1);
        assert_eq!(t.len(), 3);
        // Same coordinates on a new level count as newly explored again.
        assert_eq!(t.observe(2, [c(1, 1), c(2, 2)]), 2);
        assert_eq!(t.len(), 2);
    }
}
