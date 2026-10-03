//! Graveyard bones files and cross-run persistence mechanics.
//!
//! Formally defined and verified in Lean 4 (`NetMechanics.Bones`).

use netrust_types::Buc;

/// Corrupts any item's BUC state into strictly `Cursed` upon adventurer death.
pub fn corrupt_buc_on_death(_buc: Buc) -> Buc {
    Buc::Cursed
}

/// Computes ghost maximum HP from former adventurer's maximum HP clamped to at least 1.
pub fn create_ghost_hp(former_max_hp: u32) -> u32 {
    former_max_hp.max(1)
}

/// Checks whether a dungeon depth is eligible for bones file creation (depth >= 1).
pub fn is_valid_bones_level(depth: u32) -> bool {
    depth >= 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_corrupt_buc_on_death() {
        assert_eq!(corrupt_buc_on_death(Buc::Blessed), Buc::Cursed);
        assert_eq!(corrupt_buc_on_death(Buc::Uncursed), Buc::Cursed);
        assert_eq!(corrupt_buc_on_death(Buc::Cursed), Buc::Cursed);
    }

    #[test]
    fn test_create_ghost_hp() {
        assert_eq!(create_ghost_hp(0), 1);
        assert_eq!(create_ghost_hp(20), 20);
        assert_eq!(create_ghost_hp(100), 100);
    }

    #[test]
    fn test_is_valid_bones_level() {
        assert!(!is_valid_bones_level(0));
        assert!(is_valid_bones_level(1));
        assert!(is_valid_bones_level(5));
    }
}
