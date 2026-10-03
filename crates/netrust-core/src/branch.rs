//! Dungeon branch topology and entrance mechanics.
//!
//! Formally verified in Lean 4 (`NetMechanics.Branch`).

use netrust_types::{BranchCoord, BranchId};

/// Main dungeon entrance depth for a given branch.
pub const fn branch_entrance_depth(b: BranchId) -> usize {
    match b {
        BranchId::DungeonsOfDoom => 1,
        BranchId::GnomishMines => 3,
        BranchId::Sokoban => 4,
        BranchId::Gehennom => 5,
        BranchId::Quest => 4,
        BranchId::AstralPlane => 1,
        BranchId::WizardsTower => 3,
        BranchId::VladsTower => 3,
        BranchId::FortLudios => 1,
        BranchId::RogueLevel => 1,
    }
}

/// Maximum depth of a given branch.
pub const fn branch_max_depth(b: BranchId) -> usize {
    match b {
        BranchId::DungeonsOfDoom => 5,
        BranchId::GnomishMines => 5,
        BranchId::Sokoban => 3,
        BranchId::Gehennom => 20,
        BranchId::AstralPlane => 5,
        BranchId::Quest => 3,
        BranchId::WizardsTower => 3,
        BranchId::VladsTower => 3,
        BranchId::FortLudios => 1,
        BranchId::RogueLevel => 1,
    }
}

/// Enter a branch from the main dungeon stack at `main_depth`.
pub fn enter_branch(b: BranchId, main_depth: usize) -> Option<BranchCoord> {
    if main_depth == branch_entrance_depth(b) {
        Some(BranchCoord {
            branch: b,
            depth: 1,
        })
    } else {
        None
    }
}

/// Return to the parent branch (Dungeons of Doom) from a branch's top level.
pub fn exit_branch(bc: BranchCoord) -> Option<BranchCoord> {
    if bc.depth == 1 {
        Some(BranchCoord {
            branch: BranchId::DungeonsOfDoom,
            depth: branch_entrance_depth(bc.branch),
        })
    } else {
        None
    }
}
