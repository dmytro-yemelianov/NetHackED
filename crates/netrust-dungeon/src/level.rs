//! Complete 80x21 Dungeon Level data structure and tile accessors.

use std::collections::HashMap;
use netrust_core::engraving::Engraving;
use netrust_types::{Coord, Tile, COLNO, ROWNO};
use serde::{Deserialize, Serialize};

use crate::room::Room;

/// Complete $80 \times 21$ Dungeon Level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DungeonLevel {
    pub tiles: Vec<Vec<Tile>>,
    pub rooms: Vec<Room>,
    pub stairs_up: Coord,
    pub stairs_down: Coord,
    pub engravings: HashMap<Coord, Engraving>,
    #[serde(default)]
    pub is_dark: bool,
}

impl Default for DungeonLevel {
    fn default() -> Self {
        Self::new_solid(Tile::Stone)
    }
}

impl DungeonLevel {
    pub fn new_solid(default_tile: Tile) -> Self {
        let mut tiles = Vec::with_capacity(COLNO);
        for _ in 0..COLNO {
            let mut col = Vec::with_capacity(ROWNO);
            for _ in 0..ROWNO {
                col.push(default_tile.clone());
            }
            tiles.push(col);
        }
        Self {
            tiles,
            rooms: Vec::new(),
            stairs_up: Coord::new_unchecked(1, 1),
            stairs_down: Coord::new_unchecked(1, 1),
            engravings: HashMap::new(),
            is_dark: false,
        }
    }

    pub fn get_engraving(&self, coord: Coord) -> Option<&Engraving> {
        self.engravings.get(&coord)
    }

    pub fn set_engraving(&mut self, coord: Coord, engraving: Engraving) {
        self.engravings.insert(coord, engraving);
    }

    pub fn remove_engraving(&mut self, coord: Coord) -> Option<Engraving> {
        self.engravings.remove(&coord)
    }

    /// Smudges an engraving (e.g. from walking over dust). Returns Some(None) if completely erased,
    /// Some(Some(degraded)) if degraded, or None if no engraving existed.
    pub fn smudge_engraving(&mut self, coord: Coord) -> Option<Option<Engraving>> {
        if let Some(e) = self.engravings.get(&coord) {
            let smudged = e.smudge();
            if let Some(new_e) = smudged {
                self.engravings.insert(coord, new_e.clone());
                Some(Some(new_e))
            } else {
                self.engravings.remove(&coord);
                Some(None)
            }
        } else {
            None
        }
    }

    #[inline]
    pub fn get_tile(&self, c: Coord) -> &Tile {
        &self.tiles[c.x][c.y]
    }

    #[inline]
    pub fn get_tile_mut(&mut self, c: Coord) -> &mut Tile {
        &mut self.tiles[c.x][c.y]
    }

    #[inline]
    pub fn set_tile(&mut self, c: Coord, tile: Tile) {
        self.tiles[c.x][c.y] = tile;
    }

    pub fn is_passable(&self, c: Coord) -> bool {
        self.get_tile(c).is_passable()
    }

    pub fn is_transparent(&self, c: Coord) -> bool {
        self.get_tile(c).is_transparent()
    }

    /// Returns the room containing the coordinate within its inner floor bounds, if any.
    pub fn room_at(&self, c: Coord) -> Option<&Room> {
        self.rooms.iter().find(|r| r.contains_inner(c))
    }

    /// Returns true if the coordinate is in darkness (either whole level is dark, or within a dark room).
    pub fn is_dark_at(&self, c: Coord) -> bool {
        if self.is_dark {
            return true;
        }
        for r in &self.rooms {
            if c.x >= r.x1 && c.x <= r.x2 && c.y >= r.y1 && c.y <= r.y2 {
                return r.is_dark;
            }
        }
        false
    }
}
