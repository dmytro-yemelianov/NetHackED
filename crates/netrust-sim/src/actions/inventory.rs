//! Inventory manipulation: PickUp, Drop, Wield, Container Put & Take.

use netrust_arena::ItemLocation;
use netrust_core::energy::NORMAL_SPEED;
use netrust_types::Coord;

use crate::events::GameEvent;
use crate::world::SimulationWorld;

impl SimulationWorld {
    pub(crate) fn handle_pickup(&mut self) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let Some(player) = self.arena.actors.get(self.player_id).cloned() else {
            return events;
        };

        let floor_items = self.arena.items_at_floor(player.coord);
        let chosen_item = floor_items.iter().copied().max_by_key(|&iid| {
            if let Some(item) = self.arena.items.get(iid) {
                if item.name.contains("Amulet")
                    || item.name.starts_with("The ")
                    || item.name.contains("Orb")
                {
                    100
                } else if item.name == "corpse" {
                    1
                } else {
                    10
                }
            } else {
                0
            }
        });

        if let Some(item_id) = chosen_item {
            if let Some(item) = self.arena.items.get_mut(item_id) {
                item.location = ItemLocation::CarriedBy(self.player_id);
                let name = item.name.clone();
                events.push(GameEvent::ItemPickedUp {
                    actor: self.player_id,
                    item: item_id,
                });
                events.push(GameEvent::LogMessage {
                    text: netrust_i18n::Messages::pickup_item(&name, self.locale),
                });

                let quest_cfg = netrust_core::get_role_quest_config(&self.role_name);
                if name.eq_ignore_ascii_case(quest_cfg.artifact_name) {
                    netrust_core::pick_up_quest_artifact(&mut self.quest_state);
                    events.push(GameEvent::LogMessage {
                        text: format!("A surge of celestial energy surges through your veins as you take hold of {name}!"),
                    });
                }

                if let Some(cost) = self.get_unpaid_cost(item_id) {
                    events.push(GameEvent::LogMessage {
                        text: format!("The shopkeeper says: 'That will be {cost} zorkmids.'"),
                    });
                }
                self.scheduler.hero_act(NORMAL_SPEED);
            }
        } else {
            events.push(GameEvent::LogMessage {
                text: netrust_i18n::Messages::nothing_to_pickup(self.locale).into(),
            });
        }

        events
    }

    pub(crate) fn handle_drop(&mut self, idx: usize) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let Some(player) = self.arena.actors.get(self.player_id).cloned() else {
            return events;
        };

        let carried = self.arena.items_carried_by(self.player_id);
        if idx < carried.len() {
            let item_id = carried[idx];
            if let Some(item) = self.arena.items.get_mut(item_id) {
                item.location = ItemLocation::Floor(player.coord);
                let name = item.name.clone();
                if self.wielded_item == Some(item_id) {
                    self.wielded_item = None;
                }
                events.push(GameEvent::ItemDropped {
                    actor: self.player_id,
                    item: item_id,
                });
                events.push(GameEvent::LogMessage {
                    text: format!("You drop the {name}."),
                });
                self.scheduler.hero_act(NORMAL_SPEED);
            }
        } else {
            events.push(GameEvent::LogMessage {
                text: "You don't have that item in your pack.".into(),
            });
        }

        events
    }

    pub(crate) fn handle_wield(&mut self, idx: usize) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let carried = self.arena.items_carried_by(self.player_id);
        if idx < carried.len() {
            let item_id = carried[idx];
            self.wielded_item = Some(item_id);
            let name = self
                .arena
                .items
                .get(item_id)
                .map(|i| i.name.clone())
                .unwrap_or_default();
            events.push(GameEvent::ItemWielded {
                actor: self.player_id,
                item: item_id,
            });
            events.push(GameEvent::LogMessage {
                text: format!("You wield the {name}."),
            });
            self.scheduler.hero_act(NORMAL_SPEED);
        } else {
            events.push(GameEvent::LogMessage {
                text: "You don't have that item to wield.".into(),
            });
        }

        events
    }

    pub(crate) fn handle_put_in_container(
        &mut self,
        item_index: usize,
        container_index: usize,
    ) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let carried = self.arena.items_carried_by(self.player_id);
        if item_index < carried.len() && container_index < carried.len() {
            let item_id = carried[item_index];
            let container_id = carried[container_index];
            if item_id == container_id {
                events.push(GameEvent::LogMessage {
                    text: "You cannot put something inside itself!".into(),
                });
            } else {
                let item = self.arena.items.get(item_id).unwrap().clone();
                let container = self.arena.items.get(container_id).unwrap().clone();
                if !container.is_container {
                    events.push(GameEvent::LogMessage {
                        text: format!("The {} is not a container.", container.name),
                    });
                } else if !netrust_core::inventory::can_insert_safe_flags(
                    item.is_bag_of_holding,
                    container.is_container,
                    container.is_bag_of_holding,
                ) {
                    // Magical container explosion!
                    events.push(GameEvent::LogMessage {
                        text: "The magical energies rupture the fabric of space! The bag explodes with a blinding flash!".into(),
                    });
                    if self.wielded_item == Some(item_id) || self.wielded_item == Some(container_id)
                    {
                        self.wielded_item = None;
                    }
                    self.arena.destroy_item(item_id);
                    self.arena.destroy_item(container_id);
                    if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                        p.hp = p.hp.saturating_sub(15);
                        if p.hp == 0 {
                            p.is_dead = true;
                        }
                    }
                    self.scheduler.hero_act(NORMAL_SPEED);
                } else {
                    if let Some(i) = self.arena.items.get_mut(item_id) {
                        i.location = ItemLocation::InContainer(container_id);
                    }
                    if self.wielded_item == Some(item_id) {
                        self.wielded_item = None;
                    }
                    events.push(GameEvent::LogMessage {
                        text: format!("You put the {} into the {}.", item.name, container.name),
                    });
                    self.scheduler.hero_act(NORMAL_SPEED);
                }
            }
        } else {
            events.push(GameEvent::LogMessage {
                text: "Invalid item index for container action.".into(),
            });
        }

        events
    }

    pub(crate) fn handle_take_from_container(
        &mut self,
        container_index: usize,
        item_index: usize,
    ) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let carried = self.arena.items_carried_by(self.player_id);
        if container_index < carried.len() {
            let container_id = carried[container_index];
            let in_container = self.arena.items_in_container(container_id);
            if item_index < in_container.len() {
                let item_id = in_container[item_index];
                let item_name = self
                    .arena
                    .items
                    .get(item_id)
                    .map(|i| i.name.clone())
                    .unwrap_or_default();
                let container_name = self
                    .arena
                    .items
                    .get(container_id)
                    .map(|c| c.name.clone())
                    .unwrap_or_default();
                if let Some(i) = self.arena.items.get_mut(item_id) {
                    i.location = ItemLocation::CarriedBy(self.player_id);
                }
                events.push(GameEvent::LogMessage {
                    text: format!("You take the {item_name} out of the {container_name}."),
                });
                self.scheduler.hero_act(NORMAL_SPEED);
            } else {
                events.push(GameEvent::LogMessage {
                    text: "There is no such item in that container.".into(),
                });
            }
        } else {
            events.push(GameEvent::LogMessage {
                text: "Invalid container index.".into(),
            });
        }

        events
    }

    pub(crate) fn handle_apply(&mut self, idx: usize) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let carried = self.arena.items_carried_by(self.player_id);
        if idx < carried.len() {
            let item_id = carried[idx];
            let (item_name, _enchantment) = if let Some(it) = self.arena.items.get(item_id) {
                (it.name.clone(), it.enchantment)
            } else {
                return events;
            };

            let name = item_name.to_lowercase();
            let player_coord = self
                .arena
                .actors
                .get(self.player_id)
                .map(|p| p.coord)
                .unwrap_or(Coord::new_unchecked(0, 0));
            let on_vs = self.vibrating_square == Some(player_coord);

            if name.contains("bell of opening") {
                self.ritual_progress = netrust_core::step_ritual(
                    self.ritual_progress,
                    netrust_core::InvocationStep::RingBell,
                    on_vs,
                    &self.candelabrum_state,
                );
                events.push(GameEvent::LogMessage {
                    text: "You ring the Bell of Opening. It produces an otherworldly, reverberating silver chime.".into(),
                });
                self.scheduler.hero_act(NORMAL_SPEED);
            } else if name.contains("candelabrum") {
                if self.candelabrum_state.light() {
                    self.ritual_progress = netrust_core::step_ritual(
                        self.ritual_progress,
                        netrust_core::InvocationStep::LightCandelabrum,
                        on_vs,
                        &self.candelabrum_state,
                    );
                    events.push(GameEvent::LogMessage {
                        text: "The seven candles on the Candelabrum of Invocation blaze with holy incandescent flame!".into(),
                    });
                } else {
                    events.push(GameEvent::LogMessage {
                        text: format!("The Candelabrum of Invocation is not ready. It needs 7 candles (currently has {}).", self.candelabrum_state.candle_count),
                    });
                }
                self.scheduler.hero_act(NORMAL_SPEED);
            } else if name.contains("candle") {
                let has_candelabrum = carried.iter().any(|&cid| {
                    self.arena
                        .items
                        .get(cid)
                        .map(|it| it.name.contains("Candelabrum"))
                        .unwrap_or(false)
                });
                if has_candelabrum && self.candelabrum_state.attach_candle() {
                    self.arena.destroy_item(item_id);
                    events.push(GameEvent::LogMessage {
                        text: format!("You attach the candle to the Candelabrum of Invocation. ({}/7 candles attached)", self.candelabrum_state.candle_count),
                    });
                    self.scheduler.hero_act(NORMAL_SPEED);
                } else {
                    // Regular candle lighting toggle
                    if let Some(item) = self.arena.items.get_mut(item_id) {
                        if item.enchantment <= 0 {
                            item.enchantment = 1;
                            events.push(GameEvent::LogMessage {
                                text: format!(
                                    "You light the {}. It casts a bright illumination.",
                                    item.name
                                ),
                            });
                        } else {
                            item.enchantment = 0;
                            events.push(GameEvent::LogMessage {
                                text: format!("You extinguish the {}.", item.name),
                            });
                        }
                    }
                    self.scheduler.hero_act(NORMAL_SPEED);
                }
            } else if name.contains("lamp") || name.contains("lantern") {
                if let Some(item) = self.arena.items.get_mut(item_id) {
                    if item.enchantment <= 0 {
                        item.enchantment = 1;
                        events.push(GameEvent::LogMessage {
                            text: format!(
                                "You light the {}. It casts a bright illumination.",
                                item.name
                            ),
                        });
                    } else {
                        item.enchantment = 0;
                        events.push(GameEvent::LogMessage {
                            text: format!("You extinguish the {}.", item.name),
                        });
                    }
                }
                self.scheduler.hero_act(NORMAL_SPEED);
            } else {
                events.push(GameEvent::LogMessage {
                    text: format!("You don't know how to apply the {item_name}."),
                });
            }
        } else {
            events.push(GameEvent::LogMessage {
                text: "You don't have that item in your pack.".into(),
            });
        }
        events
    }
}
