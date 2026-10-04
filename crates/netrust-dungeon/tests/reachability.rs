//! Seed sweeps: every generated level's required locations are reachable from the arrival point.

use netrust_dungeon::*;
use netrust_types::{Coord, Tile};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

const SEEDS: u64 = 200;

fn rng(seed: u64) -> ChaCha8Rng {
    ChaCha8Rng::seed_from_u64(seed)
}

fn assert_reach(level: &DungeonLevel, targets: &[Coord], ctx: &str) {
    let reach = reachable_from(level, level.stairs_up);
    for t in targets {
        assert!(
            reach.contains(t),
            "{}: {:?} unreachable from {:?}",
            ctx,
            t,
            level.stairs_up
        );
    }
}

#[test]
fn regular_dungeon_levels_connected() {
    for s in 0..SEEDS {
        let l = generate_dungeon_level(&mut rng(s));
        assert_reach(&l, &[l.stairs_down], &format!("dungeon seed {s}"));
    }
}

#[test]
fn sokoban_prize_reachable_once_pits_are_filled() {
    let (l, boulders) = generate_sokoban_level(1);
    let reach = reachable_from_with(&l, l.stairs_up, |t| {
        t.is_passable() || matches!(t, Tile::Pit { .. })
    });
    assert!(reach.contains(&l.stairs_down), "prize room sealed off");
    assert!(reach.contains(&Coord::new(64, 10).unwrap()));
    let pits = l
        .tiles
        .iter()
        .flatten()
        .filter(|t| matches!(t, Tile::Pit { filled: false }))
        .count();
    assert!(boulders.len() >= pits);
}

#[test]
fn minetown_shops_temple_reachable() {
    for s in 0..SEEDS {
        let lay = generate_minetown_level(&mut rng(s));
        let mut targets = vec![lay.level.stairs_down, lay.priest_coord, lay.altar_coord];
        targets.extend(lay.shopkeeper_coords.iter().copied());
        targets.extend(lay.watchmen_coords.iter().copied());
        assert_reach(&lay.level, &targets, &format!("minetown seed {s}"));
    }
}

#[test]
fn mines_levels_connected() {
    for s in 0..SEEDS {
        for d in [1usize, 2, 4] {
            let l = generate_mines_cavern_level(&mut rng(s), d);
            assert_reach(&l, &[l.stairs_down], &format!("mines d{d} seed {s}"));
        }
        let (l, luck) = generate_mines_end_level(&mut rng(s));
        assert_reach(&l, &[luck], &format!("mines end seed {s}"));
    }
}

#[test]
fn quest_levels_connected() {
    for s in 0..SEEDS {
        let l = generate_quest_locate_level(&mut rng(s), 2);
        assert_reach(&l, &[l.stairs_down], &format!("quest locate seed {s}"));
        let home = generate_quest_home_level(&mut rng(s), "valkyrie");
        assert_reach(
            &home.level,
            &[home.leader_coord],
            &format!("quest home seed {s}"),
        );
    }
}

#[test]
fn gehennom_levels_connected() {
    for s in 0..SEEDS {
        let (l, _) = generate_gehennom_maze_level(&mut rng(s), 3, false);
        let mut targets = vec![l.stairs_down];
        targets.extend(l.rooms.iter().map(|r| r.center()));
        assert_reach(&l, &targets, &format!("gehennom seed {s}"));

        let (l, vs) = generate_gehennom_maze_level(&mut rng(s), 5, true);
        let mut targets = vec![vs.unwrap()];
        targets.extend(l.rooms.iter().map(|r| r.center()));
        assert_reach(&l, &targets, &format!("gehennom vs seed {s}"));

        let v = generate_valley_of_the_dead(&mut rng(s));
        assert_reach(&v, &[v.stairs_down], &format!("valley seed {s}"));
    }
}

#[test]
fn sanctum_has_up_stairs_and_reachable_altar() {
    let (l, spawn) = generate_moloch_sanctum_level(&mut rng(1));
    assert!(matches!(l.get_tile(spawn), Tile::Stairs { up: true }));
    assert_reach(&l, &[l.stairs_down], "sanctum");
}

#[test]
fn find_free_floor_skips_stairs_and_avoid() {
    let l = generate_dungeon_level(&mut rng(3));
    let room = l.rooms[0];
    let avoid = [room.center()];
    let free = find_free_floor(&l, &room.rect, &avoid);
    assert!(!free.is_empty());
    for c in &free {
        assert_eq!(*l.get_tile(*c), Tile::Room);
        assert!(*c != l.stairs_up && *c != l.stairs_down && *c != room.center());
        assert!(room.contains_inner(*c));
    }
}
