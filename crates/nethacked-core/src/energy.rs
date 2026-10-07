//! Turn Scheduler & Energy System.
//!
//! Modelled in Lean 4 in `NetMechanics.Energy` (machine-checked model; not formally linked to this code).
//! Movement gains implement selected released NetHack `allmain.c`/`mon.c` branches.
//! The uniform scheduler helper remains a legacy abstract model; production
//! simulation schedules monsters with their persisted per-actor movement credit.

use serde::{Deserialize, Serialize};

pub const NORMAL_SPEED: u32 = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulerState {
    pub turn: u64,
    pub hero_energy: u32,
    pub hero_speed: u32,
    /// Serialized legacy uniform-model budget; unused by production simulation.
    pub monster_energy: u32,
    /// Serialized legacy uniform-model speed; never copied into live actors.
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

/// Released C mon.c:1126–1167, ordinary stored speed branch. The native
/// bound-twelve draw is unconditional, including speed zero and exact multiples.
pub fn monster_movement_gain(speed: u32, mut rn2: impl FnMut(u32) -> u32) -> u32 {
    let remainder = speed % NORMAL_SPEED;
    let draw = rn2(NORMAL_SPEED);
    (speed - remainder).saturating_add(if draw < remainder { NORMAL_SPEED } else { 0 })
}

/// Released C allmain.c:123–155: speed properties precede live burden.
pub fn hero_movement_gain(
    base: u32,
    fast: bool,
    very_fast: bool,
    burden: crate::inventory::EncumbranceTier,
    mut rn2: impl FnMut(u32) -> u32,
) -> u32 {
    use crate::inventory::EncumbranceTier::*;
    let bonus = if very_fast {
        rn2(3) != 0
    } else {
        fast && rn2(3) == 0
    };
    let gain = base.saturating_add(if bonus { NORMAL_SPEED } else { 0 });
    let reduction = match burden {
        Burdened => gain / 4,
        Stressed => gain / 2,
        Strained => ((u64::from(gain) * 3) / 4) as u32,
        Overtaxed => ((u64::from(gain) * 7) / 8) as u32,
        Unencumbered | Overloaded => 0,
    };
    gain - reduction
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
