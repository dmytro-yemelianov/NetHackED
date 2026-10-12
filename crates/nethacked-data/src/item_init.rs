//! Partial random initialization: represented BUC, spe, weapon/gem quantity, poison and container lock/trap state.
//! No species/fuel/erosion initialization or discarded draws. Contents live in containers;
//! existing-object artifacts use the separately enabled room-generation hook.

use nethacked_arena::{ItemLocation, ItemRecord};
use nethacked_core::item_init::{blessorcurse_result, ring_zero_spe, rne_cap};
pub use nethacked_core::item_init::{ContainerInit, ItemInitProfile, ToolCharges, WandCharges};
use nethacked_types::Buc;

use crate::{ItemKindId, Ruleset, WandDir};

fn draw(rn2: &mut impl FnMut(u32) -> u32, n: u32) -> u32 {
    rn2(n).min(n - 1)
}

/// C `NetHack-5.0.0/src/rnd.c:192`: no draw after reaching the cap.
fn rne(hero_level: u32, rn2: &mut impl FnMut(u32) -> u32) -> i8 {
    let mut value = 1;
    while value < rne_cap(hero_level) && draw(rn2, 3) == 0 {
        value += 1;
    }
    value as i8
}

/// C `NetHack-5.0.0/src/mkobj.c:1841`: second draw only after the zero branch.
fn bc(item: &mut ItemRecord, chance: u32, rn2: &mut impl FnMut(u32) -> u32) {
    if item.buc != Buc::Uncursed {
        return;
    }
    let roll = draw(rn2, chance);
    if roll == 0 {
        item.buc = blessorcurse_result(item.buc, chance, roll, draw(rn2, 2));
    }
}

/// C `NetHack-5.0.0/src/mkobj.c:1858`: bcsign.
fn sign(buc: Buc) -> i8 {
    match buc {
        Buc::Blessed => 1,
        Buc::Cursed => -1,
        Buc::Uncursed => 0,
    }
}

impl Ruleset {
    /// Initialize only supported fields, using owned metadata and world RNG.
    /// Explicit constructors remain RNG-free; missing profiles stay fixed.
    /// C `NetHack-5.0.0/src/mkobj.c:869-1175` (partial switch, no tail).
    pub fn create_random_item_supported_fields(
        &self,
        id: ItemKindId,
        location: ItemLocation,
        hero_level: u32,
        mut rn2: impl FnMut(u32) -> u32,
    ) -> Option<ItemRecord> {
        let def = self.item_by_id(id)?;
        self.create_random_item_supported_fields_by_name(&def.name, location, hero_level, &mut rn2)
    }

    /// Name-based entry point for configured custom definitions and pack overrides.
    /// C `NetHack-5.0.0/src/mkobj.c:1180`: fresh object before mksobj_init.
    pub fn create_random_item_supported_fields_by_name(
        &self,
        name: &str,
        location: ItemLocation,
        hero_level: u32,
        mut rn2: impl FnMut(u32) -> u32,
    ) -> Option<ItemRecord> {
        let def = self.item(name)?;
        let item = self.create_item_record(&def.name, location, Buc::Uncursed)?;
        Some(initialize_supported_fields(
            item,
            def.initialization,
            def.wand_dir,
            hero_level,
            &mut rn2,
        ))
    }
}

/// Also supports explicitly configured custom definitions without built-in IDs.
/// C `NetHack-5.0.0/src/mkobj.c:869`: supported cases only, on a fresh record.
pub fn initialize_supported_fields(
    mut item: ItemRecord,
    profile: ItemInitProfile,
    wand_dir: Option<WandDir>,
    hero_level: u32,
    mut rn2: impl FnMut(u32) -> u32,
) -> ItemRecord {
    if profile.is_fixed() || !profile.matches_class(item.class) {
        return item;
    }
    item.enchantment = 0;
    match profile {
        ItemInitProfile::Weapon {
            multigen,
            poisonable,
        } => {
            item.quantity = if multigen { draw(&mut rn2, 6) + 6 } else { 1 };
            item.weight = item.weight.saturating_mul(item.quantity);
            if draw(&mut rn2, 11) == 0 {
                item.enchantment = rne(hero_level, &mut rn2);
                item.buc = if draw(&mut rn2, 2) == 0 {
                    Buc::Uncursed
                } else {
                    Buc::Blessed
                };
            } else if draw(&mut rn2, 10) == 0 {
                item.buc = Buc::Cursed;
                item.enchantment = -rne(hero_level, &mut rn2);
            } else {
                bc(&mut item, 10, &mut rn2);
            }
            if poisonable {
                item.poisoned = draw(&mut rn2, 100) == 0;
            }
        }
        ItemInitProfile::Armor { harmful } => {
            if draw(&mut rn2, 10) != 0 && (harmful || draw(&mut rn2, 11) == 0) {
                item.buc = Buc::Cursed;
                item.enchantment = -rne(hero_level, &mut rn2);
            } else if draw(&mut rn2, 10) == 0 {
                item.buc = if draw(&mut rn2, 2) == 0 {
                    Buc::Uncursed
                } else {
                    Buc::Blessed
                };
                item.enchantment = rne(hero_level, &mut rn2);
            } else {
                bc(&mut item, 10, &mut rn2);
            }
        }
        ItemInitProfile::Wand { charges } => {
            item.enchantment = match charges {
                WandCharges::Wishing => 1,
                WandCharges::Stasis => (draw(&mut rn2, 4) + 3) as i8,
                WandCharges::Standard => {
                    (draw(&mut rn2, 5)
                        + if wand_dir == Some(WandDir::NoDir) {
                            11
                        } else {
                            4
                        }) as i8
                }
            };
            bc(&mut item, 17, &mut rn2);
            item.recharged = 0;
        }
        ItemInitProfile::Ring { charged, harmful } => {
            if charged {
                bc(&mut item, 3, &mut rn2);
                if draw(&mut rn2, 10) != 0 {
                    let use_buc = draw(&mut rn2, 10) != 0 && sign(item.buc) != 0;
                    let s = if use_buc {
                        sign(item.buc)
                    } else if draw(&mut rn2, 2) != 0 {
                        1
                    } else {
                        -1
                    };
                    item.enchantment = s * rne(hero_level, &mut rn2);
                }
                if item.enchantment == 0 {
                    item.enchantment = ring_zero_spe(draw(&mut rn2, 4), draw(&mut rn2, 3));
                }
                if item.enchantment < 0 && draw(&mut rn2, 5) != 0 {
                    item.buc = Buc::Cursed;
                }
            } else if draw(&mut rn2, 10) != 0 && (harmful || draw(&mut rn2, 9) == 0) {
                item.buc = Buc::Cursed;
            }
        }
        ItemInitProfile::Amulet { harmful } => {
            if draw(&mut rn2, 10) != 0 && harmful {
                item.buc = Buc::Cursed;
            } else {
                bc(&mut item, 10, &mut rn2);
            }
        }
        ItemInitProfile::Potion | ItemInitProfile::Scroll => bc(&mut item, 4, &mut rn2),
        ItemInitProfile::Book => bc(&mut item, 17, &mut rn2),
        ItemInitProfile::Tool { charges } => {
            let (spe, chance) = match charges {
                ToolCharges::Camera => (draw(&mut rn2, 70) + 30, None),
                ToolCharges::Grease => (draw(&mut rn2, 21) + 5, Some(10)),
                ToolCharges::CrystalBall => (draw(&mut rn2, 5) + 3, Some(2)),
                ToolCharges::HornOrBag => (draw(&mut rn2, 18) + 3, None),
                ToolCharges::Instrument => (draw(&mut rn2, 5) + 4, None),
                ToolCharges::Bell => (3, None),
                ToolCharges::MagicLamp => (1, Some(2)),
            };
            item.enchantment = spe as i8;
            if let Some(chance) = chance {
                bc(&mut item, chance, &mut rn2);
            }
        }
        ItemInitProfile::Container { container } => {
            if matches!(container, ContainerInit::Chest | ContainerInit::LargeBox) {
                item.locked = draw(&mut rn2, 5) != 0;
                item.trapped = draw(&mut rn2, 10) == 0;
                item.trap_known = item.trapped && draw(&mut rn2, 100) == 0;
            }
        }
        ItemInitProfile::Gem { rock, luckstone } => {
            item.quantity = if rock {
                draw(&mut rn2, 6) + 6
            } else if !luckstone && draw(&mut rn2, 6) == 0 {
                2
            } else {
                1
            };
            item.weight = item.weight.saturating_mul(item.quantity);
        }
        ItemInitProfile::Loadstone => item.buc = Buc::Cursed,
        ItemInitProfile::Fixed => {}
    }
    item
}
