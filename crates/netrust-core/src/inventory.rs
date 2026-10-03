//! Inventory, Container Hierarchy, and Encumbrance Mechanics.
//!
//! Formalized and verified in `NetMechanics.Inventory`.
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
                        Buc::Blessed => (inner_wt + 3) / 4,
                        Buc::Uncursed => (inner_wt + 1) / 2,
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

/// Check whether an item can be safely inserted into a container without explosion.
/// In NetHack (pickup.c:2658 mbag_explodes), placing a Bag of Holding inside
/// another Bag of Holding triggers an immediate magical explosion destroying both.
pub fn can_insert_safe(item: &Item, container: &Item) -> bool {
    match container {
        Item::Box {
            is_bag_of_holding: true,
            ..
        } => match item {
            Item::Box {
                is_bag_of_holding: true,
                ..
            } => false,
            _ => true,
        },
        Item::Box {
            is_bag_of_holding: false,
            ..
        } => true,
        Item::Single { .. } => false,
    }
}

/// Convenience check using boolean flags.
#[inline]
pub fn can_insert_safe_flags(item_is_boh: bool, container_is_container: bool, container_is_boh: bool) -> bool {
    container_is_container && !(item_is_boh && container_is_boh)
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

pub fn calculate_encumbrance(weight: u32, capacity: u32) -> EncumbranceTier {
    if capacity == 0 {
        EncumbranceTier::Overloaded
    } else if weight <= capacity {
        EncumbranceTier::Unencumbered
    } else if weight <= capacity + capacity / 2 {
        EncumbranceTier::Burdened
    } else if weight <= capacity * 2 {
        EncumbranceTier::Stressed
    } else if weight <= capacity * 2 + capacity / 2 {
        EncumbranceTier::Strained
    } else if weight <= capacity * 3 {
        EncumbranceTier::Overtaxed
    } else {
        EncumbranceTier::Overloaded
    }
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

    #[test]
    fn test_boh_cannot_contain_boh() {
        let boh1 = Item::Box {
            name: "outer bag".into(),
            base_weight: 15,
            buc: Buc::Blessed,
            is_bag_of_holding: true,
            contents: vec![],
        };
        let boh2 = Item::Box {
            name: "inner bag".into(),
            base_weight: 15,
            buc: Buc::Blessed,
            is_bag_of_holding: true,
            contents: vec![],
        };
        assert!(!can_insert_safe(&boh2, &boh1));
    }

    #[test]
    fn test_encumbrance_tiers() {
        let cap = 100;
        assert_eq!(calculate_encumbrance(50, cap), EncumbranceTier::Unencumbered);
        assert_eq!(calculate_encumbrance(100, cap), EncumbranceTier::Unencumbered);
        assert_eq!(calculate_encumbrance(120, cap), EncumbranceTier::Burdened);
        assert_eq!(calculate_encumbrance(160, cap), EncumbranceTier::Stressed);
        assert_eq!(calculate_encumbrance(220, cap), EncumbranceTier::Strained);
        assert_eq!(calculate_encumbrance(280, cap), EncumbranceTier::Overtaxed);
        assert_eq!(calculate_encumbrance(350, cap), EncumbranceTier::Overloaded);
    }
}
