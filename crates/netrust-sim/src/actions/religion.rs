//! Religion system: Altar prayer, reconciliation, divine wrath, and sacrifice.

use netrust_core::energy::NORMAL_SPEED;
use netrust_types::{Alignment, Tile};

use crate::events::GameEvent;
use crate::world::SimulationWorld;

impl SimulationWorld {
    pub(crate) fn handle_pray(&mut self) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let Some(player) = self.arena.actors.get(self.player_id).cloned() else {
            return events;
        };

        let p_coord = player.coord;
        let current_tile = self.level.get_tile(p_coord).clone();
        match current_tile {
            Tile::Altar { align } => {
                if player.alignment == align {
                    if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                        p.hp = p.max_hp;
                    }
                    events.push(GameEvent::LogMessage {
                        text: "You feel devoutly reconciled with your god! You are fully healed.".into(),
                    });
                } else if align == Alignment::Neutral {
                    if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                        p.hp = (p.hp + 8).min(p.max_hp);
                    }
                    events.push(GameEvent::LogMessage {
                        text: "You feel a soothing warmth envelop you. You recover health.".into(),
                    });
                } else {
                    // Wrath from opposing god!
                    let wrath_damage = 6u32;
                    if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                        p.hp = p.hp.saturating_sub(wrath_damage).max(1);
                    }
                    events.push(GameEvent::LogMessage {
                        text: "The altar shudders violently! A voice thunders: 'Infidel!' Divine lightning strikes you!".into(),
                    });
                }
            }
            _ => {
                events.push(GameEvent::LogMessage {
                    text: "You pray to the gods of the dungeon. A harmonious chime echoes in the distance.".into(),
                });
            }
        }
        self.scheduler.hero_act(NORMAL_SPEED);
        events
    }

    pub(crate) fn handle_sacrifice(&mut self, idx: usize) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let Some(player) = self.arena.actors.get(self.player_id).cloned() else {
            return events;
        };

        let p_coord = player.coord;
        let current_tile = self.level.get_tile(p_coord).clone();
        if let Tile::Altar { align } = current_tile {
            let carried = self.arena.items_carried_by(self.player_id);
            if idx < carried.len() {
                let item_id = carried[idx];
                let item_name = self.arena.items.get(item_id).map(|i| i.name.clone()).unwrap_or_default();
                self.arena.destroy_item(item_id);
                if self.wielded_item == Some(item_id) {
                    self.wielded_item = None;
                }

                if player.alignment != align {
                    // Convert altar to player alignment!
                    self.level.set_tile(p_coord, Tile::Altar { align: player.alignment });
                    events.push(GameEvent::LogMessage {
                        text: format!("You sacrifice the {}. An astral flash erupts and the altar converts to {:?}!", item_name, player.alignment),
                    });
                } else {
                    if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                        p.max_hp += 1;
                        p.hp += 1;
                    }
                    events.push(GameEvent::LogMessage {
                        text: format!("You sacrifice the {}. An aura of divine light envelops the altar. You feel favored by your god (+1 Max HP)!", item_name),
                    });
                }
                self.scheduler.hero_act(NORMAL_SPEED);
            } else {
                events.push(GameEvent::LogMessage {
                    text: "You don't have that item in your pack to sacrifice.".into(),
                });
            }
        } else {
            events.push(GameEvent::LogMessage {
                text: "There is no altar here to sacrifice upon.".into(),
            });
        }
        events
    }
}
