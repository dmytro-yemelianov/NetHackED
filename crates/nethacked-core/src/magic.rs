//! Magic spells and power (mana) expenditure.
//!
//! Modeled in Lean 4 (`NetMechanics.Magic`).

use serde::{Deserialize, Serialize};

/// Discrete spells available to casters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SpellKind {
    ForceBolt,
    MagicMissile,
    CureLightWounds,
    ExtraHealing,
}

/// Power (mana) cost to cast a spell.
pub fn mana_cost(spell: SpellKind) -> u32 {
    match spell {
        SpellKind::ForceBolt => 5,
        SpellKind::MagicMissile => 10,
        SpellKind::CureLightWounds => 5,
        SpellKind::ExtraHealing => 15,
    }
}

/// Check if caster has sufficient power to cast spell.
pub fn can_cast(current_pw: u32, spell: SpellKind) -> bool {
    current_pw >= mana_cost(spell)
}

/// Deduct mana cost if caster has sufficient power.
pub fn cast_spell(current_pw: u32, spell: SpellKind) -> Option<u32> {
    if can_cast(current_pw, spell) {
        Some(current_pw - mana_cost(spell))
    } else {
        None
    }
}

/// Turn-based spellbook retention memory decay.
pub fn decay_retention(retention: u32) -> u32 {
    retention.saturating_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spell_casting_mana() {
        assert!(can_cast(10, SpellKind::ForceBolt));
        assert_eq!(cast_spell(10, SpellKind::ForceBolt), Some(5));

        assert!(!can_cast(3, SpellKind::ForceBolt));
        assert_eq!(cast_spell(3, SpellKind::ForceBolt), None);
    }
}
