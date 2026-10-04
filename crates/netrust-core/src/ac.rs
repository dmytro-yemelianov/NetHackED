//! Armor Class (AC) and worn armor calculation.
//!
//! Faithful to NetHack 5.0 C `ARM_BONUS(obj)` (`include/hack.h:1526-1528`)
//! and `find_ac(void)` (`src/do_wear.c:2473-2507`).

use serde::{Deserialize, Serialize};

/// Maximum absolute value of AC in NetHack 5.0 (C `AC_MAX`, `include/you.h:472`).
pub const AC_MAX: i32 = 99;

/// Canonical armor slots in NetHack (C `ARM_SUIT` .. `ARM_SHIRT`, `include/hack.h:87-95`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ArmorSlot {
    Suit,
    Cloak,
    Helmet,
    Shield,
    Gloves,
    Boots,
    Shirt,
}

/// Identifies the armor slot for an item by name (C `objects[otyp].oc_armcat`, `include/objects.h`).
///
/// Returns `None` if the item is not armor or is of another item class.
pub fn armor_slot(item_name: &str) -> Option<ArmorSlot> {
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
    } else if lower.contains("helmet")
        || lower.contains("helm")
        || lower.contains("hat")
        || lower.contains("fedora")
        || lower.contains("cornuthaum")
        || lower.contains("dunce cap")
        || lower.contains("dented pot")
        || lower.ends_with("cap")
    {
        Some(ArmorSlot::Helmet)
    } else if lower.contains("cloak")
        || lower.contains("robe")
        || lower.contains("smock")
        || lower.contains("cape")
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
        || lower.contains("dragon scale")
        || lower.contains("dragon scales")
        || lower.contains("jacket")
        || lower.contains("suit")
    {
        Some(ArmorSlot::Suit)
    } else {
        None
    }
}

/// Base armor AC bonus (NetHack C `objects[otyp].a_ac`, `include/objects.h`).
///
/// NetHack C stores `a_ac = 10 - ac` in the objects table (`include/objects.h:427`).
/// For catalog items not found in `ITEM_CATALOG`, this returns the standard C value.
pub fn armor_base_ac(item_name: &str) -> i32 {
    let lower = item_name.trim().to_ascii_lowercase();

    // Suits
    if lower.contains("dragon scale mail") {
        9
    } else if lower.contains("crystal plate mail") || lower.contains("plate mail") {
        7
    } else if lower.contains("splint mail")
        || lower.contains("banded mail")
        || lower.contains("bronze plate mail")
    {
        6
    } else if lower.contains("chain mail") || lower.contains("scale mail") {
        5
    } else if lower.contains("dragon scales") || lower.contains("ring mail") {
        3
    } else if lower.contains("leather armor") || lower.contains("studded leather armor") {
        2
    } else if lower.contains("leather jacket") {
        1
    }
    // Cloaks
    else if lower.contains("cloak of protection") {
        3
    } else if lower.contains("robe") {
        2
    } else if lower.contains("cloak") {
        if lower.contains("dwarvish") || lower.contains("orcish") {
            0
        } else {
            1
        }
    }
    // Helmets
    else if lower.contains("dwarvish iron helm") {
        2
    } else if lower.contains("fedora")
        || lower.contains("cornuthaum")
        || lower.contains("dunce cap")
    {
        0
    } else if lower.contains("helm") || lower.contains("helmet") || lower.contains("dented pot") {
        1
    }
    // Shields
    else if lower.contains("shield of reflection")
        || lower.contains("large shield")
        || lower.contains("elven shield")
        || lower.contains("dwarvish roundshield")
    {
        2
    } else if lower.contains("shield") {
        1 // small shield, orcish shield, uruk-hai shield
    }
    // Boots
    else if lower.contains("iron shoes") || lower.contains("high boots") {
        2
    } else if lower.contains("boots")
        // Gloves
        || lower.contains("gloves")
        || lower.contains("gauntlets")
    {
        1
    }
    // Shirts (Hawaiian shirt, T-shirt) and unrecognized items give 0 base AC bonus
    else {
        0
    }
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
        // Erosion 5 on a_ac 2 cannot reduce below 0 from erosion alone
        assert_eq!(arm_bonus(2, 0, 5), 0);
        // With spe +1, bonus is 1 even with heavy erosion
        assert_eq!(arm_bonus(2, 1, 5), 1);
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
