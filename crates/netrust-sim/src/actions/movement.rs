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

        let (dx, dy) = dir.delta();
        let nx = player.coord.x as isize + dx;
        let ny = player.coord.y as isize + dy;

        if let Some(target_coord) = Coord::new(nx as usize, ny as usize) {
            // Check if monster at target -> Melee Attack
            if let Some(target_id) = self.actor_at(target_coord) {
                let combat_events = self.resolve_combat(self.player_id, target_id);
                events.extend(combat_events);
                self.scheduler.hero_act(NORMAL_SPEED);
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
                        self.scheduler.hero_act(NORMAL_SPEED);
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

                        self.scheduler.hero_act(NORMAL_SPEED);
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
