//! Shop economy and store payment handling.

use netrust_core::energy::NORMAL_SPEED;

use crate::events::GameEvent;
use crate::world::SimulationWorld;

/// Hero charisma used for shop prices. The simulation has no attribute model, so a single
/// documented default stands in for `ACURR(A_CHA)` (C starting CHA is role-dependent and
/// rolled; 10 is a typical value and falls in the `8-10` band, buy x4/3, `shk.c:2963`).
pub const DEFAULT_CHARISMA: i32 = 10;

impl SimulationWorld {
    /// C `shk.c:2947-2951` dunce/tourist surcharge for the hero. Worn slots are not
    /// modelled, so a carried "dunce cap" stands in for a worn one (as carried armor does
    /// for `some_armor`); shirts are not in the item catalog, so the visible-shirt case is
    /// never true.
    pub(crate) fn hero_dunce_or_tourist(&self) -> bool {
        let dunce_cap = self
            .arena
            .items_carried_by(self.player_id)
            .into_iter()
            .any(|id| {
                self.arena
                    .items
                    .get(id)
                    .is_some_and(|it| it.name.eq_ignore_ascii_case("dunce cap"))
            });
        let ulevel = self
            .arena
            .actors
            .get(self.player_id)
            .map(|p| p.level)
            .unwrap_or(1);
        netrust_core::dunce_or_tourist_surcharge(
            dunce_cap,
            self.role_name == "Tourist",
            ulevel,
            false,
        )
    }

    /// Price the shopkeeper charges for an item of base cost `base` (C `shk.c:2877`
    /// `get_cost`). No identification model exists, so the unidentified `o_id` surcharge,
    /// artifact pricing and angry-shopkeeper surcharge are not applied.
    pub(crate) fn shop_buy_price(&self, base: u32) -> u32 {
        netrust_core::buy_price(
            base,
            DEFAULT_CHARISMA,
            self.hero_dunce_or_tourist(),
            false,
            false,
            false,
        )
    }

    /// Price the shopkeeper offers for an item of base cost `base` (C `shk.c:3148`
    /// `set_cost`); no identification model, so the lowball offer never applies.
    pub(crate) fn shop_sell_price(&self, base: u32) -> u32 {
        netrust_core::sell_price(base, self.hero_dunce_or_tourist(), false)
    }

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
                text: format!(
                    "You don't have enough gold! You owe {} zorkmids but only have {}.",
                    total_due, self.player_gold
                ),
            });
        } else {
            self.player_gold -= total_due;
            for id in to_pay {
                self.remove_unpaid(id);
            }
            events.push(GameEvent::LogMessage {
                text: format!(
                    "You pay the shopkeeper {total_due} zorkmids. 'Thank you for your business!'"
                ),
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

        let has_shopkeeper = self
            .arena
            .actors
            .values()
            .any(|a| a.name == "shopkeeper" && !a.is_dead);
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
            let buy = self.shop_buy_price(base_cost);
            let sell = self.shop_sell_price(base_cost);
            events.push(GameEvent::LogMessage {
                text: netrust_i18n::Messages::price_appraisal(
                    &item.name,
                    sell,
                    buy,
                    base_cost,
                    self.locale,
                ),
            });
            self.scheduler.hero_act(NORMAL_SPEED);
        }

        events
    }
}
