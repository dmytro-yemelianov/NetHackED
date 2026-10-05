//! Doors block diagonal moves for the hero, pets and monsters.
//!
//! NetHack C reference:
//! - `hack.c:1141` / `hack.c:1209` `test_move` (diagonal into / out of an intact doorway)
//! - `hack.c:1074-1100` closed door auto-open before the diagonal test
//! - `hack.c:4063-4073` `doorless_door` (`D_NODOOR` and `D_BROKEN` are doorless)
//! - `hack.c:2843` a refused move costs no time
//! - `hack.c:2794-2810` attacks are resolved before `test_move`
//! - `uhitm.c:474` `!rn2(7)` draw of `do_attack` (peaceful bump)
//! - `mon.c:2250-2257` `mfndpos` (monsters)

use nethacked_arena::ActorId;
use nethacked_core::ActionAst;
use nethacked_data::{create_monster_record, MonsterSpeciesId};
use nethacked_i18n::{Locale, Messages};
use nethacked_sim::{GameEvent, SimulationWorld};
use nethacked_types::{Coord, Direction, DoorState, Tile};
use rand::Rng;

const HERO: (usize, usize) = (40, 10);

fn c(x: usize, y: usize) -> Coord {
    Coord::new_unchecked(x, y)
}

fn door(state: DoorState) -> Tile {
    Tile::Door {
        state,
        trapped: false,
    }
}

/// A world whose only actor is the hero at (40, 10), standing in a room
/// (x 30..=50, y 5..=15) free of traps, engravings and floor items.
fn arena(seed: u64) -> SimulationWorld {
    let mut w = SimulationWorld::new_with_seed(seed);
    w.set_locale(Locale::En);
    let others: Vec<ActorId> = w
        .arena
        .actors
        .iter()
        .filter(|(_, a)| !a.is_player)
        .map(|(id, _)| id)
        .collect();
    for id in others {
        w.arena.destroy_actor(id);
    }
    let floor_items: Vec<_> = w
        .arena
        .items
        .iter()
        .filter(|(_, it)| matches!(it.location, nethacked_arena::ItemLocation::Floor(_)))
        .map(|(id, _)| id)
        .collect();
    for id in floor_items {
        w.arena.destroy_item(id);
    }
    w.level.traps.clear();
    w.level.engravings.clear();
    for x in 30..=50 {
        for y in 5..=15 {
            w.level.set_tile(c(x, y), Tile::Room);
        }
    }
    w.arena.actors.get_mut(w.player_id).unwrap().coord = c(HERO.0, HERO.1);
    w
}

fn hero(w: &SimulationWorld) -> Coord {
    w.arena.actors.get(w.player_id).unwrap().coord
}

/// Scheduler state: unchanged exactly when the action took no time.
fn clock(w: &SimulationWorld) -> (u64, u32, u32) {
    (
        w.scheduler.turn,
        w.scheduler.hero_energy,
        w.scheduler.monster_energy,
    )
}

fn texts(events: &[GameEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            GameEvent::LogMessage { text } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn hero_moved(w: &SimulationWorld, events: &[GameEvent]) -> bool {
    events
        .iter()
        .any(|e| matches!(e, GameEvent::ActorMoved { actor, .. } if *actor == w.player_id))
}

const DIAGONALS: [Direction; 4] = [
    Direction::NorthEast,
    Direction::SouthEast,
    Direction::SouthWest,
    Direction::NorthWest,
];

fn at(dir: Direction) -> Coord {
    let (dx, dy) = dir.delta();
    c(
        (HERO.0 as isize + dx) as usize,
        (HERO.1 as isize + dy) as usize,
    )
}

#[test]
fn hero_cannot_move_diagonally_into_open_door() {
    for dir in DIAGONALS {
        let mut w = arena(1);
        w.level.set_tile(at(dir), door(DoorState::Open));
        let before = clock(&w);
        let events = w.step_player_action(ActionAst::Move(dir));
        assert_eq!(hero(&w), c(HERO.0, HERO.1), "{dir:?}: hero must not move");
        assert!(!hero_moved(&w, &events), "{dir:?}");
        assert_eq!(
            texts(&events),
            vec![Messages::no_diagonal_into_doorway(Locale::En).to_string()],
            "{dir:?}: exactly one refusal message"
        );
        assert_eq!(
            events.len(),
            1,
            "{dir:?}: the refusal is the only event: {events:?}"
        );
        assert_eq!(clock(&w), before, "{dir:?}: a refused move takes no time");
    }
    // Control: the same door entered orthogonally works and takes time.
    let mut w = arena(1);
    w.level.set_tile(c(41, 10), door(DoorState::Open));
    let before = clock(&w);
    let events = w.step_player_action(ActionAst::Move(Direction::East));
    assert_eq!(hero(&w), c(41, 10));
    assert!(hero_moved(&w, &events));
    assert_ne!(clock(&w), before);
}

#[test]
fn hero_cannot_move_diagonally_out_of_open_door() {
    for dir in DIAGONALS {
        let mut w = arena(2);
        w.level.set_tile(c(HERO.0, HERO.1), door(DoorState::Open));
        let before = clock(&w);
        let events = w.step_player_action(ActionAst::Move(dir));
        assert_eq!(hero(&w), c(HERO.0, HERO.1), "{dir:?}: hero must not move");
        assert_eq!(
            texts(&events),
            vec![Messages::no_diagonal_out_of_doorway(Locale::En).to_string()],
            "{dir:?}"
        );
        assert_eq!(events.len(), 1, "{dir:?}: {events:?}");
        assert_eq!(clock(&w), before, "{dir:?}: a refused move takes no time");
    }
    // Control: orthogonally out of the doorway works.
    let mut w = arena(2);
    w.level.set_tile(c(HERO.0, HERO.1), door(DoorState::Open));
    let events = w.step_player_action(ActionAst::Move(Direction::North));
    assert_eq!(hero(&w), c(HERO.0, HERO.1 - 1));
    assert!(hero_moved(&w, &events));
}

#[test]
fn hero_may_move_diagonally_through_broken_door_and_empty_doorway() {
    for state in [DoorState::Broken, DoorState::NoDoor] {
        for dir in DIAGONALS {
            // into
            let mut w = arena(3);
            w.level.set_tile(at(dir), door(state));
            let before = clock(&w);
            let events = w.step_player_action(ActionAst::Move(dir));
            assert_eq!(hero(&w), at(dir), "{state:?} into {dir:?}");
            assert!(hero_moved(&w, &events));
            assert_ne!(clock(&w), before, "{state:?}: the move takes time");
            assert!(texts(&events).iter().all(|t| !t.contains("diagonally")));
            // out of
            let mut w = arena(3);
            w.level.set_tile(c(HERO.0, HERO.1), door(state));
            let events = w.step_player_action(ActionAst::Move(dir));
            assert_eq!(hero(&w), at(dir), "{state:?} out of {dir:?}");
            assert!(hero_moved(&w, &events));
        }
    }
}

#[test]
fn diagonal_bump_into_closed_door_opens_it_like_c_autoopen() {
    let mut w = arena(4);
    let d = at(Direction::NorthEast);
    w.level.set_tile(d, door(DoorState::Closed));
    let before = clock(&w);
    let events = w.step_player_action(ActionAst::Move(Direction::NorthEast));
    // hack.c:1074-1100: the closed door is auto-opened before the diagonal test.
    assert!(events.contains(&GameEvent::DoorToggled {
        coord: d,
        new_state: DoorState::Open,
    }));
    assert_eq!(*w.level.get_tile(d), door(DoorState::Open));
    assert_eq!(hero(&w), c(HERO.0, HERO.1));
    assert_ne!(clock(&w), before, "opening the door takes a move");

    // The next diagonal attempt is refused (the door is now intact and open).
    let before = clock(&w);
    let events = w.step_player_action(ActionAst::Move(Direction::NorthEast));
    assert_eq!(
        texts(&events),
        vec![Messages::no_diagonal_into_doorway(Locale::En).to_string()]
    );
    assert_eq!(hero(&w), c(HERO.0, HERO.1));
    assert_eq!(clock(&w), before);
}

#[test]
fn locked_door_diagonal_keeps_existing_behaviour() {
    let mut w = arena(5);
    let d = at(Direction::NorthEast);
    w.level.set_tile(d, door(DoorState::Locked));
    let before = clock(&w);
    let events = w.step_player_action(ActionAst::Move(Direction::NorthEast));
    // Same as an orthogonal bump: the solid-door bump text, no time, no move.
    assert_eq!(
        texts(&events),
        vec![Messages::bump_wall(Locale::En).to_string()]
    );
    assert_eq!(*w.level.get_tile(d), door(DoorState::Locked));
    assert_eq!(hero(&w), c(HERO.0, HERO.1));
    assert_eq!(clock(&w), before);
}

#[test]
fn hero_can_attack_monster_standing_in_doorway_diagonally() {
    // hack.c:2794-2810: the attack happens before test_move.
    let mut w = arena(6);
    let d = at(Direction::NorthEast);
    w.level.set_tile(d, door(DoorState::Open));
    let jackal = w
        .arena
        .spawn_actor(create_monster_record(MonsterSpeciesId::Jackal, d));
    let before = clock(&w);
    let events = w.step_player_action(ActionAst::Move(Direction::NorthEast));
    assert!(
        events.iter().any(|e| matches!(
            e,
            GameEvent::AttackLanded { attacker, target, .. }
                | GameEvent::AttackMissed { attacker, target }
                if *attacker == w.player_id && *target == jackal
        )),
        "the hero must attack the jackal: {events:?}"
    );
    assert!(texts(&events).iter().all(|t| !t.contains("diagonally")));
    assert_eq!(hero(&w), c(HERO.0, HERO.1));
    assert_ne!(clock(&w), before, "an attack takes time");
}

#[test]
fn pet_swap_diagonal_through_doorway_is_refused_and_takes_no_time() {
    let d = at(Direction::NorthEast);
    // Pet in the doorway, hero diagonal to it: refused (into).
    let mut w = arena(7);
    w.level.set_tile(d, door(DoorState::Open));
    let pet = w
        .arena
        .spawn_actor(create_monster_record(MonsterSpeciesId::Kitten, d));
    assert!(w.arena.actors.get(pet).unwrap().is_tame);
    let before = clock(&w);
    let events = w.step_player_action(ActionAst::Move(Direction::NorthEast));
    assert_eq!(hero(&w), c(HERO.0, HERO.1));
    assert_eq!(w.arena.actors.get(pet).unwrap().coord, d);
    assert_eq!(
        texts(&events),
        vec![Messages::no_diagonal_into_doorway(Locale::En).to_string()]
    );
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(clock(&w), before);

    // Hero in the doorway, pet diagonal to it: refused (out of).
    let mut w = arena(7);
    w.level.set_tile(c(HERO.0, HERO.1), door(DoorState::Open));
    let pet = w
        .arena
        .spawn_actor(create_monster_record(MonsterSpeciesId::Kitten, d));
    let before = clock(&w);
    let events = w.step_player_action(ActionAst::Move(Direction::NorthEast));
    assert_eq!(hero(&w), c(HERO.0, HERO.1));
    assert_eq!(w.arena.actors.get(pet).unwrap().coord, d);
    assert_eq!(
        texts(&events),
        vec![Messages::no_diagonal_out_of_doorway(Locale::En).to_string()]
    );
    assert_eq!(clock(&w), before);

    // Controls: a broken door and an orthogonal swap still swap and take time.
    let mut w = arena(7);
    w.level.set_tile(d, door(DoorState::Broken));
    let pet = w
        .arena
        .spawn_actor(create_monster_record(MonsterSpeciesId::Kitten, d));
    let before = clock(&w);
    w.step_player_action(ActionAst::Move(Direction::NorthEast));
    assert_eq!(hero(&w), d);
    assert_eq!(w.arena.actors.get(pet).unwrap().coord, c(HERO.0, HERO.1));
    assert_ne!(clock(&w), before);

    let mut w = arena(7);
    w.level.set_tile(c(41, 10), door(DoorState::Open));
    let pet = w
        .arena
        .spawn_actor(create_monster_record(MonsterSpeciesId::Kitten, c(41, 10)));
    w.step_player_action(ActionAst::Move(Direction::East));
    assert_eq!(hero(&w), c(41, 10));
    assert_eq!(w.arena.actors.get(pet).unwrap().coord, c(HERO.0, HERO.1));
}

#[test]
fn peaceful_bump_draws_rn2_7_before_the_door_check() {
    // uhitm.c:474 `!rn2(7)` is drawn by do_attack before test_move runs, so a
    // refused diagonal bump into a peaceful in a doorway consumes exactly one
    // rn2(7) draw (the door check itself draws nothing).
    let d = at(Direction::NorthEast);
    let mut refused = 0;
    let mut stopped = 0;
    for seed in 1..=60u64 {
        let mut w = arena(seed);
        w.level.set_tile(d, door(DoorState::Open));
        let mut jackal = create_monster_record(MonsterSpeciesId::Jackal, d);
        jackal.is_peaceful = true;
        w.arena.spawn_actor(jackal);

        let mut probe = w.rng.clone();
        let must_stop = probe.random_range(0..7u32) == 0;
        let before = clock(&w);
        let events = w.step_player_action(ActionAst::Move(Direction::NorthEast));
        assert_eq!(hero(&w), c(HERO.0, HERO.1), "seed {seed}");
        let t = texts(&events);
        if must_stop {
            stopped += 1;
            // "You stop. <mon> is in the way!" wins; no door message.
            assert!(
                t.iter().any(|m| m.contains("in the way")),
                "seed {seed}: {t:?}"
            );
            assert!(t.iter().all(|m| !m.contains("diagonally")), "seed {seed}");
        } else {
            refused += 1;
            assert_eq!(
                t,
                vec![Messages::no_diagonal_into_doorway(Locale::En).to_string()],
                "seed {seed}"
            );
            assert_eq!(clock(&w), before, "seed {seed}: no time");
            // Exactly the one rn2(7) draw was consumed: the next draws agree.
            let next_real: u64 = w.rng.random();
            let next_probe: u64 = probe.random();
            assert_eq!(next_real, next_probe, "seed {seed}: draw count differs");
        }
    }
    assert!(
        refused > 20 && stopped > 0,
        "refused {refused} stopped {stopped}"
    );
}

#[test]
fn refusal_text_is_localized() {
    let mut w = arena(8);
    w.set_locale(Locale::Uk);
    w.level
        .set_tile(at(Direction::NorthEast), door(DoorState::Open));
    let events = w.step_player_action(ActionAst::Move(Direction::NorthEast));
    assert_eq!(
        texts(&events),
        vec!["Ви не можете рухатися по діагоналі в непошкоджений дверний проріз.".to_string()]
    );
    let mut w = arena(8);
    w.set_locale(Locale::Uk);
    w.level.set_tile(c(HERO.0, HERO.1), door(DoorState::Open));
    let events = w.step_player_action(ActionAst::Move(Direction::NorthEast));
    assert_eq!(
        texts(&events),
        vec!["Ви не можете рухатися по діагоналі з непошкодженого дверного прорізу.".to_string()]
    );
    // English texts are the C strings (hack.c:1147, :1212).
    assert_eq!(
        Messages::no_diagonal_into_doorway(Locale::En),
        "You can't move diagonally into an intact doorway."
    );
    assert_eq!(
        Messages::no_diagonal_out_of_doorway(Locale::En),
        "You can't move diagonally out of an intact doorway."
    );
}

// ---------------------------------------------------------------- monsters

/// A wall column at x = 41 (y 5..=15) with a door of `state` at (41, 10).
fn walled(seed: u64, state: DoorState) -> SimulationWorld {
    let mut w = arena(seed);
    for y in 5..=15 {
        w.level.set_tile(c(41, y), Tile::Wall { horizontal: false });
    }
    w.level.set_tile(c(41, 10), door(state));
    w
}

fn is_diagonal(from: Coord, to: Coord) -> bool {
    from.x != to.x && from.y != to.y
}

/// All moves of `actor` in `events` as (from, to).
fn moves_of(events: &[GameEvent], actor: ActorId) -> Vec<(Coord, Coord)> {
    events
        .iter()
        .filter_map(|e| match e {
            GameEvent::ActorMoved {
                actor: a, from, to, ..
            } if *a == actor => Some((*from, *to)),
            _ => None,
        })
        .collect()
}

#[test]
fn hostile_monster_never_steps_diagonally_across_a_doorway() {
    let door_c = c(41, 10);
    let mut w = walled(9, DoorState::Open);
    // Hero east of the wall, jackal west of it.
    w.arena.actors.get_mut(w.player_id).unwrap().coord = c(43, 10);
    let jackal = w
        .arena
        .spawn_actor(create_monster_record(MonsterSpeciesId::Jackal, c(39, 8)));
    let mut steps = Vec::new();
    let mut reached = false;
    for _ in 0..20 {
        let events = w.step_player_action(ActionAst::Wait);
        steps.extend(moves_of(&events, jackal));
        let jc = w.arena.actors.get(jackal).unwrap().coord;
        if jc.chebyshev_distance(hero(&w)) <= 1 {
            reached = true;
            break;
        }
    }
    assert!(!steps.is_empty());
    for (from, to) in &steps {
        assert!(
            !(is_diagonal(*from, *to) && (*from == door_c || *to == door_c)),
            "diagonal step across the doorway: {from:?} -> {to:?}"
        );
    }
    assert!(
        steps.iter().any(|(_, to)| *to == door_c),
        "the jackal goes through the door: {steps:?}"
    );
    assert!(reached, "the jackal still reaches melee range: {steps:?}");
}

#[test]
fn monster_adjacent_diagonally_to_hero_in_doorway_still_attacks() {
    // Monster melee is not restricted (mfndpos only governs movement).
    let mut w = walled(10, DoorState::Open);
    w.arena.actors.get_mut(w.player_id).unwrap().coord = c(41, 10);
    let jackal = w
        .arena
        .spawn_actor(create_monster_record(MonsterSpeciesId::Jackal, c(40, 9)));
    let mut attacked = false;
    for _ in 0..12 {
        let events = w.step_player_action(ActionAst::Wait);
        if events.iter().any(|e| {
            matches!(
                e,
                GameEvent::AttackLanded { attacker, target, .. }
                    | GameEvent::AttackMissed { attacker, target }
                    if *attacker == jackal && *target == w.player_id
            )
        }) {
            attacked = true;
            break;
        }
    }
    assert!(
        attacked,
        "the diagonal monster must attack the hero in the doorway"
    );
    assert_eq!(w.arena.actors.get(jackal).unwrap().coord, c(40, 9));
}

#[test]
fn pet_following_hero_does_not_cut_door_diagonal() {
    let door_c = c(41, 10);
    let mut w = walled(11, DoorState::Open);
    w.arena.actors.get_mut(w.player_id).unwrap().coord = c(46, 10);
    let pet = w
        .arena
        .spawn_actor(create_monster_record(MonsterSpeciesId::Kitten, c(38, 7)));
    let mut steps = Vec::new();
    for _ in 0..60 {
        let events = w.step_player_action(ActionAst::Wait);
        steps.extend(moves_of(&events, pet));
    }
    for (from, to) in &steps {
        assert!(
            !(is_diagonal(*from, *to) && (*from == door_c || *to == door_c)),
            "pet cut the door diagonally: {from:?} -> {to:?}"
        );
    }
    assert!(
        steps.iter().any(|(_, to)| *to == door_c),
        "pet used the door"
    );
    let pc = w.arena.actors.get(pet).unwrap().coord;
    assert!(
        pc.x > 41,
        "the pet followed the hero through the door: {pc:?}"
    );
}

#[test]
fn peaceful_wander_never_diagonal_through_door() {
    // An open door tile in the middle of a small room: every neighbour is
    // adjacent to it, so a random walk meets it constantly.
    let door_c = c(41, 10);
    let mut w = arena(12);
    for x in 30..=50 {
        for y in 5..=15 {
            let inside = (39..=43).contains(&x) && (8..=12).contains(&y);
            w.level
                .set_tile(c(x, y), if inside { Tile::Room } else { Tile::Stone });
        }
    }
    w.level.set_tile(door_c, door(DoorState::Open));
    w.arena.actors.get_mut(w.player_id).unwrap().coord = c(43, 12);
    let mut p = create_monster_record(MonsterSpeciesId::Jackal, c(40, 10));
    p.is_peaceful = true;
    let id = w.arena.spawn_actor(p);
    let mut steps = Vec::new();
    for _ in 0..200 {
        let events = w.step_player_action(ActionAst::Wait);
        steps.extend(moves_of(&events, id));
    }
    assert!(steps.len() > 50, "the peaceful wanders: {}", steps.len());
    assert!(
        steps.iter().any(|(_, to)| *to == door_c),
        "the wander visits the door"
    );
    for (from, to) in &steps {
        assert!(
            !(is_diagonal(*from, *to) && (*from == door_c || *to == door_c)),
            "peaceful stepped diagonally at the door: {from:?} -> {to:?}"
        );
    }
}

#[test]
fn broken_door_lets_monster_step_diagonally() {
    for state in [DoorState::Broken, DoorState::NoDoor] {
        let door_c = c(41, 10);
        let mut w = walled(13, state);
        w.arena.actors.get_mut(w.player_id).unwrap().coord = c(43, 12);
        let jackal = w
            .arena
            .spawn_actor(create_monster_record(MonsterSpeciesId::Jackal, c(40, 9)));
        let mut first = None;
        for _ in 0..6 {
            let events = w.step_player_action(ActionAst::Wait);
            if let Some(m) = moves_of(&events, jackal).first() {
                first = Some(*m);
                break;
            }
        }
        assert_eq!(first, Some((c(40, 9), door_c)), "{state:?}");
    }
}
