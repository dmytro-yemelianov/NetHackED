//! Item usage: Quaff potions, Read scrolls/spellbooks, Eat food, Cast spells, Zap wands, Dip in water.

use netrust_arena::ItemLocation;
use netrust_core::{buc::WaterType, energy::NORMAL_SPEED, SpellKind};
use netrust_data::{create_item_record, ItemKindId};
use netrust_dungeon::trace_beam_path;
use netrust_types::{Buc, Coord, Direction, ItemClass};
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

                // Handle potion dilution & water transformation
                if item.class == ItemClass::Potion {
                    if into_water == WaterType::Plain {
                        if item.name.contains("extra healing") {
                            let old = item.name.clone();
                            item.name = "potion of healing".into();
                            events.push(GameEvent::LogMessage {
                                text: netrust_i18n::Messages::potion_diluted(&old, &item.name, self.locale),
                            });
                        } else if !item.name.contains("water") {
                            let old = item.name.clone();
                            item.name = "potion of water".into();
                            events.push(GameEvent::LogMessage {
                                text: netrust_i18n::Messages::potion_diluted(&old, &item.name, self.locale),
                            });
                        }
                    } else if into_water == WaterType::Holy && item.name.contains("water") {
                        item.name = "potion of holy water".into();
                    } else if into_water == WaterType::Unholy && item.name.contains("water") {
                        item.name = "potion of unholy water".into();
                    }
                }

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

    pub fn handle_rub(&mut self, item_index: usize) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let carried = self.arena.items_carried_by(self.player_id);
        if item_index >= carried.len() {
            events.push(GameEvent::LogMessage { text: "You don't have that item to rub.".into() });
            return events;
        }

        let item_id = carried[item_index];
        let item = self.arena.items.get(item_id).cloned();
        if let Some(item) = item {
            let is_magic = item.name.contains("magic lamp");
            let is_oil = item.name.contains("oil lamp");

            if is_magic {
                let (res, consumed) = netrust_core::rub_lamp(true, true, item.buc, 1000);
                if consumed {
                    if let Some(it_mut) = self.arena.items.get_mut(item_id) {
                        it_mut.name = "oil lamp".into();
                    }
                }

                match res {
                    netrust_core::RubResult::WishGranted => {
                        events.push(GameEvent::LogMessage {
                            text: netrust_i18n::Messages::djinni_wishing(self.locale).into(),
                        });
                        // Grant an immediate boon / blessed scroll of identify
                        let player_c = self.arena.actors.get(self.player_id).map(|p| p.coord).unwrap_or(Coord::new_unchecked(1, 1));
                        let gift = create_item_record(ItemKindId::ScrollOfIdentify, ItemLocation::Floor(player_c), Buc::Blessed);
                        self.arena.spawn_item(gift);
                    }
                    netrust_core::RubResult::PeacefulDjinni => {
                        events.push(GameEvent::LogMessage {
                            text: netrust_i18n::Messages::djinni_peaceful(self.locale).into(),
                        });
                    }
                    netrust_core::RubResult::HostileDjinni => {
                        events.push(GameEvent::LogMessage {
                            text: netrust_i18n::Messages::djinni_hostile(self.locale).into(),
                        });
                        let player_c = self.arena.actors.get(self.player_id).map(|p| p.coord).unwrap_or(Coord::new_unchecked(1, 1));
                        let spawn_c = player_c.neighbors().into_iter().find(|&c| self.level.is_passable(c) && self.actor_at(c).is_none()).unwrap_or(player_c);
                        let mut mon = netrust_data::create_monster_record(netrust_data::MonsterSpeciesId::Djinni, spawn_c);
                        mon.name = "hostile djinni".into();
                        self.arena.spawn_actor(mon);
                    }
                    _ => {
                        events.push(GameEvent::LogMessage {
                            text: netrust_i18n::Messages::lamp_smoke(self.locale).into(),
                        });
                    }
                }
                self.scheduler.hero_act(NORMAL_SPEED);
            } else if is_oil {
                events.push(GameEvent::LogMessage {
                    text: netrust_i18n::Messages::lamp_smoke(self.locale).into(),
                });
                self.scheduler.hero_act(NORMAL_SPEED);
            } else {
                events.push(GameEvent::LogMessage {
                    text: format!("Rubbing the {} doesn't seem to do anything.", item.name),
                });
            }
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
                    } else if item.name.contains("polymorph") {
                        // Apply polymorph to self
                        self.hero.polymorph = Some(netrust_types::PolymorphForm {
                            monster_id: 1, // Dummy id
                            hp: 20,
                            max_hp: 20,
                            duration: 100,
                        });
                        if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                            p.hp = 20;
                            p.max_hp = 20;
                        }
                        events.push(GameEvent::LogMessage { text: "You feel a change coming over you... you polymorph!".into() });
                    } else if item.name.contains("acid") {
                        netrust_core::afflictions::cure_petrification(&mut self.hero);
                        events.push(GameEvent::LogMessage { text: "You quaff the potion of acid. It burns, but you feel less stiff!".into() });
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
                    netrust_core::conducts::record_read(&mut self.conducts);
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
                    } else if item.name.contains("charging") {
                        let wand_id = self.arena.items_carried_by(self.player_id).into_iter().find(|&id| {
                            self.arena.items.get(id).map(|it| it.class == ItemClass::Wand).unwrap_or(false)
                        });
                        if let Some(wid) = wand_id {
                            let (wand_name, charges, recharges) = {
                                let w = self.arena.items.get(wid).unwrap();
                                (w.name.clone(), w.enchantment.max(0) as u32, w.erosion as u32)
                            };
                            let wand_state = netrust_types::WandCharges { charges, recharges };
                            match netrust_core::artifacts_wands::recharge_wand(wand_state, 5) {
                                netrust_types::RechargeResult::Exploded => {
                                    self.arena.destroy_item(wid);
                                    if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                                        p.hp = p.hp.saturating_sub(20);
                                        if p.hp == 0 {
                                            p.is_dead = true;
                                        }
                                    }
                                    events.push(GameEvent::LogMessage {
                                        text: netrust_i18n::Messages::wand_exploded(&wand_name, self.locale),
                                    });
                                }
                                netrust_types::RechargeResult::Success(new_w) => {
                                    if let Some(w_mut) = self.arena.items.get_mut(wid) {
                                        w_mut.enchantment = new_w.charges as i8;
                                        w_mut.erosion = new_w.recharges as u8;
                                        events.push(GameEvent::LogMessage {
                                            text: netrust_i18n::Messages::wand_recharged(&wand_name, new_w.charges, new_w.recharges, self.locale),
                                        });
                                    }
                                }
                            }
                        } else {
                            events.push(GameEvent::LogMessage { text: "You have no wands to recharge.".into() });
                        }
                    } else if item.name.contains("genocide") {
                        self.conducts.genocideless = false;
                        match item.buc {
                            Buc::Cursed => {
                                let summon_count = netrust_core::genocide::cursed_genocide_summon_count();
                                let player_c = self.arena.actors.get(self.player_id).map(|p| p.coord).unwrap_or(Coord::new_unchecked(1, 1));
                                for _ in 0..summon_count {
                                    let spawn_c = player_c.neighbors().into_iter().find(|&c| self.level.is_passable(c) && self.actor_at(c).is_none()).unwrap_or(player_c);
                                    let mut mon = netrust_data::create_monster_record(netrust_data::MonsterSpeciesId::Goblin, spawn_c);
                                    mon.name = "hostile goblin".into();
                                    self.arena.spawn_actor(mon);
                                }
                                events.push(GameEvent::LogMessage { text: "You read the cursed scroll of genocide. Monsters appear!".into() });
                            }
                            Buc::Uncursed => {
                                let target = netrust_types::GenocideTarget::Species("goblin".to_string());
                                netrust_core::genocide::apply_genocide(&mut self.genocide_registry, target.clone());
                                // Wipe from current floor
                                let mut to_remove = Vec::new();
                                for (aid, actor) in self.arena.actors.iter() {
                                    if !actor.is_player && self.actor_is_genocided(&actor.name) {
                                        to_remove.push(aid);
                                    }
                                }
                                for aid in to_remove {
                                    self.remove_actor_dropping_items(aid);
                                }
                                events.push(GameEvent::LogMessage { text: "You read the scroll of genocide. A species is wiped out!".into() });
                            }
                            Buc::Blessed => {
                                let target = netrust_types::GenocideTarget::Class('L'); // Lich class for example
                                netrust_core::genocide::apply_genocide(&mut self.genocide_registry, target.clone());
                                // Wipe from current floor
                                let mut to_remove = Vec::new();
                                for (aid, actor) in self.arena.actors.iter() {
                                    if !actor.is_player && self.actor_is_genocided(&actor.name) {
                                        to_remove.push(aid);
                                    }
                                }
                                for aid in to_remove {
                                    self.remove_actor_dropping_items(aid);
                                }
                                events.push(GameEvent::LogMessage { text: "You read the blessed scroll of genocide. A whole class of monsters is wiped out!".into() });
                            }
                        }
                    } else {
                        events.push(GameEvent::LogMessage { text: format!("You read the {}. Knowledge fills your mind!", item.name) });
                    }
                    self.scheduler.hero_act(NORMAL_SPEED);
                } else if item.class == ItemClass::Spellbook {
                    netrust_core::conducts::record_read(&mut self.conducts);
                    if item.name.contains("Book of the Dead") {
                        let player_coord = self.arena.actors.get(self.player_id).map(|p| p.coord).unwrap_or(Coord::new_unchecked(0, 0));
                        let on_vs = self.vibrating_square == Some(player_coord);
                        self.ritual_progress = netrust_core::step_ritual(
                            self.ritual_progress,
                            netrust_core::InvocationStep::ReadBook,
                            on_vs,
                            &self.candelabrum_state,
                        );
                        if netrust_core::is_sanctum_accessible(self.ritual_progress) {
                            events.push(GameEvent::LogMessage {
                                text: "The cavern trembles violently! A subterranean portal to Moloch's Sanctum opens before you!".into(),
                            });
                        } else if !on_vs {
                            events.push(GameEvent::LogMessage {
                                text: "You recite the eldritch litany of the Book of the Dead, but nothing happens. You are not on the Vibrating Square!".into(),
                            });
                        } else {
                            events.push(GameEvent::LogMessage {
                                text: "You read from the Book of the Dead, but the ritual sequence is incomplete.".into(),
                            });
                        }
                    } else {
                        let spell = if item.name.contains("force bolt") {
                            SpellKind::ForceBolt
                        } else {
                            SpellKind::CureLightWounds
                        };
                        if !self.known_spells.iter().any(|(s, _)| *s == spell) {
                            self.known_spells.push((spell, 20000));
                        }
                        events.push(GameEvent::LogMessage { text: format!("You study the {} and memorize the spell!", item.name) });
                    }
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
                        netrust_core::conducts::record_eat_meat(&mut self.conducts);
                        let corpse_race = item.corpse_race.as_deref().unwrap_or("unknown");
                        if netrust_core::nutrition::is_cannibalism(corpse_race, "human") {
                            events.push(GameEvent::LogMessage { text: "You cannibal! You feel deeply ashamed.".into() });
                        }
                        if netrust_core::nutrition::is_corpse_tainted(item.corpse_age, item.rot_threshold) {
                            events.push(GameEvent::LogMessage { text: "Ugh, this corpse is tainted!".into() });
                        }
                        
                        let monster_name = item.name.replace(" corpse", "");
                        if let Some(intrinsic) = netrust_core::nutrition::intrinsic_from_corpse(&monster_name) {
                            if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                                match intrinsic.as_str() {
                                    "fire_resistance" => p.intrinsics.fire_resistance = true,
                                    "cold_resistance" => p.intrinsics.cold_resistance = true,
                                    "shock_resistance" => p.intrinsics.shock_resistance = true,
                                    "poison_resistance" => p.intrinsics.poison_resistance = true,
                                    "sleep_resistance" => p.intrinsics.sleep_resistance = true,
                                    "telepathy" => p.intrinsics.telepathy = true,
                                    "see_invisible" => p.intrinsics.see_invisible = true,
                                    _ => {}
                                }
                            }
                            events.push(GameEvent::LogMessage { text: format!("You gained {}!", intrinsic) });
                        }

                        if item.name.contains("lizard") {
                            netrust_core::afflictions::cure_petrification(&mut self.hero);
                            events.push(GameEvent::LogMessage {
                                text: "You eat the lizard corpse. You feel limber!".into(),
                            });
                        } else if item.name.contains("ant") || item.name.contains("kobold") || item.name.contains("orc") {
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

        // Find carried wand (or default)
        let wand_id = self.arena.items_carried_by(self.player_id).into_iter().find(|&id| {
            self.arena.items.get(id).map(|it| it.class == ItemClass::Wand).unwrap_or(false)
        });

        let wand_name = if let Some(wid) = wand_id {
            if let Some(wand_item) = self.arena.items.get_mut(wid) {
                let current_charges = netrust_types::WandCharges {
                    charges: wand_item.enchantment.max(0) as u32,
                    recharges: wand_item.erosion as u32,
                };
                if let Some(new_charges) = netrust_core::artifacts_wands::zap_wand(current_charges) {
                    wand_item.enchantment = new_charges.charges as i8;
                    wand_item.name.clone()
                } else {
                    events.push(GameEvent::LogMessage {
                        text: netrust_i18n::Messages::wand_empty(self.locale).into(),
                    });
                    self.scheduler.hero_act(NORMAL_SPEED);
                    return events;
                }
            } else {
                "wand of striking".to_string()
            }
        } else {
            "wand of striking".to_string()
        };

        // If Wand of Secret Door Detection: reveals secret doors in 5x5 radius
        if wand_name.contains("secret door") {
            let mut revealed = 0;
            for dy in -3..=3 {
                for dx in -3..=3 {
                    let nx = player.coord.x as i32 + dx;
                    let ny = player.coord.y as i32 + dy;
                    if nx >= 0 && nx < netrust_types::COLNO as i32 && ny >= 0 && ny < netrust_types::ROWNO as i32 {
                        let c = Coord::new_unchecked(nx as usize, ny as usize);
                        let tile = self.level.get_tile_mut(c);
                        if matches!(tile, netrust_types::Tile::SecretDoor { .. }) {
                            tile.reveal_secret_door();
                            revealed += 1;
                        }
                    }
                }
            }
            if revealed > 0 {
                events.push(GameEvent::LogMessage {
                    text: netrust_i18n::Messages::secret_doors_found(revealed, self.locale),
                });
            } else {
                events.push(GameEvent::LogMessage {
                    text: "You feel a tingling sensation, but sense no hidden doors.".into(),
                });
            }
            self.scheduler.hero_act(NORMAL_SPEED);
            return events;
        }

        let path = trace_beam_path(&self.level, player.coord, dir, energy);
        events.push(GameEvent::BeamPropagated { path: path.clone() });

        let mut hit_coords = path;
        let (dx, dy) = dir.delta();
        if dx != 0 || dy != 0 {
            let last_c = hit_coords.last().copied().unwrap_or(player.coord);
            let tx = last_c.x as i32 + dx as i32;
            let ty = last_c.y as i32 + dy as i32;
            if tx >= 0 && tx < netrust_types::COLNO as i32 && ty >= 0 && ty < netrust_types::ROWNO as i32 {
                hit_coords.push(Coord::new_unchecked(tx as usize, ty as usize));
            }
        }

        for coord in hit_coords {
            // Environment interactions along ray path
            let current_tile = self.level.get_tile(coord).clone();
            if wand_name.contains("striking") {
                if matches!(current_tile, netrust_types::Tile::Drawbridge { .. }) {
                    let _ = netrust_core::endgame::destroy_drawbridge();
                    self.level.set_tile(coord, netrust_types::Tile::Moat);
                    events.push(GameEvent::LogMessage {
                        text: netrust_i18n::Messages::drawbridge_collapse(self.locale).into(),
                    });
                    break;
                } else if matches!(current_tile, netrust_types::Tile::Door { .. }) {
                    self.level.get_tile_mut(coord).break_door();
                    events.push(GameEvent::LogMessage {
                        text: netrust_i18n::Messages::door_splinters(self.locale).into(),
                    });
                }
            } else if wand_name.contains("cold") {
                if matches!(current_tile, netrust_types::Tile::Pool { .. }) {
                    self.level.set_tile(coord, netrust_types::Tile::Pool { frozen: true });
                    events.push(GameEvent::LogMessage {
                        text: netrust_i18n::Messages::pool_frozen(self.locale).into(),
                    });
                }
            }

            // Actor interaction
            if let Some(target_id) = self.actor_at(coord) {
                if target_id != self.player_id {
                    let wand_damage = if wand_name.contains("death") {
                        100u32
                    } else if wand_name.contains("cold") {
                        18u32
                    } else {
                        12u32
                    };
                    if let Some(target) = self.arena.actors.get_mut(target_id) {
                        if wand_name.contains("polymorph") {
                            if !target.is_unique && !target.is_player {
                                // transform monster
                                let new_species = netrust_data::MonsterSpeciesId::Goblin; // simplified
                                let new_arch = netrust_data::get_monster_species(new_species);
                                target.name = new_arch.name.to_string();
                                target.hp = new_arch.base_hp;
                                target.max_hp = new_arch.max_hp;
                                target.ac = new_arch.ac;
                                target.speed = new_arch.speed;
                                target.level = new_arch.level;
                                events.push(GameEvent::LogMessage {
                                    text: format!("The monster turns into a {}!", target.name),
                                });
                            } else {
                                events.push(GameEvent::LogMessage {
                                    text: "The monster shudders but is unaffected.".into(),
                                });
                            }
                        } else {
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
                        }
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

    pub(crate) fn handle_wish(&mut self, wish_str: String) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let Some(player) = self.arena.actors.get(self.player_id).cloned() else {
            return events;
        };

        // Find carried wand of wishing
        let wow_id = self.arena.items_carried_by(self.player_id).into_iter().find(|&id| {
            self.arena.items.get(id).map(|it| it.name.contains("wishing")).unwrap_or(false)
        });

        if let Some(wid) = wow_id {
            let wand_item = self.arena.items.get_mut(wid).unwrap();
            let charges = wand_item.enchantment.max(0) as u32;
            let recharges = wand_item.erosion as u32;
            if let Some(new_w) = netrust_core::artifacts_wands::zap_wand(netrust_types::WandCharges { charges, recharges }) {
                wand_item.enchantment = new_w.charges as i8;
            } else {
                events.push(GameEvent::LogMessage { text: netrust_i18n::Messages::wish_empty(self.locale).into() });
                self.scheduler.hero_act(NORMAL_SPEED);
                return events;
            }
        }
        netrust_core::conducts::record_wish(&mut self.conducts);

        if let Some((item_query, ench, buc)) = netrust_core::artifacts_wands::parse_wish(&wish_str) {
            let matched_arch = netrust_data::ITEM_CATALOG.iter().find(|arch| {
                arch.name.to_lowercase() == item_query.to_lowercase()
                    || arch.name.to_lowercase().contains(&item_query.to_lowercase())
                    || item_query.to_lowercase().contains(arch.name.to_lowercase().as_str())
            });

            if let Some(arch) = matched_arch {
                let mut record = create_item_record(arch.id, ItemLocation::Floor(player.coord), buc);
                record.enchantment = ench;
                let spawned_id = self.arena.spawn_item(record);
                let item_name = self.arena.items.get(spawned_id).unwrap().name.clone();
                events.push(GameEvent::LogMessage {
                    text: netrust_i18n::Messages::wish_granted(&item_name, self.locale),
                });
            } else {
                events.push(GameEvent::LogMessage {
                    text: format!("You feel a vague sense of loss. You wished for '{}', but received nothing.", wish_str),
                });
            }
        } else {
            events.push(GameEvent::LogMessage {
                text: "Your mind draws a blank. Nothing happens.".into(),
            });
        }

        self.scheduler.hero_act(NORMAL_SPEED);
        events
    }
}
