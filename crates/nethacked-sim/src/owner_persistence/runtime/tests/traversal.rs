use super::super::*;
use crate::owner_persistence::runtime::traversal::{self, TraversalAction};
use crate::{ActionAst, GameEvent};
use nethacked_arena::{ActorId, ItemId, ItemLocation};
use nethacked_types::{BranchId, Buc, Coord, Tile};
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
        .with_traversal()
}

fn install_bridge(w: &SimulationWorld) -> OwnerBridge {
    OwnerBridge::install(NativePartitions::from_native(w.clone()), config(w)).unwrap()
}

fn carry(w: &mut SimulationWorld, name: &str, buc: Buc) -> ItemId {
    let item = w
        .ruleset
        .create_item_record(name, ItemLocation::CarriedBy(w.player_id), buc)
        .unwrap();
    w.arena.spawn_item(item)
}

fn spawn_goblin(w: &mut SimulationWorld, coord: Coord) -> ActorId {
    let mut goblin = w
        .ruleset
        .create_monster_record("goblin", coord, &mut w.rng)
        .unwrap();
    goblin.hp = 20;
    goblin.max_hp = 20;
    goblin.movement = 12;
    w.arena.spawn_actor(goblin)
}

#[test]
fn traversal_manifest_bounds_and_profiles() {
    for enabled in [false, true] {
        let w = SimulationWorld::new_with_seed(721);
        let mut c = OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
            .unwrap()
            .with_timer_ticks();
        if enabled {
            c = c.with_traversal();
        }
        assert_eq!(profile_caller(&c).dependencies.len(), 64);
        assert_eq!(profile_provider(&c).exports.len(), 64);
        OwnerBridge::install(NativePartitions::from_native(w), c).unwrap();
    }

    // Microkernel transport manifests fit well within 64-port bounds
    assert!(traversal::READS.len() + traversal::WRITES.len() + 2 <= 64);
    assert_eq!(traversal::READS.len() + traversal::WRITES.len() + 2, 19);
}

#[test]
fn traversal_authentic_stale_refused() {
    let w = clean_world(804);
    let bridge = install_bridge(&w);
    let mut req = bridge.prepare_traversal(TraversalAction::Descend).unwrap();

    // Tampering with witness stamp triggers authentic stale refusal check
    req.guards[0].generation = req.guards[0].generation.wrapping_add(1);
    let err = bridge.execute_traversal(req);
    assert!(matches!(
        err,
        Err(OwnerBridgeError::Owner(owners::Error::Stale))
    ));
    assert!(traversal::stale_refusals() >= 1);
}

#[test]
fn traversal_wrong_authority_refused() {
    let mut w = clean_world(804);
    w.level
        .set_tile(Coord::new_unchecked(10, 10), Tile::Stairs { up: false });
    let bridge = install_bridge(&w);
    let req = bridge.prepare_traversal(TraversalAction::Descend).unwrap();

    traversal::trigger_wrong_resource(true);
    let res = bridge.execute_traversal(req.clone());
    traversal::trigger_wrong_resource(false);

    assert!(res.is_err());

    // State remains uncorrupted and request can still be executed successfully with valid authority
    let reply = bridge.execute_traversal(req).unwrap();
    assert!(reply.spent_time);
}

#[test]
fn traversal_zero_rounds_when_hero_dead() {
    let mut w = clean_world(804);
    w.arena.actors[w.player_id].is_dead = true;
    w.arena.actors[w.player_id].hp = 0;
    let bridge = install_bridge(&w);

    let reply = bridge.step_traversal(TraversalAction::Descend).unwrap();
    assert!(!reply.spent_time);
    assert_eq!(reply.rounds, 0);
    assert!(reply.events.is_empty());
}

// ---------------------------------------------------------------------------
// Legacy-oracle parity: the bridge must reproduce the native
// `step_player_action` bit-for-bit — event stream, full world state and RNG
// stream. The one documented exception is the session event log: legacy
// `record_events` appends to it, while the traversal transport keeps Session
// read-only, so the field is normalised before comparing save snapshots.
// ---------------------------------------------------------------------------

fn oracle_json(w: &SimulationWorld) -> serde_json::Value {
    let mut v: serde_json::Value = serde_json::from_str(&w.to_save_json().unwrap()).unwrap();
    if let Some(obj) = v.as_object_mut() {
        obj.insert("event_log".into(), serde_json::Value::Array(vec![]));
    }
    v
}

fn run_traversal_oracle(
    w: &SimulationWorld,
    ast: ActionAst,
    action: TraversalAction,
) -> (traversal::TraversalReply, SimulationWorld) {
    let mut legacy = w.clone();
    let bridge = install_bridge(w);
    let events = legacy.step_player_action(ast);
    let reply = bridge.step_traversal(action).unwrap();
    assert_eq!(reply.events, events, "event stream diverged from legacy");
    let after = bridge.export_native().unwrap();
    assert_eq!(
        oracle_json(&after),
        oracle_json(&legacy),
        "post-action world state diverged from legacy"
    );
    (reply, legacy)
}

fn run_traversal_oracle_two_step(
    w: &SimulationWorld,
    ast1: ActionAst,
    action1: TraversalAction,
    ast2: ActionAst,
    action2: TraversalAction,
) -> (
    traversal::TraversalReply,
    traversal::TraversalReply,
    SimulationWorld,
) {
    let mut legacy = w.clone();
    let bridge = install_bridge(w);
    let events1 = legacy.step_player_action(ast1);
    let reply1 = bridge.step_traversal(action1).unwrap();
    assert_eq!(reply1.events, events1, "first event stream diverged");
    let events2 = legacy.step_player_action(ast2);
    let reply2 = bridge.step_traversal(action2).unwrap();
    assert_eq!(reply2.events, events2, "second event stream diverged");
    let after = bridge.export_native().unwrap();
    assert_eq!(
        oracle_json(&after),
        oracle_json(&legacy),
        "post-action world state diverged from legacy"
    );
    (reply1, reply2, legacy)
}

#[test]
fn traversal_oracle_descend_matches_legacy() {
    let mut w = clean_world(804);
    w.level
        .set_tile(Coord::new_unchecked(10, 10), Tile::Stairs { up: false });
    let (reply, legacy) = run_traversal_oracle(&w, ActionAst::Descend, TraversalAction::Descend);
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LevelChanged {
            from_depth: 5,
            to_depth: 6
        }
    )));
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("descend deeper into dungeon level 6")
    )));
    assert_eq!(legacy.depth, 6);
    // The packed level rides in the stored-level cache.
    assert_eq!(legacy.stored_levels.len(), 1);
    assert_eq!(legacy.stored_levels[0].0, (BranchId::DungeonsOfDoom, 5));
}

#[test]
fn traversal_oracle_ascend_matches_legacy() {
    let mut w = clean_world(804);
    w.level
        .set_tile(Coord::new_unchecked(10, 10), Tile::Stairs { up: true });
    let (reply, legacy) = run_traversal_oracle(&w, ActionAst::Ascend, TraversalAction::Ascend);
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LevelChanged {
            from_depth: 5,
            to_depth: 4
        }
    )));
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("ascend to dungeon level 4")
    )));
    assert_eq!(legacy.depth, 4);
    assert_eq!(legacy.stored_levels.len(), 1);
}

#[test]
fn traversal_oracle_no_stairs_refused_matches_legacy() {
    let w = clean_world(804);
    let (reply, legacy) = run_traversal_oracle(&w, ActionAst::Descend, TraversalAction::Descend);
    assert!(!reply.spent_time);
    assert_eq!(reply.rounds, 0);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("no stairs leading down")
    )));
    assert_eq!(legacy.depth, 5);
    assert!(legacy.stored_levels.is_empty());
}

#[test]
fn traversal_oracle_branch_stairs_matches_legacy() {
    let mut w = clean_world(804);
    w.depth = 3;
    w.level.set_tile(
        Coord::new_unchecked(10, 10),
        Tile::BranchStairs {
            branch: BranchId::GnomishMines,
            level: 1,
            up: false,
        },
    );
    let (reply, legacy) = run_traversal_oracle(&w, ActionAst::Descend, TraversalAction::Descend);
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LevelChanged {
            from_depth: 3,
            to_depth: 1
        }
    )));
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("enter the GnomishMines branch")
    )));
    assert_eq!(legacy.current_branch, BranchId::GnomishMines);
    assert_eq!(legacy.depth, 1);
    // Both the old and the entered level are cached.
    assert_eq!(legacy.stored_levels.len(), 1);
    assert_eq!(legacy.stored_levels[0].0, (BranchId::DungeonsOfDoom, 3));
}

#[test]
fn traversal_oracle_cached_level_restore_matches_legacy() {
    let mut w = clean_world(804);
    w.level
        .set_tile(Coord::new_unchecked(10, 10), Tile::Stairs { up: false });
    let rock = w
        .ruleset
        .create_item_record(
            "rock",
            ItemLocation::Floor(Coord::new_unchecked(20, 10)),
            Buc::Uncursed,
        )
        .unwrap();
    let rock_id = w.arena.spawn_item(rock);
    let goblin = spawn_goblin(&mut w, Coord::new_unchecked(15, 10));

    // Descend packs the level; the generated up-stairs tile lets the hero
    // ascend back into the cached level.
    let (reply1, reply2, legacy) = run_traversal_oracle_two_step(
        &w,
        ActionAst::Descend,
        TraversalAction::Descend,
        ActionAst::Ascend,
        TraversalAction::Ascend,
    );
    assert!(reply1.spent_time);
    assert!(reply2.spent_time);
    assert_eq!(legacy.depth, 5);
    // The ascend packed the generated level before restoring the cached one:
    // the depth-6 level remains stored.
    assert_eq!(legacy.stored_levels.len(), 1);
    assert_eq!(legacy.stored_levels[0].0, (BranchId::DungeonsOfDoom, 6));
    // The packed floor item and monster were restored onto the level.
    assert!(legacy
        .arena
        .items
        .values()
        .any(|it| it.name == "rock"
            && it.location == ItemLocation::Floor(Coord::new_unchecked(20, 10))));
    assert!(legacy
        .arena
        .actors
        .values()
        .any(|m| !m.is_player && m.name == "goblin"));
    let _ = (rock_id, goblin);
}

#[test]
fn traversal_oracle_vibrating_square_portal_matches_legacy() {
    let mut w = clean_world(804);
    w.current_branch = BranchId::Gehennom;
    w.depth = 5;
    w.vibrating_square = Some(Coord::new_unchecked(10, 10));
    w.ritual_progress = nethacked_core::RitualProgress::SanctumOpened;
    let (reply, legacy) = run_traversal_oracle(&w, ActionAst::Descend, TraversalAction::Descend);
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LevelChanged {
            from_depth: 5,
            to_depth: 6
        }
    )));
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("subterranean portal into Moloch's Sanctum")
    )));
    assert_eq!(legacy.depth, 6);
    assert_eq!(legacy.current_branch, BranchId::Gehennom);
    assert_eq!(legacy.stored_levels.len(), 1);
    assert_eq!(legacy.stored_levels[0].0, (BranchId::Gehennom, 5));
}

#[test]
fn traversal_oracle_vibrating_square_sealed_matches_legacy() {
    let mut w = clean_world(804);
    w.current_branch = BranchId::Gehennom;
    w.depth = 5;
    w.vibrating_square = Some(Coord::new_unchecked(10, 10));
    // RitualProgress::Uninitiated: the portal stays sealed.
    let (reply, legacy) = run_traversal_oracle(&w, ActionAst::Descend, TraversalAction::Descend);
    assert!(!reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("subterranean way remains sealed")
    )));
    assert_eq!(legacy.depth, 5);
    assert!(legacy.stored_levels.is_empty());
}

#[test]
fn traversal_oracle_mysterious_force_matches_legacy() {
    // The force triggers iff the first draw % (mf + 4) == 0; scan a fixed seed
    // range so the test deterministically exercises a triggering seed.
    let mut triggered = false;
    for seed in 804..824 {
        let mut w = clean_world(seed);
        w.current_branch = BranchId::Gehennom;
        w.depth = 2;
        w.level
            .set_tile(Coord::new_unchecked(10, 10), Tile::Stairs { up: true });
        carry(&mut w, "Amulet of Yendor", Buc::Uncursed);
        let (reply, _legacy) = run_traversal_oracle(&w, ActionAst::Ascend, TraversalAction::Ascend);
        assert!(reply.spent_time);
        if reply.events.iter().any(|e| {
            matches!(
                e,
                GameEvent::LogMessage { text } if text.contains("mysterious force")
                    || text.contains("Mysterious Force")
            )
        }) {
            triggered = true;
        }
    }
    assert!(triggered, "no seed in range triggered the mysterious force");
}

#[test]
fn traversal_oracle_victory_matches_legacy() {
    let mut w = clean_world(804);
    w.depth = 1;
    w.level
        .set_tile(Coord::new_unchecked(10, 10), Tile::Stairs { up: true });
    carry(&mut w, "Amulet of Yendor", Buc::Uncursed);
    let (reply, _) = run_traversal_oracle(&w, ActionAst::Ascend, TraversalAction::Ascend);
    // Victory spends no time: the surface check precedes any energy spend.
    assert!(!reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(e, GameEvent::Victory)));
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("carrying the Amulet of Yendor")
    )));
}

#[test]
fn traversal_oracle_surface_without_amulet_matches_legacy() {
    let mut w = clean_world(804);
    w.depth = 1;
    w.level
        .set_tile(Coord::new_unchecked(10, 10), Tile::Stairs { up: true });
    let (reply, legacy) = run_traversal_oracle(&w, ActionAst::Ascend, TraversalAction::Ascend);
    assert!(!reply.spent_time);
    assert!(!reply.events.iter().any(|e| matches!(e, GameEvent::Victory)));
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("celestial force bars your escape")
    )));
    assert_eq!(legacy.depth, 1);
}

#[test]
fn traversal_oracle_quest_consult_matches_legacy() {
    let mut w = clean_world(804);
    w.current_branch = BranchId::Quest;
    w.depth = 1;
    w.arena.actors[w.player_id].level = 14;
    w.alignment_record = 20;
    w.level
        .set_tile(Coord::new_unchecked(10, 10), Tile::Stairs { up: false });
    let (reply, legacy) = run_traversal_oracle(&w, ActionAst::Descend, TraversalAction::Descend);
    assert!(reply.spent_time);
    // The leader consult assigns the quest before the descent.
    assert_eq!(
        legacy.quest_state.progress,
        nethacked_core::QuestProgress::Assigned
    );
    assert_eq!(legacy.depth, 2);
    assert_eq!(legacy.current_branch, BranchId::Quest);
    assert_eq!(legacy.stored_levels.len(), 1);
    assert_eq!(legacy.stored_levels[0].0, (BranchId::Quest, 1));
}

#[test]
fn traversal_oracle_timer_catchup_matches_legacy() {
    let mut w = clean_world(804);
    w.level
        .set_tile(Coord::new_unchecked(10, 10), Tile::Stairs { up: false });
    // A floor corpse with an overdue rot timer and a troll corpse with an
    // overdue revival timer. Both are packed with the level on descend and
    // caught up when the cached level is restored on ascend.
    let rat = w
        .ruleset
        .create_corpse_record("rat", ItemLocation::Floor(Coord::new_unchecked(20, 10)))
        .unwrap();
    let rat_id = w.arena.spawn_item(rat);
    w.start_object_timer(
        crate::ObjectTimerTarget::Active(rat_id),
        crate::ObjectTimerKind::RotCorpse,
        0,
    )
    .unwrap();
    let troll = w
        .ruleset
        .create_corpse_record("troll", ItemLocation::Floor(Coord::new_unchecked(21, 10)))
        .unwrap();
    let troll_id = w.arena.spawn_item(troll);
    w.start_object_timer(
        crate::ObjectTimerTarget::Active(troll_id),
        crate::ObjectTimerKind::ReviveMonster,
        0,
    )
    .unwrap();

    let (reply1, reply2, legacy) = run_traversal_oracle_two_step(
        &w,
        ActionAst::Descend,
        TraversalAction::Descend,
        ActionAst::Ascend,
        TraversalAction::Ascend,
    );
    assert!(reply1.spent_time);
    assert!(reply2.spent_time);
    assert_eq!(legacy.depth, 5);

    // Legacy-exact catch-up: the restored timers expire with bare
    // ObjectTimerExpired envelopes — no rot or revival messages.
    let expired: Vec<_> = reply2
        .events
        .iter()
        .filter(|e| matches!(e, GameEvent::ObjectTimerExpired { .. }))
        .collect();
    assert_eq!(expired.len(), 2);
    assert!(!reply2.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("rots away") || text.contains("revives")
    )));
    // The rat corpse rotted away during the restore catch-up.
    assert!(!legacy
        .arena
        .items
        .values()
        .any(|it| it.is_corpse() && it.name.contains("rat")));
}

#[test]
fn traversal_reload_and_continuation() {
    let mut w = clean_world(804);
    w.level
        .set_tile(Coord::new_unchecked(10, 10), Tile::Stairs { up: false });
    let bridge = install_bridge(&w);

    let reply1 = bridge.step_traversal(TraversalAction::Descend).unwrap();
    assert!(reply1.spent_time);

    // Save
    let wire = bridge.capture().unwrap();

    // Reconstruct into fresh bridge
    let reloaded = OwnerBridge::reconstruct(&wire, config(&w)).unwrap();

    // Continue with an ascend on the reloaded bridge: the stored-level cache
    // survived the reload and the cached level is restored.
    let reply2 = reloaded.step_traversal(TraversalAction::Ascend).unwrap();
    assert!(reply2.spent_time);

    let parts = reloaded.partitions().unwrap();
    assert!(parts.clock.scheduler.turn > 100);
    assert_eq!(parts.geography.depth, 5);
    // The ascend packed the generated level before restoring the cached one.
    assert_eq!(parts.geography.stored_keys.len(), 1);
    assert_eq!(
        parts.geography.stored_keys[0],
        (BranchId::DungeonsOfDoom, 6)
    );
}

#[test]
fn traversal_determinism() {
    let mut w = clean_world(804);
    w.level
        .set_tile(Coord::new_unchecked(10, 10), Tile::Stairs { up: false });
    let bridge_a = install_bridge(&w);
    let bridge_b = install_bridge(&w);

    let reply_a = bridge_a.step_traversal(TraversalAction::Descend).unwrap();
    let reply_b = bridge_b.step_traversal(TraversalAction::Descend).unwrap();
    assert_eq!(reply_a.events, reply_b.events);
    assert_eq!(reply_a.spent_time, reply_b.spent_time);
    assert_eq!(reply_a.rounds, reply_b.rounds);

    let world_a = bridge_a.export_native().unwrap();
    let world_b = bridge_b.export_native().unwrap();
    assert_eq!(oracle_json(&world_a), oracle_json(&world_b));
}
