//! Spatial room geometry and classification.

use nethacked_types::{Alignment, Coord, COLNO, ROWNO};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x1: usize,
    pub y1: usize,
    pub x2: usize,
    pub y2: usize,
}

impl Rect {
    pub fn new(x: usize, y: usize, w: usize, h: usize) -> Self {
        Self {
            x1: x,
            y1: y,
            x2: (x + w).min(COLNO - 1),
            y2: (y + h).min(ROWNO - 1),
        }
    }

    pub fn center(&self) -> Coord {
        Coord::new_unchecked((self.x1 + self.x2) / 2, (self.y1 + self.y2) / 2)
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        self.x1 <= other.x2 && self.x2 >= other.x1 && self.y1 <= other.y2 && self.y2 >= other.y1
    }

    /// Checks if a coordinate is strictly inside the room's interior (excluding surrounding walls).
    pub fn contains_inner(&self, c: Coord) -> bool {
        c.x > self.x1 && c.x < self.x2 && c.y > self.y1 && c.y < self.y2
    }
}

/// Canonical NetHack room classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RoomType {
    Normal,
    Shop,
    Temple { alignment: Alignment },
}

/// A classified dungeon room with spatial bounds and semantic type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Room {
    pub rect: Rect,
    pub room_type: RoomType,
    pub is_dark: bool,
}

impl std::ops::Deref for Room {
    type Target = Rect;
    fn deref(&self) -> &Self::Target {
        &self.rect
    }
}

impl Room {
    pub fn new(rect: Rect, room_type: RoomType) -> Self {
        Self {
            rect,
            room_type,
            is_dark: false,
        }
    }

    pub fn with_dark(mut self, is_dark: bool) -> Self {
        self.is_dark = is_dark;
        self
    }
}
