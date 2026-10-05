//! Frozen fingerprints of vanilla play. Recorded on main before the rule-pack
//! refactor (P1); every later change must keep them identical.
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
    10_590_427_399_958_928_342,
    5_851_926_545_638_829_079,
    6_575_111_144_931_471_423,
    13_769_611_620_460_411_800,
];
const EXPECTED_LOGS: [u64; 9] = [
    6_084_688_683_205_385_671,
    15_616_401_141_373_472_836,
    4_899_398_478_763_589_659,
    16_065_937_482_197_730_413,
    2_131_477_169_023_326_163,
    14_544_001_921_339_539_876,
    14_894_273_454_409_416_984,
    13_370_509_600_343_581_488,
    9_102_617_103_894_820_477,
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
    4_392_970_861_794_522_748,
    12_723_840_377_594_850_747,
    6_972_671_921_932_146_888,
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
