//! Frozen fingerprints of vanilla play. Arena runs and bestiary combat were
//! rebaselined on 2026-10-06 for the user-approved C-correct sampled-HP migration.
//! Arena runs were rebaselined again for approved item initialization/RNG draws.
//! All three families were rebaselined on 2026-10-07 for user-approved P04-P10 gameplay.
//! Unintended changes fail.
use nethacked_agent::arena::run_seed_games;
use nethacked_data::monsters::{MonsterArchetype, BESTIARY};
use nethacked_data::roles::{get_role, Gender, RaceId, RoleId};
use nethacked_sim::{ActionAst, SimulationWorld};
use nethacked_types::{Coord, Direction};

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
    2_228_682_572_056_874_257,
    1_092_902_970_258_507_207,
    13_826_362_149_808_501_255,
    8_647_613_173_857_399_611,
];
const EXPECTED_LOGS: [u64; 9] = [
    4_505_381_605_404_160_040,
    16_404_326_233_539_671_115,
    4_854_647_680_356_389_716,
    5_243_846_113_459_432_278,
    5_728_931_081_618_051_660,
    15_473_132_434_245_572_347,
    13_037_250_685_169_951_399,
    10_846_450_416_020_385_359,
    5_002_529_615_800_054_102,
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

/// Hero inventory as "name:enchantment" in arena order.
fn inventory_fingerprint(world: &SimulationWorld) -> String {
    world
        .arena
        .items_carried_by(world.player_id)
        .iter()
        .filter_map(|id| world.arena.items.get(*id))
        .map(|it| format!("{}:{};", it.name, it.enchantment))
        .collect()
}

fn character(role: RoleId) -> nethacked_data::CharacterConfig {
    nethacked_data::CharacterConfig {
        name: format!("{role:?}"),
        role,
        race: RaceId::Human,
        gender: Gender::Female,
        alignment: get_role(role).default_alignment,
    }
}

fn play_role(role: RoleId) -> u64 {
    let mut world = SimulationWorld::new_with_character(7, character(role));
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
    bytes.extend(inventory_fingerprint(&world).into_bytes());
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

const COMBAT_ROLES: [RoleId; 3] = [RoleId::Valkyrie, RoleId::Wizard, RoleId::Rogue];
const EXPECTED_COMBAT: [u64; 3] = [
    5_186_754_756_734_083_792,
    10_326_893_795_974_258_077,
    1_764_914_419_651_649_729,
];

/// Every bestiary entry (all 383 C species) fights the hero for up to 20
/// alternating melee/wait steps.
fn bestiary_combat(role: RoleId) -> u64 {
    assert_eq!(BESTIARY.len(), 383, "NetHack 5.0 has 383 species");
    combat_over(role, BESTIARY)
}

fn combat_over(role: RoleId, species: &[MonsterArchetype]) -> u64 {
    let mut all = Vec::new();
    for arch in species {
        let mut world = SimulationWorld::new_with_character(11, character(role));
        let hero = world.arena.actors.get(world.player_id).expect("hero").coord;
        let mut bytes = format!("{:?}|{}|", arch.id, arch.name).into_bytes();
        let Some(mid) = world.spawn_monster_near(arch.id, hero) else {
            bytes.extend(b"nospawn");
            all.push(fnv1a(&bytes));
            continue;
        };
        let mut events = Vec::new();
        for step in 0..20 {
            let (Some(h), Some(m)) = (
                world.arena.actors.get(world.player_id),
                world.arena.actors.get(mid),
            ) else {
                break;
            };
            if h.is_dead || m.is_dead {
                break;
            }
            let action = if step % 2 == 0 {
                ActionAst::MeleeAttack(m.coord)
            } else {
                ActionAst::Wait
            };
            events.push(world.step_player_action(action));
        }
        let h = world
            .arena
            .actors
            .get(world.player_id)
            .map(|h| (h.hp, h.ac));
        let m = world
            .arena
            .actors
            .get(mid)
            .map(|m| (m.hp, m.is_peaceful, m.is_dead));
        bytes.extend(serde_json::to_vec(&(&events, &world.event_log, h, m)).expect("serialize"));
        all.push(fnv1a(&bytes));
    }
    let bytes = serde_json::to_vec(&all).expect("serialize");
    fnv1a(&bytes)
}

#[test]
fn golden_vanilla_bestiary_combat_is_unchanged() {
    let got: Vec<u64> = COMBAT_ROLES.iter().map(|&r| bestiary_combat(r)).collect();
    assert_eq!(got, EXPECTED_COMBAT);
}
