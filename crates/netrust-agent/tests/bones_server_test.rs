//! Integration tests for the Networked Graveyard & Shared Bones Server.

use netrust_agent::bones::{
    create_bones_router, load_remote_bones_for_current_depth, sync_bones_on_death, BonesClient,
    GraveyardState,
};
use netrust_arena::{ItemLocation, ItemRecord};
use netrust_sim::{Coord, SimulationWorld};
use netrust_types::{Buc, ItemClass};
use std::sync::{Arc, Mutex};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_networked_bones_flow_and_ghost_reincarnation() {
    // 1. Spawn in-memory bones server on ephemeral port
    let state = Arc::new(Mutex::new(GraveyardState::default()));
    let app = create_bones_router(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = BonesClient::new(&format!("127.0.0.1:{}", addr.port()));

    // 2. Verify initial stats
    let initial_stats = client.fetch_stats().expect("Stats should fetch");
    assert_eq!(initial_stats.total_deaths, 0);
    assert_eq!(initial_stats.active_bones_count, 0);
    assert_eq!(initial_stats.total_graves, 0);

    // 3. Create Adventurer "Conan" in Session 1, descend to Depth 3
    let mut sim1 = SimulationWorld::new_with_seed(101);
    sim1.depth = 3;
    let death_coord = Coord::new(15, 12).unwrap();
    if let Some(player) = sim1.arena.actors.get_mut(sim1.player_id) {
        player.name = "Conan".to_string();
        player.coord = death_coord;
        player.hp = 0;
        player.is_dead = true;
    }

    // Give Conan a blessed long sword
    sim1.arena.spawn_item(ItemRecord {
        name: "long sword".into(),
        class: ItemClass::Weapon,
        weight: 40,
        buc: Buc::Blessed,
        is_container: false,
        is_bag_of_holding: false,
        enchantment: 2,
        erosion: 0,
        proofed: false,
        location: ItemLocation::CarriedBy(sim1.player_id),
        corpse_race: None,
        corpse_age: 0,
        rot_threshold: 50,
    });

    // 4. Conan falls in battle to a hill orc -> sync bones to server
    let grave_opt =
        sync_bones_on_death(&mut sim1, &client, "hill orc").expect("Bones upload should succeed");
    let grave = grave_opt.expect("Bones should be valid for depth 3");

    assert_eq!(grave.hero_name, "Conan");
    assert_eq!(grave.depth, 3);
    assert_eq!(grave.killer, "hill orc");
    assert!(grave.ascii_headstone.contains("REST IN PEACE"));
    assert!(grave.ascii_headstone.contains("Conan"));
    assert!(grave.ascii_headstone.contains("Died on Dlvl 3"));
    assert!(grave.ascii_headstone.contains("killed by a hill orc"));

    // 5. Verify graveyard registry and stats updated
    let stats = client.fetch_stats().expect("Stats should update");
    assert_eq!(stats.total_deaths, 1);
    assert_eq!(stats.active_bones_count, 1);
    assert_eq!(stats.total_graves, 1);
    assert_eq!(stats.haunted_depths, vec![3]);

    let graves = client.fetch_graves().expect("Graves list should fetch");
    assert_eq!(graves.len(), 1);
    assert_eq!(graves[0].hero_name, "Conan");

    let conan_grave = client
        .fetch_grave("Conan")
        .expect("Grave lookup should succeed");
    assert!(conan_grave.is_some());
    assert_eq!(conan_grave.unwrap().hero_name, "Conan");

    // 6. Session 2: Adventurer "Valkyrie" enters Depth 3 and loads remote bones
    let mut sim2 = SimulationWorld::new_with_seed(202);
    sim2.depth = 3;

    let events_opt = load_remote_bones_for_current_depth(&mut sim2, &client)
        .expect("Remote bones fetch should succeed");
    let events = events_opt.expect("Bones should be present for depth 3");

    // Verify ghost encounter event was triggered
    assert!(events.iter().any(|e| match e {
        netrust_sim::GameEvent::LogMessage { text } => text.contains("Conan"),
        _ => false,
    }));

    // Verify Ghost of Conan spawned in the dungeon arena
    let ghost_found = sim2.arena.actors.iter().any(|(_, actor)| {
        actor.name.to_ascii_lowercase().contains("ghost") && actor.name.contains("Conan")
    });
    assert!(
        ghost_found,
        "Ghost of Conan must be spawned in the dungeon arena"
    );

    // Verify Conan's long sword was scattered and corrupted to cursed
    let floor_items = sim2.arena.items_at_floor(death_coord);
    let neighbor_items: Vec<_> = death_coord
        .neighbors()
        .into_iter()
        .flat_map(|c| sim2.arena.items_at_floor(c))
        .collect();
    let all_items: Vec<_> = floor_items.into_iter().chain(neighbor_items).collect();

    let sword_corrupted = all_items.iter().any(|&it_id| {
        sim2.arena
            .items
            .get(it_id)
            .map(|it| it.name == "long sword" && it.buc == Buc::Cursed)
            .unwrap_or(false)
    });
    assert!(
        sword_corrupted,
        "Conan's blessed sword should be corrupted to cursed"
    );

    // 7. Verify bones was claimed from the server
    let post_stats = client.fetch_stats().expect("Stats should fetch");
    assert_eq!(
        post_stats.active_bones_count, 0,
        "Bones for depth 3 should be claimed"
    );

    // Next fetch for depth 3 returns None
    let empty_fetch = client.fetch_bones(3).expect("Fetch should succeed");
    assert!(empty_fetch.is_none(), "Depth 3 bones should now be empty");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_multiple_depth_bones_and_graveyard_registry() {
    let state = Arc::new(Mutex::new(GraveyardState::default()));
    let app = create_bones_router(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = BonesClient::new(&format!("127.0.0.1:{}", addr.port()));

    // Upload Hero 1: "Merlin" on Depth 5
    let mut sim_merlin = SimulationWorld::new_with_seed(303);
    sim_merlin.depth = 5;
    if let Some(player) = sim_merlin.arena.actors.get_mut(sim_merlin.player_id) {
        player.name = "Merlin".to_string();
        player.level = 8;
        player.is_dead = true;
    }
    let grave_merlin = sync_bones_on_death(&mut sim_merlin, &client, "red dragon")
        .unwrap()
        .unwrap();
    assert_eq!(grave_merlin.hero_name, "Merlin");
    assert_eq!(grave_merlin.depth, 5);

    // Upload Hero 2: "Arthur" on Depth 12
    let mut sim_arthur = SimulationWorld::new_with_seed(404);
    sim_arthur.depth = 12;
    if let Some(player) = sim_arthur.arena.actors.get_mut(sim_arthur.player_id) {
        player.name = "Arthur".to_string();
        player.level = 14;
        player.is_dead = true;
    }
    let grave_arthur = sync_bones_on_death(&mut sim_arthur, &client, "Medusa")
        .unwrap()
        .unwrap();
    assert_eq!(grave_arthur.hero_name, "Arthur");
    assert_eq!(grave_arthur.depth, 12);

    // Stats check: 2 deaths, 2 active bones, depths [5, 12]
    let stats = client.fetch_stats().unwrap();
    assert_eq!(stats.total_deaths, 2);
    assert_eq!(stats.active_bones_count, 2);
    assert_eq!(stats.haunted_depths, vec![5, 12]);

    // Query non-existent hero grave
    let none_grave = client.fetch_grave("Nobody").unwrap();
    assert!(none_grave.is_none());

    // Query Arthur's headstone
    let arthur_grave = client.fetch_grave("Arthur").unwrap().unwrap();
    assert!(arthur_grave.ascii_headstone.contains("Arthur"));
    assert!(arthur_grave.ascii_headstone.contains("Level 14"));
    assert!(arthur_grave.ascii_headstone.contains("Died on Dlvl 12"));

    // Reset graveyard
    client.reset().unwrap();
    let stats_reset = client.fetch_stats().unwrap();
    assert_eq!(stats_reset.total_deaths, 0);
    assert_eq!(stats_reset.active_bones_count, 0);
    assert_eq!(stats_reset.total_graves, 0);
}
