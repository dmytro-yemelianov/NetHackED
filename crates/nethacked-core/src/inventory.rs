//! Inventory, Container Hierarchy, and Encumbrance Mechanics.
//!
//! Modelled in Lean 4 in `NetMechanics.Inventory` (machine-checked model; not formally linked to this code).
//! Unlike NetHack's intrusive `union vptrs`, nested containment is a safe tree.

use crate::buc::Buc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemKind {
    Weapon { damage_dice: u32 },
    Armor { ac_bonus: i32 },
    Wand { charges: u32 },
    Potion,
    Scroll,
    Food { nutrition: u32 },
    Container { is_bag_of_holding: bool },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Item {
    Single {
        name: String,
        kind: ItemKind,
        weight: u32,
        buc: Buc,
    },
    Box {
        name: String,
        base_weight: u32,
        buc: Buc,
        is_bag_of_holding: bool,
        contents: Vec<Item>,
    },
}

impl Item {
    pub fn item_weight(&self) -> u32 {
        match self {
            Item::Single { weight, .. } => *weight,
            Item::Box {
                base_weight,
                buc,
                is_bag_of_holding,
                contents,
                ..
            } => {
                let inner_wt: u32 = contents.iter().map(|item| item.item_weight()).sum();
                let effective_inner = if *is_bag_of_holding {
                    match buc {
                        Buc::Blessed => inner_wt.div_ceil(4),
                        Buc::Uncursed => inner_wt.div_ceil(2),
                        Buc::Cursed => inner_wt * 2,
                    }
                } else {
                    inner_wt
                };
                base_weight + effective_inner
            }
        }
    }
}

/// What an object is, as far as `mbag_explodes` cares (C `obj->otyp`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BagCheckKind {
    /// `BAG_OF_HOLDING`.
    BagOfHolding,
    /// `BAG_OF_TRICKS` with `obj->spe` charges.
    BagOfTricks { charges: i32 },
    /// `WAN_CANCELLATION` with `obj->spe` charges.
    WandOfCancellation { charges: i32 },
    /// Anything else (a sack, a dagger, ...).
    Other,
}

/// An object being put into a Bag of Holding, with its recursive contents
/// (C `obj->cobj` list).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BagCheckItem {
    pub kind: BagCheckKind,
    pub children: Vec<BagCheckItem>,
}

/// C `mbag_explodes(obj, depthin)` (`pickup.c:2488-2507`): would putting `obj`
/// into a Bag of Holding make it explode?
///
/// Called with `depth = 0` when inserting (`pickup.c:2658`; also `pickup.c:3776`).
/// Empty (`spe <= 0`) wands of cancellation / bags of tricks never explode
/// (`pickup.c:2491-2493`). Otherwise a BoH, bag of tricks or wand of
/// cancellation explodes if `rn2(1 << min(depth, 7)) <= depth`
/// (`pickup.c:2496-2497`); else the children are scanned at `depth + 1`,
/// stopping at the first explosion.
///
/// `rn2` models C `rn2(n)`: it is called with the bound `n` and should return
/// a value in `0..n` (out-of-range results are clamped to `n - 1`). It is
/// invoked only when C draws (C short-circuits on the item-kind test and on the
/// first exploding child), so the caller's RNG stream stays in step.
pub fn mbag_explodes(obj: &BagCheckItem, depth: u32, rn2: &mut impl FnMut(u32) -> u32) -> bool {
    let magical = match obj.kind {
        BagCheckKind::WandOfCancellation { charges } | BagCheckKind::BagOfTricks { charges }
            if charges <= 0 =>
        {
            return false;
        }
        BagCheckKind::BagOfHolding
        | BagCheckKind::BagOfTricks { .. }
        | BagCheckKind::WandOfCancellation { .. } => true,
        BagCheckKind::Other => false,
    };
    if magical {
        let bound = 1u32 << depth.min(7);
        if rn2(bound).min(bound - 1) <= depth {
            return true;
        }
    }
    obj.children
        .iter()
        .any(|child| mbag_explodes(child, depth + 1, rn2))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum EncumbranceTier {
    Unencumbered,
    Burdened,
    Stressed,
    Strained,
    Overtaxed,
    Overloaded,
}

/// Carrying capacity (C `hack.c:4295`, `weight_cap`).
///
/// `str_` is the already-reduced `ACURRSTR` value (`attrib.c:1245`, 3..=25).
/// `carrcap = 25 * (str + con) + 50`; levitating heroes get `MAX_CARR_CAP`
/// (1000); otherwise the capacity is clamped to 1000 and reduced by 100 per
/// wounded leg (`wounded_legs`, clamped to 0..=2). The result is at least 1.
/// Polymorph size scaling and steeds are not modelled.
pub fn weight_cap(str_: i32, con: i32, levitating: bool, wounded_legs: u8) -> u32 {
    const MAX_CARR_CAP: i64 = 1000;
    let mut cap = 25 * (i64::from(str_) + i64::from(con)) + 50;
    if levitating {
        cap = MAX_CARR_CAP;
    } else {
        cap = cap.min(MAX_CARR_CAP);
        cap -= 100 * i64::from(wounded_legs.min(2));
    }
    cap.max(1) as u32
}

/// Encumbrance tier from total carried weight and capacity (C `hack.c:4372`,
/// `calc_capacity`): Unencumbered if `weight <= cap`; Overloaded if
/// `cap <= 1`; else tier `min((weight - cap) * 2 / cap + 1, 5)`.
/// A capacity of 0 (impossible in C, where `weight_cap >= 1`) is Overloaded.
pub fn encumbrance_tier(weight: u32, cap: u32) -> EncumbranceTier {
    if cap == 0 {
        return EncumbranceTier::Overloaded;
    }
    if weight <= cap {
        return EncumbranceTier::Unencumbered;
    }
    if cap <= 1 {
        return EncumbranceTier::Overloaded;
    }
    let excess = u64::from(weight - cap);
    let tier = (excess * 2 / u64::from(cap) + 1).min(5);
    match tier {
        1 => EncumbranceTier::Burdened,
        2 => EncumbranceTier::Stressed,
        3 => EncumbranceTier::Strained,
        4 => EncumbranceTier::Overtaxed,
        _ => EncumbranceTier::Overloaded,
    }
}

/// Alias of [`encumbrance_tier`] kept for existing callers (`hack.c:4372`).
pub fn calculate_encumbrance(weight: u32, capacity: u32) -> EncumbranceTier {
    encumbrance_tier(weight, capacity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bag_of_holding_weight() {
        let dagger = Item::Single {
            name: "dagger".into(),
            kind: ItemKind::Weapon { damage_dice: 6 },
            weight: 12,
            buc: Buc::Blessed,
        };
        let potion = Item::Single {
            name: "healing".into(),
            kind: ItemKind::Potion,
            weight: 20,
            buc: Buc::Uncursed,
        };
        let boh = Item::Box {
            name: "bag of holding".into(),
            base_weight: 15,
            buc: Buc::Blessed,
            is_bag_of_holding: true,
            contents: vec![dagger, potion],
        };
        // 12 + 20 = 32. (32 + 3) / 4 = 8. Base 15 + 8 = 23.
        assert_eq!(boh.item_weight(), 23);

        // NetHack rounding-up verification: 1 unit in blessed BoH weighs 1 unit, not 0
        let feather = Item::Single {
            name: "feather".into(),
            kind: ItemKind::Potion,
            weight: 1,
            buc: Buc::Uncursed,
        };
        let boh_feather = Item::Box {
            name: "bag of holding".into(),
            base_weight: 15,
            buc: Buc::Blessed,
            is_bag_of_holding: true,
            contents: vec![feather],
        };
        // (1 + 3) / 4 = 1. Base 15 + 1 = 16.
        assert_eq!(boh_feather.item_weight(), 16);
    }

    fn leaf(kind: BagCheckKind) -> BagCheckItem {
        BagCheckItem {
            kind,
            children: vec![],
        }
    }

    #[test]
    fn test_boh_into_boh_always_explodes() {
        let boh = leaf(BagCheckKind::BagOfHolding);
        // depth 0: rn2(1) == 0 <= 0, whatever the caller returns.
        assert!(mbag_explodes(&boh, 0, &mut |_| 999));
    }

    #[test]
    fn test_charged_cancellation_explodes_empty_does_not() {
        let w = |charges| leaf(BagCheckKind::WandOfCancellation { charges });
        assert!(mbag_explodes(&w(1), 0, &mut |_| 0));
        assert!(!mbag_explodes(&w(0), 0, &mut |_| panic!("no draw")));
        let t = |charges| leaf(BagCheckKind::BagOfTricks { charges });
        assert!(mbag_explodes(&t(3), 0, &mut |_| 0));
        assert!(!mbag_explodes(&t(0), 0, &mut |_| panic!("no draw")));
    }

    #[test]
    fn test_sack_with_boh_depth_one_always_explodes() {
        let sack = BagCheckItem {
            kind: BagCheckKind::Other,
            children: vec![leaf(BagCheckKind::BagOfHolding)],
        };
        // depth 1: rn2(2) in {0,1} <= 1 always.
        assert!(mbag_explodes(&sack, 0, &mut |_| 1));
    }

    #[test]
    fn test_deep_nesting_is_probabilistic_and_draws_in_c_order() {
        let boh = leaf(BagCheckKind::BagOfHolding);
        let sack2 = BagCheckItem {
            kind: BagCheckKind::Other,
            children: vec![BagCheckItem {
                kind: BagCheckKind::Other,
                children: vec![boh],
            }],
        };
        // BoH at depth 2: rn2(4) <= 2 explodes; 3 survives.
        let mut bounds = vec![];
        assert!(!mbag_explodes(&sack2, 0, &mut |b| {
            bounds.push(b);
            3
        }));
        assert_eq!(bounds, vec![4]);
        assert!(mbag_explodes(&sack2, 0, &mut |_| 2));
    }

    #[test]
    fn test_plain_items_never_draw() {
        let sack = BagCheckItem {
            kind: BagCheckKind::Other,
            children: vec![leaf(BagCheckKind::Other)],
        };
        assert!(!mbag_explodes(&sack, 0, &mut |_| panic!("no draw")));
    }

    #[test]
    fn test_encumbrance_tiers() {
        use EncumbranceTier::*;
        let cap = 100;
        let cases = [
            (50, Unencumbered),
            (100, Unencumbered),
            (101, Burdened),
            (149, Burdened),
            (150, Stressed),
            (199, Stressed),
            (200, Strained),
            (249, Strained),
            (250, Overtaxed),
            (299, Overtaxed),
            (300, Overloaded),
            (500, Overloaded),
        ];
        for (w, want) in cases {
            assert_eq!(encumbrance_tier(w, cap), want, "w={w}");
            assert_eq!(calculate_encumbrance(w, cap), want, "w={w}");
        }
    }

    #[test]
    fn test_encumbrance_odd_cap() {
        use EncumbranceTier::*;
        // cap 101: excess e, tier = 2e/101 + 1.
        assert_eq!(encumbrance_tier(101, 101), Unencumbered);
        assert_eq!(encumbrance_tier(102, 101), Burdened); // e=1
        assert_eq!(encumbrance_tier(151, 101), Burdened); // e=50 -> 100/101=0
        assert_eq!(encumbrance_tier(152, 101), Stressed); // e=51 -> 102/101=1
        assert_eq!(encumbrance_tier(404, 101), Overloaded); // 3cap+1
        assert_eq!(encumbrance_tier(5, 0), Overloaded);
        assert_eq!(encumbrance_tier(2, 1), Overloaded);
        assert_eq!(encumbrance_tier(1, 1), Unencumbered);
    }

    #[test]
    fn test_weight_cap() {
        assert_eq!(weight_cap(18, 18, false, 0), 950);
        assert_eq!(weight_cap(25, 25, false, 0), 1000); // clamp
        assert_eq!(weight_cap(25, 25, true, 2), 1000); // levitation ignores legs
        assert_eq!(weight_cap(18, 18, false, 1), 850);
        assert_eq!(weight_cap(18, 18, false, 2), 750);
        assert_eq!(weight_cap(25, 25, false, 2), 800); // clamp then legs
        assert_eq!(weight_cap(5, 5, false, 2), 100);
        assert_eq!(weight_cap(-100, -100, false, 2), 1); // floor
    }
}
