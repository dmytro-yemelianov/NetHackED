//! Discrete Dijkstra Scent Gradient and Monster Pathfinding Convergence.
//!
//! Modeled in Lean 4 (`NetMechanics.Pathfinding`).
//! Models steepest descent gradient stepping and proves target convergence.

use nethacked_types::{Coord, COLNO, ROWNO};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct MetricState {
    pub dist_to_target: u32,
}

impl MetricState {
    pub const fn new(dist: u32) -> Self {
        Self {
            dist_to_target: dist,
        }
    }

    /// Takes a steepest descent step towards the target.
    pub fn descent_step(self, reduced_neighbor: Option<MetricState>) -> MetricState {
        if self.dist_to_target == 0 {
            return self;
        }

        match reduced_neighbor {
            Some(n) => {
                if n.dist_to_target < self.dist_to_target {
                    n
                } else {
                    self
                }
            }
            None => self,
        }
    }
}

/// 2D discrete distance field computed via Breadth-First / Dijkstra expansion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DijkstraField {
    pub distances: Vec<u32>,
    pub target: Coord,
}

impl DijkstraField {
    pub const UNREACHABLE: u32 = u32::MAX;

    #[inline]
    fn idx(c: Coord) -> usize {
        c.x * ROWNO + c.y
    }

    /// Reuses an existing DijkstraField allocation, clearing and computing distances from `target`.
    pub fn compute_into<F>(&mut self, target: Coord, mut is_passable: F)
    where
        F: FnMut(Coord) -> bool,
    {
        self.target = target;
        let total = COLNO * ROWNO;
        if self.distances.len() != total {
            self.distances.resize(total, Self::UNREACHABLE);
        }
        self.distances.fill(Self::UNREACHABLE);

        let mut queue = VecDeque::with_capacity(128);
        self.distances[Self::idx(target)] = 0;
        queue.push_back(target);

        while let Some(current) = queue.pop_front() {
            let cur_dist = self.distances[Self::idx(current)];
            for neighbor in current.neighbors() {
                let n_idx = Self::idx(neighbor);
                if is_passable(neighbor) && self.distances[n_idx] == Self::UNREACHABLE {
                    self.distances[n_idx] = cur_dist + 1;
                    queue.push_back(neighbor);
                }
            }
        }
    }

    /// Build a discrete distance field from target over passable grid tiles.
    pub fn compute<F>(target: Coord, is_passable: F) -> Self
    where
        F: FnMut(Coord) -> bool,
    {
        let mut field = Self {
            distances: vec![Self::UNREACHABLE; COLNO * ROWNO],
            target,
        };
        field.compute_into(target, is_passable);
        field
    }

    /// Returns the distance from `c` to the target.
    pub fn distance(&self, c: Coord) -> u32 {
        self.distances[Self::idx(c)]
    }

    /// Finds the adjacent neighbor with the minimum distance (steepest descent).
    pub fn steepest_descent(&self, from: Coord) -> Option<Coord> {
        let cur_dist = self.distance(from);
        if cur_dist == 0 || cur_dist == Self::UNREACHABLE {
            return None;
        }

        let mut best_coord = None;
        let mut best_dist = cur_dist;

        for neighbor in from.neighbors() {
            let nd = self.distance(neighbor);
            if nd < best_dist {
                best_dist = nd;
                best_coord = Some(neighbor);
            }
        }

        best_coord
    }

    /// Finds the adjacent neighbor with the maximum finite distance (fleeing).
    pub fn steepest_ascent(&self, from: Coord) -> Option<Coord> {
        let cur_dist = self.distance(from);
        if cur_dist == Self::UNREACHABLE {
            return None;
        }

        let mut best_coord = None;
        let mut best_dist = cur_dist;

        for neighbor in from.neighbors() {
            let nd = self.distance(neighbor);
            if nd != Self::UNREACHABLE && nd > best_dist {
                best_dist = nd;
                best_coord = Some(neighbor);
            }
        }

        best_coord
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_target_is_fixed_point() {
        let target = MetricState::new(0);
        let neighbor = Some(MetricState::new(1));
        assert_eq!(target.descent_step(neighbor), target);
    }

    #[test]
    fn test_descent_decreases_distance() {
        let current = MetricState::new(5);
        let neighbor = Some(MetricState::new(4));
        let next = current.descent_step(neighbor);
        assert_eq!(next.dist_to_target, 4);
        assert!(next.dist_to_target < current.dist_to_target);
    }

    #[test]
    fn test_dijkstra_field_convergence() {
        let target = Coord::new_unchecked(10, 10);
        let field = DijkstraField::compute(target, |_c| true);

        assert_eq!(field.distance(target), 0);
        assert_eq!(field.distance(Coord::new_unchecked(10, 12)), 2);

        let mut current = Coord::new_unchecked(10, 14);
        let mut steps = 0;
        while current != target && steps < 20 {
            if let Some(next) = field.steepest_descent(current) {
                assert!(field.distance(next) < field.distance(current));
                current = next;
                steps += 1;
            } else {
                break;
            }
        }
        assert_eq!(current, target);
        assert_eq!(steps, 4);
    }
}
