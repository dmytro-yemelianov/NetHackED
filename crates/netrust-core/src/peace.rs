//! NetHack 5.0 C `peace_minded(ptr)` (`src/makemon.c:2268-2308`).

/// Determines whether a newly spawned monster is peaceful toward the hero.
///
/// NetHack 5.0 C `makemon.c:2268-2308`:
/// 1. `always_peaceful` (`arch_peaceful_by_default`: shopkeeper, watchman, priest,
///    quest leader, quest guardian) -> `true`.
/// 2. `always_hostile` (e.g. quest nemesis) -> `false`.
/// 3. Alignment match: `sgn(mal) != sgn(ual)` -> `false` (cross-aligned is hostile).
/// 4. Co-aligned: C draws `!!rn2(16 + max(-15, record)) && !!rn2(2 + abs(mal))`.
///    Out of `a * b` possible outcomes (where `a = 16 + max(-15, record)` and
///    `b = 2 + abs(mal)`), exactly `(a - 1) * (b - 1)` are peaceful (both draws non-zero).
///    Tested via `coaligned_roll < (a - 1) * (b - 1)`.
#[inline]
pub fn peace_minded(
    arch_peaceful_by_default: bool,
    always_hostile: bool,
    monster_alignment: i32,
    hero_alignment: i32,
    hero_align_record: i32,
    coaligned_roll: u32,
) -> bool {
    if arch_peaceful_by_default {
        return true;
    }
    if always_hostile {
        return false;
    }
    if monster_alignment.signum() != hero_alignment.signum() {
        return false;
    }
    let a = (16 + hero_align_record.max(-15)) as u32;
    let b = (2 + monster_alignment.abs()) as u32;
    let peaceful_outcomes = a.saturating_sub(1) * b.saturating_sub(1);
    coaligned_roll < peaceful_outcomes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_always_peaceful_overrides_everything() {
        assert!(peace_minded(true, true, -1, 1, -100, 9999));
        assert!(peace_minded(true, false, -1, 1, 0, 9999));
    }

    #[test]
    fn test_always_hostile_is_never_peaceful() {
        assert!(!peace_minded(false, true, 1, 1, 10, 0));
        assert!(!peace_minded(false, true, 0, 0, 0, 0));
    }

    #[test]
    fn test_cross_aligned_is_always_hostile() {
        // Chaotic (-1) vs Lawful (+1)
        assert!(!peace_minded(false, false, -1, 1, 10, 0));
        // Neutral (0) vs Lawful (+1)
        assert!(!peace_minded(false, false, 0, 1, 10, 0));
        // Lawful (+1) vs Neutral (0)
        assert!(!peace_minded(false, false, 1, 0, 10, 0));
    }

    #[test]
    fn test_alignment_rule_odds_via_enumeration() {
        for rec in [-15_i32, -10, 0, 5, 10] {
            for mal in [-2_i32, -1, 0, 1, 2] {
                let ual = mal;
                let a = (16 + rec.max(-15)) as u32;
                let b = (2 + mal.abs()) as u32;
                let total = a * b;
                let expected_peaceful = (a - 1) * (b - 1);

                let actual_peaceful = (0..total)
                    .filter(|&r| peace_minded(false, false, mal, ual, rec, r))
                    .count();

                assert_eq!(
                    actual_peaceful as u32, expected_peaceful,
                    "rec={rec}, mal={mal}, a={a}, b={b}"
                );
            }
        }
    }
}
