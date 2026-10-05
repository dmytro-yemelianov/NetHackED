//! Ranged combat and riding mechanics.

/// Resolves whether a fired projectile breaks upon impact.
/// `break_prob` is the percentage chance of breaking (0-100).
/// `roll` is a random value (0-99).
/// Returns `true` if destroyed, `false` if it drops to the floor.
pub fn resolve_projectile_impact(break_prob: u8, roll: u8) -> bool {
    roll < break_prob
}

/// Calculates the effective movement cost when potentially mounted.
/// Returns the minimum of the unmounted cost and the mount cost (if mounted).
pub fn effective_movement_cost(unmounted_cost: u32, mount_cost: Option<u32>) -> u32 {
    match mount_cost {
        Some(cost) => unmounted_cost.min(cost),
        None => unmounted_cost,
    }
}

/// Determines if a monster can be mounted.
/// Requires the monster to be tame and have a saddle equipped.
pub fn can_mount(is_tame: bool, has_saddle: bool) -> bool {
    is_tame && has_saddle
}
