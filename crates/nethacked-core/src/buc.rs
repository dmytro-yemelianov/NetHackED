//! BUC (Blessed / Uncursed / Cursed) Mechanics.
//!
//! Formalized and verified in `NetMechanics.BUC`.
//! In C NetHack (obj.h), bitfields `Bitfield(cursed, 1)` and `Bitfield(blessed, 1)`
//! allow contradictory states. In Rust, `Buc` is an enum with mutually exclusive variants.

pub use nethacked_types::{Buc, WaterType};

/// Dipping an item into water.
/// Proved idempotent in Lean 4 (`dip_holy_idempotent`, `dip_plain_always_uncursed`).
pub fn dip_water(water: WaterType, _item: Buc) -> Buc {
    match water {
        WaterType::Holy => Buc::Blessed,
        WaterType::Plain => Buc::Uncursed,
        WaterType::Unholy => Buc::Cursed,
    }
}

/// Reading an uncursing scroll or effect.
/// Proved idempotent in Lean 4 (`uncurse_idempotent`, `uncurse_never_cursed`).
pub fn uncurse(item: Buc) -> Buc {
    match item {
        Buc::Cursed => Buc::Uncursed,
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buc_mutual_exclusion() {
        let b = Buc::Blessed;
        assert!(b.is_blessed());
        assert!(!b.is_cursed());
        assert!(!b.is_uncursed());
    }

    #[test]
    fn test_dip_water_idempotence() {
        for buc in [Buc::Blessed, Buc::Uncursed, Buc::Cursed] {
            assert_eq!(dip_water(WaterType::Holy, buc), Buc::Blessed);
            assert_eq!(dip_water(WaterType::Plain, buc), Buc::Uncursed);
            assert_eq!(dip_water(WaterType::Unholy, buc), Buc::Cursed);
        }
    }

    #[test]
    fn test_uncurse_idempotence() {
        for buc in [Buc::Blessed, Buc::Uncursed, Buc::Cursed] {
            let once = uncurse(buc);
            let twice = uncurse(once);
            assert_eq!(once, twice);
            assert!(!once.is_cursed());
        }
    }
}
