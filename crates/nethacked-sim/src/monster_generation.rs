//! Ordinary fixed-coordinate random generation delegates to the shared wand slice.
//! Room callers suppress groups; explicit-species APIs remain separate.
use crate::world::SimulationWorld;
use nethacked_arena::ActorId;
use nethacked_types::{BranchId, Coord};

impl SimulationWorld {
    /// Generate one owned ordinary random monster at this exact nonhero location.
    pub fn spawn_random_monster_at(&mut self, coord: Coord) -> Option<ActorId> {
        self.spawn_random_monster_at_on_level(coord, self.current_branch, self.depth)
    }

    /// Level construction supplies its explicit destination geography.
    pub(crate) fn spawn_random_monster_at_on_level(
        &mut self,
        coord: Coord,
        branch: BranchId,
        depth: usize,
    ) -> Option<ActorId> {
        crate::actions::wand_legacy::spawn_random_at(self, coord, branch, depth)
    }
}
