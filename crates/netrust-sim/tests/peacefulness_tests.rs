//! Integration tests for NetHack 5.0 C monster peacefulness and Elbereth exemptions.
//!
//! NetHack C reference:
//! - `makemon.c:2268-2308` (`peace_minded`)
//! - `monmove.c:240-303` (`onscary`)
//! - `dogmove.c:1119-1128` (pet skipping peaceful monsters)

use netrust_arena::ActorRecord;
use netrust_core::engraving::EngravingMedium;
use netrust_core::ActionAst;
use netrust_data::{create_monster_record, MonsterSpeciesId};
use netrust_sim::{GameEvent, SimulationWorld};
use netrust_types::{BranchId, Coord, Direction, Tile};

#[test]
fn test_peaceful_monsters_spawn_peaceful_and_never_attack_over_50_turns() {
    let mut sim = SimulationWorld::new_with_seed(42);
    let p_coord = sim.arena.actors.get(sim.player_id).unwrap().coord;

    // Clear surroundings
    for dy in -2..=2 {
        for dx in -2..=2 {
            if let Some(c) = Coord::new(
                (p_coord.x as isize + dx) as usize,
                (p_coord.y as isize + dy) as usize,
            ) {
                sim.level.set_tile(c, Tile::Room);
            }
        }
    }

    // Spawn watchman, priest, and shopkeeper adjacent to hero
    let c_watchman = p_coord.step(Direction::East).unwrap();
    let c_priest = p_coord.step(Direction::West).unwrap();
    let c_shopkeeper = p_coord.step(Direction::North).unwrap();

    let watchman_id = sim.arena.spawn_actor(create_monster_record(
        MonsterSpeciesId::Watchman,
        c_watchman,
    ));
    let priest_id = sim
        .arena
        .spawn_actor(create_monster_record(MonsterSpeciesId::Priest, c_priest));
    let shopkeeper_id = sim.arena.spawn_actor(create_monster_record(
        MonsterSpeciesId::Shopkeeper,
        c_shopkeeper,
    ));

    assert!(sim.arena.actors.get(watchman_id).unwrap().is_peaceful);
    assert!(sim.arena.actors.get(priest_id).unwrap().is_peaceful);
    assert!(sim.arena.actors.get(shopkeeper_id).unwrap().is_peaceful);

    let initial_hp = sim.arena.actors.get(sim.player_id).unwrap().hp;

    // Over 50 turns of waiting, none of the peaceful monsters should ever attack the hero
    for _ in 0..50 {
        let events = sim.step_player_action(ActionAst::Wait);
        for e in events {
            if let GameEvent::AttackLanded { target, .. } = e {
                assert_ne!(target, sim.player_id, "Peaceful monster attacked the hero!");
            }
        }
        assert_eq!(
            sim.arena.actors.get(sim.player_id).unwrap().hp,
            initial_hp,
            "Hero HP should not decrease from peaceful monsters"
        );
    }
}

#[test]
fn test_hero_attack_makes_peaceful_monster_hostile_permanently() {
    let mut sim = SimulationWorld::new_with_seed(101);
    let p_coord = sim.arena.actors.get(sim.player_id).unwrap().coord;

    let c_priest = p_coord.step(Direction::East).unwrap();
    sim.level.set_tile(c_priest, Tile::Room);

    let priest_id = sim
        .arena
        .spawn_actor(create_monster_record(MonsterSpeciesId::Priest, c_priest));
    assert!(sim.arena.actors.get(priest_id).unwrap().is_peaceful);

    // Boost hero HP so hero survives the high-damage priest attack
    if let Some(p) = sim.arena.actors.get_mut(sim.player_id) {
        p.hp = 500;
        p.max_hp = 500;
    }

    // Hero attacks the peaceful priest
    let attack_events = sim.step_player_action(ActionAst::MeleeAttack(c_priest));
    assert!(
        attack_events
            .iter()
            .any(|e| matches!(e, GameEvent::LogMessage { text } if text.contains("turns hostile"))),
        "Attack on peaceful monster must log turning hostile"
    );

    let priest_after = sim.arena.actors.get(priest_id).unwrap();
    assert!(
        !priest_after.is_peaceful,
        "Priest must no longer be peaceful"
    );

    // On the hero's action or immediate next wait, the now-hostile priest attacks back
    let wait_events = sim.step_player_action(ActionAst::Wait);
    let priest_attacked = attack_events.iter().chain(wait_events.iter()).any(|e| {
        matches!(
            e,
            GameEvent::AttackLanded {
                attacker, target, ..
            } | GameEvent::AttackMissed { attacker, target }
            if *attacker == priest_id && *target == sim.player_id
        )
    });
    assert!(priest_attacked, "Now-hostile priest must attack hero back");
}

#[test]
fn test_quest_leader_and_guardians_are_peaceful_not_tame() {
    let mut sim = SimulationWorld::new_with_seed(42);
    sim.role_name = "Valkyrie".to_string();
    sim.current_branch = BranchId::Quest;
    sim.depth = 1;

    let _ = sim.unpack_or_generate_level(BranchId::Quest, 1);

    let cfg = netrust_core::get_role_quest_config("Valkyrie").unwrap();

    let leader = sim
        .arena
        .actors
        .values()
        .find(|a| a.name == cfg.leader_name)
        .expect("Quest leader must exist");
    assert!(leader.is_peaceful, "Quest leader must be peaceful");
    assert!(!leader.is_tame, "Quest leader must NOT be tame");

    let guardian = sim
        .arena
        .actors
        .values()
        .find(|a| a.name == cfg.guardian_name)
        .expect("Quest guardian must exist");
    assert!(guardian.is_peaceful, "Quest guardian must be peaceful");
    assert!(!guardian.is_tame, "Quest guardian must NOT be tame");

    // Stepping towards the quest leader must attack (not displace like a pet)
    let leader_coord = leader.coord;
    // Set hero adjacent to leader
    let adj = leader_coord.step(Direction::West).unwrap();
    sim.level.set_tile(adj, Tile::Room);
    if let Some(p) = sim.arena.actors.get_mut(sim.player_id) {
        p.coord = adj;
    }

    let move_events = sim.step_player_action(ActionAst::Move(Direction::East));
    // Must NOT displace: it's a melee attack!
    assert!(
        !move_events
            .iter()
            .any(|e| matches!(e, GameEvent::LogMessage { text } if text.contains("You displace"))),
        "Hero must not displace quest leader like a pet"
    );
}

#[test]
fn test_elbereth_exemptions_and_repulsion() {
    let mut sim = SimulationWorld::new_with_seed(55);
    let p_coord = sim.arena.actors.get(sim.player_id).unwrap().coord;

    // Engrave Elbereth on hero tile
    sim.step_player_action(ActionAst::Engrave {
        text: "Elbereth".into(),
        medium: EngravingMedium::Burned,
    });

    let c_east = p_coord.step(Direction::East).unwrap();
    sim.level.set_tile(c_east, Tile::Room);

    // 1. Goblin (not exempt, hostile) is repelled by Elbereth
    let goblin_id = sim
        .arena
        .spawn_actor(create_monster_record(MonsterSpeciesId::Goblin, c_east));
    let events = sim.step_player_action(ActionAst::Wait);
    assert!(
        events.iter().any(|e| matches!(
            e,
            GameEvent::LogMessage { text } if text.contains("repelled by the sacred ward of Elbereth")
        )),
        "Goblin must be repelled by Elbereth"
    );
    sim.arena.destroy_actor(goblin_id);

    // 2. Human / Shopkeeper (exempt) is NOT repelled even when hostile
    let c_north = p_coord.step(Direction::North).unwrap();
    sim.level.set_tile(c_north, Tile::Room);
    let mut shk_rec = create_monster_record(MonsterSpeciesId::Shopkeeper, c_north);
    shk_rec.is_peaceful = false; // Make hostile
    let shk_id = sim.arena.spawn_actor(shk_rec);

    let shk_events = sim.step_player_action(ActionAst::Wait);
    assert!(
        !shk_events.iter().any(|e| matches!(
            e,
            GameEvent::LogMessage { text } if text.contains("repelled by the sacred ward of Elbereth")
        )),
        "Shopkeeper must be exempt from Elbereth"
    );
    assert!(
        shk_events.iter().any(|e| matches!(
            e,
            GameEvent::AttackLanded { attacker, .. } | GameEvent::AttackMissed { attacker, .. }
            if *attacker == shk_id
        )),
        "Hostile shopkeeper ignores Elbereth and attacks hero"
    );
}

#[test]
fn test_companion_pet_does_not_attack_peaceful_monsters() {
    let mut sim = SimulationWorld::new_with_seed(1234);
    let p_coord = sim.arena.actors.get(sim.player_id).unwrap().coord;

    // Pet adjacent to hero
    let pet_coord = p_coord.step(Direction::East).unwrap();
    sim.level.set_tile(pet_coord, Tile::Room);
    let mut pet_rec = create_monster_record(MonsterSpeciesId::LittleDog, pet_coord);
    pet_rec.is_tame = true;
    let pet_id = sim.arena.spawn_actor(pet_rec);

    // Peaceful watchman adjacent to pet
    let watchman_coord = pet_coord.step(Direction::East).unwrap();
    sim.level.set_tile(watchman_coord, Tile::Room);
    let watchman_id = sim.arena.spawn_actor(create_monster_record(
        MonsterSpeciesId::Watchman,
        watchman_coord,
    ));
    assert!(sim.arena.actors.get(watchman_id).unwrap().is_peaceful);

    let initial_watchman_hp = sim.arena.actors.get(watchman_id).unwrap().hp;

    for _ in 0..20 {
        let events = sim.step_player_action(ActionAst::Wait);
        for e in events {
            if let GameEvent::AttackLanded {
                attacker, target, ..
            } = e
            {
                assert!(
                    !(attacker == pet_id && target == watchman_id),
                    "Pet must not attack peaceful watchman!"
                );
            }
        }
    }

    assert_eq!(
        sim.arena.actors.get(watchman_id).unwrap().hp,
        initial_watchman_hp,
        "Watchman must not take damage from companion pet"
    );
}

#[test]
fn test_serde_default_actor_record_is_peaceful() {
    let mut actor = create_monster_record(MonsterSpeciesId::Priest, Coord::new(1, 1).unwrap());
    actor.is_peaceful = true;
    let mut v = serde_json::to_value(&actor).expect("Serialization must succeed");
    v.as_object_mut().unwrap().remove("is_peaceful");

    let restored: ActorRecord = serde_json::from_value(v).expect("Deserialization must succeed");
    assert!(
        !restored.is_peaceful,
        "Missing is_peaceful must default to false"
    );
    assert_eq!(restored.name, "priest");
}
