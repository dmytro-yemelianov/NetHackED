//! Immutable current-Rust raw dressing law; no clock or RNG.
use crate::actions::{command_frame as law, equipment::EquipmentState};
use crate::GameEvent;
use nethacked_arena::ActorId;
pub(crate) struct Result {
    pub equipment: EquipmentState,
    pub ac: Option<i32>,
    pub events: Vec<GameEvent>,
}
pub(crate) fn tick(
    e: &EquipmentState,
    objects: &impl law::Objects,
    player: ActorId,
    alive: bool,
    protection: u32,
) -> Result {
    let mut equipment = law::reconcile(e, objects, player);
    let mut events = vec![];
    let mut ac = None;
    if !alive {
        equipment.dressing = None;
    } else if let Some(d) = equipment.dressing.as_mut() {
        d.turns_left = d.turns_left.saturating_sub(1);
        if d.turns_left == 0 {
            let d = equipment.dressing.take().unwrap();
            if !d.putting_on {
                equipment.armor.retain(|(_, id)| *id != d.item);
            }
            events.push(GameEvent::DressingFinished {
                item: d.item,
                putting_on: d.putting_on,
            });
            ac = Some(law::armor_class(&equipment, objects, player, protection));
        }
    }
    Result {
        equipment,
        ac,
        events,
    }
}
