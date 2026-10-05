use nethacked_types::{TrapRecord, TrapState, TrapType};

/// Whether a trap is triggered by the floor, i.e. avoided by flying or
/// levitation (NetHack 3.7 `trap.c:1061 floor_trigger`).
///
/// C set: arrow, dart, rock trap, pit, spiked pit, fire trap, sleeping gas
/// trap and rust trap are floor traps. Teleport, level teleport,
/// polymorph, anti-magic and web are not (web is never avoided by flying).
/// Hole, trap door, bear trap, land mine etc. have no `TrapType` variant yet.
pub fn is_floor_trap(trap_type: TrapType) -> bool {
    matches!(
        trap_type,
        TrapType::Arrow
            | TrapType::Dart
            | TrapType::RockFall
            | TrapType::Pit
            | TrapType::SpikedPit
            | TrapType::Fire
            | TrapType::SleepingGas
            | TrapType::Rust
    )
}

/// Whether stepping on `trap` triggers it (NetHack 3.7 `dotrap`,
/// `trap.c:2996-3046`).
///
/// `flying_or_levitating` models `check_in_air` (`trap.c:1086`): it avoids
/// floor traps only. `rn2_5` is the C `rn2(5)` draw (range `0..5`; larger
/// values are reduced mod 5, never panics); a seen (`Revealed`) trap is
/// escaped when it is 0 (`already_seen && !rn2(5)`). Disarmed traps never fire.
pub fn can_trigger_trap(trap: &TrapRecord, flying_or_levitating: bool, rn2_5: u32) -> bool {
    if trap.state == TrapState::Disarmed {
        return false;
    }
    if flying_or_levitating && is_floor_trap(trap.trap_type) {
        return false;
    }
    if trap.state == TrapState::Revealed && rn2_5 % 5 == 0 {
        return false;
    }
    true
}

/// Triggers `trap` if [`can_trigger_trap`] allows (`trap.c:2996-3046`),
/// revealing a hidden trap. Returns the triggered type.
pub fn trigger_trap(
    trap: &mut TrapRecord,
    flying_or_levitating: bool,
    rn2_5: u32,
) -> Option<TrapType> {
    if can_trigger_trap(trap, flying_or_levitating, rn2_5) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Coord;

    fn rec(t: TrapType, st: TrapState) -> TrapRecord {
        TrapRecord {
            id: 0,
            trap_type: t,
            state: st,
            coord: Coord::new_unchecked(0, 0),
        }
    }

    #[test]
    fn floor_set_matches_c_table() {
        use TrapType::*;
        for t in [
            Arrow,
            Dart,
            RockFall,
            Pit,
            SpikedPit,
            Fire,
            SleepingGas,
            Rust,
        ] {
            assert!(is_floor_trap(t), "{t:?}");
        }
        for t in [Teleport, LevelTeleport, Polymorph, AntiMagic, Web] {
            assert!(!is_floor_trap(t), "{t:?}");
        }
    }

    #[test]
    fn flying_avoids_arrow_not_web() {
        assert!(!can_trigger_trap(
            &rec(TrapType::Arrow, TrapState::Hidden),
            true,
            1
        ));
        assert!(can_trigger_trap(
            &rec(TrapType::Web, TrapState::Hidden),
            true,
            1
        ));
    }

    #[test]
    fn seen_trap_escaped_iff_roll_zero() {
        let t = rec(TrapType::Teleport, TrapState::Revealed);
        assert!(!can_trigger_trap(&t, false, 0));
        for r in 1..5 {
            assert!(can_trigger_trap(&t, false, r));
        }
        // Hidden traps are never escaped by the roll.
        assert!(can_trigger_trap(
            &rec(TrapType::Teleport, TrapState::Hidden),
            false,
            0
        ));
    }
}
