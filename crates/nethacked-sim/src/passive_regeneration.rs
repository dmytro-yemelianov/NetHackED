//! Current ordinary-form natural hero HP phase.
use nethacked_core::inventory::{capacity_strength, encumbrance_tier, weight_cap, EncumbranceTier};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Actor {
    pub hp: u32,
    pub max_hp: u32,
    pub level: u32,
    pub dead: bool,
    pub levitating: bool,
}

pub(crate) fn eligible(
    actor: Option<Actor>,
    polymorphed: bool,
    moved: bool,
    strength: i32,
    con: i32,
    carried: Option<u32>,
) -> bool {
    let Some(a) = actor else { return false };
    if a.dead || a.hp == 0 || a.hp >= a.max_hp || polymorphed {
        return false;
    }
    if moved {
        let Some(weight) = carried else { return false };
        let capacity = weight_cap(
            capacity_strength(strength),
            con.clamp(3, 25),
            a.levitating,
            0,
        );
        if encumbrance_tier(weight, capacity) >= EncumbranceTier::Stressed {
            return false;
        }
    }
    true
}

pub(crate) fn heal(level: u32, con: i32, draw: u32) -> u32 {
    u32::from(i64::from(level) + i64::from(con) > i64::from(draw))
}

pub(crate) fn hp_after(actor: Actor, con: i32, draw: u32) -> u32 {
    actor
        .hp
        .saturating_add(heal(actor.level, con, draw))
        .min(actor.max_hp)
}
