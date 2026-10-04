//! Frozen fingerprints of vanilla play. Recorded on main before the rule-pack
//! refactor (P1); every later change must keep them identical.
use netrust_agent::arena::run_seed_games;
use netrust_data::roles::{get_role, Gender, RaceId, RoleId};
use netrust_sim::{ActionAst, SimulationWorld};
use netrust_types::{Coord, Direction};

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

const ROLES: [RoleId; 9] = [
    RoleId::Valkyrie,
    RoleId::Wizard,
    RoleId::Barbarian,
    RoleId::Rogue,
    RoleId::Knight,
    RoleId::Monk,
    RoleId::Healer,
    RoleId::Tourist,
    RoleId::Archaeologist,
];

const DIRS: [Direction; 8] = [
    Direction::North,
    Direction::NorthEast,
    Direction::East,
    Direction::SouthEast,
    Direction::South,
    Direction::SouthWest,
    Direction::West,
    Direction::NorthWest,
];

const EXPECTED_RUNS: [u64; 4] = [
    10_590_427_399_958_928_342,
    11_826_438_408_668_846_075,
    1_437_892_308_041_849_470,
    5_992_343_144_856_610_526,
];
const EXPECTED_LOGS: [u64; 9] = [
    10_284_077_770_640_603_820,
    1_873_923_448_637_030_106,
    12_887_439_925_162_047_825,
    3_639_312_384_190_478_179,
    5_108_422_725_681_711_348,
    12_003_228_215_816_724_814,
    15_989_717_679_611_174_760,
    13_193_988_166_941_799_923,
    7_773_727_900_926_177_870,
];

#[test]
fn golden_vanilla_runs_are_unchanged() {
    let mut fingerprints = Vec::new();
    for seed in 1..=4u64 {
        let results = run_seed_games(seed, &ROLES, 400);
        let json = serde_json::to_vec(&results).expect("RunResult serializes");
        fingerprints.push(fnv1a(&json));
    }
    assert_eq!(fingerprints, EXPECTED_RUNS);
}

/// Neighbouring coordinate of `c` in direction `d` (saturating at 0).
fn neighbour(c: Coord, d: Direction) -> Coord {
    let (dx, dy): (isize, isize) = match d {
        Direction::North => (0, -1),
        Direction::NorthEast => (1, -1),
        Direction::East => (1, 0),
        Direction::SouthEast => (1, 1),
        Direction::South => (0, 1),
        Direction::SouthWest => (-1, 1),
        Direction::West => (-1, 0),
        Direction::NorthWest => (-1, -1),
        _ => (0, 0),
    };
    Coord {
        x: c.x.saturating_add_signed(dx),
        y: c.y.saturating_add_signed(dy),
    }
}

/// Scripted action for step `i`: movement (8 dirs x5), search x10, rest x20,
/// then melee into each neighbour, a fire, a pick-up and a descend attempt.
fn scripted_action(i: usize, player: Coord) -> ActionAst {
    // One cycle = 40 moves + 10 searches + 20 waits + 8 melee + fire + pickup + descend = 81.
    let k = i % 81;
    match k {
        0..=39 => ActionAst::Move(DIRS[k / 5]),
        40..=49 => ActionAst::Search,
        50..=69 => ActionAst::Wait,
        70..=77 => ActionAst::MeleeAttack(neighbour(player, DIRS[k - 70])),
        78 => ActionAst::Fire(Direction::East),
        79 => ActionAst::PickUp,
        _ => ActionAst::Descend,
    }
}

fn play_role(role: RoleId) -> u64 {
    let config = netrust_data::CharacterConfig {
        name: format!("{role:?}"),
        role,
        race: RaceId::Human,
        gender: Gender::Female,
        alignment: get_role(role).default_alignment,
    };
    let mut world = SimulationWorld::new_with_character(7, config);
    let mut step_events = Vec::new();
    for i in 0..300 {
        let player = world.arena.actors.get(world.player_id).map(|a| a.coord);
        let Some(player) = player else { break };
        let events = world.step_player_action(scripted_action(i, player));
        step_events.push(events);
    }
    let p = world.arena.actors.get(world.player_id).expect("player");
    let summary = (
        p.hp,
        p.max_hp,
        p.ac,
        world.depth,
        world.scheduler.turn,
        p.coord,
    );
    let mut bytes = format!("{role:?}").into_bytes();
    bytes.extend(serde_json::to_vec(&world.event_log).expect("event_log serializes"));
    bytes.extend(serde_json::to_vec(&step_events).expect("step events serialize"));
    bytes.extend(serde_json::to_vec(&summary).expect("summary serializes"));
    fnv1a(&bytes)
}

#[test]
fn golden_vanilla_event_logs_are_unchanged() {
    let logs: Vec<u64> = ROLES.iter().map(|&r| play_role(r)).collect();
    assert_eq!(logs, EXPECTED_LOGS);
}
