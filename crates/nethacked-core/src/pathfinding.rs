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

    /// Like [`DijkstraField::compute_into`], but a cell `n` is reached from
    /// `current` only when `can_step(n, current)`: the field is a distance
    /// *to* the target, so the edge is walked from `n` towards `current`.
    ///
    /// Models C `mfndpos` (mon.c:2250-2257), which forbids some single steps
    /// (diagonals into or out of a door) between two passable squares.
    /// Neighbour order is that of [`DijkstraField::compute_into`]; with
    /// `can_step` always true the result is identical.
    pub fn compute_into_with_edges<F, E>(
        &mut self,
        target: Coord,
        mut is_passable: F,
        mut can_step: E,
    ) where
        F: FnMut(Coord) -> bool,
        E: FnMut(Coord, Coord) -> bool,
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
                if is_passable(neighbor)
                    && self.distances[n_idx] == Self::UNREACHABLE
                    && can_step(neighbor, current)
                {
                    self.distances[n_idx] = cur_dist + 1;
                    queue.push_back(neighbor);
                }
            }
        }
    }

    /// Edge-aware counterpart of [`DijkstraField::compute`]
    /// (see [`DijkstraField::compute_into_with_edges`], mon.c:2250-2257).
    pub fn compute_with_edges<F, E>(target: Coord, is_passable: F, can_step: E) -> Self
    where
        F: FnMut(Coord) -> bool,
        E: FnMut(Coord, Coord) -> bool,
    {
        let mut field = Self {
            distances: vec![Self::UNREACHABLE; COLNO * ROWNO],
            target,
        };
        field.compute_into_with_edges(target, is_passable, can_step);
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

    /// [`DijkstraField::steepest_descent`] restricted to neighbours reachable
    /// by a legal step `can_step(from, neighbor)` (mon.c:2250-2257). Same
    /// neighbour order and tie-breaking as the plain version.
    pub fn steepest_descent_by<E>(&self, from: Coord, mut can_step: E) -> Option<Coord>
    where
        E: FnMut(Coord, Coord) -> bool,
    {
        let cur_dist = self.distance(from);
        if cur_dist == 0 || cur_dist == Self::UNREACHABLE {
            return None;
        }

        let mut best_coord = None;
        let mut best_dist = cur_dist;

        for neighbor in from.neighbors() {
            let nd = self.distance(neighbor);
            if nd < best_dist && can_step(from, neighbor) {
                best_dist = nd;
                best_coord = Some(neighbor);
            }
        }

        best_coord
    }

    /// [`DijkstraField::steepest_ascent`] restricted to legal steps
    /// `can_step(from, neighbor)` (mon.c:2250-2257).
    pub fn steepest_ascent_by<E>(&self, from: Coord, mut can_step: E) -> Option<Coord>
    where
        E: FnMut(Coord, Coord) -> bool,
    {
        let cur_dist = self.distance(from);
        if cur_dist == Self::UNREACHABLE {
            return None;
        }

        let mut best_coord = None;
        let mut best_dist = cur_dist;

        for neighbor in from.neighbors() {
            let nd = self.distance(neighbor);
            if nd != Self::UNREACHABLE && nd > best_dist && can_step(from, neighbor) {
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

    /// A diagonal edge that `can_step` forbids is not used by the field:
    /// the cell diagonal to the target costs 2 instead of 1.
    #[test]
    fn edge_aware_field_routes_around_door_diagonal() {
        let target = Coord::new_unchecked(10, 10);
        let diag = Coord::new_unchecked(11, 11);
        let forbidden =
            |a: Coord, b: Coord| (a == diag && b == target) || (a == target && b == diag);
        let plain = DijkstraField::compute(target, |_| true);
        assert_eq!(plain.distance(diag), 1);
        let field = DijkstraField::compute_with_edges(target, |_| true, |a, b| !forbidden(a, b));
        assert_eq!(field.distance(diag), 2);
        assert_eq!(field.distance(target), 0);
        // the other diagonals are untouched
        assert_eq!(field.distance(Coord::new_unchecked(9, 9)), 1);
        // descent from the diagonal cell never uses the forbidden edge
        let next = field
            .steepest_descent_by(diag, |a, b| !forbidden(a, b))
            .expect("a route exists");
        assert_ne!(next, target);
        assert_eq!(field.distance(next), 1);
    }

    /// On a 5x5 grid with every diagonal edge touching the centre forbidden,
    /// descent from any cell reaches the target in exactly `distance` steps
    /// and never takes a blocked edge.
    #[test]
    fn steepest_descent_by_never_picks_blocked_edge() {
        let target = Coord::new_unchecked(12, 6);
        let in_grid = |c: Coord| (10..=14).contains(&c.x) && (4..=8).contains(&c.y);
        let blocked = |a: Coord, b: Coord| {
            let diagonal = a.x != b.x && a.y != b.y;
            diagonal && (a == target || b == target)
        };
        let can_step = |a: Coord, b: Coord| !blocked(a, b);
        let field = DijkstraField::compute_with_edges(target, in_grid, can_step);
        for x in 10..=14 {
            for y in 4..=8 {
                let start = Coord::new_unchecked(x, y);
                let dist = field.distance(start);
                assert_ne!(dist, DijkstraField::UNREACHABLE);
                let mut cur = start;
                let mut steps = 0;
                while cur != target {
                    let next = field
                        .steepest_descent_by(cur, can_step)
                        .expect("descent continues until the target");
                    assert!(can_step(cur, next), "blocked edge {cur:?}->{next:?}");
                    assert!(in_grid(next));
                    cur = next;
                    steps += 1;
                    assert!(steps <= dist, "too many steps from {start:?}");
                }
                assert_eq!(steps, dist, "from {start:?}");
            }
        }
        // ascent never uses a blocked edge either
        for x in 10..=14 {
            for y in 4..=8 {
                let c = Coord::new_unchecked(x, y);
                if let Some(next) = field.steepest_ascent_by(c, can_step) {
                    assert!(can_step(c, next));
                    assert!(field.distance(next) > field.distance(c));
                }
            }
        }
    }

    /// With every edge allowed the edge-aware API is the plain API: same
    /// distances, same descent and ascent choices (protects determinism).
    #[test]
    fn edge_aware_field_equals_plain_field_without_blocked_edges() {
        let target = Coord::new_unchecked(20, 10);
        // an irregular passable region: a ring with a wall in the middle
        let passable = |c: Coord| {
            (10..=30).contains(&c.x)
                && (5..=15).contains(&c.y)
                && !(c.x == 20 && c.y == 8)
                && !(c.x == 25 && (6..=12).contains(&c.y))
        };
        let plain = DijkstraField::compute(target, passable);
        let edged = DijkstraField::compute_with_edges(target, passable, |_, _| true);
        assert_eq!(plain.distances, edged.distances);
        let mut reused = DijkstraField::compute(Coord::new_unchecked(1, 1), |_| true);
        reused.compute_into_with_edges(target, passable, |_, _| true);
        assert_eq!(plain.distances, reused.distances);
        assert_eq!(reused.target, target);
        for x in 9..=31 {
            for y in 4..=16 {
                let c = Coord::new_unchecked(x, y);
                assert_eq!(
                    plain.steepest_descent(c),
                    edged.steepest_descent_by(c, |_, _| true)
                );
                assert_eq!(
                    plain.steepest_ascent(c),
                    edged.steepest_ascent_by(c, |_, _| true)
                );
            }
        }
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
