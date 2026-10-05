//! Item usage: Quaff potions, Read scrolls/spellbooks, Eat food, Cast spells, Zap wands, Dip in water.

use netrust_arena::ItemLocation;
use netrust_core::{buc::WaterType, energy::NORMAL_SPEED, SpellKind};
use netrust_data::ItemKindId;
use netrust_dungeon::trace_beam_path;
use netrust_types::{Buc, Coord, Direction, ItemClass};
use rand::{Rng, RngCore};

use crate::events::GameEvent;
use crate::world::SimulationWorld;

/// Lowercase, trim and drop a leading article from a wish query.
pub fn normalize_wish_name(query: &str) -> String {
    let lower = query.trim().to_lowercase();
    for article in ["a ", "an ", "the "] {
        if let Some(rest) = lower.strip_prefix(article) {
            return rest.trim().to_string();
        }
    }
    lower
}

const UNWISHABLE: &[ItemKindId] = &[
    ItemKindId::BellOfOpening,
    ItemKindId::CandelabrumOfInvocation,
    ItemKindId::BookOfTheDead,
    ItemKindId::OrbOfFate,
    ItemKindId::HeartOfAhriman,
    ItemKindId::MagicMirrorOfMerlin,
    ItemKindId::EyesOfTheOverworld,
    ItemKindId::MasterKeyOfThievery,
    ItemKindId::TsurugiOfMuramasa,
    ItemKindId::PlatinumYendorianExpressCard,
    ItemKindId::StaffOfAesculapius,
    ItemKindId::OrbOfDetection,
];

impl SimulationWorld {
    /// Draws one C enchant roll from `self.rng` in the range of `draw`
    /// (`rn2(n)`: `0..n`, `rnd(n)`: `1..=n`); no draw yields 0.
    fn draw_enchant(&mut self, draw: netrust_core::EnchantDraw) -> u32 {
        match draw {
            netrust_core::EnchantDraw::None => 0,
            netrust_core::EnchantDraw::Rn2(n) => self.rng.random_range(0..n.max(1)),
            netrust_core::EnchantDraw::Rnd(n) => self.rng.random_range(1..=n.max(1)),
        }
    }

    /// Applies an enchant scroll outcome to item `id` and logs it.
    fn apply_enchant_outcome(
        &mut self,
        id: netrust_arena::ItemId,
        outcome: netrust_core::EnchantOutcome,
        is_armor: bool,
        events: &mut Vec<GameEvent>,
    ) {
        match outcome {
            netrust_core::EnchantOutcome::Evaporated => {
                let name = self
                    .arena
                    .items
                    .get(id)
                    .map(|it| it.name.clone())
                    .unwrap_or_default();
                self.arena.destroy_item(id);
                if self.wielded_item == Some(id) {
                    self.wielded_item = None;
                }
                events.push(GameEvent::LogMessage {
                    text: format!("Your {name} glows violently and evaporates!"),
                });
            }
            netrust_core::EnchantOutcome::Changed(new_spe) => {
                if let Some(it) = self.arena.items.get_mut(id) {
                    it.enchantment = new_spe;
                    let text = if is_armor {
                        format!(
                            "Your {} glows with a protective silver sheen! ({:+})",
                            it.name, new_spe
                        )
                    } else {
                        format!(
                            "Your {} glows with a silvery aura! ({:+})",
                            it.name, new_spe
                        )
                    };
                    events.push(GameEvent::LogMessage { text });
                }
            }
        }
    }

    pub(crate) fn handle_dip(
        &mut self,
        item_index: usize,
        into_water: WaterType,
    ) -> Vec<GameEvent> {
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
                                text: netrust_i18n::Messages::potion_diluted(
                                    &old,
                                    &item.name,
                                    self.locale,
                                ),
                            });
                        } else if !item.name.contains("water") {
                            let old = item.name.clone();
                            item.name = "potion of water".into();
                            events.push(GameEvent::LogMessage {
                                text: netrust_i18n::Messages::potion_diluted(
                                    &old,
                                    &item.name,
                                    self.locale,
                                ),
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
                    text: format!(
                        "You dip the {} into the water. It {}",
                        item.name, status_desc
                    ),
                });
                self.scheduler.hero_act(NORMAL_SPEED);
            }
        } else {
            events.push(GameEvent::LogMessage {
                text: "You don't have that item to dip.".into(),
            });
        }
        events
    }

    pub fn handle_rub(&mut self, item_index: usize) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let carried = self.arena.items_carried_by(self.player_id);
        if item_index >= carried.len() {
            events.push(GameEvent::LogMessage {
                text: "You don't have that item to rub.".into(),
            });
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
                        let player_c = self
                            .arena
                            .actors
                            .get(self.player_id)
                            .map(|p| p.coord)
                            .unwrap_or(Coord::new_unchecked(1, 1));
                        if let Some(gift) = self.ruleset.create_item_record_by_id(
                            ItemKindId::ScrollOfIdentify,
                            ItemLocation::Floor(player_c),
                            Buc::Blessed,
                        ) {
                            self.arena.spawn_item(gift);
                        }
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
                        let player_c = self
                            .arena
                            .actors
                            .get(self.player_id)
                            .map(|p| p.coord)
                            .unwrap_or(Coord::new_unchecked(1, 1));
                        let spawn_c = player_c
                            .neighbors()
                            .into_iter()
                            .find(|&c| self.level.is_passable(c) && self.actor_at(c).is_none())
                            .unwrap_or(player_c);
                        if let Some(mut mon) = self.ruleset.create_monster_record_by_id(
                            netrust_data::MonsterSpeciesId::Djinni,
                            spawn_c,
                        ) {
                            mon.name = "hostile djinni".into();
                            mon.is_peaceful = false;
                            if let Some(def) = self
                                .ruleset
                                .monster_by_id(netrust_data::MonsterSpeciesId::Djinni)
                            {
                                self.set_monster_malign(&mut mon, def);
                            }
                            self.arena.spawn_actor(mon);
                        }
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
                if let Some(result_name) = netrust_core::enchantment::mix_alchemy(&r_name, &t_name)
                {
                    self.arena.destroy_item(reagent_id);
                    if let Some(target_item) = self.arena.items.get_mut(target_id) {
                        target_item.name = result_name.to_string();
                    }
                    events.push(GameEvent::LogMessage {
                        text: format!(
                            "The liquids fizz and bubble furiously! You produce a {result_name}."
                        ),
                    });
                    self.scheduler.hero_act(NORMAL_SPEED);
                } else {
                    events.push(GameEvent::LogMessage {
                        text: "Nothing interesting happens.".into(),
                    });
                }
            }
        } else {
            events.push(GameEvent::LogMessage {
                text: "Invalid items to mix.".into(),
            });
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
                        events.push(GameEvent::LogMessage {
                            text: "You quaff the potion. You feel much better!".into(),
                        });
                    } else if item.name.contains("speed") {
                        if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                            p.intrinsics.fast = true;
                        }
                        events.push(GameEvent::LogMessage {
                            text: "You quaff the potion. You are moving much faster!".into(),
                        });
                    } else if item.name.contains("polymorph") {
                        // Apply polymorph to self. Save the current (actor)
                        // HP as the base form first: it is restored on
                        // rehumanize (C polymon sets only u.mh, polyself.c:872).
                        if let Some(p) = self.arena.actors.get(self.player_id) {
                            if self.hero.polymorph.is_none() {
                                Self::sync_hero_form_from_actor(&mut self.hero, p);
                            }
                        }
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
                        events.push(GameEvent::LogMessage {
                            text: "You feel a change coming over you... you polymorph!".into(),
                        });
                    } else if item.name.contains("acid") {
                        netrust_core::afflictions::cure_petrification(&mut self.hero);
                        events.push(GameEvent::LogMessage {
                            text:
                                "You quaff the potion of acid. It burns, but you feel less stiff!"
                                    .into(),
                        });
                    } else {
                        events.push(GameEvent::LogMessage {
                            text: format!("You quaff the {}. It tastes like water.", item.name),
                        });
                    }
                    self.scheduler.hero_act(NORMAL_SPEED);
                } else {
                    events.push(GameEvent::LogMessage {
                        text: "You can only quaff potions!".into(),
                    });
                }
            }
        } else {
            events.push(GameEvent::LogMessage {
                text: "You have no such potion to quaff.".into(),
            });
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
                        let wielded = self
                            .wielded_item
                            .and_then(|id| self.arena.items.get(id).map(|w| (id, w.enchantment)));
                        if let Some((wielded_id, spe)) = wielded {
                            // C draws the amount (read.c:1667) before chwepon's
                            // evaporation roll (wield.c:999-1000).
                            let gain_roll =
                                self.draw_enchant(netrust_core::weapon_gain_draw(spe, item.buc));
                            let evaporate_roll = self
                                .draw_enchant(netrust_core::weapon_evaporation_draw(spe, item.buc));
                            let outcome = netrust_core::enchant_weapon(
                                spe,
                                item.buc,
                                evaporate_roll,
                                gain_roll,
                            );
                            self.apply_enchant_outcome(wielded_id, outcome, false, &mut events);
                        } else {
                            events.push(GameEvent::LogMessage {
                                text: "Your hands itch for a moment.".into(),
                            });
                        }
                    } else if item.name.contains("enchant armor") {
                        // Carried armor stands in for C `some_armor` (no worn slots).
                        let armor = self
                            .arena
                            .items_carried_by(self.player_id)
                            .into_iter()
                            .find_map(|id| {
                                self.arena
                                    .items
                                    .get(id)
                                    .filter(|it| it.class == ItemClass::Armor)
                                    .map(|it| (id, it.enchantment, it.name.clone()))
                            });
                        if let Some((aid, spe, name)) = armor {
                            // Item records carry no object kind: elven / oc_magic are
                            // classified by name; the cornuthaum is special for Wizards.
                            let special = netrust_core::armor_is_elven(&name)
                                || (self.role_name == "Wizard" && name == "cornuthaum");
                            let magical = netrust_core::armor_is_magical(&name);
                            let evaporate_roll = self.draw_enchant(
                                netrust_core::armor_evaporation_draw(spe, item.buc, special),
                            );
                            // C draws the gain only when the armor survives (read.c:1179).
                            let gain_roll = if netrust_core::armor_evaporates(
                                spe,
                                item.buc,
                                special,
                                evaporate_roll,
                            ) {
                                0
                            } else {
                                self.draw_enchant(netrust_core::armor_gain_draw(
                                    spe, item.buc, special, magical,
                                ))
                            };
                            let outcome = netrust_core::enchant_armor(
                                spe,
                                item.buc,
                                special,
                                magical,
                                evaporate_roll,
                                gain_roll,
                            );
                            if let netrust_core::EnchantOutcome::Changed(_) = outcome {
                                // read.c:1115 seffect_enchant_armor: the armor's BUC
                                // follows the scroll (curse / bless / uncurse).
                                if let Some(armor) = self.arena.items.get_mut(aid) {
                                    armor.buc = match (item.buc, armor.buc) {
                                        (Buc::Cursed, _) => Buc::Cursed,
                                        (Buc::Blessed, _) => Buc::Blessed,
                                        (Buc::Uncursed, Buc::Cursed) => Buc::Uncursed,
                                        (Buc::Uncursed, b) => b,
                                    };
                                }
                            }
                            self.apply_enchant_outcome(aid, outcome, true, &mut events);
                        } else {
                            events.push(GameEvent::LogMessage {
                                text: "Your skin feels warm for a moment.".into(),
                            });
                        }
                    } else if item.name.contains("charging") {
                        let wand_id = self
                            .arena
                            .items_carried_by(self.player_id)
                            .into_iter()
                            .find(|&id| {
                                self.arena
                                    .items
                                    .get(id)
                                    .map(|it| it.class == ItemClass::Wand)
                                    .unwrap_or(false)
                            });
                        if let Some(wid) = wand_id {
                            let (wand_name, charges, recharges, wand_blessed) = {
                                let w = self.arena.items.get(wid).unwrap();
                                (
                                    w.name.clone(),
                                    w.enchantment.max(0) as u32,
                                    w.recharged as u32,
                                    w.buc == Buc::Blessed,
                                )
                            };
                            // read.c:737: lim = 1 wishing, 8 directional, 15 non-directional.
                            let is_wishing = wand_name.contains("wishing");
                            // Catalog `wand_dir` (objects.h oc_dir); wands outside the catalog
                            // fall back to the NODIR name list.
                            let nodir = match self.ruleset.item(&wand_name) {
                                Some(a) => a.wand_dir == Some(netrust_data::WandDir::NoDir),
                                None => [
                                    "light",
                                    "secret door detection",
                                    "create monster",
                                    "enlightenment",
                                ]
                                .iter()
                                .any(|k| wand_name.contains(k)),
                            };
                            let lim: u32 = if is_wishing {
                                1
                            } else if nodir {
                                15
                            } else {
                                8
                            };
                            // read.c:741: rn2(343) is drawn only when n > 0 && !wishing
                            // (C short-circuits); then rn1(5, lim-4) and, uncursed, rnd(n).
                            let roll_343 =
                                if netrust_core::artifacts_wands::recharge_needs_explosion_roll(
                                    recharges, is_wishing,
                                ) {
                                    self.rng.random_range(0..343u32)
                                } else {
                                    0
                                };
                            let rn5 = if lim > 1 && item.buc != Buc::Cursed {
                                self.rng.random_range(0..5u32)
                            } else {
                                0
                            };
                            let rnd_roll = if lim > 1 && item.buc == Buc::Uncursed {
                                self.rng.random_range(1..=(lim - 4 + rn5))
                            } else {
                                1
                            };
                            let wand_state = netrust_types::WandCharges { charges, recharges };
                            match netrust_core::artifacts_wands::recharge_wand(
                                wand_state,
                                item.buc,
                                lim,
                                is_wishing,
                                wand_blessed,
                                roll_343,
                                rn5,
                                rnd_roll,
                            ) {
                                netrust_types::RechargeResult::Exploded => {
                                    self.arena.destroy_item(wid);
                                    // read.c:763 wand_explode(obj, rnd(lim)); :2420-2445 dmg = d(n, k)
                                    // with n = max(2, spe + chg).
                                    let chg = self.rng.random_range(1..=lim);
                                    let dice = netrust_core::artifacts_wands::wand_explode_dice(
                                        charges as i32,
                                        chg,
                                    );
                                    let k = netrust_core::artifacts_wands::wand_explode_die_size(
                                        &wand_name,
                                    );
                                    let dmg: i32 = (0..dice)
                                        .map(|_| self.rng.random_range(1..=k) as i32)
                                        .sum();
                                    if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                                        p.hp = p.hp.saturating_sub(dmg.max(0) as u32);
                                        if p.hp == 0 {
                                            p.is_dead = true;
                                        }
                                    }
                                    events.push(GameEvent::LogMessage {
                                        text: netrust_i18n::Messages::wand_exploded(
                                            &wand_name,
                                            self.locale,
                                        ),
                                    });
                                }
                                netrust_types::RechargeResult::Success(new_w) => {
                                    if let Some(w_mut) = self.arena.items.get_mut(wid) {
                                        let charges_now = new_w.charges;
                                        w_mut.enchantment = charges_now.min(i8::MAX as u32) as i8;
                                        w_mut.recharged = new_w.recharges.min(u8::MAX as u32) as u8;
                                        events.push(GameEvent::LogMessage {
                                            text: netrust_i18n::Messages::wand_recharged(
                                                &wand_name,
                                                charges_now,
                                                new_w.recharges,
                                                self.locale,
                                            ),
                                        });
                                    }
                                }
                            }
                        } else {
                            events.push(GameEvent::LogMessage {
                                text: "You have no wands to recharge.".into(),
                            });
                        }
                    } else if item.name.contains("genocide") {
                        self.conducts.genocideless = false;
                        match item.buc {
                            Buc::Cursed => {
                                let summon_count =
                                    netrust_core::genocide::cursed_genocide_summon_count();
                                let player_c = self
                                    .arena
                                    .actors
                                    .get(self.player_id)
                                    .map(|p| p.coord)
                                    .unwrap_or(Coord::new_unchecked(1, 1));
                                for _ in 0..summon_count {
                                    let spawn_c = player_c
                                        .neighbors()
                                        .into_iter()
                                        .find(|&c| {
                                            self.level.is_passable(c) && self.actor_at(c).is_none()
                                        })
                                        .unwrap_or(player_c);
                                    if let Some(mut mon) = self.ruleset.create_monster_record_by_id(
                                        netrust_data::MonsterSpeciesId::Goblin,
                                        spawn_c,
                                    ) {
                                        mon.name = "hostile goblin".into();
                                        mon.is_peaceful = false;
                                        if let Some(def) = self
                                            .ruleset
                                            .monster_by_id(netrust_data::MonsterSpeciesId::Goblin)
                                        {
                                            self.set_monster_malign(&mut mon, def);
                                        }
                                        self.arena.spawn_actor(mon);
                                    }
                                }
                                events.push(GameEvent::LogMessage {
                                    text:
                                        "You read the cursed scroll of genocide. Monsters appear!"
                                            .into(),
                                });
                            }
                            Buc::Uncursed => {
                                let target =
                                    netrust_types::GenocideTarget::Species("goblin".to_string());
                                netrust_core::genocide::apply_genocide(
                                    &mut self.genocide_registry,
                                    target.clone(),
                                );
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
                                events.push(GameEvent::LogMessage {
                                    text:
                                        "You read the scroll of genocide. A species is wiped out!"
                                            .into(),
                                });
                            }
                            Buc::Blessed => {
                                let target = netrust_types::GenocideTarget::Class('L'); // Lich class for example
                                netrust_core::genocide::apply_genocide(
                                    &mut self.genocide_registry,
                                    target.clone(),
                                );
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
                        events.push(GameEvent::LogMessage {
                            text: format!("You read the {}. Knowledge fills your mind!", item.name),
                        });
                    }
                    self.scheduler.hero_act(NORMAL_SPEED);
                } else if item.class == ItemClass::Spellbook {
                    netrust_core::conducts::record_read(&mut self.conducts);
                    if item.name.contains("Book of the Dead") {
                        let player_coord = self
                            .arena
                            .actors
                            .get(self.player_id)
                            .map(|p| p.coord)
                            .unwrap_or(Coord::new_unchecked(0, 0));
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
                        events.push(GameEvent::LogMessage {
                            text: format!("You study the {} and memorize the spell!", item.name),
                        });
                    }
                    self.scheduler.hero_act(NORMAL_SPEED);
                } else {
                    events.push(GameEvent::LogMessage {
                        text: "You can only read scrolls or spellbooks!".into(),
                    });
                }
            }
        } else {
            events.push(GameEvent::LogMessage {
                text: "You have no such scroll or book to read.".into(),
            });
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
                    // Catalog `oc_nutrition` (objects.h FOOD); corpses (catalog 0, C takes it
                    // from the monster) and uncatalogued food keep the flat 400.
                    let nut_gain = match self.ruleset.item(&item.name) {
                        Some(a) if a.nutrition > 0 => a.nutrition as i32,
                        _ => {
                            if item.name.contains("ration") {
                                800
                            } else if item.name.contains("apple") {
                                50
                            } else {
                                400 // corpse
                            }
                        }
                    };
                    self.player_nutrition = (self.player_nutrition + nut_gain).min(2000);

                    let is_meat = self
                        .ruleset
                        .item(&item.name)
                        .map(|i| i.is_meat())
                        .unwrap_or_else(|| item.name.contains("corpse"));
                    if is_meat {
                        netrust_core::conducts::record_eat_meat(&mut self.conducts);
                        let corpse_race = item.corpse_race.as_deref().unwrap_or("unknown");
                        if netrust_core::nutrition::is_cannibalism(corpse_race, "human") {
                            events.push(GameEvent::LogMessage {
                                text: "You cannibal! You feel deeply ashamed.".into(),
                            });
                        }
                        if netrust_core::nutrition::is_corpse_tainted(
                            item.corpse_age,
                            item.rot_threshold,
                        ) {
                            events.push(GameEvent::LogMessage {
                                text: "Ugh, this corpse is tainted!".into(),
                            });
                        }

                        let monster_name = item.name.replace(" corpse", "");
                        if let Some(intrinsic) =
                            netrust_core::nutrition::intrinsic_from_corpse(&monster_name)
                        {
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
                            events.push(GameEvent::LogMessage {
                                text: format!("You gained {intrinsic}!"),
                            });
                        }

                        if item.name.contains("lizard") {
                            netrust_core::afflictions::cure_petrification(&mut self.hero);
                            events.push(GameEvent::LogMessage {
                                text: "You eat the lizard corpse. You feel limber!".into(),
                            });
                        } else if item.name.contains("ant")
                            || item.name.contains("kobold")
                            || item.name.contains("orc")
                        {
                            if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                                p.intrinsics.poison_resistance = true;
                            }
                            events.push(GameEvent::LogMessage {
                                text: "You feel a healthy warmth suffuse your body! You gained Poison Resistance!".into(),
                            });
                        }
                    }
                    events.push(GameEvent::LogMessage {
                        text: format!("You eat the {}. Delicious!", item.name),
                    });
                    self.scheduler.hero_act(NORMAL_SPEED);
                } else {
                    events.push(GameEvent::LogMessage {
                        text: "That is not edible!".into(),
                    });
                }
            }
        } else {
            events.push(GameEvent::LogMessage {
                text: "You have nothing to eat in that slot.".into(),
            });
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
                events.push(GameEvent::LogMessage {
                    text: format!(
                        "You don't have enough mana! Requires {} Pw, you have {}.",
                        cost, self.player_pw
                    ),
                });
            } else {
                self.player_pw -= cost;
                match spell {
                    SpellKind::ForceBolt | SpellKind::MagicMissile => {
                        let path = trace_beam_path(&self.level, player.coord, dir, 6);
                        events.push(GameEvent::BeamPropagated { path: path.clone() });
                        let spell_damage = if spell == SpellKind::ForceBolt {
                            18
                        } else {
                            14
                        };
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
                                    }
                                    if self.arena.actors.get(target_id).is_some_and(|t| t.is_dead) {
                                        let name = self
                                            .arena
                                            .actors
                                            .get(target_id)
                                            .map(|a| a.name.as_str())
                                            .unwrap_or("The monster");
                                        events.push(GameEvent::LogMessage {
                                            text: format!("{name} is slain by magic!"),
                                        });
                                        self.on_actor_killed(
                                            self.player_id,
                                            target_id,
                                            &mut events,
                                        );
                                    }
                                    // C bhitm (zap.c:552-554, force bolt) and buzz
                                    // (zap.c:4948, magic missile): a surviving
                                    // target is woken with `wakeup(mon, TRUE)`.
                                    if self.arena.actors.get(target_id).is_some_and(|t| !t.is_dead)
                                    {
                                        self.setmangry(target_id, &mut events);
                                    }
                                    break;
                                }
                            }
                        }
                        events.push(GameEvent::LogMessage {
                            text: format!("You cast {spell:?}!"),
                        });
                    }
                    SpellKind::CureLightWounds => {
                        if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                            p.hp = (p.hp + 12).min(p.max_hp);
                        }
                        events.push(GameEvent::LogMessage {
                            text: "You cast Cure Light Wounds! Your wounds close.".into(),
                        });
                    }
                    SpellKind::ExtraHealing => {
                        if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                            p.hp = (p.hp + 30).min(p.max_hp);
                        }
                        events.push(GameEvent::LogMessage {
                            text: "You cast Extra Healing! Divine vitality surges through you."
                                .into(),
                        });
                    }
                }
                self.scheduler.hero_act(NORMAL_SPEED);
            }
        } else {
            events.push(GameEvent::LogMessage {
                text: "You do not know that spell.".into(),
            });
        }
        events
    }

    pub(crate) fn handle_zap_wand(&mut self, dir: Direction, energy: u32) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let Some(player) = self.arena.actors.get(self.player_id).cloned() else {
            return events;
        };

        // Find carried wand (or default)
        let wand_id = self
            .arena
            .items_carried_by(self.player_id)
            .into_iter()
            .find(|&id| {
                self.arena
                    .items
                    .get(id)
                    .map(|it| it.class == ItemClass::Wand)
                    .unwrap_or(false)
            });

        let Some(wid) = wand_id else {
            events.push(GameEvent::LogMessage {
                text: "You have no wand to zap.".into(),
            });
            return events;
        };
        let Some(wand_item) = self.arena.items.get_mut(wid) else {
            return events;
        };
        let current_charges = netrust_types::WandCharges {
            charges: wand_item.enchantment.max(0) as u32,
            recharges: wand_item.recharged as u32,
        };
        let wand_name =
            if let Some(new_charges) = netrust_core::artifacts_wands::zap_wand(current_charges) {
                wand_item.enchantment = new_charges.charges.min(i8::MAX as u32) as i8;
                wand_item.name.clone()
            } else {
                events.push(GameEvent::LogMessage {
                    text: netrust_i18n::Messages::wand_empty(self.locale).into(),
                });
                self.scheduler.hero_act(NORMAL_SPEED);
                return events;
            };

        // If Wand of Secret Door Detection: reveals secret doors in 5x5 radius
        if wand_name.contains("secret door") {
            let mut revealed = 0;
            for dy in -3..=3 {
                for dx in -3..=3 {
                    let nx = player.coord.x as i32 + dx;
                    let ny = player.coord.y as i32 + dy;
                    if nx >= 0
                        && nx < netrust_types::COLNO as i32
                        && ny >= 0
                        && ny < netrust_types::ROWNO as i32
                    {
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
            if tx >= 0
                && tx < netrust_types::COLNO as i32
                && ty >= 0
                && ty < netrust_types::ROWNO as i32
            {
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
            } else if wand_name.contains("cold")
                && matches!(current_tile, netrust_types::Tile::Pool { .. })
            {
                self.level
                    .set_tile(coord, netrust_types::Tile::Pool { frozen: true });
                events.push(GameEvent::LogMessage {
                    text: netrust_i18n::Messages::pool_frozen(self.locale).into(),
                });
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
                        if wand_name.contains("digging") || wand_name.contains("teleport") {
                            // C: zap_dig (zap.c:3459) never hurts monsters, and wand of
                            // teleportation relocates them (u_teleport_mon, not modelled);
                            // neither deals beam damage.
                            events.push(GameEvent::LogMessage {
                                text: format!(
                                    "The {} has no effect on {}.",
                                    wand_name, target.name
                                ),
                            });
                        } else if wand_name.contains("polymorph") {
                            if !target.is_unique && !target.is_player {
                                // transform monster
                                let new_species = netrust_data::MonsterSpeciesId::Goblin; // simplified
                                if let Some(new_arch) = self.ruleset.monster_by_id(new_species) {
                                    target.name = new_arch.name.to_string();
                                    target.hp = new_arch.base_hp;
                                    target.max_hp = new_arch.max_hp;
                                    target.ac = new_arch.ac;
                                    target.speed = new_arch.speed;
                                    target.level = new_arch.level;
                                }
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
                    }
                    if self.arena.actors.get(target_id).is_some_and(|t| t.is_dead) {
                        let name = self
                            .arena
                            .actors
                            .get(target_id)
                            .map(|a| a.name.as_str())
                            .unwrap_or("The monster");
                        events.push(GameEvent::LogMessage {
                            text: format!("{name} is destroyed by the wand beam!"),
                        });
                        self.on_actor_killed(self.player_id, target_id, &mut events);
                    }
                    // C bhitm (zap.c:552-554) / buzz (zap.c:4948): a surviving
                    // target is woken with `wakeup(mon, TRUE)`; digging is
                    // zap_dig and never touches monsters.
                    if !wand_name.contains("digging")
                        && self.arena.actors.get(target_id).is_some_and(|t| !t.is_dead)
                    {
                        self.setmangry(target_id, &mut events);
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
        let wow_id = self
            .arena
            .items_carried_by(self.player_id)
            .into_iter()
            .find(|&id| {
                self.arena
                    .items
                    .get(id)
                    .map(|it| it.class == ItemClass::Wand && it.name == "wand of wishing")
                    .unwrap_or(false)
            });

        let Some(wid) = wow_id else {
            events.push(GameEvent::LogMessage {
                text: "You have no means of wishing.".into(),
            });
            return events;
        };
        {
            let Some(wand_item) = self.arena.items.get_mut(wid) else {
                return events;
            };
            let charges = wand_item.enchantment.max(0) as u32;
            let recharges = wand_item.recharged as u32;
            if let Some(new_w) =
                netrust_core::artifacts_wands::zap_wand(netrust_types::WandCharges {
                    charges,
                    recharges,
                })
            {
                wand_item.enchantment = new_w.charges.min(i8::MAX as u32) as i8;
            } else {
                events.push(GameEvent::LogMessage {
                    text: netrust_i18n::Messages::wish_empty(self.locale).into(),
                });
                self.scheduler.hero_act(NORMAL_SPEED);
                return events;
            }
        }
        netrust_core::conducts::record_wish(&mut self.conducts);

        if let Some((item_query, ench, buc)) = netrust_core::artifacts_wands::parse_wish(&wish_str)
        {
            let wanted = normalize_wish_name(&item_query);
            let matched_arch = self.ruleset.item(&wanted).cloned();

            match matched_arch {
                Some(arch) if arch.id == Some(ItemKindId::AmuletOfYendor) => {
                    if let Some(mut fake) = self.ruleset.create_item_record(
                        &arch.name,
                        ItemLocation::Floor(player.coord),
                        buc,
                    ) {
                        fake.name = "cheap plastic imitation of the Amulet of Yendor".into();
                        self.arena.spawn_item(fake);
                    }
                    events.push(GameEvent::LogMessage {
                        text: netrust_i18n::Messages::wish_granted(
                            "cheap plastic imitation of the Amulet of Yendor",
                            self.locale,
                        ),
                    });
                }
                Some(arch) if arch.id.map(|id| UNWISHABLE.contains(&id)).unwrap_or(false) => {
                    events.push(GameEvent::LogMessage {
                        text: format!(
                            "You feel a vague sense of loss. The {} cannot be wished for.",
                            arch.name
                        ),
                    });
                }
                Some(arch) => {
                    if let Some(mut record) = self.ruleset.create_item_record(
                        &arch.name,
                        ItemLocation::Floor(player.coord),
                        buc,
                    ) {
                        // Wands keep their initial charges; the parsed enchantment applies to other items.
                        if record.class != ItemClass::Wand {
                            record.enchantment = ench;
                        }
                        let spawned_id = self.arena.spawn_item(record);
                        let item_name = self.arena.items.get(spawned_id).unwrap().name.clone();
                        events.push(GameEvent::LogMessage {
                            text: netrust_i18n::Messages::wish_granted(&item_name, self.locale),
                        });
                    } else {
                        events.push(GameEvent::LogMessage {
                            text: format!("You feel a vague sense of loss. You wished for '{wish_str}', but received nothing."),
                        });
                    }
                }
                None => {
                    events.push(GameEvent::LogMessage {
                        text: format!("You feel a vague sense of loss. You wished for '{wish_str}', but received nothing."),
                    });
                }
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

/// True only for the genuine Amulet of Yendor (not the wished-for plastic imitation).
pub fn is_real_amulet(item: &netrust_arena::ItemRecord) -> bool {
    item.name == "Amulet of Yendor"
}
