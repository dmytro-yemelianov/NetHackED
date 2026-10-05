//! Random monster selection, ported from C `makemon.c` `rndmonst_adj()`
//! (lines 1659-1733) over the ruleset's data, so pack-added species with a
//! `frequency` take part like C ones.
//!
//! Not modelled yet: the quest's `qt_montype()` (1673), the rogue level's
//! uppercase-only rule (1679) and the elemental planes' `wrong_elem_type()`.

use crate::items::{ItemKindId, BOXIPROBS, HELLPROBS, ITEM_CATALOG, MKOBJPROBS, ROGUEPROBS};
use crate::monsters::MonsterSpeciesId;
use crate::ruleset::{MonsterDef, Ruleset};
use nethacked_types::ItemClass;

/// C `ALIGNWEIGHT` (global.h:411).
const ALIGNWEIGHT: i32 = 4;

/// A level's or dungeon's alignment (C `AM_*`, from `dat/dungeon.lua`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DungeonAlign {
    /// `AM_NONE`: the Dungeons of Doom ("unaligned") and Gehennom ("noalign").
    None,
    /// `AM_LAWFUL`: the Gnomish Mines.
    Lawful,
    /// `AM_NEUTRAL`: the Oracle level.
    Neutral,
    /// `AM_CHAOTIC`: the Medusa level.
    Chaotic,
}

/// What `rndmonst()` reads from the game state.
#[derive(Debug, Clone, Copy)]
pub struct MonsterGenContext {
    /// C `level_difficulty()` (dungeon.c:2027): usually the depth.
    pub level_difficulty: i32,
    /// Hero experience level (`u.ulevel`).
    pub hero_level: i32,
    /// `Inhell`.
    pub in_hell: bool,
    /// Alignment of the special level, else of the dungeon.
    pub align: DungeonAlign,
    /// Level temperature: > 0 hot, < 0 cold, 0 neither.
    pub temperature: i32,
}

fn has(def: &MonsterDef, flag: &str) -> bool {
    def.gen_flags.iter().any(|f| f == flag)
}

/// C `align_shift()` (makemon.c:1611-1637), 0..=5.
fn align_shift(def: &MonsterDef, align: DungeonAlign) -> i32 {
    let mal = i32::from(def.maligntyp);
    match align {
        DungeonAlign::None => 0,
        DungeonAlign::Lawful => (mal + 20) / (2 * ALIGNWEIGHT),
        DungeonAlign::Neutral => (20 - mal.abs()) / ALIGNWEIGHT,
        DungeonAlign::Chaotic => (-(mal - 20)) / (2 * ALIGNWEIGHT),
    }
}

/// C `temperature_shift()` (makemon.c:1641-1648).
fn temperature_shift(def: &MonsterDef, temperature: i32) -> i32 {
    let resists = match temperature.signum() {
        1 => def.intrinsics.fire_resistance,
        -1 => def.intrinsics.cold_resistance,
        _ => false,
    };
    if resists {
        3
    } else {
        0
    }
}

/// C `uncommon()` (makemon.c:1593-1604); `gone` is `mvitals[].mvflags & G_GONE`.
fn uncommon(def: &MonsterDef, in_hell: bool, gone: bool) -> bool {
    if has(def, "nogen") || has(def, "uniq") || gone {
        return true;
    }
    if in_hell {
        def.maligntyp > 0
    } else {
        has(def, "hell")
    }
}

/// C `rndmonst_adj(minadj, maxadj)`: weighted reservoir sampling over every
/// species from `LOW_PM` up to `SPECIAL_PM` (the long worm tail; player and
/// quest monsters after it are never random), plus pack-added species.
/// `gone` reports genocided/extinct species; `rn2(n)` must return `0..n`.
pub fn rndmonst_adj<'a>(
    ruleset: &'a Ruleset,
    ctx: &MonsterGenContext,
    minadj: i32,
    maxadj: i32,
    gone: impl Fn(&MonsterDef) -> bool,
    mut rn2: impl FnMut(u32) -> u32,
) -> Option<&'a MonsterDef> {
    let zlevel = ctx.level_difficulty;
    // monst.h:259-260 monmin_difficulty / monmax_difficulty
    let minmlev = zlevel / 6 + minadj;
    let maxmlev = (zlevel + ctx.hero_level) / 2 + maxadj;
    let special = MonsterSpeciesId::LONG_WORM_TAIL;

    let mut total: i32 = 0;
    let mut selected: Option<&MonsterDef> = None;
    for def in &ruleset.monsters {
        if def.id.is_some_and(|id| id >= special) {
            continue;
        }
        let difficulty = def.difficulty as i32;
        if difficulty < minmlev || difficulty > maxmlev {
            continue;
        }
        if uncommon(def, ctx.in_hell, gone(def)) {
            continue;
        }
        if ctx.in_hell && has(def, "nohell") {
            continue;
        }
        let weight = def.frequency as i32
            + align_shift(def, ctx.align)
            + temperature_shift(def, ctx.temperature);
        if weight > 0 {
            total += weight;
            if (rn2(total as u32) as i32) < weight {
                selected = Some(def);
            }
        }
    }
    selected
}

/// C `rndmonst()`: `rndmonst_adj(0, 0)`.
pub fn rndmonst<'a>(
    ruleset: &'a Ruleset,
    ctx: &MonsterGenContext,
    gone: impl Fn(&MonsterDef) -> bool,
    rn2: impl FnMut(u32) -> u32,
) -> Option<&'a MonsterDef> {
    rndmonst_adj(ruleset, ctx, 0, 0, gone, rn2)
}

/// Which C class table `mkobj(RANDOM_CLASS)` rolls (mkobj.c:275-278).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectTable {
    /// `mkobjprobs[]`: the ordinary dungeon.
    Dungeon,
    /// `hellprobs[]`: Gehennom.
    Hell,
    /// `rogueprobs[]`: the rogue level.
    Rogue,
    /// `boxiprobs[]`: container contents (`mkbox_cnts`).
    Box,
}

/// C `mkobj(oclass, ...)` kind selection (mkobj.c:270-300): a class from the
/// table by `rnd(100)` when `class` is `None`, then a kind weighted by `oc_prob`
/// over that class (`rnd(oclass_prob_totals[oclass])`). Artifacts never come
/// out of this roll. `rn2(n)` must return `0..n`.
pub fn mkobj_kind(
    class: Option<ItemClass>,
    table: ObjectTable,
    mut rn2: impl FnMut(u32) -> u32,
) -> Option<ItemKindId> {
    let class = match class {
        Some(c) => c,
        None => {
            let probs = match table {
                ObjectTable::Dungeon => MKOBJPROBS,
                ObjectTable::Hell => HELLPROBS,
                ObjectTable::Rogue => ROGUEPROBS,
                ObjectTable::Box => BOXIPROBS,
            };
            let mut tprob = rn2(100) as i32 + 1;
            let mut pick = probs[probs.len() - 1].1;
            for &(p, c) in probs {
                tprob -= p as i32;
                if tprob <= 0 {
                    pick = c;
                    break;
                }
            }
            pick
        }
    };
    let kinds = || {
        ITEM_CATALOG
            .iter()
            .filter(move |a| a.class == class && !a.artifact)
    };
    let total: u32 = kinds().map(|a| a.prob).sum();
    if total == 0 {
        return None;
    }
    let mut prob = rn2(total) as i64 + 1;
    for a in kinds() {
        prob -= i64::from(a.prob);
        if prob <= 0 {
            return Some(a.id);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(depth: i32, ulevel: i32) -> MonsterGenContext {
        MonsterGenContext {
            level_difficulty: depth,
            hero_level: ulevel,
            in_hell: false,
            align: DungeonAlign::None,
            temperature: 0,
        }
    }

    #[test]
    fn picks_stay_in_the_c_difficulty_window_and_skip_uncommon() {
        let rs = Ruleset::vanilla();
        let mut seed = 0x2545_f491_u32;
        let mut rn2 = move |n: u32| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed % n
        };
        for (depth, ulevel) in [(1, 1), (5, 3), (12, 10), (30, 20)] {
            let c = ctx(depth, ulevel);
            for _ in 0..200 {
                let m = rndmonst(&rs, &c, |_| false, &mut rn2).expect("a species");
                let d = m.difficulty as i32;
                assert!(
                    d >= depth / 6 && d <= (depth + ulevel) / 2,
                    "{} d{d}",
                    m.name
                );
                assert!(
                    !has(m, "nogen") && !has(m, "uniq") && !has(m, "hell"),
                    "{}",
                    m.name
                );
                assert!(m.id.unwrap() < MonsterSpeciesId::LONG_WORM_TAIL);
            }
        }
    }

    #[test]
    fn genocided_species_are_never_picked() {
        let rs = Ruleset::vanilla();
        let mut i = 0u32;
        let rn2 = move |n: u32| {
            i = i.wrapping_add(7919);
            i % n
        };
        let c = ctx(1, 1);
        let mut rn2 = rn2;
        for _ in 0..200 {
            let m = rndmonst(&rs, &c, |d| d.glyph == 'r', &mut rn2).unwrap();
            assert_ne!(m.glyph, 'r');
        }
    }

    #[test]
    fn mkobj_follows_the_c_weights() {
        let mut n = 0u32;
        let mut rn2 = move |m: u32| {
            n = n.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            (n >> 8) % m
        };
        let mut counts = std::collections::HashMap::new();
        for _ in 0..20_000 {
            let id = mkobj_kind(None, ObjectTable::Dungeon, &mut rn2).unwrap();
            let a = &ITEM_CATALOG[id.index()];
            assert!(!a.artifact && a.prob > 0, "{}", a.name);
            *counts.entry(a.class).or_insert(0u32) += 1;
        }
        // mkobjprobs: food 20%, amulets 1%.
        let food = counts[&ItemClass::Food] as f64 / 20_000.0;
        let amulet = counts[&ItemClass::Amulet] as f64 / 20_000.0;
        assert!((0.18..0.22).contains(&food), "food {food}");
        assert!((0.005..0.015).contains(&amulet), "amulet {amulet}");
        // A zero-probability kind (the Amulet of Yendor) never comes out.
        for _ in 0..5_000 {
            let id = mkobj_kind(Some(ItemClass::Amulet), ObjectTable::Dungeon, &mut rn2).unwrap();
            assert_ne!(id, ItemKindId::AMULET_OF_YENDOR);
        }
    }

    #[test]
    fn align_shift_matches_c_formula() {
        let rs = Ruleset::vanilla();
        let lawful = rs.monster_by_id(MonsterSpeciesId::ALIGNED_CLERIC).unwrap();
        // aligned cleric: maligntyp 0 -> lawful (0+20)/8 = 2, neutral 20/4 = 5, chaotic 20/8 = 2
        assert_eq!(align_shift(lawful, DungeonAlign::Lawful), 2);
        assert_eq!(align_shift(lawful, DungeonAlign::Neutral), 5);
        assert_eq!(align_shift(lawful, DungeonAlign::Chaotic), 2);
        assert_eq!(align_shift(lawful, DungeonAlign::None), 0);
    }
}
