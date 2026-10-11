//! M7 combined audit: live-extension, retained-continuation, native
//! compatibility and refusal evidence across the complete default cutover.
//!
//! The combined script drives one continuous `OwnerGameSession` on a fresh
//! world (large geography facts, the 16 KiB payload stress case) through
//! every migrated transport — I1 frame, I2 passive ticks, I3 monster ticks,
//! I4 Wait orchestration, I5 movement/doors/traps, I6 inventory, I7 item
//! effects, I8 services and I9 traversal — and requires bit-for-bit
//! agreement with the legacy `step_player_action` oracle at every step:
//! returned events, cumulative event log and full native state (RNG stream
//! included). The continuation test re-imports the session mid-script and
//! requires the reloaded session to match both the uninterrupted facade and
//! the legacy oracle. The refusal test collects soft refusals (compared
//! with legacy) and the hard helpless-cancellation refusal (accepted I1
//! boundary: refuse before publication, state untouched).
//!
//! The geography payload boundary test documents the one open limit the
//! audit found: the 16 KiB transports carry the full `DungeonLevel` in
//! `GeographyFacts`, so a fresh level serializes to 15.4-17.4 KiB depending
//! on the seed (measured over seeds 1-200). Worlds whose level fits the
//! 16 KiB kernel payload bound dispatch through the facade; worlds over
//! the bound refuse truthfully with the state untouched. Compact geography
//! facts (or an architecture-approved bound raise) remain an open
//! follow-up increment.

use super::super::*;
use crate::ActionAst;
use nethacked_arena::{ItemId, ItemLocation};
use nethacked_core::engraving::EngravingMedium;
use nethacked_types::{Buc, Coord, Direction, DoorState, Tile, TrapRecord, TrapState, TrapType};
use rand::SeedableRng;

/// The 16 KiB kernel payload bound the non-traversal transports run under.
const KERNEL_PAYLOAD: usize = 16 * 1024;

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

fn carry(w: &mut SimulationWorld, name: &str) -> ItemId {
    let item = w
        .ruleset
        .create_item_record(name, ItemLocation::CarriedBy(w.player_id), Buc::Uncursed)
        .unwrap();
    w.arena.spawn_item(item)
}

/// Fresh world (large geography facts) with controlled terrain around the
/// hero: a dagger tile north, a closed door east, a revealed pit west and
/// down-stairs under the hero. Carried potion/scroll/ration/wand plus an
/// unpaid rock; a peaceful goblin in the farthest corner ticks during Wait
/// (I3 monster ticks) without ever attacking or approaching the hero.
///
/// The seed must keep the level within the 16 KiB geography observation
/// bound even after the fixture's door/stairs/trap additions (seed 8:
/// 15854 bytes with the additions, 530 bytes of headroom).
fn m7_world(seed: u64) -> SimulationWorld {
    let mut w = SimulationWorld::new_with_seed(seed);
    w.arena.actors.retain(|id, _| id == w.player_id);
    w.arena.items = Default::default();
    w.equipment = Default::default();
    w.wielded_item = None;
    w.hero.quivered_item = None;
    w.unpaid_items.clear();
    w.hero.skills.skills.clear();
    w.level.traps.clear();

    let hero = w.player_id;
    let hc = w.arena.actors[hero].coord;
    let north = Coord::new_unchecked(hc.x, hc.y - 1);
    let east = Coord::new_unchecked(hc.x + 1, hc.y);
    let west = Coord::new_unchecked(hc.x - 1, hc.y);
    w.level.set_tile(north, Tile::Room);
    w.level.set_tile(
        east,
        Tile::Door {
            state: DoorState::Closed,
            trapped: false,
        },
    );
    w.level.set_tile(west, Tile::Room);
    w.level.set_tile(hc, Tile::Stairs { up: false });
    w.level.traps.insert(
        west,
        TrapRecord {
            id: 0,
            trap_type: TrapType::Pit,
            state: TrapState::Revealed,
            coord: west,
        },
    );
    let dagger = w
        .ruleset
        .create_item_record("dagger", ItemLocation::Floor(north), Buc::Uncursed)
        .unwrap();
    w.arena.spawn_item(dagger);
    carry(&mut w, "potion of healing"); // 0
    carry(&mut w, "scroll of identify"); // 1
    carry(&mut w, "food ration"); // 2
    carry(&mut w, "wand of fire"); // 3
    let rock = carry(&mut w, "rock"); // 4
    w.unpaid_items.push((rock, 100));
    w.player_gold = 500;

    // A peaceful goblin in the farthest corner: it ticks (I3 monster ticks
    // during Wait) but never attacks or approaches the hero.
    let corner = [
        Coord::new_unchecked(1, 1),
        Coord::new_unchecked(78, 1),
        Coord::new_unchecked(1, 19),
        Coord::new_unchecked(78, 19),
    ]
    .into_iter()
    .max_by_key(|c| c.chebyshev_distance(hc))
    .unwrap();
    let mut goblin = w
        .ruleset
        .create_monster_record("goblin", corner, &mut w.rng)
        .unwrap();
    goblin.hp = 20;
    goblin.max_hp = 20;
    goblin.movement = 12;
    goblin.is_peaceful = true;
    w.arena.spawn_actor(goblin);
    w
}

/// One continuous script spanning every migrated transport: I4 Wait with I2
/// passive ticks and I3 monster ticks, I5 movement/doors/traps/search/untrap
/// and melee, I6 pickup/wield/drop, I7 quaff/read/eat/engrave/zap, I8 pray
/// and pay, the I1 cancellation frame and I9 descend/ascend.
fn m7_script(hc: Coord) -> Vec<ActionAst> {
    let north = Coord::new_unchecked(hc.x, hc.y - 1);
    let east = Coord::new_unchecked(hc.x + 1, hc.y);
    let west = Coord::new_unchecked(hc.x - 1, hc.y);
    vec![
        ActionAst::Wait,
        ActionAst::Move(Direction::North), // onto the dagger tile
        ActionAst::PickUp,                 // dagger -> carried index 5
        ActionAst::Wield(5),
        ActionAst::Drop(5),
        ActionAst::Move(Direction::South), // back onto the down-stairs
        ActionAst::OpenDoor(east),
        ActionAst::CloseDoor(east),
        ActionAst::Kick(east),         // shatters the door
        ActionAst::Kick(east),         // already broken: silent no-op
        ActionAst::Kick(north),        // plain tile: "you kick the wall"
        ActionAst::MeleeAttack(north), // empty air
        ActionAst::Search,
        ActionAst::Untrap(west), // disarms the revealed pit
        ActionAst::Quaff(0),
        ActionAst::Read(1),
        ActionAst::Eat(2),
        ActionAst::Engrave {
            text: "Elbereth".into(),
            medium: EngravingMedium::Burned,
        },
        ActionAst::ZapWand {
            dir: Direction::South,
            energy: 6,
        },
        ActionAst::Pray,
        ActionAst::Pay,
        ActionAst::CancelZapSelection,
        ActionAst::Descend,
        ActionAst::Ascend,
    ]
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
fn m7_combined_live_extension_matches_legacy_across_all_transports() {
    let w = m7_world(8);
    let hc = w.arena.actors[w.player_id].coord;
    let mut session = OwnerGameSession::new(w.clone()).unwrap();
    let mut legacy = w;

    for ast in m7_script(hc) {
        facade_oracle_step(&mut session, &mut legacy, ast);
    }

    // The traversal phase really ran: descend packed the fresh level and
    // ascend restored it, leaving one stored level behind.
    assert_eq!(session.world().depth, 1);
    assert_eq!(session.world().stored_levels.len(), 1);
    assert_eq!(session.world().depth, legacy.depth);
    assert_eq!(
        session.world().stored_levels.len(),
        legacy.stored_levels.len()
    );
}

#[test]
fn m7_retained_continuation_across_combined_script() {
    let w = m7_world(8);
    let hc = w.arena.actors[w.player_id].coord;
    let script = m7_script(hc);
    let mut session = OwnerGameSession::new(w.clone()).unwrap();
    let mut legacy = w;

    // Run the first half (through the I5 movement phase) uninterrupted.
    let (head, tail) = script.split_at(14);
    for ast in head {
        facade_oracle_step(&mut session, &mut legacy, ast.clone());
    }

    // Re-import the facade's current native world into a fresh owner runtime
    // (the retained-continuation reload path) and run the rest on both the
    // uninterrupted session and the reloaded one; all three must agree
    // bit-for-bit at every step.
    let mut reloaded = OwnerGameSession::new(session.world().clone()).unwrap();
    for ast in tail {
        let reloaded_events = reloaded.step(ast.clone()).unwrap();
        let session_events = session.step(ast.clone()).unwrap();
        let legacy_events = legacy.step_player_action(ast.clone());
        assert_eq!(
            reloaded_events, session_events,
            "reloaded session diverged from uninterrupted facade"
        );
        assert_eq!(
            reloaded_events, legacy_events,
            "reloaded session diverged from legacy"
        );
        assert_eq!(oracle_json(reloaded.world()), oracle_json(session.world()));
        assert_eq!(oracle_json(reloaded.world()), oracle_json(&legacy));
        assert_eq!(reloaded.world().event_log, legacy.event_log);
    }
}

#[test]
fn m7_combined_refusal_evidence() {
    // Soft refusals: the facade reports the same refusal events as legacy and
    // leaves the state bit-for-bit untouched.
    let mut w = clean_world(804);
    carry(&mut w, "rock");
    let mut session = OwnerGameSession::new(w.clone()).unwrap();
    let mut legacy = w;
    for ast in [
        ActionAst::Descend,                // no stairs under the hero
        ActionAst::Quaff(0),               // a rock is not a potion
        ActionAst::Read(0),                // a rock is not a scroll
        ActionAst::Fire(Direction::North), // nothing quivered
    ] {
        let before = oracle_json(session.world());
        let events = session.step(ast.clone()).unwrap();
        let legacy_events = legacy.step_player_action(ast);
        assert_eq!(events, legacy_events, "refusal events diverged from legacy");
        assert_eq!(oracle_json(session.world()), oracle_json(&legacy));
        assert_eq!(
            oracle_json(session.world()),
            before,
            "soft refusal mutated state"
        );
    }

    // Hard refusal (accepted I1 boundary): a helpless cancellation refuses
    // before publication; the session state is bit-for-bit untouched and the
    // session keeps dispatching afterwards.
    let mut w = clean_world(804);
    w.hero.afflictions.transient.helpless = 2;
    let mut session = OwnerGameSession::new(w).unwrap();
    let before = oracle_json(session.world());
    let before_log = session.world().event_log.clone();
    assert!(session.step(ActionAst::CancelZapSelection).is_err());
    assert_eq!(oracle_json(session.world()), before);
    assert_eq!(session.world().event_log, before_log);
    session.step(ActionAst::Wait).unwrap();
}

#[test]
fn m7_geography_payload_boundary() {
    // The 16 KiB transports carry the full DungeonLevel in GeographyFacts.
    // Fresh levels serialize to 15.4-17.4 KiB depending on the seed
    // (measured over seeds 1-200), so the facade dispatches on within-bound
    // worlds and refuses truthfully (state untouched) on over-bound ones.
    // This pins the exact boundary; compact geography facts remain an open
    // follow-up increment.
    let mut within = 0;
    let mut over = 0;
    for seed in 1..=16u64 {
        let w = SimulationWorld::new_with_seed(seed);
        let geo = action_time::GeographyFacts {
            level: w.level.clone(),
            branch: w.current_branch,
            depth: w.depth,
        };
        let size = serde_json::to_vec(&geo).unwrap().len();
        let mut session = OwnerGameSession::new(w).unwrap();
        let before = oracle_json(session.world());
        let wait = session.step(ActionAst::Wait);
        let movement = session.step(ActionAst::Move(Direction::North));
        if size <= KERNEL_PAYLOAD {
            within += 1;
            assert!(
                wait.is_ok(),
                "seed {seed} fits the bound: Wait must dispatch"
            );
            assert!(
                movement.is_ok(),
                "seed {seed} fits the bound: Move must dispatch"
            );
        } else {
            over += 1;
            assert!(
                wait.is_err(),
                "seed {seed} is over the bound: Wait must refuse"
            );
            assert!(
                movement.is_err(),
                "seed {seed} is over the bound: Move must refuse"
            );
            assert_eq!(
                oracle_json(session.world()),
                before,
                "seed {seed} over-bound refusal must leave the state untouched"
            );
        }
    }
    // The sampled range exercises both sides of the boundary.
    assert!(
        within >= 8,
        "expected several within-bound seeds, got {within}"
    );
    assert!(over >= 4, "expected several over-bound seeds, got {over}");
}
