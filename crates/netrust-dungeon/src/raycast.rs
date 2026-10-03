//! Raycasting and beam propagation through dungeon geometry.
//!
//! Formally verified in `NetMechanics.Raycast`: strictly terminates in at most `max_energy` steps.

use netrust_core::{step_ray, BeamRay, StepResult, SurfaceOrientation, Velocity};
use netrust_types::{Coord, Direction, Tile, COLNO};
use crate::level::DungeonLevel;

/// Traces a wand beam ray through the dungeon grid, handling wall reflections.
/// Verified in `NetMechanics.Raycast`: strictly terminates in at most `max_energy` steps.
pub fn trace_beam_path(
    level: &DungeonLevel,
    origin: Coord,
    dir: Direction,
    max_energy: u32,
) -> Vec<Coord> {
    let mut path = Vec::new();
    let (dx, dy) = dir.delta();
    if dx == 0 && dy == 0 {
        return path;
    }

    let mut ray = BeamRay {
        x: origin.x as i32,
        y: origin.y as i32,
        vel: Velocity::new(dx as i32, dy as i32),
        energy: max_energy,
    };

    while ray.energy > 0 {
        let nx = ray.x + ray.vel.dx;
        let ny = ray.y + ray.vel.dy;

        let Some(target_coord) = Coord::new(nx as usize, ny as usize) else {
            let hit = if nx < 0 || nx >= COLNO as i32 {
                SurfaceOrientation::Vertical
            } else {
                SurfaceOrientation::Horizontal
            };
            match step_ray(ray, Some(hit)) {
                StepResult::Reflected(r) => {
                    ray = r;
                    continue;
                }
                _ => break,
            }
        };

        let tile = level.get_tile(target_coord);
        if tile.is_passable() || tile.is_transparent() {
            match step_ray(ray, None) {
                StepResult::Advanced(r) => {
                    ray = r;
                    path.push(target_coord);
                }
                _ => break,
            }
        } else {
            let orientation = match tile {
                Tile::Wall { horizontal } => {
                    if *horizontal {
                        SurfaceOrientation::Horizontal
                    } else {
                        SurfaceOrientation::Vertical
                    }
                }
                _ => SurfaceOrientation::Corner,
            };

            match step_ray(ray, Some(orientation)) {
                StepResult::Reflected(r) => {
                    ray = r;
                }
                _ => break,
            }
        }
    }

    path
}
