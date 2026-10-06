use nethacked_types::Hero;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AfflictionTickResult {
    Survived,
    StoneDeath,
    SlimeDeath,
}

/// Own all hero affliction countdowns; call once per elapsed world turn.
/// C `nh_timeout()` (timeout.c:671) decrements timed properties once, called
/// from `moveloop_core()` (allmain.c:274); helpless `multi` advances once in
/// allmain.c:381-383. The sim derives recovery messages from these transitions.
pub fn tick_afflictions(hero: &mut Hero) -> AfflictionTickResult {
    if let Some(ref mut petrification) = hero.afflictions.petrification {
        if petrification.turns_remaining > 0 {
            petrification.turns_remaining -= 1;
        }
        if petrification.turns_remaining == 0 {
            return AfflictionTickResult::StoneDeath;
        }
    }

    if let Some(ref mut sliming) = hero.afflictions.sliming {
        if sliming.turns_remaining > 0 {
            sliming.turns_remaining -= 1;
        }
        if sliming.turns_remaining == 0 {
            return AfflictionTickResult::SlimeDeath;
        }
    }

    let trans = &mut hero.afflictions.transient;
    if trans.confused > 0 {
        trans.confused -= 1;
    }
    if trans.stunned > 0 {
        trans.stunned -= 1;
    }
    if trans.hallucinating > 0 {
        trans.hallucinating -= 1;
    }
    trans.blinded = trans.blinded.saturating_sub(1);
    trans.helpless = trans.helpless.saturating_sub(1);

    AfflictionTickResult::Survived
}

pub fn cure_petrification(hero: &mut Hero) {
    hero.afflictions.petrification = None;
}

pub fn cure_sliming(hero: &mut Hero) {
    hero.afflictions.sliming = None;
}
