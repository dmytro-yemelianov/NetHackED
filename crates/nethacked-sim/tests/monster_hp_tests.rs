use nethacked_data::monsters::MonsterSpawnContext;
use nethacked_data::{MonsterSpeciesId, Ruleset};
use nethacked_sim::{SimulationWorld, Tile};
use nethacked_types::{BranchId, Coord, EndgamePlane};
use rand::RngCore;

fn spawn_tile(world: &mut SimulationWorld) -> Coord {
    let tile = Coord::new_unchecked(10, 10);
    for coord in tile.neighbors().into_iter().chain([tile]) {
        world.level.set_tile(coord, Tile::Room);
    }
    let ids: Vec<_> = world
        .arena
        .actors
        .iter()
        .filter_map(|(id, actor)| (actor.coord == tile && id != world.player_id).then_some(id))
        .collect();
    for id in ids {
        world.arena.actors.remove(id);
    }
    world.arena.actors.get_mut(world.player_id).unwrap().coord = Coord::new_unchecked(2, 2);
    world.level.stairs_up = Coord::new_unchecked(3, 3);
    world.level.stairs_down = Coord::new_unchecked(4, 4);
    tile
}

#[test]
fn real_spawns_use_world_rng_owned_level_and_plane() {
    let mut world = SimulationWorld::new_with_seed(42);
    let mut rules = (*Ruleset::vanilla()).clone();
    rules
        .monsters
        .iter_mut()
        .find(|m| m.id == Some(MonsterSpeciesId::RED_DRAGON))
        .unwrap()
        .level = 3;
    rules.reindex();
    world.ruleset = std::sync::Arc::new(rules);
    let tile = spawn_tile(&mut world);
    let mut expected_rng = world.rng.clone();
    let expected = world
        .ruleset
        .create_monster_record("red dragon", tile, &mut expected_rng)
        .unwrap();
    let id = world.spawn_monster_by_name("red dragon", tile).unwrap();
    let actual = world.arena.actors.get(id).unwrap();
    assert_eq!(
        (actual.hp, actual.max_hp, actual.level),
        (expected.hp, expected.max_hp, 3)
    );
    // Dragon is always hostile, so peace_minded makes no subsequent draws.
    assert_eq!(world.rng.next_u64(), expected_rng.next_u64());
    world.arena.actors.remove(id);
    world.current_branch = BranchId::AstralPlane;
    let mut expected_rng = world.rng.clone();
    let expected = world
        .ruleset
        .create_monster_record_with_context(
            "red dragon",
            tile,
            MonsterSpawnContext {
                endgame_plane: Some(EndgamePlane::Astral),
            },
            &mut expected_rng,
        )
        .unwrap();
    let id = world.spawn_monster_by_name("red dragon", tile).unwrap();
    let actual = world.arena.actors.get(id).unwrap();
    assert_eq!((actual.hp, actual.max_hp), (24, 24));
    assert_eq!(actual.hp, expected.hp);
    assert_eq!(world.rng.next_u64(), expected_rng.next_u64());
}

#[test]
fn save_load_keeps_spawn_rng_sequence() {
    let mut world = SimulationWorld::new_with_seed(11);
    let tile = spawn_tile(&mut world);
    let json = world.to_save_json().unwrap();
    let mut loaded =
        SimulationWorld::from_save_json(&json, world.ruleset.clone(), &world.ruleset_ref).unwrap();
    let first = world.spawn_monster_by_name("red dragon", tile).unwrap();
    let second = loaded.spawn_monster_by_name("red dragon", tile).unwrap();
    assert_eq!(
        world.arena.actors.get(first),
        loaded.arena.actors.get(second)
    );
    assert_eq!(world.rng.next_u64(), loaded.rng.next_u64());
}

#[test]
fn bones_hp_preserves_owned_definition_without_rng_draws() {
    let mut world = SimulationWorld::new_with_seed(12);
    let tile = spawn_tile(&mut world);
    let mut rules = (*world.ruleset).clone();
    rules
        .monsters
        .iter_mut()
        .find(|m| m.name == "ghost")
        .unwrap()
        .ac = -17;
    world.ruleset = std::sync::Arc::new(rules);
    world.bones_storage.push(nethacked_types::BonesData {
        depth: world.depth as u32,
        hero_name: "Remembered".into(),
        hero_level: 7,
        max_hp: 40,
        ac: 3,
        death_coord: tile,
        items: vec![],
        killer: "test".into(),
    });
    let mut expected = world.rng.clone();
    world.check_and_load_bones();
    let ghost = world
        .arena
        .actors
        .iter()
        .find(|(_, actor)| actor.name == "ghost of Remembered")
        .unwrap()
        .1;
    assert_eq!(
        (ghost.hp, ghost.max_hp, ghost.level, ghost.ac),
        (40, 40, 7, -17)
    );
    assert_eq!(world.rng.next_u64(), expected.next_u64());
}
