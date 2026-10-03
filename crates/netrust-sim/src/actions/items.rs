//! Item usage: Quaff potions, Read scrolls/spellbooks, Eat food, Cast spells, Zap wands, Dip in water.

use netrust_arena::ItemLocation;
use netrust_core::{buc::WaterType, energy::NORMAL_SPEED, SpellKind};
use netrust_data::{create_item_record, ItemKindId};
use netrust_dungeon::trace_beam_path;
use netrust_types::{Buc, Direction, ItemClass};
use rand::RngCore;

use crate::events::GameEvent;
use crate::world::SimulationWorld;

impl SimulationWorld {
    pub(crate) fn handle_dip(&mut self, item_index: usize, into_water: WaterType) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let carried = self.arena.items_carried_by(self.player_id);
        if item_index < carried.len() {
            let item_id = carried[item_index];
            if let Some(item) = self.arena.items.get_mut(item_id) {
                let prev_buc = item.buc;
                item.buc = netrust_core::buc::dip_water(into_water, prev_buc);
                let status_desc = match item.buc {
                    Buc::Blessed => "glows with a pure amber aura (blessed)!",
                    Buc::Uncursed => "glows softly and feels purified (uncursed).",
                    Buc::Cursed => "emits an ominous black glow (cursed)!",
                };
                events.push(GameEvent::LogMessage {
                    text: format!("You dip the {} into the water. It {}", item.name, status_desc),
                });
                self.scheduler.hero_act(NORMAL_SPEED);
            }
        } else {
            events.push(GameEvent::LogMessage { text: "You don't have that item to dip.".into() });
        }
        events
    }

    pub fn handle_dip_potion(&mut self, reagent_idx: usize, target_idx: usize) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let carried = self.arena.items_carried_by(self.player_id);
        if reagent_idx < carried.len() && target_idx < carried.len() && reagent_idx != target_idx {
            let reagent_id = carried[reagent_idx];
            let target_id = carried[target_idx];
            let reagent_name = self.arena.items.get(reagent_id).map(|it| it.name.clone());
            let target_name = self.arena.items.get(target_id).map(|it| it.name.clone());

            if let (Some(r_name), Some(t_name)) = (reagent_name, target_name) {
                if let Some(result_name) = netrust_core::enchantment::mix_alchemy(&r_name, &t_name) {
                    self.arena.destroy_item(reagent_id);
                    if let Some(target_item) = self.arena.items.get_mut(target_id) {
                        target_item.name = result_name.to_string();
                    }
                    events.push(GameEvent::LogMessage {
                        text: format!("The liquids fizz and bubble furiously! You produce a {}.", result_name),
                    });
                    self.scheduler.hero_act(NORMAL_SPEED);
                } else {
                    events.push(GameEvent::LogMessage { text: "Nothing interesting happens.".into() });
                }
            }
        } else {
            events.push(GameEvent::LogMessage { text: "Invalid items to mix.".into() });
        }
        events
    }

    pub(crate) fn handle_quaff(&mut self, idx: usize) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let carried = self.arena.items_carried_by(self.player_id);
        if idx < carried.len() {
            let item_id = carried[idx];
            let item = self.arena.items.get(item_id).cloned();
            if let Some(item) = item {
                if item.class == ItemClass::Potion {
                    self.arena.destroy_item(item_id);
                    if item.name.contains("healing") {
                        if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                            p.hp = (p.hp + 10).min(p.max_hp);
                        }
                        events.push(GameEvent::LogMessage { text: "You quaff the potion. You feel much better!".into() });
                    } else if item.name.contains("speed") {
                        if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                            p.intrinsics.fast = true;
                        }
                        events.push(GameEvent::LogMessage { text: "You quaff the potion. You are moving much faster!".into() });
                    } else {
                        events.push(GameEvent::LogMessage { text: format!("You quaff the {}. It tastes like water.", item.name) });
                    }
                    self.scheduler.hero_act(NORMAL_SPEED);
                } else {
                    events.push(GameEvent::LogMessage { text: "You can only quaff potions!".into() });
                }
            }
        } else {
            events.push(GameEvent::LogMessage { text: "You have no such potion to quaff.".into() });
        }
        events
    }

    pub(crate) fn handle_read(&mut self, idx: usize) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let carried = self.arena.items_carried_by(self.player_id);
        if idx < carried.len() {
            let item_id = carried[idx];
            let item = self.arena.items.get(item_id).cloned();
            if let Some(item) = item {
                if item.class == ItemClass::Scroll {
                    self.arena.destroy_item(item_id);
                    if item.name.contains("teleport") {
                        if self.level.rooms.len() > 1 {
                            let room_idx = (self.rng.next_u32() as usize) % self.level.rooms.len();
                            let new_c = self.level.rooms[room_idx].center();
                            if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                                p.coord = new_c;
                            }
                            events.push(GameEvent::LogMessage { text: "You read the scroll of teleportation and vanish in a flash of light!".into() });
                        }
                    } else if item.name.contains("curse") {
                        for id in self.arena.items_carried_by(self.player_id) {
                            if let Some(it) = self.arena.items.get_mut(id) {
                                it.buc = netrust_core::buc::uncurse(it.buc);
                            }
                        }
                        events.push(GameEvent::LogMessage { text: "You feel as though someone is helping you. Your possessions are uncursed!".into() });
                    } else if item.name.contains("enchant weapon") {
                        if let Some(wielded_id) = self.wielded_item {
                            if let Some(wielded) = self.arena.items.get_mut(wielded_id) {
                                let is_blessed = item.buc == Buc::Blessed;
                                let is_cursed = item.buc == Buc::Cursed;
                                let res = netrust_core::enchantment::enchant_item(wielded.enchantment, is_blessed, is_cursed);
                                if res.evaporated {
                                    let name = wielded.name.clone();
                                    self.arena.destroy_item(wielded_id);
                                    self.wielded_item = None;
                                    events.push(GameEvent::LogMessage {
                                        text: format!("Your {} glows violently and evaporates!", name),
                                    });
                                } else {
                                    wielded.enchantment = res.new_ench;
                                    events.push(GameEvent::LogMessage {
                                        text: format!("Your {} glows with a silvery aura! ({:+})", wielded.name, res.new_ench),
                                    });
                                }
                            }
                        } else {
                            events.push(GameEvent::LogMessage { text: "Your hands itch for a moment.".into() });
                        }
                    } else if item.name.contains("enchant armor") {
                        let armor_id = self.arena.items_carried_by(self.player_id).into_iter().find(|&id| {
                            self.arena.items.get(id).map(|it| it.class == ItemClass::Armor).unwrap_or(false)
                        });
                        if let Some(aid) = armor_id {
                            if let Some(armor) = self.arena.items.get_mut(aid) {
                                let is_blessed = item.buc == Buc::Blessed;
                                let is_cursed = item.buc == Buc::Cursed;
                                let res = netrust_core::enchantment::enchant_item(armor.enchantment, is_blessed, is_cursed);
                                if res.evaporated {
                                    let name = armor.name.clone();
                                    self.arena.destroy_item(aid);
                                    events.push(GameEvent::LogMessage {
                                        text: format!("Your {} glows violently and evaporates!", name),
                                    });
                                } else {
                                    armor.enchantment = res.new_ench;
                                    events.push(GameEvent::LogMessage {
                                        text: format!("Your {} glows with a protective silver sheen! ({:+})", armor.name, res.new_ench),
                                    });
                                }
                            }
                        } else {
                            events.push(GameEvent::LogMessage { text: "Your skin feels warm for a moment.".into() });
                        }
                    } else {
                        events.push(GameEvent::LogMessage { text: format!("You read the {}. Knowledge fills your mind!", item.name) });
                    }
                    self.scheduler.hero_act(NORMAL_SPEED);
                } else if item.class == ItemClass::Spellbook {
                    let spell = if item.name.contains("force bolt") {
                        SpellKind::ForceBolt
                    } else {
                        SpellKind::CureLightWounds
                    };
                    if !self.known_spells.iter().any(|(s, _)| *s == spell) {
                        self.known_spells.push((spell, 20000));
                    }
                    events.push(GameEvent::LogMessage { text: format!("You study the {} and memorize the spell!", item.name) });
                    self.scheduler.hero_act(NORMAL_SPEED);
                } else {
                    events.push(GameEvent::LogMessage { text: "You can only read scrolls or spellbooks!".into() });
                }
            }
        } else {
            events.push(GameEvent::LogMessage { text: "You have no such scroll or book to read.".into() });
        }
        events
    }

    pub(crate) fn handle_eat(&mut self, idx: usize) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let carried = self.arena.items_carried_by(self.player_id);
        if idx < carried.len() {
            let item_id = carried[idx];
            let item = self.arena.items.get(item_id).cloned();
            if let Some(item) = item {
                if item.class == ItemClass::Food {
                    self.arena.destroy_item(item_id);
                    let nut_gain = if item.name.contains("ration") {
                        800
                    } else if item.name.contains("apple") {
                        50
                    } else {
                        400 // corpse
                    };
                    self.player_nutrition = (self.player_nutrition + nut_gain).min(2000);

                    if item.name.contains("corpse") {
                        if item.name.contains("ant") || item.name.contains("kobold") || item.name.contains("orc") {
                            if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                                p.intrinsics.poison_resistance = true;
                            }
                            events.push(GameEvent::LogMessage {
                                text: "You feel a healthy warmth suffuse your body! You gained Poison Resistance!".into(),
                            });
                        }
                    }
                    events.push(GameEvent::LogMessage { text: format!("You eat the {}. Delicious!", item.name) });
                    self.scheduler.hero_act(NORMAL_SPEED);
                } else {
                    events.push(GameEvent::LogMessage { text: "That is not edible!".into() });
                }
            }
        } else {
            events.push(GameEvent::LogMessage { text: "You have nothing to eat in that slot.".into() });
        }
        events
    }

    pub(crate) fn handle_cast(&mut self, spell_index: usize, dir: Direction) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let Some(player) = self.arena.actors.get(self.player_id).cloned() else {
            return events;
        };

        if spell_index < self.known_spells.len() {
            let (spell, _) = self.known_spells[spell_index];
            let cost = netrust_core::magic::mana_cost(spell);
            if self.player_pw < cost {
                events.push(GameEvent::LogMessage { text: format!("You don't have enough mana! Requires {} Pw, you have {}.", cost, self.player_pw) });
            } else {
                self.player_pw -= cost;
                match spell {
                    SpellKind::ForceBolt | SpellKind::MagicMissile => {
                        let path = trace_beam_path(&self.level, player.coord, dir, 6);
                        events.push(GameEvent::BeamPropagated { path: path.clone() });
                        let spell_damage = if spell == SpellKind::ForceBolt { 18 } else { 14 };
                        for coord in path {
                            if let Some(target_id) = self.actor_at(coord) {
                                if target_id != self.player_id {
                                    if let Some(target) = self.arena.actors.get_mut(target_id) {
                                        target.hp = target.hp.saturating_sub(spell_damage);
                                        if target.hp == 0 {
                                            target.is_dead = true;
                                        }
                                        events.push(GameEvent::AttackLanded {
                                            attacker: self.player_id,
                                            target: target_id,
                                            damage: spell_damage,
                                            lethal: target.is_dead,
                                        });
                                        if target.is_dead {
                                            events.push(GameEvent::LogMessage { text: format!("{} is slain by magic!", target.name) });
                                            let corpse = create_item_record(ItemKindId::Corpse, ItemLocation::Floor(target.coord), Buc::Uncursed);
                                            self.arena.spawn_item(corpse);
                                        }
                                    }
                                    break;
                                }
                            }
                        }
                        events.push(GameEvent::LogMessage { text: format!("You cast {:?}!", spell) });
                    }
                    SpellKind::CureLightWounds => {
                        if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                            p.hp = (p.hp + 12).min(p.max_hp);
                        }
                        events.push(GameEvent::LogMessage { text: "You cast Cure Light Wounds! Your wounds close.".into() });
                    }
                    SpellKind::ExtraHealing => {
                        if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                            p.hp = (p.hp + 30).min(p.max_hp);
                        }
                        events.push(GameEvent::LogMessage { text: "You cast Extra Healing! Divine vitality surges through you.".into() });
                    }
                }
                self.scheduler.hero_act(NORMAL_SPEED);
            }
        } else {
            events.push(GameEvent::LogMessage { text: "You do not know that spell.".into() });
        }
        events
    }

    pub(crate) fn handle_zap_wand(&mut self, dir: Direction, energy: u32) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let Some(player) = self.arena.actors.get(self.player_id).cloned() else {
            return events;
        };

        let path = trace_beam_path(&self.level, player.coord, dir, energy);
        events.push(GameEvent::BeamPropagated { path: path.clone() });

        for coord in path {
            if let Some(target_id) = self.actor_at(coord) {
                if target_id != self.player_id {
                    let wand_damage = 12u32;
                    if let Some(target) = self.arena.actors.get_mut(target_id) {
                        target.hp = target.hp.saturating_sub(wand_damage);
                        if target.hp == 0 {
                            target.is_dead = true;
                        }
                        events.push(GameEvent::AttackLanded {
                            attacker: self.player_id,
                            target: target_id,
                            damage: wand_damage,
                            lethal: target.is_dead,
                        });
                        if target.is_dead {
                            events.push(GameEvent::LogMessage { text: format!("{} is destroyed by the wand beam!", target.name) });
                            let corpse = create_item_record(ItemKindId::Corpse, ItemLocation::Floor(target.coord), Buc::Uncursed);
                            self.arena.spawn_item(corpse);
                        }
                    }
                    break;
                }
            }
        }
        self.scheduler.hero_act(NORMAL_SPEED);
        events
    }
}
