//! Integration tests for Rule Packs P1 Task 3:
//! Simulation reads game data from world.ruleset; saves record ruleset_ref.

use std::sync::Arc;

use nethacked_data::ruleset::{PackManifest, Ruleset, RulesetRef};
use nethacked_data::CharacterConfig;
use nethacked_sim::{ActionAst, Coord, LoadError, RoleId, SimulationWorld};
use nethacked_types::{Attack, AttackType, DamageType};

#[test]
fn combat_reads_world_ruleset_jackal_custom_attacks() {
    let vanilla = Ruleset::vanilla();
    let mut custom_monsters = vanilla.monsters.clone();
    let jackal = custom_monsters
        .iter_mut()
        .find(|m| m.name.eq_ignore_ascii_case("jackal"))
        .expect("jackal found in vanilla");

    // Change attack to 3d6 (vanilla is 1d2, max damage 2)
    jackal.attacks = vec![Attack {
        at: AttackType::Bite,
        ad: DamageType::Phys,
        n: 3,
        d: 6,
    }];

    let custom_rs = Arc::new(Ruleset::from_parts(
        PackManifest {
            id: "high-damage-jackal".to_string(),
            name: "High Damage Jackal".to_string(),
            version: "0.1.0".to_string(),
            base: "vanilla".to_string(),
            description: "Jackal does 3d6".to_string(),
        },
        custom_monsters,
        vanilla.items.clone(),
        vanilla.roles.clone(),
        vanilla.races.clone(),
        vanilla.mechanics.clone(),
    ));

    let rref = RulesetRef {
        id: "high-damage-jackal".to_string(),
        version: "0.1.0".to_string(),
        hash: "test-hash-1".to_string(),
    };

    let cfg = CharacterConfig::default();
    let mut world =
        SimulationWorld::new_with_character_and_ruleset(42, cfg, custom_rs.clone(), rref);

    // Spawn a jackal adjacent to the player
    let player_coord = world.hero_coord();
    let jackal_coord =
        Coord::new(player_coord.x + 1, player_coord.y).expect("adjacent coord in bounds");

    // Remove any existing monster at that tile
    if let Some(existing) = world.actor_at(jackal_coord) {
        world.arena.actors.remove(existing);
    }

    let _jackal_id = world
        .spawn_monster_by_name("jackal", jackal_coord)
        .expect("spawn jackal by name succeeds");

    // Give hero high HP so they survive the test
    if let Some(player) = world.arena.actors.get_mut(world.player_id) {
        player.hp = 500;
        player.max_hp = 500;
        player.ac = 10;
    }

    // Step player resting while jackal attacks
    let mut dealt_hit_gt_2 = false;
    for _ in 0..200 {
        let prev_hp = world
            .arena
            .actors
            .get(world.player_id)
            .map(|a| a.hp)
            .unwrap_or(0);
        world.step_player_action(ActionAst::Wait);
        let curr_hp = world
            .arena
            .actors
            .get(world.player_id)
            .map(|a| a.hp)
            .unwrap_or(0);
        if prev_hp > curr_hp {
            let diff = prev_hp - curr_hp;
            if diff > 2 {
                dealt_hit_gt_2 = true;
                break;
            }
        }
    }

    assert!(
        dealt_hit_gt_2,
        "Jackal should have dealt > 2 damage with 3d6 attacks from custom ruleset"
    );
}

#[test]
fn spawn_pack_added_monster_without_enum_id() {
    let vanilla = Ruleset::vanilla();
    let mut custom_monsters = vanilla.monsters.clone();
    let mut dire_jackal = custom_monsters
        .iter()
        .find(|m| m.name.eq_ignore_ascii_case("jackal"))
        .expect("jackal found")
        .clone();
    dire_jackal.id = None;
    dire_jackal.name = "dire jackal".to_string();
    dire_jackal.level = 3;
    dire_jackal.ac = 4;
    dire_jackal.attacks = vec![Attack {
        at: AttackType::Bite,
        ad: DamageType::Phys,
        n: 2,
        d: 6,
    }];
    custom_monsters.push(dire_jackal);

    let custom_rs = Arc::new(Ruleset::from_parts(
        PackManifest {
            id: "dire-pack".to_string(),
            name: "Dire Pack".to_string(),
            version: "0.1.0".to_string(),
            base: "vanilla".to_string(),
            description: "Adds dire jackal".to_string(),
        },
        custom_monsters,
        vanilla.items.clone(),
        vanilla.roles.clone(),
        vanilla.races.clone(),
        vanilla.mechanics.clone(),
    ));

    let rref = RulesetRef {
        id: "dire-pack".to_string(),
        version: "0.1.0".to_string(),
        hash: "dire-hash".to_string(),
    };

    let cfg = CharacterConfig::default();
    let mut world =
        SimulationWorld::new_with_character_and_ruleset(99, cfg, custom_rs.clone(), rref);

    let player_coord = world.hero_coord();
    let adj = Coord::new(player_coord.x + 1, player_coord.y).expect("in bounds");
    if let Some(existing) = world.actor_at(adj) {
        world.arena.actors.remove(existing);
    }

    let actor_id = world
        .spawn_monster_by_name("dire jackal", adj)
        .expect("spawn dire jackal succeeds");

    let actor = world.arena.actors.get(actor_id).expect("actor in arena");
    assert_eq!(actor.name, "dire jackal");
    assert_eq!(actor.ac, 4);
    assert_eq!(actor.level, 3);

    // Give hero high HP and let simulation run 50 turns without panic
    if let Some(player) = world.arena.actors.get_mut(world.player_id) {
        player.hp = 200;
        player.max_hp = 200;
    }

    let start_hp = world.arena.actors.get(world.player_id).unwrap().hp;
    for _ in 0..50 {
        world.step_player_action(ActionAst::Wait);
    }
    let end_hp = world.arena.actors.get(world.player_id).unwrap().hp;
    assert!(
        end_hp < start_hp,
        "Dire jackal should deal damage to adjacent hero over 50 turns"
    );
}

#[test]
fn save_load_ruleset_ref_mismatch_and_backward_compatibility() {
    let custom_ref = RulesetRef {
        id: "pack-alpha".to_string(),
        version: "1.0.0".to_string(),
        hash: "hash-alpha".to_string(),
    };
    let world = SimulationWorld::new_with_character_and_ruleset(
        123,
        CharacterConfig::default(),
        Ruleset::vanilla(),
        custom_ref.clone(),
    );

    let json = world.to_save_json().expect("to_save_json succeeds");

    // Loading with vanilla ref must fail with RulesetMismatch
    let err = SimulationWorld::from_save_json(&json, Ruleset::vanilla(), &RulesetRef::vanilla());
    match err {
        Err(LoadError::RulesetMismatch { expected, found }) => {
            assert_eq!(*expected, RulesetRef::vanilla());
            assert_eq!(*found, custom_ref);
        }
        other => panic!("expected RulesetMismatch, got {other:?}"),
    }

    // Loading with matching ref succeeds
    let restored =
        SimulationWorld::from_save_json(&json, Ruleset::vanilla(), &custom_ref).expect("loads ok");
    assert_eq!(restored.ruleset_ref, custom_ref);

    // Removing ruleset_ref from JSON simulates pre-P1 save, must load as vanilla
    let mut v: serde_json::Value = serde_json::from_str(&json).unwrap();
    v.as_object_mut().unwrap().remove("ruleset_ref");
    let legacy_json = serde_json::to_string(&v).unwrap();

    let legacy_world =
        SimulationWorld::from_save_json(&legacy_json, Ruleset::vanilla(), &RulesetRef::vanilla())
            .expect("legacy save loads as vanilla");
    assert_eq!(legacy_world.ruleset_ref, RulesetRef::vanilla());
}

#[test]
fn quest_species_by_name_missing_species_returns_none_without_panic() {
    let vanilla = Ruleset::vanilla();
    // Filter out Archeologist quest leader ("Lord Carnarvon")
    let filtered_monsters: Vec<_> = vanilla
        .monsters
        .iter()
        .filter(|m| !m.name.eq_ignore_ascii_case("Lord Carnarvon"))
        .cloned()
        .collect();

    let custom_rs = Arc::new(Ruleset::from_parts(
        PackManifest {
            id: "missing-carnarvon".to_string(),
            name: "Missing Carnarvon".to_string(),
            version: "0.1.0".to_string(),
            base: "vanilla".to_string(),
            description: "No Carnarvon".to_string(),
        },
        filtered_monsters,
        vanilla.items.clone(),
        vanilla.roles.clone(),
        vanilla.races.clone(),
        vanilla.mechanics.clone(),
    ));

    let mut world = SimulationWorld::new_with_character_and_ruleset(
        1,
        CharacterConfig {
            role: RoleId::Archaeologist,
            ..Default::default()
        },
        custom_rs,
        RulesetRef {
            id: "missing-carnarvon".to_string(),
            version: "0.1.0".to_string(),
            hash: "test".to_string(),
        },
    );

    // Spawning missing monster returns None, does not panic
    let res = world.spawn_monster_by_name("Lord Carnarvon", Coord::new_unchecked(10, 10));
    assert!(res.is_none());
}
