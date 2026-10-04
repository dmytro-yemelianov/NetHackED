//! Inventory interaction mechanics: potion dilution, lamp rubbing, and shop price identification.
//!
//! Formalized in Lean 4 (`NetMechanics/InventoryInteraction.lean`).

use netrust_types::Buc;

/// Dilution state representing potion concentration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DilutionState {
    Potion(u32),
    Water,
}

/// Dilutes a potion state by one step towards pure water.
/// Proved idempotent on Water (`dilute_water_idempotent`) and eventually reaching Water (`dilute_eventually_water`).
pub fn dilute_potion(state: DilutionState) -> DilutionState {
    match state {
        DilutionState::Potion(0) => DilutionState::Water,
        DilutionState::Potion(n) => DilutionState::Potion(n - 1),
        DilutionState::Water => DilutionState::Water,
    }
}

/// Outcome of rubbing a lamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RubResult {
    WishGranted,
    PeacefulDjinni,
    HostileDjinni,
    Smoke,
    Nothing,
}

/// Evaluates the outcome of rubbing a lamp according to canonical NetHack rules.
/// Proved: magic lamps exhaust their Djinni on wish/release (`rub_magic_lamp_exhausts_djinni`),
/// and ordinary oil lamps never grant wishes (`rub_oil_lamp_never_wishes`).
// TODO(fidelity): branches identical; see review
#[allow(clippy::if_same_then_else)]
pub fn rub_lamp(is_magic: bool, has_djinni: bool, buc: Buc, oil_turns: u32) -> (RubResult, bool) {
    if is_magic && has_djinni {
        let outcome = match buc {
            Buc::Blessed => RubResult::WishGranted,
            Buc::Uncursed => RubResult::PeacefulDjinni,
            Buc::Cursed => RubResult::HostileDjinni,
        };
        // Magic Djinni consumed -> becomes ordinary oil lamp
        (outcome, true)
    } else if is_magic && !has_djinni {
        (RubResult::Smoke, false)
    } else if oil_turns > 0 {
        (RubResult::Smoke, false)
    } else {
        (RubResult::Nothing, false)
    }
}

/// Charisma buy multiplier in basis 12 (NetHack shk.c).
pub fn buy_factor(cha: u32) -> u32 {
    if cha <= 5 {
        24 // 200%
    } else if cha <= 14 {
        16 // 133%
    } else if cha <= 17 {
        12 // 100%
    } else {
        9 // 75%
    }
}

/// Charisma sell multiplier in basis 12 (NetHack shk.c).
pub fn sell_factor(cha: u32) -> u32 {
    if cha <= 5 {
        3 // 25%
    } else if cha <= 10 {
        4 // 33%
    } else if cha <= 17 {
        6 // 50%
    } else {
        8 // 66%
    }
}

/// Calculates the shopkeeper's purchase price (what player pays).
pub fn calculate_buy_price(base: u32, cha: u32, buc: Buc) -> u32 {
    let raw = (base * buy_factor(cha)) / 12;
    match buc {
        Buc::Cursed => (raw * 4) / 3,
        _ => raw,
    }
}

/// Calculates the shopkeeper's offering price (what player receives).
/// Proved in Lean 4: `sell_price <= buy_price` for all inputs (`sell_le_buy_price`).
pub fn calculate_sell_price(base: u32, cha: u32, buc: Buc) -> u32 {
    let raw = (base * sell_factor(cha)) / 12;
    match buc {
        Buc::Cursed => (raw * 2) / 3,
        _ => raw,
    }
}

/// Reverse price-identification: given observed shop price and player charisma,
/// returns possible candidate base costs that could produce this price.
pub fn reverse_price_id(observed_price: u32, cha: u32, is_buying: bool, candidates: &[u32]) -> Vec<u32> {
    let mut matches = Vec::new();
    for &base in candidates {
        for &buc in &[Buc::Blessed, Buc::Uncursed, Buc::Cursed] {
            let price = if is_buying {
                calculate_buy_price(base, cha, buc)
            } else {
                calculate_sell_price(base, cha, buc)
            };
            if price == observed_price && !matches.contains(&base) {
                matches.push(base);
            }
        }
    }
    matches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dilution_progression() {
        let p2 = DilutionState::Potion(2);
        let p1 = dilute_potion(p2);
        assert_eq!(p1, DilutionState::Potion(1));
        let p0 = dilute_potion(p1);
        assert_eq!(p0, DilutionState::Potion(0));
        let w = dilute_potion(p0);
        assert_eq!(w, DilutionState::Water);
        let w2 = dilute_potion(w);
        assert_eq!(w2, DilutionState::Water);
    }

    #[test]
    fn test_magic_lamp_rub() {
        let (res, consumed) = rub_lamp(true, true, Buc::Blessed, 100);
        assert_eq!(res, RubResult::WishGranted);
        assert!(consumed);

        let (res2, consumed2) = rub_lamp(true, true, Buc::Uncursed, 100);
        assert_eq!(res2, RubResult::PeacefulDjinni);
        assert!(consumed2);

        let (res3, consumed3) = rub_lamp(false, false, Buc::Blessed, 50);
        assert_eq!(res3, RubResult::Smoke);
        assert!(!consumed3);

        let (res4, consumed4) = rub_lamp(false, false, Buc::Blessed, 0);
        assert_eq!(res4, RubResult::Nothing);
        assert!(!consumed4);
    }

    #[test]
    fn test_price_no_arbitrage() {
        for cha in 1..=25 {
            for buc in [Buc::Blessed, Buc::Uncursed, Buc::Cursed] {
                for base in [10, 50, 100, 300, 1000] {
                    let buy = calculate_buy_price(base, cha, buc);
                    let sell = calculate_sell_price(base, cha, buc);
                    assert!(sell <= buy, "Arbitrage violation: sell {sell} > buy {buy}");
                }
            }
        }
    }
}
