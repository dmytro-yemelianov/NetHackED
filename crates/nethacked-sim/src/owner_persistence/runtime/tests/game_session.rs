//! M6-I10 facade tests: `OwnerGameSession` legacy-oracle parity across a
//! multi-transport script, exhaustive `ActionAst` dispatch smoke, dead-hero
//! no-op, refusal atomicity, reload continuation, locale and reset re-import.

use super::super::*;
use crate::{ActionAst, GameEvent};
use nethacked_arena::ItemLocation;
use nethacked_core::engraving::EngravingMedium;
use nethacked_types::{
    Buc, Coord, Direction, DoorState, Locale, Tile, TrapRecord, TrapState, TrapType, WaterType,
};
use rand::SeedableRng;

fn clean_world(seed: u64) -> SimulationWorld {
    let mut w = SimulationWorld::new_with_seed(seed);
    w.arena.actors.retain(|id, _| id == w.player_id);
    w.arena.items = Default::default();
    w.equipment = Default::default();
    w.wielded_item = None;
    w.hero.quivered_item = None;
    w.unpaid_items.clear();
    w.hero.skills.skills.clear();
    w.level = nethacked_dungeon::DungeonLevel::new_solid(Tile::Room);
    w.arena.actors[w.player_id].coord = Coord::new_unchecked(10, 10);
    w.arena.actors[w.player_id].level = 10;
    w.arena.actors[w.player_id].hp = 50;
    w.arena.actors[w.player_id].max_hp = 50;
    w.arena.actors[w.player_id].ac = 10;
    w.depth = 5;
    w.scheduler.turn = 100;
    w.scheduler.hero_energy = 12;
    w.scheduler.hero_speed = 12;
    w.rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
    w
}

fn oracle_json(w: &SimulationWorld) -> serde_json::Value {
    let mut v: serde_json::Value = serde_json::from_str(&w.to_save_json().unwrap()).unwrap();
    if let Some(obj) = v.as_object_mut() {
        obj.insert("event_log".into(), serde_json::Value::Array(vec![]));
    }
    v
}

/// Step one action through the facade and the legacy dispatcher and require
/// bit-for-bit agreement: returned events, cumulative event log and full
/// native state (RNG stream included).
fn facade_oracle_step(
    session: &mut OwnerGameSession,
    legacy: &mut SimulationWorld,
    ast: ActionAst,
) {
    let events = session.step(ast.clone()).unwrap();
    let legacy_events = legacy.step_player_action(ast);
    assert_eq!(events, legacy_events, "event stream diverged from legacy");
    assert_eq!(
        session.world().event_log,
        legacy.event_log,
        "cumulative event log diverged from legacy"
    );
    assert_eq!(
        oracle_json(session.world()),
        oracle_json(legacy),
        "post-action world state diverged from legacy"
    );
}

#[test]
fn facade_oracle_parity_script_across_transports() {
    let mut w = clean_world(804);
    // A closed door east of the hero's path, a revealed pit trap west of the
    // start, and a dagger on the hero's first step tile.
    let door = Coord::new_unchecked(11, 10);
    w.level.set_tile(
        door,
        Tile::Door {
            state: DoorState::Closed,
            trapped: false,
        },
    );
    let trap_coord = Coord::new_unchecked(9, 10);
    w.level.traps.insert(
        trap_coord,
        TrapRecord {
            id: 0,
            trap_type: TrapType::Pit,
            state: TrapState::Revealed,
            coord: trap_coord,
        },
    );
    let dagger = w
        .ruleset
        .create_item_record(
            "dagger",
            ItemLocation::Floor(Coord::new_unchecked(10, 9)),
            Buc::Uncursed,
        )
        .unwrap();
    w.arena.spawn_item(dagger);

    let mut session = OwnerGameSession::new(w.clone()).unwrap();
    let mut legacy = w;

    // One script spanning the I1 frame, I4 Wait, I5 movement/doors/traps,
    // I6 inventory and I8 services transports.
    let script: Vec<ActionAst> = vec![
        ActionAst::Wait,
        ActionAst::Move(Direction::North), // onto the dagger tile (10, 9)
        ActionAst::OpenDoor(door),
        ActionAst::CloseDoor(door),
        ActionAst::Kick(door),                        // shatters the door
        ActionAst::Kick(door),                        // already broken: silent no-op
        ActionAst::Kick(Coord::new_unchecked(10, 8)), // wall
        ActionAst::MeleeAttack(Coord::new_unchecked(10, 8)), // empty air
        ActionAst::Search,
        ActionAst::Untrap(trap_coord), // disarms the revealed pit
        ActionAst::PickUp,             // picks up the dagger
        ActionAst::Wield(0),
        ActionAst::Drop(0),
        ActionAst::CancelZapSelection,
        ActionAst::Pray,
    ];
    for ast in script {
        facade_oracle_step(&mut session, &mut legacy, ast);
    }
}

#[test]
fn facade_dispatches_every_action_ast_variant() {
    // Every ActionAst branch reaches owner execution: each variant either
    // executes or refuses truthfully (an Err), never panics, and the session
    // stays coherent afterwards.
    let w = SimulationWorld::new_with_seed(42);
    let quiver_id = {
        let mut w = w.clone();
        let dagger = w
            .ruleset
            .create_item_record(
                "dagger",
                ItemLocation::CarriedBy(w.player_id),
                Buc::Uncursed,
            )
            .unwrap();
        w.arena.spawn_item(dagger)
    };
    let mount_id = w.player_id;
    let variants: Vec<ActionAst> = vec![
        ActionAst::Move(Direction::North),
        ActionAst::MeleeAttack(Coord::new_unchecked(10, 9)),
        ActionAst::OpenDoor(Coord::new_unchecked(10, 9)),
        ActionAst::CloseDoor(Coord::new_unchecked(10, 9)),
        ActionAst::Kick(Coord::new_unchecked(10, 9)),
        ActionAst::PickUp,
        ActionAst::Drop(0),
        ActionAst::Wield(0),
        ActionAst::Wear(0),
        ActionAst::Remove(0),
        ActionAst::Dip {
            item_index: 0,
            into_water: WaterType::Plain,
        },
        ActionAst::PutInContainer {
            item_index: 0,
            container_index: 0,
        },
        ActionAst::TakeFromContainer {
            container_index: 0,
            item_index: 0,
        },
        ActionAst::Quaff(0),
        ActionAst::Read(0),
        ActionAst::Engrave {
            text: "Elbereth".into(),
            medium: EngravingMedium::Dust(1),
        },
        ActionAst::ZapWand {
            dir: Direction::East,
            energy: 6,
        },
        ActionAst::ZapSelectedWand {
            item_index: 0,
            expected_item: None,
            dir: Direction::East,
            energy: 6,
        },
        ActionAst::CancelZapSelection,
        ActionAst::Wait,
        ActionAst::Pray,
        ActionAst::Pay,
        ActionAst::Sacrifice(0),
        ActionAst::Ascend,
        ActionAst::Descend,
        ActionAst::Eat(0),
        ActionAst::Cast {
            spell_index: 0,
            dir: Direction::North,
        },
        ActionAst::Wish("Excalibur".into()),
        ActionAst::Rub(0),
        ActionAst::PriceCheck(0),
        ActionAst::Donate(0),
        ActionAst::Apply(0),
        ActionAst::Quiver(quiver_id),
        ActionAst::Fire(Direction::North),
        ActionAst::Mount(mount_id),
        ActionAst::Dismount,
        ActionAst::Search,
        ActionAst::Untrap(Coord::new_unchecked(10, 9)),
    ];
    assert_eq!(variants.len(), 38, "ActionAst variant count drifted");

    let mut session = OwnerGameSession::new(w).unwrap();
    for ast in variants {
        // Ok or a truthful Err; a panic here fails the test.
        let _ = session.step(ast);
    }
    // The session is still coherent and dispatches afterwards.
    let events = session.step(ActionAst::Wait).unwrap();
    assert!(session
        .world()
        .arena
        .actors
        .contains_key(session.world().player_id));
    let _ = events;
}

#[test]
fn facade_dead_hero_is_exact_noop() {
    let mut w = clean_world(804);
    w.arena.actors[w.player_id].hp = 0;
    w.arena.actors[w.player_id].is_dead = true;
    let mut session = OwnerGameSession::new(w.clone()).unwrap();
    let mut legacy = w;

    let events = session.step(ActionAst::Wait).unwrap();
    let legacy_events = legacy.step_player_action(ActionAst::Wait);
    assert!(events.is_empty());
    assert_eq!(events, legacy_events);
    assert_eq!(session.world().event_log, legacy.event_log);
    assert_eq!(oracle_json(session.world()), oracle_json(&legacy));
}

#[test]
fn facade_refused_action_leaves_state_untouched() {
    // The I1 frame refuses a cancellation while the hero is helpless (the
    // accepted helpless-substitution boundary); the refusal must publish
    // nothing and leave the session state bit-for-bit untouched.
    let mut w = clean_world(804);
    w.hero.afflictions.transient.helpless = 2;
    let mut session = OwnerGameSession::new(w).unwrap();
    let before = oracle_json(session.world());
    let before_log = session.world().event_log.clone();

    let result = session.step(ActionAst::CancelZapSelection);
    assert!(result.is_err(), "helpless cancellation must refuse");
    assert_eq!(oracle_json(session.world()), before);
    assert_eq!(session.world().event_log, before_log);
}

#[test]
fn facade_reload_continuation_matches_uninterrupted() {
    let w = clean_world(804);
    let mut session = OwnerGameSession::new(w.clone()).unwrap();
    let mut legacy = w;

    for ast in [
        ActionAst::Wait,
        ActionAst::Move(Direction::North),
        ActionAst::Search,
    ] {
        facade_oracle_step(&mut session, &mut legacy, ast);
    }

    // Re-import the facade's current native world into a fresh owner runtime
    // (the reload path) and continue; it must match the uninterrupted facade
    // and the legacy oracle exactly.
    let mut reloaded = OwnerGameSession::new(session.world().clone()).unwrap();
    for ast in [
        ActionAst::Wait,
        ActionAst::Move(Direction::East),
        ActionAst::Kick(Coord::new_unchecked(11, 10)),
        ActionAst::Wait,
    ] {
        let reloaded_events = reloaded.step(ast.clone()).unwrap();
        let session_events = session.step(ast.clone()).unwrap();
        let legacy_events = legacy.step_player_action(ast);
        assert_eq!(reloaded_events, session_events);
        assert_eq!(reloaded_events, legacy_events);
        assert_eq!(oracle_json(reloaded.world()), oracle_json(session.world()));
        assert_eq!(reloaded.world().event_log, session.world().event_log);
        assert_eq!(reloaded.world().event_log, legacy.event_log);
    }
}

#[test]
fn facade_set_locale_reimports_and_keeps_dispatching() {
    let mut session = OwnerGameSession::new(clean_world(804)).unwrap();
    session.set_locale(Locale::Uk).unwrap();
    assert_eq!(session.world().get_locale(), Locale::Uk);
    assert_eq!(session.world().locale, Locale::Uk);

    // Gameplay still dispatches through owners after the re-import.
    let mut legacy = clean_world(804);
    legacy.set_locale(Locale::Uk);
    facade_oracle_step(&mut session, &mut legacy, ActionAst::Wait);
    facade_oracle_step(&mut session, &mut legacy, ActionAst::Move(Direction::North));
}

#[test]
fn facade_reset_reimports_new_game() {
    let mut session = OwnerGameSession::new(clean_world(804)).unwrap();
    session.step(ActionAst::Wait).unwrap();

    session.reset(SimulationWorld::new_with_seed(7)).unwrap();
    assert_eq!(session.world().scheduler.turn, 1);

    let mut legacy = SimulationWorld::new_with_seed(7);
    facade_oracle_step(&mut session, &mut legacy, ActionAst::Wait);
    facade_oracle_step(&mut session, &mut legacy, ActionAst::Move(Direction::East));
}

#[test]
fn facade_step_returns_reply_events_and_carries_log() {
    // The facade returns exactly the reply's events and appends them to the
    // cumulative log under the world's cap, like legacy `record_events`.
    let mut session = OwnerGameSession::new(clean_world(804)).unwrap();
    let mut legacy = clean_world(804);

    let events = session
        .step(ActionAst::Kick(Coord::new_unchecked(10, 9)))
        .unwrap();
    let legacy_events = legacy.step_player_action(ActionAst::Kick(Coord::new_unchecked(10, 9)));
    assert_eq!(events, legacy_events);
    assert!(!events.is_empty(), "kicking a wall logs a message");
    assert_eq!(session.world().event_log, events);
    assert_eq!(session.world().event_log, legacy.event_log);
    assert!(session
        .world()
        .event_log
        .iter()
        .any(|e| matches!(e, GameEvent::LogMessage { text } if text.contains("kick the wall"))));
}
