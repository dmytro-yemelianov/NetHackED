//! Autonomous agent tournament scoring and tactical decision invariants.
//!
//! Formally verified in `NetMechanics.Tournament`.

use serde::{Deserialize, Serialize};

/// Check if health is critical (< 35% max HP).
pub fn is_hp_critical(hp: u32, max_hp: u32) -> bool {
    (hp as u64) * 100 < (max_hp as u64) * 35
}

/// Tournament scoring formula matching Lean 4 `calculateTournamentScore`.
pub fn calculate_tournament_score(turns: u64, depth: u64, kills: u64, gold: u64) -> u64 {
    (turns * 2) + (depth * 100) + (kills * 50) + gold
}

/// Tactical action priorities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TacticalAction {
    EngraveElbereth,
    SwapWithPet,
    WaitPetTest,
    AttackHostile,
    Descend,
    Explore,
}

/// Tactical decision context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TacticalContext {
    pub hp: u32,
    pub max_hp: u32,
    pub has_hostile_adj: bool,
    pub has_pet_adj: bool,
    pub has_unchecked_floor_item: bool,
    pub on_stairs_down: bool,
}

/// Tactical decision function matching Lean 4 `decideTacticalAction`.
pub fn decide_tactical_action(ctx: &TacticalContext) -> TacticalAction {
    if is_hp_critical(ctx.hp, ctx.max_hp) && ctx.has_hostile_adj {
        TacticalAction::EngraveElbereth
    } else if ctx.has_pet_adj && ctx.has_hostile_adj {
        TacticalAction::SwapWithPet
    } else if ctx.has_unchecked_floor_item && ctx.has_pet_adj {
        TacticalAction::WaitPetTest
    } else if ctx.has_hostile_adj {
        TacticalAction::AttackHostile
    } else if ctx.on_stairs_down {
        TacticalAction::Descend
    } else {
        TacticalAction::Explore
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tactical_priority_elbereth() {
        let ctx = TacticalContext {
            hp: 3,
            max_hp: 10,
            has_hostile_adj: true,
            has_pet_adj: false,
            has_unchecked_floor_item: false,
            on_stairs_down: false,
        };
        assert_eq!(decide_tactical_action(&ctx), TacticalAction::EngraveElbereth);
    }

    #[test]
    fn test_pet_test_priority() {
        let ctx = TacticalContext {
            hp: 10,
            max_hp: 10,
            has_hostile_adj: false,
            has_pet_adj: true,
            has_unchecked_floor_item: true,
            on_stairs_down: false,
        };
        assert_eq!(decide_tactical_action(&ctx), TacticalAction::WaitPetTest);
    }
}
