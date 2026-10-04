use netrust_types::Hero;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AfflictionTickResult {
    Survived,
    StoneDeath,
    SlimeDeath,
}

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

    AfflictionTickResult::Survived
}

pub fn cure_petrification(hero: &mut Hero) {
    hero.afflictions.petrification = None;
}

pub fn cure_sliming(hero: &mut Hero) {
    hero.afflictions.sliming = None;
}
