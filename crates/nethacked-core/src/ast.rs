//! Action and Effect AST in Safe Rust.
//!
//! Formalized and verified in `NetMechanics.AST`.
//! Implements deep embedding of actions, atomic state effects, and operational semantics.

use crate::buc::{Buc, WaterType};
use crate::combat::Combatant;
use crate::energy::NORMAL_SPEED;
use crate::engraving::EngravingMedium;
use crate::grid::{Coord, Tile};
use serde::{Deserialize, Serialize};

pub use nethacked_types::{ActorId, Direction, SlotId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionAst {
    Move(Direction),
    MeleeAttack(Coord),
    OpenDoor(Coord),
    CloseDoor(Coord),
    Kick(Coord),
    PickUp,
    Drop(usize),
    Wield(usize),
    Dip {
        item_index: usize,
        into_water: WaterType,
    },
    PutInContainer {
        item_index: usize,
        container_index: usize,
    },
    TakeFromContainer {
        container_index: usize,
        item_index: usize,
    },
    Quaff(usize),
    Read(usize),
    Engrave {
        text: String,
        medium: EngravingMedium,
    },
    ZapWand {
        dir: Direction,
        energy: u32,
    },
    Wait,
    Pray,
    Pay,
    Sacrifice(usize),
    Ascend,
    Descend,
    Eat(usize),
    Cast {
        spell_index: usize,
        dir: Direction,
    },
    Wish(String),
    Rub(usize),
    PriceCheck(usize),
    /// Offer gold to a nearby temple priest (C `priest.c:629-723`). `Donate(0)` offers the
    /// priest's suggested protection amount (`2 * suggested * quan`, `priest.c:645`); any
    /// offer is capped at the hero's gold (C `bribe`, `minion.c:379-382`).
    Donate(u32),
    Apply(usize),
    Quiver(SlotId),
    Fire(Direction),
    Mount(ActorId),
    Dismount,
    Search,
    Untrap(Coord),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectAst {
    SpendEnergy(u32),
    RestoreEnergy(u32),
    InflictDamage(u32),
    HealDamage(u32),
    MovePlayer(Coord),
    SetTile { coord: Coord, new_tile: Tile },
    TransformBuc { item_index: usize, new_buc: Buc },
    LogMessage(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldState {
    pub player_coord: Coord,
    pub player_energy: u32,
    pub player_combat: Combatant,
    pub turn: u64,
}

impl WorldState {
    pub fn apply_effect(&mut self, effect: &EffectAst) {
        match effect {
            EffectAst::SpendEnergy(amt) => {
                self.player_energy = self.player_energy.saturating_sub(*amt);
            }
            EffectAst::RestoreEnergy(amt) => {
                self.player_energy += *amt;
            }
            EffectAst::InflictDamage(amt) => {
                self.player_combat.apply_damage(*amt);
            }
            EffectAst::HealDamage(amt) => {
                let max_hp = self.player_combat.max_hp;
                self.player_combat.hp = (self.player_combat.hp + *amt).min(max_hp);
            }
            EffectAst::MovePlayer(coord) => {
                self.player_coord = *coord;
            }
            EffectAst::SetTile { .. } => {}
            EffectAst::TransformBuc { .. } => {}
            EffectAst::LogMessage(_) => {}
        }
    }

    pub fn apply_effects(&mut self, effects: &[EffectAst]) {
        for eff in effects {
            self.apply_effect(eff);
        }
    }

    pub fn eval_action(&mut self, action: &ActionAst) -> Vec<EffectAst> {
        let effects = match action {
            ActionAst::Wait => vec![EffectAst::SpendEnergy(NORMAL_SPEED)],
            ActionAst::Move(_) => vec![EffectAst::SpendEnergy(NORMAL_SPEED)],
            ActionAst::MeleeAttack(_) => vec![EffectAst::SpendEnergy(NORMAL_SPEED)],
            _ => vec![EffectAst::SpendEnergy(NORMAL_SPEED)],
        };
        self.apply_effects(&effects);
        effects
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ast_wait_eval() {
        let mut world = WorldState {
            player_coord: Coord::new(10, 10).unwrap(),
            player_energy: 12,
            player_combat: Combatant {
                hp: 12,
                max_hp: 12,
                ac: 10,
                level: 1,
                to_hit_bonus: 0,
                damage_bonus: 0,
                is_dead: false,
            },
            turn: 1,
        };

        let effects = world.eval_action(&ActionAst::Wait);
        assert_eq!(effects, vec![EffectAst::SpendEnergy(NORMAL_SPEED)]);
        assert_eq!(world.player_energy, 0);
    }
}
