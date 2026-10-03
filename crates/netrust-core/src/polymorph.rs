//! Polymorph HP Buffer and Revert-on-Death Invariant.
//!
//! Formally verified in `NetMechanics.Polymorph`.
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

    /// Resolves damage, absorbing with poly form first and reverting to base upon fatal damage.
    /// Returns `(updated_entity, is_dead)`.
    pub fn apply_damage(&self, damage: u32) -> (Self, bool) {
        match &self.poly_form {
            None => {
                let new_base_hp = self.base_form.hp.saturating_sub(damage);
                let is_dead = new_base_hp == 0;
                (
                    Self {
                        base_form: FormStats {
                            hp: new_base_hp,
                            max_hp: self.base_form.max_hp,
                            name: self.base_form.name.clone(),
                        },
                        poly_form: None,
                    },
                    is_dead,
                )
            }
            Some(poly) => {
                if damage < poly.hp {
                    let new_poly = FormStats {
                        hp: poly.hp - damage,
                        max_hp: poly.max_hp,
                        name: poly.name.clone(),
                    };
                    (
                        Self {
                            base_form: self.base_form.clone(),
                            poly_form: Some(new_poly),
                        },
                        false,
                    )
                } else {
                    let excess = damage - poly.hp;
                    let new_base_hp = self.base_form.hp.saturating_sub(excess);
                    let is_dead = new_base_hp == 0;
                    (
                        Self {
                            base_form: FormStats {
                                hp: new_base_hp,
                                max_hp: self.base_form.max_hp,
                                name: self.base_form.name.clone(),
                            },
                            poly_form: None,
                        },
                        is_dead,
                    )
                }
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

        // Take 15 damage: 10 absorbed by Bat, 5 penetrates to Hero
        let (reverted, is_dead) = entity.apply_damage(15);
        assert!(!reverted.is_polymorphed());
        assert_eq!(reverted.base_form.hp, 15);
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

        let (reverted, is_dead) = entity.apply_damage(10);
        assert!(!reverted.is_polymorphed());
        assert_eq!(reverted.base_form.hp, 20);
        assert!(!is_dead);
    }
}
