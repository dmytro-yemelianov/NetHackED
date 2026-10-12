//! Literal arithmetic/draw oracles from pinned mkobj.c:1389-1431 and rnd.c:192-229.
use nethacked_core::corpse_timeout::{
    initial_rot_delay, initial_troll_revival_delay, CorpseTimeoutContext,
};

fn context(age: u32, moves: u64, level: u32, generation: bool) -> CorpseTimeoutContext {
    CorpseTimeoutContext {
        elapsed_age: age,
        moves,
        hero_level: level,
        in_mklev: generation,
    }
}
fn sample(ctx: CorpseTimeoutContext, trace: &[(u32, u32)]) -> i64 {
    let mut trace = trace.iter();
    let delay = initial_rot_delay(ctx, |bound| {
        let &(expected, value) = trace.next().expect("unexpected scheduling draw");
        assert_eq!(bound, expected);
        assert!(value < bound);
        value
    });
    assert_eq!(trace.len(), 0);
    delay
}

#[test]
fn literal_integer_truncation_signed_rot_boundary_and_generation_adjustment() {
    // scale=1999*5=9995: rnz(10)=1, rnz(25)=2 in the divide branch.
    let divide = [(1000, 999), (4, 0), (4, 0), (4, 0), (4, 0), (2, 0)];
    for (age, ordinary, generation) in [(0, 241, 227), (249, -8, -22), (250, -9, -23), (251, 1, 2)]
    {
        assert_eq!(sample(context(age, 1000, 14, false), &divide), ordinary);
        assert_eq!(sample(context(age, 1000, 14, true), &divide), generation);
    }
    // scale=1999, multiply branch truncates to 19/49 rather than rounding.
    let multiply = [(1000, 999), (4, 1), (2, 1)];
    assert_eq!(sample(context(0, 1, 1, false), &multiply), 259);
    assert_eq!(sample(context(0, 1, 1, true), &multiply), 274);
    assert_eq!(sample(context(0, 0, 1, false), &multiply), 259);
    assert_eq!(sample(context(0, 0, 1, true), &multiply), 274);
    // age 251 resets the base to adjustment, rather than using negative age.
    assert_eq!(sample(context(u32::MAX, 1000, 1, false), &multiply), 19);
}

#[test]
fn literal_rne_caps_and_short_circuit_draws() {
    let cap5 = [(1000, 0), (4, 0), (4, 0), (4, 0), (4, 0), (2, 1)];
    for level in [1, 14, 15] {
        assert_eq!(sample(context(0, 1, level, false), &cap5), 290);
    }
    let cap6 = [(1000, 0), (4, 0), (4, 0), (4, 0), (4, 0), (4, 0), (2, 1)];
    assert_eq!(sample(context(0, 1, 18, false), &cap6), 300);
    let early = [(1000, 500), (4, 0), (4, 2), (2, 0)];
    assert_eq!(sample(context(0, 1, 18, false), &early), 243);
    let neutral = [(1000, 0), (4, 1), (2, 0)];
    assert_eq!(sample(context(250, 1000, 18, false), &neutral), 0);
}

#[test]
fn literal_troll_trial_ages_bounds_and_short_circuit() {
    for success_age in [2, 3, 50] {
        let mut calls = 0;
        assert_eq!(
            initial_troll_revival_delay(|bound| {
                assert_eq!(bound, 37);
                calls += 1;
                if calls == success_age - 1 {
                    0
                } else {
                    36
                }
            }),
            Some(success_age)
        );
        assert_eq!(calls, success_age - 1);
    }
    let mut calls = 0;
    assert_eq!(
        initial_troll_revival_delay(|bound| {
            assert_eq!(bound, 37);
            calls += 1;
            1
        }),
        None
    );
    assert_eq!(calls, 49);
}
