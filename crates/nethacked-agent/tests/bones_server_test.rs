//! Integration tests for the Networked Graveyard & Shared Bones Server.

use nethacked_agent::bones::{
    create_bones_router, load_remote_bones_for_current_depth, sync_bones_on_death, BonesClient,
    GraveyardState,
};
use nethacked_arena::{ItemLocation, ItemRecord};
use nethacked_sim::{Coord, SimulationWorld};
use nethacked_types::{Buc, ItemClass};
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
        definition: None,
        saved_monster_traits: None,
        artifact_id: None,
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
        corpse_no_revive: false,
        corpse_species: None,
        is_ice_box: false,
        corpse_race: None,
        corpse_age: 0,
        rot_threshold: 50,
        recharged: 0,
        quantity: 1,
        poisoned: false,
        locked: false,
        trapped: false,
        trap_known: false,
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
        nethacked_sim::GameEvent::LogMessage { text } => text.contains("Conan"),
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn networked_definition_identity_legacy_omission_and_native_restoration() {
    use nethacked_arena::ItemDefinition;
    use nethacked_data::{CharacterConfig, ItemKindId as I, MonsterSpeciesId as M, Ruleset};
    use nethacked_sim::{ActionAst, GameEvent};
    use nethacked_types::{Direction, Tile};

    fn world(rules: Arc<Ruleset>) -> SimulationWorld {
        let mut w = SimulationWorld::new_with_character_and_ruleset(
            901,
            CharacterConfig::default(),
            rules,
            nethacked_data::ruleset::RulesetRef::vanilla(),
        );
        w.arena.actors.retain(|id, _| id == w.player_id);
        w.arena.items.clear();
        w.unpaid_items.clear();
        w.equipment = Default::default();
        w.wielded_item = None;
        w.level = nethacked_dungeon::DungeonLevel::new_solid(Tile::Room);
        w.arena.actors[w.player_id].coord = Coord::new_unchecked(20, 10);
        w.arena.actors[w.player_id].hp = 1000;
        w.arena.actors[w.player_id].max_hp = 1000;
        w.depth = 7;
        w
    }
    fn save(w: &SimulationWorld) -> serde_json::Value {
        serde_json::from_str(&w.to_save_json().unwrap()).unwrap()
    }
    fn restored(w: &SimulationWorld) -> SimulationWorld {
        SimulationWorld::from_save_json(
            &w.to_save_json().unwrap(),
            w.ruleset.clone(),
            &w.ruleset_ref,
        )
        .unwrap()
    }

    let state = Arc::new(Mutex::new(GraveyardState::default()));
    let app = nethacked_agent::bones::create_bones_router_with_token(state.clone(), None);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let client = BonesClient::new(&format!("127.0.0.1:{}", addr.port()));
    let mut rules = (*Ruleset::vanilla()).clone();
    for m in &mut rules.monsters {
        m.frequency = 0;
    }
    let candidate = rules
        .monsters
        .iter_mut()
        .find(|m| m.id == Some(M::GIANT_ANT))
        .unwrap();
    candidate.frequency = 1;
    candidate.difficulty = 1;
    candidate.level = 1;
    candidate.sample_hp = false;
    candidate.base_hp = 71;
    candidate.max_hp = 83;
    candidate.speed = 0;
    candidate.gen_flags.clear();
    let def = rules
        .items
        .iter_mut()
        .find(|d| d.id == Some(I::WAN_CREATE_MONSTER))
        .unwrap();
    def.name = "Owned rod".into();
    let mut custom = def.clone();
    custom.id = None;
    custom.name = "Pack ROD".into();
    rules.items.push(custom);
    rules.reindex();
    let rules = Arc::new(rules);
    let mut source = world(rules.clone());
    for (name, display, legacy) in [
        ("Owned rod", "canonical arbitrary rename", false),
        ("Pack ROD", "wand of create monster cold death", false),
        ("Owned rod", "Owned rod", true),
    ] {
        let mut item = rules
            .create_item_record(
                name,
                ItemLocation::CarriedBy(source.player_id),
                Buc::Uncursed,
            )
            .unwrap();
        item.name = display.into();
        item.enchantment = 2;
        if legacy {
            item.definition = None;
        }
        source.arena.spawn_item(item);
    }
    let mut before_death = restored(&source);
    sync_bones_on_death(&mut source, &client, "test")
        .unwrap()
        .unwrap();
    let direct = before_death.save_bones("test").unwrap();
    assert_eq!(save(&source), save(&before_death));
    assert_eq!(state.lock().unwrap().bones_pool[&7][0], direct);
    let fetched = client.fetch_bones(7).unwrap().unwrap();
    assert_eq!(fetched, direct);
    let json = serde_json::to_value(&fetched).unwrap();
    assert!(json["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i.get("definition").is_none()));
    assert!(fetched
        .items
        .iter()
        .any(|i| i.definition == Some(ItemDefinition::Vanilla("WAN_CREATE_MONSTER".into()))));
    assert!(fetched
        .items
        .iter()
        .any(|i| i.definition == Some(ItemDefinition::Custom("pack rod".into()))));
    client.upload_bones(&fetched).unwrap();
    let mut remote = world(rules.clone());
    let mut local = restored(&remote);
    local.bones_storage.push(direct);
    let local_events = local.check_and_load_bones();
    assert_eq!(
        load_remote_bones_for_current_depth(&mut remote, &client)
            .unwrap()
            .unwrap(),
        local_events
    );
    assert_eq!(save(&remote), save(&local));
    assert_eq!(client.fetch_stats().unwrap().active_bones_count, 0);
    let canonical = remote
        .arena
        .items
        .iter()
        .find(|(_, item)| {
            item.definition == Some(ItemDefinition::Vanilla("WAN_CREATE_MONSTER".into()))
        })
        .map(|(id, _)| id)
        .unwrap();
    let custom = remote
        .arena
        .items
        .iter()
        .find(|(_, item)| item.definition == Some(ItemDefinition::Custom("pack rod".into())))
        .map(|(id, _)| id)
        .unwrap();
    let legacy = remote
        .arena
        .items
        .iter()
        .find(|(_, item)| item.definition.is_none())
        .map(|(id, _)| id)
        .unwrap();
    assert_eq!(remote.arena.items[legacy].name, "Owned rod");
    assert!(serde_json::to_value(&remote.arena.items[legacy])
        .unwrap()
        .get("definition")
        .is_none());
    remote.arena.actors.retain(|id, _| id == remote.player_id);
    remote.arena.items[canonical].name = "after remote restoration rename".into();
    remote.arena.items[canonical].buc = Buc::Uncursed;
    let at = match remote.arena.items[canonical].location {
        ItemLocation::Floor(c) => c,
        _ => panic!("not scattered"),
    };
    remote.arena.actors[remote.player_id].coord = at;
    let mut next = restored(&remote);
    assert_eq!(
        remote.step_player_action(ActionAst::PickUp),
        next.step_player_action(ActionAst::PickUp)
    );
    assert_eq!(
        remote.arena.items[canonical].location,
        ItemLocation::CarriedBy(remote.player_id)
    );
    let action = ActionAst::ZapWand {
        dir: Direction::East,
        energy: 9,
    };
    let events = remote.step_player_action(action.clone());
    assert_eq!(events, next.step_player_action(action.clone()));
    assert!(!events
        .iter()
        .any(|e| matches!(e, GameEvent::BeamPropagated { .. })));
    assert_eq!(remote.arena.items[canonical].enchantment, 1);
    assert!(remote
        .arena
        .actors
        .values()
        .any(|a| a.hp == 71 && a.max_hp == 83));
    assert_eq!(save(&remote), save(&next));
    remote.arena.items.remove(canonical);
    remote.arena.actors.retain(|id, _| id == remote.player_id);
    let at = match remote.arena.items[custom].location {
        ItemLocation::Floor(c) => c,
        _ => panic!("not scattered"),
    };
    remote.arena.actors[remote.player_id].coord = at;
    remote.arena.items[custom].buc = Buc::Uncursed;
    remote.step_player_action(ActionAst::PickUp);
    let mut next = restored(&remote);
    assert_eq!(
        remote.step_player_action(action.clone()),
        next.step_player_action(action)
    );
    assert_eq!(remote.arena.actors.len(), 1);
    assert_eq!(remote.arena.items[custom].enchantment, 1);
    assert_eq!(save(&remote), save(&next));
    remote.arena.items.remove(custom);
    let at = match remote.arena.items[legacy].location {
        ItemLocation::Floor(c) => c,
        _ => panic!("legacy item not scattered"),
    };
    remote.arena.actors[remote.player_id].coord = at;
    remote.arena.items[legacy].buc = Buc::Uncursed;
    let mut next = restored(&remote);
    assert_eq!(
        remote.step_player_action(ActionAst::PickUp),
        next.step_player_action(ActionAst::PickUp)
    );
    let action = ActionAst::ZapWand {
        dir: Direction::East,
        energy: 9,
    };
    assert_eq!(
        remote.step_player_action(action.clone()),
        next.step_player_action(action)
    );
    assert_eq!(remote.arena.items[legacy].enchantment, 1);
    assert!(remote.arena.items[legacy].definition.is_none());
    assert!(remote
        .arena
        .actors
        .values()
        .any(|a| a.hp == 71 && a.max_hp == 83));
    assert_eq!(save(&remote), save(&next));
    server.abort();
}
