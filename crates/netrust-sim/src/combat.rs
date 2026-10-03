//! Melee combat resolution and damage application.

use netrust_arena::{ActorId, ItemLocation};
use netrust_core::{combat::resolve_melee_attack, Combatant};
use netrust_data::{create_item_record, ItemKindId};
use netrust_types::Buc;

use crate::events::GameEvent;
use crate::world::SimulationWorld;

impl SimulationWorld {
    pub(crate) fn resolve_combat(&mut self, attacker_id: ActorId, defender_id: ActorId) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let Some(attacker) = self.arena.actors.get(attacker_id).cloned() else { return events; };
        let Some(defender) = self.arena.actors.get(defender_id).cloned() else { return events; };

        let def_combat = Combatant {
            hp: defender.hp,
            max_hp: defender.max_hp,
            ac: defender.ac,
            level: defender.level,
            to_hit_bonus: 0,
            damage_bonus: 0,
            is_dead: defender.is_dead,
        };

        let to_hit_bonus = attacker.level as i32;
        let d20 = 15; // default representative roll
        let dmg_roll = 6;
        let result = resolve_melee_attack(attacker.level as i32, to_hit_bonus, def_combat, d20, dmg_roll, 0);

        if result.hit {
            if let Some(target) = self.arena.actors.get_mut(defender_id) {
                target.hp = result.defender_after.hp;
                target.is_dead = result.defender_after.is_dead;
            }
            events.push(GameEvent::AttackLanded {
                attacker: attacker_id,
                target: defender_id,
                damage: result.damage_dealt,
                lethal: result.defender_after.is_dead,
            });
            let attack_msg = format!("{} hits {} for {} damage!", attacker.name, defender.name, result.damage_dealt);
            events.push(GameEvent::LogMessage { text: attack_msg });
            if result.defender_after.is_dead {
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
