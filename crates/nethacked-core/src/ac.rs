//! Armor Class (AC) and worn armor calculation.
//!
//! Faithful to NetHack 5.0 C `ARM_BONUS(obj)` (`include/hack.h:1526-1528`)
//! and `find_ac(void)` (`src/do_wear.c:2473-2507`).

use serde::{Deserialize, Serialize};

/// Maximum absolute value of AC in NetHack 5.0 (C `AC_MAX`, `include/you.h:472`).
pub const AC_MAX: i32 = 99;

/// Canonical armor slots in NetHack (C `enum obj_armor_types` `ARM_SUIT` ..
/// `ARM_SHIRT`, `include/objclass.h:37-45`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ArmorSlot {
    Suit,
    Cloak,
    Helmet,
    Shield,
    Gloves,
    Boots,
    Shirt,
}

/// Every C armor object: `(name, oc_armcat, a_ac)` with `a_ac = 10 - ac` as the
/// `ARMOR` macro stores it (`include/objects.h:422-427`). Names are lowercase.
pub const C_ARMOR: &[(&str, ArmorSlot, i32)] = &[
    ("elven leather helm", ArmorSlot::Helmet, 1), // objects.h:445
    ("orcish helm", ArmorSlot::Helmet, 1),        // objects.h:448
    ("dwarvish iron helm", ArmorSlot::Helmet, 2), // objects.h:451
    ("fedora", ArmorSlot::Helmet, 0),             // objects.h:454
    ("cornuthaum", ArmorSlot::Helmet, 0),         // objects.h:457
    ("dunce cap", ArmorSlot::Helmet, 0),          // objects.h:462
    ("dented pot", ArmorSlot::Helmet, 1),         // objects.h:467
    ("helm of brilliance", ArmorSlot::Helmet, 1), // objects.h:470
    ("helmet", ArmorSlot::Helmet, 1),             // objects.h:476
    ("helm of caution", ArmorSlot::Helmet, 1),    // objects.h:479
    ("helm of opposite alignment", ArmorSlot::Helmet, 1), // objects.h:482
    ("helm of telepathy", ArmorSlot::Helmet, 1),  // objects.h:485
    ("gray dragon scale mail", ArmorSlot::Suit, 9), // objects.h:502
    ("gold dragon scale mail", ArmorSlot::Suit, 9), // objects.h:505
    ("silver dragon scale mail", ArmorSlot::Suit, 9), // objects.h:507
    ("shimmering dragon scale mail", ArmorSlot::Suit, 9), // objects.h:510
    ("red dragon scale mail", ArmorSlot::Suit, 9), // objects.h:513
    ("white dragon scale mail", ArmorSlot::Suit, 9), // objects.h:515
    ("orange dragon scale mail", ArmorSlot::Suit, 9), // objects.h:517
    ("black dragon scale mail", ArmorSlot::Suit, 9), // objects.h:519
    ("blue dragon scale mail", ArmorSlot::Suit, 9), // objects.h:521
    ("green dragon scale mail", ArmorSlot::Suit, 9), // objects.h:523
    ("yellow dragon scale mail", ArmorSlot::Suit, 9), // objects.h:525
    ("gray dragon scales", ArmorSlot::Suit, 3),   // objects.h:530
    ("gold dragon scales", ArmorSlot::Suit, 3),   // objects.h:532
    ("silver dragon scales", ArmorSlot::Suit, 3), // objects.h:534
    ("shimmering dragon scales", ArmorSlot::Suit, 3), // objects.h:537
    ("red dragon scales", ArmorSlot::Suit, 3),    // objects.h:540
    ("white dragon scales", ArmorSlot::Suit, 3),  // objects.h:542
    ("orange dragon scales", ArmorSlot::Suit, 3), // objects.h:544
    ("black dragon scales", ArmorSlot::Suit, 3),  // objects.h:546
    ("blue dragon scales", ArmorSlot::Suit, 3),   // objects.h:548
    ("green dragon scales", ArmorSlot::Suit, 3),  // objects.h:550
    ("yellow dragon scales", ArmorSlot::Suit, 3), // objects.h:552
    ("plate mail", ArmorSlot::Suit, 7),           // objects.h:556
    ("crystal plate mail", ArmorSlot::Suit, 7),   // objects.h:559
    ("bronze plate mail", ArmorSlot::Suit, 6),    // objects.h:562
    ("splint mail", ArmorSlot::Suit, 6),          // objects.h:565
    ("banded mail", ArmorSlot::Suit, 6),          // objects.h:568
    ("dwarvish mithril-coat", ArmorSlot::Suit, 6), // objects.h:571
    ("elven mithril-coat", ArmorSlot::Suit, 5),   // objects.h:574
    ("chain mail", ArmorSlot::Suit, 5),           // objects.h:577
    ("orcish chain mail", ArmorSlot::Suit, 4),    // objects.h:580
    ("scale mail", ArmorSlot::Suit, 4),           // objects.h:583
    ("studded leather armor", ArmorSlot::Suit, 3), // objects.h:586
    ("ring mail", ArmorSlot::Suit, 3),            // objects.h:589
    ("orcish ring mail", ArmorSlot::Suit, 2),     // objects.h:592
    ("leather armor", ArmorSlot::Suit, 2),        // objects.h:595
    ("leather jacket", ArmorSlot::Suit, 1),       // objects.h:598
    ("hawaiian shirt", ArmorSlot::Shirt, 0),      // objects.h:603
    ("t-shirt", ArmorSlot::Shirt, 0),             // objects.h:606
    ("mummy wrapping", ArmorSlot::Cloak, 0),      // objects.h:611
    ("elven cloak", ArmorSlot::Cloak, 1),         // objects.h:615
    ("orcish cloak", ArmorSlot::Cloak, 0),        // objects.h:617
    ("dwarvish cloak", ArmorSlot::Cloak, 0),      // objects.h:620
    ("oilskin cloak", ArmorSlot::Cloak, 1),       // objects.h:623
    ("robe", ArmorSlot::Cloak, 2),                // objects.h:626
    ("alchemy smock", ArmorSlot::Cloak, 1),       // objects.h:630
    ("leather cloak", ArmorSlot::Cloak, 1),       // objects.h:633
    ("cloak of protection", ArmorSlot::Cloak, 3), // objects.h:637
    ("cloak of invisibility", ArmorSlot::Cloak, 1), // objects.h:641
    ("cloak of magic resistance", ArmorSlot::Cloak, 1), // objects.h:644
    ("cloak of displacement", ArmorSlot::Cloak, 1), // objects.h:648
    ("small shield", ArmorSlot::Shield, 1),       // objects.h:653
    ("shield of drain resistance", ArmorSlot::Shield, 1), // objects.h:656
    ("shield of shock resistance", ArmorSlot::Shield, 1), // objects.h:659
    ("elven shield", ArmorSlot::Shield, 2),       // objects.h:662
    ("uruk-hai shield", ArmorSlot::Shield, 1),    // objects.h:665
    ("orcish shield", ArmorSlot::Shield, 1),      // objects.h:668
    ("large shield", ArmorSlot::Shield, 2),       // objects.h:671
    ("dwarvish roundshield", ArmorSlot::Shield, 2), // objects.h:674
    ("shield of reflection", ArmorSlot::Shield, 2), // objects.h:677
    ("leather gloves", ArmorSlot::Gloves, 1),     // objects.h:686
    ("gauntlets of fumbling", ArmorSlot::Gloves, 1), // objects.h:689
    ("gauntlets of power", ArmorSlot::Gloves, 1), // objects.h:692
    ("gauntlets of dexterity", ArmorSlot::Gloves, 1), // objects.h:695
    ("low boots", ArmorSlot::Boots, 1),           // objects.h:700
    ("iron shoes", ArmorSlot::Boots, 2),          // objects.h:702
    ("high boots", ArmorSlot::Boots, 2),          // objects.h:704
    ("speed boots", ArmorSlot::Boots, 1),         // objects.h:707
    ("water walking boots", ArmorSlot::Boots, 1), // objects.h:709
    ("jumping boots", ArmorSlot::Boots, 1),       // objects.h:712
    ("elven boots", ArmorSlot::Boots, 1),         // objects.h:715
    ("kicking boots", ArmorSlot::Boots, 1),       // objects.h:718
    ("fumble boots", ArmorSlot::Boots, 1),        // objects.h:722
    ("levitation boots", ArmorSlot::Boots, 1),    // objects.h:725
];

fn c_armor_entry(item_name: &str) -> Option<(ArmorSlot, i32)> {
    let lower = item_name.trim().to_ascii_lowercase();
    C_ARMOR
        .iter()
        .find(|(name, _, _)| *name == lower)
        .map(|&(_, slot, a_ac)| (slot, a_ac))
}

/// Identifies the armor slot for an item by name (C `objects[otyp].oc_armcat`,
/// `include/objects.h:445-725`).
///
/// Exact C object names resolve through [`C_ARMOR`]. Names that are not C armor
/// objects fall back to a keyword heuristic; returns `None` if the item is not
/// recognisable as armor.
pub fn armor_slot(item_name: &str) -> Option<ArmorSlot> {
    if let Some((slot, _)) = c_armor_entry(item_name) {
        return Some(slot);
    }
    let lower = item_name.trim().to_ascii_lowercase();

    // Reject non-armor items that might have armor-like words in their name
    if lower.starts_with("scroll")
        || lower.starts_with("potion")
        || lower.starts_with("wand")
        || lower.starts_with("spellbook")
        || lower.starts_with("ring of")
        || lower.starts_with("amulet of")
        || lower.starts_with("book of")
    {
        return None;
    }

    if lower.contains("shield") {
        Some(ArmorSlot::Shield)
    } else if lower.contains("helmet") || lower.contains("helm") || lower.ends_with("cap") {
        Some(ArmorSlot::Helmet)
    } else if lower.contains("cloak")
        || lower.contains("robe")
        || lower.contains("smock")
        || lower.contains("wrapping")
    {
        Some(ArmorSlot::Cloak)
    } else if lower.contains("boots") || lower.contains("shoes") {
        Some(ArmorSlot::Boots)
    } else if lower.contains("gloves") || lower.contains("gauntlets") {
        Some(ArmorSlot::Gloves)
    } else if lower.contains("shirt") {
        Some(ArmorSlot::Shirt)
    } else if lower.contains("armor")
        || lower.contains("mail")
        || lower.contains("mithril")
        || lower.contains("dragon scales")
        || lower.contains("jacket")
    {
        Some(ArmorSlot::Suit)
    } else {
        None
    }
}

/// Base armor AC bonus (NetHack C `objects[otyp].a_ac`, stored as `10 - ac` by
/// the `ARMOR` macro, `include/objects.h:422-427`).
///
/// Looks the exact C object name up in [`C_ARMOR`]; a name that is not a C armor
/// object has no `a_ac` and returns 0.
pub fn armor_base_ac(item_name: &str) -> i32 {
    c_armor_entry(item_name).map_or(0, |(_, a_ac)| a_ac)
}

/// NetHack 5.0 C `ARM_BONUS(obj)` (`include/hack.h:1526-1528`).
///
/// Net AC bonus of a piece of armor: base `a_ac` plus enchantment `spe`,
/// minus erosion (capped at `a_ac`, so erosion cannot reduce `a_ac` below zero).
#[inline]
pub fn arm_bonus(a_ac: i32, spe: i32, erosion: u8) -> i32 {
    let ero = (erosion as i32).min(a_ac.max(0));
    a_ac + spe - ero
}

/// NetHack 5.0 C `find_ac(void)` (`src/do_wear.c:2473-2507`).
///
/// Calculates current hero armor class from `base_ac` (10 for human form,
/// `mons[u.umonnum].ac`), worn armor pieces `(a_ac, spe, erosion)`, and divine
/// `protection` (`u.ublessed`).
///
/// Clamped to `[-AC_MAX, AC_MAX]` (`[-99, 99]`).
pub fn find_ac(base_ac: i32, worn: &[(i32, i32, u8)], protection: i32) -> i32 {
    let mut uac = base_ac;
    for &(a_ac, spe, erosion) in worn {
        uac -= arm_bonus(a_ac, spe, erosion);
    }
    uac -= protection;
    uac.clamp(-AC_MAX, AC_MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arm_bonus_no_erosion() {
        assert_eq!(arm_bonus(2, 0, 0), 2);
        assert_eq!(arm_bonus(2, 2, 0), 4);
        assert_eq!(arm_bonus(5, -1, 0), 4);
    }

    #[test]
    fn test_arm_bonus_erosion_caps_at_base_ac() {
        // C hack.h:1527: min(erosion, a_ac)
        // Erosion 1 on a_ac 2 reduces bonus by 1
        assert_eq!(arm_bonus(2, 0, 1), 1);
        // Erosion 2 on a_ac 2 reduces bonus by 2 -> 0
        assert_eq!(arm_bonus(2, 0, 2), 0);
        // Erosion 3 (MAX_ERODE, obj.h:129) on a_ac 2 cannot reduce below 0
        assert_eq!(arm_bonus(2, 0, 3), 0);
        // With spe +1, bonus is 1 even with maximal erosion
        assert_eq!(arm_bonus(2, 1, 3), 1);
    }

    #[test]
    fn test_find_ac_naked_valkyrie_starts_at_10() {
        // C do_wear.c:2475: uac = mons[u.umonnum].ac (10 for human)
        assert_eq!(find_ac(10, &[], 0), 10);
    }

    #[test]
    fn test_find_ac_leather_armor_plus_zero() {
        // Leather armor a_ac = 2, spe = 0, erosion = 0 -> AC 8
        assert_eq!(find_ac(10, &[(2, 0, 0)], 0), 8);
    }

    #[test]
    fn test_find_ac_small_shield_plus_two() {
        // Small shield a_ac = 1, spe = 2, erosion = 0 -> bonus = 3
        // Alone: 10 - 3 = 7
        assert_eq!(find_ac(10, &[(1, 2, 0)], 0), 7);
        // With leather armor +0: 10 - 2 - 3 = 5
        assert_eq!(find_ac(10, &[(2, 0, 0), (1, 2, 0)], 0), 5);
    }

    #[test]
    fn test_find_ac_divine_protection() {
        // Divine protection reduces AC by protection amount
        assert_eq!(find_ac(10, &[], 3), 7);
        assert_eq!(find_ac(10, &[(2, 0, 0)], 3), 5);
    }

    #[test]
    fn test_find_ac_clamped_extremes() {
        // AC cannot exceed +99 or drop below -99 (C AC_MAX, you.h:472, do_wear.c:2505)
        assert_eq!(find_ac(10, &[(9, 100, 0)], 0), -99);
        assert_eq!(find_ac(10, &[(0, -150, 0)], 0), 99);
    }

    #[test]
    fn test_armor_slot_categorization() {
        assert_eq!(armor_slot("leather armor"), Some(ArmorSlot::Suit));
        assert_eq!(armor_slot("chain mail"), Some(ArmorSlot::Suit));
        assert_eq!(armor_slot("plate mail"), Some(ArmorSlot::Suit));
        assert_eq!(
            armor_slot("silver dragon scale mail"),
            Some(ArmorSlot::Suit)
        );
        assert_eq!(
            armor_slot("cloak of magic resistance"),
            Some(ArmorSlot::Cloak)
        );
        assert_eq!(armor_slot("small shield"), Some(ArmorSlot::Shield));
        assert_eq!(armor_slot("helmet"), Some(ArmorSlot::Helmet));
        assert_eq!(armor_slot("fedora"), Some(ArmorSlot::Helmet));
        assert_eq!(armor_slot("leather gloves"), Some(ArmorSlot::Gloves));
        assert_eq!(armor_slot("speed boots"), Some(ArmorSlot::Boots));
        assert_eq!(armor_slot("Hawaiian shirt"), Some(ArmorSlot::Shirt));

        // Non-armor items
        assert_eq!(armor_slot("dagger"), None);
        assert_eq!(armor_slot("scroll of enchant armor"), None);
        assert_eq!(armor_slot("potion of healing"), None);
        assert_eq!(armor_slot("wand of striking"), None);
        assert_eq!(armor_slot("ring of protection"), None);
        assert_eq!(armor_slot("amulet of guarding"), None);
    }

    #[test]
    fn test_armor_slot_mithril_and_mummy_wrapping() {
        // objects.h:571, :574 (ARM_SUIT); objects.h:611 CLOAK
        assert_eq!(armor_slot("dwarvish mithril-coat"), Some(ArmorSlot::Suit));
        assert_eq!(armor_slot("elven mithril-coat"), Some(ArmorSlot::Suit));
        assert_eq!(armor_slot("mummy wrapping"), Some(ArmorSlot::Cloak));
        assert_eq!(armor_slot("alchemy smock"), Some(ArmorSlot::Cloak));
        assert_eq!(armor_slot("T-shirt"), Some(ArmorSlot::Shirt));
    }

    #[test]
    fn test_armor_base_ac_specific_before_generic() {
        // a_ac = 10 - ac (objects.h:427)
        assert_eq!(armor_base_ac("bronze plate mail"), 6); // objects.h:562
        assert_eq!(armor_base_ac("studded leather armor"), 3); // objects.h:586
        assert_eq!(armor_base_ac("orcish chain mail"), 4); // objects.h:580
        assert_eq!(armor_base_ac("orcish ring mail"), 2); // objects.h:592
        assert_eq!(armor_base_ac("alchemy smock"), 1); // objects.h:630
        assert_eq!(armor_base_ac("scale mail"), 4); // objects.h:583
        assert_eq!(armor_base_ac("dwarvish mithril-coat"), 6); // objects.h:571
        assert_eq!(armor_base_ac("elven mithril-coat"), 5); // objects.h:574
        assert_eq!(armor_base_ac("mummy wrapping"), 0); // objects.h:611
        assert_eq!(armor_base_ac("elven cloak"), 1); // objects.h:615
        assert_eq!(armor_base_ac("silver dragon scales"), 3); // objects.h:534
        assert_eq!(armor_base_ac("not an armor"), 0);
    }

    #[test]
    fn test_c_armor_table_shape() {
        // 86 ARMOR_CLASS objects in objects.h:445-725; a_ac within [0, 9].
        assert_eq!(C_ARMOR.len(), 86);
        for &(name, slot, a_ac) in C_ARMOR {
            assert_eq!(armor_slot(name), Some(slot), "{name}");
            assert_eq!(armor_slot(&name.to_ascii_uppercase()), Some(slot), "{name}");
            assert!((0..=9).contains(&a_ac), "{name}");
        }
    }

    #[test]
    fn test_armor_base_ac_values() {
        assert_eq!(armor_base_ac("leather armor"), 2);
        assert_eq!(armor_base_ac("chain mail"), 5);
        assert_eq!(armor_base_ac("plate mail"), 7);
        assert_eq!(armor_base_ac("silver dragon scale mail"), 9);
        assert_eq!(armor_base_ac("small shield"), 1);
        assert_eq!(armor_base_ac("shield of reflection"), 2);
        assert_eq!(armor_base_ac("helmet"), 1);
        assert_eq!(armor_base_ac("dwarvish iron helm"), 2);
        assert_eq!(armor_base_ac("fedora"), 0);
        assert_eq!(armor_base_ac("cloak of magic resistance"), 1);
        assert_eq!(armor_base_ac("robe"), 2);
    }
}
