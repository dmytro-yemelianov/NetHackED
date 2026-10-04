//! Spawned key items and monsters on generated levels are reachable and never block stairs.

use netrust_arena::ItemLocation;
use netrust_dungeon::reachable_from;
use netrust_sim::{clamp_mysterious_force, SimulationWorld};
use netrust_types::BranchId;

fn floor_coords_named(sim: &SimulationWorld, name: &str) -> Vec<netrust_sim::Coord> {
    sim.arena
        .items
        .values()
        .filter(|it| it.name == name)
        .filter_map(|it| match it.location {
            ItemLocation::Floor(c) => Some(c),
            _ => None,
        })
        .collect()
}

#[test]
fn invocation_items_reachable_on_every_seed() {
    for seed in 0..100u64 {
        let mut sim = SimulationWorld::new_with_seed(seed);
        sim.current_branch = BranchId::Gehennom;
        sim.depth = 5;
        sim.unpack_or_generate_level(BranchId::Gehennom, 5);
        let reach = reachable_from(&sim.level, sim.level.stairs_up);
        let candles = floor_coords_named(&sim, "wax candle");
        assert_eq!(candles.len(), 7, "seed {seed}");
        for name in [
            "wax candle",
            "Candelabrum of Invocation",
            "Book of the Dead",
        ] {
            for c in floor_coords_named(&sim, name) {
                assert!(
                    reach.contains(&c),
                    "seed {seed}: {name} at {c:?} unreachable"
                );
            }
        }
        assert!(
            reach.contains(&sim.vibrating_square.unwrap()),
            "seed {seed}"
        );
    }
}

#[test]
fn spawned_monsters_never_on_stairs_or_stacked() {
    let cases = [
        (BranchId::GnomishMines, 1usize),
        (BranchId::GnomishMines, 5),
        (BranchId::Gehennom, 1),
        (BranchId::Gehennom, 3),
        (BranchId::Gehennom, 5),
        (BranchId::Quest, 2),
        (BranchId::Gehennom, 6),
        (BranchId::GnomishMines, 3),
        (BranchId::Quest, 1),
        (BranchId::Quest, 3),
        (BranchId::DungeonsOfDoom, 2),
    ];
    for seed in 0..50u64 {
        for (branch, depth) in cases {
            let mut sim = SimulationWorld::new_with_seed(seed);
            let pre: Vec<_> = sim.arena.actors.keys().collect();
            sim.current_branch = branch;
            sim.depth = depth;
            sim.unpack_or_generate_level(branch, depth);
            let mut seen = std::collections::HashSet::new();
            for (id, a) in sim.arena.actors.iter() {
                if pre.contains(&id) {
                    continue;
                }
                assert!(
                    a.coord != sim.level.stairs_up && a.coord != sim.level.stairs_down,
                    "{branch:?} d{depth} seed {seed}: {} on stairs",
                    a.name
                );
                assert!(
                    seen.insert(a.coord),
                    "{branch:?} d{depth} seed {seed}: stacked at {:?}",
                    a.coord
                );
            }
        }
    }
}

#[test]
fn mysterious_force_never_reaches_sanctum_without_invocation() {
    for depth in 1..=5usize {
        for roll in 0..300u32 {
            if let Some(pushed) = netrust_core::calculate_mysterious_force(depth, roll) {
                assert!(clamp_mysterious_force(pushed, false) <= 5);
                assert_eq!(clamp_mysterious_force(pushed, true), pushed);
            }
        }
    }
}
