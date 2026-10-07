use nethacked_arena::ActorSex;
use nethacked_data::{CharacterConfig, Gender, MonsterSpeciesId as M};
use nethacked_sim::{SimulationWorld, StoredLevel, Tile};
use nethacked_types::Coord;
use rand::{Rng, RngCore, SeedableRng};
use std::sync::Arc;
const SPOT: Coord = Coord::new_unchecked(10, 10);

fn world() -> SimulationWorld {
    let mut world = SimulationWorld::new_with_character(
        19,
        CharacterConfig {
            gender: Gender::Male,
            ..CharacterConfig::default()
        },
    );
    world.arena.actors.retain(|id, _| id == world.player_id);
    world.arena.actors[world.player_id].coord = Coord::new_unchecked(2, 2);
    world.level.set_tile(SPOT, Tile::Room);
    world.level.stairs_up = Coord::new_unchecked(3, 3);
    world.level.stairs_down = Coord::new_unchecked(4, 4);
    let mut rules = (*world.ruleset).clone();
    let def = rules
        .monsters
        .iter_mut()
        .find(|d| d.id == Some(M::JACKAL))
        .unwrap();
    def.level = 2;
    def.flags.clear();
    def.always_hostile = true;
    def.name = "Owned jackal".into();
    let mut custom = def.clone();
    custom.id = None;
    custom.name = "Pack jackal".into();
    rules.monsters.push(custom);
    rules.reindex();
    world.ruleset = Arc::new(rules);
    world.rng = rand_chacha::ChaCha8Rng::seed_from_u64(123);
    world
}

#[test]
fn public_spawns_use_owned_sex_once_after_hp_before_peace() {
    for route in 0..3 {
        let mut world = world();
        let mut expected = world.rng.clone();
        let hp: u32 = (0..2).map(|_| expected.random_range(1..=8u32)).sum();
        let hp = hp + u32::from(hp == 2);
        let sex = if expected.random_range(0..2u32) == 0 {
            ActorSex::Male
        } else {
            ActorSex::Female
        };
        let id = match route {
            0 => world.spawn_monster_by_name("Owned jackal", SPOT),
            1 => world.spawn_monster_near(M::JACKAL, SPOT),
            _ => world.spawn_monster_by_name("Pack jackal", SPOT),
        }
        .unwrap();
        let actor = &world.arena.actors[id];
        assert_eq!((actor.hp, actor.max_hp, actor.sex), (hp, hp, Some(sex)));
        assert_eq!(world.rng.next_u64(), expected.next_u64());
        assert_eq!(
            world.arena.actors[world.player_id].sex,
            Some(ActorSex::Male)
        );
    }
}

#[test]
fn full_save_cache_restore_preserves_known_sex_and_continuation_without_draws() {
    let mut world = world();
    let id = world.spawn_monster_by_name("Pack jackal", SPOT).unwrap();
    world.arena.actors[id].name = "Display changed".into();
    let actor = world.arena.actors.remove(id).unwrap();
    let expected_actor = actor.clone();
    let key = (world.current_branch, world.depth + 1);
    world.stored_levels.push((
        key,
        StoredLevel {
            level: world.level.clone(),
            monsters: vec![(id, actor)],
            items: vec![],
            unpaid_items: vec![],
        },
    ));
    let save = world.to_save_json().unwrap();
    let mut loaded =
        SimulationWorld::from_save_json(&save, world.ruleset.clone(), &world.ruleset_ref).unwrap();
    assert_eq!(
        serde_json::to_value(&world).unwrap(),
        serde_json::to_value(&loaded).unwrap()
    );
    let mut untouched = world.rng.clone();
    assert_eq!(
        world.unpack_or_generate_level(key.0, key.1),
        loaded.unpack_or_generate_level(key.0, key.1)
    );
    let restored = world
        .arena
        .actors
        .iter()
        .find(|(_, a)| a.name == "Display changed")
        .unwrap()
        .1;
    assert_eq!(restored, &expected_actor);
    assert_eq!(
        serde_json::to_value(&world).unwrap(),
        serde_json::to_value(&loaded).unwrap()
    );
    assert_eq!(world.rng.clone().next_u64(), untouched.next_u64());
    world.arena.actors.retain(|id, _| id == world.player_id);
    loaded.arena.actors.retain(|id, _| id == loaded.player_id);
    let first = world.spawn_monster_by_name("Owned jackal", SPOT).unwrap();
    let second = loaded.spawn_monster_by_name("Owned jackal", SPOT).unwrap();
    assert_eq!(world.arena.actors[first], loaded.arena.actors[second]);
    assert_eq!(world.rng.next_u64(), loaded.rng.next_u64());
}

fn erase_sex(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(fields) => {
            fields.remove("sex");
            for child in fields.values_mut() {
                erase_sex(child);
            }
        }
        serde_json::Value::Array(items) => {
            for child in items {
                erase_sex(child);
            }
        }
        _ => {}
    }
}

#[test]
fn legacy_full_load_preserves_unknown_fixed_variable_player_and_cached_actors() {
    let mut world = world();
    let variable = world.spawn_monster_by_name("Owned jackal", SPOT).unwrap();
    let fixed = world
        .ruleset
        .create_monster_record_by_id(M::MEDUSA, Coord::new_unchecked(11, 10), &mut world.rng)
        .unwrap();
    assert_eq!(fixed.sex, Some(ActorSex::Female));
    world.arena.spawn_actor(fixed);
    let cached = world.arena.actors[variable].clone();
    let key = (world.current_branch, world.depth + 1);
    world.stored_levels.push((
        key,
        StoredLevel {
            level: world.level.clone(),
            monsters: vec![(variable, cached)],
            items: vec![],
            unpaid_items: vec![],
        },
    ));
    let mut old =
        serde_json::from_str::<serde_json::Value>(&world.to_save_json().unwrap()).unwrap();
    erase_sex(&mut old);
    let mut expected_rng = world.rng.clone();
    let mut loaded = SimulationWorld::from_save_json(
        &old.to_string(),
        world.ruleset.clone(),
        &world.ruleset_ref,
    )
    .unwrap();
    assert!(loaded
        .arena
        .actors
        .values()
        .all(|actor| actor.sex.is_none()));
    assert!(loaded.stored_levels[0]
        .1
        .monsters
        .iter()
        .all(|(_, actor)| actor.sex.is_none()));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&loaded.to_save_json().unwrap()).unwrap(),
        old
    );
    assert_eq!(loaded.rng.next_u64(), expected_rng.next_u64());
}

#[test]
fn startup_rooms_persist_sex_and_player_gender() {
    for (gender, expected) in [
        (Gender::Male, ActorSex::Male),
        (Gender::Female, ActorSex::Female),
    ] {
        let world = SimulationWorld::new_with_character(
            42,
            CharacterConfig {
                gender,
                ..CharacterConfig::default()
            },
        );
        assert_eq!(world.arena.actors[world.player_id].sex, Some(expected));
        assert!(world.arena.actors.len() > 1);
        assert!(world.arena.actors.values().all(|actor| actor.sex.is_some()));
    }
}
