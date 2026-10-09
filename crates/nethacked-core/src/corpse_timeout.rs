//! Initial corpse timeout sampling, without an age or timer owner.
//! NetHack-5.0.0_Released 16ff59115315917b93185d026aeefea06db9b0f4:
//! mkobj.c:1389-1431, rnd.c:192-229, hack.h ROT_AGE=250.

/// Existing represented elapsed age, and the actual scheduling context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CorpseTimeoutContext {
    pub elapsed_age: u32,
    pub moves: u64,
    pub hero_level: u32,
    pub in_mklev: bool,
}

/// C computes noisy rot before any revival-family replacement. The result is
/// signed: a corpse near ROT_AGE can receive a deadline in the past.
pub fn initial_rot_delay(ctx: CorpseTimeoutContext, mut rn2: impl FnMut(u32) -> u32) -> i64 {
    try_initial_rot_delay(ctx, |n| Ok::<_, std::convert::Infallible>(rn2(n))).unwrap()
}

/// Fallible rot sampling with the same conditional multiplier draws.
pub fn try_initial_rot_delay<E>(
    ctx: CorpseTimeoutContext,
    mut rn2: impl FnMut(u32) -> Result<u32, E>,
) -> Result<i64, E> {
    let adjustment = if ctx.in_mklev { 25i64 } else { 10i64 };
    // ItemRecord already represents elapsed age, including at clock zero.
    let age = i64::from(ctx.elapsed_age);
    let base = if age > 250 { adjustment } else { 250 - age };
    let mut scale = 1000 + i64::from(rn2(1000)?.min(999));
    let cap = (ctx.hero_level / 3).max(5);
    let mut multiplier = 1;
    while multiplier < cap && rn2(4)?.min(3) == 0 {
        multiplier += 1;
    }
    scale *= i64::from(multiplier);
    let noisy = if rn2(2)?.min(1) != 0 {
        adjustment * scale / 1000
    } else {
        adjustment * 1000 / scale
    };
    Ok(base + noisy - adjustment)
}

/// C replaces the previously sampled rot delay on the first successful Troll
/// trial. TAINT_AGE=50 and TROLL_REVIVE_CHANCE=37 (hack.h:1404-1405).
/// Trial age is the new relative delay, independent of existing corpse age.
/// `norevive` affects the later callback, not these initial trials.
pub fn initial_troll_revival_delay(mut rn2: impl FnMut(u32) -> u32) -> Option<i64> {
    try_initial_troll_revival_delay(|n| Ok::<_, std::convert::Infallible>(rn2(n))).unwrap()
}

/// Fallible trials; stop at the first success or refused draw.
pub fn try_initial_troll_revival_delay<E>(
    mut rn2: impl FnMut(u32) -> Result<u32, E>,
) -> Result<Option<i64>, E> {
    for age in 2..=50 {
        if rn2(37)?.min(36) == 0 {
            return Ok(Some(age));
        }
    }
    Ok(None)
}
