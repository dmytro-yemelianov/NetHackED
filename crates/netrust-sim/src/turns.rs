//! Turn scheduling ticks and passive metabolic hunger consumption.

use netrust_core::energy::{StepAction, NORMAL_SPEED};

use crate::events::GameEvent;
use crate::world::SimulationWorld;

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

                    // Passive metabolic consumption
                    let old_nut = self.player_nutrition;
                    self.player_nutrition = netrust_core::nutrition::metabolic_tick(self.player_nutrition);
                    if old_nut >= 150 && self.player_nutrition < 150 {
                        events.push(GameEvent::LogMessage { text: netrust_i18n::Messages::hunger_hungry(self.locale).into() });
                    } else if old_nut >= 50 && self.player_nutrition < 50 {
                        events.push(GameEvent::LogMessage { text: netrust_i18n::Messages::hunger_weak(self.locale).into() });
                    } else if self.player_nutrition == 0 && (self.scheduler.turn % 10 == 0) {
                        if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                            p.hp = p.hp.saturating_sub(1);
                            if p.hp == 0 {
                                p.is_dead = true;
                            }
                        }
                        events.push(GameEvent::LogMessage { text: netrust_i18n::Messages::hunger_fainting(self.locale).into() });
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
