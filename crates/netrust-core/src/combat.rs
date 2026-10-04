//! Combat Resolution Mechanics.
//!
//! Formalized and verified in `NetMechanics.Combat`.
//! Replaces NetHack's `uhitm.c` / `mhitm.c` to-hit and AC calculation with a pure transition.

use netrust_types::{Attack, AttackType, DamageType, Intrinsics};
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
/// vs a non-perceiving monster, -2 when the monster is trapped. Each attack
/// slot `i` is then tested with [`monster_attack_hits`] against `rnd(20 + i)`.
pub fn monster_to_hit_value(m_level: i32, hero_ac: i32, ac_roll: u32) -> i32 {
    ac_value(hero_ac, ac_roll)
        .saturating_add(10)
        .saturating_add(m_level)
        .max(1)
}

/// `AC_VALUE(ac) = ac >= 0 ? ac : -rnd(-ac)` (C `hack.h:1538`).
///
/// - `ac_roll`: the `rnd(-ac)` draw, range `1..=-ac`; clamped into that range
///   and ignored when `ac >= 0`.
pub fn ac_value(ac: i32, ac_roll: u32) -> i32 {
    if ac >= 0 {
        ac
    } else {
        let max = ac.unsigned_abs();
        -(ac_roll.clamp(1, max).min(i32::MAX as u32) as i32)
    }
}

/// Does a ray (monster breath) hit the hero? C `zap_hit(u.uac, 0)` (`zap.c:4705`),
/// called from `buzz` (`zap.c:4962`):
///
/// `chance = rn2(20); if (!chance) return rnd(10) < ac; ac = AC_VALUE(ac); return 3 - chance < ac;`
///
/// - `chance`: the `rn2(20)` draw, range `0..=19`; clamped to at most 19.
/// - `rnd10`: the `rnd(10)` draw, range `1..=10`, used only when `chance == 0`; clamped.
/// - `ac_roll`: the `AC_VALUE` `rnd(-ac)` draw, used only when `chance != 0` and `ac < 0`.
pub fn zap_hit(ac: i32, chance: u32, rnd10: u32, ac_roll: u32) -> bool {
    let chance = chance.min(19) as i32;
    if chance == 0 {
        return (rnd10.clamp(1, 10) as i32) < ac;
    }
    3 - chance < ac_value(ac, ac_roll)
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
/// The damage die size for a hero melee attack (C `NetHack-5.0.0/src/weapon.c:216`
/// and `NetHack-5.0.0/src/uhitm.c:847`):
///
/// - Weapon (`Some((small, large))`): `large` if `target_large`, else `small`.
/// - Bare hands (`None`): `4` for martial arts (Monk, C `uhitm.c:847`), else `2` (C `uhitm.c:847`).
pub fn weapon_damage_die(
    weapon: Option<(u32, u32)>,
    target_large: bool,
    martial_arts: bool,
) -> u32 {
    match weapon {
        Some((small, large)) => {
            if target_large {
                large
            } else {
                small
            }
        }
        None => {
            if martial_arts {
                4
            } else {
                2
            }
        }
    }
}

/// Hero base weapon damage (C `dmgval`, `NetHack-5.0.0/src/weapon.c:216` and
/// `NetHack-5.0.0/src/uhitm.c:847`):
///
/// Draws `rnd(die)` where `die` is from [`weapon_damage_die`].
/// Returns 0 if `die == 0`, otherwise `roll.clamp(1, die)`.
pub fn dmgval(
    weapon: Option<(u32, u32)>,
    target_large: bool,
    martial_arts: bool,
    roll: u32,
) -> u32 {
    let die = weapon_damage_die(weapon, target_large, martial_arts);
    if die == 0 {
        0
    } else {
        roll.clamp(1, die)
    }
}

/// Damage of a landed hit before hero-AC reduction, raised to at least 1
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

/// Is `at` one of the adjacent "hand to hand" attack types resolved with the
/// `tmp > rnd(20 + i)` hit test (C `mattacku`, `mhitu.c:794-806` and AT_WEAP
/// `mhitu.c:883-912`; `mattackm`, `mhitm.c:393-442`)?
///
/// AT_BREA (ranged only, `mhitu.c:873`), AT_GAZE (`mhitu.c:832`), AT_MAGC
/// (`mhitu.c:926`) and AT_NONE (passive) are not melee hits.
pub fn is_melee_attack(at: AttackType) -> bool {
    matches!(
        at,
        AttackType::Claw
            | AttackType::Bite
            | AttackType::Kick
            | AttackType::Touch
            | AttackType::Weapon
    )
}

/// The to-hit die for the `i`-th attack slot: `rnd(20 + i)`
/// (C `mhitu.c:806`, `mhitm.c:441`).
pub fn mattacku_die(attack_index: u32) -> u32 {
    20u32.saturating_add(attack_index)
}

/// Monster-vs-monster to-hit (C `mattackm`, `mhitm.c:321`):
/// `tmp = find_mac(mdef) + magr->m_lev` (no `+10`, no floor), tested against
/// `rnd(20 + i)` (`mhitm.c:441`). Returns `(tmp, die)` with `die = 20 + i`.
///
/// Omitted C terms: `+4` when the defender is confused or helpless, `+1` for an
/// elf attacking an orc, and the attacker's weapon `hitval`.
pub fn mhitm_to_hit(attacker_level: i32, defender_ac: i32, attack_index: u32) -> (i32, u32) {
    (
        defender_ac.saturating_add(attacker_level),
        mattacku_die(attack_index),
    )
}

/// Per-attack hit test `tmp > rnd(die)` (C `mhitu.c:806`, `mhitm.c:441-442`).
///
/// - `roll`: the `rnd(die)` draw, range `1..=die`; clamped into that range.
pub fn monster_attack_hits(tmp: i32, die: u32, roll: u32) -> bool {
    let r = roll.clamp(1, die.max(1));
    i64::from(tmp) > i64::from(r)
}

/// Damage dice of one monster attack: C `d(damn, damd)` (`rnd.c` `d()`, used by
/// `hitmu` `mhitu.c:1187` and `mdamagem` `mhitm.c:1025`).
///
/// - `dice_rolls`: the `n` `rnd(d)` draws, each range `1..=d`; each is clamped
///   into that range, only the first `n` count and a missing roll counts as 1.
/// - `n == 0` (e.g. the floating eye's passive `0d70`) or `d == 0` (Medusa's
///   gaze `0d0`) yields 0: C draws no dice.
pub fn monster_attack_damage(attack: &Attack, dice_rolls: &[u32]) -> u32 {
    let d = u32::from(attack.d);
    if d == 0 {
        return 0;
    }
    (0..usize::from(attack.n))
        .map(|i| dice_rolls.get(i).copied().unwrap_or(1).clamp(1, d))
        .sum()
}

/// Does the defender's resistance zero this attack's damage?
///
/// C (`mhitm_adtyping`): `mhitm_ad_fire` (`uhitm.c:2521`) and `mhitm_ad_cold`
/// (`uhitm.c:2626`) set the damage to 0 under Fire/Cold resistance. Other
/// bestiary damage types keep their dice damage: AD_DRST (`uhitm.c:3122`) only
/// resists the `!rn2(8)` poisoning side effect, AD_PHYS has no resistance, and
/// the remaining AD types (stoning, level drain, slow, stun, paralysis,
/// AD_SAMU, spells) are treated as physical damage (side effects deferred).
pub fn resisted(ad: DamageType, intrinsics: &Intrinsics) -> bool {
    match ad {
        DamageType::Fire => intrinsics.fire_resistance,
        DamageType::Cold => intrinsics.cold_resistance,
        _ => false,
    }
}

/// Damage of one landed monster attack (C `hitmu`, `mhitu.c:1187-1211`; `mdamagem`,
/// `mhitm.c:1025`): `d(n, d)` ([`monster_attack_damage`]), zeroed when
/// `is_resisted` ([`resisted`]), then for a hero defender the negative-AC
/// absorption [`hero_damage_after_ac`] (a zero stays zero).
///
/// - `hero`: `Some((u.uac, rnd(-u.uac) draw))` when the defender is the hero,
///   `None` for monster defenders (no AC absorption in `mdamagem`).
pub fn monster_hit_damage(
    attack: &Attack,
    dice_rolls: &[u32],
    is_resisted: bool,
    hero: Option<(i32, u32)>,
) -> u32 {
    let dmg = if is_resisted {
        0
    } else {
        monster_attack_damage(attack, dice_rolls)
    };
    match hero {
        Some((hero_ac, absorb_roll)) => hero_damage_after_ac(dmg, hero_ac, absorb_roll),
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

    fn atk(at: AttackType, ad: DamageType, n: u8, d: u8) -> Attack {
        Attack { at, ad, n, d }
    }

    #[test]
    fn monster_attack_damage_sums_clamped_dice() {
        let jackal_bite = atk(AttackType::Bite, DamageType::Phys, 1, 2);
        assert_eq!(monster_attack_damage(&jackal_bite, &[1]), 1);
        assert_eq!(monster_attack_damage(&jackal_bite, &[2]), 2);
        // Each die is clamped into 1..=d.
        assert_eq!(monster_attack_damage(&jackal_bite, &[0]), 1);
        assert_eq!(monster_attack_damage(&jackal_bite, &[99]), 2);
        let bite_3d8 = atk(AttackType::Bite, DamageType::Phys, 3, 8);
        assert_eq!(monster_attack_damage(&bite_3d8, &[8, 1, 5]), 14);
        // Only the first n rolls count; a missing roll counts as 1.
        assert_eq!(monster_attack_damage(&bite_3d8, &[8, 8, 8, 8]), 24);
        assert_eq!(monster_attack_damage(&bite_3d8, &[8]), 10);
        // d(0, x) == 0 (C rnd.c d(): no dice drawn), and a 0-sided die is 0.
        let passive = atk(AttackType::Passive, DamageType::Paralyze, 0, 70);
        assert_eq!(monster_attack_damage(&passive, &[]), 0);
        let gaze = atk(AttackType::Gaze, DamageType::Stone, 0, 0);
        assert_eq!(monster_attack_damage(&gaze, &[5]), 0);
        assert_eq!(
            monster_attack_damage(&atk(AttackType::Claw, DamageType::Phys, 2, 0), &[5, 5]),
            0
        );
    }

    #[test]
    fn resisted_only_fire_and_cold_zero_damage() {
        let mut intr = Intrinsics::empty();
        assert!(!resisted(DamageType::Fire, &intr));
        assert!(!resisted(DamageType::Cold, &intr));
        intr.fire_resistance = true;
        assert!(resisted(DamageType::Fire, &intr));
        assert!(!resisted(DamageType::Cold, &intr));
        intr.cold_resistance = true;
        assert!(resisted(DamageType::Cold, &intr));
        // AD_DRST damage is not zeroed by poison resistance (uhitm.c:3122:
        // only the rn2(8) poisoning is resisted); AD_PHYS never.
        intr.poison_resistance = true;
        assert!(!resisted(DamageType::DrainStr, &intr));
        assert!(!resisted(DamageType::Phys, &intr));
    }

    #[test]
    fn mhitm_to_hit_has_no_plus_ten() {
        // mhitm.c:321 tmp = find_mac(mdef) + m_lev; die rnd(20 + i)
        assert_eq!(mhitm_to_hit(2, 7, 0), (9, 20));
        assert_eq!(mhitm_to_hit(4, -3, 2), (1, 22));
        assert_eq!(mhitm_to_hit(0, -10, 5), (-10, 25));
    }

    #[test]
    fn monster_attack_hits_uses_rnd_20_plus_i() {
        // tmp > rnd(20 + i); the roll is clamped into 1..=die.
        assert!(monster_attack_hits(22, 22, 21));
        assert!(!monster_attack_hits(21, 22, 21));
        assert!(!monster_attack_hits(1, 20, 0));
        assert!(monster_attack_hits(2, 20, 0));
        assert!(!monster_attack_hits(23, 23, 99));
        assert!(monster_attack_hits(24, 23, 99));
        assert_eq!(mattacku_die(0), 20);
        assert_eq!(mattacku_die(3), 23);
    }

    #[test]
    fn melee_attack_types_match_mattacku() {
        for at in [
            AttackType::Claw,
            AttackType::Bite,
            AttackType::Kick,
            AttackType::Touch,
            AttackType::Weapon,
        ] {
            assert!(is_melee_attack(at));
        }
        for at in [
            AttackType::Breath,
            AttackType::Gaze,
            AttackType::Magic,
            AttackType::Passive,
        ] {
            assert!(!is_melee_attack(at));
        }
    }

    #[test]
    fn monster_hit_damage_pipeline() {
        let fire = atk(AttackType::Touch, DamageType::Fire, 2, 6);
        // Unresisted, hero AC 10: plain d(2,6).
        assert_eq!(monster_hit_damage(&fire, &[3, 4], false, Some((10, 1))), 7);
        // Resisted: 0, and AC absorption does not raise it to 1.
        assert_eq!(monster_hit_damage(&fire, &[3, 4], true, Some((-5, 3))), 0);
        // Hero AC -5 absorbs rnd(5).
        assert_eq!(monster_hit_damage(&fire, &[3, 4], false, Some((-5, 3))), 4);
        assert_eq!(monster_hit_damage(&fire, &[1, 1], false, Some((-5, 5))), 1);
        // Monster defender: no absorption.
        assert_eq!(monster_hit_damage(&fire, &[6, 6], false, None), 12);
    }

    #[test]
    fn zap_hit_c_rule() {
        // chance 0: rnd(10) < ac
        assert!(zap_hit(10, 0, 9, 1));
        assert!(!zap_hit(10, 0, 10, 1));
        assert!(!zap_hit(0, 0, 1, 1));
        // chance != 0: 3 - chance < AC_VALUE(ac)
        assert!(zap_hit(0, 4, 1, 1));
        assert!(!zap_hit(0, 3, 1, 1));
        assert!(zap_hit(-5, 19, 1, 5)); // -16 < -5
        assert!(!zap_hit(-5, 2, 1, 5)); // 1 < -5 is false
        assert!(zap_hit(-5, 9, 1, 1)); // -6 < -1
        assert_eq!(ac_value(-5, 0), -1);
        assert_eq!(ac_value(-5, 99), -5);
        assert_eq!(ac_value(3, 99), 3);
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

    #[test]
    fn weapon_damage_die_c_rules() {
        // Dagger: (4, 3) per C objects.h
        assert_eq!(weapon_damage_die(Some((4, 3)), false, false), 4);
        assert_eq!(weapon_damage_die(Some((4, 3)), true, false), 3);
        // Long sword: (8, 12) per C objects.h
        assert_eq!(weapon_damage_die(Some((8, 12)), false, false), 8);
        assert_eq!(weapon_damage_die(Some((8, 12)), true, false), 12);
        // Bare hands non-Monk: C uhitm.c:847 rnd(2)
        assert_eq!(weapon_damage_die(None, false, false), 2);
        assert_eq!(weapon_damage_die(None, true, false), 2);
        // Bare hands Monk martial arts: C uhitm.c:847 rnd(4)
        assert_eq!(weapon_damage_die(None, false, true), 4);
        assert_eq!(weapon_damage_die(None, true, true), 4);
    }

    #[test]
    fn dmgval_c_rules_and_clamping() {
        // Dagger small target (die 4): rolls clamped to 1..=4
        assert_eq!(dmgval(Some((4, 3)), false, false, 0), 1);
        assert_eq!(dmgval(Some((4, 3)), false, false, 1), 1);
        assert_eq!(dmgval(Some((4, 3)), false, false, 4), 4);
        assert_eq!(dmgval(Some((4, 3)), false, false, 5), 4);

        // Dagger large target (die 3): rolls clamped to 1..=3
        assert_eq!(dmgval(Some((4, 3)), true, false, 3), 3);
        assert_eq!(dmgval(Some((4, 3)), true, false, 4), 3);

        // Long sword large target (die 12)
        assert_eq!(dmgval(Some((8, 12)), true, false, 12), 12);
        assert_eq!(dmgval(Some((8, 12)), true, false, 15), 12);

        // Bare hands (die 2): rolls clamped to 1..=2
        assert_eq!(dmgval(None, false, false, 1), 1);
        assert_eq!(dmgval(None, false, false, 2), 2);
        assert_eq!(dmgval(None, false, false, 3), 2);

        // Martial arts (die 4): rolls clamped to 1..=4
        assert_eq!(dmgval(None, false, true, 3), 3);
        assert_eq!(dmgval(None, false, true, 4), 4);
        assert_eq!(dmgval(None, false, true, 10), 4);

        // Zero die gives 0
        assert_eq!(dmgval(Some((0, 0)), false, false, 5), 0);
    }
}
