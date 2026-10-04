//! Floor Engravings and Elbereth Wards.
//!
//! Modeled in Lean 4 (`NetMechanics.Engraving`).
//! Models medium durability, smudge degradation, and monster ward repulsion.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EngravingMedium {
    Burned,
    Carved(u32),
    Marked(u32),
    Dust(u32),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Engraving {
    pub text: String,
    pub medium: EngravingMedium,
}

impl Engraving {
    pub fn new(text: impl Into<String>, medium: EngravingMedium) -> Self {
        Self {
            text: text.into(),
            medium,
        }
    }

    /// Degrades engraving through trampling/smudging.
    pub fn smudge(&self) -> Option<Self> {
        match self.medium {
            EngravingMedium::Burned => Some(self.clone()),
            EngravingMedium::Carved(d) => {
                if d == 0 {
                    None
                } else {
                    Some(Self {
                        text: self.text.clone(),
                        medium: EngravingMedium::Carved(d - 1),
                    })
                }
            }
            EngravingMedium::Marked(d) => {
                if d == 0 {
                    None
                } else {
                    Some(Self {
                        text: self.text.clone(),
                        medium: EngravingMedium::Marked(d - 1),
                    })
                }
            }
            EngravingMedium::Dust(d) => {
                if d == 0 {
                    None
                } else {
                    Some(Self {
                        text: self.text.clone(),
                        medium: EngravingMedium::Dust(d - 1),
                    })
                }
            }
        }
    }
}

/// Checks whether an engraving wards off a monster according to NetHack rules.
pub fn is_elbereth_ward_active(
    engraving: Option<&Engraving>,
    monster_blind: bool,
    monster_covetous: bool,
) -> bool {
    match engraving {
        None => false,
        Some(e) => {
            if monster_blind || monster_covetous {
                false
            } else {
                e.text == "Elbereth"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_burned_engraving_is_permanent() {
        let e = Engraving::new("Elbereth", EngravingMedium::Burned);
        let smudged = e.smudge();
        assert_eq!(smudged, Some(e));
    }

    #[test]
    fn test_dust_engraving_depletes() {
        let e = Engraving::new("Elbereth", EngravingMedium::Dust(1));
        let step1 = e.smudge().expect("Should have 0 durability left");
        assert_eq!(step1.medium, EngravingMedium::Dust(0));
        let step2 = step1.smudge();
        assert_eq!(step2, None, "0-durability dust must be completely erased");
    }

    #[test]
    fn test_elbereth_ward_conditions() {
        let elbereth = Engraving::new("Elbereth", EngravingMedium::Burned);
        let random_txt = Engraving::new("Hello World", EngravingMedium::Burned);

        // Active vs normal monster
        assert!(is_elbereth_ward_active(Some(&elbereth), false, false));

        // Inactive if monster is blind
        assert!(!is_elbereth_ward_active(Some(&elbereth), true, false));

        // Inactive if monster is covetous (e.g. Rodney, demon lords)
        assert!(!is_elbereth_ward_active(Some(&elbereth), false, true));

        // Inactive if non-Elbereth
        assert!(!is_elbereth_ward_active(Some(&random_txt), false, false));

        // Inactive if none
        assert!(!is_elbereth_ward_active(None, false, false));
    }
}
