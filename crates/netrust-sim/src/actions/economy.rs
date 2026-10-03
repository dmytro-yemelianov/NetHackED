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

    pub fn handle_price_check(&mut self, item_index: usize) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let carried = self.arena.items_carried_by(self.player_id);
        if item_index >= carried.len() {
            events.push(GameEvent::LogMessage {
                text: "You don't have that item to appraise.".into(),
            });
            return events;
        }

        let has_shopkeeper = self.arena.actors.values().any(|a| a.name == "shopkeeper" && !a.is_dead);
        if !has_shopkeeper {
            events.push(GameEvent::LogMessage {
                text: "There is no shopkeeper here to appraise your goods.".into(),
            });
            return events;
        }

        let item_id = carried[item_index];
        if let Some(item) = self.arena.items.get(item_id) {
            let base_cost = netrust_data::items::ITEM_CATALOG
                .iter()
                .find(|it| it.name == item.name)
                .map(|it| it.cost)
                .unwrap_or(20);
            let cha = 12; // default adventurer charisma
            let buy = netrust_core::calculate_buy_price(base_cost, cha, item.buc);
            let sell = netrust_core::calculate_sell_price(base_cost, cha, item.buc);
            events.push(GameEvent::LogMessage {
                text: netrust_i18n::Messages::price_appraisal(&item.name, sell, buy, base_cost, self.locale),
            });
            self.scheduler.hero_act(NORMAL_SPEED);
        }

        events
    }
}

