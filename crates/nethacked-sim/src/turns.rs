//! Turn scheduling ticks and passive metabolic hunger consumption.

use nethacked_core::energy::{StepAction, NORMAL_SPEED};

use crate::events::GameEvent;
use crate::world::SimulationWorld;
use nethacked_core::HungerState;

impl SimulationWorld {
    pub(crate) fn process_turn_ticks(&mut self) -> Vec<GameEvent> {
        let mut events = Vec::new();

        while !self.scheduler.hero_can_act() {
            match self.scheduler.step() {
                StepAction::HeroStep => break,
                StepAction::MonsterStep => {
                    events.extend(self.step_monsters());
                }
                StepAction::TurnTick => {
                    events.push(GameEvent::TurnAdvanced {
                        turn: self.scheduler.turn,
                    });

                    self.tick_hero_afflictions(&mut events);

                    // C mcalcdistress -> m_calcdistress -> mon_regen (mon.c:1193,
                    // monmove.c:311): once per turn, `if (mspec_used) mspec_used--`.
                    for (_, m) in self.arena.actors.iter_mut() {
                        if !m.is_player && !m.is_dead {
                            m.mspec_used = m.mspec_used.saturating_sub(1);
                        }
                    }

                    match nethacked_core::afflictions::tick_afflictions(&mut self.hero) {
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
                                text: nethacked_i18n::Messages::sliming_death(self.locale)
                                    .to_string(),
                            });
                        }
                        _ => {}
                    }

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
                    } else if new_state == HungerState::Fainting && (self.scheduler.turn % 10 == 0)
                    {
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

                    if self.scheduler.monster_can_act() {
                        self.scheduler.monster_act(NORMAL_SPEED);
                        events.extend(self.step_monsters());
                    }
                }
            }
        }

        events
    }
}
