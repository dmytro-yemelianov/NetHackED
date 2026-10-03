//! Wand Beam Raycasting, Wall Reflection, and Energy Attenuation.
//!
//! Formally verified in `NetMechanics.Raycast`.
//! Proves reflection involution, discrete kinetic speed preservation, and finite loop termination.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SurfaceOrientation {
    Horizontal,
    Vertical,
    Corner,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Velocity {
    pub dx: i32,
    pub dy: i32,
}

impl Velocity {
    pub const fn new(dx: i32, dy: i32) -> Self {
        Self { dx, dy }
    }

    pub fn speed_sq(self) -> i32 {
        self.dx * self.dx + self.dy * self.dy
    }
}

pub fn reflect(v: Velocity, orientation: SurfaceOrientation) -> Velocity {
    match orientation {
        SurfaceOrientation::Horizontal => Velocity::new(v.dx, -v.dy),
        SurfaceOrientation::Vertical => Velocity::new(-v.dx, v.dy),
        SurfaceOrientation::Corner => Velocity::new(-v.dx, -v.dy),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BeamRay {
    pub x: i32,
    pub y: i32,
    pub vel: Velocity,
    pub energy: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepResult {
    Terminated,
    Advanced(BeamRay),
    Reflected(BeamRay),
}

pub fn step_ray(ray: BeamRay, hit_wall: Option<SurfaceOrientation>) -> StepResult {
    if ray.energy == 0 {
        return StepResult::Terminated;
    }
    let next_energy = ray.energy - 1;
    match hit_wall {
        None => StepResult::Advanced(BeamRay {
            x: ray.x + ray.vel.dx,
            y: ray.y + ray.vel.dy,
            vel: ray.vel,
            energy: next_energy,
        }),
        Some(orientation) => StepResult::Reflected(BeamRay {
            x: ray.x,
            y: ray.y,
            vel: reflect(ray.vel, orientation),
            energy: next_energy,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reflection_involution() {
        let v = Velocity::new(1, -1);
        for orientation in [
            SurfaceOrientation::Horizontal,
            SurfaceOrientation::Vertical,
            SurfaceOrientation::Corner,
        ] {
            assert_eq!(reflect(reflect(v, orientation), orientation), v);
        }
    }

    #[test]
    fn test_reflection_speed_squared_conservation() {
        let v = Velocity::new(3, -4);
        let orig_speed_sq = v.speed_sq();
        for orientation in [
            SurfaceOrientation::Horizontal,
            SurfaceOrientation::Vertical,
            SurfaceOrientation::Corner,
        ] {
            assert_eq!(reflect(v, orientation).speed_sq(), orig_speed_sq);
        }
    }

    #[test]
    fn test_energy_attenuation_and_termination() {
        let mut ray = BeamRay {
            x: 5,
            y: 5,
            vel: Velocity::new(1, 0),
            energy: 3,
        };

        // Step 1: Advance
        match step_ray(ray, None) {
            StepResult::Advanced(r) => {
                assert_eq!(r.energy, 2);
                assert_eq!(r.x, 6);
                ray = r;
            }
            _ => panic!("Expected advanced"),
        }

        // Step 2: Wall reflection
        match step_ray(ray, Some(SurfaceOrientation::Vertical)) {
            StepResult::Reflected(r) => {
                assert_eq!(r.energy, 1);
                assert_eq!(r.vel.dx, -1);
                ray = r;
            }
            _ => panic!("Expected reflected"),
        }

        // Step 3: Advance in reverse direction
        match step_ray(ray, None) {
            StepResult::Advanced(r) => {
                assert_eq!(r.energy, 0);
                assert_eq!(r.x, 5);
                ray = r;
            }
            _ => panic!("Expected advanced"),
        }

        // Step 4: Out of energy -> Terminated
        assert_eq!(step_ray(ray, None), StepResult::Terminated);
    }
}
