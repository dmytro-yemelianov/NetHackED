use netrust_arena::{ItemLocation, ItemRecord};
use netrust_core::energy::NORMAL_SPEED;
use netrust_core::religion::{clamp_favor, consecrate_water, resolve_sacrifice};
use netrust_types::{Alignment, Buc, ItemClass, SacrificeResult, Tile};

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

        // 1. Prayer timeout check: Did the player pray too soon?
        if self.divine_state.prayer_timeout > 0 {
            self.divine_state.favor = clamp_favor(self.divine_state.favor - 3);
            let smite_damage = 8u32;
            if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                p.hp = p.hp.saturating_sub(smite_damage).max(1);
            }
            events.push(GameEvent::LogMessage {
                text: "You pray too soon! A cloud of brimstone appears, and you are struck by celestial lightning!".into(),
            });
            self.scheduler.hero_act(NORMAL_SPEED);
            return events;
        }

        // Safe prayer sets cooldown
        self.divine_state.prayer_timeout = 300;
        self.divine_state.favor = clamp_favor(self.divine_state.favor + 1);

        match current_tile {
            Tile::Altar { align } => {
                let is_coaligned = player.alignment == align;
                if is_coaligned {
                    if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                        p.hp = p.max_hp;
                    }
                    events.push(GameEvent::LogMessage {
                        text: "You feel devoutly reconciled with your god! You are fully healed.".into(),
                    });

                    // Check for water consecration into Holy Water!
                    let carried = self.arena.items_carried_by(self.player_id);
                    for item_id in carried {
                        if let Some(item) = self.arena.items.get_mut(item_id) {
                            if item.name.to_lowercase().contains("potion of water") && item.buc == Buc::Uncursed {
                                item.buc = consecrate_water(item.buc, true, self.divine_state.favor);
                                events.push(GameEvent::LogMessage {
                                    text: "A flash of divine light consecrates your water into Holy Water!".into(),
                                });
                                break;
                            }
                        }
                    }
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
            Tile::HighAltar { align } => {
                if player.alignment == align {
                    events.push(GameEvent::LogMessage {
                        text: "You kneel before the High Altar of your deity on the Astral Plane. The presence of divinity hums with eternal power.".into(),
                    });
                } else {
                    events.push(GameEvent::LogMessage {
                        text: "You sense intense celestial fury radiating from this foreign High Altar!".into(),
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
        match current_tile {
            Tile::Altar { align } => {
                let carried = self.arena.items_carried_by(self.player_id);
                if idx < carried.len() {
                    let item_id = carried[idx];
                    let item_name = self.arena.items.get(item_id).map(|i| i.name.clone()).unwrap_or_default();
                    self.arena.destroy_item(item_id);
                    if self.wielded_item == Some(item_id) {
                        self.wielded_item = None;
                    }

                    let corpse_nutrition = if item_name.contains("corpse") { 250 } else { 100 };
                    let (new_div, sac_res) = resolve_sacrifice(
                        self.divine_state,
                        player.alignment,
                        align,
                        corpse_nutrition,
                    );
                    self.divine_state = new_div;

                    match sac_res {
                        SacrificeResult::AltarConverted(new_align) => {
                            self.level.set_tile(p_coord, Tile::Altar { align: new_align });
                            events.push(GameEvent::LogMessage {
                                text: format!("You sacrifice the {}. An astral flash erupts and the altar converts to {:?}!", item_name, new_align),
                            });
                        }
                        SacrificeResult::FavorIncreased(fav) => {
                            if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                                p.max_hp += 1;
                                p.hp += 1;
                            }
                            events.push(GameEvent::LogMessage {
                                text: format!("You sacrifice the {}. An aura of divine light envelops the altar. You feel favored by your god (Favor: {}, +1 Max HP)!", item_name, fav),
                            });
                        }
                        SacrificeResult::DivineGift(artifact_name) => {
                            let gift_record = ItemRecord {
                                name: artifact_name.clone(),
                                class: ItemClass::Weapon,
                                weight: 30,
                                buc: Buc::Blessed,
                                is_container: false,
                                is_bag_of_holding: false,
                                enchantment: 2,
                                erosion: 0,
                                proofed: true,
                                location: ItemLocation::CarriedBy(self.player_id),
                            };
                            self.arena.spawn_item(gift_record);
                            events.push(GameEvent::LogMessage {
                                text: format!("A thunderous celestial horn sounds! Your deity crowns you their champion and gifts you {}!", artifact_name),
                            });
                        }
                    }
                    self.scheduler.hero_act(NORMAL_SPEED);
                } else {
                    events.push(GameEvent::LogMessage {
                        text: "You don't have that item in your pack to sacrifice.".into(),
                    });
                }
            }
            Tile::HighAltar { align } => {
                let carried = self.arena.items_carried_by(self.player_id);
                if idx < carried.len() {
                    let item_id = carried[idx];
                    let item_name = self.arena.items.get(item_id).map(|i| i.name.clone()).unwrap_or_default();

                    let is_amulet = item_name.to_lowercase().contains("amulet of yendor");
                    if is_amulet {
                        let outcome = netrust_core::endgame::offer_amulet_on_high_altar(true, player.alignment, align);
                        match outcome {
                            netrust_types::AscensionOutcome::Ascended(god_align) => {
                                self.arena.destroy_item(item_id);
                                if self.wielded_item == Some(item_id) {
                                    self.wielded_item = None;
                                }
                                events.push(GameEvent::LogMessage {
                                    text: format!("An astral choir erupts! You offer the Amulet of Yendor on your co-aligned {:?} High Altar and ascend to immortality as a demigod!", god_align),
                                });
                                events.push(GameEvent::Victory);
                            }
                            netrust_types::AscensionOutcome::Rejected(reason) => {
                                events.push(GameEvent::LogMessage {
                                    text: format!("Offering rejected! {}", reason),
                                });
                                if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                                    p.hp = p.hp.saturating_sub(15).max(1);
                                }
                            }
                        }
                    } else {
                        events.push(GameEvent::LogMessage {
                            text: "The High Altar demands nothing less than the genuine Amulet of Yendor!".into(),
                        });
                    }
                    self.scheduler.hero_act(NORMAL_SPEED);
                } else {
                    events.push(GameEvent::LogMessage {
                        text: "You don't have that item in your pack to sacrifice.".into(),
                    });
                }
            }
            _ => {
                events.push(GameEvent::LogMessage {
                    text: "There is no altar here to sacrifice upon.".into(),
                });
            }
        }
        events
    }
}
