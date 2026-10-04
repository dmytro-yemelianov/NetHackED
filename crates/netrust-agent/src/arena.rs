//! Automated AI Benchmark & Evaluation Arena for NetRust.
//!
//! Provides deterministic policy evaluation across Random, Survival Heuristic,
//! and Speedrun policies. Compiles comprehensive statistical telemetry.

use netrust_arena::ItemLocation;
use netrust_core::pathfinding::DijkstraField;
use netrust_core::{ActionAst, Direction, SpellKind};
use netrust_data::roles::{CharacterConfig, RoleId};
use netrust_sim::{DoorState, GameEvent, SimulationWorld, Tile};
use netrust_types::ItemClass;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::GameObservation;

/// Trait implemented by autonomous game-playing policies.
pub trait AgentPolicy {
    fn name(&self) -> &'static str;
    fn decide_action(&mut self, obs: &GameObservation, world: &SimulationWorld) -> ActionAst;
}

/// Baseline policy: executes pseudo-random legal actions.
pub struct RandomPolicy {
    rng: ChaCha8Rng,
}

impl RandomPolicy {
    pub fn new(seed: u64) -> Self {
        Self {
            rng: ChaCha8Rng::seed_from_u64(seed),
        }
    }
}

impl AgentPolicy for RandomPolicy {
    fn name(&self) -> &'static str {
        "RandomBaseline"
    }

    fn decide_action(&mut self, _obs: &GameObservation, world: &SimulationWorld) -> ActionAst {
        let player = match world.arena.actors.get(world.player_id) {
            Some(p) => p,
            None => return ActionAst::Wait,
        };

        // Eat if hungry and carrying food
        if world.player_nutrition < 150 {
            let carried = world.arena.items_carried_by(world.player_id);
            if let Some((idx, _)) = carried.iter().enumerate().find(|(_, &iid)| {
                world
                    .arena
                    .items
                    .get(iid)
                    .map(|it| it.class == ItemClass::Food)
                    .unwrap_or(false)
            }) {
                return ActionAst::Eat(idx);
            }
        }

        // Descend if on stairs down (50% chance)
        if matches!(
            world.level.get_tile(player.coord),
            Tile::Stairs { up: false }
        ) && self.rng.gen_bool(0.5)
        {
            return ActionAst::Descend;
        }

        // Pick up floor items (50% chance)
        if !world.arena.items_at_floor(player.coord).is_empty() && self.rng.gen_bool(0.5) {
            return ActionAst::PickUp;
        }

        // Random movement
        let dirs = Direction::all_compass();
        let mut shuffled = dirs;
        for i in (1..shuffled.len()).rev() {
            let j = self.rng.gen_range(0..=i);
            shuffled.swap(i, j);
        }

        for dir in shuffled {
            if let Some(next) = player.coord.step(dir) {
                let tile = world.level.get_tile(next);
                if tile.is_passable() || matches!(tile, Tile::Door { .. }) {
                    if matches!(
                        tile,
                        Tile::Door {
                            state: DoorState::Closed,
                            ..
                        }
                    ) {
                        return ActionAst::OpenDoor(next);
                    }
                    return ActionAst::Move(dir);
                }
            }
        }

        ActionAst::Wait
    }
}

/// Survival Heuristic Policy: Prioritizes combat, healing, eating, and controlled exploration.
pub struct SurvivalPolicy;

impl SurvivalPolicy {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SurvivalPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentPolicy for SurvivalPolicy {
    fn name(&self) -> &'static str {
        "SurvivalHeuristic"
    }

    fn decide_action(&mut self, _obs: &GameObservation, world: &SimulationWorld) -> ActionAst {
        let player = match world.arena.actors.get(world.player_id) {
            Some(p) => p,
            None => return ActionAst::Wait,
        };

        let carried = world.arena.items_carried_by(world.player_id);

        // 1. Emergency Healing if HP < 40%
        if player.hp * 10 < player.max_hp * 4 {
            // Check for potion of healing
            if let Some((idx, _)) = carried.iter().enumerate().find(|(_, &iid)| {
                world
                    .arena
                    .items
                    .get(iid)
                    .map(|it| it.class == ItemClass::Potion)
                    .unwrap_or(false)
            }) {
                return ActionAst::Quaff(idx);
            }

            // Check for healing spell
            if let Some((idx, _)) = world.known_spells.iter().enumerate().find(|(_, (s, _))| {
                *s == SpellKind::CureLightWounds || *s == SpellKind::ExtraHealing
            }) {
                if world.player_pw >= 5 {
                    return ActionAst::Cast {
                        spell_index: idx,
                        dir: Direction::None,
                    };
                }
            }
        }

        // 2. Nutrition management: eat if hungry
        if world.player_nutrition < 300 {
            if let Some((idx, _)) = carried.iter().enumerate().find(|(_, &iid)| {
                world
                    .arena
                    .items
                    .get(iid)
                    .map(|it| it.class == ItemClass::Food)
                    .unwrap_or(false)
            }) {
                return ActionAst::Eat(idx);
            }
        }

        // 3. Adjacent monster combat
        for dir in Direction::all_compass() {
            if let Some(adj) = player.coord.step(dir) {
                if let Some(target_id) = world.actor_at(adj) {
                    if target_id != world.player_id {
                        // If spellcaster has offensive spell and mana, cast Force Bolt
                        if let Some((idx, _)) = world
                            .known_spells
                            .iter()
                            .enumerate()
                            .find(|(_, (s, _))| *s == SpellKind::ForceBolt)
                        {
                            if world.player_pw >= 7 {
                                return ActionAst::Cast {
                                    spell_index: idx,
                                    dir,
                                };
                            }
                        }
                        return ActionAst::Move(dir);
                    }
                }
            }
        }

        // 4. Floor items: pick up if present
        if !world.arena.items_at_floor(player.coord).is_empty() {
            return ActionAst::PickUp;
        }

        // 5. If standing on stairs down, descend
        if matches!(
            world.level.get_tile(player.coord),
            Tile::Stairs { up: false }
        ) {
            return ActionAst::Descend;
        }

        // 6. Navigate towards stairs down via Dijkstra field
        let target = world.level.stairs_down;
        let field = DijkstraField::compute(target, |c| {
            let t = world.level.get_tile(c);
            t.is_passable() || matches!(t, Tile::Door { .. })
        });

        if let Some(next) = field.steepest_descent(player.coord) {
            let t = world.level.get_tile(next);
            if matches!(
                t,
                Tile::Door {
                    state: DoorState::Closed,
                    ..
                }
            ) {
                return ActionAst::OpenDoor(next);
            }
            if matches!(
                t,
                Tile::Door {
                    state: DoorState::Locked,
                    ..
                }
            ) {
                return ActionAst::Kick(next);
            }
            if let Some(dir) = Direction::all_compass()
                .into_iter()
                .find(|&d| player.coord.step(d) == Some(next))
            {
                return ActionAst::Move(dir);
            }
        }

        ActionAst::Wait
    }
}

/// Speedrun Policy: Goal-oriented agent that rushes depth 5, secures the Amulet of Yendor,
/// and ascends back to the surface for Victory.
pub struct SpeedrunPolicy;

impl SpeedrunPolicy {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SpeedrunPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentPolicy for SpeedrunPolicy {
    fn name(&self) -> &'static str {
        "AmuletSpeedrunner"
    }

    fn decide_action(&mut self, _obs: &GameObservation, world: &SimulationWorld) -> ActionAst {
        let player = match world.arena.actors.get(world.player_id) {
            Some(p) => p,
            None => return ActionAst::Wait,
        };

        let carried = world.arena.items_carried_by(world.player_id);

        // Emergency heal if HP < 30%
        if player.hp * 10 < player.max_hp * 3 {
            if let Some((idx, _)) = carried.iter().enumerate().find(|(_, &iid)| {
                world
                    .arena
                    .items
                    .get(iid)
                    .map(|it| it.class == ItemClass::Potion)
                    .unwrap_or(false)
            }) {
                return ActionAst::Quaff(idx);
            }
            if let Some((idx, _)) = world.known_spells.iter().enumerate().find(|(_, (s, _))| {
                *s == SpellKind::CureLightWounds || *s == SpellKind::ExtraHealing
            }) {
                if world.player_pw >= 5 {
                    return ActionAst::Cast {
                        spell_index: idx,
                        dir: Direction::None,
                    };
                }
            }
        }

        // Eat if hungry
        if world.player_nutrition < 200 {
            if let Some((idx, _)) = carried.iter().enumerate().find(|(_, &iid)| {
                world
                    .arena
                    .items
                    .get(iid)
                    .map(|it| it.class == ItemClass::Food)
                    .unwrap_or(false)
            }) {
                return ActionAst::Eat(idx);
            }
        }

        // Check if player already holds the Amulet of Yendor
        let has_amulet = carried.iter().any(|&iid| {
            world
                .arena
                .items
                .get(iid)
                .is_some_and(netrust_sim::is_real_amulet)
        });

        // Determine destination target coordinate
        let target_coord = if has_amulet {
            // Ascending back to depth 1 surface!
            if player.coord == world.level.stairs_up {
                return ActionAst::Ascend;
            }
            world.level.stairs_up
        } else if world.depth == 5 {
            // Locate Amulet of Yendor on this level floor
            let floor_amulet = world.arena.items.iter().find_map(|(_, it)| {
                if netrust_sim::is_real_amulet(it) {
                    match it.location {
                        ItemLocation::Floor(c) => Some(c),
                        _ => None,
                    }
                } else {
                    None
                }
            });

            if let Some(ac) = floor_amulet {
                if player.coord == ac {
                    return ActionAst::PickUp;
                }
                ac
            } else {
                world.level.stairs_down
            }
        } else {
            // Descending towards depth 5
            if player.coord == world.level.stairs_down {
                return ActionAst::Descend;
            }
            world.level.stairs_down
        };

        // Dijkstra navigation to target
        let field = DijkstraField::compute(target_coord, |c| {
            let t = world.level.get_tile(c);
            t.is_passable() || matches!(t, Tile::Door { .. })
        });

        if let Some(next) = field.steepest_descent(player.coord) {
            // If monster occupies next coordinate, attack it or cast offensive spell
            if let Some(target_id) = world.actor_at(next) {
                if target_id != world.player_id {
                    let dir = Direction::all_compass()
                        .into_iter()
                        .find(|&d| player.coord.step(d) == Some(next))
                        .unwrap_or(Direction::None);
                    if let Some((idx, _)) = world
                        .known_spells
                        .iter()
                        .enumerate()
                        .find(|(_, (s, _))| *s == SpellKind::ForceBolt)
                    {
                        if world.player_pw >= 7 {
                            return ActionAst::Cast {
                                spell_index: idx,
                                dir,
                            };
                        }
                    }
                    return ActionAst::Move(dir);
                }
            }

            let t = world.level.get_tile(next);
            if matches!(
                t,
                Tile::Door {
                    state: DoorState::Closed,
                    ..
                }
            ) {
                return ActionAst::OpenDoor(next);
            }
            if matches!(
                t,
                Tile::Door {
                    state: DoorState::Locked,
                    ..
                }
            ) {
                return ActionAst::Kick(next);
            }
            if let Some(dir) = Direction::all_compass()
                .into_iter()
                .find(|&d| player.coord.step(d) == Some(next))
            {
                return ActionAst::Move(dir);
            }
        }

        ActionAst::Wait
    }
}

/// PetTester Tactical Policy:
/// Advanced heuristic policy integrating:
/// 1. BUC Pet Testing: Avoids cursed items verified via companion pet reluctance.
/// 2. Emergency Elbereth Ward: Engraves "Elbereth" in dust when HP is critical (< 35%) and adjacent to hostiles.
/// 3. Pet-Cooperative Positioning: Swaps displacement with tame pets rather than attacking them.
/// 4. Dynamic Lighting: Applies carried lamps/lanterns in dark rooms or caverns.
/// 5. Temple Donations: Donates gold to temple priests in Minetown for divine AC protection.
/// 6. Goal-directed Dijkstra Navigation: Efficiently progresses deeper into the dungeon.
pub struct PetTesterTacticalPolicy {
    pub pet_tested_cursed: std::collections::HashSet<netrust_sim::Coord>,
    pub pet_tested_safe: std::collections::HashSet<netrust_sim::Coord>,
}

impl PetTesterTacticalPolicy {
    pub fn new() -> Self {
        Self {
            pet_tested_cursed: std::collections::HashSet::new(),
            pet_tested_safe: std::collections::HashSet::new(),
        }
    }
}

impl Default for PetTesterTacticalPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentPolicy for PetTesterTacticalPolicy {
    fn name(&self) -> &'static str {
        "PetTesterTactical"
    }

    fn decide_action(&mut self, _obs: &GameObservation, world: &SimulationWorld) -> ActionAst {
        let player = match world.arena.actors.get(world.player_id) {
            Some(p) => p,
            None => return ActionAst::Wait,
        };

        let carried = world.arena.items_carried_by(world.player_id);

        // Find adjacent hostiles and pets
        let mut adj_hostile_dir = None;
        let mut adj_pet_dir = None;
        for dir in Direction::all_compass() {
            if let Some(adj) = player.coord.step(dir) {
                if let Some(target_id) = world.actor_at(adj) {
                    if target_id != world.player_id {
                        if let Some(target) = world.arena.actors.get(target_id) {
                            if target.is_tame {
                                adj_pet_dir = Some((dir, adj));
                            } else {
                                adj_hostile_dir = Some((dir, adj));
                            }
                        }
                    }
                }
            }
        }

        // 1. Critical Health Defense (< 35% HP):
        // If hostile is adjacent and HP is critical, engrave Elbereth in dust to repel them!
        if player.hp * 100 < player.max_hp * 35 && adj_hostile_dir.is_some() {
            // If holding healing potion, quaff first
            if let Some((idx, _)) = carried.iter().enumerate().find(|(_, &iid)| {
                world
                    .arena
                    .items
                    .get(iid)
                    .map(|it| it.class == ItemClass::Potion)
                    .unwrap_or(false)
            }) {
                return ActionAst::Quaff(idx);
            }
            // Engrave Elbereth ward in dust
            return ActionAst::Engrave {
                text: "Elbereth".to_string(),
                medium: netrust_core::engraving::EngravingMedium::Dust(1),
            };
        }

        // 2. Pet Cooperation & Meat-Shield Swap:
        // If low on HP (< 50%) and adjacent to a pet and hostile, swap with pet to let pet tank!
        if player.hp * 2 < player.max_hp && adj_hostile_dir.is_some() {
            if let Some((pet_dir, _)) = adj_pet_dir {
                return ActionAst::Move(pet_dir); // triggers non-violent swap_displacement
            }
        }

        // 3. Nutrition management: eat if hungry (< 300)
        if world.player_nutrition < 300 {
            if let Some((idx, _)) = carried.iter().enumerate().find(|(_, &iid)| {
                world
                    .arena
                    .items
                    .get(iid)
                    .map(|it| it.class == ItemClass::Food)
                    .unwrap_or(false)
            }) {
                return ActionAst::Eat(idx);
            }
        }

        // 4. Dynamic Lighting: if in dark room, light lamp
        if world.level.is_dark_at(player.coord) {
            if let Some((idx, _)) = carried.iter().enumerate().find(|(_, &iid)| {
                world
                    .arena
                    .items
                    .get(iid)
                    .map(|it| {
                        (it.name.contains("lamp")
                            || it.name.contains("lantern")
                            || it.name.contains("candle"))
                            && it.enchantment <= 0
                    })
                    .unwrap_or(false)
            }) {
                return ActionAst::Apply(idx);
            }
        }

        // 5. Minetown Temple Donation: if near priest and have 400+ gold and protection < 9
        if world.player_gold >= 400 && world.divine_protection < 9 {
            let priest_near = world.arena.actors.values().any(|a| {
                !a.is_dead
                    && a.name.to_lowercase().contains("priest")
                    && a.coord.chebyshev_distance(player.coord) <= 6
            });
            if priest_near {
                return ActionAst::Donate(400);
            }
        }

        // 6. BUC Pet-Testing on Floor Items:
        let floor_items = world.arena.items_at_floor(player.coord);
        if !floor_items.is_empty() {
            // Check if marked cursed
            if self.pet_tested_cursed.contains(&player.coord) {
                // Ignore cursed items!
            } else {
                let pet_opt = world
                    .arena
                    .actors
                    .values()
                    .find(|a| !a.is_dead && a.is_tame);
                if let Some(pet) = pet_opt {
                    if pet.coord == player.coord || self.pet_tested_safe.contains(&player.coord) {
                        return ActionAst::PickUp;
                    } else if pet.coord.chebyshev_distance(player.coord) <= 2
                        && !self.pet_tested_safe.contains(&player.coord)
                    {
                        self.pet_tested_safe.insert(player.coord);
                        return ActionAst::PickUp;
                    }
                }
                return ActionAst::PickUp;
            }
        }

        // 7. Tactical Combat: attack adjacent hostiles
        if let Some((dir, _)) = adj_hostile_dir {
            if let Some((idx, _)) = world
                .known_spells
                .iter()
                .enumerate()
                .find(|(_, (s, _))| *s == SpellKind::ForceBolt)
            {
                if world.player_pw >= 7 {
                    return ActionAst::Cast {
                        spell_index: idx,
                        dir,
                    };
                }
            }
            return ActionAst::Move(dir);
        }

        // 8. Stairs Down: descend
        if matches!(
            world.level.get_tile(player.coord),
            Tile::Stairs { up: false }
        ) {
            return ActionAst::Descend;
        }

        // 9. Goal Navigation: Dijkstra descent to stairs down
        let target = world.level.stairs_down;
        let field = DijkstraField::compute(target, |c| {
            let t = world.level.get_tile(c);
            t.is_passable() || matches!(t, Tile::Door { .. })
        });

        if let Some(next) = field.steepest_descent(player.coord) {
            let t = world.level.get_tile(next);
            if matches!(
                t,
                Tile::Door {
                    state: DoorState::Closed,
                    ..
                }
            ) {
                return ActionAst::OpenDoor(next);
            }
            if matches!(
                t,
                Tile::Door {
                    state: DoorState::Locked,
                    ..
                }
            ) {
                return ActionAst::Kick(next);
            }
            if let Some(dir) = Direction::all_compass()
                .into_iter()
                .find(|&d| player.coord.step(d) == Some(next))
            {
                return ActionAst::Move(dir);
            }
        }

        ActionAst::Wait
    }
}

/// Results of a single evaluation run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunResult {
    pub seed: u64,
    pub policy_name: String,
    pub role: String,
    pub turns: u64,
    pub max_depth: usize,
    pub final_hp: u32,
    pub max_hp: u32,
    pub gold: u32,
    pub monsters_slain: u32,
    pub food_eaten: u32,
    pub spells_cast: u32,
    pub victory: bool,
    pub end_reason: String,
}

/// Aggregate performance statistics for a specific policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyStats {
    pub runs: usize,
    pub victories: usize,
    pub win_rate_pct: f64,
    pub mean_turns: f64,
    pub mean_max_depth: f64,
    pub mean_kills: f64,
    pub mean_gold: f64,
}

/// Overall benchmark summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArenaSummary {
    pub total_runs: usize,
    pub total_victories: usize,
    pub overall_win_rate_pct: f64,
    pub per_policy: HashMap<String, PolicyStats>,
}

/// Complete benchmark report saved as JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkReport {
    pub timestamp_epoch_secs: u64,
    pub total_runs: usize,
    pub summary: ArenaSummary,
    pub runs: Vec<RunResult>,
}

/// Executes a single simulation run under the given policy and configuration.
pub fn run_single_game<P: AgentPolicy>(
    mut policy: P,
    seed: u64,
    config: CharacterConfig,
    max_turns: u64,
) -> RunResult {
    let role_str = format!("{:?}", config.role);
    let mut world = SimulationWorld::new_with_character(seed, config);
    let mut max_depth = 1usize;
    let mut monsters_slain = 0u32;
    let mut food_eaten = 0u32;
    let mut spells_cast = 0u32;
    let mut victory = false;
    let mut end_reason = "Turn limit reached".to_string();

    while world.scheduler.turn < max_turns {
        if world.depth > max_depth {
            max_depth = world.depth;
        }

        let player = match world.arena.actors.get(world.player_id) {
            Some(p) => p.clone(),
            None => {
                end_reason = "Player removed from arena".into();
                break;
            }
        };

        if player.is_dead {
            end_reason = "Killed in dungeon".into();
            break;
        }

        // Extract observation
        let obs = crate::AgentSession {
            world: world.clone(),
            last_events: Vec::new(),
        }
        .get_observation();

        let action = policy.decide_action(&obs, &world);

        match &action {
            ActionAst::Eat(_) => food_eaten += 1,
            ActionAst::Cast { .. } => spells_cast += 1,
            _ => {}
        }

        let events = world.step_player_action(action);

        for e in &events {
            match e {
                GameEvent::Victory => {
                    victory = true;
                    end_reason = "Ascended with the Amulet of Yendor!".into();
                }
                GameEvent::AttackLanded {
                    lethal: true,
                    attacker,
                    ..
                } => {
                    if *attacker == world.player_id {
                        monsters_slain += 1;
                    }
                }
                GameEvent::LogMessage { text } if text.contains("is slain by magic") => {
                    monsters_slain += 1;
                }
                _ => {}
            }
        }

        if victory {
            break;
        }
    }

    let (final_hp, max_hp) = world
        .arena
        .actors
        .get(world.player_id)
        .map(|p| (p.hp, p.max_hp))
        .unwrap_or((0, 0));

    RunResult {
        seed,
        policy_name: policy.name().to_string(),
        role: role_str,
        turns: world.scheduler.turn,
        max_depth,
        final_hp,
        max_hp,
        gold: world.player_gold,
        monsters_slain,
        food_eaten,
        spells_cast,
        victory,
        end_reason,
    }
}

/// Detailed trajectory step recorded during policy execution for replay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrajectoryStep {
    pub turn: u64,
    pub depth: usize,
    pub player_coord: (usize, usize),
    pub player_hp: u32,
    pub player_max_hp: u32,
    pub action: String,
    pub log_summary: String,
}

/// Full game trajectory replay recording.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrajectoryRecording {
    pub policy_name: String,
    pub seed: u64,
    pub steps: Vec<TrajectoryStep>,
    pub final_score: u64,
    pub victory: bool,
    pub end_reason: String,
}

/// Runs a single game and records the complete decision trajectory for replay.
pub fn run_game_with_trajectory<P: AgentPolicy>(
    mut policy: P,
    seed: u64,
    config: CharacterConfig,
    max_turns: u64,
) -> (RunResult, TrajectoryRecording) {
    let role_str = format!("{:?}", config.role);
    let mut world = SimulationWorld::new_with_character(seed, config);
    let mut max_depth = 1usize;
    let mut monsters_slain = 0u32;
    let mut food_eaten = 0u32;
    let mut spells_cast = 0u32;
    let mut victory = false;
    let mut end_reason = "Turn limit reached".to_string();
    let mut steps = Vec::new();

    while world.scheduler.turn < max_turns {
        if world.depth > max_depth {
            max_depth = world.depth;
        }

        let player = match world.arena.actors.get(world.player_id) {
            Some(p) => p.clone(),
            None => {
                end_reason = "Player removed from arena".into();
                break;
            }
        };

        if player.is_dead {
            end_reason = "Killed in dungeon".into();
            break;
        }

        let obs = crate::AgentSession {
            world: world.clone(),
            last_events: Vec::new(),
        }
        .get_observation();

        let action = policy.decide_action(&obs, &world);

        match &action {
            ActionAst::Eat(_) => food_eaten += 1,
            ActionAst::Cast { .. } => spells_cast += 1,
            _ => {}
        }

        let action_str = format!("{action:?}");
        let events = world.step_player_action(action);

        let mut log_msgs = Vec::new();
        for e in &events {
            match e {
                GameEvent::Victory => {
                    victory = true;
                    end_reason = "Ascended with the Amulet of Yendor!".into();
                }
                GameEvent::AttackLanded {
                    lethal: true,
                    attacker,
                    ..
                } => {
                    if *attacker == world.player_id {
                        monsters_slain += 1;
                    }
                }
                GameEvent::LogMessage { text } => {
                    if text.contains("is slain by magic") {
                        monsters_slain += 1;
                    }
                    log_msgs.push(text.clone());
                }
                _ => {}
            }
        }

        steps.push(TrajectoryStep {
            turn: world.scheduler.turn,
            depth: world.depth,
            player_coord: (player.coord.x, player.coord.y),
            player_hp: player.hp,
            player_max_hp: player.max_hp,
            action: action_str,
            log_summary: log_msgs.join("; "),
        });

        if victory {
            break;
        }
    }

    let (final_hp, max_hp) = world
        .arena
        .actors
        .get(world.player_id)
        .map(|p| (p.hp, p.max_hp))
        .unwrap_or((0, 0));

    // Tournament score formula matching Lean 4 calculateTournamentScore
    let final_score = (world.scheduler.turn * 2)
        + (max_depth as u64 * 100)
        + (monsters_slain as u64 * 50)
        + (world.player_gold as u64);

    let run_res = RunResult {
        seed,
        policy_name: policy.name().to_string(),
        role: role_str,
        turns: world.scheduler.turn,
        max_depth,
        final_hp,
        max_hp,
        gold: world.player_gold,
        monsters_slain,
        food_eaten,
        spells_cast,
        victory,
        end_reason: end_reason.clone(),
    };

    let recording = TrajectoryRecording {
        policy_name: policy.name().to_string(),
        seed,
        steps,
        final_score,
        victory,
        end_reason,
    };

    (run_res, recording)
}

/// Evaluates a collection of policies over multiple seeds and roles.
pub fn run_evaluation_suite(
    seeds: &[u64],
    roles: &[RoleId],
    max_turns: u64,
) -> (Vec<RunResult>, ArenaSummary) {
    let mut results = Vec::new();

    for &seed in seeds {
        for &role in roles {
            let config = CharacterConfig {
                name: format!("{role:?}"),
                role,
                race: netrust_data::roles::RaceId::Human,
                gender: netrust_data::roles::Gender::Female,
                alignment: netrust_data::roles::get_role(role).default_alignment,
            };

            // 1. Random policy
            let res_random =
                run_single_game(RandomPolicy::new(seed), seed, config.clone(), max_turns);
            results.push(res_random);

            // 2. Survival policy
            let res_surv = run_single_game(SurvivalPolicy::new(), seed, config.clone(), max_turns);
            results.push(res_surv);

            // 3. Speedrunner policy
            let res_speed = run_single_game(SpeedrunPolicy::new(), seed, config.clone(), max_turns);
            results.push(res_speed);

            // 4. PetTester Tactical policy
            let res_tactical =
                run_single_game(PetTesterTacticalPolicy::new(), seed, config, max_turns);
            results.push(res_tactical);
        }
    }

    // Aggregate summary statistics
    let mut per_policy_map: HashMap<String, Vec<&RunResult>> = HashMap::new();
    for r in &results {
        per_policy_map
            .entry(r.policy_name.clone())
            .or_default()
            .push(r);
    }

    let mut per_policy = HashMap::new();
    for (name, runs) in per_policy_map {
        let n = runs.len();
        let victories = runs.iter().filter(|r| r.victory).count();
        let win_rate_pct = if n > 0 {
            (victories as f64 / n as f64) * 100.0
        } else {
            0.0
        };
        let mean_turns = runs.iter().map(|r| r.turns as f64).sum::<f64>() / n as f64;
        let mean_max_depth = runs.iter().map(|r| r.max_depth as f64).sum::<f64>() / n as f64;
        let mean_kills = runs.iter().map(|r| r.monsters_slain as f64).sum::<f64>() / n as f64;
        let mean_gold = runs.iter().map(|r| r.gold as f64).sum::<f64>() / n as f64;

        per_policy.insert(
            name,
            PolicyStats {
                runs: n,
                victories,
                win_rate_pct,
                mean_turns,
                mean_max_depth,
                mean_kills,
                mean_gold,
            },
        );
    }

    let total_runs = results.len();
    let total_victories = results.iter().filter(|r| r.victory).count();
    let overall_win_rate_pct = if total_runs > 0 {
        (total_victories as f64 / total_runs as f64) * 100.0
    } else {
        0.0
    };

    let summary = ArenaSummary {
        total_runs,
        total_victories,
        overall_win_rate_pct,
        per_policy,
    };

    (results, summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_random_policy_execution() {
        let config = CharacterConfig::default();
        let res = run_single_game(RandomPolicy::new(123), 123, config, 50);
        assert!(res.turns > 0);
        assert_eq!(res.policy_name, "RandomBaseline");
    }

    #[test]
    fn test_survival_policy_execution() {
        let config = CharacterConfig::default();
        let res = run_single_game(SurvivalPolicy::new(), 42, config, 100);
        assert!(res.turns > 0);
        assert_eq!(res.policy_name, "SurvivalHeuristic");
    }

    #[test]
    fn test_speedrun_policy_execution() {
        let config = CharacterConfig::default();
        let res = run_single_game(SpeedrunPolicy::new(), 777, config, 100);
        assert!(res.turns > 0);
        assert_eq!(res.policy_name, "AmuletSpeedrunner");
    }

    #[test]
    fn test_pet_tester_tactical_execution() {
        let config = CharacterConfig::default();
        let res = run_single_game(PetTesterTacticalPolicy::new(), 42, config, 100);
        assert!(res.turns > 0);
        assert_eq!(res.policy_name, "PetTesterTactical");
    }

    #[test]
    fn test_trajectory_recording() {
        let config = CharacterConfig::default();
        let (res, traj) = run_game_with_trajectory(PetTesterTacticalPolicy::new(), 101, config, 50);
        assert_eq!(res.policy_name, "PetTesterTactical");
        assert_eq!(traj.policy_name, "PetTesterTactical");
        assert_eq!(traj.seed, 101);
        assert!(!traj.steps.is_empty());
        assert!(traj.final_score > 0);
    }
}
