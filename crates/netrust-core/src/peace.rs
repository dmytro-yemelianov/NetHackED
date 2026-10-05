//! NetHack 5.0 C `peace_minded(ptr)` (`src/makemon.c:2268-2308`) and the
//! alignment adjustment `adjalign` (`src/attrib.c:1297-1315`).

/// Inputs of C `peace_minded` (`makemon.c:2268-2308`) for one monster species
/// and the current hero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PeaceMindedInput {
    /// C `always_peaceful(ptr)`: `M2_PEACEFUL` (makemon.c:2272).
    pub always_peaceful: bool,
    /// C `always_hostile(ptr)`: `M2_HOSTILE` (makemon.c:2274).
    pub always_hostile: bool,
    /// C `msound == MS_LEADER || msound == MS_GUARDIAN` (makemon.c:2276).
    pub leader_or_guardian: bool,
    /// C `msound == MS_NEMESIS` (makemon.c:2278).
    pub nemesis: bool,
    /// C `race_peaceful(ptr)` (makemon.c:2283).
    pub race_peaceful: bool,
    /// C `race_hostile(ptr)` (makemon.c:2285).
    pub race_hostile: bool,
    /// C `ptr->maligntyp` (`mal`; `A_NONE` = -128).
    pub monster_alignment: i32,
    /// C `u.ualign.type` (`ual`: -1 chaotic, 0 neutral, 1 lawful).
    pub hero_alignment: i32,
    /// C `u.ualign.record`.
    pub hero_align_record: i32,
    /// C `u.uhave.amulet` (makemon.c:2294).
    pub hero_has_amulet: bool,
    /// C `is_minion(ptr)`: `M2_MINION` (makemon.c:2298).
    pub is_minion: bool,
}

/// What C `peace_minded` decides before (or instead of) its two random draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeaceDecision {
    /// Peaceful without a draw.
    Peaceful,
    /// Hostile without a draw.
    Hostile,
    /// The co-aligned last case (makemon.c:2305-2307):
    /// `!!rn2(a) && !!rn2(b)` with `a = 16 + max(record, -15)` (`a >= 1`) and
    /// `b = 2 + abs(mal)` (`b >= 2`). `rn2(b)` is drawn only when `rn2(a) != 0`.
    CoalignedRoll { a: u32, b: u32 },
}

/// C `sgn()` (hacklib.c).
#[inline]
fn sgn(x: i32) -> i32 {
    x.signum()
}

/// The deterministic part of C `peace_minded` (`makemon.c:2268-2308`), in C order:
/// 1. `M2_PEACEFUL` -> peaceful; 2. `M2_HOSTILE` -> hostile;
/// 3. `MS_LEADER`/`MS_GUARDIAN` -> peaceful, `MS_NEMESIS` -> hostile;
/// 4. `race_peaceful` -> peaceful, `race_hostile` -> hostile;
/// 5. `sgn(mal) != sgn(ual)` -> hostile;
/// 6. `mal < A_NEUTRAL` while the hero has the Amulet -> hostile;
/// 7. minion -> peaceful iff `record >= 0`;
/// 8. otherwise the two-draw co-aligned roll.
///
/// The Erinys step (makemon.c:2280, `!u.ualign.abuse`) is not modelled: no
/// BESTIARY entry is an Erinys.
pub fn peace_decision(input: &PeaceMindedInput) -> PeaceDecision {
    if input.always_peaceful {
        return PeaceDecision::Peaceful;
    }
    if input.always_hostile {
        return PeaceDecision::Hostile;
    }
    if input.leader_or_guardian {
        return PeaceDecision::Peaceful;
    }
    if input.nemesis {
        return PeaceDecision::Hostile;
    }
    if input.race_peaceful {
        return PeaceDecision::Peaceful;
    }
    if input.race_hostile {
        return PeaceDecision::Hostile;
    }
    if sgn(input.monster_alignment) != sgn(input.hero_alignment) {
        return PeaceDecision::Hostile;
    }
    // A_NEUTRAL is 0 (align.h).
    if input.monster_alignment < 0 && input.hero_has_amulet {
        return PeaceDecision::Hostile;
    }
    if input.is_minion {
        return if input.hero_align_record >= 0 {
            PeaceDecision::Peaceful
        } else {
            PeaceDecision::Hostile
        };
    }
    // i64 so that neither `16 + record` nor `abs(mal)` can overflow; the
    // results are clamped into the `rn2` argument range (a >= 1, b >= 2).
    let a = 16 + i64::from(input.hero_align_record).max(-15);
    let b = 2 + i64::from(input.monster_alignment).abs();
    PeaceDecision::CoalignedRoll {
        a: a.clamp(1, i64::from(u32::MAX)) as u32,
        b: b.clamp(2, i64::from(u32::MAX)) as u32,
    }
}

/// C `peace_minded(ptr)` (`makemon.c:2268-2308`).
///
/// `rn2(n)` must return a value in `0..n`. It is called only in the co-aligned
/// last case, once for `rn2(16 + max(record, -15))` and, only when that draw is
/// non-zero, once more for `rn2(2 + abs(mal))` (C's `&&` short-circuit,
/// makemon.c:2305-2307).
pub fn peace_minded(input: &PeaceMindedInput, mut rn2: impl FnMut(u32) -> u32) -> bool {
    match peace_decision(input) {
        PeaceDecision::Peaceful => true,
        PeaceDecision::Hostile => false,
        PeaceDecision::CoalignedRoll { a, b } => rn2(a) != 0 && rn2(b) != 0,
    }
}

/// C `adjalign(n)` (`attrib.c:1297-1315`) on the alignment record.
///
/// A negative `n` only ever lowers the record; a positive `n` raises it and then
/// caps it at `alignlim` (C `ALIGNLIM = 10 + moves / 200`, align.h:17). The
/// `u.ualign.abuse` counter (and `adj_erinys`) is not tracked.
pub fn adjalign(record: i32, n: i32, alignlim: i32) -> i32 {
    let newalign = record.saturating_add(n);
    if n < 0 {
        newalign.min(record)
    } else if newalign > record {
        newalign.min(alignlim)
    } else {
        record
    }
}

use netrust_types::Alignment;

/// C `ALIGNLIM` (`align.h:17`): `10 + moves / 200`.
pub fn alignlim(moves: u64) -> i32 {
    i32::try_from(10 + moves / 200).unwrap_or(i32::MAX)
}

/// C `set_malign` (`makemon.c:2320-2366`).
///
/// Precalculated alignment adjustment upon monster death. Negative values mean
/// it is bad to kill this monster; positive values mean it is good.
pub fn calculate_malign(
    maligntyp: i8,
    hero_alignment: Alignment,
    is_peaceful: bool,
    is_leader: bool,
    always_peaceful: bool,
    always_hostile: bool,
) -> i32 {
    let mal = maligntyp as i32;
    let hero_sgn = match hero_alignment {
        Alignment::Chaotic => -1,
        Alignment::Neutral | Alignment::Unaligned => 0,
        Alignment::Lawful => 1,
    };
    let mal_sgn = if mal < 0 {
        -1
    } else if mal > 0 {
        1
    } else {
        0
    };
    let coaligned = mal_sgn == hero_sgn;

    if is_leader {
        -20
    } else if maligntyp == -128 {
        // A_NONE (align.h:19, makemon.c:2341-2345)
        if is_peaceful {
            0
        } else {
            20
        }
    } else if always_peaceful {
        // makemon.c:2346-2351
        let absmal = mal.abs();
        if is_peaceful {
            -3 * 5.max(absmal)
        } else {
            3 * 5.max(absmal)
        }
    } else if always_hostile {
        // makemon.c:2352-2357
        let absmal = mal.abs();
        if coaligned {
            0
        } else {
            5.max(absmal)
        }
    } else if coaligned {
        // makemon.c:2358-2363
        let absmal = mal.abs();
        if is_peaceful {
            -3 * 3.max(absmal)
        } else {
            3.max(absmal)
        }
    } else {
        // makemon.c:2364-2365: not coaligned and therefore hostile
        mal.abs()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coaligned(mal: i32, ual: i32, rec: i32) -> PeaceMindedInput {
        PeaceMindedInput {
            monster_alignment: mal,
            hero_alignment: ual,
            hero_align_record: rec,
            ..PeaceMindedInput::default()
        }
    }

    #[test]
    fn always_peaceful_overrides_everything_and_draws_nothing() {
        let input = PeaceMindedInput {
            always_peaceful: true,
            always_hostile: true,
            nemesis: true,
            race_hostile: true,
            monster_alignment: -1,
            hero_alignment: 1,
            hero_align_record: -100,
            ..PeaceMindedInput::default()
        };
        assert!(peace_minded(&input, |_| panic!("no draw")));
    }

    #[test]
    fn always_hostile_and_nemesis_never_peaceful() {
        let mut input = coaligned(1, 1, 10);
        input.always_hostile = true;
        assert!(!peace_minded(&input, |_| panic!("no draw")));
        let mut input = coaligned(1, 1, 10);
        input.nemesis = true;
        assert!(!peace_minded(&input, |_| panic!("no draw")));
        let mut input = coaligned(-1, 1, 10);
        input.leader_or_guardian = true;
        assert!(peace_minded(&input, |_| panic!("no draw")));
    }

    #[test]
    fn race_rules_precede_alignment() {
        let mut input = coaligned(-1, 1, 10);
        input.race_peaceful = true;
        assert!(peace_minded(&input, |_| panic!("no draw")));
        let mut input = coaligned(1, 1, 10);
        input.race_hostile = true;
        assert!(!peace_minded(&input, |_| panic!("no draw")));
    }

    #[test]
    fn cross_aligned_is_always_hostile_without_a_draw() {
        for (mal, ual) in [(-1, 1), (0, 1), (1, 0), (-3, 0), (4, -1)] {
            assert!(!peace_minded(&coaligned(mal, ual, 10), |_| panic!(
                "no draw"
            )));
        }
    }

    #[test]
    fn amulet_and_minion_rules() {
        let mut input = coaligned(-3, -1, 10);
        input.hero_has_amulet = true;
        assert!(!peace_minded(&input, |_| panic!("no draw")));
        // A_NEUTRAL monsters are not affected by the Amulet.
        let mut input = coaligned(0, 0, 10);
        input.hero_has_amulet = true;
        assert!(matches!(
            peace_decision(&input),
            PeaceDecision::CoalignedRoll { .. }
        ));
        let mut input = coaligned(3, 1, 0);
        input.is_minion = true;
        assert!(peace_minded(&input, |_| panic!("no draw")));
        input.hero_align_record = -1;
        assert!(!peace_minded(&input, |_| panic!("no draw")));
    }

    /// Enumerate every (r1, r2) pair: exactly `(a-1)*(b-1)` of the `a*b`
    /// equally likely outcomes are peaceful, and `rn2(b)` is drawn only after a
    /// non-zero `rn2(a)`.
    #[test]
    fn coaligned_odds_by_enumeration() {
        for rec in [-100_i32, -15, -10, 0, 5, 10, 25] {
            for mal in [-5_i32, -1, 0, 1, 4, 20] {
                let input = coaligned(mal, mal.signum(), rec);
                let PeaceDecision::CoalignedRoll { a, b } = peace_decision(&input) else {
                    panic!("co-aligned must roll");
                };
                assert_eq!(a as i32, 16 + rec.max(-15));
                assert_eq!(b as i32, 2 + mal.abs());
                let mut peaceful = 0u32;
                for r1 in 0..a {
                    for r2 in 0..b {
                        let mut draws = vec![r1, r2].into_iter();
                        let mut calls = Vec::new();
                        let p = peace_minded(&input, |n| {
                            calls.push(n);
                            draws.next().unwrap()
                        });
                        if r1 == 0 {
                            assert_eq!(calls, vec![a], "second draw skipped");
                        } else {
                            assert_eq!(calls, vec![a, b]);
                        }
                        peaceful += u32::from(p);
                    }
                }
                assert_eq!(peaceful, (a - 1) * (b - 1), "rec={rec} mal={mal}");
            }
        }
    }

    #[test]
    fn extreme_inputs_never_panic() {
        for rec in [i32::MIN, -16, i32::MAX] {
            for mal in [i32::MIN, -128, i32::MAX] {
                let input = coaligned(mal, mal.signum(), rec);
                if let PeaceDecision::CoalignedRoll { a, b } = peace_decision(&input) {
                    assert!(a >= 1 && b >= 2);
                }
                let _ = peace_minded(&input, |n| n - 1);
            }
        }
    }

    #[test]
    fn adjalign_matches_c() {
        assert_eq!(adjalign(5, -1, 10), 4);
        assert_eq!(adjalign(5, 2, 10), 7);
        assert_eq!(adjalign(9, 2, 10), 10);
        // A positive adjustment above ALIGNLIM clamps to ALIGNLIM (attrib.c:1313-1314).
        assert_eq!(adjalign(25, 2, 10), 10);
        assert_eq!(adjalign(i32::MIN, -5, 10), i32::MIN);
        assert_eq!(adjalign(i32::MAX, 5, i32::MAX), i32::MAX);
        assert_eq!(alignlim(0), 10);
        assert_eq!(alignlim(450), 12);
    }

    #[test]
    fn test_calculate_malign_canonical_cases() {
        // Leader is always -20
        assert_eq!(
            calculate_malign(0, Alignment::Lawful, true, true, false, false),
            -20
        );
        assert_eq!(
            calculate_malign(-3, Alignment::Chaotic, false, true, false, true),
            -20
        );

        // A_NONE (-128)
        assert_eq!(
            calculate_malign(-128, Alignment::Lawful, true, false, false, false),
            0
        );
        assert_eq!(
            calculate_malign(-128, Alignment::Lawful, false, false, false, false),
            20
        );

        // Always peaceful (e.g. shopkeeper mal=0)
        assert_eq!(
            calculate_malign(0, Alignment::Neutral, true, false, true, false),
            -15 // -3 * max(5, 0)
        );
        assert_eq!(
            calculate_malign(0, Alignment::Neutral, false, false, true, false),
            15 // 3 * max(5, 0)
        );
        assert_eq!(
            calculate_malign(7, Alignment::Lawful, true, false, true, false),
            -21 // -3 * max(5, 7)
        );

        // Always hostile (e.g. orc mal=-3, demon mal=-15)
        // Coaligned always hostile: 0
        assert_eq!(
            calculate_malign(-3, Alignment::Chaotic, false, false, false, true),
            0
        );
        // Crossaligned always hostile: max(5, absmal)
        assert_eq!(
            calculate_malign(-3, Alignment::Lawful, false, false, false, true),
            5 // max(5, 3)
        );
        assert_eq!(
            calculate_malign(-15, Alignment::Lawful, false, false, false, true),
            15 // max(5, 15)
        );

        // Coaligned standard monster
        // Peaceful: -3 * max(3, absmal)
        assert_eq!(
            calculate_malign(0, Alignment::Neutral, true, false, false, false),
            -9 // -3 * max(3, 0)
        );
        assert_eq!(
            calculate_malign(-4, Alignment::Chaotic, true, false, false, false),
            -12 // -3 * max(3, 4)
        );
        // Hostile (renegade): max(3, absmal)
        assert_eq!(
            calculate_malign(0, Alignment::Neutral, false, false, false, false),
            3 // max(3, 0)
        );
        assert_eq!(
            calculate_malign(4, Alignment::Lawful, false, false, false, false),
            4 // max(3, 4)
        );

        // Crossaligned hostile standard monster: abs(mal)
        assert_eq!(
            calculate_malign(-3, Alignment::Lawful, false, false, false, false),
            3
        );
        assert_eq!(
            calculate_malign(4, Alignment::Chaotic, false, false, false, false),
            4
        );
    }
}
