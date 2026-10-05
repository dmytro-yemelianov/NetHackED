//! Simulation benchmark comparing rule packs against vanilla NetHack gameplay.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use nethacked_agent::run_seed_games_with_ruleset;
use nethacked_data::roles::RoleId;
use nethacked_data::ruleset::{Ruleset, RulesetRef};
use serde::{Deserialize, Serialize};

/// Performance and survival telemetry for a character role.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RoleStats {
    pub role: String,
    pub games: usize,
    pub deaths: usize,
    pub total_depth: usize,
    pub total_turns: u64,
    pub panics: usize,
}

impl RoleStats {
    pub fn mean_depth(&self) -> f64 {
        if self.games == 0 {
            0.0
        } else {
            self.total_depth as f64 / self.games as f64
        }
    }

    pub fn mean_turns(&self) -> f64 {
        if self.games == 0 {
            0.0
        } else {
            self.total_turns as f64 / self.games as f64
        }
    }
}

/// Comprehensive comparative report across seeds and roles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationReport {
    pub seeds: u64,
    pub turns_per_game: u64,
    pub pack_stats: Vec<RoleStats>,
    pub vanilla_stats: Vec<RoleStats>,
    pub total_panics: usize,
}

/// Executes parallel simulation sweeps across seeds and roles for both pack and vanilla.
pub fn run_simulation(
    pack_rs: Arc<Ruleset>,
    pack_ref: RulesetRef,
    seeds: u64,
    turns: u64,
) -> SimulationReport {
    let vanilla_rs = Ruleset::vanilla();
    let vanilla_ref = RulesetRef::vanilla();

    let roles: Vec<RoleId> = pack_rs.roles.iter().map(|r| r.id).collect();

    let mut pack_stats_map: Vec<RoleStats> = roles
        .iter()
        .map(|r| RoleStats {
            role: format!("{r:?}"),
            ..Default::default()
        })
        .collect();

    let mut vanilla_stats_map: Vec<RoleStats> = roles
        .iter()
        .map(|r| RoleStats {
            role: format!("{r:?}"),
            ..Default::default()
        })
        .collect();

    let mut total_panics = 0;

    for seed in 1..=seeds {
        for (i, &role) in roles.iter().enumerate() {
            // Run pack
            let pack_res = catch_unwind(AssertUnwindSafe(|| {
                run_seed_games_with_ruleset(
                    seed,
                    &[role],
                    turns,
                    Arc::clone(&pack_rs),
                    pack_ref.clone(),
                )
            }));

            match pack_res {
                Ok(runs) => {
                    for run in runs {
                        pack_stats_map[i].games += 1;
                        if run.end_reason == "Killed in dungeon" {
                            pack_stats_map[i].deaths += 1;
                        }
                        pack_stats_map[i].total_depth += run.max_depth;
                        pack_stats_map[i].total_turns += run.turns;
                    }
                }
                Err(_) => {
                    pack_stats_map[i].panics += 1;
                    total_panics += 1;
                }
            }

            // Run vanilla
            let van_res = catch_unwind(AssertUnwindSafe(|| {
                run_seed_games_with_ruleset(
                    seed,
                    &[role],
                    turns,
                    Arc::clone(&vanilla_rs),
                    vanilla_ref.clone(),
                )
            }));

            match van_res {
                Ok(runs) => {
                    for run in runs {
                        vanilla_stats_map[i].games += 1;
                        if run.end_reason == "Killed in dungeon" {
                            vanilla_stats_map[i].deaths += 1;
                        }
                        vanilla_stats_map[i].total_depth += run.max_depth;
                        vanilla_stats_map[i].total_turns += run.turns;
                    }
                }
                Err(_) => {
                    vanilla_stats_map[i].panics += 1;
                    total_panics += 1;
                }
            }
        }
    }

    SimulationReport {
        seeds,
        turns_per_game: turns,
        pack_stats: pack_stats_map,
        vanilla_stats: vanilla_stats_map,
        total_panics,
    }
}
