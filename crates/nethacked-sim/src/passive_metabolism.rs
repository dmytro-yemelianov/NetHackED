//! The current Rust raw metabolic block; no turn orchestration or regeneration.
use crate::{world::DEFAULT_PLAYER_CON, GameEvent};
use nethacked_core::nutrition::{hunger_of_nutrition, metabolic_tick, HungerState};
use nethacked_types::Locale;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Actor {
    pub hp: u32,
    pub dead: bool,
}
pub(crate) struct Outcome {
    pub nutrition: i32,
    pub actor: Option<Actor>,
    pub events: Vec<GameEvent>,
}
pub(crate) fn tick(nutrition: i32, mut actor: Option<Actor>, turn: u64, locale: Locale) -> Outcome {
    let old = hunger_of_nutrition(nutrition, DEFAULT_PLAYER_CON);
    let nutrition = metabolic_tick(nutrition);
    let new = hunger_of_nutrition(nutrition, DEFAULT_PLAYER_CON);
    let mut events = Vec::new();
    let mut say = |text: &str| events.push(GameEvent::LogMessage { text: text.into() });
    if old != new && new == HungerState::Hungry {
        say(nethacked_i18n::Messages::hunger_hungry(locale));
    } else if old != new && new == HungerState::Weak {
        say(nethacked_i18n::Messages::hunger_weak(locale));
    } else if new == HungerState::Fainting && turn % 10 == 0 {
        let mut died = false;
        if let Some(p) = actor.as_mut() {
            if !p.dead {
                p.hp = p.hp.saturating_sub(1);
                if p.hp == 0 {
                    p.dead = true;
                    died = true;
                }
            }
        }
        say(nethacked_i18n::Messages::hunger_fainting(locale));
        if died {
            say(nethacked_i18n::t("fainted_death", locale));
        }
    } else if new == HungerState::Starved {
        if let Some(p) = actor.as_mut() {
            if !p.dead {
                p.hp = 0;
                p.dead = true;
                say(nethacked_i18n::t("starved", locale));
            }
        }
    }
    Outcome {
        nutrition,
        actor,
        events,
    }
}
