//! Door opening, closing, kicking, and manual melee attack actions.

use netrust_core::energy::NORMAL_SPEED;
use netrust_types::{Coord, DoorState, Tile};

use crate::events::GameEvent;
use crate::world::SimulationWorld;

impl SimulationWorld {
    pub(crate) fn handle_open_door(&mut self, target_coord: Coord) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let Some(player) = self.arena.actors.get(self.player_id).cloned() else {
            return events;
        };

        if player.coord.is_adjacent(target_coord) {
            let tile = self.level.get_tile(target_coord).clone();
            if let Tile::Door { state: DoorState::Closed, trapped } = tile {
                self.level.set_tile(target_coord, Tile::Door { state: DoorState::Open, trapped });
                events.push(GameEvent::DoorToggled {
                    coord: target_coord,
                    new_state: DoorState::Open,
                });
                self.scheduler.hero_act(NORMAL_SPEED);
            } else {
                events.push(GameEvent::LogMessage {
                    text: "There is no closed door there to open.".into(),
                });
            }
        }
        events
    }

    pub(crate) fn handle_close_door(&mut self, target_coord: Coord) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let Some(player) = self.arena.actors.get(self.player_id).cloned() else {
            return events;
        };

        if player.coord.is_adjacent(target_coord) {
            let tile = self.level.get_tile(target_coord).clone();
            if let Tile::Door { state: DoorState::Open, trapped } = tile {
                self.level.set_tile(target_coord, Tile::Door { state: DoorState::Closed, trapped });
                events.push(GameEvent::DoorToggled {
                    coord: target_coord,
                    new_state: DoorState::Closed,
                });
                self.scheduler.hero_act(NORMAL_SPEED);
            } else {
                events.push(GameEvent::LogMessage {
                    text: "There is no open door there to close.".into(),
                });
            }
        }
        events
    }

    pub(crate) fn handle_kick(&mut self, target_coord: Coord) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let Some(player) = self.arena.actors.get(self.player_id).cloned() else {
            return events;
        };

        if player.coord.is_adjacent(target_coord) {
            let tile = self.level.get_tile(target_coord).clone();
            if let Tile::Door { state, .. } = tile {
                if state != DoorState::Broken {
                    self.level.set_tile(target_coord, Tile::Door { state: DoorState::Broken, trapped: false });
                    events.push(GameEvent::DoorToggled {
                        coord: target_coord,
                        new_state: DoorState::Broken,
                    });
                    events.push(GameEvent::LogMessage {
                        text: "As you kick the door, it shatters into splinters!".into(),
                    });
                    self.scheduler.hero_act(NORMAL_SPEED);
                }
            } else {
                events.push(GameEvent::LogMessage {
                    text: "WHAMMM!!! You kick the wall!".into(),
                });
                self.scheduler.hero_act(NORMAL_SPEED);
            }
        }
        events
    }

    pub(crate) fn handle_melee_attack(&mut self, target_coord: Coord) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let Some(player) = self.arena.actors.get(self.player_id).cloned() else {
            return events;
        };

        if player.coord.is_adjacent(target_coord) {
            if let Some(target_id) = self.actor_at(target_coord) {
                let combat_events = self.resolve_combat(self.player_id, target_id);
                events.extend(combat_events);
                self.scheduler.hero_act(NORMAL_SPEED);
            } else {
                events.push(GameEvent::LogMessage {
                    text: "You swing at empty air.".into(),
                });
                self.scheduler.hero_act(NORMAL_SPEED);
            }
        }
        events
    }
}
