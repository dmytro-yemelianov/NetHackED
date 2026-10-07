//! Turn scheduling ticks and passive metabolic hunger consumption.

use nethacked_core::energy::{hero_movement_gain, monster_movement_gain, NORMAL_SPEED};

use crate::events::GameEvent;
use crate::world::SimulationWorld;
use nethacked_core::HungerState;
use rand::Rng;

impl SimulationWorld {
    pub(crate) fn process_turn_ticks(&mut self, spent_time: bool, moved: bool) -> Vec<GameEvent> {
        let mut events = Vec::new();

        if !spent_time {
            return events;
        }
        loop {
            if self
                .arena
                .actors
                .get(self.player_id)
                .is_none_or(|hero| hero.is_dead || hero.hp == 0)
            {
                break;
            }
            // C always takes one snapshot pass after a time-spent hero action.
            events.extend(self.step_monsters());
            if self
                .arena
                .actors
                .get(self.player_id)
                .is_none_or(|hero| hero.is_dead || hero.hp == 0)
                || self.scheduler.hero_can_act()
            {
                break;
            }
            if self.arena.actors.iter().any(|(_, actor)| {
                !actor.is_player && !actor.is_dead && actor.hp > 0 && actor.movement >= NORMAL_SPEED
            }) {
                continue;
            }
            // Once-round distress precedes allocation, never once per action.
            for (_, actor) in self.arena.actors.iter_mut() {
                if !actor.is_player && !actor.is_dead && actor.hp > 0 {
                    actor.mspec_used = actor.mspec_used.saturating_sub(1);
                }
            }
            for (_, actor) in self.arena.actors.iter_mut() {
                if !actor.is_player && !actor.is_dead && actor.hp > 0 {
                    let gain =
                        monster_movement_gain(actor.speed, |bound| self.rng.random_range(0..bound));
                    actor.movement = actor.movement.saturating_add(gain);
                }
            }
            let burden = self.hero_encumbrance();
            let properties = self.arena.actors[self.player_id].intrinsics;
            let gain = hero_movement_gain(
                self.scheduler.hero_speed,
                properties.fast,
                properties.very_fast,
                burden,
                |bound| self.rng.random_range(0..bound),
            );
            self.scheduler.hero_energy = self.scheduler.hero_energy.saturating_add(gain);
            self.scheduler.turn += 1;
            self.arena.advance_corpse_ages();
            // Cached records retain original handles for ancestry lookup.
            for (_, stored) in &mut self.stored_levels {
                let advancing: Vec<_> = stored
                    .items
                    .iter()
                    .enumerate()
                    .filter_map(|(index, (_, item))| {
                        (item.is_corpse()
                            && !nethacked_arena::frozen_in_ice_box(item, |parent| {
                                stored
                                    .items
                                    .iter()
                                    .find(|(id, _)| *id == parent)
                                    .map(|(_, item)| item)
                            }))
                        .then_some(index)
                    })
                    .collect();
                for index in advancing {
                    stored.items[index].1.corpse_age =
                        stored.items[index].1.corpse_age.saturating_add(1);
                }
            }
            events.push(GameEvent::TurnAdvanced {
                turn: self.scheduler.turn,
            });

            let affliction_result = self.tick_hero_afflictions(&mut events);

            match affliction_result {
                nethacked_core::afflictions::AfflictionTickResult::StoneDeath => {
                    if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                        p.is_dead = true;
                    }
                    events.push(GameEvent::LogMessage {
                        text: nethacked_i18n::Messages::petrification_death(self.locale)
                            .to_string(),
                    });
                }
                nethacked_core::afflictions::AfflictionTickResult::SlimeDeath => {
                    if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                        p.is_dead = true;
                    }
                    events.push(GameEvent::LogMessage {
                        text: nethacked_i18n::Messages::sliming_death(self.locale).to_string(),
                    });
                }
                _ => {}
            }

            // C allmain.c: nh_timeout precedes regen_hp, which precedes
            // gethungry. Fatal afflictions above must never regenerate.
            // timeout.c:947-948 drains object timers after afflictions.
            events.extend(self.run_object_timers());
            self.regenerate_natural_hero_hp(moved);

            // Passive metabolic consumption
            let old_state = self.hunger_state();
            self.player_nutrition =
                nethacked_core::nutrition::metabolic_tick(self.player_nutrition);
            let new_state = self.hunger_state();
            if old_state != new_state && new_state == HungerState::Hungry {
                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::hunger_hungry(self.locale).into(),
                });
            } else if old_state != new_state && new_state == HungerState::Weak {
                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::hunger_weak(self.locale).into(),
                });
            } else if new_state == HungerState::Fainting && (self.scheduler.turn % 10 == 0) {
                // NetHackED approximation: C faints (loses turns, `eat.c` newuhs);
                // here the hero loses 1 HP and dies at 0 HP.
                let mut died = false;
                if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                    if !p.is_dead {
                        p.hp = p.hp.saturating_sub(1);
                        if p.hp == 0 {
                            p.is_dead = true;
                            died = true;
                        }
                    }
                }
                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::hunger_fainting(self.locale).into(),
                });
                if died {
                    events.push(GameEvent::LogMessage {
                        text: nethacked_i18n::t("fainted_death", self.locale).into(),
                    });
                }
            } else if new_state == HungerState::Starved {
                // Kill and log once, on the transition only.
                if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                    if !p.is_dead {
                        p.hp = 0;
                        p.is_dead = true;
                        events.push(GameEvent::LogMessage {
                            text: nethacked_i18n::t("starved", self.locale).into(),
                        });
                    }
                }
            }

            // C allmain.c:379-387: negative multi advances after
            // timeout, regeneration and hunger, once per world turn.
            self.tick_dressing(&mut events);

            // Replenishment returns directly to a ready hero (no extra pass).
            if self.scheduler.hero_can_act() {
                break;
            }
        }

        events
    }

    /// NetHack-5.0.0_Released allmain.c:625-679, ordinary-form natural branch.
    /// ActorRecord remains the HP owner. Polymorph, Pw, Regeneration properties,
    /// Sleepy sleeping bonus, prayer invulnerability, effective equipment CON
    /// and voluntary multi-turn interruption are separate contexts not
    /// represented by this path.
    fn regenerate_natural_hero_hp(&mut self, moved: bool) {
        let Some(hero) = self.arena.actors.get(self.player_id) else {
            return;
        };
        if hero.is_dead || hero.hp == 0 || hero.hp >= hero.max_hp || self.hero.polymorph.is_some() {
            return;
        }
        // C encumbrance_ok = wtcap < MOD_ENCUMBER || !u.umoved.
        if moved && self.hero_encumbrance() >= nethacked_core::inventory::EncumbranceTier::Stressed
        {
            return;
        }
        let level = hero.level;
        let con = self.hero_attr(crate::progress::A_CON);
        let heal = natural_hp_heal(level, con, |bound| self.rng.random_range(0..bound));
        let hero = self.arena.actors.get_mut(self.player_id).unwrap();
        hero.hp = hero.hp.saturating_add(heal).min(hero.max_hp);
    }
}

/// C `heal = (u.ulevel + ACURR(A_CON)) > rn2(100)`; the natural path
/// always draws once when eligible, even when the sum guarantees healing.
fn natural_hp_heal(level: u32, con: i32, mut rn2: impl FnMut(u32) -> u32) -> u32 {
    u32::from(i64::from(level) + i64::from(con) > i64::from(rn2(100)))
}

#[cfg(test)]
mod natural_hp_tests {
    use super::natural_hp_heal;

    #[test]
    fn released_c_prescribed_draw_boundaries() {
        // Literal source-derived expectations, including equality (strict >),
        // minimum/maximum draw, and level/CON thresholds independently.
        for (level, con, draw, heal) in [
            (1, 3, 0, 1),
            (1, 3, 3, 1),
            (1, 3, 4, 0),
            (1, 3, 99, 0),
            (9, 12, 20, 1),
            (9, 12, 21, 0),
            (10, 12, 21, 1),
            (10, 12, 22, 0),
            (10, 13, 22, 1),
            (10, 13, 23, 0),
            (30, 25, 54, 1),
            (30, 25, 55, 0),
            (75, 25, 99, 1),
        ] {
            let mut draws = Vec::new();
            assert_eq!(
                natural_hp_heal(level, con, |bound| {
                    draws.push(bound);
                    draw
                }),
                heal
            );
            assert_eq!(draws, [100]);
        }
    }
}
