//! Floor engraving creation across various media (dust, burn, carve, mark).

use netrust_core::{energy::NORMAL_SPEED, engraving::EngravingMedium};

use crate::events::GameEvent;
use crate::world::SimulationWorld;

impl SimulationWorld {
    pub(crate) fn handle_engrave(
        &mut self,
        text: String,
        medium: EngravingMedium,
    ) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let Some(player) = self.arena.actors.get(self.player_id).cloned() else {
            return events;
        };

        let p_coord = player.coord;
        let engraving = netrust_core::engraving::Engraving::new(text.clone(), medium);
        self.level.set_engraving(p_coord, engraving);
        events.push(GameEvent::EngravingAdded {
            coord: p_coord,
            text: text.clone(),
        });
        let medium_desc = match medium {
            netrust_core::engraving::EngravingMedium::Dust(_) => "in the dust",
            netrust_core::engraving::EngravingMedium::Burned => "with searing fire",
            netrust_core::engraving::EngravingMedium::Carved(_) => "into the stone",
            netrust_core::engraving::EngravingMedium::Marked(_) => "with marking ink",
        };
        events.push(GameEvent::LogMessage {
            text: format!("You write \"{text}\" {medium_desc} on the floor."),
        });
        self.scheduler.hero_act(NORMAL_SPEED);

        events
    }
}
