//! Epistemic Item Identification Lattice and Monotonicity.
//!
//! Modeled in Lean 4 (`NetMechanics.Identification`).
//! Models the 4-tier knowledge progression and proves knowledge monotonicity.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum KnowledgeLevel {
    Unidentified = 0,
    TypeIdentified = 1,
    BucKnown = 2,
    FullyIdentified = 3,
}

pub fn learn_type(k: KnowledgeLevel) -> KnowledgeLevel {
    match k {
        KnowledgeLevel::Unidentified => KnowledgeLevel::TypeIdentified,
        _ => k,
    }
}

pub fn learn_buc(k: KnowledgeLevel) -> KnowledgeLevel {
    match k {
        KnowledgeLevel::Unidentified | KnowledgeLevel::TypeIdentified => KnowledgeLevel::BucKnown,
        _ => k,
    }
}

pub fn identify_fully(_k: KnowledgeLevel) -> KnowledgeLevel {
    KnowledgeLevel::FullyIdentified
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identification_monotonicity() {
        for k in [
            KnowledgeLevel::Unidentified,
            KnowledgeLevel::TypeIdentified,
            KnowledgeLevel::BucKnown,
            KnowledgeLevel::FullyIdentified,
        ] {
            assert!(k <= learn_type(k));
            assert!(k <= learn_buc(k));
            assert!(k <= identify_fully(k));
        }
    }

    #[test]
    fn test_identify_fully_idempotent() {
        for k in [
            KnowledgeLevel::Unidentified,
            KnowledgeLevel::TypeIdentified,
            KnowledgeLevel::BucKnown,
            KnowledgeLevel::FullyIdentified,
        ] {
            assert_eq!(identify_fully(identify_fully(k)), identify_fully(k));
        }
    }
}
