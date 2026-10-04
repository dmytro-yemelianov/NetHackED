//! Polymorph HP Buffer and Revert-on-Death Invariant.
//!
//! Modeled in Lean 4 (`NetMechanics.Polymorph`).
//! Models shape-shifting HP buffer and proves form reversion upon fatal damage.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormStats {
    pub hp: u32,
    pub max_hp: u32,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolyEntity {
    pub base_form: FormStats,
    pub poly_form: Option<FormStats>,
}

impl PolyEntity {
    pub fn new_base(name: impl Into<String>, hp: u32, max_hp: u32) -> Self {
        Self {
            base_form: FormStats {
                hp,
                max_hp,
                name: name.into(),
            },
            poly_form: None,
        }
    }

    pub fn is_polymorphed(&self) -> bool {
        self.poly_form.is_some()
    }

    /// Resolves damage per C `hack.c:4256` `losehp` / `polyself.c:1367`
    /// `rehumanize`: the polyform absorbs damage; when its HP would drop below
    /// 1 the entity reverts and the excess is discarded (base HP untouched),
    /// unless `unchanging`, in which case it dies.
    /// Returns `(updated_entity, is_dead)`.
    pub fn apply_damage(&self, damage: u32, unchanging: bool) -> (Self, bool) {
        match &self.poly_form {
            None => {
                let new_base_hp = self.base_form.hp.saturating_sub(damage);
                let mut e = self.clone();
                e.base_form.hp = new_base_hp;
                (e, new_base_hp == 0)
            }
            Some(poly) if damage < poly.hp => {
                let mut e = self.clone();
                if let Some(p) = &mut e.poly_form {
                    p.hp = poly.hp - damage;
                }
                (e, false)
            }
            Some(_) if unchanging => {
                let mut e = self.clone();
                if let Some(p) = &mut e.poly_form {
                    p.hp = 0;
                }
                (e, true)
            }
            Some(_) => {
                let mut e = self.clone();
                e.poly_form = None;
                let dead = e.base_form.hp == 0;
                (e, dead)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fatal_poly_damage_reverts_to_base() {
        let base = FormStats {
            hp: 20,
            max_hp: 20,
            name: "Hero".into(),
        };
        let poly = FormStats {
            hp: 10,
            max_hp: 10,
            name: "Bat".into(),
        };
        let entity = PolyEntity {
            base_form: base,
            poly_form: Some(poly),
        };

        // Take 15 damage: Bat dies, the 5 excess is discarded (C rehumanize)
        let (reverted, is_dead) = entity.apply_damage(15, false);
        assert!(!reverted.is_polymorphed());
        assert_eq!(reverted.base_form.hp, 20);
        assert!(!is_dead);
    }

    #[test]
    fn test_exact_poly_damage_leaves_base_untouched() {
        let base = FormStats {
            hp: 20,
            max_hp: 20,
            name: "Hero".into(),
        };
        let poly = FormStats {
            hp: 10,
            max_hp: 10,
            name: "Bat".into(),
        };
        let entity = PolyEntity {
            base_form: base,
            poly_form: Some(poly),
        };

        let (reverted, is_dead) = entity.apply_damage(10, false);
        assert!(!reverted.is_polymorphed());
        assert_eq!(reverted.base_form.hp, 20);
        assert!(!is_dead);
    }
}

use netrust_types::{EquipSlot, Hero, MonsterId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolyDamageResult {
    /// Polymorphed and the polyform survived (`u.mh >= 1`).
    Absorbed,
    /// Not polymorphed; base HP reduced and the hero survived (`u.uhp >= 1`).
    BaseDamaged,
    /// Polyform HP dropped below 1: the hero rehumanized. Excess damage is
    /// discarded and base HP is untouched.
    Reverted,
    /// The hero died (base HP < 1, or polyform HP < 1 while `Unchanging`).
    Dead,
}

/// Apply `damage` to the hero (negative damage is treated as 0).
///
/// C `hack.c:4256` `losehp`: when `Upolyd`, `u.mh -= n` and `u.mh < 1` calls
/// `rehumanize()` with no carry-over to `u.uhp`; otherwise `u.uhp -= n` and
/// `u.uhp < 1` is death. C `polyself.c:1367` `rehumanize`: with `Unchanging`
/// the hero dies instead of reverting; else `polyman()` restores the normal
/// form with `u.uhp` unchanged (death only if `u.uhp < 1` already).
///
/// `unchanging`: whether the hero has the Unchanging property. The Hero type
/// does not track it yet, so the sim passes `false`.
pub fn apply_poly_damage(hero: &mut Hero, damage: i32, unchanging: bool) -> PolyDamageResult {
    let damage = damage.max(0);
    if let Some(poly) = &mut hero.polymorph {
        poly.hp = poly.hp.saturating_sub(damage);
        if poly.hp >= 1 {
            return PolyDamageResult::Absorbed;
        }
        if unchanging {
            return PolyDamageResult::Dead;
        }
        hero.polymorph = None;
        if hero.base_hp < 1 {
            PolyDamageResult::Dead
        } else {
            PolyDamageResult::Reverted
        }
    } else {
        hero.base_hp = hero.base_hp.saturating_sub(damage);
        if hero.base_hp < 1 {
            PolyDamageResult::Dead
        } else {
            PolyDamageResult::BaseDamaged
        }
    }
}

pub fn can_wear_in_form(form: MonsterId, slot: EquipSlot) -> bool {
    // Humanoid forms can wear armor, animal forms unequip armor.
    // Assuming form is a usize. Let's just say form % 2 == 0 is humanoid for now,
    // or if MonsterSpeciesId is involved we could map it. But here we have MonsterId as usize.
    match slot {
        EquipSlot::Helmet
        | EquipSlot::Suit
        | EquipSlot::Shirt
        | EquipSlot::Cloak
        | EquipSlot::Gloves
        | EquipSlot::Boots
        | EquipSlot::Shield
        | EquipSlot::Weapon => {
            // Simplified check: if form is even, assume humanoid. In a real impl, look up monster archetype.
            form % 2 == 0
        }
        _ => true,
    }
}

pub fn cure_lycanthropy(hero: &mut Hero) -> bool {
    if hero.lycanthropy.is_some() {
        hero.lycanthropy = None;
        true
    } else {
        false
    }
}

#[cfg(test)]
mod hero_tests {
    use super::*;
    use netrust_types::PolymorphForm;

    fn poly_hero(base: i32, poly: i32) -> Hero {
        Hero {
            base_hp: base,
            base_max_hp: base,
            polymorph: Some(PolymorphForm {
                monster_id: 1,
                hp: poly,
                max_hp: poly,
                duration: 100,
            }),
            mount: None,
            quivered_item: None,
            lycanthropy: None,
            afflictions: Default::default(),
            skills: Default::default(),
        }
    }

    #[test]
    fn overkill_reverts_without_touching_base_hp() {
        let mut h = poly_hero(20, 10);
        assert_eq!(
            apply_poly_damage(&mut h, 15, false),
            PolyDamageResult::Reverted
        );
        assert!(h.polymorph.is_none());
        assert_eq!(h.base_hp, 20);
    }

    #[test]
    fn huge_overkill_never_kills_without_unchanging() {
        let mut h = poly_hero(1, 5);
        assert_eq!(
            apply_poly_damage(&mut h, 9999, false),
            PolyDamageResult::Reverted
        );
        assert_eq!(h.base_hp, 1);
    }

    #[test]
    fn unchanging_dies_instead_of_reverting() {
        let mut h = poly_hero(20, 10);
        assert_eq!(apply_poly_damage(&mut h, 10, true), PolyDamageResult::Dead);
    }

    #[test]
    fn unpolymorphed_survival_is_base_damaged_and_negative_is_zero() {
        let mut h = poly_hero(20, 10);
        h.polymorph = None;
        assert_eq!(
            apply_poly_damage(&mut h, 5, false),
            PolyDamageResult::BaseDamaged
        );
        assert_eq!(h.base_hp, 15);
        assert_eq!(
            apply_poly_damage(&mut h, -7, false),
            PolyDamageResult::BaseDamaged
        );
        assert_eq!(h.base_hp, 15);
        assert_eq!(apply_poly_damage(&mut h, 15, false), PolyDamageResult::Dead);
    }
}
