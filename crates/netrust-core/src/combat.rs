//! Combat Resolution Mechanics.
//!
//! Formalized and verified in `NetMechanics.Combat`.
//! Replaces NetHack's `uhitm.c` / `mhitm.c` to-hit and AC calculation with a pure transition.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Combatant {
    pub hp: u32,
    pub max_hp: u32,
    pub ac: i32,
    pub level: u32,
    pub to_hit_bonus: i32,
    pub damage_bonus: i32,
    pub is_dead: bool,
}

impl Combatant {
    pub fn apply_damage(&mut self, dmg: u32) {
        if dmg >= self.hp {
            self.hp = 0;
            self.is_dead = true;
        } else {
            self.hp -= dmg;
        }
    }
}

pub fn to_hit_threshold(attacker_bonus: i32, target_ac: i32) -> i32 {
    10 + target_ac + attacker_bonus
}

pub fn attack_lands(d20_roll: u32, threshold: i32) -> bool {
    (d20_roll as i32) <= threshold
}

pub fn calculate_damage(roll: u32, enchant: i32, bonus: i32, defender_ac: i32) -> u32 {
    let total = (roll as i32) + enchant + bonus;
    if total > 0 {
        let raw = total as u32;
        if defender_ac < 0 {
            let absorb = (-defender_ac) as u32;
            raw.saturating_sub(absorb).max(1)
        } else {
            raw
        }
    } else {
        0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttackResult {
    pub hit: bool,
    pub damage_dealt: u32,
    pub defender_after: Combatant,
}

pub fn resolve_melee_attack(
    attacker_bonus: i32,
    attacker_dmg_bonus: i32,
    mut defender: Combatant,
    d20_roll: u32,
    dmg_roll: u32,
    weapon_enchant: i32,
) -> AttackResult {
    let thresh = to_hit_threshold(attacker_bonus, defender.ac);
    if attack_lands(d20_roll, thresh) {
        let dmg = calculate_damage(dmg_roll, weapon_enchant, attacker_dmg_bonus, defender.ac);
        defender.apply_damage(dmg);
        AttackResult {
            hit: true,
            damage_dealt: dmg,
            defender_after: defender,
        }
    } else {
        AttackResult {
            hit: false,
            damage_dealt: 0,
            defender_after: defender,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_combat_hit_and_damage() {
        let goblin = Combatant {
            hp: 12,
            max_hp: 12,
            ac: 7,
            level: 1,
            to_hit_bonus: 0,
            damage_bonus: 0,
            is_dead: false,
        };

        // Threshold = 10 + 7 + 4 = 21. Roll 15 lands.
        // Damage = 8 + 1 + 2 = 11.
        let result = resolve_melee_attack(4, 2, goblin, 15, 8, 1);
        assert!(result.hit);
        assert_eq!(result.damage_dealt, 11);
        assert_eq!(result.defender_after.hp, 1);
        assert!(!result.defender_after.is_dead);
    }

    #[test]
    fn test_combat_lethal_damage() {
        let goblin = Combatant {
            hp: 8,
            max_hp: 12,
            ac: 7,
            level: 1,
            to_hit_bonus: 0,
            damage_bonus: 0,
            is_dead: false,
        };

        let result = resolve_melee_attack(4, 2, goblin, 10, 8, 1);
        assert!(result.hit);
        assert_eq!(result.damage_dealt, 11);
        assert_eq!(result.defender_after.hp, 0);
        assert!(result.defender_after.is_dead);
    }

    #[test]
    fn test_combat_miss() {
        let goblin = Combatant {
            hp: 12,
            max_hp: 12,
            ac: -5, // Good armor
            level: 5,
            to_hit_bonus: 0,
            damage_bonus: 0,
            is_dead: false,
        };

        // Threshold = 10 + (-5) + 0 = 5. Roll 10 misses.
        let result = resolve_melee_attack(0, 0, goblin.clone(), 10, 8, 0);
        assert!(!result.hit);
        assert_eq!(result.damage_dealt, 0);
        assert_eq!(result.defender_after, goblin);
    }

    #[test]
    fn test_combat_negative_ac_absorption() {
        let plate_knight = Combatant {
            hp: 20,
            max_hp: 20,
            ac: -4, // -4 AC absorbs 4 damage
            level: 5,
            to_hit_bonus: 0,
            damage_bonus: 0,
            is_dead: false,
        };

        // Threshold = 10 + (-4) + 15 = 21. Roll 10 hits!
        // Raw damage = 8 + 0 + 2 = 10.
        // AC absorption: 10 - 4 = 6 damage.
        let result = resolve_melee_attack(15, 2, plate_knight, 10, 8, 0);
        assert!(result.hit);
        assert_eq!(result.damage_dealt, 6);
        assert_eq!(result.defender_after.hp, 14);
    }
}
