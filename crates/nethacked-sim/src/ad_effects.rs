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
            // uhitm.c:2479-2488 -> losexp("life drainage"); Drain resistance
            // is not carried in Intrinsics yet.
            DamageType::DrainLife => {
                if self.rng.random_range(0..3u32) == 0 && !self.hero_negates(true, events) {
                    self.losexp(Some("life drainage"), events);
                }
                dmg
            }
            // uhitm.c:3143-3158 -> poisoned(buf, A_STR/A_DEX/A_CON, ..., 30, FALSE)
            DamageType::DrainStr | DamageType::DrainDex | DamageType::DrainCon => {
                let negated = self.hero_negates(false, events);
                if !negated && self.rng.random_range(0..8u32) == 0 {
                    let typ = match attack.ad {
                        DamageType::DrainDex => crate::progress::A_DEX,
                        DamageType::DrainCon => crate::progress::A_CON,
                        _ => crate::progress::A_STR,
                    };
                    let reason = format!(
                        "{}'s {}",
                        capitalize(&attacker.name),
                        poison_subject(attack.at)
                    );
                    self.poisoned(&reason, typ, events);
                }
                dmg
            }
            // uhitm.c:3431-3478 -> paralyze_monst/nomul(-rnd(10)); Free_action resists
            DamageType::Paralyze => {
                if self.rng.random_range(0..3u32) == 0 && !self.hero_negates(true, events) {
                    if intr.poison_resistance {
                        say(events, "You momentarily stiffen.");
                    } else {
                        if intr.blind {
                            say(events, "You are frozen!");
                        } else {
                            say(events, &format!("You are frozen by {}!", attacker.name));
                        }
                        self.hero.afflictions.transient.helpless = self.rng.random_range(1..=10u32);
                    }
                }
                dmg
            }
            // uhitm.c:2281-2337 -> erode_armor(ERODE_RUST); completelyrusts -> rehumanize
            DamageType::Rust => {
                if !self.hero_negates(false, events) {
                    self.erode_hero_armor(crate::progress::ErosionKind::Rust, events);
                }
                dmg
            }
            // uhitm.c:2338-2362 -> erode_armor(ERODE_CORRODE)
            DamageType::Corrode => {
                if !self.hero_negates(false, events) {
                    self.erode_hero_armor(crate::progress::ErosionKind::Corrode, events);
                }
                dmg
            }
            // uhitm.c:2363-2417 -> erode_armor(ERODE_ROT)
            DamageType::Decay => {
                if !self.hero_negates(false, events) {
                    self.erode_hero_armor(crate::progress::ErosionKind::Rot, events);
                }
                dmg
            }
            // uhitm.c:3897-3980 -> make_hallucinated(HHallucination + dmg); damage = 0
            DamageType::Hallucinate => {
                if !self.hero_negates(false, events) {
                    let t = &mut self.hero.afflictions.transient;
                    if t.hallucinating == 0 {
                        say(events, "You are freaking out.");
                    } else {
                        say(events, "You are getting even more confused.");
                    }
                    t.hallucinating = t.hallucinating.saturating_add(dmg);
                }
                0
            }
            // uhitm.c:3832-3836 -> diseasemu(pa) -> make_sick
            DamageType::Disease => {
                if self.rng.random_range(0..2u32) == 0 && !self.hero_negates(false, events) {
                    say(
                        events,
                        &format!("You feel fever and chills from {}!", attacker.name),
                    );
                    self.make_sick(dmg, events);
                }
                dmg
            }
            // uhitm.c:3306-3336 -> set_ustuck if !sticks && !negated
            DamageType::Sticky => {
                if !self.hero_negates(false, events) {
                    say(events, &format!("You stick to {}!", attacker.name));
                    // ustuck tracking not fully modelled; message only
                }
                dmg
            }
            // uhitm.c:3337-3430 -> set_ustuck, drowning if in pool
            DamageType::Wrap => {
                if !self.hero_negates(false, events) {
                    say(
                        events,
                        &format!("{} coils around you!", capitalize(&attacker.name)),
                    );
                    // ustuck tracking not fully modelled; message only
                }
                dmg
            }
            // uhitm.c:2790-2858 -> steal item, mhm->damage = 0
            DamageType::StealItem => {
                if !self.hero_negates(true, events) {
                    self.steal_hero_item(attacker, events);
                }
                0
            }
            // uhitm.c:2790-2858 -> steal gold
            DamageType::StealGold => {
                if !self.hero_negates(true, events) {
                    self.steal_hero_gold(attacker, events);
                }
                0
            }
            // uhitm.c:??? -> seduction steal (foocubus)
            DamageType::Seduce => {
                if !self.hero_negates(true, events) {
                    // Foocubus: steal item or gold, then teleport away
                    self.seduce_hero(attacker, events);
                }
                dmg
            }
            // uhitm.c:2859-2957 -> teleport hero, damage = 0
            DamageType::Teleport => {
                if !self.hero_negates(false, events) {
                    say(events, "Your position suddenly seems very uncertain!");
                    self.teleport_hero(events);
                }
                0
            }
            // uhitm.c:3603-3651 -> drain_item (erode armor/rings)
            DamageType::Disenchant => {
                if !self.hero_negates(false, events) {
                    self.disenchant_hero_item(events);
                }
                dmg
            }
            // uhitm.c:??? -> AD_LEGS (leprechaun legs?)
            DamageType::Legs => {
                // No hero-side effect modelled yet (leprechaun kick doesn't do special hero effect)
                dmg
            }
            // uhitm.c:3168-3305 -> losexp + attr drain (INT)
            DamageType::DrainInt => {
                if self.rng.random_range(0..3u32) == 0 && !self.hero_negates(true, events) {
                    self.losexp(Some("intelligence drain"), events);
                    // Also drain INT via poisoned
                    self.poisoned(
                        &format!("{}'s attack", capitalize(&attacker.name)),
                        crate::progress::A_INT,
                        events,
                    );
                }
                dmg
            }
            // uhitm.c:3729-3776 -> mon_poly (hero polymorph)
            DamageType::Polymorph => {
                if !self.hero_negates(false, events) {
                    say(events, "You feel a change coming over you.");
                    self.polymorph_hero(dmg, events);
                }
                0
            }
            // uhitm.c:4203-4264 -> do_stone_u (petrification)
            DamageType::Stone => {
                if !self.hero_negates(true, events)
                    && !intr.disintegration_resistance
                    && self.hero.afflictions.petrification.is_none()
                {
                    say(
                        events,
                        &format!("{} turns you to stone!", capitalize(&attacker.name)),
                    );
                    self.hero.afflictions.petrification =
                        Some(nethacked_types::PetrificationState { turns_remaining: 5 });
                }
                dmg
            }
            // uhitm.c:3526-3602 -> make_slimed
            DamageType::Slime => {
                if self.rng.random_range(0..4u32) == 0
                    && !self.hero_negates(false, events)
                    && self.hero.afflictions.sliming.is_none()
                {
                    say(events, "You don't feel very well.");
                    self.hero.afflictions.sliming = Some(nethacked_types::SlimingState {
                        turns_remaining: 10,
                    });
                }
                dmg
            }
            // uhitm.c:4265-4271 -> lycanthropy
            DamageType::Lycanthropy => {
                if self.rng.random_range(0..2u32) == 0 && !self.hero_negates(false, events) {
                    say(
                        events,
                        &format!("You feel feverish from {}'s bite!", attacker.name),
                    );
                    // Lycanthropy not fully modelled; message only
                }
                dmg
            }
            // uhitm.c:3015-3097 -> curse items (attrcurse)
            DamageType::Curse => {
                if self.rng.random_range(0..10u32) == 0 && !self.hero_negates(false, events) {
                    say(events, &format!("{} chuckles.", capitalize(&attacker.name)));
                    self.curse_hero_items(events);
                }
                dmg
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

/// C `mpoisons_subj()` (mhitu.c:145) for a monster without a poisoned weapon.
fn poison_subject(at: nethacked_types::AttackType) -> &'static str {
    use nethacked_types::AttackType as A;
    match at {
        A::Weapon => "attack",
        A::Touch => "contact",
        A::Gaze => "gaze",
        A::Bite => "bite",
        _ => "sting",
    }
}
