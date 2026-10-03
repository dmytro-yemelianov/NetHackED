//! Melee combat resolution and damage application.

use netrust_arena::{ActorId, ItemLocation};
use netrust_core::{combat::resolve_melee_attack, Combatant};
use netrust_data::{create_item_record, ItemKindId};
use netrust_types::Buc;
use rand::RngCore;

use crate::events::GameEvent;
use crate::world::SimulationWorld;

impl SimulationWorld {
    pub(crate) fn resolve_combat(&mut self, attacker_id: ActorId, defender_id: ActorId) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let Some(attacker) = self.arena.actors.get(attacker_id).cloned() else { return events; };
        let Some(defender) = self.arena.actors.get(defender_id).cloned() else { return events; };

        // Check defender armor enchantment
        let armor_ench: i32 = self.arena.items_carried_by(defender_id).into_iter()
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
                        "the eye of the aethiopica" | "eye of the aethiopica" => Some(netrust_types::ArtifactKind::EyeOfTheAethiopica),
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

        let to_hit_bonus = attacker.level as i32 + weapon_ench;
        let d20 = 15; // default representative roll
        let dmg_roll = 6;
        let result = resolve_melee_attack(attacker.level as i32, to_hit_bonus, def_combat, d20, dmg_roll, weapon_ench);

        if result.hit {
            let is_demon_or_undead = defender.name.to_lowercase().contains("demon")
                || defender.name.to_lowercase().contains("lich")
                || defender.name.to_lowercase().contains("vampire")
                || defender.name.to_lowercase().contains("zombie")
                || defender.name.to_lowercase().contains("skeleton");

            let mut final_damage = result.damage_dealt;
            if let Some(art) = artifact {
                final_damage = netrust_core::artifacts_wands::resolve_artifact_damage(art, final_damage, is_demon_or_undead);
            }

            let mut lethal = false;
            if let Some(target) = self.arena.actors.get_mut(defender_id) {
                if artifact == Some(netrust_types::ArtifactKind::VorpalBlade) {
                    // Vorpal decapitation check
                    let decap = (self.rng.next_u32() % 20) == 0;
                    let (new_hp, dead) = netrust_core::artifacts_wands::apply_vorpal_strike(target.hp, decap);
                    if decap {
                        events.push(GameEvent::LogMessage {
                            text: format!("*SNICKER-SNACK!* Vorpal Blade decapitates {}!", defender.name),
                        });
                    }
                    target.hp = new_hp.saturating_sub(final_damage);
                    target.is_dead = dead || target.hp == 0;
                    lethal = target.is_dead;
                } else {
                    target.hp = target.hp.saturating_sub(final_damage);
                    target.is_dead = target.hp == 0;
                    lethal = target.is_dead;
                }
            }
            events.push(GameEvent::AttackLanded {
                attacker: attacker_id,
                target: defender_id,
                damage: final_damage,
                lethal,
            });
            let attack_msg = format!("{} hits {} for {} damage!", attacker.name, defender.name, final_damage);
            events.push(GameEvent::LogMessage { text: attack_msg });
            if lethal {
                events.push(GameEvent::LogMessage { text: format!("{} is killed!", defender.name) });
                if defender_id != self.player_id {
                    let corpse = create_item_record(ItemKindId::Corpse, ItemLocation::Floor(defender.coord), Buc::Uncursed);
                    self.arena.spawn_item(corpse);
                }
            }
        } else {
            events.push(GameEvent::AttackMissed {
                attacker: attacker_id,
                target: defender_id,
            });
            events.push(GameEvent::LogMessage { text: format!("{} misses {}.", attacker.name, defender.name) });
        }

        events
    }
}
