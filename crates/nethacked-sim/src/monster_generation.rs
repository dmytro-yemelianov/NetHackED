//! Ordinary fixed-coordinate random generation, released C 16ff5911.
//! Room callers suppress groups (mklev.c:975); explicit-species APIs remain separate.
//! Random-location/byhero/Sokoban and special initialization are separate contexts.

use nethacked_arena::ActorId;
use nethacked_data::generation::rndmonst;
use nethacked_data::ruleset::MonsterDef;
use nethacked_types::{BranchId, Coord, COLNO, ROWNO};
use rand::Rng;

use crate::peace::{peace_input, roll_peace_minded};
use crate::world::SimulationWorld;

impl SimulationWorld {
    /// Ordinary create_critters(NULL): original role placement before selection.
    /// Only the owned small-group flag is expanded at this random gameplay call.
    pub(crate) fn create_random_monsters_by_hero(&mut self) -> Result<(), &'static str> {
        let role = self
            .role_id
            .or_else(|| {
                serde_json::from_value(serde_json::Value::String(self.role_name.clone())).ok()
            })
            .ok_or("Your original form is unknown.")?;
        let rs = std::sync::Arc::clone(&self.ruleset);
        let original = rs
            .original_monster_for_role(role)
            .ok_or("Your original form is unknown.")?;
        let count = if self.rng.random_range(0..23u32) != 0 {
            1
        } else {
            self.rng.random_range(0..7u32) + 2
        };
        for _ in 0..count {
            // Reset origin for each attempt, including after failed creation.
            let origin = self.arena.actors[self.player_id].coord;
            if let Some(coord) = self.monster_adjacent(origin, original) {
                if let Some(leader) = self.spawn_random_monster_at(coord) {
                    self.expand_owned_small_group(leader);
                }
            }
        }
        Ok(())
    }

    /// Generate one owned ordinary random monster at this exact nonhero location.
    /// Like C makemon(NULL,x,y,MM_NOGRP), occupancy rejects before selection;
    /// no nearby fallback or group expansion occurs. The 51st species choice
    /// is accepted without another compatibility check, as in C :1226.
    pub fn spawn_random_monster_at(&mut self, coord: Coord) -> Option<ActorId> {
        self.spawn_random_monster_at_on_level(coord, self.current_branch, self.depth)
    }

    /// Level construction supplies its explicit destination geography; callers
    /// may generate a floor before updating the world's current depth/branch.
    pub(crate) fn spawn_random_monster_at_on_level(
        &mut self,
        coord: Coord,
        branch: BranchId,
        depth: usize,
    ) -> Option<ActorId> {
        // C isok excludes column zero; unsupported (0,0)/byhero contexts
        // must not silently become fixed-coordinate generation.
        if coord.x == 0 || coord.x >= COLNO || coord.y >= ROWNO || self.actor_at(coord).is_some() {
            return None;
        }
        let rs = std::sync::Arc::clone(&self.ruleset);
        let (birth, context) = self.item_birth_context(branch, depth, true);
        let mut selected = None;
        for attempt in 0..51 {
            let registry = &self.genocide_registry;
            let def = rndmonst(
                &rs,
                &context,
                |d| {
                    nethacked_core::genocide::is_genocided(
                        registry,
                        &d.name.to_lowercase(),
                        d.glyph,
                    )
                },
                |n| self.rng.random_range(0..n),
            )?;
            if attempt == 50 || self.monster_goodpos_in_branch(coord, def, true, branch) {
                selected = Some(def);
                break;
            }
        }
        let def = selected?;
        Some(self.insert_initialized_random_monster(
            def,
            coord,
            branch,
            (birth.level_difficulty, birth.hero_level),
        ))
    }

    /// Shared ordinary initialization for selected leaders and explicit followers.
    /// Placement/selection/genocide decisions belong to the caller.
    fn insert_initialized_random_monster(
        &mut self,
        def: &MonsterDef,
        coord: Coord,
        branch: BranchId,
        level_context: (u32, u32),
    ) -> ActorId {
        let rs = std::sync::Arc::clone(&self.ruleset);
        let mut actor = rs.create_random_monster_record_from_def(
            def,
            coord,
            nethacked_data::monsters::MonsterSpawnContext {
                endgame_plane: (branch == BranchId::AstralPlane)
                    .then_some(nethacked_types::EndgamePlane::Astral),
            },
            level_context,
            &mut self.rng,
        );
        let input = peace_input(
            def,
            self.hero_alignment(),
            self.hero_race,
            self.alignment_record,
            self.hero_has_amulet(),
        );
        actor.is_peaceful = roll_peace_minded(&input, &mut self.rng);
        self.set_monster_malign(&mut actor, def);
        self.normalize_revival_actor_allocator();
        self.arena.spawn_actor(actor)
    }

    /// Released makemon.c:85–145,1430–1432; no recursive group expansion.
    fn expand_owned_small_group(&mut self, leader_id: ActorId) {
        let rs = std::sync::Arc::clone(&self.ruleset);
        let Some(leader) = self.arena.actors.get(leader_id) else {
            return;
        };
        let Some(def) = rs.monster_for_actor(leader) else {
            return;
        };
        if !def.gen_flags.iter().any(|flag| flag == "sgroup") || self.rng.random_range(0..2u32) == 0
        {
            return;
        }
        let mut origin = leader.coord;
        let raw = self.rng.random_range(0..3u32) + 1;
        let divisor = match self.arena.actors[self.player_id].level {
            0..=2 => 4,
            3..=4 => 2,
            _ => 1,
        };
        let count = (raw / divisor).max(1);
        let (birth, _) = self.item_birth_context(self.current_branch, self.depth, true);
        for _ in 0..count {
            let input = peace_input(
                def,
                self.hero_alignment(),
                self.hero_race,
                self.alignment_record,
                self.hero_has_amulet(),
            );
            if roll_peace_minded(&input, &mut self.rng) {
                continue;
            }
            let Some(coord) = self.monster_adjacent(origin, def) else {
                continue;
            };
            // C enexto output remains the next origin even if makemon rejects.
            origin = coord;
            if nethacked_core::genocide::is_genocided(
                &self.genocide_registry,
                &def.name.to_lowercase(),
                def.glyph,
            ) {
                continue;
            }
            let id = self.insert_initialized_random_monster(
                def,
                coord,
                self.current_branch,
                (birth.level_difficulty, birth.hero_level),
            );
            let mut follower = self.arena.actors[id].clone();
            follower.is_peaceful = false;
            // C also clears mavenge; revenge state is unrepresented in this model.
            self.set_monster_malign(&mut follower, def);
            self.arena.actors[id] = follower;
        }
    }
}
