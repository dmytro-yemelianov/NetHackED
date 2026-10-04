//! Melee combat resolution and damage application.

use netrust_arena::{ActorId, ItemLocation};
use netrust_core::{combat::resolve_melee_attack, Combatant};
use netrust_data::{create_item_record, ItemKindId};
use netrust_types::Buc;
use rand::{Rng, RngCore};

use crate::events::GameEvent;
use crate::world::SimulationWorld;

impl SimulationWorld {
    pub(crate) fn resolve_combat(
        &mut self,
        attacker_id: ActorId,
        defender_id: ActorId,
    ) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let Some(attacker) = self.arena.actors.get(attacker_id).cloned() else {
            return events;
        };
        let Some(defender) = self.arena.actors.get(defender_id).cloned() else {
            return events;
        };

        // Check defender armor enchantment
        let armor_ench: i32 = self
            .arena
            .items_carried_by(defender_id)
            .into_iter()
            .filter_map(|id| self.arena.items.get(id))
            .filter(|it| it.class == netrust_types::ItemClass::Armor)
            .map(|a| a.enchantment as i32)
            .sum();

        let def_combat = Combatant {
            hp: defender.hp,
            max_hp: defender.max_hp,
            ac: defender.ac - armor_ench,
            level: defender.level,
            to_hit_bonus: 0,
            damage_bonus: 0,
            is_dead: defender.is_dead,
        };

        // Check attacker wielded weapon enchantment and signature artifact
        let (weapon_ench, artifact) = if attacker_id == self.player_id {
            if let Some(wid) = self.wielded_item {
                if let Some(w) = self.arena.items.get(wid) {
                    let art = match w.name.to_lowercase().as_str() {
                        "excalibur" => Some(netrust_types::ArtifactKind::Excalibur),
                        "vorpal blade" => Some(netrust_types::ArtifactKind::VorpalBlade),
                        "mjollnir" => Some(netrust_types::ArtifactKind::Mjollnir),
                        "magicbane" => Some(netrust_types::ArtifactKind::Magicbane),
                        "the eye of the aethiopica" | "eye of the aethiopica" => {
                            Some(netrust_types::ArtifactKind::EyeOfTheAethiopica)
                        }
                        _ => None,
                    };
                    (w.enchantment as i32, art)
                } else {
                    (0, None)
                }
            } else {
                (0, None)
            }
        } else {
            (0, None)
        };

        let mut skill_hit_bonus = 0;
        let mut skill_dmg_bonus = 0;
        if attacker_id == self.player_id {
            if let Some(wid) = self.wielded_item {
                if let Some(w) = self.arena.items.get(wid) {
                    let sk = match w.name.to_lowercase().as_str() {
                        n if n.contains("dagger") => Some(netrust_types::SkillClass::Dagger),
                        n if n.contains("long sword") => Some(netrust_types::SkillClass::LongSword),
                        n if n.contains("short sword") => {
                            Some(netrust_types::SkillClass::ShortSword)
                        }
                        n if n.contains("bow") => Some(netrust_types::SkillClass::Bow),
                        n if n.contains("crossbow") => Some(netrust_types::SkillClass::Crossbow),
                        n if n.contains("club") => Some(netrust_types::SkillClass::Club),
                        _ => None,
                    };
                    if let Some(skill_class) = sk {
                        let level = self
                            .hero
                            .skills
                            .skills
                            .get(&skill_class)
                            .copied()
                            .unwrap_or(netrust_types::SkillLevel::Unskilled);
                        skill_hit_bonus = netrust_core::skills::skill_to_hit_bonus(level);
                        skill_dmg_bonus = netrust_core::skills::skill_damage_bonus(level);
                    }
                }
            } else {
                let level = self
                    .hero
                    .skills
                    .skills
                    .get(&netrust_types::SkillClass::BareHanded)
                    .copied()
                    .unwrap_or(netrust_types::SkillLevel::Unskilled);
                skill_hit_bonus = netrust_core::skills::skill_to_hit_bonus(level);
                skill_dmg_bonus = netrust_core::skills::skill_damage_bonus(level);
            }
        }

        let to_hit_bonus = attacker.level as i32 + weapon_ench + skill_hit_bonus;
        let d20 = self.rng.random_range(1..=20u32);
        let dmg_roll = (self.rng.random_range(1..=6i32) + skill_dmg_bonus).max(1) as u32;

        // skill_dmg_bonus is already folded into dmg_roll; pass 0 to avoid double counting.
        let result = resolve_melee_attack(to_hit_bonus, 0, def_combat, d20, dmg_roll, weapon_ench);

        if result.hit {
            let is_demon_or_undead = defender.name.to_lowercase().contains("demon")
                || defender.name.to_lowercase().contains("lich")
                || defender.name.to_lowercase().contains("vampire")
                || defender.name.to_lowercase().contains("zombie")
                || defender.name.to_lowercase().contains("skeleton");

            let mut final_damage = result.damage_dealt;
            if let Some(art) = artifact {
                final_damage = netrust_core::artifacts_wands::resolve_artifact_damage(
                    art,
                    final_damage,
                    is_demon_or_undead,
                );
            }

            let mut lethal = false;
            if let Some(target) = self.arena.actors.get_mut(defender_id) {
                let decap = if artifact == Some(netrust_types::ArtifactKind::VorpalBlade) {
                    (self.rng.next_u32() % 20) == 0
                } else {
                    false
                };

                if decap {
                    events.push(GameEvent::LogMessage {
                        text: netrust_i18n::Messages::vorpal_decapitate(
                            &defender.name,
                            self.locale,
                        ),
                    });
                }

                if defender_id == self.player_id {
                    let mut actual_damage = final_damage as i32;
                    if decap {
                        actual_damage += 9999;
                    } // Force fatal

                    let poly_res =
                        netrust_core::polymorph::apply_poly_damage(&mut self.hero, actual_damage);
                    match poly_res {
                        netrust_core::polymorph::PolyDamageResult::Absorbed => {
                            if let Some(poly) = &self.hero.polymorph {
                                target.hp = poly.hp as u32;
                                target.max_hp = poly.max_hp as u32;
                            } else {
                                target.hp = self.hero.base_hp as u32;
                                target.max_hp = self.hero.base_max_hp as u32;
                            }
                            lethal = false;
                        }
                        netrust_core::polymorph::PolyDamageResult::Reverted {
                            excess_damage: _,
                        } => {
                            target.hp = self.hero.base_hp as u32;
                            target.max_hp = self.hero.base_max_hp as u32;
                            lethal = false;
                            events.push(GameEvent::LogMessage {
                                text: netrust_i18n::Messages::revert_form(self.locale).to_string(),
                            });
                        }
                        netrust_core::polymorph::PolyDamageResult::Dead => {
                            target.hp = 0;
                            target.is_dead = true;
                            lethal = true;
                        }
                    }
                } else {
                    let (new_hp, dead) = if decap {
                        netrust_core::artifacts_wands::apply_vorpal_strike(target.hp, decap)
                    } else {
                        (
                            target.hp.saturating_sub(final_damage),
                            target.hp <= final_damage,
                        )
                    };
                    target.hp = new_hp;
                    target.is_dead = dead || target.hp == 0;
                    lethal = target.is_dead;
                }
            }
            events.push(GameEvent::AttackLanded {
                attacker: attacker_id,
                target: defender_id,
                damage: final_damage,
                lethal,
            });
            let attack_msg = netrust_i18n::Messages::attack_hit(
                &attacker.name,
                &defender.name,
                final_damage,
                self.locale,
            );
            events.push(GameEvent::LogMessage { text: attack_msg });
            if lethal {
                if attacker_id == self.player_id {
                    netrust_core::conducts::record_kill(&mut self.conducts);
                }
                events.push(GameEvent::LogMessage {
                    text: netrust_i18n::Messages::killed(&defender.name, self.locale),
                });
                if defender_id != self.player_id {
                    // Check if the defeated enemy is the unique Class Nemesis
                    let quest_cfg = netrust_core::get_role_quest_config(&self.role_name);
                    if defender.name.eq_ignore_ascii_case(quest_cfg.nemesis_name) {
                        netrust_core::attack_nemesis(&mut self.quest_state, 9999);
                        let art_id = match quest_cfg.role_name.to_lowercase().as_str() {
                            "valkyrie" => ItemKindId::OrbOfFate,
                            "wizard" => ItemKindId::EyeOfTheAethiopica,
                            "barbarian" => ItemKindId::HeartOfAhriman,
                            "knight" => ItemKindId::MagicMirrorOfMerlin,
                            "monk" => ItemKindId::EyesOfTheOverworld,
                            "rogue" => ItemKindId::MasterKeyOfThievery,
                            "tourist" => ItemKindId::PlatinumYendorianExpressCard,
                            "healer" => ItemKindId::StaffOfAesculapius,
                            _ => ItemKindId::OrbOfDetection,
                        };
                        let art_rec = create_item_record(
                            art_id,
                            ItemLocation::Floor(defender.coord),
                            Buc::Blessed,
                        );
                        self.arena.spawn_item(art_rec);
                        events.push(GameEvent::LogMessage {
                            text: netrust_i18n::Messages::quest_nemesis_defeat(
                                quest_cfg.nemesis_name,
                                quest_cfg.artifact_name,
                                self.locale,
                            ),
                        });
                    }

                    let corpse = create_item_record(
                        ItemKindId::Corpse,
                        ItemLocation::Floor(defender.coord),
                        Buc::Uncursed,
                    );
                    self.arena.spawn_item(corpse);
                }
            }
        } else {
            events.push(GameEvent::AttackMissed {
                attacker: attacker_id,
                target: defender_id,
            });
            events.push(GameEvent::LogMessage {
                text: netrust_i18n::Messages::attack_miss(
                    &attacker.name,
                    &defender.name,
                    self.locale,
                ),
            });
        }

        events
    }
}
