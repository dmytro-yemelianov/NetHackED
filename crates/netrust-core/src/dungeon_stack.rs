//! Multi-floor dungeon depth progression and invariants.
//!
//! Modeled in Lean 4 (`NetMechanics.DungeonStack`).

use serde::{Deserialize, Serialize};

/// Strongly typed, non-zero dungeon depth (Dungeons of Doom depth 1..N).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DungeonDepth(usize);

impl DungeonDepth {
    /// Surface entrance to the Dungeons of Doom (depth 1).
    pub const SURFACE: Self = Self(1);

    pub fn new(depth: usize) -> Option<Self> {
        if depth >= 1 {
            Some(Self(depth))
        } else {
            None
        }
    }

    pub fn as_usize(&self) -> usize {
        self.0
    }

    /// Descending down stairs strictly increases depth by 1.
    pub fn descend(&self) -> Self {
        Self(self.0 + 1)
    }

    /// Ascending stairs decreases depth if depth > 1, or stays at 1 (celestial barrier).
    pub fn ascend(&self) -> Self {
        if self.0 > 1 {
            Self(self.0 - 1)
        } else {
            *self
        }
    }
}

impl Default for DungeonDepth {
    fn default() -> Self {
        Self::SURFACE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_depth_transitions_and_inverses() {
        let d1 = DungeonDepth::SURFACE;
        let d2 = d1.descend();
        assert_eq!(d2.as_usize(), 2);
        assert_eq!(d2.ascend(), d1);
        assert_eq!(d1.ascend(), d1); // Surface invariant
    }
}
