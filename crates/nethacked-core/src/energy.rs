//! Turn Scheduler & Energy System.
//!
//! Formalized and verified in `NetMechanics.Energy`.
//! Implements the speed tick model from NetHack's `allmain.c:moveloop_core`.

use serde::{Deserialize, Serialize};

pub const NORMAL_SPEED: u32 = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulerState {
    pub turn: u64,
    pub hero_energy: u32,
    pub hero_speed: u32,
    pub monster_energy: u32,
    pub monster_speed: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepAction {
    HeroStep,
    MonsterStep,
    TurnTick,
}

impl SchedulerState {
    pub fn new(hero_speed: u32, monster_speed: u32) -> Self {
        Self {
            turn: 1,
            hero_energy: hero_speed,
            hero_speed,
            monster_energy: monster_speed,
            monster_speed,
        }
    }

    pub fn hero_can_act(&self) -> bool {
        self.hero_energy >= NORMAL_SPEED
    }

    pub fn monster_can_act(&self) -> bool {
        self.monster_energy >= NORMAL_SPEED
    }

    pub fn hero_act(&mut self, cost: u32) {
        self.hero_energy = self.hero_energy.saturating_sub(cost);
    }

    pub fn monster_act(&mut self, cost: u32) {
        self.monster_energy = self.monster_energy.saturating_sub(cost);
    }

    pub fn allocate_new_turn(&mut self) {
        self.turn += 1;
        self.hero_energy += self.hero_speed;
        self.monster_energy += self.monster_speed;
    }

    /// Step the simulation scheduler.
    /// Invariant proved in Lean 4 (`scheduler_progress`):
    /// Every call advances actor energy or global turn count.
    pub fn step(&mut self) -> StepAction {
        if self.hero_can_act() {
            self.hero_act(NORMAL_SPEED);
            StepAction::HeroStep
        } else if self.monster_can_act() {
            self.monster_act(NORMAL_SPEED);
            StepAction::MonsterStep
        } else {
            self.allocate_new_turn();
            StepAction::TurnTick
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_turn_progression() {
        let mut sched = SchedulerState::new(12, 6);
        assert_eq!(sched.turn, 1);
        assert_eq!(sched.step(), StepAction::HeroStep);
        assert_eq!(sched.hero_energy, 0);

        // Neither hero (0) nor monster (6) can act with 12
        assert_eq!(sched.step(), StepAction::TurnTick);
        assert_eq!(sched.turn, 2);
        assert_eq!(sched.hero_energy, 12);
        assert_eq!(sched.monster_energy, 12);

        // Turn 2: hero acts first
        assert_eq!(sched.step(), StepAction::HeroStep);
        assert_eq!(sched.hero_energy, 0);

        // Then monster acts (it has 12 points)
        assert_eq!(sched.step(), StepAction::MonsterStep);
        assert_eq!(sched.monster_energy, 0);
    }
}
