use netrust_arena::{ActorId, ItemLocation};
use netrust_core::{
    energy::NORMAL_SPEED,
    ranged::{can_mount, resolve_projectile_impact},
};
use netrust_types::{Direction, MountState, SlotId};
use rand::Rng;

use crate::{events::GameEvent, world::SimulationWorld};

impl SimulationWorld {
    pub fn handle_quiver(&mut self, idx: SlotId) -> Vec<GameEvent> {
        let mut events = Vec::new();
        self.scheduler.hero_act(NORMAL_SPEED);

        // Check if item exists in arena and is carried by player
        let items = self.arena.items_carried_by(self.player_id);
        if items.contains(&idx) {
            self.hero.quivered_item = Some(idx);
            let item_name = self
                .arena
                .items
                .get(idx)
                .map(|i| i.name.as_str())
                .unwrap_or("item");
            events.push(GameEvent::LogMessage {
                text: netrust_i18n::Messages::quiver_success(item_name, self.locale),
            });
        }
        events
    }

    pub fn handle_fire(&mut self, dir: Direction) -> Vec<GameEvent> {
        let mut events = Vec::new();
        self.scheduler.hero_act(NORMAL_SPEED);

        let quivered_item_opt = self.hero.quivered_item;
        if let Some(item_id) = quivered_item_opt {
            let start = self
                .arena
                .actors
                .get(self.player_id)
                .map(|a| a.coord)
                .unwrap();
            let mut current = start;
            let mut target_actor = None;

            for _ in 0..10 {
                if let Some(next) = current.step(dir) {
                    current = next;
                    if let Some(actor_id) = self.actor_at(current) {
                        target_actor = Some(actor_id);
                        break;
                    }
                    if !self.level.is_passable(current) {
                        break;
                    }
                } else {
                    break;
                }
            }

            if let Some(actor_id) = target_actor {
                let damage = 2; // Simple damage for now
                events.push(GameEvent::LogMessage {
                    text: netrust_i18n::Messages::hit_monster(self.locale).into(),
                });

                let lethal = if let Some(target) = self.arena.actors.get_mut(actor_id) {
                    target.hp = target.hp.saturating_sub(damage);
                    target.hp == 0
                } else {
                    false
                };

                events.push(GameEvent::AttackLanded {
                    attacker: self.player_id,
                    target: actor_id,
                    damage,
                    lethal,
                });

                if lethal {
                    self.arena.destroy_actor(actor_id);
                } else if actor_id != self.player_id {
                    // C thitmonst -> hmon -> hmon_hitmon (uhitm.c:1923-1926):
                    // `wakeup(mon, TRUE)` -> setmangry.
                    self.setmangry(actor_id, &mut events);
                }

                // Breakage check
                let roll = self.rng.random_range(0..100);
                if resolve_projectile_impact(25, roll) {
                    events.push(GameEvent::LogMessage {
                        text: netrust_i18n::Messages::projectile_breaks(self.locale).into(),
                    });
                    self.arena.destroy_item(item_id);
                    if true {
                        let player = &mut self.hero;
                        player.quivered_item = None;
                    }
                } else {
                    if let Some(item) = self.arena.items.get_mut(item_id) {
                        item.location = ItemLocation::Floor(current);
                    }
                    if true {
                        let player = &mut self.hero;
                        player.quivered_item = None;
                    }
                }
            } else {
                events.push(GameEvent::LogMessage {
                    text: netrust_i18n::Messages::projectile_misses(self.locale).into(),
                });
                if let Some(item) = self.arena.items.get_mut(item_id) {
                    item.location = ItemLocation::Floor(current);
                }
                if true {
                    let player = &mut self.hero;
                    player.quivered_item = None;
                }
            }
        } else {
            events.push(GameEvent::LogMessage {
                text: netrust_i18n::Messages::quiver_empty(self.locale).into(),
            });
        }

        events
    }

    pub fn handle_mount(&mut self, actor_id: ActorId) -> Vec<GameEvent> {
        let mut events = Vec::new();
        self.scheduler.hero_act(NORMAL_SPEED);

        if let Some(target) = self.arena.actors.get(actor_id) {
            if can_mount(target.is_tame, true) {
                // simplified saddle check
                if true {
                    let player = &mut self.hero;
                    player.mount = Some(MountState {
                        steed_id: actor_id,
                        saddle_equipped: true,
                    });
                    events.push(GameEvent::LogMessage {
                        text: netrust_i18n::Messages::mount_steed(self.locale).into(),
                    });
                }
            } else {
                events.push(GameEvent::LogMessage {
                    text: netrust_i18n::Messages::cannot_mount(self.locale).into(),
                });
            }
        }

        events
    }

    pub fn handle_dismount(&mut self) -> Vec<GameEvent> {
        let mut events = Vec::new();
        self.scheduler.hero_act(NORMAL_SPEED);

        if true {
            let player = &mut self.hero;
            if player.mount.is_some() {
                player.mount = None;
                events.push(GameEvent::LogMessage {
                    text: netrust_i18n::Messages::dismount_steed(self.locale).into(),
                });
            } else {
                events.push(GameEvent::LogMessage {
                    text: netrust_i18n::Messages::not_mounted(self.locale).into(),
                });
            }
        }

        events
    }
}
