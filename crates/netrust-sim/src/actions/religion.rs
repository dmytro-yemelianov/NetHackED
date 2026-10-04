use netrust_arena::{ItemLocation, ItemRecord};
use netrust_core::energy::NORMAL_SPEED;
use netrust_core::religion::{clamp_favor, consecrate_water, resolve_sacrifice};
use netrust_i18n::Messages;
use netrust_types::{Alignment, Buc, ItemClass, SacrificeResult, Tile};

use crate::events::GameEvent;
use crate::world::SimulationWorld;

impl SimulationWorld {
    pub(crate) fn handle_pray(&mut self) -> Vec<GameEvent> {
        let mut events = Vec::new();
        netrust_core::conducts::record_altar_action(&mut self.conducts);
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
                text: Messages::prayer_timeout(self.locale).into(),
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
                        text: Messages::prayer_coaligned(self.locale).into(),
                    });

                    // Check for water consecration into Holy Water!
                    let carried = self.arena.items_carried_by(self.player_id);
                    for item_id in carried {
                        if let Some(item) = self.arena.items.get_mut(item_id) {
                            if item.name.to_lowercase().contains("potion of water") && item.buc == Buc::Uncursed {
                                item.buc = consecrate_water(item.buc, true, self.divine_state.favor);
                                events.push(GameEvent::LogMessage {
                                    text: Messages::holy_water_consecrated(self.locale).into(),
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
                        text: Messages::prayer_neutral(self.locale).into(),
                    });
                } else {
                    // Wrath from opposing god!
                    let wrath_damage = 6u32;
                    if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                        p.hp = p.hp.saturating_sub(wrath_damage).max(1);
                    }
                    events.push(GameEvent::LogMessage {
                        text: Messages::prayer_wrath(self.locale).into(),
                    });
                }
            }
            Tile::HighAltar { align } => {
                if player.alignment == align {
                    events.push(GameEvent::LogMessage {
                        text: Messages::prayer_high_altar_coaligned(self.locale).into(),
                    });
                } else {
                    events.push(GameEvent::LogMessage {
                        text: Messages::prayer_high_altar_foreign(self.locale).into(),
                    });
                }
            }
            _ => {
                events.push(GameEvent::LogMessage {
                    text: Messages::prayer_general(self.locale).into(),
                });
            }
        }
        self.scheduler.hero_act(NORMAL_SPEED);
        events
    }

    pub(crate) fn handle_sacrifice(&mut self, idx: usize) -> Vec<GameEvent> {
        let mut events = Vec::new();
        netrust_core::conducts::record_altar_action(&mut self.conducts);
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
                                text: Messages::altar_converted(&item_name, new_align, self.locale),
                            });
                        }
                        SacrificeResult::FavorIncreased(fav) => {
                            if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                                p.max_hp += 1;
                                p.hp += 1;
                            }
                            events.push(GameEvent::LogMessage {
                                text: Messages::sacrifice_favor_increased(&item_name, fav, self.locale),
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
                                corpse_race: None,
                                corpse_age: 0,
                                rot_threshold: 50,
                            };
                            self.arena.spawn_item(gift_record);
                            events.push(GameEvent::LogMessage {
                                text: Messages::divine_crowning(&artifact_name, self.locale),
                            });
                        }
                    }
                    self.scheduler.hero_act(NORMAL_SPEED);
                } else {
                    events.push(GameEvent::LogMessage {
                        text: Messages::no_sacrifice_item(self.locale).into(),
                    });
                }
            }
            Tile::HighAltar { align } => {
                let carried = self.arena.items_carried_by(self.player_id);
                if idx < carried.len() {
                    let item_id = carried[idx];
                    let is_amulet = self.arena.items.get(item_id).is_some_and(crate::actions::items::is_real_amulet);
                    if is_amulet {
                        let outcome = netrust_core::endgame::offer_amulet_on_high_altar(true, player.alignment, align);
                        match outcome {
                            netrust_types::AscensionOutcome::Ascended(god_align) => {
                                self.arena.destroy_item(item_id);
                                if self.wielded_item == Some(item_id) {
                                    self.wielded_item = None;
                                }
                                events.push(GameEvent::LogMessage {
                                    text: Messages::ascension_victory(god_align, self.locale),
                                });
                                events.push(GameEvent::Victory);
                            }
                            netrust_types::AscensionOutcome::Rejected(reason) => {
                                events.push(GameEvent::LogMessage {
                                    text: Messages::ascension_rejected(&reason, self.locale),
                                });
                                if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                                    p.hp = p.hp.saturating_sub(15).max(1);
                                }
                            }
                        }
                    } else {
                        events.push(GameEvent::LogMessage {
                            text: Messages::high_altar_demands_amulet(self.locale).into(),
                        });
                    }
                    self.scheduler.hero_act(NORMAL_SPEED);
                } else {
                    events.push(GameEvent::LogMessage {
                        text: Messages::no_sacrifice_item(self.locale).into(),
                    });
                }
            }
            _ => {
                events.push(GameEvent::LogMessage {
                    text: Messages::no_altar(self.locale).into(),
                });
            }
        }
        events
    }

    pub(crate) fn handle_donate(&mut self, amount: u32) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let Some(player) = self.arena.actors.get(self.player_id).cloned() else {
            return events;
        };

        // Find a priest on the floor near player (distance <= 6)
        let priest_id = self.arena.actors.iter().find_map(|(id, a)| {
            if !a.is_dead && a.name.to_lowercase().contains("priest") && a.coord.chebyshev_distance(player.coord) <= 6 {
                Some(id)
            } else {
                None
            }
        });

        let Some(_pid) = priest_id else {
            events.push(GameEvent::LogMessage {
                text: "There is no priest nearby to receive your donation.".into(),
            });
            return events;
        };

        let donation = if amount == 0 {
            // Default donation: 400 * level
            netrust_core::mines::protection_donation_cost(player.level)
        } else {
            amount
        };

        if self.player_gold < donation {
            events.push(GameEvent::LogMessage {
                text: format!("You do not have enough gold to donate {} zm.", donation),
            });
            return events;
        }

        self.player_gold -= donation;

        // Apply divine protection
        let prev_prot = self.divine_protection;
        let new_prot = netrust_core::mines::apply_priest_donation(prev_prot, donation, player.level);
        if new_prot > prev_prot {
            let gained = new_prot - prev_prot;
            self.divine_protection = new_prot;
            if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                p.ac -= gained as i32; // Lower AC is better in NetHack
            }
            events.push(GameEvent::LogMessage {
                text: format!("You feel much safer! You are granted +{} divine AC protection (total: +{}).", gained, new_prot),
            });
        } else if prev_prot >= netrust_core::mines::MAX_DIVINE_PROTECTION {
            events.push(GameEvent::LogMessage {
                text: "The priest smiles benevolently: 'You already possess the maximum divine protection.'".into(),
            });
        } else {
            events.push(GameEvent::LogMessage {
                text: "The priest thanks you for your contribution to the temple.".into(),
            });
        }

        // Uncursing service: if donation >= 200 * level, uncurse cursed items
        let uncurse_threshold = 200 * player.level.max(1);
        if donation >= uncurse_threshold {
            let carried = self.arena.items_carried_by(self.player_id);
            let mut uncursed_count = 0;
            for iid in carried {
                if let Some(item) = self.arena.items.get_mut(iid) {
                    if item.buc == Buc::Cursed {
                        item.buc = netrust_core::mines::priest_uncurse(item.buc);
                        uncursed_count += 1;
                    }
                }
            }
            if uncursed_count > 0 {
                events.push(GameEvent::LogMessage {
                    text: format!("The priest sprinkles holy water! {} cursed items glow and are uncursed.", uncursed_count),
                });
            }
        }

        self.divine_state.favor = netrust_core::religion::clamp_favor(self.divine_state.favor + 2);
        self.scheduler.hero_act(NORMAL_SPEED);
        events
    }
}
