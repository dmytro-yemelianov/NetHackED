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

/// Charisma buy adjustment as `(multiplier, divisor)` (C `shk.c:2953-2964`, `get_cost`):
/// `>18`: 1/2; `18`: 2/3; `16-17`: 3/4; `11-15`: 1/1; `8-10`: 4/3; `6-7`: 3/2; `<=5`: 2/1.
pub fn buy_factor(charisma: i32) -> (u32, u32) {
    if charisma > 18 {
        (1, 2)
    } else if charisma == 18 {
        (2, 3)
    } else if charisma >= 16 {
        (3, 4)
    } else if charisma <= 5 {
        (2, 1)
    } else if charisma <= 7 {
        (3, 2)
    } else if charisma <= 10 {
        (4, 3)
    } else {
        (1, 1)
    }
}

/// Sell adjustment as `(multiplier, divisor)` (C `shk.c:3148-3175`, `set_cost`): the
/// shopkeeper offers 1/2, or 1/3 with the dunce/tourist surcharge; a lowballing shopkeeper
/// takes a further 3/4 (applied only when the pre-adjustment value exceeds 1, see
/// [`sell_price`]). Charisma and BUC never affect the sell price.
pub fn sell_factor(dunce_or_tourist: bool, shk_lowball: bool) -> (u32, u32) {
    let divisor = if dunce_or_tourist { 3 } else { 2 };
    if shk_lowball {
        (3, divisor * 4)
    } else {
        (1, divisor)
    }
}

/// Dunce/tourist surcharge predicate (C `shk.c:2947-2951` buy, `shk.c:3154-3160` sell):
/// a worn dunce cap, or a Tourist below experience level `MAXULEV / 2` (15), or a
/// visible shirt (`uarmu && !uarm && !uarmc`).
pub fn dunce_or_tourist_surcharge(
    dunce_cap_worn: bool,
    is_tourist: bool,
    ulevel: u32,
    shirt_visible: bool,
) -> bool {
    dunce_cap_worn || (is_tourist && ulevel < 15) || shirt_visible
}

/// Unidentified-object buy surcharge (C `shk.c:2864-2874`, `oid_price_adjustment`):
/// an object whose type is not known (and is not a glass gem, which is repriced as a
/// real gem instead) is surcharged 4/3 iff `o_id % 4 == 0`. Deterministic, no RNG.
pub fn oid_price_adjustment(unidentified: bool, is_glass_gem: bool, o_id: u32) -> bool {
    unidentified && !is_glass_gem && o_id % 4 == 0
}

/// Sell-side lowball (C `shk.c:3162-3175`): an unidentified non-gem is bought at a further
/// 3/4 by shopkeepers with `m_id % 4 == 0` (per shopkeeper, deterministic, no RNG).
/// Unidentified gems use a separate per-shopkeeper table that NetRust does not model.
pub fn shk_sell_lowball(unidentified: bool, is_gem: bool, shk_m_id: u32) -> bool {
    unidentified && !is_gem && shk_m_id % 4 == 0
}

/// C rounding `tmp = ((tmp * multiplier * 10 / divisor) + 5) / 10` (shk.c:2966-2974).
fn round_mul_div(tmp: u64, multiplier: u64, divisor: u64) -> u64 {
    let tmp = tmp * multiplier;
    if divisor > 1 {
        (tmp * 10 / divisor + 5) / 10
    } else {
        tmp
    }
}

/// Price the shopkeeper charges for one unit (C `shk.c:2877-2988`, `get_cost`).
///
/// `base` is `getprice(obj, FALSE)` (`shk.c:4319`); 0 is priced at 5. The unidentified
/// (`unid_surcharge`, see [`oid_price_adjustment`]), dunce/tourist (see
/// [`dunce_or_tourist_surcharge`]) and charisma ([`buy_factor`]) adjustments are combined
/// into one multiplier/divisor with C rounding, floored at 1; artifacts then cost x4 and an
/// angry shopkeeper adds `(tmp + 2) / 3`. BUC does not affect the price. No RNG.
pub fn buy_price(
    base: u32,
    charisma: i32,
    dunce_or_tourist: bool,
    unid_surcharge: bool,
    is_artifact: bool,
    angry_surcharge: bool,
) -> u32 {
    let tmp = if base == 0 { 5 } else { u64::from(base) };
    let (mut multiplier, mut divisor) = (1u64, 1u64);
    if unid_surcharge {
        multiplier *= 4;
        divisor *= 3;
    }
    if dunce_or_tourist {
        multiplier *= 4;
        divisor *= 3;
    }
    let (cm, cd) = buy_factor(charisma);
    multiplier *= u64::from(cm);
    divisor *= u64::from(cd);
    let mut price = round_mul_div(tmp, multiplier, divisor).max(1);
    if is_artifact {
        price *= 4;
    }
    if angry_surcharge {
        price += price.div_ceil(3); // C `(tmp + 2) / 3`
    }
    u32::try_from(price).unwrap_or(u32::MAX)
}

/// Price the shopkeeper offers for a stack (C `shk.c:3148-3192`, `set_cost`).
///
/// `base` is `getprice(obj, TRUE) * units`. Divisor 2, or 3 with the dunce/tourist
/// surcharge; `shk_lowball` (see [`shk_sell_lowball`]) applies a further 3/4 when `base > 1`.
/// C rounding, floored at 1 for a nonzero base; a zero base stays 0. Independent of
/// charisma and BUC. No RNG.
pub fn sell_price(base: u32, dunce_or_tourist: bool, shk_lowball: bool) -> u32 {
    if base == 0 {
        return 0;
    }
    let (multiplier, divisor) = sell_factor(dunce_or_tourist, shk_lowball && base > 1);
    let price = round_mul_div(u64::from(base), u64::from(multiplier), u64::from(divisor)).max(1);
    u32::try_from(price).unwrap_or(u32::MAX)
}

/// Reverse price-identification: given an observed shop price, returns the candidate base
/// costs that could produce it, trying both the unidentified surcharge (buying) or the
/// lowball offer (selling) since the player cannot see `o_id` / `m_id`.
pub fn reverse_price_id(
    observed_price: u32,
    charisma: i32,
    dunce_or_tourist: bool,
    is_buying: bool,
    candidates: &[u32],
) -> Vec<u32> {
    let mut matches = Vec::new();
    for &base in candidates {
        let hit = [false, true].iter().any(|&adj| {
            let price = if is_buying {
                buy_price(base, charisma, dunce_or_tourist, adj, false, false)
            } else {
                sell_price(base, dunce_or_tourist, adj)
            };
            price == observed_price
        });
        if hit && !matches.contains(&base) {
            matches.push(base);
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

    /// C shk.c:2953-2964 charisma table at every boundary, base 300, no other adjustment.
    #[test]
    fn test_buy_price_charisma_table_boundaries() {
        // (cha, expected): 300*m/d rounded.
        let table = [
            (3, 600),  // <=5: *2
            (5, 600),  // <=5: *2
            (6, 450),  // 6-7: *3/2
            (7, 450),  // 6-7: *3/2
            (8, 400),  // 8-10: *4/3
            (10, 400), // 8-10: *4/3
            (11, 300), // 11-15: x1
            (15, 300), // 11-15: x1
            (16, 225), // 16-17: *3/4
            (17, 225), // 16-17: *3/4
            (18, 200), // 18: *2/3
            (19, 150), // >18: /2
            (25, 150), // >18: /2
        ];
        for (cha, expected) in table {
            assert_eq!(
                buy_price(300, cha, false, false, false, false),
                expected,
                "cha {cha}"
            );
        }
    }

    #[test]
    fn test_buy_price_rounding_and_floor() {
        // 10 * 4/3 = 13.33 -> 13; 5 * 4/3 = 6.67 -> 7 (C rounds via ((x*10/d)+5)/10).
        assert_eq!(buy_price(10, 10, false, false, false, false), 13);
        assert_eq!(buy_price(5, 10, false, false, false, false), 7);
        // base 0 is priced at 5 (shk.c:2894).
        assert_eq!(buy_price(0, 12, false, false, false, false), 5);
        // 1 / 2 = 0.5 -> rounds to 1; never 0.
        assert_eq!(buy_price(1, 19, false, false, false, false), 1);
    }

    #[test]
    fn test_buy_price_surcharges() {
        // dunce/tourist *4/3, unidentified (o_id%4==0) *4/3 stack: 300*16/9 = 533.3 -> 533.
        assert_eq!(buy_price(300, 12, true, false, false, false), 400);
        assert_eq!(buy_price(300, 12, false, true, false, false), 400);
        assert_eq!(buy_price(300, 12, true, true, false, false), 533);
        // With CHA 10 as well: 300*64/27 = 711.1 -> 711.
        assert_eq!(buy_price(300, 10, true, true, false, false), 711);
        // Artifact x4 after rounding; angry surcharge tmp += (tmp+2)/3.
        assert_eq!(buy_price(300, 12, false, false, true, false), 1200);
        assert_eq!(buy_price(300, 12, false, false, false, true), 400);
        assert_eq!(buy_price(10, 12, false, false, false, true), 14);
    }

    #[test]
    fn test_surcharge_predicates() {
        // shk.c:2947-2951
        assert!(dunce_or_tourist_surcharge(true, false, 30, false));
        assert!(dunce_or_tourist_surcharge(false, true, 14, false));
        assert!(!dunce_or_tourist_surcharge(false, true, 15, false));
        assert!(dunce_or_tourist_surcharge(false, false, 1, true));
        assert!(!dunce_or_tourist_surcharge(false, false, 1, false));
        // shk.c:2864-2874 oid_price_adjustment
        assert!(oid_price_adjustment(true, false, 8));
        assert!(!oid_price_adjustment(true, false, 9));
        assert!(!oid_price_adjustment(false, false, 8));
        assert!(!oid_price_adjustment(true, true, 8));
        // shk.c:3162-3175 sell lowball
        assert!(shk_sell_lowball(true, false, 4));
        assert!(!shk_sell_lowball(true, false, 5));
        assert!(!shk_sell_lowball(false, false, 4));
        assert!(!shk_sell_lowball(true, true, 4));
    }

    #[test]
    fn test_sell_price_c_rules() {
        // shk.c:3148: base/2, or base/3 with dunce/tourist; lowball *3/4.
        assert_eq!(sell_price(300, false, false), 150);
        assert_eq!(sell_price(300, true, false), 100);
        assert_eq!(sell_price(300, false, true), 113); // 300*3/8 = 112.5 -> 113
        assert_eq!(sell_price(300, true, true), 75);
        assert_eq!(sell_price(1, false, false), 1); // min 1
        assert_eq!(sell_price(1, false, true), 1); // lowball needs tmp > 1
        assert_eq!(sell_price(0, false, false), 0); // worthless stays 0
        assert_eq!(sell_price(5, false, false), 3); // 2.5 -> 3
    }

    #[test]
    fn test_price_no_arbitrage() {
        for cha in -2..=25 {
            for dunce in [false, true] {
                for (unid, art, angry) in [(false, false, false), (true, true, true)] {
                    for lowball in [false, true] {
                        for base in [0, 1, 2, 3, 5, 10, 50, 100, 300, 1000] {
                            let buy = buy_price(base, cha, dunce, unid, art, angry);
                            let sell = sell_price(base, dunce, lowball);
                            assert!(sell <= buy, "sell {sell} > buy {buy}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn test_reverse_price_id() {
        // CHA 12, no surcharge: 300 is either a 300 base or a 225 base with unid surcharge.
        let candidates = [100, 200, 225, 300, 400];
        let hits = reverse_price_id(300, 12, false, true, &candidates);
        assert_eq!(hits, vec![225, 300]);
        // Selling: 150 comes from base 300 (or 400 lowballed).
        let hits = reverse_price_id(150, 12, false, false, &candidates);
        assert_eq!(hits, vec![300, 400]);
    }
}
