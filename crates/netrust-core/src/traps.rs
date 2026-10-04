use netrust_types::{TrapRecord, TrapState, TrapType};

pub fn is_floor_trap(trap_type: TrapType) -> bool {
    matches!(
        trap_type,
        TrapType::Pit | TrapType::SpikedPit | TrapType::Web
    )
}

pub fn can_trigger_trap(trap: &TrapRecord, is_flying: bool) -> bool {
    if trap.state == TrapState::Disarmed {
        return false;
    }
    if is_flying && is_floor_trap(trap.trap_type) {
        return false;
    }
    true
}

pub fn trigger_trap(trap: &mut TrapRecord, is_flying: bool) -> Option<TrapType> {
    if can_trigger_trap(trap, is_flying) {
        if trap.state == TrapState::Hidden {
            trap.state = TrapState::Revealed;
        }
        Some(trap.trap_type)
    } else {
        None
    }
}

pub fn disarm_trap(trap: &mut TrapRecord) -> bool {
    if trap.state != TrapState::Disarmed {
        trap.state = TrapState::Disarmed;
        true
    } else {
        false
    }
}
