//! Item enchantment, erosion, proofing, and potion alchemy.
//!
//! Modeled in Lean 4 (`NetMechanics.Enchantment`).

use nethacked_types::Buc;

pub const MAX_EROSION: u8 = 4;

/// Highest weapon `spe` that a non-cursed enchant weapon scroll can never
/// evaporate (C `wield.c:999`: evaporation needs `spe > 5`).
pub const WEAPON_SAFE_LIMIT: i8 = 5;
/// Highest armor `spe` that enchant armor can never evaporate (C `read.c:1179`:
/// evaporation needs `s > 3`, or `s > 5` for elven / Wizard's cornuthaum).
pub const ARMOR_SAFE_LIMIT: i8 = 3;
/// [`ARMOR_SAFE_LIMIT`] for "special" armor (elven armor, Wizard's cornuthaum).
pub const SPECIAL_ARMOR_SAFE_LIMIT: i8 = 5;

/// Outcome of reading an enchant armor / enchant weapon scroll on an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnchantOutcome {
    /// The item survives with this new `spe` (may equal the old one).
    Changed(i8),
    /// The item violently glows and evaporates (is destroyed).
    Evaporated,
}

/// A C random draw an enchant routine performs, with its range.
///
/// `Rn2(n)` is C `rn2(n)` (range `0..n`), `Rnd(n)` is C `rnd(n)` (range `1..=n`),
/// `None` means C performs no draw at that point. `n >= 1` always.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnchantDraw {
    None,
    Rn2(u32),
    Rnd(u32),
}

impl EnchantDraw {
    /// Clamps `roll` into this draw's range (`Rn2(n)`: `0..=n-1`, `Rnd(n)`: `1..=n`).
    fn clamp(self, roll: u32) -> u32 {
        match self {
            EnchantDraw::None => 0,
            EnchantDraw::Rn2(n) => roll.min(n.saturating_sub(1)),
            EnchantDraw::Rnd(n) => roll.clamp(1, n.max(1)),
        }
    }
}

fn to_i8_saturating(v: i32) -> i8 {
    v.clamp(i32::from(i8::MIN), i32::from(i8::MAX)) as i8
}

/// Enchant armor's working value `s` before the evaporation check
/// (C `read.c:1115`, `seffect_enchant_armor`): `scursed ? -spe : spe`.
fn armor_s(spe: i8, buc: Buc) -> i32 {
    if buc == Buc::Cursed {
        -i32::from(spe)
    } else {
        i32::from(spe)
    }
}

/// Armor gain die before rolling (C `read.c:1180-1183`):
/// `s = (4 - s) / 2` (C division truncates toward zero, as does Rust `/`),
/// `+1` if special, `+1` if not `oc_magic`, `+1` if the scroll is blessed.
fn armor_gain_die(spe: i8, buc: Buc, is_elven_or_special: bool, is_magical: bool) -> i32 {
    let mut s = (4 - armor_s(spe, buc)) / 2;
    if is_elven_or_special {
        s += 1;
    }
    if !is_magical {
        s += 1;
    }
    if buc == Buc::Blessed {
        s += 1;
    }
    s
}

/// The evaporation draw of enchant armor (C `read.c:1179`):
/// `if (s > (special_armor ? 5 : 3) && rn2(s))` evaporates, so C draws `rn2(s)`
/// only when `s` exceeds the limit (`s = scursed ? -spe : spe`).
pub fn armor_evaporation_draw(spe: i8, buc: Buc, is_elven_or_special: bool) -> EnchantDraw {
    let s = armor_s(spe, buc);
    let limit = i32::from(if is_elven_or_special {
        SPECIAL_ARMOR_SAFE_LIMIT
    } else {
        ARMOR_SAFE_LIMIT
    });
    if s > limit {
        EnchantDraw::Rn2(s as u32)
    } else {
        EnchantDraw::None
    }
}

/// The gain draw of enchant armor (C `read.c:1180-1186`): `rnd(s)` when the gain
/// die `s > 0`; otherwise `!rn2(otmp->spe)` when `otmp->spe > 0`; otherwise none.
pub fn armor_gain_draw(
    spe: i8,
    buc: Buc,
    is_elven_or_special: bool,
    is_magical: bool,
) -> EnchantDraw {
    let s = armor_gain_die(spe, buc, is_elven_or_special, is_magical);
    if s > 0 {
        EnchantDraw::Rnd(s as u32)
    } else if spe > 0 {
        EnchantDraw::Rn2(spe as u32)
    } else {
        EnchantDraw::None
    }
}

/// Whether enchant armor evaporates the armor (C `read.c:1179`):
/// `s > (special ? 5 : 3) && rn2(s)`, with `s = scursed ? -spe : spe`.
/// `evaporate_roll` is the `rn2(s)` draw (range `0..s`, clamped to `s-1`); it is
/// ignored when `s` is within the safe limit. Callers use this to skip the gain draw
/// after evaporation, as C does.
pub fn armor_evaporates(spe: i8, buc: Buc, is_elven_or_special: bool, evaporate_roll: u32) -> bool {
    let evap = armor_evaporation_draw(spe, buc, is_elven_or_special);
    evap != EnchantDraw::None && evap.clamp(evaporate_roll) != 0
}

/// Reads a scroll of enchant armor on a piece of armor with enchantment `spe`
/// (C `read.c:1115` `seffect_enchant_armor`, evaporation `read.c:1179`).
///
/// - `s = scursed ? -spe : spe`; if `s > 3` (`> 5` when `is_elven_or_special`)
///   and `rn2(s) != 0` the armor evaporates (probability `(s-1)/s`).
/// - Otherwise `s = (4 - s) / 2`, `+1` special, `+1` if not `is_magical`
///   (`oc_magic`), `+1` blessed; if `s <= 0` the gain is `1` when
///   `spe > 0 && !rn2(spe)`, else `0`; otherwise the gain is `rnd(s)`. The gain is
///   capped at 11 and negated for a cursed scroll.
///
/// Rolls (see [`armor_evaporation_draw`] / [`armor_gain_draw`] for when C draws them):
/// - `evaporate_roll`: the `rn2(s)` draw, range `0..s`; larger values clamp to `s-1`.
///   Ignored when `s` is within the safe limit.
/// - `gain_roll`: `rnd(s)` (range `1..=s`, clamped into it) or `rn2(spe)` (range
///   `0..spe`, clamped to `spe-1`) per [`armor_gain_draw`]; ignored when no draw.
///
/// The new enchantment saturates at `i8::MIN`/`i8::MAX` (C `cap_spe` is not modelled).
pub fn enchant_armor(
    spe: i8,
    buc: Buc,
    is_elven_or_special: bool,
    is_magical: bool,
    evaporate_roll: u32,
    gain_roll: u32,
) -> EnchantOutcome {
    if armor_evaporates(spe, buc, is_elven_or_special, evaporate_roll) {
        return EnchantOutcome::Evaporated;
    }
    let gain = match armor_gain_draw(spe, buc, is_elven_or_special, is_magical) {
        EnchantDraw::None => 0,
        d @ EnchantDraw::Rn2(_) => i32::from(d.clamp(gain_roll) == 0),
        d @ EnchantDraw::Rnd(_) => d.clamp(gain_roll) as i32,
    }
    .min(11);
    let delta = if buc == Buc::Cursed { -gain } else { gain };
    EnchantOutcome::Changed(to_i8_saturating(i32::from(spe) + delta))
}

/// The amount draw of enchant weapon (C `read.c:1667`):
/// `scursed ? -1 : spe >= 9 ? (rn2(spe) == 0) : sblessed ? rnd(3 - spe/3) : 1`.
pub fn weapon_gain_draw(spe: i8, buc: Buc) -> EnchantDraw {
    if buc == Buc::Cursed {
        EnchantDraw::None
    } else if spe >= 9 {
        EnchantDraw::Rn2(spe as u32)
    } else if buc == Buc::Blessed {
        EnchantDraw::Rnd((3 - i32::from(spe) / 3) as u32)
    } else {
        EnchantDraw::None
    }
}

/// The evaporation draw of `chwepon` (C `wield.c:999-1000`):
/// `((spe > 5 && amount >= 0) || (spe < -5 && amount < 0)) && rn2(3)`.
/// `amount < 0` exactly when the scroll is cursed.
pub fn weapon_evaporation_draw(spe: i8, buc: Buc) -> EnchantDraw {
    let cursed = buc == Buc::Cursed;
    if (!cursed && spe > WEAPON_SAFE_LIMIT) || (cursed && spe < -WEAPON_SAFE_LIMIT) {
        EnchantDraw::Rn2(3)
    } else {
        EnchantDraw::None
    }
}

/// Reads a scroll of enchant weapon on the wielded weapon with enchantment `spe`
/// (C `read.c:1667` amount, `wield.c:918` `chwepon`, evaporation `wield.c:999-1000`).
///
/// - Amount: cursed `-1`; `spe >= 9`: `1` if `rn2(spe) == 0` else `0`; blessed
///   `rnd(3 - spe/3)`; otherwise `1`.
/// - Evaporates when `rn2(3) != 0` (2/3) if `spe > 5` with a non-cursed scroll or
///   `spe < -5` with a cursed one.
///
/// Rolls (C draws the amount first, then the evaporation roll):
/// - `evaporate_roll`: the `rn2(3)` draw, range `0..=2`; larger values clamp to 2.
/// - `gain_roll`: `rn2(spe)` (range `0..spe`, clamped to `spe-1`) or
///   `rnd(3 - spe/3)` (range `1..=3 - spe/3`, clamped into it) per
///   [`weapon_gain_draw`]; ignored when no draw.
///
/// The "no wielded weapon" case is the caller's. Saturates at the `i8` bounds.
pub fn enchant_weapon(spe: i8, buc: Buc, evaporate_roll: u32, gain_roll: u32) -> EnchantOutcome {
    let amount: i32 = if buc == Buc::Cursed {
        -1
    } else {
        match weapon_gain_draw(spe, buc) {
            d @ EnchantDraw::Rn2(_) => i32::from(d.clamp(gain_roll) == 0),
            d @ EnchantDraw::Rnd(_) => d.clamp(gain_roll) as i32,
            EnchantDraw::None => 1,
        }
    };
    let evap = weapon_evaporation_draw(spe, buc);
    if evap != EnchantDraw::None && evap.clamp(evaporate_roll) != 0 {
        return EnchantOutcome::Evaporated;
    }
    EnchantOutcome::Changed(to_i8_saturating(i32::from(spe) + amount))
}

/// Whether armor counts as elven (C `is_elven_armor`, `objclass.h`), by item name.
/// Item records carry no object kind, so the sim classifies by name.
pub fn armor_is_elven(name: &str) -> bool {
    name.to_lowercase().contains("elven")
}

/// Whether armor has `oc_magic` set (C `include/objects.h`), by item name: dragon
/// scale mail (`objects.h:502-525`; plain dragon scales are non-magic, `:528-552`),
/// cornuthaum, dunce cap, helms of brilliance / caution / opposite alignment /
/// telepathy, gauntlets of power / fumbling / dexterity, speed / water walking /
/// jumping / elven / kicking / fumble / levitation boots, magic cloaks (protection,
/// invisibility, magic resistance, displacement), elven cloak, alchemy smock, robe
/// and shields of drain resistance / shock resistance / reflection.
pub fn armor_is_magical(name: &str) -> bool {
    const MAGIC: [&str; 15] = [
        "dragon scale mail",
        "cornuthaum",
        "dunce cap",
        "helm of ",
        "gauntlets of ",
        "speed boots",
        "water walking boots",
        "jumping boots",
        "elven boots",
        "kicking boots",
        "fumble boots",
        "levitation boots",
        "cloak of ",
        "elven cloak",
        "shield of ",
    ];
    let n = name.to_lowercase();
    MAGIC.iter().any(|m| n.contains(m)) || n.contains("alchemy smock") || n.ends_with("robe")
}

/// Applies corrosive exposure (acid, rust, fire, water).
/// Proven in the Lean 4 model to be monotonic and preserve proofed items.
pub fn apply_erosion(cur_erosion: u8, proofed: bool) -> u8 {
    if proofed {
        cur_erosion
    } else if cur_erosion < MAX_EROSION {
        cur_erosion + 1
    } else {
        cur_erosion
    }
}

/// Canonical potion alchemy dipping matrix.
/// Proven in the Lean 4 model to be deterministic and commutative for complementary reagents.
pub fn mix_alchemy(pot_a: &str, pot_b: &str) -> Option<&'static str> {
    let a = pot_a.to_lowercase();
    let b = pot_b.to_lowercase();

    if a.contains("healing") && b.contains("speed") || a.contains("speed") && b.contains("healing")
    {
        Some("potion of extra healing")
    } else if a.contains("water") || b.contains("water") {
        Some("potion of water")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use EnchantOutcome::{Changed, Evaporated};

    const ROLLS: [u32; 5] = [0, 1, 2, 3, u32::MAX];

    #[test]
    fn armor_plain_threshold_3_vs_4() {
        // spe 3 (s = 3, not > 3): never evaporates; s=(4-3)/2=0, +1 non-magic -> rnd(1).
        for r in ROLLS {
            assert_eq!(
                enchant_armor(3, Buc::Uncursed, false, false, r, 1),
                Changed(4)
            );
        }
        // spe 4: rn2(4) != 0 evaporates (3/4), 0 survives.
        assert_eq!(
            enchant_armor(4, Buc::Uncursed, false, false, 0, 1),
            Changed(5)
        );
        assert_eq!(
            enchant_armor(4, Buc::Uncursed, false, false, 1, 1),
            Evaporated
        );
        assert_eq!(
            enchant_armor(4, Buc::Uncursed, false, false, 3, 1),
            Evaporated
        );
        assert_eq!(
            enchant_armor(4, Buc::Uncursed, false, false, u32::MAX, 1),
            Evaporated
        );
    }

    #[test]
    fn armor_special_threshold_5_vs_6() {
        for r in ROLLS {
            assert_ne!(
                enchant_armor(5, Buc::Uncursed, true, true, r, 1),
                Evaporated
            );
        }
        assert_eq!(
            enchant_armor(6, Buc::Uncursed, true, true, 1, 1),
            Evaporated
        );
        assert_eq!(
            enchant_armor(6, Buc::Uncursed, true, true, 5, 1),
            Evaporated
        );
        // survives: s=(4-6)/2=-1, +1 special = 0 -> !rn2(6) gives +1 on 0, else 0.
        assert_eq!(
            enchant_armor(6, Buc::Uncursed, true, true, 0, 0),
            Changed(7)
        );
        assert_eq!(
            enchant_armor(6, Buc::Uncursed, true, true, 0, 5),
            Changed(6)
        );
    }

    #[test]
    fn armor_gain_ranges() {
        // blessed non-magic +0: s = 2 + 1 + 1 = 4 -> rnd(4); rolls clamp to 1..=4.
        assert_eq!(
            enchant_armor(0, Buc::Blessed, false, false, 0, 0),
            Changed(1)
        );
        assert_eq!(
            enchant_armor(0, Buc::Blessed, false, false, 0, 1),
            Changed(1)
        );
        assert_eq!(
            enchant_armor(0, Buc::Blessed, false, false, 0, 4),
            Changed(4)
        );
        assert_eq!(
            enchant_armor(0, Buc::Blessed, false, false, 0, 99),
            Changed(4)
        );
        // magic uncursed +3: s = 0 -> !rn2(3).
        assert_eq!(
            enchant_armor(3, Buc::Uncursed, false, true, 0, 0),
            Changed(4)
        );
        assert_eq!(
            enchant_armor(3, Buc::Uncursed, false, true, 0, 2),
            Changed(3)
        );
        // cursed +0 non-magic: s = (4-0)/2 + 1 = 3 -> -rnd(3).
        assert_eq!(
            enchant_armor(0, Buc::Cursed, false, false, 0, 3),
            Changed(-3)
        );
        // cursed -4: s = 4 > 3 -> rn2(4) evaporation check.
        assert_eq!(
            enchant_armor(-4, Buc::Cursed, false, false, 1, 1),
            Evaporated
        );
        assert_eq!(
            enchant_armor(-4, Buc::Cursed, false, false, 0, 1),
            Changed(-5)
        );
        // very negative: gain capped at 11.
        assert_eq!(
            enchant_armor(-40, Buc::Uncursed, false, false, 0, 99),
            Changed(-29)
        );
    }

    #[test]
    fn armor_i8_extremes_saturate() {
        assert_eq!(
            enchant_armor(127, Buc::Blessed, true, false, 0, 0),
            Changed(127)
        );
        assert_eq!(
            enchant_armor(-128, Buc::Uncursed, false, false, 0, 999),
            Changed(-117)
        );
        assert_eq!(
            enchant_armor(-128, Buc::Cursed, false, false, 0, 0),
            Changed(-128)
        );
        assert_eq!(
            enchant_armor(-128, Buc::Cursed, false, false, 1, 0),
            Evaporated
        );
        assert_eq!(
            enchant_armor(127, Buc::Cursed, false, false, 0, 999),
            Changed(116)
        );
    }

    #[test]
    fn weapon_threshold_5_vs_6() {
        for r in ROLLS {
            assert_eq!(enchant_weapon(5, Buc::Uncursed, r, 1), Changed(6));
        }
        assert_eq!(enchant_weapon(6, Buc::Uncursed, 0, 1), Changed(7));
        assert_eq!(enchant_weapon(6, Buc::Uncursed, 1, 1), Evaporated);
        assert_eq!(enchant_weapon(6, Buc::Uncursed, 2, 1), Evaporated);
        assert_eq!(enchant_weapon(6, Buc::Uncursed, u32::MAX, 1), Evaporated);
    }

    #[test]
    fn weapon_gain_ranges() {
        // blessed rnd(3 - spe/3): 1..3 below +3, 1..2 at +3..+5, 1 at +6..+8.
        assert_eq!(enchant_weapon(0, Buc::Blessed, 0, 0), Changed(1));
        assert_eq!(enchant_weapon(0, Buc::Blessed, 0, 3), Changed(3));
        assert_eq!(enchant_weapon(0, Buc::Blessed, 0, 99), Changed(3));
        assert_eq!(enchant_weapon(3, Buc::Blessed, 0, 99), Changed(5));
        assert_eq!(enchant_weapon(6, Buc::Blessed, 0, 99), Changed(7));
        // spe >= 9: amount = (rn2(spe) == 0).
        assert_eq!(enchant_weapon(9, Buc::Blessed, 0, 0), Changed(10));
        assert_eq!(enchant_weapon(9, Buc::Uncursed, 0, 5), Changed(9));
        assert_eq!(enchant_weapon(9, Buc::Uncursed, 1, 5), Evaporated);
        // cursed: -1, evaporates only below -5.
        assert_eq!(enchant_weapon(10, Buc::Cursed, 2, 0), Changed(9));
        assert_eq!(enchant_weapon(-5, Buc::Cursed, 2, 0), Changed(-6));
        assert_eq!(enchant_weapon(-6, Buc::Cursed, 1, 0), Evaporated);
        assert_eq!(enchant_weapon(-6, Buc::Cursed, 0, 0), Changed(-7));
    }

    #[test]
    fn weapon_i8_extremes_saturate() {
        assert_eq!(enchant_weapon(127, Buc::Uncursed, 0, 0), Changed(127));
        assert_eq!(enchant_weapon(-128, Buc::Cursed, 0, 0), Changed(-128));
        assert_eq!(enchant_weapon(-128, Buc::Blessed, 0, 999), Changed(-83));
    }

    #[test]
    fn draws_match_c_call_sites() {
        assert_eq!(
            armor_evaporation_draw(3, Buc::Uncursed, false),
            EnchantDraw::None
        );
        assert_eq!(
            armor_evaporation_draw(4, Buc::Uncursed, false),
            EnchantDraw::Rn2(4)
        );
        assert_eq!(
            armor_evaporation_draw(-6, Buc::Cursed, true),
            EnchantDraw::Rn2(6)
        );
        assert_eq!(
            armor_gain_draw(0, Buc::Blessed, false, false),
            EnchantDraw::Rnd(4)
        );
        assert_eq!(
            armor_gain_draw(3, Buc::Uncursed, false, true),
            EnchantDraw::Rn2(3)
        );
        assert_eq!(
            armor_gain_draw(-4, Buc::Cursed, false, true),
            EnchantDraw::None
        );
        assert_eq!(weapon_gain_draw(0, Buc::Blessed), EnchantDraw::Rnd(3));
        assert_eq!(weapon_gain_draw(0, Buc::Uncursed), EnchantDraw::None);
        assert_eq!(weapon_gain_draw(9, Buc::Uncursed), EnchantDraw::Rn2(9));
        assert_eq!(weapon_gain_draw(9, Buc::Cursed), EnchantDraw::None);
        assert_eq!(weapon_evaporation_draw(5, Buc::Blessed), EnchantDraw::None);
        assert_eq!(
            weapon_evaporation_draw(6, Buc::Blessed),
            EnchantDraw::Rn2(3)
        );
        assert_eq!(weapon_evaporation_draw(6, Buc::Cursed), EnchantDraw::None);
        assert_eq!(
            weapon_evaporation_draw(-6, Buc::Cursed),
            EnchantDraw::Rn2(3)
        );
    }

    #[test]
    fn armor_name_classifiers() {
        assert!(armor_is_elven("elven mithril-coat"));
        assert!(!armor_is_elven("plate mail"));
        assert!(armor_is_magical("cloak of magic resistance"));
        assert!(armor_is_magical("silver dragon scale mail"));
        assert!(!armor_is_magical("silver dragon scales"));
        assert!(armor_is_magical("shield of drain resistance"));
        assert!(armor_is_magical("helm of caution"));
        assert!(!armor_is_magical("plate mail"));
        assert!(!armor_is_magical("leather armor"));
    }
}
