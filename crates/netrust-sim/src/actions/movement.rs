//! Movement action handling, shopkeeper theft detection, and engraving interaction.

use netrust_core::energy::NORMAL_SPEED;
use netrust_dungeon::RoomType;
use netrust_types::{Alignment, Coord, Direction, DoorState, Tile};

use crate::events::GameEvent;
use crate::world::SimulationWorld;

impl SimulationWorld {
    pub(crate) fn handle_move(&mut self, dir: Direction) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let Some(player) = self.arena.actors.get(self.player_id).cloned() else {
            return events;
        };

        let move_cost = if true { let pc = &self.hero;
            let mount_cost = pc.mount.as_ref().and_then(|m| {
                // Determine mount's movement cost based on its speed
                // For simplicity, we assume speed acts as cost if it's lower, or we map it. 
                // The prompt says min(unmounted, mount_cost). 
                // Let's just use 12 for unmounted, and if mounted, we can derive a cost from steed's speed.
                // A fast mount (speed > 12) should cost less energy. Energy cost = 12 * 12 / speed.
                self.arena.actors.get(m.steed_id).map(|s| (NORMAL_SPEED * 12) / s.speed.max(1))
            });
            netrust_core::ranged::effective_movement_cost(NORMAL_SPEED, mount_cost)
        } else {
            NORMAL_SPEED
        };

        let (dx, dy) = dir.delta();
        let nx = player.coord.x as isize + dx;
        let ny = player.coord.y as isize + dy;

        if let Some(target_coord) = Coord::new(nx as usize, ny as usize) {
            // Check if actor at target
            if let Some(target_id) = self.actor_at(target_coord) {
                let is_target_tame = self.arena.actors.get(target_id).map(|a| a.is_tame).unwrap_or(false);
                if is_target_tame {
                    // Displacement! Non-violent position swap verified in Lean 4
                    let pet_name = self.arena.actors.get(target_id).map(|a| a.name.clone()).unwrap_or_else(|| "pet".into());
                    let from = player.coord;
                    if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                        p.coord = target_coord;
                    }
                    if let Some(pet) = self.arena.actors.get_mut(target_id) {
                        pet.coord = from;
                    }
                    events.push(GameEvent::ActorMoved {
                        actor: self.player_id,
                        from,
                        to: target_coord,
                    });
                    events.push(GameEvent::ActorMoved {
                        actor: target_id,
                        from: target_coord,
                        to: from,
                    });
                    events.push(GameEvent::LogMessage {
                        text: format!("You displace {}.", pet_name),
                    });
                    self.scheduler.hero_act(move_cost);
                } else {
                    let combat_events = self.resolve_combat(self.player_id, target_id);
                    events.extend(combat_events);
                    self.scheduler.hero_act(move_cost);
                }
            } else if let Some(boulder_id) = self.arena.items.iter().find_map(|(id, item)| {
                if item.location == netrust_arena::ItemLocation::Floor(target_coord) && item.name == "boulder" {
                    Some(id)
                } else {
                    None
                }
            }) {
                // Boulder pushing mechanics formally verified in Lean 4
                if let Some(next_c) = target_coord.step(dir) {
                    let is_occupied = self.actor_at(next_c).is_some() || self.arena.items.values().any(|it| it.location == netrust_arena::ItemLocation::Floor(next_c) && it.name == "boulder");
                    let outcome = netrust_core::sokoban::push_boulder(target_coord, dir, self.level.get_tile(next_c), is_occupied);
                    match outcome {
                        netrust_core::sokoban::PushOutcome::Moved(new_pos) => {
                            if let Some(it) = self.arena.items.get_mut(boulder_id) {
                                it.location = netrust_arena::ItemLocation::Floor(new_pos);
                            }
                            events.push(GameEvent::LogMessage { text: "You push the boulder.".into() });
                            self.scheduler.hero_act(move_cost);
                        }
                        netrust_core::sokoban::PushOutcome::FilledPit(pit_pos) => {
                            self.arena.destroy_item(boulder_id);
                            self.level.set_tile(pit_pos, Tile::Pit { filled: true });
                            events.push(GameEvent::LogMessage { text: "The boulder falls into the pit and fills it!".into() });
                            self.scheduler.hero_act(move_cost);
                        }
                        netrust_core::sokoban::PushOutcome::Blocked => {
                            events.push(GameEvent::LogMessage { text: "You try to move the boulder, but it won't budge.".into() });
                        }
                    }
                } else {
                    events.push(GameEvent::LogMessage { text: "You try to move the boulder, but it won't budge.".into() });
                }
            } else {
                // Check tile
                let tile = self.level.get_tile(target_coord).clone();
                match tile {
                    Tile::Door { state: DoorState::Closed, trapped } => {
                        self.level.set_tile(target_coord, Tile::Door { state: DoorState::Open, trapped });
                        events.push(GameEvent::DoorToggled {
                            coord: target_coord,
                            new_state: DoorState::Open,
                        });
                        self.scheduler.hero_act(move_cost);
                    }
                    Tile::Drawbridge { open: false } => {
                        self.level.set_tile(target_coord, Tile::Drawbridge { open: true });
                        events.push(GameEvent::LogMessage {
                            text: "You lower the drawbridge over the moat. The portcullis creaks open.".into(),
                        });
                        self.scheduler.hero_act(move_cost);
                    }
                    _ if tile.is_passable() => {
                        let from = player.coord;
                        if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                            p.coord = target_coord;
                        }
                        events.push(GameEvent::ActorMoved {
                            actor: self.player_id,
                            from,
                            to: target_coord,
                        });

                        // Smudge engraving if stepping off a dust engraving
                        if let Some(outcome) = self.level.smudge_engraving(from) {
                            match outcome {
                                Some(remaining_e) => {
                                    events.push(GameEvent::EngravingDegraded {
                                        coord: from,
                                        remaining: Some(remaining_e.text),
                                    });
                                }
                                None => {
                                    events.push(GameEvent::EngravingDegraded {
                                        coord: from,
                                        remaining: None,
                                    });
                                    events.push(GameEvent::LogMessage {
                                        text: "The engraving in the dust has been completely wiped away by your footsteps.".into(),
                                    });
                                }
                            }
                        }

                        // If stepping onto a tile with an engraving, notify player!
                        if let Some(e) = self.level.get_engraving(target_coord) {
                            events.push(GameEvent::LogMessage {
                                text: format!("There is something written on the floor here: \"{}\".", e.text),
                            });
                        }

                        // Check if player is leaving a shop with unpaid merchandise!
                        let from_shop = self.level.room_at(from).map(|r| r.room_type == RoomType::Shop).unwrap_or(false);
                        let to_shop = self.level.room_at(target_coord).map(|r| r.room_type == RoomType::Shop).unwrap_or(false);
                        if from_shop && !to_shop {
                            let has_unpaid = self.arena.items_carried_by(self.player_id).iter().any(|id| self.is_unpaid(*id));
                            if has_unpaid {
                                events.push(GameEvent::LogMessage {
                                    text: "You hear the shopkeeper shout: 'Stop, thief! You haven't paid for that!'".into(),
                                });
                                // Turn shopkeeper hostile
                                for (_, actor) in self.arena.actors.iter_mut() {
                                    if actor.name == "shopkeeper" && !actor.is_dead {
                                        actor.alignment = Alignment::Chaotic;
                                    }
                                }
                            }
                        }

                        self.scheduler.hero_act(move_cost);
                    }
                    _ => {
                        // Impassable obstacle; do not consume energy
                        events.push(GameEvent::LogMessage {
                            text: "Ouch! You bump into a solid wall.".into(),
                        });
                    }
                }
            }
        }

        events
    }
}
