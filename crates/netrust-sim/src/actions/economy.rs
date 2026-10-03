//! Shop economy and store payment handling.

use netrust_core::energy::NORMAL_SPEED;

use crate::events::GameEvent;
use crate::world::SimulationWorld;

impl SimulationWorld {
    pub(crate) fn handle_pay(&mut self) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let carried = self.arena.items_carried_by(self.player_id);
        let mut total_due = 0u32;
        let mut to_pay = Vec::new();

        for &item_id in &carried {
            if let Some(cost) = self.get_unpaid_cost(item_id) {
                total_due += cost;
                to_pay.push(item_id);
            }
        }

        if to_pay.is_empty() {
            events.push(GameEvent::LogMessage {
                text: "You have no unpaid items to pay for.".into(),
            });
        } else if self.player_gold < total_due {
            events.push(GameEvent::LogMessage {
                text: format!("You don't have enough gold! You owe {} zorkmids but only have {}.", total_due, self.player_gold),
            });
        } else {
            self.player_gold -= total_due;
            for id in to_pay {
                self.remove_unpaid(id);
            }
            events.push(GameEvent::LogMessage {
                text: format!("You pay the shopkeeper {} zorkmids. 'Thank you for your business!'", total_due),
            });
            self.scheduler.hero_act(NORMAL_SPEED);
        }

        events
    }
}
