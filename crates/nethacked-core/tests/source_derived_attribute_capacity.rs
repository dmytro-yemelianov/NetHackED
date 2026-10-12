//! Literal expectations derived by reading NetHack-5.0.0_Released:
//! attrib.c:1245-1265, hack.c:4295-4381. C was not executed.
//! Ordinary form, no effective equipment/temporary properties.
use nethacked_core::inventory::{
    capacity_strength, encumbrance_tier, weight_cap, EncumbranceTier::*,
};

#[test]
fn encoded_strength_capacity_edges_are_source_derived() {
    for (strength, reduced, capacity) in [
        (i32::MIN, 3, 375),
        (3, 3, 375),
        (18, 18, 750),
        (19, 19, 775),
        (49, 19, 775),
        (50, 20, 800),
        (99, 20, 800),
        (100, 21, 825),
        (118, 21, 825),
        (119, 21, 825),
        (121, 21, 825),
        (122, 22, 850),
        (125, 25, 925),
        (i32::MAX, 25, 925),
    ] {
        assert_eq!(capacity_strength(strength), reduced);
        assert_eq!(
            weight_cap(capacity_strength(strength), 10, false, 0),
            capacity
        );
    }
    assert_eq!(weight_cap(25, 25, false, 0), 1000);
    assert_eq!(weight_cap(3, 3, true, 0), 1000);
}

#[test]
fn literal_capacity_classification_boundaries() {
    for (weight, expected) in [
        (825, Unencumbered),
        (826, Burdened),
        (1237, Burdened),
        (1238, Stressed),
        (1649, Stressed),
        (1650, Strained),
        (2062, Strained),
        (2063, Overtaxed),
        (2474, Overtaxed),
        (2475, Overloaded),
        (u32::MAX, Overloaded),
    ] {
        assert_eq!(encumbrance_tier(weight, 825), expected);
    }
}
