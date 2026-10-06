//! Hero attributes and experience, ported from C `attrib.c` (`init_attr`,
//! `adjattrib`, `newhp`, `poisoned`) and `exper.c` (`experience`, `newuexp`,
//! `more_experienced`, `pluslvl`, `losexp`), over the role/race tables
//! extracted from `src/role.c`.

use nethacked_arena::ActorRecord;
use nethacked_data::roles::{race_stats, role_stats, Advance, RaceStats, RoleStats};
use nethacked_types::{AttackType, DamageType};
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::events::GameEvent;
use crate::world::SimulationWorld;

/// C `A_STR` .. `A_CHA` indices.
pub const A_STR: usize = 0;
pub const A_INT: usize = 1;
pub const A_WIS: usize = 2;
pub const A_DEX: usize = 3;
pub const A_CON: usize = 4;
pub const A_CHA: usize = 5;

/// Erosion kind for armor (C ERODE_RUST, ERODE_CORRODE, ERODE_ROT).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErosionKind {
    Rust,
    Corrode,
    Rot,
}

/// C `MAXULEV` (global.h).
const MAXULEV: u32 = 30;

/// The hero's attributes and experience bookkeeping (C `u.acurr`/`u.amax`,
/// `u.uexp`, `u.uhpinc[]`, `u.ueninc[]`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeroProgress {
    /// `ABASE()`: Str, Int, Wis, Dex, Con, Cha (Str 18/xx is 18+xx).
    pub attr_base: [i32; 6],
    /// `AMAX()`.
    pub attr_max: [i32; 6],
    /// `u.uexp`.
    pub exp: u64,
    /// `u.uhpinc[lev]`: HP gained when reaching level `lev + 1`.
    pub hp_increments: Vec<i32>,
    /// `u.ueninc[lev]`.
    pub en_increments: Vec<i32>,
    /// `u.ulevelmax`.
    pub level_max: u32,
}

/// C `newuexp()` (exper.c:14): experience needed for level `lev + 1`.
pub fn newuexp(lev: u32) -> u64 {
    match lev {
        0 => 0,
        1..=9 => 10 * (1u64 << lev),
        10..=19 => 10_000 * (1u64 << (lev - 10)),
        _ => 10_000_000 * u64::from(lev - 19),
    }
}

/// C `experience()` (exper.c:85-160) for killing a monster: `m_lev`, AC,
/// speed and attacks from the monster's data. Revived/cloned discounts and
/// the eel-drowning bonus (needs `Amphibious`) are not modelled.
pub fn experience(
    level: u32,
    ac: i32,
    speed: u32,
    attacks: &[nethacked_types::Attack],
    nasty: bool,
    mail_daemon: bool,
) -> u64 {
    let lev = level as i64;
    let mut tmp: i64 = 1 + lev * lev;
    if ac < 3 {
        tmp += i64::from(7 - ac) * if ac < 0 { 2 } else { 1 };
    }
    if speed > 12 {
        tmp += if speed > 18 { 5 } else { 3 };
    }
    for a in attacks {
        // C: aatyp > AT_BUTT (AT_NONE..AT_BUTT are 0..4)
        let weak = matches!(
            a.at,
            AttackType::Passive
                | AttackType::Claw
                | AttackType::Bite
                | AttackType::Kick
                | AttackType::Butt
        );
        if !weak {
            tmp += match a.at {
                AttackType::Weapon => 5,
                AttackType::Magic => 10,
                _ => 3,
            };
        }
    }
    for a in attacks {
        // C: AD_PHYS < adtyp < AD_BLND (MAGM, FIRE, COLD, SLEE, DISN, ELEC, DRST, ACID)
        let elemental = matches!(
            a.ad,
            DamageType::MagicMissile
                | DamageType::Fire
                | DamageType::Cold
                | DamageType::Sleep
                | DamageType::Disintegrate
                | DamageType::Elec
                | DamageType::DrainStr
                | DamageType::Acid
        );
        if elemental {
            tmp += 2 * lev;
        } else if matches!(
            a.ad,
            DamageType::DrainLife | DamageType::Stone | DamageType::Slime
        ) {
            tmp += 50;
        } else if a.ad != DamageType::Phys {
            tmp += lev;
        }
        if i64::from(a.d) * i64::from(a.n) > 23 {
            tmp += lev;
        }
    }
    if nasty {
        tmp += 7 * lev;
    }
    if level > 8 {
        tmp += 50;
    }
    if mail_daemon {
        tmp = 1;
    }
    tmp.max(0) as u64
}

impl SimulationWorld {
    fn stats(&self) -> (&'static RoleStats, &'static RaceStats) {
        let role = role_stats(&self.role_name)
            .or_else(|| role_stats("valkyrie"))
            .expect("C role table");
        (role, race_stats(self.hero_race))
    }

    fn hero_level(&self) -> u32 {
        self.arena.actors.get(self.player_id).map_or(1, |p| p.level)
    }

    /// C `init_attr(75)` (attrib.c): role minimums, then the remaining points
    /// distributed by the role's weights up to the race maximum.
    pub(crate) fn init_hero_attributes(&mut self) {
        let (role, race) = self.stats();
        let mut np: i32 = 75;
        let mut base = role.attr_base;
        np -= base.iter().sum::<i32>();
        let mut tries = 0;
        while np > 0 && tries < 100 {
            let mut x = self.rng.random_range(0..100i32);
            let mut i = 0;
            while i < 6 {
                x -= role.attr_dist[i];
                if x < 0 {
                    break;
                }
                i += 1;
            }
            if i >= 6 {
                continue;
            }
            if base[i] >= race.attr_max[i] {
                tries += 1;
                continue;
            }
            tries = 0;
            base[i] += 1;
            np -= 1;
        }
        self.progress.attr_base = base;
        self.progress.attr_max = base;
        self.progress.level_max = self.hero_level();
    }

    /// `ACURR(i)` without worn-item bonuses (not modelled yet); C caps Str at 125.
    pub fn hero_attr(&self, i: usize) -> i32 {
        self.progress.attr_base[i]
    }

    /// C `adjattrib(ndx, incr, msgflg)` decrease path (attrib.c), clamped to the
    /// race minimum. Returns whether the attribute changed.
    pub(crate) fn adjattrib(&mut self, i: usize, incr: i32) -> bool {
        if incr == 0 || self.progress.attr_base == [0; 6] {
            return false;
        }
        let (_, race) = self.stats();
        let old = self.progress.attr_base[i];
        self.progress.attr_base[i] += incr;
        if incr > 0 {
            if self.progress.attr_base[i] > self.progress.attr_max[i] {
                self.progress.attr_max[i] = self.progress.attr_base[i].min(race.attr_max[i]);
                self.progress.attr_base[i] = self.progress.attr_max[i];
            }
        } else if self.progress.attr_base[i] < race.attr_min[i] {
            let decr = self
                .rng
                .random_range(0..=(race.attr_min[i] - self.progress.attr_base[i]));
            self.progress.attr_base[i] = race.attr_min[i];
            self.progress.attr_max[i] = (self.progress.attr_max[i] - decr).max(race.attr_min[i]);
        }
        self.progress.attr_base[i] != old
    }

    fn roll_advance(&mut self, role: &Advance, race: &Advance, level: u32, xlev: u32) -> i32 {
        let (fix, rnd) = if level == 0 {
            ((role.infix + race.infix), [role.inrnd, race.inrnd])
        } else if level < xlev {
            ((role.lofix + race.lofix), [role.lornd, race.lornd])
        } else {
            ((role.hifix + race.hifix), [role.hirnd, race.hirnd])
        };
        let mut v = fix;
        for r in rnd {
            if r > 0 {
                v += self.rng.random_range(1..=r);
            }
        }
        v
    }

    /// C `newhp()` (attrib.c) for a level gain at the current level.
    fn newhp(&mut self) -> i32 {
        let (role, race) = self.stats();
        let level = self.hero_level();
        let mut hp = self.roll_advance(&role.hp, &race.hp, level, role.xlev);
        if level > 0 {
            hp += match self.hero_attr(A_CON) {
                ..=3 => -2,
                4..=6 => -1,
                7..=14 => 0,
                15..=16 => 1,
                17 => 2,
                18 => 3,
                _ => 4,
            };
        }
        hp.max(1)
    }

    /// C `newpw()` (exper.c) without the `enermod` role multiplier's Wis term
    /// simplified: role/race energy advance plus `enermod`.
    fn newpw(&mut self) -> i32 {
        let (role, race) = self.stats();
        let level = self.hero_level();
        let en = self.roll_advance(&role.energy, &race.energy, level, role.xlev);
        let en = match self.role_name.to_ascii_lowercase().as_str() {
            "priest" | "cleric" | "wizard" => 2 * en,
            "healer" | "knight" => (3 * en) / 2,
            "barbarian" | "valkyrie" => (3 * en) / 4,
            _ => en,
        };
        en.max(0)
    }

    /// C `more_experienced(exp, 0)` then `newexplevel()` (exper.c:169, 300).
    pub(crate) fn gain_experience(&mut self, exp: u64, events: &mut Vec<GameEvent>) {
        self.progress.exp = self.progress.exp.saturating_add(exp);
        let level = self.hero_level();
        if level < MAXULEV && self.progress.exp >= newuexp(level) {
            self.pluslvl(events);
        }
    }

    /// C `pluslvl(TRUE)` (exper.c:307).
    pub(crate) fn pluslvl(&mut self, events: &mut Vec<GameEvent>) {
        let hpinc = self.newhp();
        let eninc = self.newpw();
        let level = self.hero_level();
        let idx = level as usize;
        if self.progress.hp_increments.len() <= idx {
            self.progress.hp_increments.resize(idx + 1, 0);
            self.progress.en_increments.resize(idx + 1, 0);
        }
        self.progress.hp_increments[idx] = hpinc;
        self.progress.en_increments[idx] = eninc;
        if let Some(p) = self.arena.actors.get_mut(self.player_id) {
            p.max_hp = p.max_hp.saturating_add(hpinc as u32);
            p.hp = p.hp.saturating_add(hpinc as u32);
        }
        self.player_max_pw = self.player_max_pw.saturating_add(eninc as u32);
        self.player_pw = self.player_pw.saturating_add(eninc as u32);
        if level < MAXULEV {
            let cap = newuexp(level + 1);
            if self.progress.exp >= cap {
                self.progress.exp = cap - 1;
            }
            let new_level = level + 1;
            if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                p.level = new_level;
            }
            let back = if self.progress.level_max < new_level {
                ""
            } else {
                "back "
            };
            events.push(GameEvent::LogMessage {
                text: format!("Welcome {back}to experience level {new_level}."),
            });
            self.progress.level_max = self.progress.level_max.max(new_level);
        }
    }

    /// C `losexp(drainer)` (exper.c:207). Returns `true` when the drain killed
    /// a level-1 hero.
    pub(crate) fn losexp(&mut self, drainer: Option<&str>, events: &mut Vec<GameEvent>) -> bool {
        let level = self.hero_level();
        if level > 1 || drainer.is_some() {
            events.push(GameEvent::LogMessage {
                text: format!("Goodbye level {level}."),
            });
        }
        if level > 1 {
            if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                p.level = level - 1;
            }
        } else {
            if drainer.is_some() {
                if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                    p.hp = 0;
                    p.is_dead = true;
                }
                return true;
            }
            self.progress.exp = 0;
        }
        let new_level = self.hero_level();
        let idx = new_level as usize;
        let num = self
            .progress
            .hp_increments
            .get(idx)
            .copied()
            .unwrap_or(0)
            .max(0) as u32;
        let uhpmin = new_level.max(10);
        if let Some(p) = self.arena.actors.get_mut(self.player_id) {
            let old_max = p.max_hp;
            p.max_hp = p.max_hp.saturating_sub(num).max(uhpmin).min(old_max);
            p.hp = p.hp.saturating_sub(num).clamp(1, p.max_hp);
        }
        let en = self
            .progress
            .en_increments
            .get(idx)
            .copied()
            .unwrap_or(0)
            .max(0) as u32;
        self.player_max_pw = self.player_max_pw.saturating_sub(en);
        self.player_pw = self.player_pw.saturating_sub(en).min(self.player_max_pw);
        if self.progress.exp > 0 {
            self.progress.exp = newuexp(new_level).saturating_sub(1);
        }
        false
    }

    /// C `poisoned(reason, typ, pkiller, 30, FALSE)` (attrib.c:317) for a
    /// monster's poisonous hit. Returns `true` if the poison killed the hero.
    pub(crate) fn poisoned(
        &mut self,
        reason: &str,
        typ: usize,
        events: &mut Vec<GameEvent>,
    ) -> bool {
        let say =
            |events: &mut Vec<GameEvent>, t: String| events.push(GameEvent::LogMessage { text: t });
        say(events, format!("{reason} was poisoned!"));
        let resists = self
            .arena
            .actors
            .get(self.player_id)
            .is_some_and(|p| p.intrinsics.poison_resistance);
        if resists {
            say(events, "The poison doesn't seem to affect you.".into());
            return false;
        }
        let i = self.rng.random_range(0..30u32);
        if i == 0 {
            let loss: u32 = 6 + (0..4).map(|_| self.rng.random_range(1..=6u32)).sum::<u32>();
            let hp = self.arena.actors.get(self.player_id).map_or(0, |p| p.hp);
            if hp <= loss {
                if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                    p.hp = 0;
                    p.is_dead = true;
                }
                say(events, "The poison was deadly...".into());
                return true;
            }
            let level = self.hero_level();
            if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                p.max_hp = p.max_hp.saturating_sub(loss / 2).max(level.max(3));
                p.hp = p.hp.saturating_sub(loss).min(p.max_hp).max(1);
            }
            if self.adjattrib(A_CON, if typ != A_CON { -1 } else { -3 }) {
                say(events, poison_tell(A_CON).into());
            }
            if typ != A_CON && self.adjattrib(typ, -3) {
                say(events, poison_tell(typ).into());
            }
        } else if i > 5 {
            let loss = 6 + self.rng.random_range(0..10u32); // rn1(10, 6)
            if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                p.hp = p.hp.saturating_sub(loss);
                if p.hp == 0 {
                    p.is_dead = true;
                    return true;
                }
            }
        } else {
            let loss = (0..2).map(|_| self.rng.random_range(1..=2i32)).sum::<i32>(); // d(2,2)
            if self.adjattrib(typ, -loss) {
                say(events, poison_tell(typ).into());
            }
        }
        false
    }

    /// Experience for the hero killing `victim` (C `xkilled` -> `experience`).
    pub(crate) fn award_kill_experience(
        &mut self,
        victim: &ActorRecord,
        events: &mut Vec<GameEvent>,
    ) {
        let rs = std::sync::Arc::clone(&self.ruleset);
        let Some(def) = rs.monster(&victim.name) else {
            return;
        };
        let exp = experience(
            victim.level,
            victim.ac,
            def.speed,
            &def.attacks,
            def.has_flag("nasty"),
            def.id == Some(nethacked_data::MonsterSpeciesId::MAIL_DAEMON),
        );
        self.gain_experience(exp, events);
    }
}

/// C `poiseff[]` (attrib.c:283).
fn poison_tell(typ: usize) -> &'static str {
    match typ {
        0 => "You feel weaker!",
        1 => "Your brain is on fire!",
        2 => "Your judgement is impaired!",
        3 => "Your muscles won't obey you!",
        4 => "You feel very sick!",
        _ => "You break out in hives!",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newuexp_matches_c_table() {
        assert_eq!(newuexp(0), 0);
        assert_eq!(newuexp(1), 20);
        assert_eq!(newuexp(9), 5_120);
        assert_eq!(newuexp(10), 10_000);
        assert_eq!(newuexp(19), 5_120_000);
        assert_eq!(newuexp(20), 10_000_000);
    }

    #[test]
    fn experience_for_a_newt_and_a_soldier_ant() {
        use nethacked_types::Attack;
        let bite = |ad, n, d| Attack {
            at: AttackType::Bite,
            ad,
            n,
            d,
        };
        // newt: level 0, AC 8, speed 6, bite 1d3 -> 1 + 0
        assert_eq!(
            experience(0, 8, 6, &[bite(DamageType::Phys, 1, 3)], false, false),
            1
        );
        // soldier ant: level 3, AC 3, speed 18, bite 2d4, sting 3d4 AD_DRST
        let sting = Attack {
            at: AttackType::Sting,
            ad: DamageType::DrainStr,
            n: 3,
            d: 4,
        };
        // 1+9, speed 18 > 12 -> +3, sting > AT_BUTT -> +3, DRST -> +2*3
        assert_eq!(
            experience(
                3,
                3,
                18,
                &[bite(DamageType::Phys, 2, 4), sting],
                false,
                false
            ),
            22
        );
    }

    #[test]
    fn attributes_start_from_the_c_role_tables() {
        for seed in 0..20 {
            let sim = SimulationWorld::new_with_seed(seed);
            let (role, race) = sim.stats();
            let a = sim.progress.attr_base;
            for i in 0..6 {
                assert!(
                    a[i] >= role.attr_base[i] && a[i] <= race.attr_max[i],
                    "{a:?}"
                );
            }
            assert_eq!(a.iter().sum::<i32>(), 75, "{a:?}");
        }
    }

    #[test]
    fn enough_experience_levels_up_and_losexp_takes_it_back() {
        let mut sim = SimulationWorld::new_with_seed(3);
        let pid = sim.player_id;
        let (hp0, lvl0) = (sim.arena.actors[pid].max_hp, sim.arena.actors[pid].level);
        let mut ev = Vec::new();
        sim.gain_experience(newuexp(lvl0), &mut ev);
        assert_eq!(sim.arena.actors[pid].level, lvl0 + 1);
        assert!(ev.iter().any(|e| matches!(e, GameEvent::LogMessage { text } if text == &format!("Welcome to experience level {}.", lvl0 + 1))));
        let gained = sim.arena.actors[pid].max_hp - hp0;
        assert!(gained >= 1);
        let mut ev = Vec::new();
        assert!(!sim.losexp(Some("life drainage"), &mut ev));
        assert_eq!(sim.arena.actors[pid].level, lvl0);
        assert!(ev.iter().any(|e| matches!(e, GameEvent::LogMessage { text } if text == &format!("Goodbye level {}.", lvl0 + 1))));
        assert_eq!(sim.arena.actors[pid].max_hp, hp0.max(lvl0.max(10)));
    }
}
