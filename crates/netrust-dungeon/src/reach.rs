//! Reachability analysis and free-floor placement helpers shared by generators and the sim.

use netrust_types::{Coord, Tile};
use std::collections::{HashSet, VecDeque};

use crate::level::DungeonLevel;
use crate::room::Rect;

/// All coordinates reachable from `start` by 8-way moves over passable tiles.
pub fn reachable_from(level: &DungeonLevel, start: Coord) -> HashSet<Coord> {
    reachable_from_with(level, start, |t| t.is_passable())
}

/// Like [`reachable_from`] but with a caller-supplied passability rule.
pub fn reachable_from_with(
    level: &DungeonLevel,
    start: Coord,
    passable: impl Fn(&Tile) -> bool,
) -> HashSet<Coord> {
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    visited.insert(start);
    queue.push_back(start);
    while let Some(current) = queue.pop_front() {
        for n in current.neighbors() {
            if !visited.contains(&n) && passable(level.get_tile(n)) {
                visited.insert(n);
                queue.push_back(n);
            }
        }
    }
    visited
}

/// Interior floor tiles of `rect` in row-major order, excluding stairs and `avoid`.
pub fn find_free_floor(level: &DungeonLevel, rect: &Rect, avoid: &[Coord]) -> Vec<Coord> {
    let mut out = Vec::new();
    for y in (rect.y1 + 1)..rect.y2 {
        for x in (rect.x1 + 1)..rect.x2 {
            let c = Coord::new_unchecked(x, y);
            if *level.get_tile(c) == Tile::Room
                && c != level.stairs_up
                && c != level.stairs_down
                && !avoid.contains(&c)
            {
                out.push(c);
            }
        }
    }
    out
}
