//! Action execution and dispatch across modular domain handlers.

pub mod doors;
pub mod economy;
pub mod engrave;
pub mod inventory;
pub mod items;
pub mod movement;
pub mod religion;
pub mod stairs;

use netrust_core::{energy::NORMAL_SPEED, ActionAst};

use crate::events::GameEvent;
use crate::world::SimulationWorld;

impl SimulationWorld {
    /// Step the simulation with a player action, processing subsequent monster turns.
    pub fn step_player_action(&mut self, action: ActionAst) -> Vec<GameEvent> {
        let mut events = Vec::new();

        let Some(player) = self.arena.actors.get(self.player_id).cloned() else {
            return events;
        };

        if player.is_dead {
            return events;
        }

        match action {
            ActionAst::Move(dir) => events.extend(self.handle_move(dir)),
            ActionAst::OpenDoor(coord) => events.extend(self.handle_open_door(coord)),
            ActionAst::CloseDoor(coord) => events.extend(self.handle_close_door(coord)),
            ActionAst::Kick(coord) => events.extend(self.handle_kick(coord)),
            ActionAst::MeleeAttack(coord) => events.extend(self.handle_melee_attack(coord)),
            ActionAst::Descend => events.extend(self.handle_descend()),
            ActionAst::Ascend => events.extend(self.handle_ascend()),
            ActionAst::PickUp => events.extend(self.handle_pickup()),
            ActionAst::Drop(idx) => events.extend(self.handle_drop(idx)),
            ActionAst::Wield(idx) => events.extend(self.handle_wield(idx)),
            ActionAst::PutInContainer { item_index, container_index } => {
                events.extend(self.handle_put_in_container(item_index, container_index))
            }
            ActionAst::TakeFromContainer { container_index, item_index } => {
                events.extend(self.handle_take_from_container(container_index, item_index))
            }
            ActionAst::Dip { item_index, into_water } => events.extend(self.handle_dip(item_index, into_water)),
            ActionAst::Quaff(idx) => events.extend(self.handle_quaff(idx)),
            ActionAst::Read(idx) => events.extend(self.handle_read(idx)),
            ActionAst::Eat(idx) => events.extend(self.handle_eat(idx)),
            ActionAst::Cast { spell_index, dir } => events.extend(self.handle_cast(spell_index, dir)),
            ActionAst::ZapWand { dir, energy } => events.extend(self.handle_zap_wand(dir, energy)),
            ActionAst::Pray => events.extend(self.handle_pray()),
            ActionAst::Sacrifice(idx) => events.extend(self.handle_sacrifice(idx)),
            ActionAst::Pay => events.extend(self.handle_pay()),
            ActionAst::Engrave { text, medium } => events.extend(self.handle_engrave(text, medium)),
            ActionAst::Wait => {
                self.scheduler.hero_act(NORMAL_SPEED);
            }
        }

        // Process monster actions and turn scheduler ticks
        let sim_events = self.process_turn_ticks();
        events.extend(sim_events);

        self.event_log.extend(events.clone());
        events
    }
}
