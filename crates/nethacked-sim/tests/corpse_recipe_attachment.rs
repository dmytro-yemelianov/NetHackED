//! P08b1 exercises sampling/installation only; expired callbacks remain neutral.
use nethacked_arena::{ItemId, ItemLocation};
use nethacked_data::containers::{ContainerContext, GeneratedItem};
use nethacked_data::corpses::CorpseTimeoutRecipe as Recipe;
use nethacked_data::monsters::CorpseTimeoutPolicy as Policy;
use nethacked_data::{ItemKindId as K, MonsterSpeciesId as M};
use nethacked_sim::{
    ActionAst, ObjectTimerError, ObjectTimerKind as Kind, ObjectTimerTarget as Target,
    SimulationWorld, StoredLevel,
};
use nethacked_types::{BranchId, Buc, Tile};
use rand::RngCore;

fn world() -> SimulationWorld {
    world_with_seed(831)
}
fn world_with_seed(seed: u64) -> SimulationWorld {
    let mut world = SimulationWorld::new_with_seed(seed);
    world.arena.actors.retain(|id, _| id == world.player_id);
    world.arena.items.clear();
    world.equipment = Default::default();
    world.wielded_item = None;
    world
}
fn canonical_save(world: &SimulationWorld) -> serde_json::Value {
    serde_json::from_str(&world.to_save_json().unwrap()).unwrap()
}
fn generated(world: &SimulationWorld, kind: K, moves: u64, trace: &[(u32, u32)]) -> GeneratedItem {
    generated_species(world, kind, M::JACKAL, moves, trace)
}
fn generated_species(
    world: &SimulationWorld,
    kind: K,
    species: M,
    moves: u64,
    trace: &[(u32, u32)],
) -> GeneratedItem {
    let mut rules = (*world.ruleset).clone();
    for def in &mut rules.monsters {
        def.frequency = u32::from(def.id == Some(species));
        def.difficulty = 5;
        def.gen_flags
            .retain(|flag| flag != "nogen" && flag != "uniq" && flag != "hell" && flag != "nohell");
    }
    rules.reindex();
    let mut trace = trace.iter();
    let tree = rules
        .create_random_item_tree(
            &rules.item_by_id(kind).unwrap().name,
            ItemLocation::Limbo,
            ContainerContext {
                hero_level: 18,
                level_difficulty: 10,
                moves,
                in_mklev: true,
            },
            |bound| {
                let &(expected, value) = trace.next().expect("unexpected generated draw");
                assert_eq!(bound, expected);
                value
            },
        )
        .unwrap();
    assert_eq!(trace.len(), 0);
    tree
}
fn loaded(world: &SimulationWorld) -> SimulationWorld {
    SimulationWorld::from_save_json(
        &world.to_save_json().unwrap(),
        world.ruleset.clone(),
        &world.ruleset_ref,
    )
    .unwrap()
}
fn install_corpse(world: &mut SimulationWorld, location: ItemLocation, delay: i64) -> ItemId {
    let tree = GeneratedItem {
        item: world
            .ruleset
            .create_corpse_record("human", ItemLocation::Limbo)
            .unwrap(),
        contents: vec![],
        corpse_timeout: Some(Recipe::Rot {
            sampled_at: world.scheduler.turn,
            delay,
        }),
        corpse_timer_operations: Vec::new(),
    };
    world.attach_generated_item(tree, location).unwrap()
}

#[test]
fn ordinary_materialization_preserves_sampling_clock_without_draws_and_full_save_continues() {
    let mut world = world();
    // Select jackal; rnz(25)=49 gives 274 remaining at the sampled clock 73.
    let tree = generated(
        &world,
        K::CORPSE,
        73,
        &[(1, 0), (2, 0), (1000, 999), (4, 1), (2, 1)],
    );
    assert_eq!(
        tree.corpse_timeout,
        Some(Recipe::Rot {
            sampled_at: 73,
            delay: 274
        })
    );
    world.scheduler.turn = 100;
    let mut expected_rng = world.rng.clone();
    let player = world.player_id;
    let id = world
        .attach_generated_item(tree, ItemLocation::CarriedBy(player))
        .unwrap();
    assert_eq!(world.object_timers.entries()[0].due, 347);
    assert_eq!(world.object_timers.entries()[0].serial, 0);
    assert_eq!(world.arena.items[id].corpse_age, 0);
    assert_eq!(world.rng.next_u64(), expected_rng.next_u64());
    let mut copy = loaded(&world);
    assert_eq!(copy.object_timers, world.object_timers);
    assert_eq!(canonical_save(&copy), canonical_save(&world));
    assert_eq!(copy.rng.next_u64(), world.rng.next_u64());
    for _ in 0..3 {
        assert_eq!(
            copy.step_player_action(ActionAst::Wait),
            world.step_player_action(ActionAst::Wait)
        );
        assert_eq!(canonical_save(&copy), canonical_save(&world));
    }
}

#[test]
fn ice_children_start_then_cancel_and_next_live_timer_retains_literal_counter() {
    let mut world = world();
    let tree = generated(
        &world,
        K::ICE_BOX,
        1,
        &[
            (21, 2),
            (1, 0),
            (2, 0),
            (1000, 0),
            (4, 1),
            (2, 1),
            (1, 0),
            (2, 0),
            (1000, 0),
            (4, 1),
            (2, 1),
        ],
    );
    let mut expected_rng = world.rng.clone();
    let player = world.player_id;
    let ice = world
        .attach_generated_item(tree, ItemLocation::CarriedBy(player))
        .unwrap();
    assert_eq!(world.arena.items_in_container(ice).len(), 2);
    assert!(world.object_timers.entries().is_empty());
    assert_eq!(world.rng.next_u64(), expected_rng.next_u64());
    let mut copy = loaded(&world);
    for sim in [&mut world, &mut copy] {
        let id = install_corpse(sim, ItemLocation::Limbo, 50);
        assert_eq!(sim.object_timers.entries()[0].serial, 2);
        assert_eq!(sim.object_timers.entries()[0].target, Target::Active(id));
        assert_eq!(sim.object_timers.entries()[0].due, 51);
    }
    assert_eq!(canonical_save(&world), canonical_save(&copy));
    let children = world.arena.items_in_container(ice);
    world.step_player_action(ActionAst::Wait);
    assert!(children
        .iter()
        .all(|id| world.arena.items[*id].corpse_age == 0));
}

#[test]
fn nested_frozen_attachment_cancels_and_unsupported_rider_installs_nothing() {
    let mut world = world();
    let player = world.player_id;
    let ice = world.arena.spawn_item(
        world
            .ruleset
            .create_item_record_by_id(K::ICE_BOX, ItemLocation::CarriedBy(player), Buc::Uncursed)
            .unwrap(),
    );
    let mut sack = generated(&world, K::ICE_BOX, 1, &[(21, 0)]);
    sack.contents.push(generated(
        &world,
        K::CORPSE,
        1,
        &[(1, 0), (2, 0), (1000, 0), (4, 1), (2, 1)],
    ));
    let sack = world
        .attach_generated_item(sack, ItemLocation::InContainer(ice))
        .unwrap();
    assert!(world.object_timers.entries().is_empty());
    assert_eq!(world.arena.items_in_container(sack).len(), 1);
    let unsupported = GeneratedItem {
        item: world
            .ruleset
            .create_corpse_record("Death", ItemLocation::Limbo)
            .unwrap(),
        contents: vec![],
        corpse_timeout: Some(Recipe::UnsupportedRider),
        corpse_timer_operations: Vec::new(),
    };
    world
        .attach_generated_item(unsupported, ItemLocation::Limbo)
        .unwrap();
    assert!(world.object_timers.entries().is_empty());
    install_corpse(&mut world, ItemLocation::Limbo, 50);
    assert_eq!(world.object_timers.entries()[0].serial, 1);
}

#[test]
fn signed_deadlines_preserve_past_and_zero_without_wrapping_or_clamping() {
    let mut world = world();
    world.scheduler.turn = 100;
    let item = world.arena.spawn_item(
        world
            .ruleset
            .create_corpse_record("human", ItemLocation::Limbo)
            .unwrap(),
    );
    assert_eq!(
        world.start_object_timer_signed(Target::Active(item), Kind::RotCorpse, -8),
        Ok(0)
    );
    assert_eq!(world.object_timers.entries()[0].due, 92);
    assert_eq!(
        world.stop_object_timer(Target::Active(item), Kind::RotCorpse),
        Some(0)
    );
    assert_eq!(
        world.start_object_timer_signed(Target::Active(item), Kind::RotCorpse, 0),
        Ok(1)
    );
    assert_eq!(world.object_timers.entries()[0].due, 100);
    world.cancel_object_timers(Target::Active(item));
    assert_eq!(
        world.start_object_timer_signed(Target::Active(item), Kind::RotCorpse, -101),
        Err(ObjectTimerError::ClockOverflow)
    );
    world.scheduler.turn = u64::MAX;
    assert_eq!(
        world.start_object_timer_signed(Target::Active(item), Kind::RotCorpse, 1),
        Err(ObjectTimerError::ClockOverflow)
    );
    assert!(world.object_timers.entries().is_empty());
}

#[test]
fn installed_recipe_survives_public_cache_return_and_mid_cache_save_without_resampling() {
    let mut world = world();
    let floor = ItemLocation::Floor(world.level.stairs_down);
    let id = install_corpse(&mut world, floor, 50);
    world.arena.items[id].name = "sampled local".into();
    world.stored_levels.push((
        (BranchId::DungeonsOfDoom, 2),
        StoredLevel {
            level: world.level.clone(),
            monsters: vec![],
            items: vec![],
            unpaid_items: vec![],
        },
    ));
    world
        .level
        .set_tile(world.level.stairs_down, Tile::Stairs { up: false });
    world.arena.actors[world.player_id].coord = world.level.stairs_down;
    world.step_player_action(ActionAst::Descend);
    assert_eq!(world.scheduler.turn, 2);
    assert!(matches!(
        world.object_timers.entries()[0].target,
        Target::Cached { .. }
    ));
    let mut copy = loaded(&world);
    for sim in [&mut world, &mut copy] {
        sim.step_player_action(ActionAst::Wait);
        sim.level
            .set_tile(sim.level.stairs_up, Tile::Stairs { up: true });
        sim.arena.actors[sim.player_id].coord = sim.level.stairs_up;
        sim.step_player_action(ActionAst::Ascend);
        assert_eq!(sim.scheduler.turn, 4);
        assert_eq!(
            (
                sim.object_timers.entries()[0].serial,
                sim.object_timers.entries()[0].due
            ),
            (0, 51)
        );
        let Target::Active(reminted) = sim.object_timers.entries()[0].target else {
            panic!("not remapped")
        };
        assert_eq!(sim.arena.items[reminted].name, "sampled local");
        assert_eq!(sim.arena.items[reminted].corpse_age, 3);
    }
    assert_eq!(canonical_save(&world), canonical_save(&copy));
    assert_eq!(world.rng.next_u64(), copy.rng.next_u64());
}

#[test]
fn public_descend_generates_sampled_timers_with_actual_clock_and_hero_level() {
    let mut generated_count = 0;
    // Room class selection can pick an empty class. Cover a fixed native seed
    // population, preserving that real public generation behavior.
    for seed in 0..32 {
        let mut world = world_with_seed(seed);
        let mut rules = (*world.ruleset).clone();
        for item in &mut rules.items {
            item.generation_weight = u32::from(item.id == Some(K::CORPSE));
        }
        for monster in &mut rules.monsters {
            monster.corpse_timeout_policy = Policy::Ordinary;
            if monster.glyph == 'T' {
                monster.gen_flags.push("nogen".into());
            }
        }
        rules.reindex();
        world.ruleset = std::sync::Arc::new(rules);
        world.scheduler.turn = 73;
        world.arena.actors[world.player_id].level = 18;
        world
            .level
            .set_tile(world.level.stairs_down, Tile::Stairs { up: false });
        world.arena.actors[world.player_id].coord = world.level.stairs_down;
        world.step_player_action(ActionAst::Descend);
        assert_eq!(world.depth, 2);
        let corpses: Vec<_> = world
            .arena
            .items
            .iter()
            .filter_map(|(id, item)| item.is_corpse().then_some(id))
            .collect();
        assert_eq!(world.object_timers.entries().len(), corpses.len());
        for id in corpses {
            assert_eq!(
                world
                    .object_timers
                    .entries()
                    .iter()
                    .filter(|timer| timer.target == Target::Active(id))
                    .count(),
                1
            );
        }
        for timer in world.object_timers.entries() {
            let Target::Active(id) = timer.target else {
                panic!("new room item cached")
            };
            assert!(world.arena.items[id].is_corpse());
            assert_eq!(timer.kind, Kind::RotCorpse);
            assert!(timer.due >= 300 && timer.due <= 597); // rnz(25) is 2..299 at XL18.
            generated_count += 1;
        }
    }
    assert!(generated_count > 0);
}

#[test]
fn generated_troll_success_and_failure_preserve_norevive_sampling_clock_and_save() {
    for no_revive in [false, true] {
        for success in [false, true] {
            let mut world = world();
            let mut trace = vec![(1, 0), (2, 0), (1000, 999), (4, 1), (2, 0)];
            trace.extend(std::iter::repeat_n((37, 1), if success { 8 } else { 49 }));
            if success {
                trace.push((37, 0)); // age10, after failures at ages2..9.
            }
            let mut tree = generated_species(&world, K::CORPSE, M::TROLL, 73, &trace);
            assert_eq!(
                tree.corpse_timeout,
                Some(if success {
                    Recipe::Revive {
                        sampled_at: 73,
                        delay: 10,
                    }
                } else {
                    Recipe::Rot {
                        sampled_at: 73,
                        delay: 237,
                    }
                })
            );
            // The persisted decision controls future callback execution; neither
            // initial sampling nor materialization drops or changes the recipe.
            tree.item.corpse_no_revive = no_revive;
            world.scheduler.turn = 75;
            let mut expected_rng = world.rng.clone();
            let id = world
                .attach_generated_item(tree, ItemLocation::Limbo)
                .unwrap();
            let timer = &world.object_timers.entries()[0];
            assert_eq!(
                timer.kind,
                if success {
                    Kind::ReviveMonster
                } else {
                    Kind::RotCorpse
                }
            );
            assert_eq!(timer.due, if success { 83 } else { 310 });
            assert_eq!(timer.target, Target::Active(id));
            assert_eq!(timer.serial, 0);
            assert_eq!(world.arena.items[id].corpse_no_revive, no_revive);
            assert_eq!(world.arena.items[id].corpse_age, 0);
            assert_eq!(world.rng.next_u64(), expected_rng.next_u64());
            let mut copy = loaded(&world);
            assert_eq!(canonical_save(&copy), canonical_save(&world));
            assert_eq!(copy.rng.next_u64(), world.rng.next_u64());
            for _ in 0..3 {
                assert_eq!(
                    copy.step_player_action(ActionAst::Wait),
                    world.step_player_action(ActionAst::Wait)
                );
                assert_eq!(canonical_save(&copy), canonical_save(&world));
            }
        }
    }
}

#[test]
fn generated_frozen_troll_success_and_failed_trials_start_cancel_selected_kinds() {
    let mut world = world();
    let mut trace = vec![(21, 2), (1, 0), (2, 0), (1000, 0), (4, 1), (2, 1), (37, 0)];
    trace.extend([(1, 0), (2, 0), (1000, 0), (4, 1), (2, 1)]);
    trace.extend(std::iter::repeat_n((37, 1), 49));
    let tree = generated_species(&world, K::ICE_BOX, M::TROLL, 1, &trace);
    assert_eq!(
        tree.contents[0].corpse_timeout,
        Some(Recipe::Revive {
            sampled_at: 1,
            delay: 2
        })
    );
    assert_eq!(
        tree.contents[1].corpse_timeout,
        Some(Recipe::Rot {
            sampled_at: 1,
            delay: 250
        })
    );
    let mut expected_rng = world.rng.clone();
    let player = world.player_id;
    let ice = world
        .attach_generated_item(tree, ItemLocation::CarriedBy(player))
        .unwrap();
    let children = world.arena.items_in_container(ice);
    assert_eq!(children.len(), 2);
    assert!(world.object_timers.entries().is_empty());
    assert_eq!(world.rng.next_u64(), expected_rng.next_u64());
    let mut copy = loaded(&world);
    for sim in [&mut world, &mut copy] {
        install_corpse(sim, ItemLocation::Limbo, 50);
        assert_eq!(sim.object_timers.entries()[0].serial, 2);
        sim.step_player_action(ActionAst::Wait);
        assert!(children
            .iter()
            .all(|id| sim.arena.items[*id].corpse_age == 0));
    }
    assert_eq!(canonical_save(&copy), canonical_save(&world));
}

#[test]
fn troll_revive_recipe_survives_public_cache_return_and_mid_cache_save() {
    let mut world = world();
    let mut trace = vec![(1, 0), (2, 0), (1000, 0), (4, 1), (2, 1)];
    trace.extend(std::iter::repeat_n((37, 1), 48));
    trace.push((37, 0));
    let mut tree = generated_species(&world, K::CORPSE, M::TROLL, 1, &trace);
    tree.item.corpse_no_revive = true;
    let id = world
        .attach_generated_item(tree, ItemLocation::Floor(world.level.stairs_down))
        .unwrap();
    world.arena.items[id].name = "sampled Troll local".into();
    world.stored_levels.push((
        (BranchId::DungeonsOfDoom, 2),
        StoredLevel {
            level: world.level.clone(),
            monsters: vec![],
            items: vec![],
            unpaid_items: vec![],
        },
    ));
    world
        .level
        .set_tile(world.level.stairs_down, Tile::Stairs { up: false });
    world.arena.actors[world.player_id].coord = world.level.stairs_down;
    world.step_player_action(ActionAst::Descend);
    assert_eq!(world.scheduler.turn, 2);
    assert!(matches!(
        world.object_timers.entries()[0].target,
        Target::Cached { .. }
    ));
    let mut copy = loaded(&world);
    for sim in [&mut world, &mut copy] {
        sim.step_player_action(ActionAst::Wait);
        sim.level
            .set_tile(sim.level.stairs_up, Tile::Stairs { up: true });
        sim.arena.actors[sim.player_id].coord = sim.level.stairs_up;
        sim.step_player_action(ActionAst::Ascend);
        assert_eq!(sim.scheduler.turn, 4);
        let timer = &sim.object_timers.entries()[0];
        assert_eq!(
            (timer.serial, timer.due, timer.kind),
            (0, 51, Kind::ReviveMonster)
        );
        let Target::Active(reminted) = timer.target else {
            panic!("not remapped")
        };
        assert_eq!(sim.arena.items[reminted].name, "sampled Troll local");
        assert!(sim.arena.items[reminted].corpse_no_revive);
        assert_eq!(sim.arena.items[reminted].corpse_age, 3);
    }
    assert_eq!(canonical_save(&world), canonical_save(&copy));
    assert_eq!(world.rng.next_u64(), copy.rng.next_u64());
}

#[test]
fn public_descend_generates_troll_trial_successes_and_failures() {
    let mut revived = 0;
    let mut rotted = 0;
    for seed in 0..32 {
        let mut world = world_with_seed(seed);
        let mut rules = (*world.ruleset).clone();
        for item in &mut rules.items {
            item.generation_weight = u32::from(item.id == Some(K::CORPSE));
        }
        for monster in &mut rules.monsters {
            monster.frequency = u32::from(monster.id == Some(M::TROLL));
            monster.difficulty = 1;
            monster.gen_flags.retain(|flag| {
                flag != "nogen" && flag != "uniq" && flag != "hell" && flag != "nohell"
            });
        }
        rules.reindex();
        world.ruleset = std::sync::Arc::new(rules);
        world.scheduler.turn = 73;
        world.arena.actors[world.player_id].level = 18;
        world
            .level
            .set_tile(world.level.stairs_down, Tile::Stairs { up: false });
        world.arena.actors[world.player_id].coord = world.level.stairs_down;
        world.step_player_action(ActionAst::Descend);
        assert_eq!(world.depth, 2);
        let corpses: Vec<_> = world
            .arena
            .items
            .iter()
            .filter_map(|(id, item)| item.is_corpse().then_some(id))
            .collect();
        assert_eq!(world.object_timers.entries().len(), corpses.len());
        for id in corpses {
            assert_eq!(
                world.arena.items[id].corpse_species,
                Some(nethacked_arena::CorpseSpecies::Vanilla("TROLL".into()))
            );
            let timers: Vec<_> = world
                .object_timers
                .entries()
                .iter()
                .filter(|timer| timer.target == Target::Active(id))
                .collect();
            assert_eq!(timers.len(), 1);
            let timer = timers[0];
            match timer.kind {
                Kind::ReviveMonster => {
                    assert!((75..=123).contains(&timer.due));
                    revived += 1;
                }
                Kind::RotCorpse => {
                    assert!((300..=597).contains(&timer.due));
                    rotted += 1;
                }
            }
        }
    }
    assert!(revived > 0);
    assert!(rotted > 0);
}
