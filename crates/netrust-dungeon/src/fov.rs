//! Field of View (FOV) computation via symmetric raycasting.
//!
//! Modeled in Lean 4 (`NetMechanics.FOV`); the Lean LOS model is transparent-cell connectivity, not straight-line `clear_path` (see docs/lean4-verification-guide.md, Known limitations).

use crate::level::DungeonLevel;
use netrust_types::{Coord, COLNO, ROWNO};
use std::collections::HashSet;

/// Compute Field of View (FOV) using symmetric raycasting.
/// Is intended to mirror the reflexivity and symmetry theorems stated in `NetMechanics.FOV` (no proptest links them).
pub fn compute_fov(level: &DungeonLevel, origin: Coord, max_radius: usize) -> HashSet<Coord> {
    let mut visible = HashSet::new();
    visible.insert(origin);

    // Cast rays to perimeter of the bounding square [origin - max_radius, origin + max_radius]
    let min_x = origin.x.saturating_sub(max_radius);
    let max_x = (origin.x + max_radius).min(COLNO - 1);
    let min_y = origin.y.saturating_sub(max_radius);
    let max_y = (origin.y + max_radius).min(ROWNO - 1);

    for x in min_x..=max_x {
        cast_ray(level, origin, Coord::new_unchecked(x, min_y), &mut visible);
        cast_ray(level, origin, Coord::new_unchecked(x, max_y), &mut visible);
    }
    for y in min_y..=max_y {
        cast_ray(level, origin, Coord::new_unchecked(min_x, y), &mut visible);
        cast_ray(level, origin, Coord::new_unchecked(max_x, y), &mut visible);
    }

    visible
}

fn cast_ray(level: &DungeonLevel, from: Coord, to: Coord, visible: &mut HashSet<Coord>) {
    let mut x = from.x as isize;
    let mut y = from.y as isize;
    let dx = (to.x as isize - x).abs();
    let dy = (to.y as isize - y).abs();
    let sx = if from.x < to.x { 1 } else { -1 };
    let sy = if from.y < to.y { 1 } else { -1 };
    let mut err = dx - dy;

    loop {
        let current = Coord::new_unchecked(x as usize, y as usize);
        visible.insert(current);

        if current == to {
            break;
        }

        // If tile blocks light, ray terminates after revealing the blocking tile
        if current != from && !level.is_transparent(current) {
            break;
        }

        let e2 = 2 * err;
        if e2 > -dy {
            err -= dy;
            x += sx;
        }
        if e2 < dx {
            err += dx;
            y += sy;
        }

        if x < 0 || x >= COLNO as isize || y < 0 || y >= ROWNO as isize {
            break;
        }
    }
}

/// Compute illuminated tiles from multiple light sources.
/// Each light source provides an origin coordinate and luminescence radius.
pub fn compute_illumination(
    level: &DungeonLevel,
    light_sources: &[(Coord, u32)],
) -> HashSet<Coord> {
    let mut illuminated = HashSet::new();
    for &(origin, radius) in light_sources {
        if radius > 0 {
            let fov = compute_fov(level, origin, radius as usize);
            illuminated.extend(fov);
        }
    }
    illuminated
}
