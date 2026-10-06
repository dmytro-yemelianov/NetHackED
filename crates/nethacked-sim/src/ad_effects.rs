//! Hero-side effects of a landed monster attack, one handler per C damage
//! type: the `mdef == &gy.youmonst` ("mhitu") branches of the
//! `mhitm_ad_*` functions in `uhitm.c`. Each handler may change the damage the
//! hit deals, like C's `mhm->damage`.
//!
//! Damage types without a handler here keep their dice damage (physical);
//! `docs/parity/behaviour-coverage.md` lists which ones are handled.

use nethacked_arena::ActorRecord;
use nethacked_types::{Attack, DamageType, ItemClass};
use rand::Rng;

use crate::events::GameEvent;
use crate::world::SimulationWorld;

impl SimulationWorld {
    /// C `magic_negation()` (mhitu.c:1089) for the hero: the best `a_can` of
    /// worn armor. Carried armor counts as worn here (see `compute_hero_ac`).
    fn hero_magic_cancellation(&self) -> u32 {
        self.arena
            .items_carried_by(self.player_id)
            .into_iter()
            .filter_map(|id| self.arena.items.get(id))
            .filter(|it| it.class == ItemClass::Armor)
            .filter_map(|it| self.ruleset.item(&it.name)?.armor.as_ref())
            .map(|a| a.magic_cancellation)
            .max()
            .unwrap_or(0)
    }

    /// C `mhitm_mgc_atk_negated()` (uhitm.c:75-100), hero defender:
    /// `negated = !(rn2(10) >= 3 * armpro)`. Monster cancellation (`mcan`) is
    /// not modelled.
    fn hero_negates(&mut self, verbosely: bool, events: &mut Vec<GameEvent>) -> bool {
        let armpro = self.hero_magic_cancellation();
        let negated = self.rng.random_range(0..10u32) < 3 * armpro;
        if negated && verbosely {
            say(events, "You avoid harm.");
        }
        negated
    }

    /// Apply the C `mhitm_ad_*` hero branch for `attack` and return the damage
    /// the hit deals afterwards.
    pub(crate) fn hero_ad_effect(
        &mut self,
        attacker: &ActorRecord,
        attacker_id: nethacked_arena::ActorId,
        attack: &Attack,
        dmg: u32,
        events: &mut Vec<GameEvent>,
    ) -> u32 {
        let Some(hero) = self.arena.actors.get(self.player_id) else {
            return dmg;
        };
        let intr = hero.intrinsics;
        match attack.ad {
            // uhitm.c:2706-2721
            DamageType::Elec => {
                if self.hero_negates(true, events) {
                    return 0;
                }
                say(events, "You get zapped!");
                if intr.shock_resistance {
                    say(events, "The zap doesn't shock you!");
                    return 0;
                }
                dmg
            }
            // uhitm.c:2750-2765
            DamageType::Acid => {
                if self.rng.random_range(0..3u32) != 0 {
                    return 0;
                }
                if intr.acid_resistance {
                    say(events, "You're covered in acid, but it seems harmless.");
                    return 0;
                }
                say(events, "You're covered in acid!  It burns!");
                dmg
            }
            // uhitm.c:2976-2985: can_blnd() is approximated by "not blind yet or
            // already blind" (no eye protection is modelled).
            DamageType::Blind => {
                let t = &mut self.hero.afflictions.transient;
                if t.blinded == 0 && !intr.blind {
                    say(
                        events,
                        &format!("{} blinds you!", capitalize(&attacker.name)),
                    );
                }
                t.blinded = t.blinded.saturating_add(dmg);
                if let Some(h) = self.arena.actors.get_mut(self.player_id) {
                    h.intrinsics.blind = true;
                }
                0
            }
            // uhitm.c:4403-4409
            DamageType::Stun => {
                if self.rng.random_range(0..4u32) != 0 {
                    return dmg;
                }
                let t = &mut self.hero.afflictions.transient;
                if t.stunned == 0 {
                    say(events, "You stagger...");
                }
                t.stunned = t.stunned.saturating_add(dmg);
                dmg / 2
            }
            // uhitm.c:3701-3712
            DamageType::Confuse => {
                if self.rng.random_range(0..4u32) == 0 && attacker.mspec_used == 0 {
                    let extra = self.rng.random_range(0..6u32);
                    if let Some(m) = self.arena.actors.get_mut(attacker_id) {
                        m.mspec_used = m
                            .mspec_used
                            .saturating_add(u8::try_from(dmg + extra).unwrap_or(u8::MAX));
                    }
                    let t = &mut self.hero.afflictions.transient;
                    say(
                        events,
                        if t.confused > 0 {
                            "You are getting even more confused."
                        } else {
                            "You are getting confused."
                        },
                    );
                    t.confused = t.confused.saturating_add(dmg);
                }
                0
            }
            // uhitm.c:3492-3507: fall_asleep(-rnd(10)).
            DamageType::Sleep => {
                if self.hero.afflictions.transient.helpless == 0
                    && self.rng.random_range(0..5u32) == 0
                    && !self.hero_negates(true, events)
                    && !intr.sleep_resistance
                {
                    let turns = self.rng.random_range(1..=10u32);
                    self.hero.afflictions.transient.helpless = turns;
                    say(
                        events,
                        &format!("You are put to sleep by {}!", attacker.name),
                    );
                }
                dmg
            }
            // uhitm.c:3671-3675: u_slow_down() (timeout.c) clears intrinsic speed.
            DamageType::Slow => {
                let negated = self.hero_negates(false, events);
                if !negated && intr.fast && self.rng.random_range(0..4u32) == 0 {
                    if let Some(h) = self.arena.actors.get_mut(self.player_id) {
                        h.intrinsics.fast = false;
                    }
                    say(events, "You slow down.");
                }
                dmg
            }
            // uhitm.c:2429-2434 -> drain_en() (trap.c:5200-5240)
            DamageType::DrainEnergy => {
                let negated = self.hero_negates(false, events);
                if !negated && self.rng.random_range(0..4u32) == 0 {
                    self.drain_en(dmg, events);
                }
                0
            }
            _ => dmg,
        }
    }

    /// C `drain_en(n, FALSE)` (trap.c:5200).
    fn drain_en(&mut self, mut n: u32, events: &mut Vec<GameEvent>) {
        if self.player_max_pw < 1 {
            self.player_pw = 0;
            self.player_max_pw = 0;
            say(events, "You feel momentarily lethargic.");
            return;
        }
        if n > (self.player_pw + self.player_max_pw) / 3 {
            n = self.rng.random_range(1..=n.max(1));
        }
        let punct = if n > self.player_pw { '!' } else { '.' };
        say(
            events,
            &format!("You feel your magical energy drain away{punct}"),
        );
        if n > self.player_pw {
            let over = n - self.player_pw;
            let loss = self.rng.random_range(1..=over);
            self.player_max_pw = self.player_max_pw.saturating_sub(loss);
            self.player_pw = 0;
        } else {
            self.player_pw -= n;
            self.player_pw = self.player_pw.min(self.player_max_pw);
        }
    }

    /// C `nh_timeout()` (timeout.c:730-750) for the hero's transient
    /// afflictions, once per turn: counters run down and announce their end.
    pub(crate) fn tick_hero_afflictions(&mut self, events: &mut Vec<GameEvent>) {
        let t = &mut self.hero.afflictions.transient;
        let mut msgs: Vec<&str> = Vec::new();
        if t.stunned > 0 {
            t.stunned -= 1;
            if t.stunned == 0 {
                msgs.push("You feel a bit steadier now.");
            }
        }
        if t.confused > 0 {
            t.confused -= 1;
            if t.confused == 0 {
                msgs.push("You feel less confused now.");
            }
        }
        let mut cleared_blind = false;
        if t.blinded > 0 {
            t.blinded -= 1;
            if t.blinded == 0 {
                cleared_blind = true;
                msgs.push("You can see again.");
            }
        }
        if t.helpless > 0 {
            t.helpless -= 1;
            if t.helpless == 0 {
                // hack.c:973 fall_asleep wakeup message
                msgs.push("You wake up.");
            }
        }
        if cleared_blind {
            if let Some(h) = self.arena.actors.get_mut(self.player_id) {
                h.intrinsics.blind = false;
            }
        }
        for m in msgs {
            say(events, m);
        }
    }

    /// C `u_maybe_impaired()` (hack.c:2420): stunned, or confused and `!rn2(5)`.
    pub(crate) fn hero_moves_at_random(&mut self) -> bool {
        let t = &self.hero.afflictions.transient;
        let (stunned, confused) = (t.stunned > 0, t.confused > 0);
        stunned || (confused && self.rng.random_range(0..5u32) == 0)
    }
}

fn say(events: &mut Vec<GameEvent>, text: &str) {
    events.push(GameEvent::LogMessage {
        text: text.to_string(),
    });
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}
