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

/// Hero luck contribution to melee to-hit: `sgn(Luck) * ((|Luck| + 2) / 3)`.
///
/// C: `uhitm.c:365` (`find_roll_to_hit`). Luck is clamped to the C range
/// `-13..=13` (`u.uluck + u.moreluck`), giving a bonus in `-5..=5`.
pub fn luck_to_hit_bonus(luck: i32) -> i32 {
    let l = luck.clamp(-13, 13);
    l.signum() * ((l.abs() + 2) / 3)
}

/// Hero melee to-hit value `tmp` (C `find_roll_to_hit`, `uhitm.c:365`):
///
/// `tmp = 1 + abon + find_mac(mdef) + u.ulevel + luck bonus + hitval + weapon_hit_bonus`
///
/// - `level`: hero experience level (`u.ulevel`).
/// - `luck`: current Luck, clamped to `-13..=13` (see [`luck_to_hit_bonus`]).
/// - `enchant`: wielded weapon `spe` (the `hitval` term, `weapon.c:149`).
/// - `skill_hit`: `weapon_hit_bonus` (`weapon.c:1545`, bare-handed `weapon.c:1601`).
/// - `target_ac`: defender AC (`find_mac`); higher AC is easier to hit.
///
/// Omitted C terms (not tracked by NetRust): `abon()` (Str/Dex, low-level
/// +1; taken as 0), `u.uhitinc` (rings of increase accuracy), monster
/// stunned/fleeing/sleeping/immobile bonuses, Monk armor/martial bonuses,
/// elf-vs-orc, encumbrance (`near_capacity`) and trapped (`u.utrap`)
/// penalties, `oc_hitbon` and blessed-vs-undead `hitval` extras, riding.
pub fn to_hit_value(level: i32, luck: i32, enchant: i32, skill_hit: i32, target_ac: i32) -> i32 {
    1i32.saturating_add(target_ac)
        .saturating_add(level)
        .saturating_add(luck_to_hit_bonus(luck))
        .saturating_add(enchant)
        .saturating_add(skill_hit)
}

/// Monster-vs-hero to-hit value (C `mattacku`, `mhitu.c:709`):
///
/// `tmp = AC_VALUE(u.uac) + 10 + m_lev`, at least 1, where
/// `AC_VALUE(ac) = ac >= 0 ? ac : -rnd(-ac)` (`hack.h:1538`).
///
/// - `ac_roll`: the `rnd(-ac)` draw, range `1..=-hero_ac`; clamped into that
///   range and ignored when `hero_ac >= 0`.
///
/// Omitted C terms: +4 when the hero is helpless, -2 for an invisible hero
/// vs a non-perceiving monster, -2 when the monster is trapped, and the
/// per-attack-index `rnd(20 + i)` die (the sim makes a single attack).
pub fn monster_to_hit_value(m_level: i32, hero_ac: i32, ac_roll: u32) -> i32 {
    let ac_value = if hero_ac >= 0 {
        hero_ac
    } else {
        let max = hero_ac.unsigned_abs();
        -(ac_roll.clamp(1, max).min(i32::MAX as u32) as i32)
    };
    ac_value.saturating_add(10).saturating_add(m_level).max(1)
}

/// Hit test: `mhit = tmp > dieroll` with `dieroll = rnd(20)`
/// (C `uhitm.c:780-782`; same comparison in `mhitu.c`).
///
/// - `d20`: the `rnd(20)` draw, range `1..=20`; out-of-range values are clamped.
pub fn attack_hits(d20: u32, to_hit: i32) -> bool {
    (d20.clamp(1, 20) as i32) < to_hit
}

/// Damage of a landed melee hit before any hero-AC reduction:
/// `base_roll + enchant + dmg_bonus`, raised to 1 if lower
/// (C `hmon_hitmon_dmg_recalc`, `uhitm.c:1505`: "don't let penalty turn a hit into a miss").
///
/// - `base_roll`: the weapon/attack damage dice draw (`dmgval`/`d(n, s)`).
pub fn melee_damage(base_roll: u32, enchant: i32, dmg_bonus: i32) -> u32 {
    let total = i64::from(base_roll) + i64::from(enchant) + i64::from(dmg_bonus);
    total.clamp(1, i64::from(u32::MAX)) as u32
}

/// Negative hero AC damage reduction (C `hitmu`, `mhitu.c:1208-1211`):
///
/// `if (dmg && u.uac < 0) { dmg -= rnd(-u.uac); if (dmg < 1) dmg = 1; }`
///
/// Applies only to damage dealt TO THE HERO; monsters get no AC reduction.
/// - `absorb_roll`: the `rnd(-u.uac)` draw, range `1..=-hero_ac`; clamped into
///   that range and ignored when `hero_ac >= 0` or `damage == 0`.
pub fn hero_damage_after_ac(damage: u32, hero_ac: i32, absorb_roll: u32) -> u32 {
    if damage == 0 || hero_ac >= 0 {
        return damage;
    }
    let absorb = absorb_roll.clamp(1, hero_ac.unsigned_abs());
    damage.saturating_sub(absorb).max(1)
}

/// Full damage of a landed hit: [`melee_damage`], then [`hero_damage_after_ac`]
/// when `hero_absorb_roll` is `Some` (the defender is the hero).
///
/// C: `uhitm.c:1505` (min 1) and `mhitu.c:1208` (hero AC reduction).
/// - `hero_absorb_roll`: `Some(rnd(-u.uac))` only when the defender is the hero;
///   `None` for monster defenders (no AC reduction in C `hmon`).
pub fn calculate_damage(
    base_roll: u32,
    enchant: i32,
    dmg_bonus: i32,
    defender_ac: i32,
    hero_absorb_roll: Option<u32>,
) -> u32 {
    let dmg = melee_damage(base_roll, enchant, dmg_bonus);
    match hero_absorb_roll {
        Some(roll) => hero_damage_after_ac(dmg, defender_ac, roll),
        None => dmg,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttackResult {
    pub hit: bool,
    pub damage_dealt: u32,
    pub defender_after: Combatant,
}

/// Resolve one melee attack from explicit rolls.
///
/// - `to_hit`: the C `tmp` ([`to_hit_value`] for the hero, [`monster_to_hit_value`]
///   for a monster attacker).
/// - `d20`: `rnd(20)` draw (`uhitm.c:780`), clamped to `1..=20`.
/// - `base_roll`, `enchant`, `dmg_bonus`: see [`melee_damage`] (`uhitm.c:1505`).
/// - `hero_absorb_roll`: `Some(rnd(-u.uac))` only when the defender is the hero
///   (`mhitu.c:1208`).
pub fn resolve_melee_attack(
    to_hit: i32,
    mut defender: Combatant,
    d20: u32,
    base_roll: u32,
    enchant: i32,
    dmg_bonus: i32,
    hero_absorb_roll: Option<u32>,
) -> AttackResult {
    if attack_hits(d20, to_hit) {
        let dmg = calculate_damage(base_roll, enchant, dmg_bonus, defender.ac, hero_absorb_roll);
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

    fn goblin(hp: u32, ac: i32) -> Combatant {
        Combatant {
            hp,
            max_hp: 12,
            ac,
            level: 1,
            to_hit_bonus: 0,
            damage_bonus: 0,
            is_dead: false,
        }
    }

    #[test]
    fn luck_bonus_matches_c_table() {
        // sgn(L) * ((|L| + 2) / 3)
        assert_eq!(luck_to_hit_bonus(0), 0);
        assert_eq!(luck_to_hit_bonus(1), 1);
        assert_eq!(luck_to_hit_bonus(3), 1);
        assert_eq!(luck_to_hit_bonus(4), 2);
        assert_eq!(luck_to_hit_bonus(13), 5);
        assert_eq!(luck_to_hit_bonus(-1), -1);
        assert_eq!(luck_to_hit_bonus(-13), -5);
        // Out-of-range luck is clamped to -13..=13.
        assert_eq!(luck_to_hit_bonus(100), 5);
        assert_eq!(luck_to_hit_bonus(i32::MIN), -5);
    }

    #[test]
    fn to_hit_value_c_formula_boundaries() {
        // tmp = 1 + AC + level + luck bonus + enchant + skill
        assert_eq!(to_hit_value(1, 0, 0, 0, 10), 12);
        assert_eq!(to_hit_value(1, 0, 0, 0, -10), -8);
        assert_eq!(to_hit_value(1, 13, 0, 0, 0), 7);
        assert_eq!(to_hit_value(1, -13, 0, 0, 0), -3);
        assert_eq!(to_hit_value(5, 0, 2, -4, 7), 11);
        // Higher target AC is easier to hit.
        assert!(to_hit_value(1, 0, 0, 0, 10) > to_hit_value(1, 0, 0, 0, -10));
    }

    #[test]
    fn attack_hits_is_strict_less_than() {
        assert!(!attack_hits(1, 1));
        assert!(attack_hits(1, 2));
        assert!(!attack_hits(20, 20));
        assert!(attack_hits(20, 21));
        assert!(attack_hits(19, 20));
        // d20 out of range is clamped to 1..=20.
        assert!(!attack_hits(0, 1));
        assert!(attack_hits(0, 2));
        assert!(attack_hits(999, 21));
        assert!(!attack_hits(999, 20));
    }

    #[test]
    fn monster_to_hit_value_c_formula() {
        // tmp = AC_VALUE(u.uac) + 10 + m_lev; AC_VALUE(ac) = ac >= 0 ? ac : -rnd(-ac)
        assert_eq!(monster_to_hit_value(1, 10, 1), 21);
        assert_eq!(monster_to_hit_value(0, 0, 1), 10);
        assert_eq!(monster_to_hit_value(3, -5, 2), 11);
        // ac roll clamped to 1..=-ac
        assert_eq!(monster_to_hit_value(3, -5, 0), 12);
        assert_eq!(monster_to_hit_value(3, -5, 99), 8);
        // never below 1
        assert_eq!(monster_to_hit_value(0, -20, 20), 1);
    }

    #[test]
    fn melee_damage_floor_is_one() {
        assert_eq!(melee_damage(0, 0, 0), 1);
        assert_eq!(melee_damage(1, -5, -2), 1);
        assert_eq!(melee_damage(6, 2, 1), 9);
    }

    #[test]
    fn hero_damage_after_ac_c_rule() {
        // non-negative AC: no reduction
        assert_eq!(hero_damage_after_ac(10, 0, 5), 10);
        assert_eq!(hero_damage_after_ac(10, 10, 5), 10);
        // ac -1: absorb roll clamped to 1..=1
        assert_eq!(hero_damage_after_ac(10, -1, 1), 9);
        assert_eq!(hero_damage_after_ac(10, -1, 7), 9);
        assert_eq!(hero_damage_after_ac(10, -1, 0), 9);
        // ac -20: absorb up to 20, floor 1
        assert_eq!(hero_damage_after_ac(10, -20, 5), 5);
        assert_eq!(hero_damage_after_ac(10, -20, 20), 1);
        assert_eq!(hero_damage_after_ac(10, -20, 999), 1);
        // zero damage is not raised (C: `if (dmg && u.uac < 0)`)
        assert_eq!(hero_damage_after_ac(0, -20, 5), 0);
        assert_eq!(hero_damage_after_ac(5, i32::MIN, u32::MAX), 1);
    }

    #[test]
    fn calculate_damage_absorbs_only_with_roll() {
        // Hero -> monster: no AC reduction even against negative AC.
        assert_eq!(calculate_damage(8, 1, 2, -5, None), 11);
        // Monster -> hero: rnd(-AC) reduction.
        assert_eq!(calculate_damage(8, 1, 2, -5, Some(3)), 8);
        assert_eq!(calculate_damage(0, 0, 0, 3, None), 1);
    }

    #[test]
    fn test_combat_hit_and_damage() {
        // tmp = 1 + 7 + 1 + 0 + 1 + 0 = 10; d20 = 9 < 10 hits.
        let tmp = to_hit_value(1, 0, 1, 0, 7);
        assert_eq!(tmp, 10);
        let result = resolve_melee_attack(tmp, goblin(12, 7), 9, 8, 1, 2, None);
        assert!(result.hit);
        assert_eq!(result.damage_dealt, 11);
        assert_eq!(result.defender_after.hp, 1);
        assert!(!result.defender_after.is_dead);
    }

    #[test]
    fn test_combat_lethal_damage() {
        let result = resolve_melee_attack(10, goblin(8, 7), 1, 8, 1, 2, None);
        assert!(result.hit);
        assert_eq!(result.damage_dealt, 11);
        assert_eq!(result.defender_after.hp, 0);
        assert!(result.defender_after.is_dead);
    }

    #[test]
    fn test_combat_miss() {
        // d20 = 10 is not < 10.
        let g = goblin(12, -5);
        let result = resolve_melee_attack(10, g.clone(), 10, 8, 0, 0, None);
        assert!(!result.hit);
        assert_eq!(result.damage_dealt, 0);
        assert_eq!(result.defender_after, g);
    }

    #[test]
    fn test_combat_negative_ac_absorption_hero_defender() {
        let mut hero = goblin(20, -4);
        hero.max_hp = 20;
        // Raw damage = 8 + 0 + 2 = 10; absorb roll 3 (1..=4) -> 7.
        let result = resolve_melee_attack(21, hero, 10, 8, 0, 2, Some(3));
        assert!(result.hit);
        assert_eq!(result.damage_dealt, 7);
        assert_eq!(result.defender_after.hp, 13);
    }

    #[test]
    fn test_combat_no_absorption_monster_defender() {
        let result = resolve_melee_attack(21, goblin(20, -4), 10, 8, 0, 2, None);
        assert_eq!(result.damage_dealt, 10);
    }
}
