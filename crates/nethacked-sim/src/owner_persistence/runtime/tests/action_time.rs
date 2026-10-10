use super::super::*;
use crate::owner_persistence::runtime::action_time;
use crate::GameEvent;
use nethacked_arena::ItemLocation;
use nethacked_types::{Coord, Tile};
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
        .with_wait_action()
}

fn install_bridge(w: &SimulationWorld) -> OwnerBridge {
    OwnerBridge::install(NativePartitions::from_native(w.clone()), config(w)).unwrap()
}

#[test]
fn wait_manifest_bounds_and_profiles() {
    for wait in [false, true] {
        let w = SimulationWorld::new_with_seed(721);
        let mut c = OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
            .unwrap()
            .with_timer_ticks();
        if wait {
            c = c.with_wait_action();
        }
        assert_eq!(profile_caller(&c).dependencies.len(), 64);
        assert_eq!(profile_provider(&c).exports.len(), 64);
        OwnerBridge::install(NativePartitions::from_native(w), c).unwrap();
    }

    // Microkernel transport manifests fit well within 64-port bounds
    assert!(action_time::READS.len() + action_time::WRITES.len() + 2 <= 64);
    assert_eq!(action_time::READS.len() + action_time::WRITES.len() + 2, 18);
}

#[test]
fn wait_authentic_stale_refused() {
    let w = clean_world(804);
    let bridge = install_bridge(&w);
    let mut req = bridge.prepare_wait().unwrap();

    // Tampering with witness stamp triggers authentic stale refusal check
    req.guards[0].generation = req.guards[0].generation.wrapping_add(1);
    let err = bridge.execute_wait(req);
    assert!(matches!(
        err,
        Err(OwnerBridgeError::Owner(owners::Error::Stale))
    ));
    assert!(action_time::stale_refusals() >= 1);
}

#[test]
fn wait_wrong_authority_refused() {
    let w = clean_world(804);
    let bridge = install_bridge(&w);
    let req = bridge.prepare_wait().unwrap();

    action_time::trigger_wrong_resource(true);
    let res = bridge.execute_wait(req.clone());
    action_time::trigger_wrong_resource(false);

    assert!(res.is_err());

    // State remains uncorrupted and request can still be executed successfully with valid authority
    let reply = bridge.execute_wait(req).unwrap();
    assert!(reply.spent_time);
}

#[test]
fn wait_zero_rounds_when_hero_dead() {
    let mut w = clean_world(804);
    w.arena.actors[w.player_id].is_dead = true;
    w.arena.actors[w.player_id].hp = 0;
    let bridge = install_bridge(&w);

    let reply = bridge.step_wait().unwrap();
    assert!(!reply.spent_time);
    assert_eq!(reply.rounds, 0);
    assert!(reply.events.is_empty());
}

#[test]
fn wait_zero_rounds_when_excess_energy() {
    let mut w = clean_world(804);
    // Give hero 24 energy (2 actions worth)
    w.scheduler.hero_energy = 24;
    let bridge = install_bridge(&w);

    let reply = bridge.step_wait().unwrap();
    assert!(reply.spent_time);
    assert_eq!(reply.rounds, 0);

    // Clock turn should remain unchanged because hero can still act
    let parts = bridge.partitions().unwrap();
    assert_eq!(parts.clock.scheduler.turn, 100);
    assert_eq!(parts.clock.scheduler.hero_energy, 12);
}

#[test]
fn wait_one_round_normal_speed() {
    let w = clean_world(804);
    let bridge = install_bridge(&w);

    let reply = bridge.step_wait().unwrap();
    assert!(reply.spent_time);
    assert_eq!(reply.rounds, 1);

    // World turn advanced by 1
    let parts = bridge.partitions().unwrap();
    assert_eq!(parts.clock.scheduler.turn, 101);

    // TurnAdvanced event was emitted
    let has_turn_advanced = reply
        .events
        .iter()
        .any(|e| matches!(e, GameEvent::TurnAdvanced { turn: 101 }));
    assert!(has_turn_advanced);
}

#[test]
fn wait_multiple_rounds_slow() {
    let mut w = clean_world(804);
    // Hero with very low speed (gain < 12 per round)
    w.scheduler.hero_speed = 6;
    let bridge = install_bridge(&w);

    let reply = bridge.step_wait().unwrap();
    assert!(reply.spent_time);
    assert!(
        reply.rounds >= 2,
        "Slow hero must take multiple rounds to recover energy (got {})",
        reply.rounds
    );

    let parts = bridge.partitions().unwrap();
    assert_eq!(parts.clock.scheduler.turn, 100 + reply.rounds as u64);
}

#[test]
fn wait_combat_and_monsters_during_wait() {
    let mut w = clean_world(804);
    // Spawn a hostile goblin adjacent to hero at (11, 10) with 12 movement
    let mut goblin = w
        .ruleset
        .create_monster_record("goblin", Coord::new_unchecked(11, 10), &mut w.rng)
        .unwrap();
    goblin.movement = 12;
    let goblin_id = w.arena.spawn_actor(goblin);

    let bridge = install_bridge(&w);
    let reply = bridge.step_wait().unwrap();

    assert!(reply.spent_time);
    assert_eq!(reply.rounds, 1);

    // Goblin should have acted and either hit or missed hero
    let combat_happened = reply.events.iter().any(|e| match e {
        GameEvent::AttackLanded {
            attacker, target, ..
        } => *attacker == goblin_id && *target == w.player_id,
        GameEvent::AttackMissed { attacker, target } => {
            *attacker == goblin_id && *target == w.player_id
        }
        GameEvent::LogMessage { text } => text.contains("goblin"),
        _ => false,
    });
    assert!(
        combat_happened,
        "Hostile monster must act during player wait"
    );
}

#[test]
fn wait_passive_ticks_integration() {
    let mut w = clean_world(804);
    // Hero damaged so natural hp regeneration can trigger
    w.arena.actors[w.player_id].hp = 30;
    w.arena.actors[w.player_id].max_hp = 50;
    w.player_nutrition = 900;

    // Floor corpse to verify aging
    let mut corpse = w
        .ruleset
        .create_corpse_record("goblin", ItemLocation::Floor(Coord::new_unchecked(5, 5)))
        .unwrap();
    corpse.corpse_age = 5;
    let corpse_id = w.arena.spawn_item(corpse);

    let bridge = install_bridge(&w);
    let reply = bridge.step_wait().unwrap();

    assert!(reply.spent_time);
    assert_eq!(reply.rounds, 1);

    let parts = bridge.partitions().unwrap();
    // Corpse aged by 1
    assert_eq!(parts.objects.items[corpse_id].corpse_age, 6);
    // Nutrition ticked down
    assert!(parts.hero.player_nutrition < 900);
}

#[test]
fn wait_reload_and_continuation() {
    let w = clean_world(804);
    let bridge = install_bridge(&w);

    let reply1 = bridge.step_wait().unwrap();
    assert_eq!(reply1.rounds, 1);

    // Save
    let wire = bridge.capture().unwrap();

    // Reconstruct into fresh bridge
    let reloaded = OwnerBridge::reconstruct(&wire, config(&w)).unwrap();

    // Continue wait on reloaded bridge
    let reply2 = reloaded.step_wait().unwrap();
    assert_eq!(reply2.rounds, 1);

    let parts = reloaded.partitions().unwrap();
    assert_eq!(parts.clock.scheduler.turn, 102);
}
