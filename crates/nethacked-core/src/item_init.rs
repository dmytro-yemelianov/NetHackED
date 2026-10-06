//! Supported BUC/spe initialization; excludes species, fuel, erosion and artifacts.

use nethacked_types::{Buc, ItemClass};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ItemInitProfile {
    #[default]
    Fixed,
    Weapon {
        multigen: bool,
        poisonable: bool,
    },
    Armor {
        harmful: bool,
    },
    Wand {
        charges: WandCharges,
    },
    Ring {
        charged: bool,
        harmful: bool,
    },
    Amulet {
        harmful: bool,
    },
    Potion,
    Scroll,
    Book,
    Tool {
        charges: ToolCharges,
    },
    Loadstone,
    Gem {
        rock: bool,
        luckstone: bool,
    },
    Container {
        container: ContainerInit,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ContainerInit {
    Chest,
    LargeBox,
    Sack,
    OilskinSack,
    HoldingBag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WandCharges {
    Standard,
    Wishing,
    Stasis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ToolCharges {
    Camera,
    Grease,
    CrystalBall,
    HornOrBag,
    Instrument,
    Bell,
    MagicLamp,
}

impl ItemInitProfile {
    pub fn is_fixed(&self) -> bool {
        *self == Self::Fixed
    }

    pub fn matches_class(self, class: ItemClass) -> bool {
        match self {
            Self::Fixed => true,
            Self::Weapon { .. } => class == ItemClass::Weapon,
            Self::Armor { .. } => class == ItemClass::Armor,
            Self::Wand { .. } => class == ItemClass::Wand,
            Self::Ring { .. } => class == ItemClass::Ring,
            Self::Amulet { .. } => class == ItemClass::Amulet,
            Self::Potion => class == ItemClass::Potion,
            Self::Scroll => class == ItemClass::Scroll,
            Self::Book => class == ItemClass::Spellbook,
            Self::Tool { .. } => class == ItemClass::Tool,
            Self::Loadstone | Self::Gem { .. } => class == ItemClass::Gem,
            Self::Container { .. } => class == ItemClass::Tool,
        }
    }
}

/// C `NetHack-5.0.0/src/rnd.c:196`: capped geometric roll limit.
/// The i8 ceiling only bounds malformed hero levels outside C's valid range.
pub fn rne_cap(hero_level: u32) -> u32 {
    if hero_level < 15 {
        5
    } else {
        (hero_level / 3).min(i8::MAX as u32)
    }
}

/// C `NetHack-5.0.0/src/mkobj.c:1841`: existing BUC is unchanged.
/// `rn2_chance` is clamped to chance-1; `rn2_2` is clamped to 1.
pub fn blessorcurse_result(buc: Buc, chance: u32, rn2_chance: u32, rn2_2: u32) -> Buc {
    if buc != Buc::Uncursed || chance == 0 || rn2_chance.min(chance - 1) != 0 {
        buc
    } else if rn2_2.min(1) == 0 {
        Buc::Cursed
    } else {
        Buc::Blessed
    }
}

/// C `NetHack-5.0.0/src/mkobj.c:1139`: +0 fallback, with bounded explicit rolls.
pub fn ring_zero_spe(rn2_4: u32, rn2_3: u32) -> i8 {
    rn2_4.min(3) as i8 - rn2_3.min(2) as i8
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn cap_matches_c(level in 0u32..128) {
            let reference = if level < 15 { 5 } else { level / 3 };
            prop_assert_eq!(rne_cap(level), reference);
        }
        #[test]
        fn buc_matches_c(chance in 1u32..30, roll in 0u32..30, sign in 0u32..4) {
            let roll = roll.min(chance - 1);
            let reference = if roll != 0 { Buc::Uncursed }
                else if sign.min(1) == 0 { Buc::Cursed } else { Buc::Blessed };
            prop_assert_eq!(blessorcurse_result(Buc::Uncursed, chance, roll, sign), reference);
            prop_assert_eq!(blessorcurse_result(Buc::Blessed, chance, roll, sign), Buc::Blessed);
        }
        #[test]
        fn ring_fallback_matches_c(a in 0u32..4, b in 0u32..3) {
            prop_assert_eq!(ring_zero_spe(a,b) as i32, a as i32 - b as i32);
        }
    }
}
