use super::super::*;
use crate::owner_persistence::runtime::movement_actions::{self, MovementAction};
use crate::{ActionAst, GameEvent};
use nethacked_arena::ItemLocation;
use nethacked_core::inventory::EncumbranceTier;
use nethacked_types::{Buc, Coord, Direction, DoorState, Tile, TrapRecord, TrapState, TrapType};
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

fn config(w: &SimulationWorld) -> OwnerBridgeConfig {
    OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
        .unwrap()
        .with_movement_actions()
}

fn install_bridge(w: &SimulationWorld) -> OwnerBridge {
    OwnerBridge::install(NativePartitions::from_native(w.clone()), config(w)).unwrap()
}

#[test]
fn movement_manifest_bounds_and_profiles() {
    for movement in [false, true] {
        let w = SimulationWorld::new_with_seed(721);
        let mut c = OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
            .unwrap()
            .with_timer_ticks();
        if movement {
            c = c.with_movement_actions();
        }
        assert_eq!(profile_caller(&c).dependencies.len(), 64);
        assert_eq!(profile_provider(&c).exports.len(), 64);
        OwnerBridge::install(NativePartitions::from_native(w), c).unwrap();
    }

    // Microkernel transport manifests fit well within 64-port bounds
    assert!(movement_actions::READS.len() + movement_actions::WRITES.len() + 2 <= 64);
    assert_eq!(
        movement_actions::READS.len() + movement_actions::WRITES.len() + 2,
        18
    );
}

#[test]
fn movement_authentic_stale_refused() {
    let w = clean_world(804);
    let bridge = install_bridge(&w);
    let mut req = bridge
        .prepare_movement(MovementAction::Move(Direction::North))
        .unwrap();

    // Tampering with witness stamp triggers authentic stale refusal check
    req.guards[0].generation = req.guards[0].generation.wrapping_add(1);
    let err = bridge.execute_movement(req);
    assert!(matches!(
        err,
        Err(OwnerBridgeError::Owner(owners::Error::Stale))
    ));
    assert!(movement_actions::stale_refusals() >= 1);
}

#[test]
fn movement_wrong_authority_refused() {
    let w = clean_world(804);
    let bridge = install_bridge(&w);
    let req = bridge
        .prepare_movement(MovementAction::Move(Direction::North))
        .unwrap();

    movement_actions::trigger_wrong_resource(true);
    let res = bridge.execute_movement(req.clone());
    movement_actions::trigger_wrong_resource(false);

    assert!(res.is_err());

    // State remains uncorrupted and request can still be executed successfully with valid authority
    let reply = bridge.execute_movement(req).unwrap();
    assert!(reply.spent_time);
}

#[test]
fn movement_zero_rounds_when_hero_dead() {
    let mut w = clean_world(804);
    w.arena.actors[w.player_id].is_dead = true;
    w.arena.actors[w.player_id].hp = 0;
    let bridge = install_bridge(&w);

    let reply = bridge
        .step_movement(MovementAction::Move(Direction::North))
        .unwrap();
    assert!(!reply.spent_time);
    assert_eq!(reply.rounds, 0);
    assert!(reply.events.is_empty());
}

#[test]
fn movement_move_into_open_floor() {
    let w = clean_world(804);
    let bridge = install_bridge(&w);

    let reply = bridge
        .step_movement(MovementAction::Move(Direction::North))
        .unwrap();
    assert!(reply.spent_time);
    assert_eq!(reply.rounds, 1);

    // Hero moved from (10,10) to (10,9)
    let parts = bridge.partitions().unwrap();
    assert_eq!(
        parts.actors.actors[w.player_id].coord,
        Coord::new_unchecked(10, 9)
    );

    // ActorMoved event was emitted
    let has_moved = reply.events.iter().any(|e| matches!(
        e,
        GameEvent::ActorMoved { actor, from, to }
            if *actor == w.player_id && *from == Coord::new_unchecked(10, 10) && *to == Coord::new_unchecked(10, 9)
    ));
    assert!(has_moved);
}

#[test]
fn movement_move_into_wall_bumps() {
    let mut w = clean_world(804);
    // Wall off the tile north of the hero (level stays within the 16 KiB
    // observation bound; a solid-wall level would not fit).
    w.level.set_tile(
        Coord::new_unchecked(10, 9),
        Tile::Wall { horizontal: false },
    );
    let bridge = install_bridge(&w);

    let reply = bridge
        .step_movement(MovementAction::Move(Direction::North))
        .unwrap();
    // Legacy handle_move: bumping an impassable obstacle does not consume
    // energy (the `_ =>` arm never calls hero_act).
    assert!(!reply.spent_time);
    assert_eq!(reply.rounds, 0);

    // Hero did not move
    let parts = bridge.partitions().unwrap();
    assert_eq!(
        parts.actors.actors[w.player_id].coord,
        Coord::new_unchecked(10, 10)
    );

    // Bump wall message was emitted
    let has_bump = reply.events.iter().any(|e| {
        matches!(
            e,
            GameEvent::LogMessage { text } if text.contains("wall") || text.contains("boulder")
        )
    });
    assert!(has_bump, "Expected bump message, got: {:?}", reply.events);
}

#[test]
fn movement_move_into_closed_door_opens() {
    let mut w = clean_world(804);
    // Place a closed door north of hero
    w.level.set_tile(
        Coord::new_unchecked(10, 9),
        Tile::Door {
            state: DoorState::Closed,
            trapped: false,
        },
    );
    let bridge = install_bridge(&w);

    let reply = bridge
        .step_movement(MovementAction::Move(Direction::North))
        .unwrap();
    assert!(reply.spent_time);

    // Door is now open
    let parts = bridge.partitions().unwrap();
    assert_eq!(
        parts.geography.level.get_tile(Coord::new_unchecked(10, 9)),
        &Tile::Door {
            state: DoorState::Open,
            trapped: false,
        }
    );

    // DoorToggled event was emitted
    let has_door = reply.events.iter().any(|e| {
        matches!(
            e,
            GameEvent::DoorToggled { coord, new_state }
                if *coord == Coord::new_unchecked(10, 9) && *new_state == DoorState::Open
        )
    });
    assert!(has_door);
}

#[test]
fn movement_open_door_action() {
    let mut w = clean_world(804);
    // Place a closed door adjacent to hero
    w.level.set_tile(
        Coord::new_unchecked(11, 10),
        Tile::Door {
            state: DoorState::Closed,
            trapped: false,
        },
    );
    let bridge = install_bridge(&w);

    let reply = bridge
        .step_movement(MovementAction::OpenDoor(Coord::new_unchecked(11, 10)))
        .unwrap();
    assert!(reply.spent_time);

    // Door is now open
    let parts = bridge.partitions().unwrap();
    assert_eq!(
        parts.geography.level.get_tile(Coord::new_unchecked(11, 10)),
        &Tile::Door {
            state: DoorState::Open,
            trapped: false,
        }
    );
}

#[test]
fn movement_close_door_action() {
    let mut w = clean_world(804);
    // Place an open door adjacent to hero
    w.level.set_tile(
        Coord::new_unchecked(11, 10),
        Tile::Door {
            state: DoorState::Open,
            trapped: false,
        },
    );
    let bridge = install_bridge(&w);

    let reply = bridge
        .step_movement(MovementAction::CloseDoor(Coord::new_unchecked(11, 10)))
        .unwrap();
    assert!(reply.spent_time);

    // Door is now closed
    let parts = bridge.partitions().unwrap();
    assert_eq!(
        parts.geography.level.get_tile(Coord::new_unchecked(11, 10)),
        &Tile::Door {
            state: DoorState::Closed,
            trapped: false,
        }
    );
}

#[test]
fn movement_search_finds_trap() {
    let mut w = clean_world(804);
    // Place a hidden trap adjacent to hero
    let trap_coord = Coord::new_unchecked(11, 10);
    w.level.traps.insert(
        trap_coord,
        TrapRecord {
            id: 0,
            trap_type: TrapType::Pit,
            state: TrapState::Hidden,
            coord: trap_coord,
        },
    );
    let bridge = install_bridge(&w);

    let reply = bridge.step_movement(MovementAction::Search).unwrap();
    assert!(reply.spent_time);

    // Trap is now revealed
    let parts = bridge.partitions().unwrap();
    assert_eq!(
        parts.geography.level.traps[&trap_coord].state,
        TrapState::Revealed
    );
}

#[test]
fn movement_untrap_disarms() {
    let mut w = clean_world(804);
    // Place a revealed trap adjacent to hero
    let trap_coord = Coord::new_unchecked(11, 10);
    w.level.traps.insert(
        trap_coord,
        TrapRecord {
            id: 0,
            trap_type: TrapType::Pit,
            state: TrapState::Revealed,
            coord: trap_coord,
        },
    );
    let bridge = install_bridge(&w);

    let reply = bridge
        .step_movement(MovementAction::Untrap(trap_coord))
        .unwrap();
    assert!(reply.spent_time);

    // Trap is now disarmed (removed or state changed)
    let parts = bridge.partitions().unwrap();
    let trap = parts.geography.level.traps.get(&trap_coord);
    assert!(
        trap.is_none() || trap.is_some_and(|t| t.state == TrapState::Disarmed),
        "Trap should be disarmed or removed"
    );
}

#[test]
fn movement_combat_bump_attacks_monster() {
    let mut w = clean_world(804);
    // Spawn a hostile goblin adjacent to hero at (11, 10)
    let mut goblin = w
        .ruleset
        .create_monster_record("goblin", Coord::new_unchecked(11, 10), &mut w.rng)
        .unwrap();
    goblin.movement = 0; // Don't let it act during turn ticks
    let goblin_id = w.arena.spawn_actor(goblin);

    let bridge = install_bridge(&w);
    let reply = bridge
        .step_movement(MovementAction::Move(Direction::East))
        .unwrap();

    assert!(reply.spent_time);

    // Hero should have attacked the goblin (hit or miss)
    let combat_happened = reply.events.iter().any(|e| match e {
        GameEvent::AttackLanded {
            attacker, target, ..
        } => *attacker == w.player_id && *target == goblin_id,
        GameEvent::AttackMissed { attacker, target } => {
            *attacker == w.player_id && *target == goblin_id
        }
        _ => false,
    });
    assert!(combat_happened, "Hero bump should trigger combat");
}

#[test]
fn movement_reload_and_continuation() {
    let w = clean_world(804);
    let bridge = install_bridge(&w);

    let reply1 = bridge
        .step_movement(MovementAction::Move(Direction::North))
        .unwrap();
    assert_eq!(reply1.rounds, 1);

    // Save
    let wire = bridge.capture().unwrap();

    // Reconstruct into fresh bridge
    let reloaded = OwnerBridge::reconstruct(&wire, config(&w)).unwrap();

    // Continue movement on reloaded bridge
    let reply2 = reloaded
        .step_movement(MovementAction::Move(Direction::North))
        .unwrap();
    assert_eq!(reply2.rounds, 1);

    let parts = reloaded.partitions().unwrap();
    assert_eq!(parts.clock.scheduler.turn, 102);
    // Hero moved twice: (10,10) -> (10,9) -> (10,8)
    assert_eq!(
        parts.actors.actors[w.player_id].coord,
        Coord::new_unchecked(10, 8)
    );
}

// ---------------------------------------------------------------------------
// Legacy-oracle parity: the bridge must reproduce the native
// `step_player_action` bit-for-bit — event stream, full world state and RNG
// stream. The one documented exception is the session event log: legacy
// `record_events` appends to it, while the movement transport keeps Session
// read-only, so the field is normalised before comparing save snapshots.
// ---------------------------------------------------------------------------

fn oracle_json(w: &SimulationWorld) -> serde_json::Value {
    let mut v: serde_json::Value = serde_json::from_str(&w.to_save_json().unwrap()).unwrap();
    if let Some(obj) = v.as_object_mut() {
        obj.insert("event_log".into(), serde_json::Value::Array(vec![]));
    }
    v
}

fn run_movement_oracle(
    w: &SimulationWorld,
    ast: ActionAst,
    action: MovementAction,
) -> (movement_actions::MovementReply, SimulationWorld) {
    let mut legacy = w.clone();
    let bridge = install_bridge(w);
    let events = legacy.step_player_action(ast);
    let reply = bridge.step_movement(action).unwrap();
    assert_eq!(reply.events, events, "event stream diverged from legacy");
    let after = bridge.export_native().unwrap();
    assert_eq!(
        oracle_json(&after),
        oracle_json(&legacy),
        "post-action world state diverged from legacy"
    );
    (reply, legacy)
}

#[test]
fn movement_oracle_overload_refusal_matches_legacy() {
    let mut w = clean_world(804);
    // A carried boulder (6000 weight) overloads any hero. Confusion must not
    // draw a direction: the overload guard precedes the impaired-direction
    // draw (hack.c:2730,2747), so the move is refused in place.
    w.hero.afflictions.transient.confused = 2;
    let boulder = w
        .ruleset
        .create_item_record(
            "boulder",
            ItemLocation::CarriedBy(w.player_id),
            Buc::Uncursed,
        )
        .unwrap();
    w.arena.spawn_item(boulder);
    assert_eq!(w.hero_encumbrance(), EncumbranceTier::Overloaded);

    let (reply, legacy) = run_movement_oracle(
        &w,
        ActionAst::Move(Direction::North),
        MovementAction::Move(Direction::North),
    );
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("collapse under your load")
    )));
    // Refused in place: no ActorMoved, hero coord unchanged.
    assert!(!reply.events.iter().any(|e| matches!(
        e,
        GameEvent::ActorMoved { actor, .. } if *actor == w.player_id
    )));
    assert_eq!(
        legacy.arena.actors[w.player_id].coord,
        Coord::new_unchecked(10, 10)
    );
}

#[test]
fn movement_oracle_stunned_random_direction_matches_legacy() {
    let mut w = clean_world(804);
    // Stunned always moves at random: exactly one direction draw, and the
    // confused 1/5 draw is short-circuited.
    w.hero.afflictions.transient.stunned = 2;
    let (reply, legacy) = run_movement_oracle(
        &w,
        ActionAst::Move(Direction::North),
        MovementAction::Move(Direction::North),
    );
    assert!(reply.spent_time);
    assert_ne!(
        legacy.arena.actors[w.player_id].coord,
        Coord::new_unchecked(10, 10)
    );
}

#[test]
fn movement_oracle_confused_direction_draw_matches_legacy() {
    let mut w = clean_world(804);
    // Confused draws the 1/5 impairment check first; on a miss the requested
    // direction is kept, on a hit a random direction is drawn.
    w.hero.afflictions.transient.confused = 2;
    let (reply, legacy) = run_movement_oracle(
        &w,
        ActionAst::Move(Direction::North),
        MovementAction::Move(Direction::North),
    );
    assert!(reply.spent_time);
    assert_ne!(
        legacy.arena.actors[w.player_id].coord,
        Coord::new_unchecked(10, 10)
    );
}

#[test]
fn movement_oracle_helpless_confused_waits_matches_legacy() {
    let mut w = clean_world(804);
    // Helpless substitutes a stationary wait before any impaired-direction
    // draw, so even a confused hero never draws a direction.
    w.hero.afflictions.transient.helpless = 2;
    w.hero.afflictions.transient.confused = 2;
    let (reply, legacy) = run_movement_oracle(
        &w,
        ActionAst::Move(Direction::North),
        MovementAction::Move(Direction::North),
    );
    assert!(reply.spent_time);
    assert!(!reply.events.iter().any(|e| matches!(
        e,
        GameEvent::ActorMoved { actor, .. } if *actor == w.player_id
    )));
    assert_eq!(
        legacy.arena.actors[w.player_id].coord,
        Coord::new_unchecked(10, 10)
    );
}

#[test]
fn movement_oracle_trap_trigger_matches_legacy() {
    let mut w = clean_world(804);
    // A hidden arrow trap always fires on entry (only a revealed trap can be
    // escaped with the rn2(5) draw); the draw itself still happens.
    let trap_coord = Coord::new_unchecked(10, 9);
    w.level.traps.insert(
        trap_coord,
        TrapRecord {
            id: 0,
            trap_type: TrapType::Arrow,
            state: TrapState::Hidden,
            coord: trap_coord,
        },
    );
    let (reply, legacy) = run_movement_oracle(
        &w,
        ActionAst::Move(Direction::North),
        MovementAction::Move(Direction::North),
    );
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("shoots")
    )));
    // Hero stepped onto the trap tile and took the 2-point hit (natural regen
    // may add at most one point back during the turn ticks).
    assert_eq!(legacy.arena.actors[w.player_id].coord, trap_coord);
    let hp = legacy.arena.actors[w.player_id].hp;
    assert!((48..=49).contains(&hp), "unexpected trap hp: {hp}");
    assert_eq!(legacy.level.traps[&trap_coord].state, TrapState::Revealed);
}

#[test]
fn movement_oracle_moved_flag_gates_regen_matches_legacy() {
    let mut w = clean_world(804);
    // Stressed (not overloaded) burden: cap = 25*(3+3)+50 = 200, a 350-weight
    // helmet puts the hero at tier 2. A successful move sets the moved flag,
    // which suppresses the natural-regen draw (turns.rs:175-178); a refused
    // move would let the draw through.
    w.progress.attr_base[crate::progress::A_STR] = 3;
    w.progress.attr_base[crate::progress::A_CON] = 3;
    w.arena.actors[w.player_id].hp = 40;
    let mut heavy = w
        .ruleset
        .create_item_record(
            "helmet",
            ItemLocation::CarriedBy(w.player_id),
            Buc::Uncursed,
        )
        .unwrap();
    heavy.weight = 350;
    w.arena.spawn_item(heavy);
    assert_eq!(w.hero_encumbrance(), EncumbranceTier::Stressed);

    let (reply, legacy) = run_movement_oracle(
        &w,
        ActionAst::Move(Direction::North),
        MovementAction::Move(Direction::North),
    );
    assert!(reply.spent_time);
    // The moved flag gated natural regen: no heal despite hp < max_hp.
    assert_eq!(legacy.arena.actors[w.player_id].hp, 40);
}

#[test]
fn movement_oracle_door_search_untrap_match_legacy() {
    // OpenDoor on a closed adjacent door.
    let mut w = clean_world(804);
    w.level.set_tile(
        Coord::new_unchecked(11, 10),
        Tile::Door {
            state: DoorState::Closed,
            trapped: false,
        },
    );
    let (reply, legacy) = run_movement_oracle(
        &w,
        ActionAst::OpenDoor(Coord::new_unchecked(11, 10)),
        MovementAction::OpenDoor(Coord::new_unchecked(11, 10)),
    );
    assert!(reply.spent_time);
    assert_eq!(
        legacy.level.get_tile(Coord::new_unchecked(11, 10)),
        &Tile::Door {
            state: DoorState::Open,
            trapped: false,
        }
    );

    // CloseDoor on an open adjacent door.
    let mut w = clean_world(804);
    w.level.set_tile(
        Coord::new_unchecked(11, 10),
        Tile::Door {
            state: DoorState::Open,
            trapped: false,
        },
    );
    let (reply, legacy) = run_movement_oracle(
        &w,
        ActionAst::CloseDoor(Coord::new_unchecked(11, 10)),
        MovementAction::CloseDoor(Coord::new_unchecked(11, 10)),
    );
    assert!(reply.spent_time);
    assert_eq!(
        legacy.level.get_tile(Coord::new_unchecked(11, 10)),
        &Tile::Door {
            state: DoorState::Closed,
            trapped: false,
        }
    );

    // Search reveals an adjacent hidden trap.
    let mut w = clean_world(804);
    let trap_coord = Coord::new_unchecked(11, 10);
    w.level.traps.insert(
        trap_coord,
        TrapRecord {
            id: 0,
            trap_type: TrapType::Pit,
            state: TrapState::Hidden,
            coord: trap_coord,
        },
    );
    let (reply, legacy) = run_movement_oracle(&w, ActionAst::Search, MovementAction::Search);
    assert!(reply.spent_time);
    assert_eq!(legacy.level.traps[&trap_coord].state, TrapState::Revealed);

    // Untrap disarms a revealed trap.
    let mut w = clean_world(804);
    w.level.traps.insert(
        trap_coord,
        TrapRecord {
            id: 0,
            trap_type: TrapType::Pit,
            state: TrapState::Revealed,
            coord: trap_coord,
        },
    );
    let (reply, legacy) = run_movement_oracle(
        &w,
        ActionAst::Untrap(trap_coord),
        MovementAction::Untrap(trap_coord),
    );
    assert!(reply.spent_time);
    assert_eq!(legacy.level.traps[&trap_coord].state, TrapState::Disarmed);
}

// ---------------------------------------------------------------------------
// M6-I10: Kick and MeleeAttack dispatch branches. Kick has no combat, so it
// is bit-for-bit legacy parity; MeleeAttack reuses the accepted I5 simplified
// hero-melee law (`compute_hero_melee_attack`), the same law the movement
// transport applies to a melee bump — full `resolve_combat` parity (defender
// AC, weapon skills, artifacts, vorpal) remains a documented follow-up.

#[test]
fn movement_oracle_kick_closed_door_breaks_matching_legacy() {
    let mut w = clean_world(804);
    let door = Coord::new_unchecked(10, 9);
    w.level.set_tile(
        door,
        Tile::Door {
            state: DoorState::Closed,
            trapped: false,
        },
    );
    let (reply, legacy) =
        run_movement_oracle(&w, ActionAst::Kick(door), MovementAction::Kick(door));
    assert!(reply.spent_time);
    assert_eq!(
        legacy.level.get_tile(door),
        &Tile::Door {
            state: DoorState::Broken,
            trapped: false,
        }
    );
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::DoorToggled { coord, new_state }
            if *coord == door && *new_state == DoorState::Broken
    )));
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("shatters into splinters")
    )));
}

#[test]
fn movement_oracle_kick_wall_matching_legacy() {
    let w = clean_world(804);
    let wall = Coord::new_unchecked(10, 9);
    let (reply, legacy) =
        run_movement_oracle(&w, ActionAst::Kick(wall), MovementAction::Kick(wall));
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("kick the wall")
    )));
    // No tile changed and the hero did not move.
    assert_eq!(legacy.level.get_tile(wall), &Tile::Room);
    assert_eq!(
        legacy.arena.actors[w.player_id].coord,
        Coord::new_unchecked(10, 10)
    );
}

#[test]
fn movement_oracle_kick_broken_door_is_silent_noop_matching_legacy() {
    let mut w = clean_world(804);
    let door = Coord::new_unchecked(10, 9);
    w.level.set_tile(
        door,
        Tile::Door {
            state: DoorState::Broken,
            trapped: false,
        },
    );
    let (reply, legacy) =
        run_movement_oracle(&w, ActionAst::Kick(door), MovementAction::Kick(door));
    assert!(!reply.spent_time);
    assert!(reply.events.is_empty());
    assert_eq!(
        legacy.level.get_tile(door),
        &Tile::Door {
            state: DoorState::Broken,
            trapped: false,
        }
    );
}

#[test]
fn movement_oracle_kick_non_adjacent_does_nothing_matching_legacy() {
    let w = clean_world(804);
    let far = Coord::new_unchecked(15, 15);
    let (reply, legacy) = run_movement_oracle(&w, ActionAst::Kick(far), MovementAction::Kick(far));
    assert!(!reply.spent_time);
    assert!(reply.events.is_empty());
    assert_eq!(
        legacy.arena.actors[w.player_id].coord,
        Coord::new_unchecked(10, 10)
    );
}

#[test]
fn movement_oracle_melee_empty_air_matching_legacy() {
    let w = clean_world(804);
    let empty = Coord::new_unchecked(10, 9);
    let (reply, legacy) = run_movement_oracle(
        &w,
        ActionAst::MeleeAttack(empty),
        MovementAction::MeleeAttack(empty),
    );
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("empty air")
    )));
    assert_eq!(
        legacy.arena.actors[w.player_id].coord,
        Coord::new_unchecked(10, 10)
    );
}

#[test]
fn movement_oracle_melee_non_adjacent_does_nothing_matching_legacy() {
    let w = clean_world(804);
    let far = Coord::new_unchecked(15, 15);
    let (reply, legacy) = run_movement_oracle(
        &w,
        ActionAst::MeleeAttack(far),
        MovementAction::MeleeAttack(far),
    );
    assert!(!reply.spent_time);
    assert!(reply.events.is_empty());
    assert_eq!(
        legacy.arena.actors[w.player_id].coord,
        Coord::new_unchecked(10, 10)
    );
}

#[test]
fn movement_oracle_helpless_substitutes_wait_for_kick_and_melee_matching_legacy() {
    // Helpless substitutes a stationary wait for any action, so a kick at a
    // closed door must not break it and a melee attack must not swing.
    let mut w = clean_world(804);
    let door = Coord::new_unchecked(10, 9);
    w.level.set_tile(
        door,
        Tile::Door {
            state: DoorState::Closed,
            trapped: false,
        },
    );
    w.hero.afflictions.transient.helpless = 2;
    let (reply, legacy) =
        run_movement_oracle(&w, ActionAst::Kick(door), MovementAction::Kick(door));
    assert!(reply.spent_time);
    assert!(!reply.events.iter().any(|e| matches!(
        e,
        GameEvent::DoorToggled { .. } | GameEvent::LogMessage { .. }
    )));
    assert_eq!(
        legacy.level.get_tile(door),
        &Tile::Door {
            state: DoorState::Closed,
            trapped: false,
        }
    );

    let mut w = clean_world(804);
    w.hero.afflictions.transient.helpless = 2;
    let empty = Coord::new_unchecked(10, 9);
    let (reply, legacy) = run_movement_oracle(
        &w,
        ActionAst::MeleeAttack(empty),
        MovementAction::MeleeAttack(empty),
    );
    assert!(reply.spent_time);
    assert!(!reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { .. }
            | GameEvent::AttackLanded { .. }
            | GameEvent::AttackMissed { .. }
    )));
    assert_eq!(
        legacy.arena.actors[w.player_id].coord,
        Coord::new_unchecked(10, 10)
    );
}

#[test]
fn melee_attack_matches_move_bump_transport_consistency() {
    // Transport-internal consistency (not legacy parity): MeleeAttack and a
    // melee bump both resolve through the accepted simplified hero-melee law,
    // so from identical worlds they must produce identical events and state.
    let mut w = clean_world(804);
    let goblin_coord = Coord::new_unchecked(11, 10);
    let mut goblin = w
        .ruleset
        .create_monster_record("goblin", goblin_coord, &mut w.rng)
        .unwrap();
    goblin.movement = 0; // Don't let it act during turn ticks
    let goblin_id = w.arena.spawn_actor(goblin);

    let bridge_a = install_bridge(&w);
    let reply_a = bridge_a
        .step_movement(MovementAction::MeleeAttack(goblin_coord))
        .unwrap();
    let world_a = bridge_a.export_native().unwrap();

    let bridge_b = install_bridge(&w);
    let reply_b = bridge_b
        .step_movement(MovementAction::Move(Direction::East))
        .unwrap();
    let world_b = bridge_b.export_native().unwrap();

    assert!(reply_a.spent_time);
    assert!(reply_b.spent_time);
    let combat_happened = reply_a.events.iter().any(|e| match e {
        GameEvent::AttackLanded {
            attacker, target, ..
        } => *attacker == w.player_id && *target == goblin_id,
        GameEvent::AttackMissed { attacker, target } => {
            *attacker == w.player_id && *target == goblin_id
        }
        _ => false,
    });
    assert!(combat_happened, "MeleeAttack should trigger combat");
    assert_eq!(reply_a.events, reply_b.events);
    assert_eq!(oracle_json(&world_a), oracle_json(&world_b));
}
