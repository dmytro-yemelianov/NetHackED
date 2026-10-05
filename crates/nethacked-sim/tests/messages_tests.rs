//! `turn_messages`: the single event-to-text policy of both frontends.
//!
//! NetHack C reference:
//! - `lock.c:906` `pline_The("door opens.")`, `lock.c:1040` `pline_The("door closes.")`
//! - `win/tty/topl.c:251-303` `update_topl` (what the message window does with the text)

use nethacked_arena::ActorId;
use nethacked_core::ActionAst;
use nethacked_data::{create_monster_record, MonsterSpeciesId};
use nethacked_i18n::{t, Locale, Messages};
use nethacked_sim::{turn_messages, GameEvent, SimulationWorld};
use nethacked_types::{Coord, Direction, DoorState, Tile};

fn c(x: usize, y: usize) -> Coord {
    Coord::new_unchecked(x, y)
}

/// Only the hero at (40, 10) in an empty room (x 30..=50, y 5..=15).
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
    w.arena.actors.get_mut(w.player_id).unwrap().coord = c(40, 10);
    w
}

fn log(text: &str) -> GameEvent {
    GameEvent::LogMessage { text: text.into() }
}

fn log_texts(events: &[GameEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            GameEvent::LogMessage { text } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn turn_messages_skips_attack_event_paired_with_log_message() {
    let mut w = arena(11);
    let east = c(41, 10);
    let mut jackal = create_monster_record(MonsterSpeciesId::Jackal, east);
    jackal.ac = 30; // the hero always hits
    jackal.hp = 1000;
    jackal.max_hp = 1000;
    w.arena.spawn_actor(jackal);

    let events = w.step_player_action(ActionAst::MeleeAttack(east));
    let landed = events
        .iter()
        .position(|e| matches!(e, GameEvent::AttackLanded { .. }))
        .expect("a real melee step emits AttackLanded");
    assert!(
        matches!(events[landed + 1], GameEvent::LogMessage { .. }),
        "combat.rs pairs the attack event with its log line: {events:?}"
    );
    let out = turn_messages(&w, &events);
    assert_eq!(out, log_texts(&events), "only the engine's own lines");
    assert!(
        out.iter().all(|m| !m.starts_with("You hit the monster")),
        "no duplicate generic hit text: {out:?}"
    );
}

#[test]
fn turn_messages_falls_back_for_unpaired_attack_events() {
    let w = arena(12);
    let p = w.player_id;
    let hit = GameEvent::AttackLanded {
        attacker: p,
        target: p,
        damage: 7,
        lethal: false,
    };
    let kill = GameEvent::AttackLanded {
        attacker: p,
        target: p,
        damage: 3,
        lethal: true,
    };
    let miss = GameEvent::AttackMissed {
        attacker: p,
        target: p,
    };
    assert_eq!(
        turn_messages(&w, &[hit.clone()]),
        vec![Messages::tui_hit(7, false, Locale::En)]
    );
    assert_eq!(
        turn_messages(&w, &[kill]),
        vec![Messages::tui_hit(3, true, Locale::En)]
    );
    assert_eq!(
        turn_messages(&w, &[miss.clone()]),
        vec![t("tui.miss", Locale::En).to_string()]
    );
    // Followed by something that is not a LogMessage: still the fallback.
    assert_eq!(
        turn_messages(&w, &[hit.clone(), GameEvent::TurnAdvanced { turn: 2 }]),
        vec![Messages::tui_hit(7, false, Locale::En)]
    );
    // Followed by combat.rs's own pair line: paired, so only the log line.
    let hit_line = Messages::attack_hit("jackal", "hero", 7, Locale::En);
    assert_eq!(
        turn_messages(&w, &[hit, log(&hit_line)]),
        vec![hit_line.clone()]
    );
    let miss_line = Messages::attack_miss("jackal", "hero", Locale::En);
    assert_eq!(turn_messages(&w, &[miss, log(&miss_line)]), vec![miss_line]);
}

#[test]
fn turn_messages_door_open_close_and_broken() {
    let w = arena(13);
    let d = c(41, 10);
    let toggled = |s| GameEvent::DoorToggled {
        coord: d,
        new_state: s,
    };
    assert_eq!(
        turn_messages(&w, &[toggled(DoorState::Open)]),
        vec!["The door opens."]
    );
    assert_eq!(
        turn_messages(&w, &[toggled(DoorState::Closed)]),
        vec!["The door closes."]
    );
    assert!(turn_messages(&w, &[toggled(DoorState::Broken)]).is_empty());
    // Nothing in the sim pairs a log line with Open/Closed, so an unrelated
    // line after the toggle never hides it (lock.c:906, :1040 always print).
    assert_eq!(
        turn_messages(&w, &[toggled(DoorState::Open), log("It is stuck.")]),
        vec!["The door opens.", "It is stuck."]
    );
    assert_eq!(
        turn_messages(&w, &[toggled(DoorState::Broken), log("It shatters.")]),
        vec!["It shatters."]
    );

    // Real steps: open, close, kick.
    let mut w = arena(13);
    w.level.set_tile(
        d,
        Tile::Door {
            state: DoorState::Closed,
            trapped: false,
        },
    );
    let ev = w.step_player_action(ActionAst::OpenDoor(d));
    assert_eq!(turn_messages(&w, &ev), vec!["The door opens."]);
    let ev = w.step_player_action(ActionAst::CloseDoor(d));
    assert_eq!(turn_messages(&w, &ev), vec!["The door closes."]);
    let ev = w.step_player_action(ActionAst::Kick(d));
    assert_eq!(
        turn_messages(&w, &ev),
        log_texts(&ev),
        "kick pairs its own text; Broken adds nothing"
    );
    assert_eq!(turn_messages(&w, &ev).len(), 1);
}

#[test]
fn turn_messages_ignores_level_changed_and_movement() {
    let mut w = arena(14);
    let p = w.player_id;
    let events = vec![
        GameEvent::ActorMoved {
            actor: p,
            from: c(40, 10),
            to: c(41, 10),
        },
        GameEvent::LevelChanged {
            from_depth: 1,
            to_depth: 2,
        },
        GameEvent::TurnAdvanced { turn: 3 },
        GameEvent::BeamPropagated { path: vec![] },
        GameEvent::Victory,
    ];
    assert!(turn_messages(&w, &events).is_empty());
    assert!(turn_messages(&w, &[]).is_empty());
    let ev = w.step_player_action(ActionAst::Move(Direction::East));
    assert!(ev.iter().any(|e| matches!(e, GameEvent::ActorMoved { .. })));
    assert_eq!(turn_messages(&w, &ev), log_texts(&ev));
}

#[test]
fn turn_messages_uk_locale_uses_uk_door_text() {
    let mut w = arena(15);
    w.set_locale(Locale::Uk);
    let toggled = |s| GameEvent::DoorToggled {
        coord: c(41, 10),
        new_state: s,
    };
    assert_eq!(
        turn_messages(&w, &[toggled(DoorState::Open)]),
        vec!["Двері відчиняються."]
    );
    assert_eq!(
        turn_messages(&w, &[toggled(DoorState::Closed)]),
        vec!["Двері зачиняються."]
    );
    let p = w.player_id;
    assert_eq!(
        turn_messages(
            &w,
            &[GameEvent::AttackMissed {
                attacker: p,
                target: p
            }]
        ),
        vec![t("tui.miss", Locale::Uk).to_string()]
    );
}

#[test]
fn turn_messages_is_deterministic_for_same_seed() {
    let run = |seed: u64| -> Vec<Vec<String>> {
        let mut w = SimulationWorld::new_with_seed(seed);
        let dirs = [
            Direction::East,
            Direction::South,
            Direction::West,
            Direction::North,
        ];
        let mut out = Vec::new();
        for i in 0..60 {
            let ev = w.step_player_action(ActionAst::Move(dirs[i % 4]));
            let a = turn_messages(&w, &ev);
            assert_eq!(a, turn_messages(&w, &ev), "pure function of its input");
            out.push(a);
        }
        out
    };
    assert_eq!(run(21), run(21));
}

/// Regression: a log line from the monster phase right after the toggle must
/// not swallow "The door opens." / "The door closes." (lock.c:906, :1040).
#[test]
fn turn_messages_door_toggle_survives_unrelated_log_line() {
    let w = arena(21);
    let toggled = |s| GameEvent::DoorToggled {
        coord: c(41, 10),
        new_state: s,
    };
    assert_eq!(
        turn_messages(
            &w,
            &[
                toggled(DoorState::Open),
                log("The floating eye's gaze paralyzes you!")
            ]
        ),
        vec!["The door opens.", "The floating eye's gaze paralyzes you!"]
    );
    assert_eq!(
        turn_messages(
            &w,
            &[toggled(DoorState::Closed), log("The kitten whimpers.")]
        ),
        vec!["The door closes.", "The kitten whimpers."]
    );
}

/// Real step: opening a door while an Elbereth-scared goblin is adjacent;
/// `step_monsters` adds its own line after the toggle (a `TurnAdvanced` may sit
/// between them, so this pins the end-to-end text, not the adjacency).
#[test]
fn real_step_door_open_then_monster_line_shows_both() {
    use nethacked_core::EngravingMedium;
    let d = c(41, 10);
    let mut w = arena(22);
    w.step_player_action(ActionAst::Engrave {
        text: "Elbereth".into(),
        medium: EngravingMedium::Burned,
    });
    w.level.set_tile(
        d,
        Tile::Door {
            state: DoorState::Closed,
            trapped: false,
        },
    );
    w.arena
        .spawn_actor(create_monster_record(MonsterSpeciesId::Goblin, c(39, 10)));
    let ev = w.step_player_action(ActionAst::OpenDoor(d));
    assert!(
        ev.iter()
            .any(|e| matches!(e, GameEvent::DoorToggled { .. })),
        "door toggled: {ev:?}"
    );
    let out = turn_messages(&w, &ev);
    assert_eq!(out[0], "The door opens.", "{out:?}");
    assert!(out.iter().any(|m| m.contains("Elbereth")), "{out:?}");
}

/// Regression: an `AttackLanded` followed by an unrelated line (the ranged
/// "projectile breaks" text, a spell kill line) keeps its generic hit text;
/// only the combat.rs pair (`Messages::attack_hit` / `attack_miss`) is paired.
#[test]
fn turn_messages_attack_pairs_only_with_its_own_exchange_line() {
    let w = arena(23);
    let p = w.player_id;
    let hit = GameEvent::AttackLanded {
        attacker: p,
        target: p,
        damage: 2,
        lethal: false,
    };
    let miss = GameEvent::AttackMissed {
        attacker: p,
        target: p,
    };
    let breaks = Messages::projectile_breaks(Locale::En);
    assert_eq!(
        turn_messages(&w, &[hit.clone(), log(breaks)]),
        vec![Messages::tui_hit(2, false, Locale::En), breaks.to_string()]
    );
    assert_eq!(
        turn_messages(&w, &[miss.clone(), log("The goblin yelps.")]),
        vec![
            t("tui.miss", Locale::En).to_string(),
            "The goblin yelps.".to_string()
        ]
    );
    // The real pair text (any names) still suppresses the generic text.
    let pair = Messages::attack_hit("jackal", "hero", 2, Locale::En);
    assert_eq!(turn_messages(&w, &[hit.clone(), log(&pair)]), vec![pair]);
    let pair = Messages::attack_miss("jackal", "hero", Locale::En);
    assert_eq!(turn_messages(&w, &[miss.clone(), log(&pair)]), vec![pair]);
    // Same verb, other damage: not this exchange.
    let other = Messages::attack_hit("jackal", "hero", 9, Locale::En);
    assert_eq!(
        turn_messages(&w, &[hit, log(&other)]),
        vec![Messages::tui_hit(2, false, Locale::En), other]
    );
    // Ukrainian pair text.
    let mut wu = arena(24);
    wu.set_locale(Locale::Uk);
    let hit = GameEvent::AttackLanded {
        attacker: p,
        target: p,
        damage: 4,
        lethal: false,
    };
    let pair = Messages::attack_hit("jackal", "hero", 4, Locale::Uk);
    assert_eq!(turn_messages(&wu, &[hit, log(&pair)]), vec![pair]);
}
