//! Graveyard bones files and cross-run persistence mechanics.
//!
//! Formally defined and verified in Lean 4 (`NetMechanics.Bones`).

use netrust_types::Buc;

/// BUC of an item after the hero dies and the bones file is built.
///
/// C `bones.c:290-291` (`drop_upon_death`): `if (rn2(5)) curse(otmp);` so 4/5 of items
/// become cursed (blessed included) and 1/5 keep their BUC. Converted quest items are
/// always cursed (`bones.c:173-189`, `resetobjs`).
///
/// `rn2_5` is the `rn2(5)` draw, range `0..=4`; larger values are clamped to 4.
pub fn corrupt_buc_on_death(original: Buc, is_quest_item: bool, rn2_5: u32) -> Buc {
    if is_quest_item || rn2_5.min(4) != 0 {
        Buc::Cursed
    } else {
        original
    }
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
        for b in [Buc::Blessed, Buc::Uncursed, Buc::Cursed] {
            assert_eq!(corrupt_buc_on_death(b, false, 0), b);
            for r in 1..=4 {
                assert_eq!(corrupt_buc_on_death(b, false, r), Buc::Cursed);
            }
            assert_eq!(corrupt_buc_on_death(b, true, 0), Buc::Cursed);
            assert_eq!(corrupt_buc_on_death(b, false, 99), Buc::Cursed);
        }
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
